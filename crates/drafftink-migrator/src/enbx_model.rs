//! ENBX 已解析数据模型（上游 `drafftink-enbx` 产出）。
//!
//! 本模块仅声明 migrator 实际用到的字段。在真实集成时，这些中间类型可由
//! `drafftink-enbx` 的解析结果通过 `From` 适配而来，业务转换代码无需改动。
//!
//! 关于 Shape 字段映射（上游 XML 解析职责）：
//! - `<X>/<Y>/<Width>/<Height>` → `x,y,w,h`
//! - `<Background>` / `<ColorBrush>`（填充）→ `fill`
//! - `<Foreground>` / `<ColorBrush>`（描边）→ `stroke`
//! - `<Thickness>` → `stroke_width`
//! - `<Opacity>` → `opacity`
//! - `<Path>`（SVG 文本）→ `raw_path`
//! - `<GeometryType>` → `geometry_type`
//!
//! 关于多媒体资源映射（本模块新增）：
//! - `Reference`：由 ENBX 解包后的 `Reference.xml` 解析而来，记录 `Resources/`
//!   下每个二进制文件的 `id → 相对路径` 关系。
//! - `<Picture>` → `EnbxPicture`，`source` 形如 `id://<id>`，通过 `Reference::resolve` 反查文件。
//! - `<Video>`  → `EnbxVideo`，`source` / `thumbnail` 同样走 `id://` 反查。

use std::collections::HashMap;

/// 顶层解析结果（作业入口输入）。
#[derive(Debug, Clone)]
pub struct EnbxParsed {
    pub board: BoardXml,
    pub slides: Vec<SlideXml>,
    pub thumbnails: HashMap<usize, String>,
    /// ENBX 解包后的 `Reference.xml` 解析结果（资源清单）。
    ///
    /// 即使为空（`Default`），`convert` 也不会崩溃——仅会导致媒体无法解析，
    /// 相应元素降级为占位符。
    pub reference: Reference,
}

/// 白板（课件）元信息。
#[derive(Debug, Clone)]
pub struct BoardXml {
    pub name: String,
    /// 设计宽度，可能为 0 或异常值，转换时会校准到视口内。
    pub width: f64,
    /// 设计高度。
    pub height: f64,
}

/// 单页（一屏）幻灯片。
#[derive(Debug, Clone)]
pub struct SlideXml {
    pub id: String,
    pub elements: Vec<EnbxElement>,
}

/// 元素枚举：文本 / 图片 / 形状 / 图片资源 / 视频 / 未知。
///
/// `Unknown` 承载所有未识别标签（如 `<seewo:xxx>`、动画标签等），
/// V1 策略下会被降级为占位符而非崩溃。
#[derive(Debug, Clone)]
pub enum EnbxElement {
    Text(TextXml),
    /// 内嵌 / base64 图片（无外部资源引用的简单情形）。
    Image(ImageXml),
    Shape(ShapeXml),
    /// `<Picture>` 元素，引用 `Reference.xml` 中的外部资源。
    Picture(EnbxPicture),
    /// `<Video>` 元素，引用 `Reference.xml` 中的外部资源（及可选缩略图）。
    Video(EnbxVideo),
    /// `<Cylinder>` 3D 形状（drftx V1 不支持，降级为占位符）。
    Cylinder(Enbx3dShape),
    /// `<Cone>` 3D 形状（drftx V1 不支持，降级为占位符）。
    Cone(Enbx3dShape),
    /// `<ActivityItem>`：课堂活动的容器或素材元素。
    ActivityItem(EnbxActivityItem),
    /// `<Activity>`（如 `<Classify>`）分类课堂活动配置（drftx V1 不支持，降级为占位符）。
    Activity(EnbxActivity),
    /// `<Topic>`（思维导图 / 鱼骨图 / 组织结构图），展开为多个独立文本节点。
    Topic(EnbxTopic),
    /// 未识别标签，携带原始标签名。
    Unknown(String),
}

/// 文本元素。
#[derive(Debug, Clone)]
pub struct TextXml {
    pub content: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// 文本 run 列表，首个 run 的样式作为整段样式（V1 简化）。
    pub runs: Vec<TextRun>,
}

/// 单个文本 run 的样式。
#[derive(Debug, Clone)]
pub struct TextRun {
    pub font: String,
    pub size: f64,
    pub color: String,
}

