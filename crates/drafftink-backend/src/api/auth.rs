//! # 认证接口
//!
//! 提供：
//! - `POST /api/auth/login`：用户名/密码登录（Argon2 校验 + 限流），签发 Access/Refresh 令牌，
//!   并以 `HttpOnly; Secure; SameSite=Strict` Cookie 下发，同时初始化用户共享上下文。
//! - `POST /api/auth/refresh`：用 Refresh 令牌静默续期（轮换并吊销旧令牌）。
//! - `POST /api/auth/logout`：吊销 Refresh 令牌并清除 Cookie。
//! - `GET  /api/auth/me`：返回当前登录用户信息（需认证）。
//! - `GET  /api/admin/schools/:id`：管理员专用（需认证 + Admin 角色）。
//! - `GET  /api/tenant/:id`：多租户数据隔离演示（需认证，强制 tenant_id 一致）。

use axum::extract::{Json, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use drafftink_core::auth::{
    claims_role, ACCESS_TOKEN_TTL_SECS, REFRESH_TOKEN_TTL_SECS, LoginRequest, LoginResponse,
    RefreshRequest, RefreshResponse, UserInfo,
};
use drafftink_core::integration::SharedAppContext;

use crate::auth::jwt;
use crate::auth::password::verify_password;
use crate::auth::rbac::ensure_tenant_access;
use crate::auth::{client_ip, require_role, AuthUser};
use crate::error::AppError;
use crate::state::AppState;

/// 登录接口
#[tracing::instrument(skip(state))]
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Response, AppError> {
    // 1) 速率限制（按客户端 IP，防暴力破解）
    state.login_ratelimit.check(client_ip(&headers))?;

    // 2) 查找用户
    let user = state
        .db
        .get_user_by_username(&req.username)?
        .ok_or_else(|| AppError::Unauthorized("用户名或密码错误".to_string()))?;

    if !user.active {
        return Err(AppError::Unauthorized("账户已被禁用".to_string()));
    }

    // 3) 校验密码（Argon2id）
    if !verify_password(&req.password, &user.password_hash) {
        return Err(AppError::Unauthorized("用户名或密码错误".to_string()));
    }

    // 4) 设备指纹（优先请求头，其次请求体）
    let device_fp = if req.device_fp.is_empty() {
        headers
            .get("x-device-fp")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string()
    } else {
        req.device_fp.clone()
    };

    // 5) 签发 Access / Refresh 令牌
    let secret = state.config.jwt.secret.clone();
    let access = jwt::generate_access_token(&user, &device_fp, &secret)?;
    let (refresh, jti, exp) = jwt::generate_refresh_token(&user, &secret)?;
    // 记录刷新令牌，支持主动吊销
    state.refresh_store.store(&jti, exp);

    // 6) 登录后初始化该用户的共享上下文（任务要求）
    {
        let mut sessions = state.sessions.lock().unwrap();
        sessions.insert(
            user.id,
            SharedAppContext {
                account: Some(user.username.clone()),
                jwt_token: Some(access.clone()),
                backend_url: state.config.listen_addr.clone(),
                ..Default::default()
            },
        );
    }

    // 7) 返回 JSON + 安全 Cookie
    let body = LoginResponse {
        access_token: access.clone(),
        refresh_token: refresh.clone(),
        token_type: "Bearer".to_string(),
        expires_in: ACCESS_TOKEN_TTL_SECS,
        user: UserInfo::from(&user),
    };
    Ok(build_token_response(&body, &access, &refresh))
}

/// 刷新令牌接口：轮换并吊销旧令牌
pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RefreshRequest>,
) -> Result<Response, AppError> {
    let token = req
        .refresh_token
        .clone()
        .or_else(|| cookie_value(&headers, "refresh_token"))
        .ok_or_else(|| AppError::Unauthorized("缺少刷新令牌".to_string()))?;

    let secret = state.config.jwt.secret.clone();
    let claims = jwt::verify_refresh_token(&token, &secret)?;

    // 已吊销（登出 / 旧令牌）则拒绝
    if state.refresh_store.is_revoked(&claims.jti) {
        return Err(AppError::Unauthorized(
            "刷新令牌已失效，请重新登录".to_string(),
        ));
    }

    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::BadRequest("无效的令牌主体".to_string()))?;
    let user = state
        .db
        .get_user(user_id)?
        .ok_or_else(|| AppError::Unauthorized("用户不存在".to_string()))?;

    // 轮换：吊销旧刷新令牌，签发新一对
    state.refresh_store.revoke(&claims.jti);
    let access = jwt::generate_access_token(&user, &claims.device_fp, &secret)?;
    let (new_refresh, new_jti, new_exp) = jwt::generate_refresh_token(&user, &secret)?;
    state.refresh_store.store(&new_jti, new_exp);

    let body = RefreshResponse {
        access_token: access.clone(),
        refresh_token: new_refresh.clone(),
        token_type: "Bearer".to_string(),
        expires_in: ACCESS_TOKEN_TTL_SECS,
    };
    Ok(build_token_response(&body, &access, &new_refresh))
}

