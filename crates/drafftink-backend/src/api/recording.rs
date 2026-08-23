//! # recording API — 录播与资源发布 HTTP 接口
//!
//! 路由（在 [`crate::api::router`] 中注册）：
//!
//! - `GET  /api/live/:room_id` — 直播 WebSocket（B/S 架构，延时 ≤ 3 秒）
//! - `POST /api/live/:room_id/frame` — 教师端发布直播帧（含自动导播信号）
//! - `POST /api/recording/resource` — 发布课件资源（教师/管理员）
//! - `GET  /api/recording/resource/search?q=` — 按 JY/T 1004 字段检索
//! - `GET  /api/recording/resource/:id` — 获取资源元数据（点播权限）
//! - `GET  /api/recording/resource/:id/comments` — 查看评语（评语权限）

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::http::HeaderMap;
use axum::response::Response;
use std::collections::HashMap;
use axum::Json;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chrono::Utc;
use drafftink_core::recording::{
    CoursewareResource, DirectingSignals, LiveView, RecordingParams, ResourcePermission,
};
use drafftink_core::{auth::claims_role, Role};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::jwt;
use crate::auth::{require_role, AuthUser};
use crate::error::AppError;
use crate::recording::{live::handle_socket, LiveFrame, ResourceManager};
use crate::recording::resource::{PublishResourceRequest, SearchQuery};
use crate::state::AppState;

/// 直播 WebSocket 接口：基于现有公网网关，B/S 架构，延时 ≤ 3 秒。
///
/// 鉴权复用现有 JWT（通过 `?token=` 或 `Authorization` 头传递），并校验直播接收权限。
/// 升级过程交由 axum 内建 `WebSocketUpgrade` 完成（自动处理 RFC 6455 握手）。
pub async fn live_ws(
    Path(room_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Result<Response, AppError> {
    // 1. 提取并校验访问令牌
    let token = extract_live_token(&headers, &params)?;
    let claims = jwt::verify_access_token(&token, &state.config.jwt.secret)?;
    let role = claims_role(&claims);
    if !matches!(role, Role::Admin | Role::Teacher | Role::Student) {
        return Err(AppError::Forbidden("无权接收直播".to_string()));
    }

    // 2. 升级为 WebSocket，并在连接内驱动直播帧广播 / 导播控制
    let hub = state.live.clone();
    Ok(ws.on_upgrade(move |socket| async move {
        handle_socket(socket, hub, room_id, role).await;
    }))
}

/// 教师端发布直播帧（携带可选自动导播信号）。仅教师/管理员可调用。
pub async fn publish_frame(
    Path(room_id): Path<String>,
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<PublishFrameRequest>,
) -> Result<Json<Value>, AppError> {
    require_role(&auth, &[Role::Teacher, Role::Admin])?;
    let frame = LiveFrame::media(req.view, req.data);
    // 若提供导播信号，hub 内部自动导播选择最优视角后再广播
    state.live.publish(&room_id, frame, req.signals);
    Ok(Json(json!({ "ok": true })))
}

/// 发布课件资源到资源管理平台（自动上传存储 + 建立 JY/T 1004 分类索引）。
pub async fn publish_resource(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<PublishResourceRequest>,
) -> Result<Json<Value>, AppError> {
    require_role(&auth, &[Role::Teacher, Role::Admin])?;
    let resource_id = req.resource_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let storage_key = CoursewareResource::storage_key_for(&resource_id);
    let meta = CoursewareResource {
        resource_id: resource_id.clone(),
        title: req.title,
        classification: req.classification,
        params: req.params.unwrap_or_else(RecordingParams::standard),
        mode: req.mode,
        permission: req.permission.unwrap_or_else(ResourcePermission::public),
        storage_key: storage_key.clone(),
        drftx_key: req.drftx_key,
        created_at: Utc::now().to_rfc3339(),
    };
    let mgr = ResourceManager::new(state.db.clone(), state.storage.clone());
    mgr.publish(&meta, req.data)?;
    Ok(Json(json!({
        "resource_id": resource_id,
        "storage_key": storage_key,
    })))
}

/// 按关键字检索课件资源（教师姓名 / 课件名称 / 章节索引等 JY/T 1004 字段）。
pub async fn search_resource(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<CoursewareResource>>, AppError> {
    require_role(&auth, &[Role::Teacher, Role::Student, Role::Admin])?;
    let mgr = ResourceManager::new(state.db.clone(), state.storage.clone());
    Ok(Json(mgr.search(&q.q)))
}

/// 获取课件资源元数据（点播权限校验）。
pub async fn get_resource(
    Path(resource_id): Path<String>,
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<CoursewareResource>, AppError> {
    require_role(&auth, &[Role::Teacher, Role::Student, Role::Admin])?;
    let mgr = ResourceManager::new(state.db.clone(), state.storage.clone());
    let meta = mgr.get(&resource_id)?;
    if !meta.permission.can_vod(claims_role(&auth.0)) {
        return Err(AppError::Forbidden("无权点播该资源".to_string()));
    }
    Ok(Json(meta))
}

/// 查看课件评语（评语查看权限校验）。
pub async fn get_resource_comments(
    Path(resource_id): Path<String>,
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    require_role(&auth, &[Role::Teacher, Role::Student, Role::Admin])?;
    let mgr = ResourceManager::new(state.db.clone(), state.storage.clone());
    let meta = mgr.get(&resource_id)?;
    if !meta.permission.can_comment(claims_role(&auth.0)) {
        return Err(AppError::Forbidden("无权查看该资源评语".to_string()));
    }
    Ok(Json(json!({ "resource_id": resource_id, "comment_view": true })))
}

/// 提取直播访问令牌：优先 `Authorization: Bearer`，其次 `?token=`。
fn extract_live_token(
    headers: &HeaderMap,
    params: &HashMap<String, String>,
) -> Result<String, AppError> {
    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        if let Ok(s) = auth.to_str() {
            if let Some(t) = s.strip_prefix("Bearer ") {
                return Ok(t.trim().to_string());
            }
        }
    }
    if let Some(token) = params.get("token") {
        return Ok(token.clone());
    }
    Err(AppError::Unauthorized("缺少直播访问令牌".to_string()))
}

/// 教师端发布直播帧的请求体。
#[derive(Debug, Deserialize)]
pub struct PublishFrameRequest {
    /// 导播视角
    pub view: LiveView,
    /// 画面数据（base64 编码）
    #[serde(deserialize_with = "b64_deserialize")]
    pub data: Vec<u8>,
    /// 自动导播信号（可选）
    #[serde(default)]
    pub signals: Option<DirectingSignals>,
}

/// 将 base64 字符串反序列化为字节（供直播帧 / 资源上传复用）。
fn b64_deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    let s = String::deserialize(d)?;
    B64.decode(s).map_err(serde::de::Error::custom)
}
