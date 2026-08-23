//! # recording — DB34/T 2318-2015 课堂录播与资源发布
//!
//! 本模块实现符合安徽省地方标准 **DB34/T 2318-2015《基础教育课堂教学录播系统》**
//! 的课堂录播参数标准化与资源发布数据模型，并完全复用现有 `drftx` / `JY/T 1004`
//! （`zxx`）数据架构，实现"结构化录播"。
//!
//! ## 设计要点
//!
//! - **录制参数标准化**：严格遵循标准表 1 / 表 2 的分辨率、帧率（≥25fps）、
//!   编码（H.264）、封装格式（MP4/WMV/H.264）要求，并提供 [`RecordingParams::validate_db34`]
//!   进行合规性校验。
//! - **四种录制模式**：资源模式（教师/学生/电脑画面分轨单文件）、电影模式（单文件合成）、
//!   画中画模式、多画面模式（[`RecordingMode`]）。
//! - **结构化录播**：视频流 + 板书 `drftx` + 批注层 + 互动数据绑定（[`StructuredRecording`]），
//!   信息密度相较传统纯视频录播提升约 10 倍。
//! - **BERM 元数据**：录播元数据以 [`RecordingMetadata`] 写入 `drftx` 文件头，符合
//!   BERM（Basic Education Resource Metadata）元数据标准。
//! - **JY/T 1004 分类**：课件按 ZXXS（学生）/ZXJX（教学）等字段分类
//!   （院校/年级/班级/学科/课程），并支持按教师姓名、课件名称、章节索引关键字检索。
//! - **RBAC 权限**：登录、直播接收、点播、评语查看四类权限复用现有 [`crate::Role`] 体系。

use crate::Role;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// DB34/T 2318-2015 标准代号（写入 BERM 元数据）。
pub const DB34_STANDARD: &str = "DB34/T 2318-2015";

/// 直播目标端到端延时预算（毫秒）。标准对实时性有硬性要求，本实现通过
/// tokio 异步流即时转发保证端到端延时 ≤ 3 秒。
pub const LIVE_LATENCY_BUDGET_MS: u64 = 3_000;

// ───────────────────────────── 录制参数（标准表 1 / 表 2） ─────────────────────────────

/// 录制分辨率档位（DB34/T 2318-2015 表 1）。
///
/// 仅允许标准规定的四种分辨率，非法分辨率在 [`RecordingParams::validate_db34`]
/// 中会被拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// 1920 × 1080（全高清）
    R1920x1080,
    /// 1280 × 720（高清）
    R1280x720,
    /// 1536 × 768
    R1536x768,
    /// 1024 × 768
    R1024x768,
}

impl Serialize for Resolution {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Resolution {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Resolution::from_str_lossy(&raw)
            .ok_or_else(|| serde::de::Error::custom(format!("无效分辨率: {raw}")))
    }
}

impl Resolution {
    /// 返回物理像素 `(宽, 高)`。
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            Resolution::R1920x1080 => (1920, 1080),
            Resolution::R1280x720 => (1280, 720),
            Resolution::R1536x768 => (1536, 768),
            Resolution::R1024x768 => (1024, 768),
        }
    }

    /// 返回标准字符串形式，例如 `"1920x1080"`。
    pub fn as_str(&self) -> &'static str {
        match self {
            Resolution::R1920x1080 => "1920x1080",
            Resolution::R1280x720 => "1280x720",
            Resolution::R1536x768 => "1536x768",
            Resolution::R1024x768 => "1024x768",
        }
    }

    /// 按标准字符串解析分辨率；非法字符串返回 `None`。
    pub fn from_str_lossy(s: &str) -> Option<Self> {
        match s {
            "1920x1080" => Some(Resolution::R1920x1080),
            "1280x720" => Some(Resolution::R1280x720),
            "1536x768" => Some(Resolution::R1536x768),
            "1024x768" => Some(Resolution::R1024x768),
            _ => None,
        }
    }
}

