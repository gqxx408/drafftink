//! Request forwarding to the intranet backend.
//!
//! The gateway acts as a reverse proxy: it receives public requests,
//! applies security checks (rate limiting, WAF, JWT + device binding),
//! and forwards them to the intranet backend using a pure-Rust HTTP
//! client (`hyper-util`).
//!
//! Audit logs are forwarded to the backend's audit endpoint — the gateway
//! itself stores NO business data locally.

use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderMap, Method, header};
use axum::response::Response;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};

use drafftink_core::{JwtClaims, JwtConfig, verify_jwt};

use crate::error::GatewayError;
use crate::state::GatewayState;

/// Extract and verify the JWT from the `Authorization` header.
///
/// Also extracts the device fingerprint from the `X-Device-FP` header
/// and verifies that it matches the `device_fp` claim inside the JWT.
///
/// Returns the decoded [`JwtClaims`] on success, or a
/// [`GatewayError::Unauthorized`] on failure.
pub fn verify_request_auth(
    headers: &HeaderMap,
    jwt_config: &JwtConfig,
) -> Result<JwtClaims, GatewayError> {
    // Extract Bearer token from Authorization header.
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| GatewayError::Unauthorized("Missing Authorization header".into()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| {
            GatewayError::Unauthorized("Invalid Authorization header format".into())
        })?;

    // Extract device fingerprint from X-Device-FP header.
    let device_fp = headers
        .get("x-device-fp")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| GatewayError::Unauthorized("Missing X-Device-FP header".into()))?;

    // Verify JWT signature, expiry, and device fingerprint binding.
    verify_jwt(token, jwt_config, device_fp)
        .map_err(|e| GatewayError::Unauthorized(e.to_string()))
}

/// Forward a request to the backend and return the response.
///
/// The original request method, path, headers (except `Host` and
/// `Content-Length`), and body are forwarded. An `X-Forwarded-For`
/// header is added with the client IP.
pub async fn forward_to_backend(
    state: &GatewayState,
    method: Method,
    path: &str,
    headers: &HeaderMap,
    body: Bytes,
    client_ip: &str,
) -> Result<Response, GatewayError> {
    let url = format!("{}{}", state.backend_url, path);

    // Build the forwarded request.
    let mut req_builder = axum::http::Request::builder().method(method);

    // Forward original headers, skipping Host and Content-Length
    // (the HTTP client sets these automatically based on the URI and body).
    // Also skip any client-supplied `x-forwarded-for`: we MUST override it with
    // the gateway's own view of the client IP rather than append, so a
    // malicious client cannot spoof its source address to the backend.
    for (name, value) in headers.iter() {
        if name != header::HOST
            && name != header::CONTENT_LENGTH
            && name.as_str() != "x-forwarded-for"
        {
            req_builder = req_builder.header(name, value);
        }
    }
    // Single, authoritative X-Forwarded-For (override — never append).
    req_builder = req_builder.header("x-forwarded-for", client_ip);

    let request = req_builder
        .body(Full::new(body))
        .map_err(|e| GatewayError::BadGateway(format!("Failed to build request: {e}")))?;

    tracing::debug!("Forwarding request to {url}");

    // Send with a 30-second timeout.
    let response = tokio::time::timeout(
        Duration::from_secs(30),
        state.http_client.request(request),
    )
    .await
    .map_err(|_| GatewayError::BadGateway("Backend request timed out".into()))?
    .map_err(|e| GatewayError::BadGateway(format!("Backend request failed: {e}")))?;

    // Convert the hyper response into an axum response.
    let status = response.status();
    let resp_headers = response.headers().clone();
    let body_bytes = response
        .into_body()
        .collect()
        .await
        .map_err(|e| GatewayError::BadGateway(format!("Failed to read response body: {e}")))?
        .to_bytes();

    let mut resp_builder = Response::builder().status(status);
    for (name, value) in resp_headers.iter() {
        resp_builder = resp_builder.header(name, value);
    }

    resp_builder
        .body(Body::from(body_bytes))
        .map_err(|e| GatewayError::BadGateway(format!("Failed to build response: {e}")))
}

/// Send an audit log entry to the backend's audit endpoint.
///
/// This is **fire-and-forget**: the log is sent in a background tokio task
/// and the caller does not wait for the result. The gateway stores NO
/// audit data locally — all logs are forwarded to the backend.
pub fn send_audit_log(
    state: &GatewayState,
    user_id: &str,
    device_fp: &str,
    client_ip: &str,
    path: &str,
    method: &str,
) {
    let client = state.http_client.clone();
    let backend_url = state.backend_url.clone();

    let log_entry = serde_json::json!({
        "id": uuid::Uuid::new_v4(),
        "user_id": user_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "ip_address": client_ip,
        "device_fp": device_fp,
        "details": format!("{{\"path\":\"{path}\",\"method\":\"{method}\"}}"),
    });

    let body = match serde_json::to_vec(&log_entry) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("Failed to serialize audit log: {e}");
            return;
        }
    };

    tokio::spawn(async move {
        let url = format!("{backend_url}/api/audit/log");
        let req = match axum::http::Request::builder()
            .method("POST")
            .uri(&url)
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(body)))
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Failed to build audit log request: {e}");
                return;
            }
        };

        if let Err(e) = client.request(req).await {
            tracing::warn!("Failed to send audit log to backend: {e}");
        }
    });
}
