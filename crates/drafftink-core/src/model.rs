//! Data model for the SeewoClass courseware editor.
//!
//! Defines all element types, the document structure, and serialization helpers.

use egui::Color32;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Identifier
// ---------------------------------------------------------------------------

/// Unique identifier for each element on the canvas.
pub type ElementId = Uuid;

// ---------------------------------------------------------------------------
// Color serialization helper
// ---------------------------------------------------------------------------

mod color_serde {
    use egui::Color32;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(color: &Color32, s: S) -> Result<S::Ok, S::Error> {
        [color.r(), color.g(), color.b(), color.a()].serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Color32, D::Error> {
        let rgba = <[u8; 4]>::deserialize(d)?;
        Ok(Color32::from_rgba_unmultiplied(
            rgba[0], rgba[1], rgba[2], rgba[3],
        ))
    }
}

// ---------------------------------------------------------------------------
// Base element (common to all element types)
// ---------------------------------------------------------------------------

/// Common fields shared by every element on the canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseElement {
    pub id: ElementId,
    /// World-space position `[x, y]`.
    pub position: [f32; 2],
    /// World-space size `[width, height]`.
    pub size: [f32; 2],
    /// Rotation in radians.
    pub rotation: f32,
    /// Depth ordering — higher values render on top.
    pub z_order: i32,
    #[serde(with = "color_serde")]
    pub fill_color: Color32,
    #[serde(with = "color_serde")]
    pub stroke_color: Color32,
    pub stroke_width: f32,
    /// 0.0 = fully transparent, 1.0 = fully opaque.
    pub opacity: f32,
    pub locked: bool,
    pub visible: bool,
    /// Human-readable label (shown in layer panel).
    #[serde(default)]
    pub name: String,
}

impl Default for BaseElement {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            position: [0.0, 0.0],
            size: [100.0, 100.0],
            rotation: 0.0,
            z_order: 0,
            fill_color: Color32::from_rgb(0x3A, 0x86, 0xFF),
            stroke_color: Color32::BLACK,
            stroke_width: 2.0,
            opacity: 1.0,
            locked: false,
            visible: true,
            name: String::new(),
        }
    }
}

impl BaseElement {
    /// World-space bounding rect `[left, top, right, bottom]`.
    pub fn world_bounds(&self) -> [f32; 4] {
        let [x, y] = self.position;
        let [w, h] = self.size;
        [x, y, x + w, y + h]
    }

    /// Check whether a world-space point hits this element.
    pub fn hit_test(&self, world_pt: [f32; 2]) -> bool {
        let [l, t, r, b] = self.world_bounds();
        world_pt[0] >= l && world_pt[0] <= r && world_pt[1] >= t && world_pt[1] <= b
    }
}

// ---------------------------------------------------------------------------
// Element variants
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ShapeType {
    Rectangle,
    Ellipse,
    Line,
    Arrow,
    Bracket,
    Brace,
    /// Filled circular sector / wedge (扇形), rendered via SVG path with fill.
    Fan,
}