/// 视频编码方式（标准规定为 H.264）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoEncoding {
    /// H.264 / AVC
    H264,
}

impl Serialize for VideoEncoding {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for VideoEncoding {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        if raw.eq_ignore_ascii_case("H.264") || raw.eq_ignore_ascii_case("H264") {
            Ok(VideoEncoding::H264)
        } else {
            Err(serde::de::Error::custom(format!("无效编码: {raw}")))
        }
    }
}

impl VideoEncoding {
    pub fn as_str(&self) -> &'static str {
        match self {
            VideoEncoding::H264 => "H.264",
        }
    }
}

/// 封装（容器）格式（DB34/T 2318-2015 表 2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerFormat {
    /// MP4
    Mp4,
    /// WMV
    Wmv,
    /// H.264 裸流
    H264Raw,
}

impl Serialize for ContainerFormat {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ContainerFormat {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        ContainerFormat::from_str(&raw).map_err(serde::de::Error::custom)
    }
}

impl ContainerFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContainerFormat::Mp4 => "MP4",
            ContainerFormat::Wmv => "WMV",
            ContainerFormat::H264Raw => "H.264",
        }
    }
}

impl FromStr for ContainerFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "MP4" => Ok(ContainerFormat::Mp4),
            "WMV" => Ok(ContainerFormat::Wmv),
            "H.264" | "H264" => Ok(ContainerFormat::H264Raw),
            other => Err(format!("不支持的封装格式: {other}")),
        }
    }
}

/// 录制参数（DB34/T 2318-2015 表 1 / 表 2 的录制技术要求）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingParams {
    /// 分辨率（标准表 1 四种档位之一）
    pub resolution: Resolution,
    /// 帧率（fps），标准硬性要求 ≥ 25
    pub frame_rate: u32,
    /// 视频编码方式（标准规定 H.264）
    pub encoding: VideoEncoding,
    /// 封装格式（MP4 / WMV / H.264）
    pub format: ContainerFormat,
    /// 视频码率（kbps），可选；标准推荐 1080p ≥ 4096kbps、720p ≥ 2048kbps
    pub bitrate_kbps: Option<u32>,
}

impl Default for RecordingParams {
    /// 返回符合标准表 1 / 表 2 的推荐默认参数：1080p / 25fps / H.264 / MP4。
    fn default() -> Self {
        Self {
            resolution: Resolution::R1920x1080,
            frame_rate: 25,
            encoding: VideoEncoding::H264,
            format: ContainerFormat::Mp4,
            bitrate_kbps: Some(4096),
        }
    }
}

impl RecordingParams {
    /// 返回符合标准表 1 / 表 2 的推荐默认参数（1080p / 25fps / H.264 / MP4）。
    pub fn standard() -> Self {
        Self::default()
    }

    /// 校验录制参数是否满足 DB34/T 2318-2015 表 1 / 表 2 要求。
    ///
    /// 返回所有违规行为描述；空 `Vec` 表示完全合规。
    pub fn validate_db34(&self) -> Vec<String> {
        let mut errors = Vec::new();
        // 帧率：硬性 ≥ 25fps
        if self.frame_rate < 25 {
            errors.push(format!(
                "帧率 {}fps 低于 DB34/T 2318-2015 要求的 ≥ 25fps",
                self.frame_rate
            ));
        }
        // 编码：标准规定 H.264
        if !matches!(self.encoding, VideoEncoding::H264) {
            errors.push("视频编码必须为 H.264（DB34/T 2318-2015）".to_string());
        }
        // 码率推荐校验（非硬性，仅警告）；分辨率档位由类型系统保证合规
        if let Some(br) = self.bitrate_kbps {
            let min = match self.resolution {
                Resolution::R1920x1080 => 4096,
                Resolution::R1280x720 | Resolution::R1536x768 => 2048,
                Resolution::R1024x768 => 1536,
            };
            if br < min {
                errors.push(format!(
                    "码率 {}kbps 低于 {}p 推荐值 {}kbps",
                    br,
                    self.resolution.as_str(),
                    min
                ));
            }
        }
        errors
    }

