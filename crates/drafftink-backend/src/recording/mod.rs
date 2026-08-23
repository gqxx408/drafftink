//! # recording — 课堂录播与资源发布（DB34/T 2318-2015）
//!
//! 基于现有 RBAC / 公网网关 / MinIO 存储架构，实现符合安徽省地方标准
//! DB34/T 2318-2015 的课堂录播子系统：
//!
//! - [`live`]：网络直播（B/S 架构 WebSocket，延时 ≤ 3 秒）+ 自动/手动导播。
//! - [`resource`]：资源管理平台（分类检索、权限控制、MinIO 上传）。
//! - [`minio`]：MinIO（S3 兼容）存储后端，复用现有 `Storage` 抽象。

pub mod live;
pub mod minio;
pub mod resource;

pub use live::{ClientControl, LiveFrame, LiveHub};
pub use minio::MinioStorage;
pub use resource::ResourceManager;
