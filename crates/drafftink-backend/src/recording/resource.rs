//! # resource — 资源管理平台（DB34/T 2318-2015）
//!
//! 录制的课件自动发布到存储（MinIO / 本地），按 JY/T 1004 字段（院校/年级/班级/
//! 学科/课程）分类建索引，支持按教师姓名、课件名称、章节索引关键字检索，并复用
//! 现有 RBAC 进行登录 / 点播 / 评语查看权限控制。

use std::sync::Arc;

use drafftink_core::recording::{
    CoursewareClassification, CoursewareResource, RecordingMode, RecordingParams,
    ResourcePermission,
};

use crate::db::Database;
use crate::storage::Storage;

/// 资源管理器：负责课件资源的索引与检索，视频字节通过 [`Storage`] 持久化。
pub struct ResourceManager {
    db: Arc<dyn Database>,
    storage: Arc<dyn Storage>,
}

impl ResourceManager {
    /// 构造资源管理器。
    pub fn new(db: Arc<dyn Database>, storage: Arc<dyn Storage>) -> Self {
        Self { db, storage }
    }

    /// 发布课件资源：将元数据写入索引，若携带视频字节则持久化到存储后端。
    pub fn publish(&self, meta: &CoursewareResource, video: Option<Vec<u8>>) -> anyhow::Result<()> {
        if let Some(bytes) = video {
            self.storage.save(&meta.storage_key, bytes)?;
        }
        self.db.save_resource_meta(&meta.resource_id, &serde_json::to_string(meta)?)?;
        Ok(())
    }

    /// 按关键字检索课件资源（跨 JY/T 1004 分类字段，不区分大小写）。
    pub fn search(&self, keyword: &str) -> Vec<CoursewareResource> {
        match self.db.scan_resource_meta() {
            Ok(list) => list
                .iter()
                .filter_map(|(_, json)| serde_json::from_str::<CoursewareResource>(json).ok())
                .filter(|r| r.matches_keyword(keyword))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 按资源 ID 获取课件元数据。
    pub fn get(&self, resource_id: &str) -> anyhow::Result<CoursewareResource> {
        let json = self
            .db
            .get_resource_meta(resource_id)?
            .ok_or_else(|| anyhow::anyhow!("资源不存在: {resource_id}"))?;
        serde_json::from_str(&json).map_err(Into::into)
    }

    /// 读取视频字节（点播用）。
    pub fn load_video(&self, storage_key: &str) -> anyhow::Result<Vec<u8>> {
        self.storage.load(storage_key)
    }
}

/// 发布课件的请求体。
#[derive(Debug, serde::Deserialize)]
pub struct PublishResourceRequest {
    /// 资源 ID（可选，缺省自动生成）
    #[serde(default)]
    pub resource_id: Option<String>,
    /// 课件标题
    pub title: String,
    /// JY/T 1004 分类
    pub classification: CoursewareClassification,
    /// 录制参数（可选，缺省标准参数）
    #[serde(default)]
    pub params: Option<RecordingParams>,
    /// 录制模式
    pub mode: RecordingMode,
    /// RBAC 权限策略（可选，缺省公开）
    #[serde(default)]
    pub permission: Option<ResourcePermission>,
    /// 关联板书 drftx 存储 key（可选）
    #[serde(default)]
    pub drftx_key: Option<String>,
    /// 视频字节（可选，随发布一并上传）
    #[serde(default)]
    pub data: Option<Vec<u8>>,
}

/// 检索请求（Query 参数）。
#[derive(Debug, serde::Deserialize)]
pub struct SearchQuery {
    /// 关键字：教师姓名 / 课件名称 / 章节索引等
    pub q: String,
}