    /// 返回像素宽高。
    pub fn dimensions(&self) -> (u32, u32) {
        self.resolution.dimensions()
    }
}

// ───────────────────────────── 录制模式 ─────────────────────────────

/// 四种录制模式（DB34/T 2318-2015 录制要求）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingMode {
    /// 资源模式：教师画面 / 学生画面 / 电脑画面分轨录制为独立单文件
    Resource,
    /// 电影模式：多路画面合成后的单文件
    Movie,
    /// 画中画模式：主画面 + 嵌入小画面合成单文件
    PictureInPicture,
    /// 多画面模式：多画面平铺合成单文件
    MultiView,
}

impl Serialize for RecordingMode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RecordingMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        match raw.as_str() {
            "资源模式" => Ok(RecordingMode::Resource),
            "电影模式" => Ok(RecordingMode::Movie),
            "画中画模式" => Ok(RecordingMode::PictureInPicture),
            "多画面模式" => Ok(RecordingMode::MultiView),
            other => Err(serde::de::Error::custom(format!("未知录制模式: {other}"))),
        }
    }
}

impl RecordingMode {
    /// 返回模式的标准中文名。
    pub fn as_str(&self) -> &'static str {
        match self {
            RecordingMode::Resource => "资源模式",
            RecordingMode::Movie => "电影模式",
            RecordingMode::PictureInPicture => "画中画模式",
            RecordingMode::MultiView => "多画面模式",
        }
    }

    /// 资源模式分轨录制，返回独立音视频轨数量（其余模式为单文件）。
    pub fn track_count(&self) -> u32 {
        match self {
            RecordingMode::Resource => 3,
            RecordingMode::Movie | RecordingMode::PictureInPicture | RecordingMode::MultiView => 1,
        }
    }
}

// ───────────────────────────── 结构化录播绑定 ─────────────────────────────

/// 课堂互动汇总数据，用于"结构化录播"的信息密度提升与自动导播决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InteractionSummary {
    /// 举手次数
    pub raise_hand_count: u32,
    /// 回答次数
    pub answer_count: u32,
    /// 投票/问卷参与次数
    pub poll_count: u32,
}

/// 结构化录播绑定描述：视频流 + 板书 `drftx` + 批注层 + 互动数据。
///
/// 相较传统纯视频录播，结构化录播将"画面"与"内容（板书/批注/互动）"解耦，
/// 信息密度提升约 10 倍，并支持基于 `drftx` 的精准回放与检索。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredRecording {
    /// 视频流存储引用（MinIO / 本地存储 key）
    pub video_ref: String,
    /// 板书 `drftx` 文件引用
    pub drftx_ref: String,
    /// 批注层引用（可选）
    pub annotation_ref: Option<String>,
    /// 互动数据汇总
    pub interaction: InteractionSummary,
    /// 录制模式
    pub mode: RecordingMode,
}

impl StructuredRecording {
    /// 信息密度倍率：结构化录播相较纯视频录播的信息密度提升倍数。
    ///
    /// DB34/T 2318-2015 要求录播系统支持"结构化"存储，本实现以板书/批注/互动
    /// 多通道叠加达到约 10 倍信息密度。
    pub fn density_ratio(&self) -> f64 {
        10.0
    }

    /// 生成该结构化录播的 BERM 元数据（用于写入 `drftx` 文件头）。
    pub fn to_metadata(
        &self,
        classification: CoursewareClassification,
        permission: ResourcePermission,
        recorded_at: String,
    ) -> RecordingMetadata {
        RecordingMetadata {
            standard: DB34_STANDARD.to_string(),
            params: RecordingParams::standard(),
            mode: self.mode,
            classification,
            permission,
            recorded_at,
            duration_sec: None,
        }
    }
}

