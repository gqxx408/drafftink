//! # 认证与授权
//!
//! - [`auth_middleware`]：Axum 中间件，从 `Authorization: Bearer` 头或 `access_token` Cookie
//!   提取并校验 JWT，将解析出的 [`AuthContext`] 注入请求扩展，供后续 Handler 使用。
//! - [`admin_middleware`]：认证 + 仅管理员可访问的组合中间件。
//! - [`AuthUser`]：Handler 提取器，从请求扩展中取出当前用户 Claims。
//! - [`require_role`]：函数式 RBAC 检查，可在 Handler 内部对角色做细粒度控制。

pub mod jwt;
pub mod mobile;
pub mod password;
pub mod ratelimit;
pub mod rbac;
pub mod refresh;

use async_trait::async_trait;
use axum::extract::{Request, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use drafftink_core::{auth::claims_role, JwtClaims, Role};

use crate::error::AppError;
use crate::state::AppState;

/// 注入到请求扩展中的认证上下文（由中间件写入，供 Handler 读取）。
#[derive(Clone)]
pub struct AuthContext {
    pub claims: JwtClaims,
}

/// 已认证用户提取器：从请求扩展中读取 [`AuthContext`]。
///
/// 仅在路由已被 [`auth_middleware`] / [`admin_middleware`] 包裹时可用。
pub struct AuthUser(pub JwtClaims);

#[async_trait]
impl axum::extract::FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthContext>()
            .map(|ctx| AuthUser(ctx.claims.clone()))
            .ok_or_else(|| AppError::Unauthorized("未认证或会话已过期".to_string()))
    }
}

/// 认证中间件：从 Header 或 Cookie 提取 Bearer Token，校验后注入 [`AuthContext`]。
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    match extract_and_verify(&req, &state) {
        Ok(claims) => {
            req.extensions_mut().insert(AuthContext { claims });
            next.run(req).await
        }
        Err(e) => e.into_response(),
    }
}

/// 管理员专用中间件：认证 + 校验 Admin 角色。
pub async fn admin_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    match extract_and_verify(&req, &state) {
        Ok(claims) => {
            if claims_role(&claims) != Role::Admin {
                return AppError::Forbidden("仅管理员可访问该接口".to_string()).into_response();
            }
            req.extensions_mut().insert(AuthContext { claims });
            next.run(req).await
        }
        Err(e) => e.into_response(),
    }
}

/// 从请求中提取并校验访问令牌，返回 Claims。
fn extract_and_verify(req: &Request, state: &AppState) -> Result<JwtClaims, AppError> {
    let token = extract_bearer(req.headers())
        .or_else(|| extract_cookie(req))
        .ok_or_else(|| AppError::Unauthorized("缺少访问令牌".to_string()))?;
    jwt::verify_access_token(&token, &state.config.jwt.secret)
}

/// 从 `Authorization: Bearer` 头提取令牌。
fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .map(|t| t.trim().to_string())
}

/// 从 `Cookie` 头解析 `access_token`。
fn extract_cookie(req: &Request) -> Option<String> {
    let cookie = req.headers().get(header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        let part = part.trim();
        let mut it = part.splitn(2, '=');
        let (k, v) = (it.next()?, it.next()?);
        if k == "access_token" {
            return Some(v.to_string());
        }
    }
    None
}

/// 函数式 RBAC 检查：要求 `auth` 所属角色在 `allowed` 集合中。
///
/// 示例：`require_role(&auth, &[Role::Teacher, Role::Admin])?;`
pub fn require_role(auth: &AuthUser, allowed: &[Role]) -> Result<(), AppError> {
    let role = claims_role(&auth.0);
    if allowed.contains(&role) {
        Ok(())
    } else {
        Err(AppError::Forbidden(format!(
            "权限不足：需要 {allowed:?} 之一，当前为 {role:?}"
        )))
    }
}

/// 提取客户端 IP（用于登录限流）。
///
/// 优先读取反向代理提供的 `X-Forwarded-For` / `X-Real-IP`，回退到未指定地址。
pub fn client_ip(headers: &HeaderMap) -> std::net::IpAddr {
    let try_parse = |h: Option<&axum::http::HeaderValue>| -> Option<std::net::IpAddr> {
        let s = h?.to_str().ok()?;
        let first = s.split(',').next()?.trim();
        first.parse().ok()
    };
    try_parse(headers.get("x-forwarded-for"))
        .or_else(|| try_parse(headers.get("x-real-ip")))
        .unwrap_or_else(|| "0.0.0.0".parse().unwrap())
}
