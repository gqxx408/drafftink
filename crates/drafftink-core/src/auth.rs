//! # 认证与授权数据契约（DTO）
//!
//! 定义登录/刷新相关的请求与响应结构体，以及标准 `Claims` 别名。
//! 这些类型位于 `drafftink-core`，以便后端与上层应用共享同一套合规数据底座。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::crypto::JwtClaims;
use crate::models::{Role, User};

pub use crate::crypto::JwtClaims as Claims;

/// 访问令牌有效期（秒）：15 分钟
pub const ACCESS_TOKEN_TTL_SECS: i64 = 15 * 60;
/// 刷新令牌有效期（秒）：7 天
pub const REFRESH_TOKEN_TTL_SECS: i64 = 7 * 24 * 60 * 60;

/// 登录请求
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// 用户名
    pub username: String,
    /// 明文密码（后端使用 Argon2 校验，不存储）
    pub password: String,
    /// 设备指纹（用于令牌绑定；可选，缺失时由请求头 `X-Device-Fp` 提供）
    #[serde(default)]
    pub device_fp: String,
}

/// 登录响应
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// 访问令牌（AccessToken，15 分钟有效）
    pub access_token: String,
    /// 刷新令牌（RefreshToken，7 天有效）
    pub refresh_token: String,
    /// Token 类型，固定为 `Bearer`
    pub token_type: String,
    /// 访问令牌剩余有效期（秒）
    pub expires_in: i64,
    /// 当前用户信息
    pub user: UserInfo,
}

/// 刷新请求（RefreshToken 可放在 Cookie 或请求体）
#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    /// 刷新令牌（可选；缺失时从 Cookie `refresh_token` 读取）
    #[serde(default)]
    pub refresh_token: Option<String>,
}

/// 刷新响应
#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    /// 新的访问令牌
    pub access_token: String,
    /// 新的刷新令牌（轮换）
    pub refresh_token: String,
    /// Token 类型，固定为 `Bearer`
    pub token_type: String,
    /// 访问令牌剩余有效期（秒）
    pub expires_in: i64,
}

/// 返回给客户端的安全用户信息（不含密码哈希）
#[derive(Debug, Serialize)]
pub struct UserInfo {
    /// 用户唯一 ID
    pub id: Uuid,
    /// 用户名
    pub username: String,
    /// 显示名称
    pub display_name: String,
    /// 角色字符串（admin / teacher / student）
    pub role: String,
    /// 所属班级 ID（学生专用）
    pub class_id: Option<Uuid>,
    /// 租户 ID（学校 ID），用于数据隔离
    pub tenant_id: Uuid,
}

impl From<&User> for UserInfo {
    fn from(u: &User) -> Self {
        Self {
            id: u.id,
            username: u.username.clone(),
            display_name: u.display_name.clone(),
            role: u.role.as_str().to_string(),
            class_id: u.class_id,
            tenant_id: u.tenant_id,
        }
    }
}

/// 将 JWT Claims 还原为 `Role`（未知角色按最小权限 `Student` 处理）
pub fn claims_role(claims: &JwtClaims) -> Role {
    claims.role.parse::<Role>().unwrap_or(Role::Student)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_info_from_user_includes_tenant() {
        let user = User {
            id: Uuid::new_v4(),
            username: "alice".into(),
            display_name: "Alice".into(),
            role: Role::Teacher,
            class_id: None,
            tenant_id: Uuid::new_v4(),
            password_hash: String::new(),
            created_at: chrono::Utc::now(),
            active: true,
        };
        let info = UserInfo::from(&user);
        assert_eq!(info.username, "alice");
        assert_eq!(info.role, "teacher");
        assert_eq!(info.tenant_id, user.tenant_id);
    }
}