/// 备课顶边栏「形状」选择器使用的形状种类（宿主层叠加层）。
///
/// 与文档层的 [`ShapeType`] 相互独立：本枚举驱动「顶边栏选择 → 形状叠加层插入」
/// 这条宿主侧流程；而 [`ShapeType`] 是 `.enbx` 文档里已落盘的 `Element::Shape` 类型。
/// 两者刻意解耦，避免改动 [`ShapeType`] 的穷举匹配破坏既有渲染器
/// （`drafftink-edit` / `drafftink-display` 的 `draw_shape`）。形状叠加层不进文档、
/// 不持久化（与图片/视频叠加层一致），因此 [`ShapeKind`] 的增删不影响 269 项既有测试。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ShapeKind {
    /// 圆（取 rect 宽高中较小者为直径）。
    Circle,
    /// 正方形（比例由 rect 决定，初始为 200×200）。
    Square,
    /// 长方形。
    Rectangle,
    /// 圆角矩形。
    RoundedRect,
    /// 小括号 `(`。
    Parenthesis,
    /// 中括号 `[`。
    Bracket,
    /// 大括号 `{`。
    Brace,
    /// 单箭头 `→`。
    Arrow,
    /// 双箭头 `⇌`。
    DoubleArrow,
    /// 直线段（虚拟教具提交：三角尺沿尺边画线）。
    ///
    /// 线段即 rect 的一条对角线：`line_flipped = false` 为左上→右下，
    /// `true` 为右上→左下。rect 恰为线段两端点的包围盒。
    Line,
    /// 圆弧（虚拟教具提交：圆规弧模式 / 量角器画弧）。
    ///
    /// 圆心 = rect 中心，半径 = rect 宽高较小者的一半；
    /// 起止角由 `arc_degrees: (start, end)` 给出（屏幕空间角度，度）。
    Arc,
    /// 扇形（虚拟教具提交：圆规扇形模式）。
    ///
    /// 几何约定与 [`ShapeKind::Arc`] 一致，提交时携带浅色填充。
    Sector,
    /// 角（虚拟教具提交：量角器画角模式）。
    ///
    /// 顶点 = rect 中心，两条射线长度 = rect 宽高较小者的一半，
    /// 射线方向由 `arc_degrees: (边1角, 边2角)` 给出。
    Angle,
    /// 正多边形（虚拟教具提交：正多边形工具）。
    ///
    /// `center`/`radius` 为定义几何（屏幕空间，与教具预览一致）；`sides` 为边数（3–12）。
    /// 实际渲染时与 [`ShapeKind::Arc`]/[`ShapeKind::Sector`] 一致，由叠加层 rect 派生
    /// 中心与半径（保证拖拽缩放后几何仍跟随），因此 `center`/`radius` 主要作为
    /// 提交时的定义参数保存，渲染以 rect 为唯一真相来源。
    Polygon { center: [f32; 2], radius: f32, sides: u8 },
    /// 数轴（虚拟教具提交：数轴工具）。
    ///
    /// 携带完整刻度参数（[`NumberLineData`]）：主/次刻度、数值标签（起点数值 +
    /// 每格增量）、左右箭头。渲染时主线方向由叠加层 rect 宽高比派生（宽 ≥ 高 →
    /// 水平左→右；否则垂直上→下），刻度沿主线按 `step` 等距排布——与
    /// [`ShapeKind::Polygon`] 一致，以 rect 为唯一真相来源，`start`/`end` 主要
    /// 作为提交时的定义参数保存（保证拖拽缩放后刻度仍跟随）。
    NumberLine(NumberLineData),
}

/// 数轴完整刻度参数（[`ShapeKind::NumberLine`] 的载荷）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NumberLineData {
    /// 左端点（屏幕坐标，提交时保存；渲染以 rect 派生为准）。
    pub start: [f32; 2],
    /// 右端点（屏幕坐标）。
    pub end: [f32; 2],
    /// 主刻度间隔（像素）。
    pub step: f32,
    /// 每个主刻度的细分数量（≥1，默认 2 → 半格次刻度）。
    pub minor_divisions: u8,
    /// 每几个主刻度标一个数字（≥1，默认 1 → 每格都标）。
    pub label_interval: i32,
    /// 左端点对应的数值（默认 0.0）。
    pub start_value: f32,
    /// 每个主刻度代表的数值增量（默认 1.0）。
    pub unit_per_major: f32,
    /// 右端是否画箭头（默认 true；左端始终画小号反向箭头，双向数轴风格）。
    pub show_arrow: bool,
    /// 主刻度线长度（像素，默认 10.0）。
    pub tick_length: f32,
    /// 次刻度线长度（像素，默认 5.0）。
    pub minor_tick_length: f32,
}

impl Default for NumberLineData {
    fn default() -> Self {
        Self {
            start: [0.0, 0.0],
            end: [200.0, 0.0],
            step: 40.0,
            minor_divisions: 2,
            label_interval: 1,
            start_value: 0.0,
            unit_per_major: 1.0,
            show_arrow: true,
            tick_length: 10.0,
            minor_tick_length: 5.0,
        }
    }
}

