//! Element mapping between the internal `ElementData` format and the Seewo
//! `.enbx` format.
//!
//! ## Coordinate Systems
//!
//! The Seewo `.enbx` format uses a **Y-down** coordinate system (origin at
//! top-left, Y increases downward).  The internal format uses **Y-up**
//! (origin at bottom-left, Y increases upward).  Use [`flip_y`] to convert
//! between the two.
//!
//! ## Colours
//!
//! Seewo stores colours as 8-digit ARGB hex strings (e.g. `"FF000000"` for
//! opaque black).  Internally, colours are [`egui::Color32`].  Use
//! [`color32_to_argb_hex`] and [`argb_hex_to_color32`] for conversion.

use std::sync::{Mutex, OnceLock};

use egui::Color32;

use drafftink_core::model::{
    BaseElement, ImageElement, PathElement, ShapeElement, ShapeType, SvgShapeElement, TextElement,
};
use drafftink_core::element::{AudioElement, VideoElement};
use drafftink_core::ElementData;

use crate::parser::{
    Enbx3dShape, EnbxActivity, EnbxActivityItem, EnbxAudio, EnbxElement, EnbxGroup, EnbxImage,
    EnbxPath, EnbxShape, EnbxText, EnbxTopic, EnbxVideo, XmlValue,
};

// ---------------------------------------------------------------------------
// Coordinate system conversion
// ---------------------------------------------------------------------------

/// Flip a Y coordinate between Y-up and Y-down coordinate systems.
///
/// `slide_height` is the total height of the slide/canvas.  Applying `flip_y`
/// twice with the same `slide_height` returns the original value, making the
/// operation an involution (its own inverse).
///
/// # Examples
///
/// ```
/// # use drafftink_enbx::flip_y;
/// assert_eq!(flip_y(0.0, 720.0), 720.0);
/// assert_eq!(flip_y(720.0, 720.0), 0.0);
/// assert!((flip_y(flip_y(123.4, 720.0), 720.0) - 123.4).abs() < 1e-9);
/// ```
pub fn flip_y(y: f64, slide_height: f64) -> f64 {
    slide_height - y
}

// ---------------------------------------------------------------------------
// Colour conversion
// ---------------------------------------------------------------------------

/// Convert an [`egui::Color32`] to an 8-digit ARGB hex string (no `#` prefix).
///
/// # Examples
///
/// ```
/// # use drafftink_enbx::color32_to_argb_hex;
/// # use egui::Color32;
/// let c = Color32::from_rgba_unmultiplied(0, 0, 0, 255);
/// assert_eq!(color32_to_argb_hex(&c), "FF000000");
/// ```
pub fn color32_to_argb_hex(color: &Color32) -> String {
    format!(
        "{:02X}{:02X}{:02X}{:02X}",
        color.a(),
        color.r(),
        color.g(),
        color.b()
    )
}

/// Parse an ARGB hex colour string into an [`egui::Color32`].
///
/// Accepts `"AARRGGBB"`, `"RRGGBB"`, and variants with a leading `#`.
/// Malformed input falls back to opaque black.
///
/// # Examples
///
/// ```
/// # use drafftink_enbx::argb_hex_to_color32;
/// # use egui::Color32;
/// let c = argb_hex_to_color32("FF0000FF");
/// assert_eq!(c, Color32::from_rgba_unmultiplied(0, 0, 255, 255));
/// ```
pub fn argb_hex_to_color32(hex: &str) -> Color32 {
    let s = hex.trim().trim_start_matches('#');
    match s.len() {
        8 => {
            let v = u32::from_str_radix(s, 16).unwrap_or(0xFF000000);
            let a = ((v >> 24) & 0xFF) as u8;
            let r = ((v >> 16) & 0xFF) as u8;
            let g = ((v >> 8) & 0xFF) as u8;
            let b = (v & 0xFF) as u8;
            Color32::from_rgba_unmultiplied(r, g, b, a)
        }
        6 => {
            let v = u32::from_str_radix(s, 16).unwrap_or(0x000000);
            let r = ((v >> 16) & 0xFF) as u8;
            let g = ((v >> 8) & 0xFF) as u8;
            let b = (v & 0xFF) as u8;
            Color32::from_rgba_unmultiplied(r, g, b, 255)
        }
        _ => Color32::from_rgba_unmultiplied(0, 0, 0, 255),
    }
}

// ---------------------------------------------------------------------------
// Shape type mapping
// ---------------------------------------------------------------------------

/// Map an internal [`ShapeType`] to a Seewo shape-type string.
fn shape_type_to_enbx(st: ShapeType) -> &'static str {
    match st {
        ShapeType::Rectangle => "rectangle",
        ShapeType::Ellipse => "ellipse",
        ShapeType::Line => "line",
        ShapeType::Arrow => "arrow",
        ShapeType::Bracket => "bracket",
        ShapeType::Brace => "brace",
        ShapeType::Fan => "fan",
    }
}

/// Map a Seewo shape-type string to an internal [`ShapeType`].
///
/// Unknown strings fall back to [`ShapeType::Rectangle`].
fn shape_type_from_enbx(s: &str) -> ShapeType {
    match s.to_lowercase().as_str() {
        "rectangle" | "rect" => ShapeType::Rectangle,
        "ellipse" | "circle" | "oval" => ShapeType::Ellipse,
        "line" => ShapeType::Line,
        "arrow" => ShapeType::Arrow,
        "bracket" => ShapeType::Bracket,
        "brace" => ShapeType::Brace,
        "fan" | "sector" | "wedge" => ShapeType::Fan,
        "triangle" => ShapeType::Rectangle, // closest fallback
        _ => ShapeType::Rectangle,
    }
}

// ---------------------------------------------------------------------------
// Internal → Enbx mapping
// ---------------------------------------------------------------------------

/// Map an internal [`ElementData`] to an [`EnbxElement`].
///
/// Returns `None` for element types that have no meaningful Seewo equivalent
/// (e.g. `Unknown` XML preserved from a previous import).
///
/// # Special-Case Downgrades
///
/// | Internal type  | Enbx mapping        | Note                                |
/// |----------------|---------------------|-------------------------------------|
/// | `Formula`      | `EnbxText`          | Function expression as text (Seewo  |
/// |                |                     | cannot render curves; a full impl   |
/// |                |                     | would sample points into SVG path)  |
/// | `MindMap`      | `EnbxGroup`         | Text boxes with connectors          |
/// | `Cosmos`       | `EnbxText`          | 3D solid → wireframe description    |
/// | `Quiz`         | `EnbxText`          | Question text                       |
/// | `Geometry`     | `EnbxShape`         | Generic shape                       |
pub fn map_element_to_enbx(element: &ElementData) -> Option<EnbxElement> {
    match element {
        ElementData::Text(t) => Some(text_to_enbx(t)),
        ElementData::Image(i) => Some(image_to_enbx(i)),
        ElementData::Shape(s) => Some(shape_to_enbx(s)),
        ElementData::Path(p) => Some(path_to_enbx(p)),
        ElementData::SvgShape(s) => Some(svg_shape_to_enbx(s)),
        ElementData::Formula(f) => Some(formula_to_enbx(f)),
        ElementData::MindMap(m) => Some(mindmap_to_enbx(m)),
        ElementData::Quiz(q) => Some(quiz_to_enbx(q)),
        ElementData::Cosmos(c) => Some(cosmos_to_enbx(c)),
        ElementData::Geometry(g) => Some(geometry_to_enbx(g)),
        ElementData::Video(v) => Some(video_to_enbx(v)),
        ElementData::Audio(a) => Some(audio_to_enbx(a)),
    }
}