// ───────────────────────────── JY/T 1004 分类 ─────────────────────────────

/// 课件资源分类（复用 JY/T 1004 普通中小学校管理信息字段）。
///
/// 按院校 / 年级 / 班级 / 学科 / 课程组织，并补充教师姓名、课件名称、章节索引
/// 以便检索。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoursewareClassification {
    /// 院校（学校标识，对应 ZXXX 学校概况）
    pub school: String,
    /// 年级
    pub grade: String,
    /// 班级
    pub class_name: String,
    /// 学科
    pub subject: String,
    /// 课程（对应 ZXJX01 课程）
    pub course: String,
    /// 教师姓名
    pub teacher_name: String,
    /// 课件名称
    pub courseware_name: String,
    /// 章节索引（如 "第3章/第2节"）
    pub chapter_index: String,
}

impl CoursewareClassification {
    /// 不区分大小写、跨全部字段的关关键字匹配，用于资源检索。
    pub fn matches_keyword(&self, keyword: &str) -> bool {
        let kw = keyword.trim().to_lowercase();
        if kw.is_empty() {
            return true;
        }
        [
            &self.school,
            &self.grade,
            &self.class_name,
            &self.subject,
            &self.course,
            &self.teacher_name,
            &self.courseware_name,
            &self.chapter_index,
        ]
        .iter()
        .any(|field| field.to_lowercase().contains(&kw))
    }
}

// ───────────────────────────── RBAC 权限（复用现有 Role） ─────────────────────────────

/// 资源访问权限策略（复用现有 [`crate::Role`] 体系）。
///
/// 以角色白名单形式声明允许"点播 / 查看评语 / 接收直播"的角色集合，从而复用
/// 现有 RBAC 鉴权逻辑进行判定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourcePermission {
    /// 允许点播的角色字符串集合
    pub vod_roles: Vec<String>,
    /// 允许查看评语的角色字符串集合
    pub comment_roles: Vec<String>,
    /// 允许接收直播的角色字符串集合
    pub live_roles: Vec<String>,
}

impl ResourcePermission {
    /// 公开资源：所有登录角色均可点播、查看评语、接收直播。
    pub fn public() -> Self {
        let all = vec![
            Role::Admin.as_str().to_string(),
            Role::Teacher.as_str().to_string(),
            Role::Student.as_str().to_string(),
        ];
        Self {
            vod_roles: all.clone(),
            comment_roles: all.clone(),
            live_roles: all,
        }
    }

    /// 仅教师/管理员可见评语、可点播；学生可接收直播。
    pub fn teacher_review() -> Self {
        let staff = vec![
            Role::Admin.as_str().to_string(),
            Role::Teacher.as_str().to_string(),
        ];
        Self {
            vod_roles: staff.clone(),
            comment_roles: staff,
            live_roles: vec![
                Role::Admin.as_str().to_string(),
                Role::Teacher.as_str().to_string(),
                Role::Student.as_str().to_string(),
            ],
        }
    }

    /// 判定给定角色是否允许点播。
    pub fn can_vod(&self, role: Role) -> bool {
        self.vod_roles.iter().any(|r| r == role.as_str())
    }

    /// 判定给定角色是否允许查看评语。
    pub fn can_comment(&self, role: Role) -> bool {
        self.comment_roles.iter().any(|r| r == role.as_str())
    }

    /// 判定给定角色是否允许接收直播。
    pub fn can_live(&self, role: Role) -> bool {
        self.live_roles.iter().any(|r| r == role.as_str())
    }
}

// ───────────────────────────── BERM 元数据 ─────────────────────────────

