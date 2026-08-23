//! # 数据库相关数据结构
//!
//! 由于 `User.password_hash` 字段带有 `#[serde(skip)]`，
//! bincode 序列化时会跳过该字段。因此密码哈希需要单独存储。

use serde::{Deserialize, Serialize};

/// 用户凭据（密码哈希），单独存储在 sled 中
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCredentials {
    /// 密码哈希
    ///
    /// MVP 阶段直接存储明文密码用于演示。
    /// 生产环境应使用 Argon2 哈希，不要存储明文密码。
    pub password_hash: String,
}

/// sled 键前缀常量
pub(crate) const PREFIX_USER: &str = "user:";
pub(crate) const PREFIX_PWD: &str = "pwd:";
pub(crate) const PREFIX_USERNAME: &str = "username:";
pub(crate) const PREFIX_CLASS: &str = "class:";
pub(crate) const PREFIX_HW: &str = "hw:";
pub(crate) const PREFIX_HW_CLASS: &str = "hw_class:";
pub(crate) const PREFIX_HW_TEACHER: &str = "hw_teacher:";
pub(crate) const PREFIX_SUB: &str = "sub:";
pub(crate) const PREFIX_SUB_HW: &str = "sub_hw:";
pub(crate) const PREFIX_AUDIT: &str = "audit:";
