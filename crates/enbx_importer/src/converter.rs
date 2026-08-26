//! Slide → CoursewareDoc converter.
//!
//! Streams Slide_*.xml via quick-xml `Reader`, extracts text/shapes/images,
//! and maps everything to `CoursewareDoc` elements.

use std::collections::HashMap;
use std::io::Read;
use uuid::Uuid;

use crate::error::EnbxError;
use crate::parser;
use crate::ImportReport;
use drafftink_core::model::{
    BaseElement, CoursewareDoc, Element, ImageElement, PageContent, ShapeElement, ShapeType,
    SvgShapeElement, TextElement,
};

/// Progress callback: (current_step, total_steps, description)
pub type ProgressFn = Box<dyn Fn(usize, usize, &str)>;

/// Convert all slides to a CoursewareDoc.
pub fn convert_slides(
    archive: &mut zip::ZipArchive<impl Read + std::io::Seek>,
    slide_indices: &[usize],
    ref_map: &HashMap<String, String>,
    canvas_w: f32,
    canvas_h: f32,
    bg_color: [u8; 4],
    progress: Option<&ProgressFn>,
) -> Result<(CoursewareDoc, ImportReport), EnbxError> {
    let total = slide_indices.len();
    let mut pages: Vec<PageContent> = Vec::with_capacity(total);
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut resources = 0usize;

    for (step, &idx) in slide_indices.iter().enumerate() {
        if let Some(ref cb) = progress {
            cb(step, total, &format!("parsing Slide_{idx}"));
        }
        // Try multiple naming conventions
        let candidates = [
            format!("Slides/Slide_{idx}.xml"),
            format!("slides/slide_{idx}.xml"),
            format!("Slide/Slide_{idx}.xml"),
            format!("Slide_{idx}.xml"),
        ];
        let mut found = false;
        for c in &candidates {
            if let Ok(elems) = parse_slide_stream(archive, c, ref_map) {
                log::debug!(
                    "{}: {} elements ({} img {} shp {} txt)",
                    c,
                    elems.len(),
                    elems
                        .iter()
                        .filter(|e| matches!(e, Element::Image(_)))
                        .count(),
                    elems
                        .iter()
                        .filter(|e| matches!(e, Element::Shape(_)))
                        .count(),
                    elems
                        .iter()
                        .filter(|e| matches!(e, Element::Text(_)))
                        .count(),
                );
                resources += elems
                    .iter()
                    .filter(|e| matches!(e, Element::Image(_)))
                    .count();
                pages.push(PageContent {
                    elements: elems,
                    annotations_data: Vec::new(),
                    ..Default::default()
                });
                ok += 1;
                found = true;
                break;
            }
        }
        if !found {
            log::warn!("Slide {idx} not found (tried: {candidates:?})");
            pages.push(PageContent::default());
            failed += 1;
        }
    }

    if pages.is_empty() {
        pages.push(PageContent::default());
    }

    let first_elems = pages
        .first()
        .map(|p| p.elements.clone())
        .unwrap_or_default();
    let doc = CoursewareDoc {
        version: "2.0".into(),
        page_size: [canvas_w, canvas_h],
        background_color: bg_color,
        elements: first_elems,
        pages,
    };

    let report = ImportReport {
        pages_ok: ok,
        pages_failed: failed,
        resources_extracted: resources,
        warnings: Vec::new(),
        title: None,
    };

    Ok((doc, report))
}

// ---------------------------------------------------------------------------
// Streaming slide parser
// ---------------------------------------------------------------------------

fn parse_slide_stream(
    archive: &mut zip::ZipArchive<impl Read + std::io::Seek>,
    entry: &str,
    ref_map: &HashMap<String, String>,
) -> Result<Vec<Element>, EnbxError> {
    let f = archive
        .by_name(entry)
        .map_err(|e| EnbxError::SlideError(format!("not found: {e}")))?;
    let size = f.size() as usize;

    // Read the slide XML into a buffer — slides are typically < 2 MB each
    let mut xml = String::with_capacity(size.min(2_097_152));
    let mut reader = f;
    reader.read_to_string(&mut xml)?;
    drop(reader);

    let head: String = xml.chars().take(500).collect();
    log::debug!("Slide XML head: {head}");

    // Detect format: OOXML (<p:sp>) or native ENBX (<Slide><Elements>)
    let elements = if xml.contains("<Elements>") {
        parse_enbx_native(&xml, ref_map)
    } else {
        // Legacy OOXML fallback (stripped namespaces)
        let xml = strip_ns_prefixes(&xml);
        parse_ooxml(&xml, ref_map)
    };

    log::debug!("{entry}");
    for e in &elements {
        match e {
            Element::Text(t) => log::debug!(
                "  TEXT  ({}pt) @ ({:.0},{:.0}): {:?}",
                t.font_size,
                t.base.position[0],
                t.base.position[1],
                &t.text[..t.text.len().min(80)]
            ),
            Element::Shape(s) => log::debug!(
                "  SHAPE {:?} @ ({:.0},{:.0}) {}x{}",
                s.shape_type,
                s.base.position[0],
                s.base.position[1],
                s.base.size[0],
                s.base.size[1]
            ),
            Element::Image(i) => log::debug!(
                "  IMAGE {} @ ({:.0},{:.0}) {}x{}",
                i.image_path,
                i.base.position[0],
                i.base.position[1],
                i.base.size[0],
                i.base.size[1]
            ),
            _ => {}
        }
    }

    Ok(elements)
}

