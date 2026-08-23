//! # 资源接口
//!
//! - `POST /api/resource/upload` — 上传文件
//! - `GET /api/resource/*path` — 下载文件

use axum::body::Bytes;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use uuid::Uuid;

use drafftink_core::Role;

use crate::auth::{require_role, AuthUser};
use crate::error::AppError;
use crate::state::AppState;

/// 上传响应
#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub path: String,
    pub size: usize,
}

/// POST /api/resource/upload
///
/// 接受 multipart 文件上传，存储到本地文件系统。
pub async fn upload(
    auth: AuthUser,
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<UploadResponse>, AppError> {
    // 仅老师/管理员可上传教学资源
    require_role(&auth, &[Role::Teacher, Role::Admin])?;
    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart 解析失败: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            filename = field
                .file_name()
                .map(|s| s.to_string());
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("读取文件失败: {e}")))?;
            file_data = Some(bytes.to_vec());
        }
    }

    let data = file_data.ok_or_else(|| AppError::BadRequest("缺少 file 字段".to_string()))?;
    let size = data.len();

    // 生成安全存储路径
    let resource_id = Uuid::new_v4();
    let safe_name = filename
        .as_deref()
        .map(|n| n.replace("..", "").replace(['/', '\\'], "_"))
        .unwrap_or_else(|| "unnamed".to_string());
    let path = format!("resources/{resource_id}/{safe_name}");

    state.storage.save(&path, data)?;

    Ok(Json(UploadResponse { path, size }))
}

/// GET /api/resource/*path
///
/// 下载资源文件。使用 catch-all 路由捕获包含子路径的资源路径。
pub async fn download(
    _auth: AuthUser,
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Result<Response, AppError> {
    let data = state.storage.load(&path)?;

    let mut response = Bytes::from(data).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        "application/octet-stream".parse().unwrap_or_else(|_| {
            axum::http::HeaderValue::from_static("application/octet-stream")
        }),
    );
    Ok(response)
}
