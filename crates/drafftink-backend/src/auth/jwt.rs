//! JWT 服务：基于 `jsonwebtoken`（HS256）签发与校验访问/刷新令牌。
//!
//! - 访问令牌（AccessToken）：15 分钟有效，用于携带身份访问受保护接口。
//! - 刷新令牌（RefreshToken）：7 天有效，存储于可吊销的 [`crate::auth::refresh::RefreshTokenStore`]，
//!   用于在访问令牌过期后静默续期。
//!
//! 密钥来自环境变量 `DRAFTTINK_JWT_SECRET`，严禁硬编码。

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use uuid::Uuid;

use drafftink_core::auth::ACCESS_TOKEN_TTL_SECS;
use drafftink_core::auth::REFRESH_TOKEN_TTL_SECS;
use drafftink_core::{JwtClaims, User};

use crate::error::AppError;

/// 生成访问令牌（AccessToken，15 分钟有效）。
pub fn generate_access_token(
    user: &User,
    device_fp: &str,
    secret: &[u8],
) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = JwtClaims {
        sub: user.id.to_string(),
        name: user.username.clone(),
        role: user.role.as_str().to_string(),
        class_id: user.class_id.map(|id| id.to_string()),
        device_fp: device_fp.to_string(),
        tenant_id: user.tenant_id.to_string(),
        typ: Some("access".to_string()),
        iat: now.timestamp(),
        exp: (now + Duration::seconds(ACCESS_TOKEN_TTL_SECS)).timestamp(),
        jti: Uuid::new_v4().to_string(),
    };
    encode_token(&claims, secret)
}

/// 生成刷新令牌（RefreshToken，7 天有效）。
///
/// 返回 `(token, jti, expires_at_unix)`，调用方需将 `jti` 写入可吊销存储。
pub fn generate_refresh_token(
    user: &User,
    secret: &[u8],
) -> Result<(String, String, i64), AppError> {
    let now = Utc::now();
    let exp = (now + Duration::seconds(REFRESH_TOKEN_TTL_SECS)).timestamp();
    let jti = Uuid::new_v4().to_string();
    let claims = JwtClaims {
        sub: user.id.to_string(),
        name: user.username.clone(),
        role: user.role.as_str().to_string(),
        class_id: user.class_id.map(|id| id.to_string()),
        device_fp: String::new(),
        tenant_id: user.tenant_id.to_string(),
        typ: Some("refresh".to_string()),
        iat: now.timestamp(),
        exp,
        jti: jti.clone(),
    };
    let token = encode_token(&claims, secret)?;
    Ok((token, jti, exp))
}

fn encode_token(claims: &JwtClaims, secret: &[u8]) -> Result<String, AppError> {
    let key = EncodingKey::from_secret(secret);
    encode(&Header::new(Algorithm::HS256), claims, &key)
        .map_err(|e| AppError::Internal(format!("JWT 编码失败: {e}")))
}

/// 校验访问令牌，返回 Claims。
pub fn verify_access_token(token: &str, secret: &[u8]) -> Result<JwtClaims, AppError> {
    verify_token(token, secret, Some("access"))
}

/// 校验刷新令牌，返回 Claims。
pub fn verify_refresh_token(token: &str, secret: &[u8]) -> Result<JwtClaims, AppError> {
    verify_token(token, secret, Some("refresh"))
}

fn verify_token(
    token: &str,
    secret: &[u8],
    expected_typ: Option<&str>,
) -> Result<JwtClaims, AppError> {
    let key = DecodingKey::from_secret(secret);
    let mut validation = Validation::new(Algorithm::HS256);
    // 强制校验过期时间（库默认即开启，显式声明以防配置漂移）
    validation.validate_exp = true;
    let data = decode::<JwtClaims>(token, &key, &validation)
        .map_err(|e| AppError::Unauthorized(format!("令牌校验失败: {e}")))?;
    if let Some(exp) = expected_typ {
        match data.claims.typ.as_deref() {
            Some(t) if t == exp => {}
            _ => return Err(AppError::Unauthorized("令牌类型不匹配".to_string())),
        }
    }
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use drafftink_core::{Role, User};
    use uuid::Uuid;

    fn sample_user() -> User {
        User {
            id: Uuid::new_v4(),
            username: "u".into(),
            display_name: "U".into(),
            role: Role::Teacher,
            class_id: None,
            tenant_id: Uuid::new_v4(),
            password_hash: String::new(),
            created_at: Utc::now(),
            active: true,
        }
    }

    #[test]
    fn test_access_token_roundtrip() {
        let secret = b"test-secret-0123456789";
        let user = sample_user();
        let tok = generate_access_token(&user, "fp", secret).unwrap();
        let claims = verify_access_token(&tok, secret).unwrap();
        assert_eq!(claims.sub, user.id.to_string());
        assert_eq!(claims.typ.as_deref(), Some("access"));
        assert_eq!(claims.tenant_id, user.tenant_id.to_string());
    }

    #[test]
    fn test_wrong_secret_rejected() {
        let user = sample_user();
        let tok = generate_access_token(&user, "fp", b"secret-a").unwrap();
        assert!(verify_access_token(&tok, b"secret-b").is_err());
    }

    #[test]
    fn test_typ_mismatch_rejected() {
        let user = sample_user();
        let access = generate_access_token(&user, "fp", b"secret").unwrap();
        // 用刷新校验器验证访问令牌应被拒（typ 不符）
        assert!(verify_refresh_token(&access, b"secret").is_err());
    }

    #[test]
    fn test_refresh_token_roundtrip() {
        let secret = b"refresh-secret";
        let user = sample_user();
        let (tok, jti, exp) = generate_refresh_token(&user, secret).unwrap();
        let claims = verify_refresh_token(&tok, secret).unwrap();
        assert_eq!(claims.jti, jti);
        assert_eq!(claims.typ.as_deref(), Some("refresh"));
        assert!(exp > Utc::now().timestamp());
    }
}
