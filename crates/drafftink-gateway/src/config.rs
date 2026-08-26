//! Gateway configuration.
//!
//! Loads from environment variables with sensible defaults.

use std::path::PathBuf;

use drafftink_core::JwtConfig;

/// Gateway configuration.
///
/// All fields can be overridden via environment variables.
/// The gateway stores NO business data locally — `log_db_path` is metadata
/// forwarded to the backend so it knows where to persist audit logs.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Address to listen on (default `0.0.0.0:80`).
    ///
    /// 监听标准 TLS 端口（443）时**必须**同时提供 `GATEWAY_TLS_CERT_PATH` 与
    /// `GATEWAY_TLS_KEY_PATH`，否则 [`GatewayConfig::validate_tls`] 会拒绝启动。
    pub listen_addr: String,
    /// Backend URL to forward requests to (default `http://127.0.0.1:8080`).
    pub backend_url: String,
    /// Rate limit per minute per IP (default 60).
    pub rate_limit_per_minute: u32,
    /// Maximum request body size in bytes (default 10 MB).
    pub max_request_size: usize,
    /// JWT 配置（密钥必须与后端共享同一个 `DRAFTTINK_JWT_SECRET`）。
    pub jwt: JwtConfig,
    /// Path for audit logs — forwarded to the backend, not stored locally.
    pub log_db_path: String,
    /// TLS certificate path. `None` means HTTP-only (development mode).
    pub tls_cert_path: Option<PathBuf>,
    /// TLS private key path.
    pub tls_key_path: Option<PathBuf>,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:80".to_string(),
            backend_url: "http://127.0.0.1:8080".to_string(),
            rate_limit_per_minute: 60,
            max_request_size: 10 * 1024 * 1024, // 10 MB
            jwt: JwtConfig::default(),
            log_db_path: "data/gateway_logs".to_string(),
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

impl GatewayConfig {
    /// Load configuration from environment variables, falling back to defaults.
    pub fn from_env() -> Self {
        Self {
            listen_addr: std::env::var("GATEWAY_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:80".to_string()),
            backend_url: std::env::var("GATEWAY_BACKEND_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string()),
            rate_limit_per_minute: std::env::var("GATEWAY_RATE_LIMIT_PER_MINUTE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
            max_request_size: std::env::var("GATEWAY_MAX_REQUEST_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10 * 1024 * 1024),
            jwt: match std::env::var("DRAFTTINK_JWT_SECRET")
                .ok()
                .filter(|s| !s.is_empty())
            {
                Some(s) => JwtConfig {
                    secret: s.into_bytes(),
                    ..Default::default()
                },
                // 兼容旧部署：回退读取 GATEWAY_JWT_SECRET。但后端只认
                // DRAFTTINK_JWT_SECRET，若仅网关设置则后端会因缺失而拒绝启动，
                // 从而避免信任错配导致的静默不安全。
                None => match std::env::var("GATEWAY_JWT_SECRET")
                    .ok()
                    .filter(|s| !s.is_empty())
                {
                    Some(s) => JwtConfig {
                        secret: s.into_bytes(),
                        ..Default::default()
                    },
                    None => JwtConfig::default(),
                },
            },
            log_db_path: std::env::var("GATEWAY_LOG_DB_PATH")
                .unwrap_or_else(|_| "data/gateway_logs".to_string()),
            tls_cert_path: std::env::var("GATEWAY_TLS_CERT_PATH")
                .ok()
                .map(PathBuf::from),
            tls_key_path: std::env::var("GATEWAY_TLS_KEY_PATH")
                .ok()
                .map(PathBuf::from),
        }
    }

    /// 校验「监听端口」与「TLS 配置」的一致性（fail-closed，启动前调用）。
    ///
    /// 规则：
    /// 1. 仅配置了 `tls_cert_path` / `tls_key_path` 之一 → 拒绝（配置不完整）。
    /// 2. 未启用 TLS 却监听**标准 TLS 端口（443）** → 拒绝启动，因为在不加密的
    ///    443 端口提供明文服务既是协议违规，也会让凭证以明文暴露。
    ///    运维必须二选一：配置 `GATEWAY_TLS_CERT` + `GATEWAY_TLS_KEY` 启用 TLS，
    ///    或将 `GATEWAY_LISTEN_ADDR` 改为非 TLS 端口（如 `0.0.0.0:8080`）。
    pub fn validate_tls(&self) -> Result<(), String> {
        // cert/key 必须同时提供或同时缺失。
        crate::tls::TlsConfig::new(self.tls_cert_path.clone(), self.tls_key_path.clone())
            .validate()?;
        let has_cert = self.tls_cert_path.is_some();
        if !has_cert {
            if let Some(443) = self.listen_port() {
                return Err("拒绝在未启用 TLS 的情况下监听 443（标准 TLS 端口）。\n\
                     请二选一：\n\
                     1) 配置 GATEWAY_TLS_CERT 与 GATEWAY_TLS_KEY 以启用 TLS；或\n\
                     2) 将 GATEWAY_LISTEN_ADDR 改为非 TLS 端口（如 0.0.0.0:8080）。"
                    .to_string());
            }
        }
        Ok(())
    }

    /// 解析 `listen_addr` 中的端口号；解析失败返回 `None`。
    fn listen_port(&self) -> Option<u16> {
        self.listen_addr
            .rsplit(':')
            .next()
            .and_then(|p| p.trim_end_matches(']').parse().ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_tls_refuses_plaintext_on_443() {
        let cfg = GatewayConfig {
            listen_addr: "0.0.0.0:443".to_string(),
            ..Default::default()
        }; // 默认 0.0.0.0:80
        assert!(cfg.validate_tls().is_err(), "0.0.0.0:443 无 TLS 必须被拒绝");
    }

    #[test]
    fn validate_tls_allows_non_tls_port() {
        let mut cfg = GatewayConfig::default();
        cfg.listen_addr = "0.0.0.0:8080".to_string();
        assert!(
            cfg.validate_tls().is_ok(),
            "非 TLS 端口无 TLS 应允许（开发/内网 LB 场景）"
        );
    }

    #[test]
    fn validate_tls_allows_when_certs_present() {
        let cfg = GatewayConfig {
            listen_addr: "0.0.0.0:443".to_string(),
            tls_cert_path: Some(PathBuf::from("/etc/gateway/cert.pem")),
            tls_key_path: Some(PathBuf::from("/etc/gateway/key.pem")),
            ..Default::default()
        }; // 0.0.0.0:80
        assert!(cfg.validate_tls().is_ok(), "同时提供 cert+key 应允许");
    }

    #[test]
    fn validate_tls_refuses_partial_certs() {
        let cfg = GatewayConfig {
            listen_addr: "0.0.0.0:8080".to_string(),
            tls_cert_path: Some(PathBuf::from("/etc/gateway/cert.pem")),
            ..Default::default()
        };
        // key 缺失
        assert!(
            cfg.validate_tls().is_err(),
            "仅提供 cert 缺少 key 必须被拒绝"
        );
    }
}