// ===========================================================================
// ENBX native format: <Slide><Elements><Text>...</Text></Elements></Slide>
// ===========================================================================

fn parse_enbx_native(xml: &str, ref_map: &HashMap<String, String>) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut z = 0i32;

    let elements_start = match xml.find("<Elements>") {
        Some(p) => p + 10,
        None => return elements,
    };
    let elements_end = match xml[elements_start..].find("</Elements>") {
        Some(p) => elements_start + p,
        None => return elements,
    };
    let block = &xml[elements_start..elements_end];

    // Split by top-level element tags: <Text ...>, <Image ...>, <Group ...>
    let mut rest = block;
    let mut iterations = 0u32;
    while let Some(pos) = rest.find('<') {
        iterations += 1;
        if iterations > 10000 {
            break;
        } // safety valve

        rest = &rest[pos..];
        let gt = rest.find('>').unwrap_or(rest.len());
        let tag_line = &rest[..=gt];

        // Skip closing tags
        if tag_line.starts_with("</") {
            rest = &rest[gt + 1..];
            continue;
        }

        // Identify element type
        let tag_name = name_between(tag_line, "<", " ")
            .or_else(|| name_between(tag_line, "<", ">"))
            .or_else(|| name_between(tag_line, "<", "/>"))
            .unwrap_or("");
        let tag_name = tag_name.trim();

        match tag_name {
            "Text" => {
                let close = find_close_robust(rest, "Text");
                let block = &rest[..close];
                if let Ok(Some(e)) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    parse_enbx_text(block, z)
                })) {
                    z += 1;
                    elements.push(e);
                }
                if close < rest.len() {
                    rest = &rest[close..];
                } else {
                    rest = &rest[rest.len()..];
                }
            }
            "Image" | "Picture" => {
                let close = find_close_robust(rest, tag_name);
                let block = &rest[..close];
                if let Ok(Some(e)) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    parse_enbx_image(block, ref_map, z)
                })) {
                    z += 1;
                    elements.push(e);
                }
                if close < rest.len() {
                    rest = &rest[close..];
                } else {
                    rest = &rest[rest.len()..];
                }
            }
            "Shape" => {
                let close = find_close_robust(rest, "Shape");
                let block = &rest[..close];
                if let Ok(Some(e)) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    parse_enbx_shape(block, z)
                })) {
                    z += 1;
                    elements.push(e);
                }
                if close < rest.len() {
                    rest = &rest[close..];
                } else {
                    rest = &rest[rest.len()..];
                }
            }
            _ => {
                // Unknown element — skip to its closing tag
                let close = find_close_robust(rest, tag_name);
                if close > gt {
                    rest = &rest[close..];
                } else {
                    rest = &rest[gt + 1..];
                }
            }
        }
    }

    elements
}

/// Find close tag with depth tracking (handles nested elements)
fn find_close_robust(xml: &str, tag: &str) -> usize {
    let open_pat = format!("<{tag}");
    let close_pat = format!("</{tag}>");
    let mut depth: i32 = 0;
    let mut pos = 0usize;
    let bytes = xml.as_bytes();

    while pos < bytes.len() {
        // Check for opening tag
        if bytes[pos..].starts_with(open_pat.as_bytes()) {
            let after = pos + open_pat.len();
            if after < bytes.len()
                && matches!(bytes[after], b'>' | b' ' | b'\n' | b'\r' | b'\t' | b'/')
            {
                depth += 1;
                // Find '>' to skip past the tag
                if after < bytes.len()
                    && bytes[after] == b'/'
                    && after + 1 < bytes.len()
                    && bytes[after + 1] == b'>'
                {
                    depth -= 1; // self-closing
                    if depth == 0 {
                        return pos + open_pat.len() + 2;
                    }
                    pos = after + 2;
                    continue;
                }
                if let Some(e) = xml[pos..].find('>') {
                    pos += e + 1;
                } else {
                    pos += 1;
                }
                continue;
            }
        }
        // Check for closing tag
        if bytes[pos..].starts_with(close_pat.as_bytes()) {
            depth -= 1;
            if depth <= 0 {
                return pos + close_pat.len();
            }
            pos += close_pat.len();
            continue;
        }
        pos += 1;
    }
    xml.len()
}

