//! TLS configuration (placeholder for Let's Encrypt integration).
//!
//! For the MVP the gateway runs in HTTP-only mode. Production deployments
//! should use `rustls` with Let's Encrypt certificates for TLS 1.3.
//!
//! ## Production TODO
//!
//! 1. Use `axum-server` with the `rustls` feature, or `tokio-rustls` directly.
//! 2. Support automatic certificate renewal via Let's Encrypt (ACME protocol).
//! 3. Enforce TLS 1.3 only — disable TLS 1.2 and below.
//! 4. Use ECDHE key exchange with X25519.
//!
//! ```ignore
//! use axum_server::tls_rustls::RustlsConfig;
//!
//! let config = RustlsConfig::from_pem_file(cert, key).await?;
//! axum_server::bind_rustls(addr, config)
//!     .serve(app.into_make_service())
//!     .await?;
//! ```

use std::path::PathBuf;

/// TLS configuration for the gateway.
#[derive(Debug, Clone, Default)]
pub struct TlsConfig {
    /// Path to the TLS certificate (PEM format).
    pub cert_path: Option<PathBuf>,
    /// Path to the TLS private key (PEM format).
    pub key_path: Option<PathBuf>,
}

impl TlsConfig {
    /// Create a new TLS configuration.
    ///
    /// If both paths are `None`, the gateway runs in HTTP-only mode
    /// (suitable for development or when behind a TLS-terminating
    /// load balancer).
    pub fn new(cert_path: Option<PathBuf>, key_path: Option<PathBuf>) -> Self {
        Self {
            cert_path,
            key_path,
        }
    }

    /// Returns `true` if TLS is enabled (both cert and key paths are set).
    pub fn is_enabled(&self) -> bool {
        self.cert_path.is_some() && self.key_path.is_some()
    }

    /// 校验 TLS 配置是否自洽：cert 与 key 必须**同时**提供或**同时**缺失。
    ///
    /// 只提供其一属于不完整配置，调用方（[`crate::config::GatewayConfig::validate_tls`]）
    /// 应据此拒绝启动，避免「看似启用 TLS 实则明文」的安全假象。
    pub fn validate(&self) -> Result<(), String> {
        match (self.cert_path.is_some(), self.key_path.is_some()) {
            (true, true) | (false, false) => Ok(()),
            _ => Err(
                "TLS 配置不完整：必须同时提供 cert 与 key，或都不提供".to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_validate_requires_both_or_neither() {
        assert!(TlsConfig::new(None, None).validate().is_ok());
        assert!(TlsConfig::new(
            Some(PathBuf::from("/c.pem")),
            Some(PathBuf::from("/k.pem")),
        )
        .validate()
        .is_ok());
        assert!(TlsConfig::new(Some(PathBuf::from("/c.pem")), None)
            .validate()
            .is_err());
        assert!(TlsConfig::new(None, Some(PathBuf::from("/k.pem")))
            .validate()
            .is_err());
    }
}