impl ShapeKind {
    /// 顶边栏形状选择器展示的全部种类（顺序即展示顺序）。
    ///
    /// 注意：`Line` / `Arc` / `Sector` / `Angle` 是虚拟教具（圆规 / 三角尺 /
    /// 量角器）的提交产物，**不进**本列表——它们由教具交互直接产生，
    /// 不通过「形状选择器 → 插入」这条普通流程，保持顶边栏选择器不变。
    pub const ALL: [ShapeKind; 9] = [
        ShapeKind::Circle,
        ShapeKind::Square,
        ShapeKind::Rectangle,
        ShapeKind::RoundedRect,
        ShapeKind::Parenthesis,
        ShapeKind::Bracket,
        ShapeKind::Brace,
        ShapeKind::Arrow,
        ShapeKind::DoubleArrow,
    ];
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShapeElement {
    pub base: BaseElement,
    pub shape_type: ShapeType,
    /// Whether to draw an arrow head at the start point.
    #[serde(default)]
    pub has_start_arrow: bool,
    /// Whether to draw an arrow head at the end point.
    #[serde(default)]
    pub has_end_arrow: bool,
    /// Curvature parameter for Brace shapes (0.0–1.0).
    #[serde(default)]
    pub scale_y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextElement {
    pub base: BaseElement,
    pub text: String,
    pub font_size: f32,
    pub font_family: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageElement {
    pub base: BaseElement,
    /// Path relative to the .courseware file's asset directory.
    pub image_path: String,
    /// In-memory pixel data (never serialized).
    #[serde(skip)]
    pub image_data: Option<Vec<u8>>,
    /// Preserve original aspect ratio.
    pub keep_aspect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathElement {
    pub base: BaseElement,
    /// Series of world-space points `[[x, y], ...]`.
    pub points: Vec<[f32; 2]>,
    pub is_closed: bool,
}

/// An element described by an SVG path string (e.g. FreeLine curve arrows).
///
/// The `svg_path` uses coordinates relative to the element's bounding box
/// (`base.position` + `base.size`). The renderer is responsible for parsing
/// the path data and applying the world-to-screen transform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SvgShapeElement {
    pub base: BaseElement,
    /// Raw SVG path data (e.g. "M10,10 Q50,50 100,10").
    pub svg_path: String,
    /// Whether the path is closed (filled).
    #[serde(default)]
    pub is_closed: bool,
    /// Whether to draw an arrow head at the end of the path.
    #[serde(default)]
    pub has_end_arrow: bool,
    /// Whether to draw an arrow head at the start of the path.
    #[serde(default)]
    pub has_start_arrow: bool,
}

// ---------------------------------------------------------------------------
// Element enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Element {
    #[serde(rename = "shape")]
    Shape(ShapeElement),
    #[serde(rename = "text")]
    Text(TextElement),
    #[serde(rename = "image")]
    Image(ImageElement),
    #[serde(rename = "path")]
    Path(PathElement),
    #[serde(rename = "svg_shape")]
    SvgShape(SvgShapeElement),
}

impl Element {
    /// Return a reference to the shared base.
    pub fn base(&self) -> &BaseElement {
        match self {
            Element::Shape(s) => &s.base,
            Element::Text(t) => &t.base,
            Element::Image(i) => &i.base,
            Element::Path(p) => &p.base,
            Element::SvgShape(s) => &s.base,
        }
    }

    /// Return a mutable reference to the shared base.
    pub fn base_mut(&mut self) -> &mut BaseElement {
        match self {
            Element::Shape(s) => &mut s.base,
            Element::Text(t) => &mut t.base,
            Element::Image(i) => &mut i.base,
            Element::Path(p) => &mut p.base,
            Element::SvgShape(s) => &mut s.base,
        }
    }

    pub fn id(&self) -> ElementId {
        self.base().id
    }

    /// Approximate vertex count of this element (for undo-stack bounding).
    pub fn point_count(&self) -> usize {
        match self {
            Element::Shape(_) | Element::Text(_) | Element::Image(_) | Element::SvgShape(_) => 4, // bounding quad
            Element::Path(p) => p.points.len(),
        }
    }
}

// ---------------------------------------------------------------------------
// Page content (multi-page support)
// ---------------------------------------------------------------------------

/// Content of a single page in a multi-page courseware document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct PageContent {
    pub elements: Vec<Element>,
    /// Bincode-serialised `Vec<StrokeData>` (annotation layer).
    #[serde(default)]
    pub annotations_data: Vec<u8>,
    /// Per-element animation configurations for this page.
    #[serde(default)]
    pub animations: HashMap<Uuid, crate::animation::ElementAnimation>,
    /// Play-order sequence for this page.
    #[serde(default)]
    pub animation_sequence: Option<crate::animation::SlideAnimationSequence>,
}


// ---------------------------------------------------------------------------
// Document
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoursewareDoc {
    pub version: String,
    /// Page dimensions in world units `[width, height]`.
    #[serde(default = "default_page_size")]
    pub page_size: [f32; 2],
    /// Background color as `[r, g, b, a]`.
    #[serde(default = "default_bg_color")]
    pub background_color: [u8; 4],
    pub elements: Vec<Element>,
    /// Multi-page content.  If empty (legacy files), `elements` is used as page 0.
    #[serde(default)]
    pub pages: Vec<PageContent>,
}

fn default_page_size() -> [f32; 2] { [1920.0, 1080.0] }
fn default_bg_color() -> [u8; 4] { [255, 255, 255, 255] }

impl Default for CoursewareDoc {
    fn default() -> Self {
        Self {
            version: "1.0".into(),
            page_size: [1920.0, 1080.0],
            background_color: [255, 255, 255, 255],
            elements: Vec::new(),
            pages: Vec::new(),
        }
    }
}

impl CoursewareDoc {
    /// Create an empty document with one blank page.  Used by display.exe
    /// when launched without a file argument.
    pub fn empty() -> Self {
        Self {
            version: "1.0".into(),
            page_size: [1920.0, 1080.0],
            background_color: [255, 255, 255, 255],
            elements: Vec::new(),
            pages: vec![PageContent::default()],
        }
    }

