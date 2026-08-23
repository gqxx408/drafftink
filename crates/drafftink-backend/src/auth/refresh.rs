//! 刷新令牌存储：支持主动吊销（登出 / 轮换），满足安全合规要求。
//!
//! 规范要求使用 Redis 存储；当前仓库未引入 Redis 依赖，这里以 sled 提供持久化实现，
//! 并通过 [`RefreshTokenStore`] trait 抽象，生产环境可无缝替换为 Redis 实现。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;

/// 刷新令牌存储抽象。实现可插拔（内存 / sled / Redis）。
pub trait RefreshTokenStore: Send + Sync {
    /// 记录一个刷新令牌（按 jti 索引，附带过期时间）。
    fn store(&self, jti: &str, expires_at: i64);
    /// 该 jti 是否已被吊销。
    fn is_revoked(&self, jti: &str) -> bool;
    /// 吊销一个 jti（登出 / 轮换时使用）。
    fn revoke(&self, jti: &str);
}

/// 内存实现（主要用于单元测试与单实例开发环境）。
pub struct MemoryRefreshTokenStore {
    revoked: Mutex<HashMap<String, bool>>,
}

impl MemoryRefreshTokenStore {
    pub fn new() -> Self {
        Self {
            revoked: Mutex::new(HashMap::new()),
        }
    }
}

impl RefreshTokenStore for MemoryRefreshTokenStore {
    fn store(&self, _jti: &str, _expires_at: i64) {
        // 内存实现中刷新令牌的存活由 JWT 自身 exp 控制，仅需记录吊销集合
    }
    fn is_revoked(&self, jti: &str) -> bool {
        self.revoked.lock().unwrap().contains_key(jti)
    }
    fn revoke(&self, jti: &str) {
        self.revoked.lock().unwrap().insert(jti.to_string(), true);
    }
}

impl Default for MemoryRefreshTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

/// sled 持久化实现（默认生产实现，重启后仍可保持吊销记录）。
pub struct SledRefreshTokenStore {
    db: sled::Db,
}

impl SledRefreshTokenStore {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }

    /// 单次过期清理：扫描 `tok:` 与 `rev:` 命名空间，删除已过期的记录，避免数据库无限增长。
    ///
    /// - `tok:` 记录承载令牌原始过期时间，过期即失效，可安全删除。
    /// - `rev:` 记录承载令牌原始过期时间，令牌过期后吊销记录无意义，可清理。
    ///
    /// 返回本次删除的条目数。
    pub fn sweep_once(&self) -> usize {
        sweep_expired(&self.db)
    }

    /// 启动后台过期清理线程：每隔 `interval` 执行一次 [`Self::sweep_once`]。
    ///
    /// 返回 `JoinHandle`，调用方可保留以便优雅退出。当前为无限循环守护线程，
    /// 仅在需要持久化刷新令牌存储的生产部署中调用。
    pub fn start_expiry_sweeper(&self, interval: Duration) -> std::thread::JoinHandle<()> {
        let db = self.db.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(interval);
            sweep_expired(&db);
        })
    }
}

/// 清理过期的 `tok:` / `rev:` 记录，返回删除的条目数。
///
/// `tok:` 记录 value 为 8 字节大端 `i64` 过期时间；`rev:` 记录 value 同样为该过期时间。
/// 仅当字节长度正确且已过期时才删除。
fn sweep_expired(db: &sled::Db) -> usize {
    let now = Utc::now().timestamp();
    let mut removed = 0usize;
    for prefix in [b"tok:" as &[u8], b"rev:"] {
        let keys: Vec<_> = db
            .scan_prefix(prefix)
            .filter_map(|kv| match kv {
                Ok((k, v)) if v.len() == 8 => {
                    let exp = i64::from_be_bytes(v.as_ref().try_into().unwrap_or([0u8; 8]));
                    if exp < now {
                        Some(k)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        for k in keys {
            if db.remove(k).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

impl RefreshTokenStore for SledRefreshTokenStore {
    fn store(&self, jti: &str, expires_at: i64) {
        // 记录刷新令牌的存活信息（独立于吊销命名空间），供登出 / 轮换时吊销。
        let _ = self.db.insert(tok_key(jti), expires_at.to_be_bytes().to_vec());
    }
    fn is_revoked(&self, jti: &str) -> bool {
        // 仅检查吊销命名空间，避免与 store 写入冲突
        self.db.contains_key(rev_key(jti)).unwrap_or(false)
    }
    fn revoke(&self, jti: &str) {
        // 吊销时记录该令牌原本的过期时间，便于后台清理（令牌过期后吊销记录即无意义）。
        let exp = self
            .db
            .get(tok_key(jti))
            .ok()
            .flatten()
            .filter(|v| v.len() == 8)
            .map(|v| i64::from_be_bytes(v.as_ref().try_into().unwrap_or([0u8; 8])))
            .unwrap_or(0);
        let _ = self.db.insert(rev_key(jti), exp.to_be_bytes().to_vec());
    }
}

#[inline]
fn rev_key(jti: &str) -> Vec<u8> {
    format!("rev:{jti}").into_bytes()
}

#[inline]
fn tok_key(jti: &str) -> Vec<u8> {
    format!("tok:{jti}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_store_revoke() {
        let s = MemoryRefreshTokenStore::new();
        assert!(!s.is_revoked("j1"));
        s.store("j1", 0);
        s.revoke("j1");
        assert!(s.is_revoked("j1"));
    }

    #[test]
    fn test_sled_store_revoke() {
        let dir = std::env::temp_dir().join(format!("drafftink_refresh_test_{}", uuid::Uuid::new_v4()));
        let s = SledRefreshTokenStore::open(&dir).unwrap();
        s.store("j2", 0);
        assert!(!s.is_revoked("j2"));
        s.revoke("j2");
        assert!(s.is_revoked("j2"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_sweep_removes_expired_tokens() {
        let dir = std::env::temp_dir().join(format!("drafftink_refresh_sweep_{}", uuid::Uuid::new_v4()));
        let s = SledRefreshTokenStore::open(&dir).unwrap();

        // 过期的 tok: 记录应被清理
        let past = Utc::now().timestamp() - 1000;
        s.store("expired_jti", past);
        assert_eq!(s.sweep_once(), 1, "过期的 tok: 记录应被清理");
        assert!(
            s.db.get(tok_key("expired_jti")).unwrap().is_none(),
            "tok: 应已删除"
        );

        // revoke 后 rev: 记录携带过期时间，过期后同样应被清理
        s.db
            .insert(rev_key("rev_jti"), past.to_be_bytes().to_vec())
            .unwrap();
        assert_eq!(s.sweep_once(), 1, "过期的 rev: 记录应被清理");
        assert!(
            s.db.get(rev_key("rev_jti")).unwrap().is_none(),
            "rev: 应已删除"
        );

        // 未过期的记录不应被清理
        let future = Utc::now().timestamp() + 3600;
        s.store("live_jti", future);
        assert_eq!(s.sweep_once(), 0, "未过期的记录应保留");
        assert!(
            s.db.get(tok_key("live_jti")).unwrap().is_some(),
            "未过期 tok: 应保留"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