/// 内嵌图片元素（base64 或 src 引用，无外部资源清单）。
#[derive(Debug, Clone)]
pub struct ImageXml {
    pub src: String,
    /// 内嵌 base64（若有）。
    pub data: Option<String>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 形状元素（ENBX XML 已解析）。
///
/// 所有坐标、样式、SVG Path 与 GeometryType 均已由上游抽取。
#[derive(Debug, Clone)]
pub struct ShapeXml {
    /// 几何类型（如 Rectangle / Circle / Star / Love …）。
    pub geometry_type: String,
    /// 完整 SVG Path 文本（如 "M0,0 L100,0 L100,50 Z"）。
    pub raw_path: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// 填充色（希沃格式 #AARRGGBB，转换时去 Alpha）。
    pub fill: Option<String>,
    /// 描边色（同上）。
    pub stroke: Option<String>,
    /// 描边宽度。
    pub stroke_width: f64,
    /// 不透明度 0..=1。
    pub opacity: f64,
}

/// `<Picture>` 元素（ENBX `Slide_N.xml` 的 `<Elements>` 内）。
///
/// 坐标经 `DisplayRegion` / `<X><Y><Width><Height>` 抽取，统一为 `f64` 与全工程一致。
#[derive(Debug, Clone)]
pub struct EnbxPicture {
    /// 资源引用，形如 `id://<MediaReference.id>`。
    pub source: String,
    pub picture_name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// 原始显示区域字符串（如 `"0,0,1440,2200"`），原样保留供调试。
    pub display_region: Option<String>,
}

/// `<Video>` 元素。
#[derive(Debug, Clone)]
pub struct EnbxVideo {
    /// 视频资源引用，形如 `id://<id>`。
    pub source: String,
    pub media_name: String,
    /// 缩略图引用（可选），形如 `id://<id>`。
    pub thumbnail: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_loop: bool,
    pub is_auto_play: bool,
}

/// `<Cylinder>` / `<Cone>` 等 3D 形状（ENBX 3D 元素）。
///
/// 坐标使用 `f32` 与上游 XML 字段类型一致；转换时统一 cast 为 `f64` 再做坐标校准。
#[derive(Debug, Clone)]
pub struct Enbx3dShape {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// 3D 变换矩阵（16 个分量，逗号分隔），drftx V1 不支持，仅透传保留。
    pub transform: Option<String>,
}

/// `<ActivityItem>`：课堂活动的容器（Container）或素材（Material）元素。
#[derive(Debug, Clone)]
pub struct EnbxActivityItem {
    /// `"Container"` | `"Material"`。
    pub resource_type: String,
    pub activity_id: String,
    pub resource_id: String,
    /// 背景资源引用，形如 `id://<id>`（对应 `Reference.xml` 中的资源）。
    pub background_source: Option<String>,
    /// 第一层文本（从 `<Text><RichText><Text>` 抽取）。
    pub text_content: Option<String>,
    /// 独立 `<RichText><Text>` 文本（作为 `text_content` 的 fallback）。
    pub rich_text_content: Option<String>,
    pub font_size: f32,
    pub font_weight: String,
    pub font_style: String,
    pub font_family: String,
    pub foreground_color: String,
    pub background_color: String,
    pub text_offset_x: f32,
    pub text_offset_y: f32,
    pub text_editor_width: f32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// `<Activity>`（如 `<Classify>`）分类课堂活动配置。
#[derive(Debug, Clone)]
pub struct EnbxActivity {
    pub id: String,
    /// 活动类型键，如 `"Classify"`。
    pub key: String,
    pub name: String,
    pub description: String,
    /// 缩略图绝对路径（来自希沃安装目录，**可能不存在**）。
    pub thumbnail_abs_path: Option<String>,
    pub classifies: Vec<EnbxClassify>,
}

/// 单个分类（如「城市名称」）。
#[derive(Debug, Clone)]
pub struct EnbxClassify {
    pub id: String,
    pub name: String,
    pub items: Vec<EnbxClassifyItem>,
}

/// 分类下的一个选项（如「北京」）。
#[derive(Debug, Clone)]
pub struct EnbxClassifyItem {
    pub id: String,
    pub name: String,
}

/// `<Topic>`（思维导图 / 鱼骨图 / 组织结构图）根节点。
#[derive(Debug, Clone)]
pub struct EnbxTopic {
    /// 类型：`MindMap` / `FishBoneMap` / `Organization`。
    pub topic_type: String,
    /// 连线样式：`Ellipse` / `StraightLine` / `PolyLineWithRadius`。
    pub branch_type: String,
    /// 皮肤：`BlueSkin` 等。
    pub skin_type: String,
    /// 中心节点文本（从 `<Title><Text>` 提取）。
    pub center_text: String,
    /// 中心节点字号。
    pub center_font_size: f64,
    /// 中心节点前景色（#AARRGGBB）。
    pub center_color: String,
    /// 中心节点背景色（#AARRGGBB）。
    pub center_bg_color: String,
    /// Topic 包围盒（左上角坐标 + 尺寸）。
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// 中心节点内容尺寸。
    pub content_width: f64,
    pub content_height: f64,
    /// 子节点（分支）。
    pub children: Vec<EnbxTopicNode>,
}

/// `<Topic>` 的一个子节点（分支）。
#[derive(Debug, Clone)]
pub struct EnbxTopicNode {
    /// 子节点文本（从 `<Title><Text>` 提取）。
    pub text: String,
    pub font_size: f64,
    /// 前景色（#AARRGGBB）。
    pub color: String,
    /// 背景色（#AARRGGBB）。
    pub bg_color: String,
    /// 相对 Topic 中心的偏移，形如 `"290.5,-128"`。
    pub location: String,
    pub content_width: f64,
    pub content_height: f64,
}

/// 迁移错误（目前仅 XML 解析失败场景）。
#[derive(Debug, Clone, PartialEq)]
pub enum MigratorError {
    Xml(String),
}

/// `Reference.xml` 解析结果：资源关系表。
///
/// 结构（ENBX 真实数据）：
/// ```xml
/// <SaveInfoMetadataFile>
///   <MetadataContract>
///     <Relationship>
///       <Id>9f03886f...cc69</Id>
///       <Target>Resources\9f03886f...cc69.jpg</Target>
///       <Hash>0d292276...5c5e</Hash>
///     </Relationship>
///   </MetadataContract>
/// </SaveInfoMetadataFile>
/// ```
/// 注意 `<Target>` 使用反斜杠 `\` 分隔路径，本模块在拼接时统一兼容 `\` 与 `/`。
#[derive(Debug, Clone, Default)]
pub struct Reference {
    /// key = `MediaReference.id`（即 `id://` 之后的部分）。
    pub relationships: HashMap<String, MediaReference>,
}

/// 单条资源关系。
#[derive(Debug, Clone)]
pub struct MediaReference {
    /// 资源唯一 id（无 `id://` 前缀）。
    pub id: String,
    /// 相对路径（相对 ENBX 解包根目录），可能含 `\` 或 `/`。
    pub target: String,
    /// 文件哈希（用于完整性校验，本迁移器仅透传）。
    pub hash: String,
    /// 由 `target` 推导出的纯文件名（如 `9f...cc69.jpg`）。
    pub filename: String,
    /// 由 `target` 推导出的小写扩展名（如 `jpg`），用于 MIME 推断。
    pub extension: String,
}

impl Reference {
    /// 从 `Reference.xml` 文本解析资源关系表。
    ///
    /// 采用轻量标签扫描（零第三方依赖；完整 XML 解析由上游 `drafftink-enbx` 完成，
    /// 此处仅需抽取结构简单、可预期的 `<Relationship>` 块）。大小写敏感匹配已知标签。
    pub fn from_xml(xml: &str) -> Result<Self, MigratorError> {
        let mut relationships: HashMap<String, MediaReference> = HashMap::new();
        let mut rest = xml;
        while let Some(open) = rest.find("<Relationship") {
            // 找到最近的 </Relationship> 作为块结束（Relationship 不会嵌套）。
            let close_offset = match rest[open..].find("</Relationship>") {
                Some(o) => open + o + "</Relationship>".len(),
                None => break,
            };
            let block = &rest[open..close_offset];

            let id = match tag_text(block, "Id") {
                Some(v) => v,
                None => {
                    rest = &rest[close_offset..];
                    continue;
                }
            };
            let target = tag_text(block, "Target").unwrap_or_default();
            let hash = tag_text(block, "Hash").unwrap_or_default();

            let filename = target
                .rsplit('\\')
                .next()
                .or_else(|| target.rsplit('/').next())
                .unwrap_or(&target)
                .to_string();
            let extension = std::path::Path::new(&filename)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            relationships.insert(
                id.clone(),
                MediaReference {
                    id,
                    target,
                    hash,
                    filename,
                    extension,
                },
            );
            rest = &rest[close_offset..];
        }
        Ok(Reference { relationships })
    }

    /// 解析形如 `id://<id>` 的 `source`，返回对应的 `MediaReference`。
    ///
    /// 已自动剥离 `id://` 前缀；若传入的已是裸 id 也可直接命中。
    pub fn resolve(&self, source: &str) -> Option<&MediaReference> {
        let id = source.strip_prefix("id://").unwrap_or(source);
        self.relationships.get(id)
    }
}

/// 在 `s` 中抽取 `<tag>content</tag>` 的 `content`（大小写敏感，忽略两端空白）。
fn tag_text(s: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = s.find(&open)? + open.len();
    let end = s[start..].find(&close)? + start;
    Some(s[start..end].trim().to_string())
}