/// 登出接口：吊销刷新令牌并清除 Cookie
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if let Some(token) = cookie_value(&headers, "refresh_token") {
        if let Ok(claims) = jwt::verify_refresh_token(&token, &state.config.jwt.secret) {
            state.refresh_store.revoke(&claims.jti);
        }
    }
    let clear_ac = "access_token=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0";
    let clear_rf = "refresh_token=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0";
    let mut resp = (StatusCode::OK, Json(json!({ "message": "已退出登录" }))).into_response();
    if let Ok(v) = HeaderValue::from_str(clear_ac) {
        resp.headers_mut().append(header::SET_COOKIE, v);
    }
    if let Ok(v) = HeaderValue::from_str(clear_rf) {
        resp.headers_mut().append(header::SET_COOKIE, v);
    }
    Ok(resp)
}

/// 当前用户信息（需认证）
pub async fn me(auth: AuthUser) -> Result<Json<UserInfo>, AppError> {
    let info = UserInfo {
        id: Uuid::parse_str(&auth.0.sub).unwrap_or_default(),
        username: auth.0.name.clone(),
        display_name: auth.0.name.clone(),
        role: auth.0.role.clone(),
        class_id: auth.0.class_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        tenant_id: Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default(),
    };
    Ok(Json(info))
}

/// 管理员专用：查询指定学校信息（跨租户可见）
pub async fn admin_school(
    Path(school_id): Path<String>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    require_role(&auth, &[drafftink_core::Role::Admin])?;
    // 管理员可跨校访问（数据隔离对 Admin 豁免），此处仅作演示返回
    Ok(Json(json!({
        "school_id": school_id,
        "accessible": true,
    })))
}

/// 多租户数据隔离演示：仅当 `tenant_id` 与当前用户租户一致（或管理员）时放行
pub async fn tenant_view(
    State(_state): State<AppState>,
    Path(tenant_id): Path<String>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let role = claims_role(&auth.0);
    ensure_tenant_access(&auth.0.tenant_id, &tenant_id, role)?;
    Ok(Json(json!({
        "tenant_id": tenant_id,
        "allowed": true,
    })))
}

/// 构造带安全 Cookie 的令牌响应
fn build_token_response<T: Serialize>(body: &T, access: &str, refresh: &str) -> Response {
    let ac = cookie_string("access_token", access, ACCESS_TOKEN_TTL_SECS as u64);
    let rf = cookie_string("refresh_token", refresh, REFRESH_TOKEN_TTL_SECS as u64);
    let mut resp = (StatusCode::OK, Json(body)).into_response();
    if let Ok(v) = HeaderValue::from_str(&ac) {
        resp.headers_mut().append(header::SET_COOKIE, v);
    }
    if let Ok(v) = HeaderValue::from_str(&rf) {
        resp.headers_mut().append(header::SET_COOKIE, v);
    }
    resp
}

/// 构造 HttpOnly + Secure + SameSite=Strict 的 Cookie 字符串
fn cookie_string(name: &str, value: &str, max_age_secs: u64) -> String {
    format!(
        "{name}={value}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age={max_age_secs}"
    )
}

