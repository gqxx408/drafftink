//! # drafftink-backend
//!
//! 校本教学套件内网后端服务。
//!
//! 基于 Axum + sled + 本地文件系统，纯 Rust 实现，无 C 依赖。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tracing::info;
use uuid::Uuid;

use drafftink_core::{Class, Role, User};
use drafftink_backend::api;
use drafftink_backend::auth::password::hash_password;
use drafftink_backend::auth::ratelimit::LoginRateLimiter;
use drafftink_backend::auth::refresh::SledRefreshTokenStore;
use drafftink_backend::backup;
use drafftink_backend::config::BackendConfig;
use drafftink_backend::db::SledDb;
use drafftink_backend::recording::minio::MinioStorage;
use drafftink_backend::recording::LiveHub;
use drafftink_backend::state::AppState;
use drafftink_backend::storage::{LocalStorage, Storage};
use drafftink_backend::auth::mobile::MobileAuth;
use drafftink_backend::workflow::WorkflowStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drafftink_backend=info,tower_http=info".into()),
        )
        .init();

    // 加载配置
    let config = BackendConfig::from_env();
    info!("配置已加载: 监听 {}", config.listen_addr);

    // 启动安全闸门（P0-1）：JWT 密钥缺失或使用已知默认硬编码密钥时，
    // 直接拒绝启动，避免任何人可伪造 Admin 令牌。
    if let Err(e) = config.jwt.validate_not_default() {
        eprintln!("ERROR: JWT secret not configured. Set DRAFTTINK_JWT_SECRET environment variable.");
        eprintln!("Refusing to start with default/empty secret (security risk).");
        eprintln!("Details: {e}");
        std::process::exit(1);
    }

    // 确保目录存在
    if let Err(e) = std::fs::create_dir_all(&config.db_path) {
        return Err(anyhow::anyhow!("创建数据库目录失败: {e}"));
    }
    if let Err(e) = std::fs::create_dir_all(&config.storage_path) {
        return Err(anyhow::anyhow!("创建存储目录失败: {e}"));
    }
    if let Err(e) = std::fs::create_dir_all(&config.backup_path) {
        return Err(anyhow::anyhow!("创建备份目录失败: {e}"));
    }

    // 打开数据库
    let sled_db = SledDb::open(&config.db_path)?;
    info!("数据库已打开: {}", config.db_path.display());

    // 创建存储：配置 MinIO 时对接现有 MinIO（S3 兼容），否则使用本地存储
    let storage: Arc<dyn Storage> = if let Some(minio_cfg) = &config.minio {
        match MinioStorage::new(minio_cfg) {
            Ok(m) => {
                info!("存储已切换为 MinIO: {}", minio_cfg.endpoint);
                Arc::new(m)
            }
            Err(e) => {
                return Err(anyhow::anyhow!("MinIO 初始化失败: {e}"));
            }
        }
    } else {
        let local = LocalStorage::new(&config.storage_path)?;
        info!("存储已初始化（本地）: {}", config.storage_path.display());
        Arc::new(local)
    };

    // 创建应用状态
    let state = AppState {
        db: Arc::new(sled_db),
        storage,
        config: config.clone(),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        login_ratelimit: Arc::new(LoginRateLimiter::new(5, std::time::Duration::from_secs(60))),
        refresh_store: {
            let store = Arc::new(
                SledRefreshTokenStore::open(&config.db_path.join("refresh_tokens"))
                    .map_err(|e| anyhow::anyhow!("打开刷新令牌存储失败: {e}"))?,
            );
            // 启动后台过期清理：定期删除已过期的刷新令牌 / 吊销记录，避免 sled 数据库无限增长。
            let _refresh_sweeper =
                store.start_expiry_sweeper(std::time::Duration::from_secs(3600));
            info!("刷新令牌过期清理任务已启动（每小时执行一次）");
            store
        },
        live: LiveHub::new(),
        workflow: WorkflowStore::new(),
        mobile_auth: MobileAuth::new(),
    };

    // 播种默认数据
    seed_default_data(&state)?;

    // 启动备份任务
    backup::start_backup_task(
        config.db_path.clone(),
        config.storage_path.clone(),
        config.backup_path.clone(),
        config.backup_hour,
    );
    info!("备份任务已启动，每日 {:02}:00 执行", config.backup_hour);

    // 构建路由
    let app = api::router(state);
    info!("路由已构建");

    // 启动 HTTP 服务器
    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .map_err(|e| anyhow::anyhow!("绑定地址 {} 失败: {e}", config.listen_addr))?;
    info!("服务器启动: http://{}", config.listen_addr);

    // 优雅关闭
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| anyhow::anyhow!("服务器错误: {e}"))?;

    info!("服务器已关闭");
    Ok(())
}

