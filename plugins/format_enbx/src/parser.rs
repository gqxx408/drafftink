//! Slide XML parser — extracts TextElement and ShapeElementData lists from Seewo Slide XML.
//! Also provides Board/Document metadata parsing.

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::elements::{
    text::{ArgbColor, TextElement},
    shape::{GeometryKind, ShapeElementData, SlideElement},
};

/// Parse a Slide XML string into a list of slide elements (Text + Shape).
///
/// Handles Seewo's XML namespace by using `local_name()` on each tag.
#[allow(unused_assignments, unused_variables)]
pub fn parse_slide_xml(xml: &str) -> Result<Vec<SlideElement>, String> {
    log::info!("[parser] parse_slide_xml start, {} bytes", xml.len());
    let mut reader = Reader::from_str(xml);
    // Do NOT use trim_text(true): long SVG Paths may be split across multiple Text events,
    // and trimming boundaries would glue numbers together (e.g. "100 " + " 200" → "100200").
    // Each field handler trims its own value where appropriate.
    reader.trim_text(false);
    reader.expand_empty_elements(true);

    let mut elements: Vec<SlideElement> = Vec::new();

    // ── Text parsing state ───────────────────────────────────────
    let mut text_elem: Option<TextElement> = None;
    let mut in_element_wrapper = false; // <Element> wrapper (texts)
    let mut in_text_elem = false;        // <Text> directly under <Elements>
    let mut in_rich_text = false;
    let mut in_text_run = false;
    let mut in_background = false;
    let mut in_foreground = false;
    let mut text_content_captured = false;
    let mut text_depth: u32 = 0;
    let mut in_font_size = false;
    let mut in_font_weight = false;
    let mut in_font_family = false;
    let mut in_source = false;

    // ── Shape parsing state ──────────────────────────────────────
    let mut shape: Option<ShapeElementData> = None;
    let mut in_shape = false;
    let mut in_geometry = false;
    let mut in_preset_geometry = false;
    let mut in_custom_geometry = false;
    let mut in_adjusts = false;
    let mut in_adjust = false;
    let mut in_line = false;
    let mut in_head_end = false;
    let mut in_tail_end = false;
    let mut in_geometry_type = false;
    let mut in_scale_y = false;
    let mut in_end_type = false;
    // Common shape/text shared fields
    let mut in_x = false;
    let mut in_y = false;
    let mut in_width = false;
    let mut in_height = false;
    let mut in_rotation = false;
    let mut in_is_locked = false;
    let mut in_id = false;
    let mut in_thickness = false;
    let mut in_color_brush = false;
    let mut color_brush_target: Option<&'static str> = None; // "shape-stroke" | "shape-fill" | "text-fg" | "text-bg"

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                // Convert tag to owned bytes to avoid borrowing conflicts when
                // we call reader.read_event_into() inside the <Path> inner loop.
                let tag_owned: Vec<u8> = e.local_name().as_ref().to_vec();
                match tag_owned.as_slice() {
                    // ── Element wrapper (texts) ──────────────────────
                    b"Element" if !in_shape => {
                        in_element_wrapper = true;
                        text_elem = Some(TextElement::default());
                    }
                    b"Text" if !in_shape => {
                        text_depth += 1;
                        if text_depth == 1 && !in_rich_text {
                            in_text_elem = true;
                            if text_elem.is_none() {
                                text_elem = Some(TextElement::default());
                            }
                        }
                    }
                    b"RichText" if (in_text_elem || in_element_wrapper) && !in_shape => {
                        in_rich_text = true;
                    }
                    b"TextRun" if in_rich_text && !in_shape => {
                        in_text_run = true;
                    }

                    // ── Shape ────────────────────────────────────────
                    b"Shape" => {
                        in_shape = true;
                        shape = Some(ShapeElementData::default());
                        // Reset ALL shape sub-state to avoid leakage from previous elements
                        in_geometry = false;
                        in_preset_geometry = false;
                        in_custom_geometry = false;
                        in_adjusts = false;
                        in_adjust = false;
                        in_line = false;
                        in_head_end = false;
                        in_tail_end = false;
                        in_geometry_type = false;
                        in_scale_y = false;
                        in_end_type = false;
                        in_x = false;
                        in_y = false;
                        in_width = false;
                        in_height = false;
                        in_rotation = false;
                        in_is_locked = false;
                        in_id = false;
                        in_thickness = false;
                        in_background = false;
                        in_foreground = false;
                        in_color_brush = false;
                        color_brush_target = None;
                    }
                    b"Geometry" if in_shape => { in_geometry = true; }
                    b"PresetGeometry" if in_shape => { in_preset_geometry = true; }
                    b"CustomGeometry" if in_shape => { in_custom_geometry = true; }
                    b"Adjusts" if in_shape => { in_adjusts = true; }
                    b"Adjust" if in_shape && in_adjusts => { in_adjust = true; }
                    b"Line" if in_shape => { in_line = true; }
                    b"HeadEnd" if in_shape && in_line => { in_head_end = true; }
                    b"TailEnd" if in_shape && in_line => { in_tail_end = true; }
                    b"Path" if in_shape => {
                        // <Path> holds SVG path data for FreeLine/Fan/Ellipse/Circle.
                        // Long paths may be split across multiple Text events by quick_xml.
                        // Use an inner loop with read_event_into to read ALL text until </Path>.
                        let mut svg_path = String::new();
                        let mut inner_buf = Vec::new();
                        loop {
                            inner_buf.clear();
                            match reader.read_event_into(&mut inner_buf) {
                                Ok(Event::Text(e)) => {
                                    let txt = e.unescape().unwrap_or_default();
                                    svg_path.push_str(&txt);
                                }
                                Ok(Event::CData(e)) => {
                                    if let Ok(s) = std::str::from_utf8(e.as_ref()) {
                                        svg_path.push_str(s);
                                    }
                                }
                                Ok(Event::End(ref end_e)) if end_e.local_name().as_ref() == b"Path" => break,
                                Ok(Event::Eof) => break,
                                Err(_) => break,
                                _ => (),
                            }
                        }
                        let path_trimmed = svg_path.trim();
                        if !path_trimmed.is_empty() {
                            if let Some(s) = shape.as_mut() {
                                s.svg_path.push_str(path_trimmed);
                            }
                        }
                        eprintln!("[parser] Path captured: len={}, preview={:?}",
                            svg_path.len(),
                            &svg_path[..svg_path.len().min(60)]);
                    }

                    // ── Common sub-elements (work for both text & shape) ──
                    b"GeometryType" if in_shape => { in_geometry_type = true; }
                    b"ScaleY" if in_shape && in_adjust => { in_scale_y = true; }
                    b"Type" if in_shape && (in_head_end || in_tail_end) => { in_end_type = true; }

                    b"Id" => { in_id = true; }
                    b"X" => { in_x = true; }
                    b"Y" => { in_y = true; }
                    b"Width" => { in_width = true; }
                    b"Height" => { in_height = true; }
                    b"Rotation" => { in_rotation = true; }
                    b"IsLocked" => { in_is_locked = true; }
                    b"Thickness" if in_shape => { in_thickness = true; }

                    b"Background" => {
                        in_background = true;
                        if in_shape {
                            color_brush_target = Some("shape-fill");
                        } else if !in_shape && (in_element_wrapper || in_text_elem) && !in_rich_text {
                            // text background handled by old logic
                        }
                    }
                    b"Foreground" => {
                        in_foreground = true;
                        if in_shape {
                            color_brush_target = Some("shape-stroke");
                        } else if in_text_run {
                            color_brush_target = Some("text-fg");
                        }
                    }
                    b"ColorBrush" => {
                        in_color_brush = true;
                    }

                    // ── Text-only sub-elements ───────────────────────
                    b"FontSize" if in_text_run => { in_font_size = true; }
                    b"FontWeight" if in_text_run => { in_font_weight = true; }
                    b"FontFamily" if in_text_run => { in_font_family = true; }
                    b"Source" if in_text_run || in_font_family => { in_source = true; }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                let text_val = match e.unescape() {
                    Ok(cow) => cow.to_string(),
                    Err(_) => continue,
                };
                let trimmed = text_val.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // ── Geometry type (shape) ──────────────────────────
                if in_geometry_type {
                    if let Some(s) = shape.as_mut() {
                        s.geometry = match trimmed {
                            "Line" => GeometryKind::Line,
                            "LineArrowEnd" => { s.has_end_arrow = true; GeometryKind::LineArrowEnd }
                            "LineArrowStart" => { s.has_start_arrow = true; GeometryKind::LineArrowStart }
                            "LineArrowStartEnd" => {
                                s.has_start_arrow = true;
                                s.has_end_arrow = true;
                                GeometryKind::LineArrowStartEnd
                            }
                            "FreeLine" => GeometryKind::FreeLine,
                            "Rectangle" => GeometryKind::Rectangle,
                            "Ellipse" => GeometryKind::Ellipse,
                            "Circle" => GeometryKind::Circle,
                            "Bracket" => GeometryKind::Bracket,
                            "Brace" => GeometryKind::Brace,
                            "Fan" => GeometryKind::Fan,
                            other => GeometryKind::Other(other.to_string()),
                        };
                    }
                    continue;
                }

                // ── Arrow end type (Line/HeadEnd/TailEnd/Type) ─────
                if in_end_type {
                    if let Some(s) = shape.as_mut() {
                        if trimmed == "Arrow" {
                            if in_head_end { s.has_start_arrow = true; }
                            if in_tail_end { s.has_end_arrow = true; }
                        }
                    }
                    continue;
                }

                // ── ScaleY for Adjusts ─────────────────────────────
                if in_scale_y {
                    if let Some(s) = shape.as_mut() {
                        if let Ok(v) = trimmed.parse::<f32>() {
                            s.scale_y = v;
                        }
                    }
                    continue;
                }

                // ── Common numeric attributes ──────────────────────
                macro_rules! parse_f32_into {
                    ($slot:expr) => {
                        if let Some(inner) = $slot {
                            if let Ok(v) = trimmed.parse::<f32>() {
                                *inner = v;
                            }
                        }
                    };
                }
                macro_rules! parse_bool_into {
                    ($slot:expr) => {
                        if let Some(inner) = $slot {
                            *inner = trimmed == "True";
                        }
                    };
                }
                macro_rules! parse_str_into {
                    ($slot:expr) => {
                        if let Some(inner) = $slot {
                            *inner = trimmed.to_string();
                        }
                    };
                }

                if in_id {
                    if in_shape {
                        parse_str_into!(shape.as_mut().map(|s| &mut s.id));
                    } else {
                        parse_str_into!(text_elem.as_mut().map(|t| &mut t.id));
                    }
                    continue;
                }
                if in_x {
                    parse_f32_into!(shape.as_mut().map(|s| &mut s.x));
                    parse_f32_into!(text_elem.as_mut().map(|t| &mut t.x));
                    continue;
                }
                if in_y {
                    parse_f32_into!(shape.as_mut().map(|s| &mut s.y));
                    parse_f32_into!(text_elem.as_mut().map(|t| &mut t.y));
                    continue;
                }
                if in_width {
                    parse_f32_into!(shape.as_mut().map(|s| &mut s.width));
                    parse_f32_into!(text_elem.as_mut().map(|t| &mut t.width));
                    continue;
                }
                if in_height {
                    parse_f32_into!(shape.as_mut().map(|s| &mut s.height));
                    parse_f32_into!(text_elem.as_mut().map(|t| &mut t.height));
                    continue;
                }
                if in_rotation {
                    parse_f32_into!(shape.as_mut().map(|s| &mut s.rotation));
                    parse_f32_into!(text_elem.as_mut().map(|t| &mut t.rotation));
                    continue;
                }
                if in_is_locked {
                    parse_bool_into!(shape.as_mut().map(|s| &mut s.is_locked));
                    parse_bool_into!(text_elem.as_mut().map(|t| &mut t.is_locked));
                    continue;
                }
                if in_thickness {
                    if let Some(s) = shape.as_mut() {
                        if let Ok(v) = trimmed.parse::<f32>() {
                            s.thickness = v;
                        }
                    }
                    continue;
                }

                // ── ColorBrush ─────────────────────────────────────
                if in_color_brush {
                    if let Ok(color) = ArgbColor::from_hex(trimmed) {
                        match color_brush_target {
                            Some("shape-stroke") => if let Some(s) = shape.as_mut() { s.stroke_color = color; },
                            Some("shape-fill") => if let Some(s) = shape.as_mut() { s.fill_color = color; },
                            Some("text-fg") => if let Some(t) = text_elem.as_mut() { t.foreground = color; },
                            Some("text-bg") => if let Some(t) = text_elem.as_mut() { t.background = color; },
                            _ => {}
                        }
                    }
                    continue;
                }

                // ── Text content inside RichText/TextRun/Text ──────
                if text_depth > 1 && in_rich_text && !text_content_captured {
                    if let Some(elem) = text_elem.as_mut() {
                        elem.content.push_str(trimmed);
                        text_content_captured = true;
                    }
                    continue;
                }

                // ── Text-only fields (FontSize, FontWeight, Source) ─
                if in_font_size {
                    if let Some(elem) = text_elem.as_mut() {
                        if let Ok(v) = trimmed.parse::<f32>() {
                            elem.font_size = v;
                        }
                    }
                    continue;
                }
                if in_font_weight {
                    if let Some(elem) = text_elem.as_mut() {
                        elem.font_weight = trimmed.to_string();
                    }
                    continue;
                }
                if in_source {
                    if let Some(elem) = text_elem.as_mut() {
                        elem.font_family = trimmed.to_string();
                    }
                    continue;
                }

                // Text content inside RichText/TextRun/Text (fallback)
                if in_text_run {
                    // Unknown text in text run, skip
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = e.local_name();
                match tag.as_ref() {
                    // ── Text end tags ─────────────────────────────────
                    b"Text" if !in_shape => {
                        text_depth -= 1;
                        if text_depth == 0 && in_text_elem {
                            if let Some(elem) = text_elem.take() {
                                log::info!("[enbx] Parsed Text: '{}' at ({:.1},{:.1})",
                                    elem.content, elem.x, elem.y);
                                elements.push(SlideElement::Text(elem));
                            }
                            // Reset ALL text parsing state
                            in_text_elem = false;
                            in_element_wrapper = false;
                            in_rich_text = false;
                            in_text_run = false;
                            text_content_captured = false;
                            in_background = false;
                            in_foreground = false;
                            in_color_brush = false;
                            color_brush_target = None;
                            in_font_size = false;
                            in_font_weight = false;
                            in_font_family = false;
                            in_source = false;
                            in_x = false;
                            in_y = false;
                            in_width = false;
                            in_height = false;
                            in_rotation = false;
                            in_is_locked = false;
                            in_id = false;
                        }
                    }
                    b"RichText" => { in_rich_text = false; }
                    b"TextRun" => { in_text_run = false; text_content_captured = false; }
                    b"Background" => { in_background = false; color_brush_target = None; }
                    b"Foreground" => { in_foreground = false; color_brush_target = None; }
                    b"Element" if !in_shape => {
                        if let Some(elem) = text_elem.take() {
                            log::info!("[enbx] Parsed Text(wrap): '{}' at ({:.1},{:.1})",
                                elem.content, elem.x, elem.y);
                            elements.push(SlideElement::Text(elem));
                        }
                        // Reset ALL text parsing state
                        in_element_wrapper = false;
                        in_text_elem = false;
                        in_rich_text = false;
                        in_text_run = false;
                        text_content_captured = false;
                        in_background = false;
                        in_foreground = false;
                        in_color_brush = false;
                        color_brush_target = None;
                        in_font_size = false;
                        in_font_weight = false;
                        in_font_family = false;
                        in_source = false;
                        in_x = false;
                        in_y = false;
                        in_width = false;
                        in_height = false;
                        in_rotation = false;
                        in_is_locked = false;
                        in_id = false;
                    }

                    // ── Shape end tags ───────────────────────────────
                    b"Shape" => {
                        if let Some(s) = shape.take() {
                            log::info!(
                                "[enbx] Parsed Shape: geom={:?} at ({:.1},{:.1}) size={:.1}x{:.1} stroke=#{:02X}{:02X}{:02X}{:02X} path_len={}",
                                s.geometry, s.x, s.y, s.width, s.height,
                                s.stroke_color.a, s.stroke_color.r, s.stroke_color.g, s.stroke_color.b,
                                s.svg_path.len()
                            );
                            elements.push(SlideElement::Shape(s));
                        }
                        // Reset ALL shape parsing state
                        in_shape = false;
                        in_geometry = false;
                        in_preset_geometry = false;
                        in_custom_geometry = false;
                        in_adjusts = false;
                        in_adjust = false;
                        in_line = false;
                        in_head_end = false;
                        in_tail_end = false;
                        in_geometry_type = false;
                        in_scale_y = false;
                        in_end_type = false;
                        in_x = false;
                        in_y = false;
                        in_width = false;
                        in_height = false;
                        in_rotation = false;
                        in_is_locked = false;
                        in_id = false;
                        in_thickness = false;
                        in_background = false;
                        in_foreground = false;
                        in_color_brush = false;
                        color_brush_target = None;
                    }
                    b"Geometry" => { in_geometry = false; }
                    b"PresetGeometry" => { in_preset_geometry = false; }
                    b"CustomGeometry" => { in_custom_geometry = false; }
                    b"Adjusts" => { in_adjusts = false; }
                    b"Adjust" => { in_adjust = false; in_scale_y = false; }
                    b"Line" => { in_line = false; }
                    b"HeadEnd" => { in_head_end = false; in_end_type = false; }
                    b"TailEnd" => { in_tail_end = false; in_end_type = false; }
                    b"Path" => { /* Path consumed inline in Start handler */ }

                    b"GeometryType" => { in_geometry_type = false; }
                    b"ScaleY" => { in_scale_y = false; }
                    b"Type" => { in_end_type = false; }

                    b"Id" => { in_id = false; }
                    b"X" => { in_x = false; }
                    b"Y" => { in_y = false; }
                    b"Width" => { in_width = false; }
                    b"Height" => { in_height = false; }
                    b"Rotation" => { in_rotation = false; }
                    b"IsLocked" => { in_is_locked = false; }
                    b"Thickness" => { in_thickness = false; }
                    b"ColorBrush" => { in_color_brush = false; color_brush_target = None; }

                    b"FontSize" => { in_font_size = false; }
                    b"FontWeight" => { in_font_weight = false; }
                    b"FontFamily" => { in_font_family = false; }
                    b"Source" => { in_source = false; }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
    }

    log::info!("[parser] Done: {} slide elements", elements.len());
    Ok(elements)
}

// ── Board / Document metadata parsing ────────────────────────────

/// Parse Board.xml to extract canvas dimensions (width, height).
pub fn parse_board(xml: &str) -> Result<(f32, f32), String> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);
    let mut width: f32 = 1920.0;
    let mut height: f32 = 1080.0;
    let mut in_w = false;
    let mut in_h = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                match e.local_name().as_ref() {
                    b"SlideWidth" => in_w = true,
                    b"SlideHeight" => in_h = true,
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_w {
                    if let Ok(s) = e.unescape() {
                        if let Ok(v) = s.trim().parse::<f32>() { width = v; }
                    }
                } else if in_h {
                    if let Ok(s) = e.unescape() {
                        if let Ok(v) = s.trim().parse::<f32>() { height = v; }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                match e.local_name().as_ref() {
                    b"SlideWidth" => in_w = false,
                    b"SlideHeight" => in_h = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("Board.xml parse error: {}", e)),
            _ => {}
        }
    }
    Ok((width, height))
}

/// Parse Document.xml to extract title and author.
pub fn parse_document(xml: &str) -> (String, String) {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);
    let mut title = String::new();
    let mut author = String::new();
    let mut in_title = false;
    let mut in_author = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                match e.local_name().as_ref() {
                    b"Title" => in_title = true,
                    b"Author" => in_author = true,
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_title {
                    if let Ok(s) = e.unescape() { title.push_str(s.trim()); }
                } else if in_author {
                    if let Ok(s) = e.unescape() { author.push_str(s.trim()); }
                }
            }
            Ok(Event::End(ref e)) => {
                match e.local_name().as_ref() {
                    b"Title" => in_title = false,
                    b"Author" => in_author = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    (title, author)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_line_arrow_end() {
        let xml = r#"<?xml version="1.0"?><Slide>
            <Elements>
                <Shape>
                    <Geometry><PresetGeometry><GeometryType>LineArrowEnd</GeometryType>
                        <Adjusts><Adjust><Id>0</Id><ScaleX>0</ScaleX><ScaleY>0</ScaleY></Adjust>
                        <Adjust><Id>1</Id><ScaleX>1</ScaleX><ScaleY>0</ScaleY></Adjust></Adjusts>
                    </PresetGeometry></Geometry>
                    <Path></Path>
                    <Foreground><ColorBrush>#FF166EE1</ColorBrush></Foreground>
                    <Thickness>2</Thickness>
                    <Line><TailEnd><Type>Arrow</Type></TailEnd></Line>
                    <Id>s1</Id><X>100</X><Y>200</Y><Width>300</Width><Height>0.001</Height>
                    <Rotation>0</Rotation><IsLocked>False</IsLocked>
                </Shape>
            </Elements>
        </Slide>"#;
        let elems = parse_slide_xml(xml).expect("parse ok");
        assert_eq!(elems.len(), 1);
        match &elems[0] {
            SlideElement::Shape(s) => {
                assert_eq!(s.geometry, GeometryKind::LineArrowEnd);
                assert!(s.has_end_arrow);
                assert!(!s.has_start_arrow);
                assert!((s.x - 100.0).abs() < 0.01);
                assert!((s.y - 200.0).abs() < 0.01);
                assert!((s.width - 300.0).abs() < 0.01);
                assert_eq!(s.thickness, 2.0);
                assert_eq!(s.stroke_color.r, 0x16);
                assert_eq!(s.stroke_color.g, 0x6E);
                assert_eq!(s.stroke_color.b, 0xE1);
            }
            _ => panic!("expected shape"),
        }
    }

    #[test]
    fn parse_double_arrow() {
        let xml = r#"<?xml version="1.0"?><Slide><Elements><Shape>
            <Geometry><PresetGeometry><GeometryType>LineArrowStartEnd</GeometryType></PresetGeometry></Geometry>
            <Foreground><ColorBrush>#FF166EE1</ColorBrush></Foreground>
            <Thickness>2</Thickness>
            <Line><HeadEnd><Type>Arrow</Type></HeadEnd><TailEnd><Type>Arrow</Type></TailEnd></Line>
            <Id>s2</Id><X>0</X><Y>0</Y><Width>100</Width><Height>100</Height>
        </Shape></Elements></Slide>"#;
        let elems = parse_slide_xml(xml).expect("parse ok");
        match &elems[0] {
            SlideElement::Shape(s) => {
                assert!(s.has_start_arrow);
                assert!(s.has_end_arrow);
            }
            _ => panic!("expected shape"),
        }
    }

    #[test]
    fn parse_free_line() {
        let svg = "M175.8,0Q-0.5,169.5,12,209.8C24.5,250.2,142.4,230.5,144.9,251.8";
        let xml = format!(r#"<?xml version="1.0"?><Slide><Elements><Shape>
            <Geometry><CustomGeometry><GeometryType>FreeLine</GeometryType><Path>{}</Path></CustomGeometry></Geometry>
            <Path>{}</Path>
            <Foreground><ColorBrush>#FF166EE1</ColorBrush></Foreground>
            <Thickness>2</Thickness>
            <Line><TailEnd><Type>Arrow</Type></TailEnd></Line>
            <Id>s3</Id><X>500</X><Y>100</Y><Width>175</Width><Height>337</Height>
        </Shape></Elements></Slide>"#, svg, svg);
        let elems = parse_slide_xml(&xml).expect("parse ok");
        match &elems[0] {
            SlideElement::Shape(s) => {
                assert_eq!(s.geometry, GeometryKind::FreeLine);
                assert!(s.has_end_arrow);
                assert!(!s.svg_path.is_empty());
            }
            _ => panic!("expected shape"),
        }
    }
}