    /// Find an element by id.
    pub fn get(&self, id: ElementId) -> Option<&Element> {
        self.elements.iter().find(|e| e.id() == id)
            .or_else(|| self.pages.iter().flat_map(|p| p.elements.iter()).find(|e| e.id() == id))
    }

    /// Find an element by id (mutable).
    pub fn get_mut(&mut self, id: ElementId) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id() == id)
            .or_else(|| self.pages.iter_mut().flat_map(|p| p.elements.iter_mut()).find(|e| e.id() == id))
    }

    /// Remove an element by id, returning it.
    pub fn remove(&mut self, id: ElementId) -> Option<Element> {
        if let Some(pos) = self.elements.iter().position(|e| e.id() == id) {
            Some(self.elements.remove(pos))
        } else {
            for page in &mut self.pages {
                if let Some(pos) = page.elements.iter().position(|e| e.id() == id) {
                    return Some(page.elements.remove(pos));
                }
            }
            None
        }
    }

    /// Insert a new element and assign it a z-order that places it on top.
    pub fn push(&mut self, element: Element) {
        let max_z = self
            .elements
            .iter()
            .map(|e| e.base().z_order)
            .max()
            .unwrap_or(-1);
        let mut element = element;
        element.base_mut().z_order = max_z + 1;
        if self.pages.is_empty() {
            self.elements.push(element);
        } else {
            self.pages[0].elements.push(element);
        }
    }

    /// Sort elements by z_order (lowest first).
    pub fn sort_by_z(&mut self) {
        self.elements.sort_by_key(|e| e.base().z_order);
        for page in &mut self.pages {
            page.elements.sort_by_key(|e| e.base().z_order);
        }
    }
}