/// 监听 Ctrl+C 信号，实现优雅关闭
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Ctrl+C 信号监听失败: {e}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                tracing::error!("SIGTERM 信号监听失败: {e}");
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("收到关闭信号，正在停止...");
}

/// 播种默认数据（管理员、老师、学生账号）
///
/// 仅在数据库为空时创建默认账号。
///
/// **安全闸门（P0-3）**：生产环境（`dev_mode == false`）默认**不播种**，
/// 避免弱口令账号（admin/admin123 等）随部署暴露。仅当 `dev_mode == true` 时播种，
/// 且默认口令必须由环境变量覆盖或随机生成（不再使用硬编码弱口令）。
fn seed_default_data(state: &AppState) -> anyhow::Result<()> {
    // 生产环境（dev_mode=false）跳过播种，防止弱口令账号被部署。
    if !state.config.dev_mode {
        info!("dev_mode=false, skipping default data seed");
        return Ok(());
    }

    // 检查是否已有 admin 用户
    if state.db.get_user_by_username("admin")?.is_some() {
        info!("默认数据已存在，跳过播种");
        return Ok(());
    }

    let school_id = Uuid::new_v4();
    let now = Utc::now();

    // 口令解析：优先环境变量覆盖，未设置则随机生成 24 字符密码并输出运维日志。
    let admin_pw = resolve_seed_password("DRAFTTINK_ADMIN_PASSWORD", "admin");
    let teacher_pw = resolve_seed_password("DRAFTTINK_TEACHER_PASSWORD", "teacher01");
    let student_pw = resolve_seed_password("DRAFTTINK_STUDENT_PASSWORD", "student01");

    // 创建管理员
    let admin = User {
        id: Uuid::new_v4(),
        username: "admin".to_string(),
        display_name: "系统管理员".to_string(),
        role: Role::Admin,
        class_id: None,
        tenant_id: school_id,
        password_hash: hash_password(&admin_pw),
        created_at: now,
        active: true,
    };
    state.db.save_user(&admin)?;
    info!("已创建管理员账号: admin（口令来自环境变量或随机生成）");

    // 创建老师
    let teacher_id = Uuid::new_v4();
    let teacher = User {
        id: teacher_id,
        username: "teacher01".to_string(),
        display_name: "王老师".to_string(),
        role: Role::Teacher,
        class_id: None,
        tenant_id: school_id,
        password_hash: hash_password(&teacher_pw),
        created_at: now,
        active: true,
    };
    state.db.save_user(&teacher)?;
    info!("已创建老师账号: teacher01（口令来自环境变量或随机生成）");

    // 创建班级
    let class_id = Uuid::new_v4();
    let class = Class {
        id: class_id,
        name: "三年二班".to_string(),
        grade: "三年级".to_string(),
        teacher_id: Some(teacher_id),
        school_id,
        created_at: now,
    };
    state.db.save_class(&class)?;
    info!("已创建班级: 三年二班");

    // 创建学生
    let student = User {
        id: Uuid::new_v4(),
        username: "student01".to_string(),
        display_name: "李同学".to_string(),
        role: Role::Student,
        class_id: Some(class_id),
        tenant_id: school_id,
        password_hash: hash_password(&student_pw),
        created_at: now,
        active: true,
    };
    state.db.save_user(&student)?;
    info!("已创建学生账号: student01（口令来自环境变量或随机生成）");

    // 默认通知公告（ZXBG0201），便于移动端公告页直接可见
    state.workflow.add_announcement(drafftink_backend::workflow::types::Announcement {
        notice_id: "NT20260001".into(),
        title: "欢迎使用校园移动办公平台".into(),
        publish_date: chrono::Utc::now().format("%Y%m%d").to_string(),
        publisher: "校办".into(),
        recv_scope: "全体教职工".into(),
        body: "移动办公平台已上线，支持待办审批、公文流转、通知公告、会议预约与用印申请。".into(),
        tenant_id: school_id,
        pinned: true,
    });
    info!("已发布默认通知公告");

    Ok(())
}

