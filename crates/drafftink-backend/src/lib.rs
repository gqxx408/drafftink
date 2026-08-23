//! # drafftink-backend 库
//!
//! 校本教学套件内网后端服务的核心逻辑，供二进制入口（`main.rs`）与集成测试共享。
//!
//! 该库导出全部模块，使 `tests/` 下的集成测试能够直接构建路由、应用状态并验证
//! 鉴权 / RBAC / 多租户隔离等核心安全能力。

pub mod api;
pub mod auth;
pub mod backup;
pub mod config;
pub mod db;
pub mod error;
pub mod recording;
pub mod state;
pub mod storage;
pub mod workflow;