fn parse_enbx_text(xml: &str, z_order: i32) -> Option<Element> {
    // Position: <X>, <Y>, <Width>, <Height> as children of <Text>
    let x = xml_val(xml, "X")
        .or_else(|| xml_val(xml, "Left"))
        .or_else(|| attr_val(xml, "Left"))
        .unwrap_or(0.0);
    let y = xml_val(xml, "Y")
        .or_else(|| xml_val(xml, "Top"))
        .or_else(|| attr_val(xml, "Top"))
        .unwrap_or(0.0);
    let w = xml_val(xml, "Width")
        .or_else(|| attr_val(xml, "Width"))
        .unwrap_or(200.0);
    let h = xml_val(xml, "Height")
        .or_else(|| attr_val(xml, "Height"))
        .unwrap_or(60.0);

    // Font size: <TextRun><FontSize>40</FontSize>
    let font_size = xml_val(xml, "FontSize")
        .or_else(|| xml_val(xml, "FontSizeInPt"))
        .unwrap_or(18.0);

    // Color: search for <Foreground> block, extract its <ColorBrush>
    let color_hex = {
        let mut result = None;
        let mut rest = xml;
        while let Some(pos) = rest.find("<Foreground>") {
            rest = &rest[pos + 12..];
            if let Some(end) = rest.find("</Foreground>") {
                let fg_block = &rest[..end];
                if let Some(c) = xml_str(fg_block, "ColorBrush") {
                    result = Some(c);
                    break;
                }
                rest = &rest[end..];
            } else {
                break;
            }
        }
        result
    }
    .or_else(|| xml_str(xml, "ColorBrush"))
    .or_else(|| xml_str(xml, "TextColor"))
    .unwrap_or_else(|| "#FF000000".to_string());
    let fill = parse_enbx_color(&color_hex);
    log::debug!(
        "text color: hex={:?} raw=[{},{},{},{}]",
        color_hex,
        fill[0],
        fill[1],
        fill[2],
        fill[3]
    );

    // Font family
    let font_family = xml_str(xml, "Source").unwrap_or_else(|| "Microsoft YaHei".to_string());

    // Text content: priority order:
    // 1. <RichText><Text>HI,SEEWO</Text></RichText>  (simple short form)
    // 2. <TextRun><Text>...</Text></TextRun>          (rich form)
    // 3. <Text>...</Text> (anywhere)
    let mut text = String::new();
    if let Some(t) = xml_str(xml, "Text") {
        text = t;
    }
    if text.is_empty() {
        let mut r = xml;
        while let Some(p) = r.find("<TextRun>") {
            r = &r[p..];
            let close = find_close_robust(r, "TextRun");
            if close == 0 || close > r.len() {
                break;
            }
            let block = &r[..close];
            if let Some(t) = xml_str(block, "Text") {
                text.push_str(&t);
            }
            if close >= r.len() {
                break;
            }
            r = &r[close..];
        }
    }

    if text.is_empty() {
        return None;
    }

    Some(Element::Text(TextElement {
        base: BaseElement {
            id: Uuid::new_v4(),
            position: [x, y],
            size: [w, h.max(font_size * 2.0)],
            rotation: 0.0,
            z_order,
            fill_color: rgba(fill),
            stroke_color: rgba([0, 0, 0, 0]),
            stroke_width: 0.0,
            opacity: 1.0,
            locked: false,
            visible: true,
            name: String::new(),
        },
        text,
        font_size,
        font_family,
    }))
}

