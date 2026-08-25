//! # API 路由设置
//!
//! 所有 HTTP 路由在此注册。认证采用分层路由：
//! - `public`：健康检查和登录/刷新/登出等无需认证的接口。
//! - `protected`：所有需 JWT 认证的接口，由 [`auth::auth_middleware`] 统一鉴权。
//! - `admin`：仅管理员可访问的接口，由 [`auth::admin_middleware`] 鉴权 + 角色校验。

pub mod auth;
pub mod etl;
pub mod health;
pub mod homework;
pub mod mobile;
pub mod recording;
pub mod resource;
pub mod standards;

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::auth::{admin_middleware, auth_middleware};
use crate::state::AppState;

/// 构建 API 路由
pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 限制请求体大小为 50MB（作业文件可能较大）
    let body_limit = RequestBodyLimitLayer::new(50 * 1024 * 1024);

    // 公开路由：无需认证
    let public = Router::new()
        .route("/api/health", get(health::health))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/refresh", post(auth::refresh))
        .route("/api/auth/logout", post(auth::logout))
        // 移动办公：登录（含设备指纹绑定 + 下发短信验证码）
        .route("/api/mobile/login", post(mobile::mobile_login))
        // 移动办公：短信二次验证（MFA），通过后签发 SSO 令牌
        .route("/api/mobile/mfa/verify", post(mobile::mfa_verify))
        // 方向一：国标代码表只读查询（公开，无需认证）
        .route("/api/v1/lookup/:table", get(standards::lookup))
        // 方向二衔接：CSV 在线清洗（复用 drafftink-etl，内存处理，不落盘）
        .route("/api/v1/etl/clean-csv", post(etl::clean_csv));

    // 受保护路由：所有接口均需 JWT 认证（由中间件注入 AuthContext）
    let protected = Router::new()
        .route("/api/auth/me", get(auth::me))
        .route("/api/homework/create", post(homework::create))
        .route("/api/homework/list", get(homework::list))
        .route("/api/homework/:id", get(homework::get))
        .route("/api/homework/submit", post(homework::submit))
        .route("/api/homework/grade", post(homework::grade))
        .route("/api/resource/upload", post(resource::upload))
        .route("/api/resource/*path", get(resource::download))
        // 录播资源管理平台（复用 RBAC）
        .route("/api/recording/resource", post(recording::publish_resource))
        .route(
            "/api/recording/resource/search",
            get(recording::search_resource),
        )
        .route("/api/recording/resource/:id", get(recording::get_resource))
        .route(
            "/api/recording/resource/:id/comments",
            get(recording::get_resource_comments),
        )
        // 教师端发布直播帧（含自动导播信号）
        .route("/api/live/:room_id/frame", post(recording::publish_frame))
        // 多租户数据隔离演示接口
        .route("/api/tenant/:id", get(auth::tenant_view))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // 移动办公受保护路由：复用统一认证中间件 + RBAC
    let mobile_protected = Router::new()
        .route("/api/mobile/todos", get(mobile::todos))
        .route("/api/mobile/workflow/start", post(mobile::workflow_start))
        .route("/api/mobile/workflow/:id", get(mobile::workflow_get))
        .route(
            "/api/mobile/workflow/approve",
            post(mobile::workflow_approve),
        )
        .route("/api/mobile/announcements", get(mobile::announcements))
        .route("/api/mobile/meeting/book", post(mobile::meeting_book))
        .route("/api/mobile/seal/apply", post(mobile::seal_apply))
        .route("/api/mobile/messages", get(mobile::messages))
        .route("/api/mobile/sso/token", get(mobile::sso_token))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // 管理员专用路由：认证 + 仅 Admin 可访问
    let admin = Router::new()
        .route("/api/admin/schools/:id", get(auth::admin_school))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            admin_middleware,
        ));

    // 直播 WebSocket 路由：手动 JWT 鉴权（?token= / Authorization 头），不经 auth_middleware
    let live = Router::new().route("/api/live/:room_id", get(recording::live_ws));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(mobile_protected)
        .merge(admin)
        .merge(live)
        // 全局中间件
        .layer(body_limit)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
