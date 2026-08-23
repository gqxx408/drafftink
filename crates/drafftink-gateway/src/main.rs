//! # drafftink-gateway
//!
//! Lightweight public network gateway for the school teaching suite.
//!
//! Forwards requests to the intranet backend, implements security measures
//! (rate limiting, WAF, device binding), and stores NO business data locally.
//!
//! ## Exposed Routes
//!
//! | Method | Path                        | Auth Required |
//! |--------|-----------------------------|---------------|
//! | POST   | `/api/auth/login`           | No            |
//! | GET    | `/api/homework/:id`         | JWT + device  |
//! | POST   | `/api/homework/submit`      | JWT + device  |
//! | GET    | `/api/homework/result/:id`  | JWT + device  |
//!
//! ## Middleware Pipeline (outermost first)
//!
//! 1. **TraceLayer** — request/response logging via `tracing`.
//! 2. **DefaultBodyLimit** — reject oversized request bodies.
//! 3. **Rate limiting** — sliding-window per-IP limit.
//! 4. **WAF** — SQL injection / XSS / path traversal checks.

mod config;
mod error;
mod proxy;
mod security;
mod state;
mod tls;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Request, State};
use axum::http::{HeaderMap, Method};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use bytes::Bytes;
use http_body_util::{BodyExt, Limited};
use tower_http::trace::TraceLayer;