fn parse_enbx_image(
    xml: &str,
    _ref_map: &HashMap<String, String>,
    z_order: i32,
) -> Option<Element> {
    let x = xml_val(xml, "X")
        .or_else(|| xml_val(xml, "Left"))
        .or_else(|| attr_val(xml, "Left"))
        .unwrap_or(0.0);
    let y = xml_val(xml, "Y")
        .or_else(|| xml_val(xml, "Top"))
        .or_else(|| attr_val(xml, "Top"))
        .unwrap_or(0.0);
    let w = xml_val(xml, "Width")
        .or_else(|| attr_val(xml, "Width"))
        .unwrap_or(100.0);
    let h = xml_val(xml, "Height")
        .or_else(|| attr_val(xml, "Height"))
        .unwrap_or(100.0);

    // <Source>path/to/image.png</Source> or <FileName>img.png</FileName>
    let path = xml_str(xml, "Source")
        .or_else(|| xml_str(xml, "FileName"))
        .or_else(|| xml_str(xml, "Src"))
        .or_else(|| xml_str(xml, "Path"))
        .unwrap_or_default();
    if path.is_empty() {
        return None;
    }

    Some(Element::Image(ImageElement {
        base: BaseElement {
            id: Uuid::new_v4(),
            position: [x, y],
            size: [w, h],
            rotation: 0.0,
            z_order,
            fill_color: rgba([255, 255, 255, 255]),
            stroke_color: rgba([0, 0, 0, 0]),
            stroke_width: 0.0,
            opacity: 1.0,
            locked: false,
            visible: true,
            name: String::new(),
        },
        image_path: path,
        image_data: None,
        keep_aspect: true,
    }))
}

// ---------------------------------------------------------------------------
// ENBX native Shape parser
// ---------------------------------------------------------------------------

/// Parse a `<Shape>...</Shape>` block from ENBX native format.
///
/// Two modes:
/// 1. **Preset geometry** (`<PresetGeometry><GeometryType>Bracket</GeometryType>`)
///    → `Element::Shape(ShapeElement)` with `shape_type` + `scale_y` from Adjusts.
/// 2. **Custom geometry** (`<CustomGeometry><GeometryType>FreeLine</GeometryType>`
///    with non-empty `<Path>`) → `Element::SvgShape(SvgShapeElement)` with SVG path data.
fn parse_enbx_shape(xml: &str, z_order: i32) -> Option<Element> {
    // --- Position & size ---
    // Use a tolerant extractor: Seewo nests `<Width>Small</Width>` inside
    // `<Line><TailEnd>`, whose non-numeric content would otherwise shadow the
    // geometry width and collapse arrowed shapes to the 100px default.
    let x = xml_val_skip_bad(xml, "X").unwrap_or(0.0);
    let y = xml_val_skip_bad(xml, "Y").unwrap_or(0.0);
    let w = xml_val_skip_bad(xml, "Width").unwrap_or(100.0);
    let h = xml_val_skip_bad(xml, "Height").unwrap_or(100.0);

    // --- Colors ---
    // Background > ColorBrush → fill  (default transparent)
    let fill_hex =
        extract_color_brush(xml, "Background").unwrap_or_else(|| "#00000000".to_string());

    // Foreground > ColorBrush → stroke (default black)
    let stroke_hex =
        extract_color_brush(xml, "Foreground").unwrap_or_else(|| "#FF000000".to_string());

    let fill = parse_enbx_color(&fill_hex);
    let stroke = parse_enbx_color(&stroke_hex);

    // --- Stroke width ---
    let stroke_width = xml_val(xml, "Thickness").unwrap_or(2.0);

    // --- Opacity ---
    let opacity = xml_val(xml, "Opacity").unwrap_or(1.0);

    // --- Geometry type ---
    let geometry_type = xml_str(xml, "GeometryType").unwrap_or_else(|| "Rectangle".to_string());

    // --- Path data (only for CustomGeometry / FreeLine) ---
    let path_data = xml_str(xml, "Path").filter(|s| !s.is_empty() && s.len() > 3);

    // --- Arrows ---
    let has_start_arrow = xml
        .find("<HeadEnd>")
        .map(|p| xml[p..].contains(">Arrow<"))
        .unwrap_or(false);
    let has_end_arrow = xml
        .find("<TailEnd>")
        .map(|p| xml[p..].contains(">Arrow<"))
        .unwrap_or(false);

    // --- Adjusts → scale_y for Brace / Bracket ---
    let mut scale_y = 0.0f32;
    if let Some(adj_start) = xml.find("<Adjust>") {
        let adj_end = xml.len().min(adj_start + 200);
        let adj_block = &xml[adj_start..adj_end];
        if let Some(sy) = xml_str(adj_block, "ScaleY") {
            if let Ok(v) = sy.parse::<f32>() {
                scale_y = v;
            }
        }
    }

    // --- Build base ---
    let base = BaseElement {
        id: Uuid::new_v4(),
        position: [x, y],
        size: [w, h],
        rotation: xml_val(xml, "Rotation").unwrap_or(0.0),
        z_order,
        fill_color: rgba(fill),
        stroke_color: rgba(stroke),
        stroke_width,
        opacity,
        locked: xml_str(xml, "IsLocked")
            .map(|s| s == "True")
            .unwrap_or(false),
        visible: true,
        name: format!("{geometry_type}_shape"),
    };

    // --- Dispatch: path data → SvgShape; preset → Shape ---
    if let Some(ref path) = path_data {
        // FreeLine / custom path → render as SVG shape
        log::debug!("Shape has path data ({} chars), using SvgShape", path.len());
        Some(Element::SvgShape(SvgShapeElement {
            base,
            svg_path: path.clone(),
            is_closed: false, // FreeLine is open by default
            has_start_arrow,
            has_end_arrow,
        }))
    } else {
        // Preset geometry → ShapeElement with type mapping
        let shape_type = match geometry_type.as_str() {
            "Bracket" => ShapeType::Bracket,
            "Brace" => ShapeType::Brace,
            "Fan" => ShapeType::Fan,
            "LineArrowEnd" | "LineArrowStart" | "LineArrowStartEnd" => ShapeType::Arrow,
            "Line" => ShapeType::Line,
            "Arrow" => ShapeType::Arrow,
            "Ellipse" | "Oval" => ShapeType::Ellipse,
            _ => ShapeType::Rectangle, // default fallback
        };
        log::debug!("Shape preset: {shape_type:?} scale_y={scale_y}");
        Some(Element::Shape(ShapeElement {
            base,
            shape_type,
            has_start_arrow,
            has_end_arrow,
            scale_y,
        }))
    }
}