/// 解析种子账号口令：优先使用环境变量覆盖；未设置则随机生成 24 字符密码并输出到 stderr。
fn resolve_seed_password(env_key: &str, label: &str) -> String {
    match std::env::var(env_key) {
        Ok(p) if !p.is_empty() => p,
        _ => {
            let pw = random_password(24);
            eprintln!("[SEED] Generated {label} password: {pw}");
            eprintln!("[SEED] Please change it immediately after first login.");
            pw
        }
    }
}

/// 生成长度为 `len` 的随机密码（十六进制，基于 uuid 随机源，避免引入额外依赖）。
fn random_password(len: usize) -> String {
    let mut s = Uuid::new_v4().as_simple().to_string();
    while s.len() < len {
        s.push_str(&Uuid::new_v4().as_simple().to_string());
    }
    s.truncate(len);
    s
}

#[cfg(test)]
mod tests {
    // `main.rs` is a binary; the domain modules live in the `drafftink_backend`
    // lib crate, so reference them by lib path rather than `crate::`.
    // `super::*` already brings in `SledDb`, `LocalStorage`, `Storage`,
    // `AppState`, `BackendConfig`, `LiveHub`, `WorkflowStore`, `MobileAuth`,
    // `LoginRateLimiter` (imported at the top of main.rs).
    use super::*;
    use drafftink_backend::auth::refresh::MemoryRefreshTokenStore;
    use drafftink_backend::db::Database;
    use std::sync::Arc;

    fn test_state(dev_mode: bool) -> AppState {
        let db: Arc<dyn Database> = Arc::new(
            SledDb::open(&std::env::temp_dir().join(format!(
                "drafftink_seed_test_{}",
                Uuid::new_v4()
            )))
            .unwrap(),
        );
        let storage: Arc<dyn Storage> = Arc::new(
            LocalStorage::new(&std::env::temp_dir().join(format!(
                "drafftink_seed_store_{}",
                Uuid::new_v4()
            )))
            .unwrap(),
        );
        let mut config = BackendConfig::default();
        config.dev_mode = dev_mode;
        AppState {
            db,
            storage,
            config,
            sessions: Arc::new(std::sync::Mutex::new(Default::default())),
            login_ratelimit: Arc::new(LoginRateLimiter::new(5, std::time::Duration::from_secs(60))),
            refresh_store: Arc::new(MemoryRefreshTokenStore::new()),
            live: LiveHub::new(),
            workflow: WorkflowStore::new(),
            mobile_auth: MobileAuth::new(),
        }
    }

    #[test]
    fn seed_skips_when_dev_mode_false() {
        let state = test_state(false);
        seed_default_data(&state).unwrap();
        assert!(
            state.db.get_user_by_username("admin").unwrap().is_none(),
            "dev_mode=false 时不应写入任何种子数据"
        );
        assert!(state.db.get_user_by_username("teacher01").unwrap().is_none());
        assert!(state.db.get_user_by_username("student01").unwrap().is_none());
    }

    #[test]
    fn seed_writes_when_dev_mode_true() {
        let state = test_state(true);
        seed_default_data(&state).unwrap();
        assert!(state.db.get_user_by_username("admin").unwrap().is_some());
        assert!(state.db.get_user_by_username("teacher01").unwrap().is_some());
        assert!(state.db.get_user_by_username("student01").unwrap().is_some());
    }
}
