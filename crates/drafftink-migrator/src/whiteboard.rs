//! drftx 文档模型（输出）与迁移报告。

use std::collections::HashMap;

/// 顶层文档模型。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WhiteboardDoc {
    pub metadata: Metadata,
    pub canvas: Canvas,
    pub pages: Vec<WbPage>,
    /// 迁移日志（供 `generate_report` 输出）。
    pub notes: Vec<String>,
    /// 结构化迁移说明（V4 新增）：逐元素记录降级原因与建议，便于下游审计。
    pub migration_notes: Vec<MigrationNote>,
    /// 图片媒体字典：key = media_id。
    pub media: HashMap<String, MediaAsset>,
}

/// 结构化迁移说明（V4 新增）。
///
/// 相比 `notes: Vec<String>`，本结构携带页面索引与元素类型，便于程序化消费。
/// 典型的 `suggestion` 会指引用户在 drftx 中如何手工补全被降级的元素。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MigrationNote {
    /// 所属页面（0-based）。
    pub page_index: u32,
    /// 元素类型（如 `"Cylinder"` / `"Activity"` / `"ActivityItem"`）。
    pub element_type: String,
    /// 降级 / 转换细节描述。
    pub detail: String,
    /// 可选的手工修复建议。
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Metadata {
    pub title: String,
    pub source: String,
    pub generator: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Canvas {
    pub width: f64,
    pub height: f64,
    pub background: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WbPage {
    pub index: usize,
    pub elements: Vec<WbElement>,
    pub thumbnail: Option<String>,
}

/// 元素枚举。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WbElement {
    Text(WbText),
    Image(WbImage),
    Shape(WbShape),
    Placeholder(WbPlaceholder),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WbText {
    pub content: String,
    pub font: String,
    pub size: f64,
    pub color: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WbImage {
    /// 引用 `WhiteboardDoc.media` 的 key（即 `MediaReference.id`）。
    pub media_id: String,
    /// 原始资源路径 / 引用（来自 ENBX，供追溯）。
    pub src: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 形状几何类型枚举。
///
/// 简单形状映射为具名枚举；其余（Star / Love / 任意不规则路径）统一用 `Path(raw_path)`，
/// 由序列化层优先输出其 SVG Path 指令。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WbShapeType {
    Rectangle,
    Circle,
    Ellipse,
    Triangle,
    Line,
    Polygon,
    Path(String),
}

/// 矢量图形元素。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WbShape {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// 原始 SVG Path 文本；序列化时若非空则优先写入。
    pub raw_path: String,
    /// ENBX 几何类型名（如 "Rectangle" / "Star" / "Love"）。
    pub geometry_type: String,
    /// 分类后的形状类型（简单形状用枚举，其余为 Path）。
    pub shape_type: WbShapeType,
    /// 填充色（#RRGGBB）。
    pub fill: Option<String>,
    /// 描边色（#RRGGBB）。
    pub stroke: Option<String>,
    /// 描边宽度。
    pub stroke_width: f64,
    /// 不透明度 0..=1。
    pub opacity: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WbPlaceholder {
    pub reason: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 二进制媒体资源（图片 / 视频 / 缩略图等）。
///
/// 由 `convert_picture` / `convert_video` 从 ENBX `Resources/` 目录读取字节后填充，
/// 并以 `MediaReference.id` 为 key 存入 `WhiteboardDoc.media`。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MediaAsset {
    /// 文件名（由 `Reference.Target` 推导）。
    pub filename: String,
    /// MIME 类型（由扩展名推断，如 `image/jpeg` / `video/x-matroska`）。
    pub mime: String,
    /// 文件原始字节。
    pub data: Vec<u8>,
}

/// 迁移报告（可序列化为 JSON）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MigrationReport {
    pub total_elements: usize,
    pub success_count: usize,
    pub placeholders: usize,
    pub logs: Vec<String>,
}