// ===========================================================================
// Diagnostic test: parse real Seewo slide XML and report element kinds.
// Run: cargo test -p enbx_importer diag_shape_parsing -- --nocapture
// ===========================================================================
// 该测试模块位于文件中部（其后仍有生产代码），clippy 对此告警属正常结构，
// 显式放行（不移动模块以免破坏并行协作者的 diff）。
#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod diagnostic {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    fn diag_shape_parsing() {
        let dir = Path::new(r"C:\en5\形状\Slides");
        if !dir.exists() {
            eprintln!("[diag] {dir:?} not found, skipping");
            return;
        }
        for name in ["Slide_0.xml", "Slide_2.xml", "Slide_3.xml", "Slide_4.xml"] {
            let p = dir.join(name);
            let Ok(xml) = std::fs::read_to_string(&p) else {
                continue;
            };
            let elems = parse_enbx_native(&xml, &HashMap::new());
            eprintln!("=== {name}: {} element(s) ===", elems.len());
            for e in &elems {
                match e {
                    Element::Shape(s) => eprintln!(
                        "  SHAPE {:?} @ ({:.1},{:.1}) {}x{} fill_a={} stroke_a={} sw={:.1} sy={:.2}",
                        s.shape_type,
                        s.base.position[0],
                        s.base.position[1],
                        s.base.size[0],
                        s.base.size[1],
                        s.base.fill_color.a(),
                        s.base.stroke_color.a(),
                        s.base.stroke_width,
                        s.scale_y,
                    ),
                    Element::SvgShape(s) => eprintln!(
                        "  SVGSIZE path({} chars) @ ({:.1},{:.1}) {}x{} stroke_a={}",
                        s.svg_path.len(),
                        s.base.position[0],
                        s.base.position[1],
                        s.base.size[0],
                        s.base.size[1],
                        s.base.stroke_color.a(),
                    ),
                    _other => eprintln!("  OTHER variant"),
                }
            }
        }
    }
}

// ===========================================================================
// Legacy OOXML parser (fallback)
// ===========================================================================

fn parse_ooxml(xml: &str, ref_map: &HashMap<String, String>) -> Vec<Element> {
    let mut elements: Vec<Element> = Vec::new();
    let mut z = 0i32;
    elements.extend(extract_shapes(xml, &mut z));
    elements.extend(extract_images(xml, ref_map, &mut z));
    elements.extend(extract_text(xml, &mut z));
    elements
}

// ===========================================================================
// Helpers
// ===========================================================================

/// Like [`xml_val`] but returns the first occurrence whose content actually
/// parses as an `f32`.  Seewo shapes nest a non-numeric `<Width>Small</Width>`
/// inside `<Line><TailEnd>`; a naive first-match extractor would pick that up
/// and then fail to parse it, collapsing arrowed shapes to the default size.
/// Scanning for the first *numeric* match avoids the shadowing entirely.
fn xml_val_skip_bad(xml: &str, tag: &str) -> Option<f32> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut search = 0usize;
    while let Some(p) = xml[search..].find(&open) {
        let abs = p + search;
        let after = abs + open.len();
        match xml[after..].find(&close) {
            Some(end) => {
                let val = xml[after..after + end].trim();
                if let Ok(v) = val.parse::<f32>() {
                    return Some(v);
                }
                // Non-numeric content (e.g. "Small") — keep scanning past it.
                search = after;
            }
            None => break,
        }
    }
    None
}

