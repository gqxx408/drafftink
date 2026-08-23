//! # 应用状态
//!
//! 使用 `Arc<dyn Trait>` 封装数据库和存储，便于测试和替换实现。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use drafftink_core::integration::SharedAppContext;

use crate::auth::mobile::MobileAuth;
use crate::auth::ratelimit::LoginRateLimiter;
use crate::auth::refresh::RefreshTokenStore;
use crate::config::BackendConfig;
use crate::db::Database;
use crate::recording::LiveHub;
use crate::storage::Storage;
use crate::workflow::WorkflowStore;

/// 应用共享状态
#[derive(Clone)]
pub struct AppState {
    /// 数据库实例
    pub db: Arc<dyn Database>,
    /// 存储实例
    pub storage: Arc<dyn Storage>,
    /// 配置
    pub config: BackendConfig,
    /// 已登录用户的共享上下文（登录后初始化）
    pub sessions: Arc<Mutex<HashMap<uuid::Uuid, SharedAppContext>>>,
    /// 登录接口速率限制器（防暴力破解）
    pub login_ratelimit: Arc<LoginRateLimiter>,
    /// 刷新令牌存储（支持主动吊销）
    pub refresh_store: Arc<dyn RefreshTokenStore>,
    /// 直播中枢（直播间广播 + 自动/手动导播）
    pub live: LiveHub,
    /// 移动办公：审批工作流与办公数据存储
    pub workflow: WorkflowStore,
    /// 移动办公：MFA / SSO / SM4 信封加密状态
    pub mobile_auth: MobileAuth,
}
