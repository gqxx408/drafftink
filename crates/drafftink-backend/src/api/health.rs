//! # 健康检查接口

use axum::Json;
use serde::Serialize;

/// GET /api/health
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
}