/// 从 `Cookie` 头中按名称解析 Cookie 值。
fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        let part = part.trim();
        let mut it = part.splitn(2, '=');
        let (k, v) = (it.next()?, it.next()?);
        if k == name {
            return Some(v.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::mobile::MobileAuth;
    use crate::auth::password::hash_password;
    use crate::workflow::WorkflowStore;
    use crate::auth::ratelimit::LoginRateLimiter;
    use crate::auth::refresh::MemoryRefreshTokenStore;
    use crate::config::BackendConfig;
    use crate::recording::LiveHub;
    use crate::db::{Database, SledDb};
    use std::sync::Arc;

    fn test_state() -> AppState {
        let db: Arc<dyn Database> = Arc::new(SledDb::open(&std::env::temp_dir().join(format!(
            "drafftink_auth_test_{}",
            Uuid::new_v4()
        ))).unwrap());
        let storage: Arc<dyn crate::storage::Storage> =
            Arc::new(crate::storage::LocalStorage::new(
                &std::env::temp_dir().join(format!("drafftink_auth_store_{}", Uuid::new_v4())),
            ).unwrap());
        AppState {
            db: db.clone(),
            storage,
            config: BackendConfig::default(),
            sessions: Arc::new(std::sync::Mutex::new(Default::default())),
            login_ratelimit: Arc::new(LoginRateLimiter::new(5, std::time::Duration::from_secs(60))),
            refresh_store: Arc::new(MemoryRefreshTokenStore::new()),
            workflow: WorkflowStore::new(),
            mobile_auth: MobileAuth::new(),
            live: LiveHub::new(),
        }
    }

    #[tokio::test]
    async fn test_login_wrong_password_returns_401() {
        let state = test_state();
        let username = format!("u_{}", Uuid::new_v4());
        let user = drafftink_core::User {
            id: Uuid::new_v4(),
            username: username.clone(),
            display_name: "T".into(),
            role: drafftink_core::Role::Teacher,
            class_id: None,
            tenant_id: Uuid::new_v4(),
            password_hash: hash_password("right-pass"),
            created_at: chrono::Utc::now(),
            active: true,
        };
        state.db.save_user(&user).unwrap();

        let resp = login(
            State(state.clone()),
            HeaderMap::new(),
            Json(LoginRequest {
                username,
                password: "wrong-pass".into(),
                device_fp: String::new(),
            }),
        )
        .await
        .unwrap_err();
        // 错误密码应返回未认证
        assert!(format!("{resp:?}").contains("Unauthorized") || matches!(resp, AppError::Unauthorized(_)));
    }

    #[tokio::test]
    async fn test_login_success_issues_tokens() {
        let state = test_state();
        let username = format!("u_{}", Uuid::new_v4());
        let tenant = Uuid::new_v4();
        let user = drafftink_core::User {
            id: Uuid::new_v4(),
            username: username.clone(),
            display_name: "T".into(),
            role: drafftink_core::Role::Teacher,
            class_id: None,
            tenant_id: tenant,
            password_hash: hash_password("secret"),
            created_at: chrono::Utc::now(),
            active: true,
        };
        state.db.save_user(&user).unwrap();

        let resp = login(
            State(state.clone()),
            HeaderMap::new(),
            Json(LoginRequest {
                username: username.clone(),
                password: "secret".into(),
                device_fp: "fp1".into(),
            }),
        )
        .await
        .unwrap();

        // 响应应包含 Set-Cookie（HttpOnly + Secure + SameSite=Strict）
        let headers = resp.headers();
        let set_cookie = headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap_or("").to_string())
            .collect::<Vec<_>>();
        assert!(
            set_cookie.iter().any(|c| c.contains("access_token=") && c.contains("HttpOnly") && c.contains("SameSite=Strict")),
            "缺少安全 Cookie: {set_cookie:?}"
        );
        // 用户上下文应已初始化
        assert!(state.sessions.lock().unwrap().contains_key(&user.id));
    }
}
