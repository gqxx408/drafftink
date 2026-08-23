//! 登录接口速率限制（防暴力破解）。
//!
//! 当前 [`LoginRateLimiter`] 为**进程内**实现：按客户端 IP 在单个进程的内存中计数，
//! 仅对单实例部署有效。在多实例（负载均衡 / 多副本）场景下，各实例独立计数会放大
//! 允许的总请求量，因此生产环境必须替换为 Redis 等**共享后端**（见 [`RateLimitBackend`]
//! 抽象与 [`RedisRateLimitBackend`] 占位）。此处用 `std` 实现以避免引入额外基础设施依赖。

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::AppError;

#[derive(Clone)]
pub struct LoginRateLimiter {
    inner: std::sync::Arc<Mutex<HashMap<IpAddr, Vec<Instant>>>>,
    max_attempts: usize,
    window: Duration,
}

impl LoginRateLimiter {
    pub fn new(max_attempts: usize, window: Duration) -> Self {
        Self {
            inner: std::sync::Arc::new(Mutex::new(HashMap::new())),
            max_attempts,
            window,
        }
    }

    /// 检查并累加该 IP 的登录尝试次数。
    ///
    /// 若窗口内尝试次数已达上限，返回 429；否则记录本次尝试并放行。
    pub fn check(&self, ip: IpAddr) -> Result<(), AppError> {
        let now = Instant::now();
        let mut map = self.inner.lock().unwrap();
        let attempts = map.entry(ip).or_default();
        // 滑动窗口：丢弃窗口外的旧记录
        attempts.retain(|t| now.duration_since(*t) <= self.window);
        if attempts.len() >= self.max_attempts {
            return Err(AppError::TooManyRequests(
                "登录尝试过于频繁，请稍后再试".to_string(),
            ));
        }
        attempts.push(now);
        // 防止内存无限增长：限制每个 IP 最多保留 max_attempts*2 条
        if attempts.len() > self.max_attempts * 2 {
            attempts.drain(0..attempts.len() - self.max_attempts);
        }
        Ok(())
    }
}

/// 速率限制后端抽象：将计数语义与具体存储解耦。
///
/// 多实例部署时可用 Redis 等共享实现替换进程内 [`LoginRateLimiter`]，
/// 从而在多个网关 / 后端副本之间保持一致的限流阈值。
pub trait RateLimitBackend: Send + Sync {
    /// 记录一次请求并返回该 key 当前窗口内的计数。
    fn record(&self, key: &str) -> Result<usize, AppError>;
    /// 读取该 key 当前窗口内的计数（不写入）。
    fn count(&self, key: &str) -> Result<usize, AppError>;
}

/// Redis 速率限制后端占位实现（尚未启用）。
///
/// 多实例部署时需启用：典型实现可用 `INCR` + `EXPIRE`（固定窗口）或 Lua 脚本
/// 实现滑动窗口 / 令牌桶。当前仓库未引入 `redis` 依赖，此处仅定义契约占位，
/// 避免热路径误用导致 panic。启用方式（示意）：
///
/// ```ignore
/// let client = redis::Client::open(url)?;
/// let backend = RedisRateLimitBackend::new(client);
/// ```
pub struct RedisRateLimitBackend {
    #[allow(dead_code)]
    url: String,
}

impl RedisRateLimitBackend {
    /// 构造 Redis 后端占位（实际连接延迟到调用时由具体 redis 客户端完成）。
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

impl RateLimitBackend for RedisRateLimitBackend {
    fn record(&self, _key: &str) -> Result<usize, AppError> {
        Err(AppError::Internal(
            "Redis 速率限制后端尚未启用：请在 Cargo.toml 引入 redis 依赖并实现 INCR/EXPIRE 逻辑"
                .to_string(),
        ))
    }
    fn count(&self, _key: &str) -> Result<usize, AppError> {
        Err(AppError::Internal(
            "Redis 速率限制后端尚未启用：请在 Cargo.toml 引入 redis 依赖并实现查询逻辑".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_blocks_after_threshold() {
        let limiter = LoginRateLimiter::new(3, Duration::from_secs(60));
        let ip = "127.0.0.1".parse::<IpAddr>().unwrap();
        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_ok());
        // 第 4 次应被拦截
        assert!(limiter.check(ip).is_err());
    }

    #[test]
    fn test_different_ips_independent() {
        let limiter = LoginRateLimiter::new(1, Duration::from_secs(60));
        let a = "127.0.0.1".parse::<IpAddr>().unwrap();
        let b = "127.0.0.2".parse::<IpAddr>().unwrap();
        assert!(limiter.check(a).is_ok());
        assert!(limiter.check(b).is_ok(), "不同 IP 应独立计数");
        assert!(limiter.check(a).is_err());
    }
}
