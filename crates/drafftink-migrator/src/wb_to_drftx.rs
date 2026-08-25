//! drftx 序列化层：将 `WhiteboardDoc` 输出为 drftx 文档 JSON。
//!
//! 关键规则：对于 `WbShape`，若 `raw_path` 非空，**优先写入 SVG Path 指令**，
//! 而非退化为多边形顶点；简单形状在 `raw_path` 为空时也会由 `shape_path` 合成 SVG Path。

use serde_json::{json, to_value, Value};

use crate::whiteboard::{WbElement, WbShape, WbShapeType, WhiteboardDoc};

/// 将整个文档序列化为 drftx JSON 字符串。
pub fn to_drftx(doc: &WhiteboardDoc) -> String {
    let value = json!({
        "metadata": to_value(&doc.metadata).unwrap_or(Value::Null),
        "canvas": to_value(&doc.canvas).unwrap_or(Value::Null),
        "pages": doc.pages.iter().map(|p| json!({
            "index": p.index,
            "thumbnail": p.thumbnail,
            "elements": p.elements.iter().map(element_to_drftx).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        // 媒体字典：二进制以 base64 输出，避免巨大的字节数组。
        "media": doc.media.iter().map(|(k, a)| json!({
            "id": k,
            "filename": a.filename,
            "mime": a.mime,
            "data": crate::enbx_to_wb::base64_encode(&a.data),
            "size": a.data.len(),
        })).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

/// 单个元素 → drftx JSON。
fn element_to_drftx(el: &WbElement) -> Value {
    match el {
        WbElement::Text(t) => with_type(to_value(t).unwrap_or(Value::Null), "text"),
        WbElement::Image(i) => with_type(to_value(i).unwrap_or(Value::Null), "image"),
        WbElement::Shape(s) => {
            let mut v = to_value(s).unwrap_or(Value::Null);
            if let Value::Object(ref mut m) = v {
                m.insert("type".to_string(), json!("shape"));
                // 优先写入 SVG Path 指令（raw_path 或合成路径）。
                m.insert("path".to_string(), json!(shape_path(s)));
                m.insert(
                    "shape_type".to_string(),
                    json!(shape_type_name(&s.shape_type)),
                );
            }
            v
        }
        WbElement::Placeholder(p) => with_type(to_value(p).unwrap_or(Value::Null), "placeholder"),
    }
}

/// 在序列化对象上追加 `"type"` 字段。
fn with_type(mut v: Value, kind: &str) -> Value {
    if let Value::Object(ref mut m) = v {
        m.insert("type".to_string(), json!(kind));
    }
    v
}

/// 形状类型名（drftx 友好字符串）。
fn shape_type_name(t: &WbShapeType) -> &'static str {
    match t {
        WbShapeType::Rectangle => "rectangle",
        WbShapeType::Circle => "circle",
        WbShapeType::Ellipse => "ellipse",
        WbShapeType::Triangle => "triangle",
        WbShapeType::Line => "line",
        WbShapeType::Polygon => "polygon",
        WbShapeType::Path(_) => "path",
    }
}

/// 解析出用于 drftx 的 SVG Path 字符串。
///
/// - `raw_path` 非空 → 直接采用（优先级最高）。
/// - 否则按 `shape_type` 合成等效 SVG Path。
pub fn shape_path(s: &WbShape) -> String {
    if !s.raw_path.is_empty() {
        return s.raw_path.clone();
    }
    let (x, y, w, h) = (s.x, s.y, s.w, s.h);
    match &s.shape_type {
        WbShapeType::Rectangle | WbShapeType::Polygon => format!(
            "M {x} {y} L {} {y} L {} {} L {x} {} Z",
            x + w,
            x + w,
            y + h,
            y + h
        ),
        WbShapeType::Triangle => format!(
            "M {} {y} L {} {} L {x} {} Z",
            x + w / 2.0,
            x + w,
            y + h,
            y + h
        ),
        WbShapeType::Circle | WbShapeType::Ellipse => {
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            let rx = w / 2.0;
            let ry = h / 2.0;
            format!("M {cx} {cy} m -{rx} 0 a {rx} {ry} 0 1 0 {w} 0 a {rx} {ry} 0 1 0 -{w} 0 Z")
        }
        WbShapeType::Line => format!("M {x} {y} L {} {}", x + w, y + h),
        WbShapeType::Path(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enbx_model::{BoardXml, EnbxElement, EnbxParsed, Reference, ShapeXml, SlideXml};
    use crate::enbx_to_wb::convert;
    use crate::whiteboard::WbElement;
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    fn rectangle_synthesizes_path_when_raw_empty() {
        let parsed = EnbxParsed {
            board: BoardXml {
                name: "t".into(),
                width: 1920.0,
                height: 1080.0,
            },
            slides: vec![SlideXml {
                id: "t".into(),
                elements: vec![EnbxElement::Shape(ShapeXml {
                    geometry_type: "Rectangle".into(),
                    raw_path: String::new(),
                    x: 10.0,
                    y: 20.0,
                    w: 100.0,
                    h: 50.0,
                    fill: None,
                    stroke: None,
                    stroke_width: 0.0,
                    opacity: 1.0,
                })],
            }],
            thumbnails: HashMap::new(),
            reference: Reference::default(),
        };
        let doc = convert(&parsed, Path::new("Resources"));
        let shape = match &doc.pages[0].elements[0] {
            WbElement::Shape(s) => s,
            _ => unreachable!(),
        };
        let p = shape_path(shape);
        assert!(p.contains("M 10 20"), "synthesized rect path: {p}");
    }
}