fn text_to_enbx(t: &TextElement) -> EnbxElement {
    EnbxElement::Text(EnbxText {
        x: t.base.position[0] as f64,
        y: t.base.position[1] as f64,
        width: t.base.size[0] as f64,
        height: t.base.size[1] as f64,
        content: t.text.clone(),
        font_size: t.font_size as f64,
        font_color: color32_to_argb_hex(&t.base.fill_color),
        bold: false,
        italic: false,
    })
}

fn image_to_enbx(i: &ImageElement) -> EnbxElement {
    EnbxElement::Image(EnbxImage {
        x: i.base.position[0] as f64,
        y: i.base.position[1] as f64,
        width: i.base.size[0] as f64,
        height: i.base.size[1] as f64,
        resource_id: i.image_path.clone(),
        opacity: i.base.opacity as f64,
    })
}

fn shape_to_enbx(s: &ShapeElement) -> EnbxElement {
    EnbxElement::Shape(EnbxShape {
        x: s.base.position[0] as f64,
        y: s.base.position[1] as f64,
        width: s.base.size[0] as f64,
        height: s.base.size[1] as f64,
        shape_type: shape_type_to_enbx(s.shape_type).to_string(),
        fill_color: color32_to_argb_hex(&s.base.fill_color),
        stroke_color: color32_to_argb_hex(&s.base.stroke_color),
        stroke_width: s.base.stroke_width as f64,
        geometry_type: None,
        path_data: None,
        line_type: None,
        arrow_head: None,
        arrow_tail: None,
        adjusts: Vec::new(),
    })
}

fn path_to_enbx(p: &PathElement) -> EnbxElement {
    let points: Vec<(f64, f64)> = p
        .points
        .iter()
        .map(|pt| (pt[0] as f64, pt[1] as f64))
        .collect();
    EnbxElement::Path(EnbxPath {
        points,
        stroke_color: color32_to_argb_hex(&p.base.stroke_color),
        stroke_width: p.base.stroke_width as f64,
        fill_color: if p.base.fill_color.a() > 0 {
            Some(color32_to_argb_hex(&p.base.fill_color))
        } else {
            None
        },
    })
}

fn svg_shape_to_enbx(s: &SvgShapeElement) -> EnbxElement {
    // Convert an SVG path element to an EnbxPath.  We extract the SVG path
    // data and store it; for simple linear paths we parse the M/L commands.
    let points = parse_svg_path_points(&s.svg_path, s.base.position, s.base.size);
    EnbxElement::Path(EnbxPath {
        points,
        stroke_color: color32_to_argb_hex(&s.base.stroke_color),
        stroke_width: s.base.stroke_width as f64,
        fill_color: if s.is_closed {
            Some(color32_to_argb_hex(&s.base.fill_color))
        } else {
            None
        },
    })
}

/// For function curves: convert to an EnbxText describing the expression.
///
/// A full implementation would use `drafftink-functions` to sample the curve
/// and generate SVG path data.  In this compatibility module we degrade to a
/// text annotation so that the expression is at least visible in Seewo.
fn formula_to_enbx(f: &drafftink_core::element::FormulaElement) -> EnbxElement {
    EnbxElement::Text(EnbxText {
        x: f.base.position[0] as f64,
        y: f.base.position[1] as f64,
        width: f.base.size[0] as f64,
        height: f.base.size[1] as f64,
        content: format!("f(x) = {}", f.expression),
        font_size: 16.0,
        font_color: color32_to_argb_hex(&Color32::from_rgba_unmultiplied(
            f.color[0],
            f.color[1],
            f.color[2],
            f.color[3],
        )),
        bold: false,
        italic: true,
    })
}