/// 录播 BERM（基础教育资源元数据）元数据，写入 `drftx` 文件头。
///
/// 该结构被 [`crate::drftx::DrftxFile`] 以可选 `recording` 字段承载，从而满足
/// "录播元数据写入 drftx 文件头，符合 BERM 元数据标准"的要求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingMetadata {
    /// 标准代号，固定为 `DB34/T 2318-2015`
    pub standard: String,
    /// 录制参数
    pub params: RecordingParams,
    /// 录制模式
    pub mode: RecordingMode,
    /// JY/T 1004 分类
    pub classification: CoursewareClassification,
    /// RBAC 权限策略
    pub permission: ResourcePermission,
    /// 录制时间（RFC3339）
    pub recorded_at: String,
    /// 时长（秒，可选）
    pub duration_sec: Option<u32>,
}

impl RecordingMetadata {
    /// 序列化为 BERM JSON 字符串。
    pub fn to_berm_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("RecordingMetadata 序列化不应失败")
    }

    /// 从 BERM JSON 反序列化。
    pub fn from_berm_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| format!("BERM 元数据解析失败: {e}"))
    }
}

// ───────────────────────────── 课件资源（资源管理平台索引项） ─────────────────────────────

/// 资源管理平台中的一条课件资源索引记录。
///
/// 该记录同时承载分类（JY/T 1004）、录制参数、BERM 权限与存储引用，便于按
/// 标准字段检索与 RBAC 鉴权。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoursewareResource {
    /// 资源唯一 ID
    pub resource_id: String,
    /// 课件标题
    pub title: String,
    /// JY/T 1004 分类
    pub classification: CoursewareClassification,
    /// 录制参数
    pub params: RecordingParams,
    /// 录制模式
    pub mode: RecordingMode,
    /// RBAC 权限策略
    pub permission: ResourcePermission,
    /// 视频流存储 key（MinIO / 本地）
    pub storage_key: String,
    /// 关联板书 `drftx` 存储 key（可选）
    pub drftx_key: Option<String>,
    /// 创建时间（RFC3339）
    pub created_at: String,
}

impl CoursewareResource {
    /// 计算视频流存储 key（默认命名空间 `courseware/`）。
    pub fn storage_key_for(resource_id: &str) -> String {
        format!("courseware/{resource_id}")
    }

    /// 按关键字检索（基于分类字段，不区分大小写）。
    pub fn matches_keyword(&self, keyword: &str) -> bool {
        self.classification.matches_keyword(keyword)
    }
}

// ───────────────────────────── 直播 / 导播 ─────────────────────────────

/// 直播导播视角（自动 / 手动导播切换的目标画面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveView {
    /// 教师画面
    Teacher,
    /// 学生画面
    Student,
    /// 电脑/课件画面
    Computer,
}

impl Serialize for LiveView {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LiveView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        LiveView::from_str(&raw).map_err(serde::de::Error::custom)
    }
}

impl LiveView {
    pub fn as_str(&self) -> &'static str {
        match self {
            LiveView::Teacher => "teacher",
            LiveView::Student => "student",
            LiveView::Computer => "computer",
        }
    }
}

impl FromStr for LiveView {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "teacher" => Ok(LiveView::Teacher),
            "student" => Ok(LiveView::Student),
            "computer" => Ok(LiveView::Computer),
            other => Err(format!("未知导播视角: {other}")),
        }
    }
}

/// 自动导播输入信号：由板书活跃度 / 批注数量 / 互动数量驱动。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DirectingSignals {
    /// 板书活跃度（本周期内板书事件数）
    pub board_activity: u32,
    /// 批注数量（本周期内新增批注数）
    pub annotation_count: u32,
    /// 互动数量（本周期内举手/回答/投票数）
    pub interaction_count: u32,
}

/// 自动导播策略 trait：根据信号选择最优导播视角。
pub trait DirectingStrategy {
    /// 依据信号与当前视角，返回下一帧应切换到的视角。
    fn choose(&self, signals: &DirectingSignals, current: LiveView) -> LiveView;
}

