//! # 后端配置
//!
//! 从环境变量加载配置，所有字段都有默认值。

use std::path::PathBuf;

use drafftink_core::crypto::JwtConfig;

use crate::recording::minio::MinioConfig;

/// 后端配置结构体
#[derive(Debug, Clone)]
pub struct BackendConfig {
    /// 监听地址
    pub listen_addr: String,
    /// sled 数据库路径
    pub db_path: PathBuf,
    /// 资源存储路径
    pub storage_path: PathBuf,
    /// 备份路径
    pub backup_path: PathBuf,
    /// JWT 配置（密钥必须由环境变量 `DRAFTTINK_JWT_SECRET` 提供，缺失即拒绝启动）
    pub jwt: JwtConfig,
    /// 开发模式：开启后暴露演示用接口（如短信验证码回显），仅用于本地演示，生产务必关闭
    pub dev_mode: bool,
    /// 每日备份时间（小时，0-23）
    pub backup_hour: u32,
    /// MinIO（S3 兼容）存储配置；为 `None` 时使用本地存储（默认）
    pub minio: Option<MinioConfig>,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:8080".to_string(),
            db_path: PathBuf::from("data/db"),
            storage_path: PathBuf::from("data/storage"),
            backup_path: PathBuf::from("data/backup"),
            jwt: JwtConfig::default(),
            dev_mode: false,
            backup_hour: 2,
            minio: None,
        }
    }
}

impl BackendConfig {
    /// 从环境变量加载配置，缺失时使用默认值。
    pub fn from_env() -> Self {
        let defaults = Self::default();

        let listen_addr =
            std::env::var("DRAFTTINK_LISTEN_ADDR").unwrap_or_else(|_| defaults.listen_addr.clone());

        let db_path = std::env::var("DRAFTTINK_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| defaults.db_path.clone());

        let storage_path = std::env::var("DRAFTTINK_STORAGE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| defaults.storage_path.clone());

        let backup_path = std::env::var("DRAFTTINK_BACKUP_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| defaults.backup_path.clone());

        let jwt = match std::env::var("DRAFTTINK_JWT_SECRET") {
            Ok(s) if !s.is_empty() => JwtConfig {
                secret: s.into_bytes(),
                ..Default::default()
            },
            // 缺失或为空时不提供任何默认密钥；启动闸门会捕获并拒绝启动。
            _ => JwtConfig::default(),
        };

        let dev_mode = std::env::var("DRAFTTINK_DEV_MODE")
            .ok()
            .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
            .unwrap_or(defaults.dev_mode);

        let backup_hour = std::env::var("DRAFTTINK_BACKUP_HOUR")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(defaults.backup_hour);

        Self {
            listen_addr,
            db_path,
            storage_path,
            backup_path,
            jwt,
            dev_mode,
            backup_hour,
            minio: MinioConfig::from_env(),
        }
    }
}