/// Extract a numeric value: <Name>123.4</Name>
pub(crate) fn xml_val(xml: &str, tag: &str) -> Option<f32> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let p = xml.find(&open)?;
    let rest = &xml[p + open.len()..];
    let end = rest.find(&close)?;
    rest[..end].trim().parse::<f32>().ok()
}

/// Extract a string value: <Name>text here</Name>.
/// Skips container elements (where content starts with '<').
pub(crate) fn xml_str(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut search_from = 0;
    while let Some(p) = xml[search_from..].find(&open) {
        let abs = p + search_from;
        let after = abs + open.len();
        // Skip if the content looks like XML (container element)
        if let Some(&b) = xml.as_bytes().get(after) {
            if b == b'<' || b == b'\r' || b == b'\n' {
                search_from = abs + 1;
                continue;
            }
        }
        let rest = &xml[after..];
        if let Some(end) = rest.find(&close) {
            let s = rest[..end].trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
        search_from = abs + 1;
    }
    None
}

/// Extract an attribute value: attr="value" or attr='value'
pub(crate) fn attr_val(xml: &str, attr: &str) -> Option<f32> {
    for q in b"\"'" {
        let pat = format!("{attr}={}", *q as char);
        let p = xml.find(&pat)?;
        let rest = &xml[p + pat.len()..];
        let end = rest.find(*q as char)?;
        if let Ok(v) = rest[..end].trim().parse::<f32>() {
            return Some(v);
        }
    }
    None
}

/// Extract tag name between two delimiters
fn name_between<'a>(s: &'a str, left: &str, right: &str) -> Option<&'a str> {
    let p = s.find(left)?;
    let rest = &s[p + left.len()..];
    let end = rest.find(right)?;
    Some(&rest[..end])
}

/// Parse #AARRGGBB or #RRGGBB hex color
fn parse_enbx_color(s: &str) -> [u8; 4] {
    let s = s.trim().trim_start_matches('#');
    match s.len() {
        8 => {
            let v = u32::from_str_radix(s, 16).unwrap_or(0xFF000000);
            [
                ((v >> 16) as u8),
                ((v >> 8) as u8),
                (v as u8),
                ((v >> 24) as u8),
            ]
        }
        6 => {
            let v = u32::from_str_radix(s, 16).unwrap_or(0x000000);
            [((v >> 16) as u8), ((v >> 8) as u8), (v as u8), 255]
        }
        _ => [0, 0, 0, 255],
    }
}

/// Extract <Container><ColorBrush>#AARRGGBB</ColorBrush></Container> hex value.
fn extract_color_brush(xml: &str, container: &str) -> Option<String> {
    let open_tag = format!("<{container}>");
    let p = xml.find(&open_tag)?;
    let inner = &xml[p + open_tag.len()..];
    let cb_start = inner.find("<ColorBrush>")?;
    let after_cb = &inner[cb_start + "<ColorBrush>".len()..];
    let cb_end = after_cb.find("</ColorBrush>")?;
    Some(after_cb[..cb_end].trim().to_string())
}

/// Remove XML tags from text: `<a>b</a>` → `b`
#[allow(dead_code)]
fn strip_xml_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

// ---------------------------------------------------------------------------
// Shape extraction
// ---------------------------------------------------------------------------

fn extract_shapes(xml: &str, z: &mut i32) -> Vec<Element> {
    let mut out = Vec::new();
    let mut rest = xml;

    // Find <sp> blocks (namespace prefix already stripped)
    while let Some(pos) = find_tag_start(rest, "sp") {
        rest = &rest[pos..];
        let end = find_element_end(rest, "sp");
        let block = &rest[..end];
        if let Some(e) = parse_shape(block, *z) {
            *z += 1;
            out.push(e);
        }
        rest = &rest[1..];
    }
    out
}