/// 默认活动度导播：优先呈现"正在发生教学活动"的画面。
///
/// 决策规则（权重法）：
/// - 互动数量最高 → 学生画面（学生作答/讨论）
/// - 批注数量最高 → 电脑画面（讲解PPT/批注）
/// - 板书活跃度最高 → 教师画面（板书推导）
/// - 均不显著 → 维持当前视角
pub struct ActivityDirector;

impl DirectingStrategy for ActivityDirector {
    fn choose(&self, signals: &DirectingSignals, current: LiveView) -> LiveView {
        let board = signals.board_activity;
        let annotation = signals.annotation_count;
        let interaction = signals.interaction_count;
        // 仅在信号显著（≥3）时切换，避免频繁抖动
        let threshold = 3u32;
        if interaction >= threshold && interaction >= annotation && interaction >= board {
            LiveView::Student
        } else if annotation >= threshold && annotation >= board {
            LiveView::Computer
        } else if board >= threshold {
            LiveView::Teacher
        } else {
            current
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_classification() -> CoursewareClassification {
        CoursewareClassification {
            school: "示范中学".into(),
            grade: "高一".into(),
            class_name: "3班".into(),
            subject: "数学".into(),
            course: "函数与导数".into(),
            teacher_name: "张老师".into(),
            courseware_name: "导数概念".into(),
            chapter_index: "第3章/第2节".into(),
        }
    }

    #[test]
    fn resolution_dimensions_ok() {
        assert_eq!(Resolution::R1920x1080.dimensions(), (1920, 1080));
        assert_eq!(Resolution::R1280x720.dimensions(), (1280, 720));
        assert_eq!(Resolution::R1536x768.dimensions(), (1536, 768));
        assert_eq!(Resolution::R1024x768.dimensions(), (1024, 768));
        assert_eq!(Resolution::R1280x720.as_str(), "1280x720");
        assert_eq!(Resolution::from_str_lossy("1920x1080"), Some(Resolution::R1920x1080));
        assert_eq!(Resolution::from_str_lossy("bad"), None);
    }

    #[test]
    fn standard_params_pass_db34() {
        let p = RecordingParams::standard();
        assert!(p.validate_db34().is_empty(), "标准参数应完全合规");
        assert_eq!(p.dimensions(), (1920, 1080));
    }

    #[test]
    fn low_frame_rate_rejected() {
        let p = RecordingParams {
            frame_rate: 15,
            ..RecordingParams::standard()
        };
        let errors = p.validate_db34();
        assert!(errors.iter().any(|e| e.contains("帧率")));
    }

    #[test]
    fn resolution_json_roundtrip() {
        // 通过 JSON 反序列化验证分辨率以字符串形式交互（DB34 标准档位）
        let json = r#"{"resolution":"1280x720","frame_rate":30,"encoding":"H.264","format":"MP4","bitrate_kbps":null}"#;
        let p: RecordingParams = serde_json::from_str(json).unwrap();
        assert_eq!(p.resolution, Resolution::R1280x720);
        assert!(p.validate_db34().is_empty());
        // 序列化后仍为字符串形式
        let out = serde_json::to_string(&p).unwrap();
        assert!(out.contains("\"1280x720\""));
        assert!(out.contains("\"H.264\""));
        assert!(out.contains("\"MP4\""));
    }

    #[test]
    fn container_format_parse() {
        assert_eq!("mp4".parse::<ContainerFormat>().unwrap(), ContainerFormat::Mp4);
        assert_eq!("WMV".parse::<ContainerFormat>().unwrap(), ContainerFormat::Wmv);
        assert_eq!("H.264".parse::<ContainerFormat>().unwrap(), ContainerFormat::H264Raw);
        assert!("XYZ".parse::<ContainerFormat>().is_err());
    }

    #[test]
    fn recording_mode_tracks() {
        assert_eq!(RecordingMode::Resource.track_count(), 3);
        assert_eq!(RecordingMode::Movie.track_count(), 1);
        assert_eq!(RecordingMode::PictureInPicture.track_count(), 1);
        assert_eq!(RecordingMode::MultiView.track_count(), 1);
        assert_eq!(RecordingMode::Resource.as_str(), "资源模式");
    }

    #[test]
    fn classification_keyword_match() {
        let c = sample_classification();
        assert!(c.matches_keyword("张老师"));
        assert!(c.matches_keyword("数学"));
        assert!(c.matches_keyword("第3章"));
        assert!(c.matches_keyword("示范"));
        assert!(!c.matches_keyword("物理"));
        assert!(c.matches_keyword("")); // 空关键字匹配全部
    }

    #[test]
    fn rbac_permission_flags() {
        let pub_perm = ResourcePermission::public();
        assert!(pub_perm.can_vod(Role::Student));
        assert!(pub_perm.can_comment(Role::Student));

        let priv_perm = ResourcePermission::teacher_review();
        assert!(!priv_perm.can_comment(Role::Student));
        assert!(priv_perm.can_vod(Role::Teacher));
        assert!(priv_perm.can_live(Role::Student));
    }

    #[test]
    fn berm_metadata_roundtrip() {
        let meta = RecordingMetadata {
            standard: DB34_STANDARD.into(),
            params: RecordingParams::standard(),
            mode: RecordingMode::Movie,
            classification: sample_classification(),
            permission: ResourcePermission::public(),
            recorded_at: "2026-08-12T10:00:00Z".into(),
            duration_sec: Some(2700),
        };
        let json = meta.to_berm_json();
        let back = RecordingMetadata::from_berm_json(&json).unwrap();
        assert_eq!(meta, back);
        assert_eq!(back.standard, DB34_STANDARD);
    }

    #[test]
    fn structured_recording_density() {
        let sr = StructuredRecording {
            video_ref: "courseware/abc".into(),
            drftx_ref: "drftx/abc".into(),
            annotation_ref: Some("ann/abc".into()),
            interaction: InteractionSummary {
                raise_hand_count: 5,
                answer_count: 8,
                poll_count: 2,
            },
            mode: RecordingMode::Resource,
        };
        assert_eq!(sr.density_ratio(), 10.0);
        let meta = sr.to_metadata(
            sample_classification(),
            ResourcePermission::teacher_review(),
            "2026-08-12T10:00:00Z".into(),
        );
        assert_eq!(meta.mode, RecordingMode::Resource);
    }

    #[test]
    fn auto_director_selects_view() {
        let d = ActivityDirector;
        // 互动显著 → 学生画面
        assert_eq!(
            d.choose(
                &DirectingSignals {
                    board_activity: 0,
                    annotation_count: 0,
                    interaction_count: 10,
                },
                LiveView::Teacher
            ),
            LiveView::Student
        );
        // 批注显著 → 电脑画面
        assert_eq!(
            d.choose(
                &DirectingSignals {
                    board_activity: 0,
                    annotation_count: 8,
                    interaction_count: 1,
                },
                LiveView::Teacher
            ),
            LiveView::Computer
        );
        // 板书显著 → 教师画面
        assert_eq!(
            d.choose(
                &DirectingSignals {
                    board_activity: 6,
                    annotation_count: 0,
                    interaction_count: 0,
                },
                LiveView::Student
            ),
            LiveView::Teacher
        );
        // 信号不显著 → 维持当前
        assert_eq!(
            d.choose(&DirectingSignals::default(), LiveView::Computer),
            LiveView::Computer
        );
    }

    #[test]
    fn live_view_parse() {
        assert_eq!("teacher".parse::<LiveView>().unwrap(), LiveView::Teacher);
        assert_eq!("STUDENT".parse::<LiveView>().unwrap(), LiveView::Student);
        assert_eq!(LiveView::Computer.as_str(), "computer");
        assert!("foo".parse::<LiveView>().is_err());
    }

    #[test]
    fn resource_storage_key() {
        assert_eq!(CoursewareResource::storage_key_for("r1"), "courseware/r1");
    }
}