/// For mind maps: convert to text boxes with connectors (EnbxGroup).
fn mindmap_to_enbx(m: &drafftink_core::element::MindMapElement) -> EnbxElement {
    let mut group_elements: Vec<EnbxElement> = Vec::new();

    // Try to extract node text from the JSON payload.
    if let Some(nodes) = m.nodes.as_array() {
        for (i, node) in nodes.iter().enumerate() {
            let text = node
                .get("text")
                .or_else(|| node.get("label"))
                .or_else(|| node.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("Node");
            group_elements.push(EnbxElement::Text(EnbxText {
                x: m.base.position[0] as f64 + (i as f64) * 20.0,
                y: m.base.position[1] as f64 + (i as f64) * 50.0,
                width: 200.0,
                height: 40.0,
                content: text.to_string(),
                font_size: 18.0,
                font_color: "FF000000".to_string(),
                bold: i == 0,
                italic: false,
            }));
        }
    }

    if group_elements.is_empty() {
        group_elements.push(EnbxElement::Text(EnbxText {
            x: m.base.position[0] as f64,
            y: m.base.position[1] as f64,
            width: m.base.size[0] as f64,
            height: m.base.size[1] as f64,
            content: format!("MindMap ({})", m.layout),
            font_size: 18.0,
            font_color: "FF000000".to_string(),
            bold: false,
            italic: false,
        }));
    }

    EnbxElement::Group(EnbxGroup {
        x: m.base.position[0] as f64,
        y: m.base.position[1] as f64,
        width: m.base.size[0] as f64,
        height: m.base.size[1] as f64,
        elements: group_elements,
    })
}

/// For quiz elements: convert to a text box with the question.
fn quiz_to_enbx(q: &drafftink_core::element::QuizElement) -> EnbxElement {
    EnbxElement::Text(EnbxText {
        x: q.base.position[0] as f64,
        y: q.base.position[1] as f64,
        width: q.base.size[0] as f64,
        height: q.base.size[1] as f64,
        content: format!("[{}] {}", q.question_type, q.question),
        font_size: 18.0,
        font_color: "FF000000".to_string(),
        bold: false,
        italic: false,
    })
}

/// For 3D cosmos objects: in solid mode, downgrade to a text description for
/// Seewo compatibility (Seewo cannot render 3D content).
fn cosmos_to_enbx(c: &drafftink_core::element::CosmosElement) -> EnbxElement {
    EnbxElement::Text(EnbxText {
        x: c.base.position[0] as f64,
        y: c.base.position[1] as f64,
        width: c.base.size[0] as f64,
        height: c.base.size[1] as f64,
        content: if c.show_orbits {
            "3D Solar System (wireframe)".to_string()
        } else {
            "3D Solar System (solid → wireframe)".to_string()
        },
        font_size: 16.0,
        font_color: "FF333333".to_string(),
        bold: false,
        italic: true,
    })
}

/// For geometry elements: convert to a generic shape.
fn geometry_to_enbx(g: &drafftink_core::element::GeometryElement) -> EnbxElement {
    EnbxElement::Shape(EnbxShape {
        x: g.base.position[0] as f64,
        y: g.base.position[1] as f64,
        width: g.base.size[0] as f64,
        height: g.base.size[1] as f64,
        shape_type: "geometry".to_string(),
        fill_color: color32_to_argb_hex(&g.base.fill_color),
        stroke_color: color32_to_argb_hex(&g.base.stroke_color),
        stroke_width: g.base.stroke_width as f64,
        geometry_type: None,
        path_data: None,
        line_type: None,
        arrow_head: None,
        arrow_tail: None,
        adjusts: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Enbx → Internal mapping
// ---------------------------------------------------------------------------

/// Map an [`EnbxElement`] to an internal [`ElementData`].
///
/// Unlike earlier versions, this function **never silently drops** an element:
/// `Unknown` elements are surfaced as a labelled placeholder (with a one-time
/// `warn!` per tag), and richer Seewo types (Video, 3D shapes, Activity,
/// ActivityItem, Topic) are downgraded to the closest internal representation
/// rather than being discarded.
pub fn map_element_from_enbx(enbx_elem: &EnbxElement) -> ElementData {
    match enbx_elem {
        EnbxElement::Text(t) => text_from_enbx(t),
        EnbxElement::Image(i) => image_from_enbx(i),
        EnbxElement::Shape(s) => shape_from_enbx(s),
        EnbxElement::Path(p) => path_from_enbx(p),
        EnbxElement::Group(g) => group_from_enbx(g)
            .map(ElementData::Shape)
            .unwrap_or_else(|| placeholder_shape(g.x, g.y, g.width, g.height, "Group")),
        EnbxElement::Video(v) => video_from_enbx(v),
        EnbxElement::Audio(a) => audio_from_enbx(a),
        EnbxElement::Cylinder(s) | EnbxElement::Cone(s) => shape3d_from_enbx(s),
        EnbxElement::ActivityItem(a) => activity_item_from_enbx(a),
        EnbxElement::Activity(a) => activity_from_enbx(a),
        EnbxElement::Topic(t) => topic_from_enbx(t),
        EnbxElement::Unknown(xv) => {
            warn_unknown_once(&xv.tag);
            placeholder_shape(
                0.0,
                0.0,
                200.0,
                150.0,
                &format!("Unknown: {}", xv.tag),
            )
        }
    }
}

fn text_from_enbx(t: &EnbxText) -> ElementData {
    let base = BaseElement {
        id: uuid::Uuid::new_v4(),
        position: [t.x as f32, t.y as f32],
        size: [t.width as f32, t.height as f32],
        rotation: 0.0,
        z_order: 0,
        fill_color: argb_hex_to_color32(&t.font_color),
        stroke_color: Color32::TRANSPARENT,
        stroke_width: 0.0,
        opacity: 1.0,
        locked: false,
        visible: true,
        name: String::new(),
    };
    ElementData::Text(TextElement {
        base,
        text: t.content.clone(),
        font_size: t.font_size as f32,
        font_family: "Microsoft YaHei".to_string(),
    })
}

fn image_from_enbx(i: &EnbxImage) -> ElementData {
    let base = BaseElement {
        id: uuid::Uuid::new_v4(),
        position: [i.x as f32, i.y as f32],
        size: [i.width as f32, i.height as f32],
        rotation: 0.0,
        z_order: 0,
        fill_color: Color32::WHITE,
        stroke_color: Color32::TRANSPARENT,
        stroke_width: 0.0,
        opacity: i.opacity as f32,
        locked: false,
        visible: true,
        name: String::new(),
    };
    ElementData::Image(ImageElement {
        base,
        image_path: i.resource_id.clone(),
        image_data: None,
        keep_aspect: true,
    })
}

fn shape_from_enbx(s: &EnbxShape) -> ElementData {
    // Rich shapes carry raw SVG path data — render them via the dedicated
    // SVG-shape element, which has a real renderer (kurbo + lyon in the
    // display backend) that preserves full curve fidelity.  `PathElement` is
    // point-sampled (freeline/ink) and stores no SVG string, so it cannot
    // represent a raw `<Path>` payload.
    if let Some(path_data) = &s.path_data {
        let fill = argb_hex_to_color32(&s.fill_color);
        let base = BaseElement {
            id: uuid::Uuid::new_v4(),
            position: [s.x as f32, s.y as f32],
            size: [s.width as f32, s.height as f32],
            rotation: 0.0,
            z_order: 0,
            fill_color: fill,
            stroke_color: argb_hex_to_color32(&s.stroke_color),
            stroke_width: s.stroke_width as f32,
            opacity: 1.0,
            locked: false,
            visible: true,
            name: s
                .geometry_type
                .clone()
                .unwrap_or_else(|| s.shape_type.clone()),
        };
        return ElementData::SvgShape(SvgShapeElement {
            base,
            svg_path: path_data.clone(),
            is_closed: fill.a() > 0,
            has_end_arrow: s.arrow_tail.is_some(),
            has_start_arrow: s.arrow_head.is_some(),
        });
    }

    // No path data: fall back to a primitive shape, annotated with the
    // resolved geometry preset when one was present in the source.
    let base = BaseElement {
        id: uuid::Uuid::new_v4(),
        position: [s.x as f32, s.y as f32],
        size: [s.width as f32, s.height as f32],
        rotation: 0.0,
        z_order: 0,
        fill_color: argb_hex_to_color32(&s.fill_color),
        stroke_color: argb_hex_to_color32(&s.stroke_color),
        stroke_width: s.stroke_width as f32,
        opacity: 1.0,
        locked: false,
        visible: true,
        name: s.geometry_type.clone().unwrap_or_default(),
    };
    ElementData::Shape(ShapeElement {
        base,
        shape_type: shape_type_from_enbx(&s.shape_type),
        has_start_arrow: s.arrow_head.is_some(),
        has_end_arrow: s.arrow_tail.is_some(),
        scale_y: 0.0,
    })
}

fn path_from_enbx(p: &EnbxPath) -> ElementData {
    let points: Vec<[f32; 2]> = p
        .points
        .iter()
        .map(|(x, y)| [*x as f32, *y as f32])
        .collect();
    let fill_color = p
        .fill_color
        .as_ref()
        .map(|s| argb_hex_to_color32(s))
        .unwrap_or(Color32::TRANSPARENT);
    let base = BaseElement {
        id: uuid::Uuid::new_v4(),
        position: points.first().copied().unwrap_or([0.0, 0.0]),
        size: [100.0, 100.0],
        rotation: 0.0,
        z_order: 0,
        fill_color,
        stroke_color: argb_hex_to_color32(&p.stroke_color),
        stroke_width: p.stroke_width as f32,
        opacity: 1.0,
        locked: false,
        visible: true,
        name: String::new(),
    };
    ElementData::Path(PathElement {
        base,
        points,
        is_closed: p.fill_color.is_some(),
    })
}

/// Map a group to a bounding-box shape (the group itself has no direct
/// internal equivalent).  Callers that need the group's children should
/// iterate `g.elements` and map each one individually.
fn group_from_enbx(g: &EnbxGroup) -> Option<ShapeElement> {
    let base = BaseElement {
        id: uuid::Uuid::new_v4(),
        position: [g.x as f32, g.y as f32],
        size: [g.width as f32, g.height as f32],
        rotation: 0.0,
        z_order: 0,
        fill_color: Color32::TRANSPARENT,
        stroke_color: Color32::from_rgb(0x80, 0x80, 0x80),
        stroke_width: 1.0,
        opacity: 1.0,
        locked: false,
        visible: true,
        name: "group".to_string(),
    };
    Some(ShapeElement {
        base,
        shape_type: ShapeType::Rectangle,
        has_start_arrow: false,
        has_end_arrow: false,
        scale_y: 0.0,
    })
}

// ---------------------------------------------------------------------------
// V4/V5 backport: richer element mapping (no silent drops)
// ---------------------------------------------------------------------------

/// Build a neutral placeholder [`ElementData::Shape`] for unsupported elements.
fn placeholder_shape(x: f64, y: f64, w: f64, h: f64, label: &str) -> ElementData {
    let base = BaseElement {
        id: uuid::Uuid::new_v4(),
        position: [x as f32, y as f32],
        size: [w as f32, h as f32],
        rotation: 0.0,
        z_order: 0,
        fill_color: Color32::from_rgb(0xE0, 0xE0, 0xE0),
        stroke_color: Color32::from_rgb(0x80, 0x80, 0x80),
        stroke_width: 1.0,
        opacity: 1.0,
        locked: false,
        visible: true,
        name: label.to_string(),
    };
    ElementData::Shape(ShapeElement {
        base,
        shape_type: ShapeType::Rectangle,
        has_start_arrow: false,
        has_end_arrow: false,
        scale_y: 0.0,
    })
}

/// Map a `<Video>` to an internal [`VideoElement`] (playable video element).
fn video_from_enbx(v: &EnbxVideo) -> ElementData {
    let base = BaseElement {
        id: uuid::Uuid::new_v4(),
        position: [v.x as f32, v.y as f32],
        size: [v.width as f32, v.height as f32],
        rotation: 0.0,
        z_order: 0,
        fill_color: Color32::WHITE,
        stroke_color: Color32::TRANSPARENT,
        stroke_width: 0.0,
        opacity: 1.0,
        locked: false,
        visible: true,
        name: "video".to_string(),
    };
    ElementData::Video(VideoElement {
        base,
        resource_id: v.resource_id.clone(),
        is_loop: v.is_loop,
        is_auto_play: v.is_auto_play,
        volume: v.volume,
        thumbnail_id: v.thumbnail_id.clone(),
    })
}

/// Inverse of [`video_from_enbx`]: map an internal [`VideoElement`] back to the
/// Seewo `<Video>` representation for `.enbx` export.
fn video_to_enbx(v: &VideoElement) -> EnbxElement {
    EnbxElement::Video(EnbxVideo {
        resource_id: v.resource_id.clone(),
        x: v.base.position[0] as f64,
        y: v.base.position[1] as f64,
        width: v.base.size[0] as f64,
        height: v.base.size[1] as f64,
        is_loop: v.is_loop,
        is_auto_play: v.is_auto_play,
        volume: v.volume,
        thumbnail_id: v.thumbnail_id.clone(),
    })
}

/// Inverse of [`audio_to_enbx`]: map a Seewo `<Audio>` back to an internal
/// [`AudioElement`].
fn audio_from_enbx(a: &EnbxAudio) -> ElementData {
    let base = BaseElement {
        id: uuid::Uuid::new_v4(),
        position: [a.x as f32, a.y as f32],
        size: [a.width as f32, a.height as f32],
        rotation: 0.0,
        z_order: 0,
        fill_color: Color32::TRANSPARENT,
        stroke_color: Color32::TRANSPARENT,
        stroke_width: 0.0,
        opacity: 1.0,
        locked: false,
        visible: true,
        name: "audio".to_string(),
    };
    ElementData::Audio(AudioElement {
        base,
        resource_id: a.resource_id.clone(),
        duration_ms: a.duration_ms,
        is_loop: a.is_loop,
        is_auto_play: a.is_auto_play,
        volume: a.volume,
    })
}

/// Map an internal [`AudioElement`] to the Seewo `<Audio>` representation for
/// `.enbx` export. Mirrors [`video_to_enbx`].
fn audio_to_enbx(a: &AudioElement) -> EnbxElement {
    EnbxElement::Audio(EnbxAudio {
        resource_id: a.resource_id.clone(),
        x: a.base.position[0] as f64,
        y: a.base.position[1] as f64,
        width: a.base.size[0] as f64,
        height: a.base.size[1] as f64,
        is_loop: a.is_loop,
        is_auto_play: a.is_auto_play,
        volume: a.volume,
        duration_ms: a.duration_ms,
    })
}

/// 3D shapes cannot be rendered in 2D — emit a placeholder and warn.
fn shape3d_from_enbx(s: &Enbx3dShape) -> ElementData {
    log::warn!("3D shape (cylinder/cone) not renderable in 2D; emitting placeholder");
    placeholder_shape(s.x, s.y, s.width, s.height, "3D Shape")
}

/// Map an `<ActivityItem>` to an internal element.
///
/// Per the Seewo schema an item is either a background image **or** a text
/// label; when a `background_source` is present it wins (it is the card's
/// visual), otherwise the `text_content` label is used.
fn activity_item_from_enbx(a: &EnbxActivityItem) -> ElementData {
    if let Some(bg) = &a.background_source {
        let base = BaseElement {
            id: uuid::Uuid::new_v4(),
            position: [a.x as f32, a.y as f32],
            size: [a.width as f32, a.height as f32],
            rotation: 0.0,
            z_order: 0,
            fill_color: Color32::WHITE,
            stroke_color: Color32::TRANSPARENT,
            stroke_width: 0.0,
            opacity: 1.0,
            locked: false,
            visible: true,
            name: "activity-item".to_string(),
        };
        return ElementData::Image(ImageElement {
            base,
            image_path: bg.clone(),
            image_data: None,
            keep_aspect: true,
        });
    }

    let text = a
        .text_content
        .clone()
        .unwrap_or_else(|| "活动素材".to_string());
    let base = BaseElement {
        id: uuid::Uuid::new_v4(),
        position: [a.x as f32, a.y as f32],
        size: [a.width as f32, a.height as f32],
        rotation: 0.0,
        z_order: 0,
        fill_color: argb_hex_to_color32(&a.font_color),
        stroke_color: Color32::TRANSPARENT,
        stroke_width: 0.0,
        opacity: 1.0,
        locked: false,
        visible: true,
        name: "activity-item".to_string(),
    };
    ElementData::Text(TextElement {
        base,
        text,
        font_size: a.font_size as f32,
        font_family: "Microsoft YaHei".to_string(),
    })
}

/// Map an `<Activity>` (e.g. Classify) — render as a labelled placeholder.
fn activity_from_enbx(a: &EnbxActivity) -> ElementData {
    log::warn!(
        "Activity element (key={}) not fully renderable; emitting placeholder",
        a.key
    );
    let label = format!("课堂活动: {} - {}", a.key, a.name);
    placeholder_shape(0.0, 0.0, 300.0, 200.0, &label)
}

/// Map a `<Topic>` (mind map / fishbone / organization chart) to an internal
/// [`MindMapElement`], preserving the centre node and all child branches.
fn topic_from_enbx(t: &EnbxTopic) -> ElementData {
    let mut nodes: Vec<serde_json::Value> = Vec::new();

    if !t.center_text.trim().is_empty() {
        nodes.push(serde_json::json!({
            "text": t.center_text,
            "bold": true,
        }));
    }

    for child in &t.children {
        if child.text.trim().is_empty() {
            continue;
        }
        let (ox, oy) = parse_location(&child.location);
        nodes.push(serde_json::json!({
            "text": child.text,
            "offset_x": ox,
            "offset_y": oy,
            "color": child.color,
            "bg_color": child.bg_color,
        }));
    }

    let base = BaseElement {
        id: uuid::Uuid::new_v4(),
        position: [t.center_x as f32, t.center_y as f32],
        size: [t.center_w as f32, t.center_h as f32],
        rotation: 0.0,
        z_order: 0,
        fill_color: Color32::TRANSPARENT,
        stroke_color: Color32::TRANSPARENT,
        stroke_width: 0.0,
        opacity: 1.0,
        locked: false,
        visible: true,
        name: format!("topic:{}", t.topic_type),
    };

    ElementData::mindmap(base, &t.topic_type, serde_json::Value::Array(nodes))
}

/// Parse a topic node `Location` offset, e.g. `"290.5,-128"` → `(290.5, -128.0)`.
///
/// Comma-separated, tolerant of whitespace; any unparsable component falls back
/// to `0.0` (never panics).
fn parse_location(loc: &str) -> (f64, f64) {
    let mut it = loc.split(',');
    let x = it
        .next()
        .map(|s| s.trim().parse::<f64>().unwrap_or(0.0))
        .unwrap_or(0.0);
    let y = it
        .next()
        .map(|s| s.trim().parse::<f64>().unwrap_or(0.0))
        .unwrap_or(0.0);
    (x, y)
}

/// De-duplicated warning for unknown element tags — emits at most once per tag.
static UNKNOWN_WARNED: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();

fn warn_unknown_once(tag: &str) {
    let warned = UNKNOWN_WARNED.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    match warned.lock() {
        Ok(mut set) => {
            if set.insert(tag.to_string()) {
                log::warn!(
                    "Unknown ENBX element tag: {tag} (further warnings for this tag suppressed)"
                );
            }
        }
        Err(_) => {
            // Poisoned lock — fall back to an unconditional warn.
            log::warn!("Unknown ENBX element tag: {tag}");
        }
    }
}

// ---------------------------------------------------------------------------
// SVG path parsing helper
// ---------------------------------------------------------------------------

/// Parse simple SVG path commands (M, L, H, V) into world-space points.
///
/// Only linear commands are handled; curves (Q, C, A) are approximated by
/// their endpoints.  Coordinates in the SVG path are relative to the element's
/// bounding box and are converted to world space.
fn parse_svg_path_points(
    path_data: &str,
    position: [f32; 2],
    _size: [f32; 2],
) -> Vec<(f64, f64)> {
    let mut points: Vec<(f64, f64)> = Vec::new();
    let mut current_x = position[0] as f64;
    let mut current_y = position[1] as f64;

    for token in path_data.split_whitespace() {
        if token.starts_with('M') || token.starts_with('L') {
            if let Some((x, y)) = parse_coord(&token[1..]) {
                current_x = position[0] as f64 + x;
                current_y = position[1] as f64 + y;
                points.push((current_x, current_y));
            }
        } else if let Some(rest) = token.strip_prefix('H') {
            if let Ok(x) = rest.parse::<f64>() {
                current_x = position[0] as f64 + x;
                points.push((current_x, current_y));
            }
        } else if let Some(rest) = token.strip_prefix('V') {
            if let Ok(y) = rest.parse::<f64>() {
                current_y = position[1] as f64 + y;
                points.push((current_x, current_y));
            }
        }
    }

    if points.is_empty() {
        points.push((position[0] as f64, position[1] as f64));
    }
    points
}

/// Parse a coordinate pair from a string like `"10,20"` or `"10 20"`.
fn parse_coord(s: &str) -> Option<(f64, f64)> {
    let parts: Vec<&str> = s.split(|c: char| c == ',' || c.is_whitespace()).collect();
    if parts.len() >= 2 {
        let x = parts[0].parse::<f64>().ok()?;
        let y = parts[1].parse::<f64>().ok()?;
        Some((x, y))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// XmlValue serialisation helper (used by the generator)
// ---------------------------------------------------------------------------

/// Serialise an [`XmlValue`] back to an XML string.
pub(crate) fn xml_value_to_string(xv: &XmlValue, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    out.push_str(&pad);
    out.push('<');
    out.push_str(&xv.tag);
    for (k, v) in &xv.attributes {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        out.push_str(&xml_escape(v));
        out.push('"');
    }
    if xv.children.is_empty() && xv.content.is_empty() {
        out.push_str(" />\n");
    } else {
        out.push_str(">\n");
        if !xv.content.is_empty() {
            out.push_str(&"  ".repeat(indent + 1));
            out.push_str(&xml_escape(&xv.content));
            out.push('\n');
        }
        for child in &xv.children {
            out.push_str(&xml_value_to_string(child, indent + 1));
        }
        out.push_str(&pad);
        out.push_str("</");
        out.push_str(&xv.tag);
        out.push_str(">\n");
    }
    out
}

/// Escape special XML characters in text content.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_slide_xml;
    use crate::parser::ArrowEnd;
    use crate::parser::EnbxTopicNode;
    use uuid::Uuid;

    fn make_base() -> BaseElement {
        BaseElement {
            id: Uuid::new_v4(),
            position: [100.0, 200.0],
            size: [300.0, 150.0],
            ..Default::default()
        }
    }

    // ── Colour conversion ─────────────────────────────────────────────

    #[test]
    fn color_to_argb_hex_basic() {
        // Opaque colours (a = 255) store values verbatim — no premultiplication.
        let c = Color32::from_rgba_unmultiplied(0, 0, 0, 255);
        assert_eq!(color32_to_argb_hex(&c), "FF000000");

        let c2 = Color32::from_rgba_unmultiplied(255, 0, 0, 255);
        assert_eq!(color32_to_argb_hex(&c2), "FFFF0000");

        let c3 = Color32::from_rgba_unmultiplied(10, 20, 30, 255);
        assert_eq!(color32_to_argb_hex(&c3), "FF0A141E");
    }

    #[test]
    fn argb_hex_to_color_basic() {
        let c = argb_hex_to_color32("FF000000");
        assert_eq!(c, Color32::from_rgba_unmultiplied(0, 0, 0, 255));

        let c2 = argb_hex_to_color32("FFFF0000");
        assert_eq!(c2, Color32::from_rgba_unmultiplied(255, 0, 0, 255));

        let c3 = argb_hex_to_color32("80FF0000");
        assert_eq!(c3, Color32::from_rgba_unmultiplied(255, 0, 0, 128));
    }

    #[test]
    fn color_round_trip() {
        // Only opaque (a = 255) and fully-transparent (a = 0) colours
        // round-trip cleanly.  egui's `from_rgba_unmultiplied` applies
        // premultiplication + sRGB gamma correction for partial alpha
        // (0 < a < 255), so those values cannot be losslessly serialised
        // to a plain ARGB hex string.
        let colors = [
            Color32::from_rgba_unmultiplied(0, 0, 0, 0),
            Color32::from_rgba_unmultiplied(0, 0, 0, 255),
            Color32::from_rgba_unmultiplied(255, 255, 255, 255),
            Color32::from_rgba_unmultiplied(128, 64, 32, 255),
            Color32::from_rgba_unmultiplied(10, 20, 30, 255),
        ];
        for c in &colors {
            let hex = color32_to_argb_hex(c);
            let back = argb_hex_to_color32(&hex);
            assert_eq!(c, &back, "round-trip failed for {c:?}");
        }
    }

    #[test]
    fn argb_hex_with_hash_prefix() {
        let c = argb_hex_to_color32("#FF0000FF");
        assert_eq!(c, Color32::from_rgba_unmultiplied(0, 0, 255, 255));
    }

    #[test]
    fn argb_hex_six_digit_rgb() {
        let c = argb_hex_to_color32("FF0000");
        assert_eq!(c, Color32::from_rgba_unmultiplied(255, 0, 0, 255));
    }

    #[test]
    fn argb_hex_malformed_fallback() {
        let c = argb_hex_to_color32("xyz");
        assert_eq!(c, Color32::from_rgba_unmultiplied(0, 0, 0, 255));
    }

    // ── Coordinate conversion ─────────────────────────────────────────

    #[test]
    fn flip_y_basic() {
        assert_eq!(flip_y(0.0, 720.0), 720.0);
        assert_eq!(flip_y(720.0, 720.0), 0.0);
        assert_eq!(flip_y(360.0, 720.0), 360.0);
    }

    #[test]
    fn flip_y_involution() {
        for y in [0.0, 100.0, 360.0, 500.5, 720.0] {
            let round = flip_y(flip_y(y, 720.0), 720.0);
            assert!((round - y).abs() < 0.001, "round-trip failed for y={y}");
        }
    }

    #[test]
    fn flip_y_different_heights() {
        assert_eq!(flip_y(0.0, 1080.0), 1080.0);
        assert_eq!(flip_y(540.0, 1080.0), 540.0);
        assert_eq!(flip_y(1080.0, 1080.0), 0.0);
    }

    // ── Shape type mapping ────────────────────────────────────────────

    #[test]
    fn shape_type_round_trip() {
        for st in [
            ShapeType::Rectangle,
            ShapeType::Ellipse,
            ShapeType::Line,
            ShapeType::Arrow,
            ShapeType::Bracket,
            ShapeType::Brace,
            ShapeType::Fan,
        ] {
            let enbx_str = shape_type_to_enbx(st);
            let back = shape_type_from_enbx(enbx_str);
            assert_eq!(st, back, "round-trip failed for {st:?}");
        }
    }

    #[test]
    fn shape_type_aliases() {
        assert_eq!(shape_type_from_enbx("rect"), ShapeType::Rectangle);
        assert_eq!(shape_type_from_enbx("circle"), ShapeType::Ellipse);
        assert_eq!(shape_type_from_enbx("oval"), ShapeType::Ellipse);
        assert_eq!(shape_type_from_enbx("sector"), ShapeType::Fan);
    }

    #[test]
    fn shape_type_unknown_fallback() {
        assert_eq!(shape_type_from_enbx("unknown"), ShapeType::Rectangle);
        assert_eq!(shape_type_from_enbx("triangle"), ShapeType::Rectangle);
    }

    // ── Element mapping round-trip ────────────────────────────────────

    #[test]
    fn audio_round_trip() {
        use drafftink_core::element::AudioElement;
        let base = make_base();
        let original = ElementData::Audio(AudioElement {
            base: base.clone(),
            resource_id: "file:///tmp/sample.mp3".to_string(),
            duration_ms: 12_345,
            is_loop: true,
            is_auto_play: false,
            volume: 0.6,
        });

        let enbx = map_element_to_enbx(&original).expect("audio to enbx");
        // Round-trip back to an internal element.
        let back = map_element_from_enbx(&enbx);
        match back {
            ElementData::Audio(a) => {
                assert_eq!(a.resource_id, "file:///tmp/sample.mp3");
                assert_eq!(a.duration_ms, 12_345);
                assert!(a.is_loop);
                assert!(!a.is_auto_play);
                assert!((a.volume - 0.6).abs() < 1e-6);
            }
            other => panic!("expected Audio, got {other:?}"),
        }
    }

    fn text_round_trip() {
        let base = make_base();
        let original = ElementData::Text(TextElement {
            base: base.clone(),
            text: "Hello".to_string(),
            font_size: 24.0,
            font_family: "Arial".to_string(),
        });

        let enbx = map_element_to_enbx(&original).expect("to enbx");
        let back = map_element_from_enbx(&enbx);

        match back {
            ElementData::Text(t) => {
                assert_eq!(t.text, "Hello");
                assert_eq!(t.font_size, 24.0);
                assert_eq!(t.base.position, base.position);
                assert_eq!(t.base.size, base.size);
                // fill_color is mapped through ARGB hex
                assert_eq!(t.base.fill_color, base.fill_color);
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn shape_round_trip() {
        let base = make_base();
        let original = ElementData::Shape(ShapeElement {
            base: base.clone(),
            shape_type: ShapeType::Ellipse,
            has_start_arrow: false,
            has_end_arrow: false,
            scale_y: 0.0,
        });

        let enbx = map_element_to_enbx(&original).expect("to enbx");
        let back = map_element_from_enbx(&enbx);

        match back {
            ElementData::Shape(s) => {
                assert_eq!(s.shape_type, ShapeType::Ellipse);
                assert_eq!(s.base.position, base.position);
                assert_eq!(s.base.size, base.size);
                assert_eq!(s.base.fill_color, base.fill_color);
                assert_eq!(s.base.stroke_color, base.stroke_color);
                assert_eq!(s.base.stroke_width, base.stroke_width);
            }
            other => panic!("expected Shape, got {other:?}"),
        }
    }

    #[test]
    fn image_round_trip() {
        let base = make_base();
        let original = ElementData::Image(ImageElement {
            base: base.clone(),
            image_path: "images/photo.png".to_string(),
            image_data: None,
            keep_aspect: true,
        });

        let enbx = map_element_to_enbx(&original).expect("to enbx");
        let back = map_element_from_enbx(&enbx);

        match back {
            ElementData::Image(i) => {
                assert_eq!(i.image_path, "images/photo.png");
                assert_eq!(i.base.position, base.position);
                assert_eq!(i.base.size, base.size);
                assert!(i.keep_aspect);
            }
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[test]
    fn path_round_trip() {
        let base = make_base();
        let original = ElementData::Path(PathElement {
            base: base.clone(),
            points: vec![[10.0, 20.0], [30.0, 40.0], [50.0, 60.0]],
            is_closed: false,
        });

        let enbx = map_element_to_enbx(&original).expect("to enbx");
        let back = map_element_from_enbx(&enbx);

        match back {
            ElementData::Path(p) => {
                assert_eq!(p.points.len(), 3);
                assert_eq!(p.points[0], [10.0, 20.0]);
                assert_eq!(p.points[2], [50.0, 60.0]);
            }
            other => panic!("expected Path, got {other:?}"),
        }
    }

    #[test]
    fn unknown_element_maps_to_placeholder() {
        let xv = XmlValue::new("CustomWidget");
        let enbx_elem = EnbxElement::Unknown(xv);
        let mapped = map_element_from_enbx(&enbx_elem);
        match mapped {
            ElementData::Shape(s) => assert_eq!(s.base.name, "Unknown: CustomWidget"),
            other => panic!("expected placeholder Shape, got {other:?}"),
        }
    }

    // ── V4/V5 backport: richer element mapping ──────────────────────────

    #[test]
    fn video_maps_to_video() {
        let v = EnbxVideo {
            resource_id: "vid.mp4".to_string(),
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
            is_loop: false,
            is_auto_play: true,
            volume: 0.5,
            thumbnail_id: Some("poster.png".to_string()),
        };
        let mapped = map_element_from_enbx(&EnbxElement::Video(v));
        match mapped {
            ElementData::Video(vid) => {
                assert_eq!(vid.resource_id, "vid.mp4");
                assert_eq!(vid.base.position, [1.0, 2.0]);
                assert_eq!(vid.base.size, [3.0, 4.0]);
                assert!(vid.is_auto_play);
                assert_eq!(vid.volume, 0.5);
                assert_eq!(vid.thumbnail_id.as_deref(), Some("poster.png"));
            }
            other => panic!("expected Video, got {other:?}"),
        }
    }

    #[test]
    fn shape_with_path_data_maps_to_svg_shape() {
        let s = EnbxShape {
            x: 10.0,
            y: 20.0,
            width: 120.0,
            height: 80.0,
            shape_type: "cloud".to_string(),
            fill_color: "FFFF0000".to_string(),
            stroke_color: "FF000000".to_string(),
            stroke_width: 2.0,
            geometry_type: Some("Cloud".to_string()),
            path_data: Some("M0,0 L100,0 C150,50 150,150 100,100 Z".to_string()),
            line_type: Some("Dashed".to_string()),
            arrow_head: Some(ArrowEnd {
                arrow_type: "Triangle".to_string(),
                width: "Medium".to_string(),
                length: "Medium".to_string(),
            }),
            arrow_tail: None,
            adjusts: vec![],
        };
        let mapped = map_element_from_enbx(&EnbxElement::Shape(s));
        match mapped {
            ElementData::SvgShape(svg) => {
                assert_eq!(svg.svg_path, "M0,0 L100,0 C150,50 150,150 100,100 Z");
                assert_eq!(svg.base.position, [10.0, 20.0]);
                assert_eq!(svg.base.size, [120.0, 80.0]);
                assert!(svg.is_closed, "red fill (>0 alpha) => closed");
                assert!(svg.has_start_arrow, "arrow_head => start arrow");
                assert!(!svg.has_end_arrow, "no arrow_tail => no end arrow");
                assert_eq!(svg.base.name, "Cloud");
            }
            other => panic!("expected SvgShape, got {other:?}"),
        }
    }

    #[test]
    fn shape_without_path_data_maps_to_primitive() {
        let s = EnbxShape {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
            shape_type: "rectangle".to_string(),
            fill_color: "FFE0E0E0".to_string(),
            stroke_color: "FF404040".to_string(),
            stroke_width: 1.5,
            geometry_type: Some("Rectangle".to_string()),
            path_data: None,
            line_type: None,
            arrow_head: None,
            arrow_tail: None,
            adjusts: vec![],
        };
        let mapped = map_element_from_enbx(&EnbxElement::Shape(s));
        match mapped {
            ElementData::Shape(sh) => {
                assert_eq!(sh.base.name, "Rectangle");
                assert!(!sh.has_start_arrow);
                assert!(!sh.has_end_arrow);
            }
            other => panic!("expected Shape, got {other:?}"),
        }
    }

    #[test]
    fn topic_maps_to_mindmap() {
        let topic = EnbxTopic {
            topic_type: "MindMap".to_string(),
            center_text: "Root".to_string(),
            center_x: 100.0,
            center_y: 100.0,
            center_w: 200.0,
            center_h: 100.0,
            children: vec![EnbxTopicNode {
                text: "Child".to_string(),
                font_size: 16.0,
                color: "FF000000".to_string(),
                bg_color: "FFE0E0E0".to_string(),
                location: "50,-25".to_string(),
                content_width: 80.0,
                content_height: 30.0,
            }],
        };
        let mapped = map_element_from_enbx(&EnbxElement::Topic(topic));
        match mapped {
            ElementData::MindMap(m) => {
                assert_eq!(m.layout, "MindMap");
                let arr = m.nodes.as_array().expect("nodes should be an array");
                // Root (centre) + Child branch.
                assert_eq!(arr.len(), 2);
            }
            other => panic!("expected MindMap, got {other:?}"),
        }
    }

    #[test]
    fn activity_item_maps_to_text_or_image() {
        // Text-only → Text.
        let item = EnbxActivityItem {
            resource_id: String::new(),
            activity_id: "a1".to_string(),
            background_source: None,
            text_content: Some("拖拽卡片".to_string()),
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
            font_size: 18.0,
            font_color: "FF000000".to_string(),
            bold: false,
            italic: false,
        };
        let mapped = map_element_from_enbx(&EnbxElement::ActivityItem(item));
        assert!(
            matches!(mapped, ElementData::Text(_)),
            "text-only ActivityItem should map to Text"
        );

        // Background source → Image.
        let item2 = EnbxActivityItem {
            resource_id: String::new(),
            activity_id: "a1".to_string(),
            background_source: Some("bg.png".to_string()),
            text_content: None,
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
            font_size: 18.0,
            font_color: "FF000000".to_string(),
            bold: false,
            italic: false,
        };
        let mapped2 = map_element_from_enbx(&EnbxElement::ActivityItem(item2));
        match mapped2 {
            ElementData::Image(i) => assert_eq!(i.image_path, "bg.png"),
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[test]
    fn cylinder_maps_to_placeholder() {
        let s = Enbx3dShape {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
            transform: None,
        };
        let mapped = map_element_from_enbx(&EnbxElement::Cylinder(s));
        match mapped {
            ElementData::Shape(sh) => assert_eq!(sh.base.name, "3D Shape"),
            other => panic!("expected Shape placeholder, got {other:?}"),
        }
    }

    #[test]
    fn activity_maps_to_placeholder() {
        let a = EnbxActivity {
            id: "1".to_string(),
            key: "Classify".to_string(),
            name: "分类".to_string(),
            description: String::new(),
            classifies: vec![],
        };
        let mapped = map_element_from_enbx(&EnbxElement::Activity(a));
        match mapped {
            ElementData::Shape(sh) => assert!(sh.base.name.contains("Classify")),
            other => panic!("expected Shape placeholder, got {other:?}"),
        }
    }

    #[test]
    fn formula_maps_to_text() {
        let base = make_base();
        let formula = ElementData::formula(base, "sin(x)");
        let enbx = map_element_to_enbx(&formula).expect("to enbx");
        match enbx {
            EnbxElement::Text(t) => {
                assert!(t.content.contains("sin(x)"));
                assert!(t.italic);
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn mindmap_maps_to_group() {
        let base = make_base();
        let mindmap = ElementData::mindmap(
            base,
            "tree",
            serde_json::json!([{"text": "Root"}, {"text": "Child 1"}, {"text": "Child 2"}]),
        );
        let enbx = map_element_to_enbx(&mindmap).expect("to enbx");
        match enbx {
            EnbxElement::Group(g) => {
                assert_eq!(g.elements.len(), 3);
                // First node should be bold (root)
                if let EnbxElement::Text(t) = &g.elements[0] {
                    assert!(t.bold);
                    assert_eq!(t.content, "Root");
                } else {
                    panic!("expected first child to be Text");
                }
            }
            other => panic!("expected Group, got {other:?}"),
        }
    }

    #[test]
    fn cosmos_downgrades_to_text() {
        let base = make_base();
        let cosmos = ElementData::cosmos(base, serde_json::json!({}));
        let enbx = map_element_to_enbx(&cosmos).expect("to enbx");
        match enbx {
            EnbxElement::Text(t) => {
                assert!(t.content.contains("3D") || t.content.contains("wireframe"));
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    // ── XmlValue serialisation ────────────────────────────────────────

    #[test]
    fn xml_value_round_trip_string() {
        let xv = XmlValue {
            tag: "Widget".to_string(),
            attributes: {
                let mut m = std::collections::HashMap::new();
                m.insert("id".to_string(), "w1".to_string());
                m
            },
            content: "hello".to_string(),
            children: vec![],
        };
        let s = xml_value_to_string(&xv, 0);
        assert!(s.contains("<Widget"));
        assert!(s.contains("id=\"w1\""));
        assert!(s.contains("hello"));
        assert!(s.contains("</Widget>"));
    }

    #[test]
    fn xml_escape_special_chars() {
        assert_eq!(xml_escape("a<b>c&d\"e"), "a&lt;b&gt;c&amp;d&quot;e");
    }

    /// Full pipeline: parse a compound "梦幻岛屿"-style slide, then map every
    /// element.  Asserts the previously-broken behaviour is fixed — the Unknown
    /// widget is no longer silently dropped, and the new V4/V5 element types all
    /// map to a concrete `ElementData` variant.
    #[test]
    fn compound_slide_full_pipeline() {
        let xml = r#"<Slide width="1280" height="720">
  <Elements>
    <Video>
      <X>40</X><Y>40</Y><Width>480</Width><Height>270</Height>
      <Source>id://intro</Source><IsLoop>true</IsLoop><IsAutoPlay>true</IsAutoPlay>
    </Video>
    <Cylinder><X>560</X><Y>40</Y><Width>120</Width><Height>240</Height></Cylinder>
    <Cone><X>700</X><Y>40</Y><Width>120</Width><Height>240</Height></Cone>
    <ActivityItem>
      <X>40</X><Y>340</Y><Width>160</Width><Height>90</Height>
      <ResourceId>card1</ResourceId><ActivityId>act_drag</ActivityId>
      <Text>海星卡片</Text><FontSize>22</FontSize><ForegroundColor>FF00AA00</ForegroundColor>
    </ActivityItem>
    <Activity type="Classify" id="act_drag">
      <Name>海洋生物分类</Name>
      <Classify id="sea" name="海洋"><Items><Item id="x1" name="鲸鱼"/></Items></Classify>
    </Activity>
    <Topic type="MindMap">
      <X>840</X><Y>340</Y><Width>380</Width><Height>320</Height>
      <Title><Text>岛屿生态</Text></Title>
      <Nodes><Node><Title><Text>植物</Text></Title><Location>200,-100</Location></Node></Nodes>
    </Topic>
    <CustomWidget mode="legacy"><Payload>opaque</Payload></CustomWidget>
  </Elements>
</Slide>"#;

        let slide = parse_slide_xml(xml, &std::collections::HashMap::new()).expect("parse slide");
        assert_eq!(slide.elements.len(), 7);

        // map_element_from_enbx now returns ElementData directly (no Option) — a
        // dropped element would have been a compile error, but we still assert
        // the Unknown branch yields a labelled placeholder rather than nothing.
        let mapped: Vec<ElementData> = slide
            .elements
            .iter()
            .map(map_element_from_enbx)
            .collect();

        // Video → Video (no longer dropped into Unknown / coerced to Image)
        assert!(matches!(mapped[0], ElementData::Video(_)));
        // Cylinder / Cone → Shape placeholders
        assert!(matches!(mapped[1], ElementData::Shape(_)));
        assert!(matches!(mapped[2], ElementData::Shape(_)));
        // ActivityItem → Text (no background source)
        assert!(matches!(mapped[3], ElementData::Text(_)));
        // Activity → Shape placeholder
        assert!(matches!(mapped[4], ElementData::Shape(_)));
        // Topic → MindMap
        assert!(matches!(mapped[5], ElementData::MindMap(_)));
        // Unknown CustomWidget → labelled placeholder Shape (NOT dropped)
        match &mapped[6] {
            ElementData::Shape(sh) => assert!(sh.base.name.starts_with("Unknown:")),
            other => panic!("expected placeholder Shape for Unknown, got {other:?}"),
        }
    }
}