fn parse_shape(block: &str, z_order: i32) -> Option<Element> {
    let shape_type = if block.contains("rect") {
        ShapeType::Rectangle
    } else if block.contains("ellipse") {
        ShapeType::Ellipse
    } else if block.contains("line") {
        ShapeType::Line
    } else if block.contains("arrow") {
        ShapeType::Arrow
    } else {
        return None; // not a supported shape
    };

    let [x, y, w, h] = extract_rect_emu(block);

    let fill = extract_attr_val(block, "fillClr")
        .or_else(|| extract_attr_val(block, "srgbClr"))
        .or_else(|| extract_attr_val(block, "fill"))
        .map(|s| parser::parse_hex(&s))
        .unwrap_or([0xE0, 0xE0, 0xE0, 255]);

    let stroke = extract_attr_val(block, "lnClr")
        .or_else(|| extract_attr_val(block, "stroke"))
        .map(|s| parser::parse_hex(&s))
        .unwrap_or([0x40, 0x40, 0x40, 255]);

    let sw = extract_attr_val(block, "strokeWidth")
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(1.5);

    Some(Element::Shape(ShapeElement {
        base: BaseElement {
            id: Uuid::new_v4(),
            position: [x, y],
            size: [w, h],
            rotation: 0.0,
            z_order,
            fill_color: rgba(fill),
            stroke_color: rgba(stroke),
            stroke_width: sw,
            opacity: 1.0,
            locked: false,
            visible: true,
            name: String::new(),
        },
        shape_type,
        has_start_arrow: false,
        has_end_arrow: false,
        scale_y: 0.0,
    }))
}

// ---------------------------------------------------------------------------
// Image extraction
// ---------------------------------------------------------------------------

fn extract_images(xml: &str, ref_map: &HashMap<String, String>, z: &mut i32) -> Vec<Element> {
    let mut out = Vec::new();
    let mut rest = xml;

    while let Some(pos) = find_tag_start(rest, "pic") {
        rest = &rest[pos..];
        let end = find_element_end(rest, "pic");
        let block = &rest[..end];

        let r_id = extract_attr_val(block, "r:embed")
            .or_else(|| extract_attr_val(block, "r:id"))
            .or_else(|| extract_attr_val(block, "embed"));

        // Resolve image path: try Reference map first, then direct src/file
        let image_path = r_id
            .as_ref()
            .and_then(|rid| ref_map.get(rid).cloned())
            .or_else(|| extract_attr_val(block, "src"))
            .or_else(|| extract_attr_val(block, "file"))
            .or_else(|| r_id.clone());

        if let Some(path) = image_path {
            let [x, y, w, h] = extract_rect_emu(block);

            out.push(Element::Image(ImageElement {
                base: BaseElement {
                    id: Uuid::new_v4(),
                    position: [x, y],
                    size: [w, h],
                    rotation: 0.0,
                    z_order: *z,
                    fill_color: rgba([255, 255, 255, 255]),
                    stroke_color: rgba([0, 0, 0, 0]),
                    stroke_width: 0.0,
                    opacity: 1.0,
                    locked: false,
                    visible: true,
                    name: String::new(),
                },
                image_path: path,
                image_data: None,
                keep_aspect: true,
            }));
            *z += 1;
        }
        rest = &rest[1..];
    }
    out
}

// ---------------------------------------------------------------------------
// Text extraction
// ---------------------------------------------------------------------------

fn extract_text(xml: &str, z: &mut i32) -> Vec<Element> {
    let mut out = Vec::new();
    let mut rest = xml;

    while let Some(pos) = find_tag_start(rest, "txBody") {
        rest = &rest[pos..];
        let body_end = find_element_end(rest, "txBody");
        let body = &rest[..body_end];

        // Parent <sp> for position
        let parent_start = find_tag_start(xml, "sp").unwrap_or(0);
        let parent = &xml[parent_start..];
        let [px, py, pw, ph] = extract_rect_emu(parent);

        // For text, the font size is in hundredths of a point
        let font_size = extract_attr_val(body, "sz")
            .and_then(|s| s.parse::<f32>().ok())
            .map(|emu100| emu100 / 100.0)
            .unwrap_or(18.0);

        let text_color = extract_attr_val(body, "srgbClr")
            .or_else(|| extract_attr_val(parent, "srgbClr"))
            .map(|s| parser::parse_hex(&s))
            .unwrap_or([0x1E, 0x1E, 0x1E, 255]);

        let content = collect_text_runs(body);

        if !content.is_empty() {
            let base = BaseElement {
                id: Uuid::new_v4(),
                position: [px, py],
                size: [pw, ph.max(font_size * 2.0)],
                rotation: 0.0,
                z_order: *z,
                fill_color: rgba(text_color),
                stroke_color: rgba([0, 0, 0, 0]),
                stroke_width: 0.0,
                opacity: 1.0,
                locked: false,
                visible: true,
                name: String::new(),
            };
            out.push(Element::Text(TextElement {
                base,
                text: content,
                font_size,
                font_family: "Microsoft YaHei".into(),
            }));
            *z += 1;
        }
        rest = &rest[1..];
    }
    out
}