use config::GatewayConfig;
use error::GatewayError;
use state::GatewayState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Load configuration from environment.
    let config = GatewayConfig::from_env();
    tracing::info!("Gateway starting on {}", config.listen_addr);

    // 启动安全闸门（P0-1）：JWT 密钥缺失或使用已知默认硬编码密钥时，
    // 直接拒绝启动，避免与后端信任错配或任何人伪造令牌。
    if let Err(e) = config.jwt.validate_not_default() {
        eprintln!("ERROR: JWT secret not configured. Set DRAFTTINK_JWT_SECRET environment variable.");
        eprintln!("Refusing to start with default/empty secret (security risk).");
        eprintln!("Details: {e}");
        std::process::exit(1);
    }
    tracing::info!("Backend URL: {}", config.backend_url);
    tracing::info!(
        "Rate limit: {} req/min per IP",
        config.rate_limit_per_minute
    );
    tracing::info!("Max request size: {} bytes", config.max_request_size);
    tracing::info!("Audit log path (backend): {}", config.log_db_path);

    // 启动安全闸门（P1）：未启用 TLS 却监听 443 等标准 TLS 端口时拒绝启动，
    // 避免以明文暴露凭证；或仅配置 cert/key 之一时拒绝（配置不完整）。
    if let Err(e) = config.validate_tls() {
        eprintln!("ERROR: TLS configuration invalid.");
        eprintln!("{e}");
        std::process::exit(1);
    }

    let tls_config = tls::TlsConfig::new(
        config.tls_cert_path.clone(),
        config.tls_key_path.clone(),
    );
    if tls_config.is_enabled() {
        tracing::warn!(
            "TLS is configured but not yet implemented; running HTTP-only"
        );
    } else {
        tracing::info!("Running in HTTP-only mode (no TLS)");
    }

    // Create shared state.
    let state = Arc::new(GatewayState::new(&config));

    // Start background rate-limiter cleanup task.
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let mut limiter = cleanup_state.rate_limiter.lock().await;
            limiter.cleanup();
            tracing::debug!("Rate limiter cleanup: {} tracked IPs", limiter.tracked_ip_count());
        }
    });

    // Build router and start server.
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!("Gateway listening on {}", config.listen_addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

/// Build the gateway router with all routes and middleware.
fn build_router(state: Arc<GatewayState>) -> Router {
    let max_request_size = state.max_request_size;

    Router::new()
        // Route 1: Login (no JWT required).
        .route("/api/auth/login", post(login_handler))
        // Route 2: Get homework (JWT + device fingerprint required).
        .route("/api/homework/:id", get(homework_handler))
        // Route 3: Submit homework (JWT + device fingerprint required).
        .route("/api/homework/submit", post(submit_handler))
        // Route 4: Get homework result (JWT + device fingerprint required).
        .route("/api/homework/result/:id", get(result_handler))
        // Middleware: WAF check (innermost — reads body before handler).
        .layer(from_fn_with_state(state.clone(), waf_middleware))
        // Middleware: Rate limiting.
        .layer(from_fn_with_state(state.clone(), rate_limit_middleware))
        // Middleware: Request body size limit.
        .layer(DefaultBodyLimit::max(max_request_size))
        // Middleware: Request/response tracing.
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// ════════════════════════════════════════════════════════════════════════════
//  Helper
// ════════════════════════════════════════════════════════════════════════════

/// Extract the client IP address.
///
/// Checks the `X-Forwarded-For` header first (when behind a load
/// balancer or reverse proxy), then falls back to the TCP connection's
/// remote address.
fn extract_client_ip(addr: &SocketAddr, headers: &HeaderMap) -> String {
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(s) = forwarded.to_str() {
            if let Some(first_ip) = s.split(',').next() {
                let ip = first_ip.trim();
                if !ip.is_empty() {
                    return ip.to_string();
                }
            }
        }
    }
    addr.ip().to_string()
}

// ════════════════════════════════════════════════════════════════════════════
//  Middleware
// ════════════════════════════════════════════════════════════════════════════

/// Rate limiting middleware.
///
/// Uses a sliding-window counter per IP. Requests exceeding the
/// configured per-minute limit receive a 429 response.
async fn rate_limit_middleware(
    State(state): State<Arc<GatewayState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Result<Response, GatewayError> {
    let ip = extract_client_ip(&addr, req.headers());
    let allowed = {
        let mut limiter = state.rate_limiter.lock().await;
        limiter.check_rate(&ip, state.rate_limit_per_minute)
    };

    if !allowed {
        tracing::warn!("Rate limit exceeded for IP: {ip}");
        return Err(GatewayError::RateLimited);
    }

    Ok(next.run(req).await)
}

/// WAF (Web Application Firewall) middleware.
///
/// Reads the request body (with size limit), checks it for malicious
/// patterns, then reconstructs the request for the handler.
async fn waf_middleware(
    State(state): State<Arc<GatewayState>>,
    req: Request,
    next: Next,
) -> Result<Response, GatewayError> {
    let (parts, body) = req.into_parts();

    // Read the body with a size limit to prevent memory exhaustion.
    let limited_body = Limited::new(body, state.max_request_size);
    let body_bytes = limited_body
        .collect()
        .await
        .map_err(|e| GatewayError::BadGateway(format!("Failed to read request body: {e}")))?
        .to_bytes();

    // Run WAF checks on method, path, and body.
    state
        .waf_checker
        .check_request(parts.method.as_str(), parts.uri.path(), &body_bytes)
        .map_err(GatewayError::Blocked)?;

    // Reconstruct the request with the (now buffered) body.
    let req = Request::from_parts(parts, axum::body::Body::from(body_bytes));
    Ok(next.run(req).await)
}

// ════════════════════════════════════════════════════════════════════════════
//  Handlers
// ════════════════════════════════════════════════════════════════════════════

/// `POST /api/auth/login` — forward to backend (no JWT required).
async fn login_handler(
    State(state): State<Arc<GatewayState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, GatewayError> {
    let client_ip = extract_client_ip(&addr, &headers);

    let response = proxy::forward_to_backend(
        &state,
        Method::POST,
        "/api/auth/login",
        &headers,
        body,
        &client_ip,
    )
    .await?;

    // Forward audit log to backend (fire-and-forget, no local storage).
    proxy::send_audit_log(
        &state,
        "anonymous",
        "",
        &client_ip,
        "/api/auth/login",
        "POST",
    );

    Ok(response)
}

/// `GET /api/homework/:id` — forward to backend (JWT + device fingerprint).
async fn homework_handler(
    State(state): State<Arc<GatewayState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, GatewayError> {
    let claims = proxy::verify_request_auth(&headers, &state.jwt_config)?;
    let client_ip = extract_client_ip(&addr, &headers);
    let path = format!("/api/homework/{id}");

    let response =
        proxy::forward_to_backend(&state, Method::GET, &path, &headers, body, &client_ip).await?;

    proxy::send_audit_log(
        &state,
        &claims.sub,
        &claims.device_fp,
        &client_ip,
        &path,
        "GET",
    );

    Ok(response)
}

/// `POST /api/homework/submit` — forward to backend (JWT + device fingerprint).
async fn submit_handler(
    State(state): State<Arc<GatewayState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, GatewayError> {
    let claims = proxy::verify_request_auth(&headers, &state.jwt_config)?;
    let client_ip = extract_client_ip(&addr, &headers);

    let response = proxy::forward_to_backend(
        &state,
        Method::POST,
        "/api/homework/submit",
        &headers,
        body,
        &client_ip,
    )
    .await?;

    proxy::send_audit_log(
        &state,
        &claims.sub,
        &claims.device_fp,
        &client_ip,
        "/api/homework/submit",
        "POST",
    );

    Ok(response)
}

/// `GET /api/homework/result/:id` — forward to backend (JWT + device fingerprint).
async fn result_handler(
    State(state): State<Arc<GatewayState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, GatewayError> {
    let claims = proxy::verify_request_auth(&headers, &state.jwt_config)?;
    let client_ip = extract_client_ip(&addr, &headers);
    let path = format!("/api/homework/result/{id}");

    let response =
        proxy::forward_to_backend(&state, Method::GET, &path, &headers, body, &client_ip).await?;

    proxy::send_audit_log(
        &state,
        &claims.sub,
        &claims.device_fp,
        &client_ip,
        &path,
        "GET",
    );

    Ok(response)
}
