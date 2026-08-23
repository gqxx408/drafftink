//! Gateway shared state.
//!
//! [`GatewayState`] is wrapped in `Arc` and shared across all handlers
//! and middleware. It holds the backend URL, rate limiter, JWT config,
//! and an HTTP client for forwarding requests.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use drafftink_core::JwtConfig;
use http_body_util::Full;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use tokio::sync::Mutex;

use crate::config::GatewayConfig;
use crate::security::{RateLimiter, WafChecker};

/// HTTP client type used to forward requests to the backend.
///
/// Uses `hyper-util`'s legacy client with an `HttpConnector` (pure Rust,
/// no C dependencies) and `Full<Bytes>` request bodies.
pub type GatewayHttpClient = Client<HttpConnector, Full<Bytes>>;

/// Shared gateway state accessible to all handlers and middleware.
pub struct GatewayState {
    /// Backend URL (e.g. `http://127.0.0.1:8080`).
    pub backend_url: String,
    /// Rate limiter (protected by async mutex).
    pub rate_limiter: Arc<Mutex<RateLimiter>>,
    /// JWT configuration (shared secret with the backend).
    pub jwt_config: JwtConfig,
    /// Maximum request body size in bytes.
    pub max_request_size: usize,
    /// HTTP client for forwarding requests to the backend.
    pub http_client: GatewayHttpClient,
    /// WAF checker instance.
    pub waf_checker: WafChecker,
    /// Rate limit per minute per IP.
    pub rate_limit_per_minute: u32,
}

impl GatewayState {
    /// Create new gateway state from the given configuration.
    pub fn new(config: &GatewayConfig) -> Self {
        let http_client = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(Some(Duration::from_secs(30)))
            .build_http::<Full<Bytes>>();

        Self {
            backend_url: config.backend_url.clone(),
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new())),
            jwt_config: config.jwt.clone(),
            max_request_size: config.max_request_size,
            http_client,
            waf_checker: WafChecker::new(),
            rate_limit_per_minute: config.rate_limit_per_minute,
        }
    }
}