fn collect_text_runs(xml: &str) -> String {
    let mut text = String::new();
    let mut rest = xml;

    // <a:t>...</a:t>
    while let Some(pos) = rest.find("<a:t>") {
        rest = &rest[pos + 5..];
        if let Some(end) = rest.find("</a:t>") {
            text.push_str(&rest[..end]);
            rest = &rest[end..];
        } else {
            break;
        }
    }
    // <a:t ...>...</a:t>
    if text.is_empty() {
        rest = xml;
        while let Some(pos) = rest.find("<a:t ") {
            rest = &rest[pos..];
            if let Some(gt) = rest.find('>') {
                rest = &rest[gt + 1..];
                if let Some(end) = rest.find("</a:t>") {
                    text.push_str(&rest[..end]);
                    rest = &rest[end..];
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
    // <t> plain text
    if text.is_empty() {
        rest = xml;
        while let Some(pos) = rest.find("<t>") {
            rest = &rest[pos + 3..];
            if let Some(end) = rest.find("</t>") {
                text.push_str(&rest[..end]);
                rest = &rest[end..];
            } else {
                break;
            }
        }
    }

    text
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Find the end index of an element including its closing tag.
fn find_element_end(xml: &str, tag: &str) -> usize {
    let close = format!("</{tag}>");
    if let Some(p) = xml.find(&close) {
        return p + close.len();
    }
    // Self-closing
    if let Some(p) = xml.find("/>") {
        return p + 2;
    }
    xml.len()
}

/// Find the byte offset where `<tag` starts (matched against opening `<tag`,
/// `<tag>`, or `<tag ...>`).  Returns the offset, or None.
fn find_tag_start(xml: &str, tag: &str) -> Option<usize> {
    let open = format!("<{tag}");
    let mut search_from = 0;
    while let Some(pos) = xml[search_from..].find(&open) {
        let abs = search_from + pos;
        let after = abs + open.len();
        let bytes = xml.as_bytes();
        // Match must end with `>`, ` `, `\n`, `\r`, `\t`, or `/`
        if let Some(&b) = bytes.get(after) {
            if matches!(b, b'>' | b' ' | b'\n' | b'\r' | b'\t' | b'/') {
                return Some(abs);
            }
        }
        search_from = abs + 1;
    }
    None
}

/// Extract `[x, y, cx, cy]` in EMU, converted to pixels.
fn extract_rect_emu(xml: &str) -> [f32; 4] {
    [
        extract_attr_val(xml, "x")
            .and_then(|s| s.parse::<f64>().ok())
            .map(parser::emu_to_px)
            .unwrap_or(0.0),
        extract_attr_val(xml, "y")
            .and_then(|s| s.parse::<f64>().ok())
            .map(parser::emu_to_px)
            .unwrap_or(0.0),
        extract_attr_val(xml, "cx")
            .and_then(|s| s.parse::<f64>().ok())
            .map(parser::emu_to_px)
            .unwrap_or(100.0),
        extract_attr_val(xml, "cy")
            .and_then(|s| s.parse::<f64>().ok())
            .map(parser::emu_to_px)
            .unwrap_or(100.0),
    ]
}

fn extract_attr_val(xml: &str, attr: &str) -> Option<String> {
    let q = format!("{attr}=\"");
    if let Some(p) = xml.find(&q) {
        let rest = &xml[p + q.len()..];
        if let Some(end) = rest.find('"') {
            let val = rest[..end].trim();
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    // Single-quote variant
    let q2 = format!("{attr}='");
    if let Some(p) = xml.find(&q2) {
        let rest = &xml[p + q2.len()..];
        if let Some(end) = rest.find('\'') {
            let val = rest[..end].trim();
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

fn rgba(c: [u8; 4]) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// Strip XML namespace prefixes: `<p:sp>` → `<sp>`, `<a:t>` → `<t>`
/// Also handles `</p:sp>`, and attribute `p:attr` → `attr`.
fn strip_ns_prefixes(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let bytes = xml.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            out.push('<');
            i += 1;
            // Eat any prefix before the tag name
            if i < bytes.len() && (bytes[i] == b'/' || bytes[i] == b'?') {
                out.push(bytes[i] as char);
                i += 1;
            }
            // Skip "ns:" prefix
            while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                if i + 1 < bytes.len() && bytes[i + 1] == b':' {
                    i += 2; // skip prefix+colon
                    break;
                }
                out.push(bytes[i] as char);
                i += 1;
            }
            // Copy rest of tag
            while i < bytes.len() && bytes[i] != b'>' {
                out.push(bytes[i] as char);
                i += 1;
            }
            if i < bytes.len() {
                out.push('>');
                i += 1;
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}
