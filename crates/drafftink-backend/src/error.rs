//! # 错误类型
//!
//! `AppError` 实现了 `IntoResponse`，将错误转换为合适的 HTTP 响应。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// 应用错误类型
#[derive(Debug)]
pub enum AppError {
    /// 资源未找到
    NotFound(String),
    /// 权限不足
    Forbidden(String),
    /// 请求参数错误
    BadRequest(String),
    /// 未认证
    Unauthorized(String),
    /// 请求过于频繁（限流）
    TooManyRequests(String),
    /// 内部错误
    Internal(String),
}

/// JSON 错误响应体
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

impl AppError {
    /// 获取 HTTP 状态码
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// 获取错误类别字符串
    fn error_kind(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::Forbidden(_) => "forbidden",
            Self::BadRequest(_) => "bad_request",
            Self::Unauthorized(_) => "unauthorized",
            Self::TooManyRequests(_) => "too_many_requests",
            Self::Internal(_) => "internal",
        }
    }

    /// 获取错误消息
    fn message(&self) -> &str {
        match self {
            Self::NotFound(msg) => msg,
            Self::Forbidden(msg) => msg,
            Self::BadRequest(msg) => msg,
            Self::Unauthorized(msg) => msg,
            Self::TooManyRequests(msg) => msg,
            Self::Internal(msg) => msg,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ErrorResponse {
            error: self.error_kind().to_string(),
            message: self.message().to_string(),
        };
        (status, axum::Json(body)).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err.to_string())
    }
}

impl From<sled::Error> for AppError {
    fn from(err: sled::Error) -> Self {
        Self::Internal(format!("数据库错误: {err}"))
    }
}

impl From<bincode::Error> for AppError {
    fn from(err: bincode::Error) -> Self {
        Self::Internal(format!("序列化错误: {err}"))
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        Self::Internal(format!("IO 错误: {err}"))
    }
}
