//! Gateway error types.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Errors that can occur during gateway request processing.
#[derive(Debug)]
pub enum GatewayError {
    /// Client has exceeded the rate limit (429 Too Many Requests).
    RateLimited,
    /// Request blocked by WAF rules (403 Forbidden).
    Blocked(String),
    /// Backend communication error (502 Bad Gateway).
    BadGateway(String),
    /// Authentication or authorization failure (401 Unauthorized).
    Unauthorized(String),
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimited => write!(f, "Rate limit exceeded"),
            Self::Blocked(reason) => write!(f, "Blocked by WAF: {reason}"),
            Self::BadGateway(reason) => write!(f, "Bad gateway: {reason}"),
            Self::Unauthorized(reason) => write!(f, "Unauthorized: {reason}"),
        }
    }
}

impl std::error::Error for GatewayError {}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        match self {
            Self::RateLimited => {
                (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded").into_response()
            }
            Self::Blocked(reason) => {
                (StatusCode::FORBIDDEN, format!("Blocked by WAF: {reason}")).into_response()
            }
            Self::BadGateway(reason) => {
                (StatusCode::BAD_GATEWAY, format!("Bad gateway: {reason}")).into_response()
            }
            Self::Unauthorized(reason) => {
                (StatusCode::UNAUTHORIZED, format!("Unauthorized: {reason}")).into_response()
            }
        }
    }
}
