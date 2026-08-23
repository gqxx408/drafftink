//! .enbx loader — ZIP extraction, ZipSlip defence, parser pipeline.
//!
//! Converts a raw .enbx byte buffer into a drafftink `CoursewareDoc`.

use std::io::{Cursor, Read};

use drafftink_core::model::{CoursewareDoc, PageContent};
use drafftink_core::plugin::api::PluginContext;
use egui::Color32;

use crate::elements::{
    shape::{GeometryKind, ShapeElementData, SlideElement},
    text::TextElement as ParserText,
};
use crate::parser;

/// Load an .enbx byte buffer (ZIP container) into a CoursewareDoc.
pub fn load_enbx(data: &[u8], ctx: &dyn PluginContext) -> Result<CoursewareDoc, String> {
    let cursor = Cursor::new(data);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("ZIP open failed: {}", e))?;

    ctx.log("info", &format!("ZIP container with {} entries", archive.len()));

    let mut doc = CoursewareDoc::empty();
    // Each Slide_*.xml becomes a PageContent. Collect (page_num, xml) pairs.
    let mut slide_data: Vec<(usize, String)> = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();

        // ── Zip Slip check ──
        if !is_safe_path(&name) {
            ctx.log("warn", &format!("Skipping unsafe path: {}", name));
            continue;
        }

        // ── Board.xml: read canvas size ──
        if name == "Board.xml" {
            let mut xml = String::new();
            entry.read_to_string(&mut xml).map_err(|e| e.to_string())?;
            if let Ok((w, h)) = parse_board_size(&xml) {
                doc.page_size = [w, h];
                ctx.log("info", &format!("Canvas: {}x{}", w, h));
            }
        }

        // ── Detect slide XML (case-insensitive, robust matching like importer) ──
        let lower = name.to_lowercase();
        if lower.contains("slide") && lower.ends_with(".xml")
            && !lower.contains("slideshow")
            && !lower.contains("slidelayout")
            && !lower.contains("slidemaster")
        {
            // Extract first contiguous digit sequence as page number
            let digits: String = name
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            let page_num = if digits.is_empty() {
                slide_data.len()
            } else {
                digits.parse::<usize>().unwrap_or(slide_data.len())
            };
            let mut xml = String::new();
            entry.read_to_string(&mut xml).map_err(|e| e.to_string())?;
            slide_data.push((page_num, xml));
        }
    }

    if slide_data.is_empty() {
        ctx.log("warn", "No slide XML files found in archive");
        return Ok(doc);
    }

    // Sort by page number and deduplicate
    slide_data.sort_by_key(|(page, _)| *page);
    let mut seen = std::collections::HashSet::new();
    slide_data.retain(|(page, _)| seen.insert(*page));

    ctx.log("info", &format!(
        "Found {} slides, pages: {:?}",
        slide_data.len(),
        slide_data.iter().map(|(p, _)| *p).collect::<Vec<_>>()
    ));

    // Parse each slide in order
    for (page_num, xml) in &slide_data {
        match parser::parse_slide_xml(xml) {
            Ok(elements) => {
                ctx.log("info", &format!(
                    "Page {} parsed: {} elements",
                    page_num + 1,
                    elements.len()
                ));
                let page = elements_to_page(&elements, ctx);
                doc.pages.push(page);
            }
            Err(e) => {
                ctx.log("warn", &format!("Parse skipped page {}: {}", page_num + 1, e));
            }
        }
    }

    // If empty() gave us a blank page but we loaded real pages, remove the blank first page
    if doc.pages.len() > 1 {
        doc.pages = doc.pages.split_off(1);
    }
    if doc.pages.is_empty() {
        doc.pages.push(PageContent::default());
    }
    Ok(doc)
}

// ── Zip Slip defence ─────────────────────────────────────────────

/// Reject `../` traversals and absolute paths.
fn is_safe_path(name: &str) -> bool {
    let p = std::path::Path::new(name);
    // No absolute paths
    if p.is_absolute() {
        return false;
    }
    // No component should be ".."
    for c in p.components() {
        if c == std::path::Component::ParentDir {
            return false;
        }
    }
    true
}

// ── Board.xml → canvas size ──────────────────────────────────────

fn parse_board_size(xml: &str) -> Result<(f32, f32), String> {
    let sw = extract_tag(xml, "SlideWidth")?.parse::<f32>().unwrap_or(1920.0);
    let sh = extract_tag(xml, "SlideHeight")?.parse::<f32>().unwrap_or(1080.0);
    Ok((sw, sh))
}

fn extract_tag(xml: &str, tag: &str) -> Result<String, String> {
    let start = format!("<{}>", tag);
    let end = format!("</{}>", tag);
    let s = xml.find(&start).ok_or_else(|| format!("<{}> not found", tag))?;
    let e = xml[s..]
        .find(&end)
        .ok_or_else(|| format!("</{}> not found", tag))?;
    Ok(xml[s + start.len()..s + e].to_string())
}

// ── Color conversion ─────────────────────────────────────────────

fn argb_to_color32(c: &crate::elements::text::ArgbColor) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

// ── Conversion: parsed SlideElement → drafftink model ────────────

fn elements_to_page(elements: &[SlideElement], ctx: &dyn PluginContext) -> PageContent {
    let mut page = PageContent::default();
    let mut text_count = 0usize;
    let mut shape_count = 0usize;

    for el in elements {
        match el {
            SlideElement::Text(t) => {
                page.elements.push(text_to_element(t));
                text_count += 1;
            }
            SlideElement::Shape(s) => {
                page.elements.push(shape_to_element(s));
                shape_count += 1;
            }
        }
    }

    ctx.log(
        "info",
        &format!(
            "Imported {} elements ({} text, {} shapes)",
            text_count + shape_count,
            text_count,
            shape_count
        ),
    );
    page
}

fn text_to_element(t: &ParserText) -> drafftink_core::model::Element {
    use drafftink_core::model::{BaseElement, Element, TextElement};
    let fill = argb_to_color32(&t.foreground);
    let bg = argb_to_color32(&t.background);
    let base = BaseElement {
        position: [t.x, t.y],
        size: [t.width.max(10.0), t.height.max(10.0)],
        rotation: t.rotation,
        fill_color: bg,
        stroke_color: fill,
        stroke_width: 1.0,
        locked: t.is_locked,
        name: t.content.chars().take(20).collect(),
        ..Default::default()
    };
    Element::Text(TextElement {
        base,
        text: t.content.clone(),
        font_size: if t.font_size > 0.0 { t.font_size } else { 24.0 },
        font_family: t.font_family.clone(),
    })
}

fn shape_to_element(s: &ShapeElementData) -> drafftink_core::model::Element {
    use drafftink_core::model::{BaseElement, Element, ShapeElement, ShapeType, SvgShapeElement};
    let stroke = argb_to_color32(&s.stroke_color);
    let fill = argb_to_color32(&s.fill_color);

    let position = [s.x, s.y];
    let size = [s.width.max(0.001), s.height.max(0.001)];

    let make_base = || -> BaseElement {
        BaseElement {
            position,
            size,
            rotation: s.rotation,
            fill_color: fill,
            stroke_color: stroke,
            stroke_width: s.thickness.max(0.0),
            locked: s.is_locked,
            name: format!("{:?}", s.geometry),
            ..Default::default()
        }
    };

    match &s.geometry {
        GeometryKind::FreeLine => Element::SvgShape(SvgShapeElement {
            base: make_base(),
            svg_path: s.svg_path.clone(),
            is_closed: false,
            has_end_arrow: s.has_end_arrow,
            has_start_arrow: s.has_start_arrow,
        }),
        GeometryKind::Fan => Element::SvgShape(SvgShapeElement {
            base: make_base(),
            svg_path: s.svg_path.clone(),
            is_closed: true,
            has_end_arrow: false,
            has_start_arrow: false,
        }),
        GeometryKind::Rectangle if s.thickness <= 0.0 => {
            Element::Shape(ShapeElement {
                base: BaseElement {
                    stroke_width: 0.0,
                    ..make_base()
                },
                shape_type: ShapeType::Rectangle,
                has_start_arrow: false,
                has_end_arrow: false,
                scale_y: s.scale_y,
            })
        }
        GeometryKind::Rectangle => Element::Shape(ShapeElement {
            base: make_base(),
            shape_type: ShapeType::Rectangle,
            has_start_arrow: false,
            has_end_arrow: false,
            scale_y: s.scale_y,
        }),
        GeometryKind::Ellipse | GeometryKind::Circle => {
            // Seewo represents circles/ellipses using cubic Bezier SVG paths.
            // If we have path data, render via SvgShape for accurate shape;
            // otherwise fall back to rounded-rect approximation.
            if !s.svg_path.trim().is_empty() {
                Element::SvgShape(SvgShapeElement {
                    base: make_base(),
                    svg_path: s.svg_path.clone(),
                    is_closed: true, // circles/ellipses are always closed
                    has_end_arrow: false,
                    has_start_arrow: false,
                })
            } else {
                Element::Shape(ShapeElement {
                    base: make_base(),
                    shape_type: ShapeType::Ellipse,
                    has_start_arrow: false,
                    has_end_arrow: false,
                    scale_y: s.scale_y,
                })
            }
        },
        GeometryKind::Bracket => Element::Shape(ShapeElement {
            base: make_base(),
            shape_type: ShapeType::Bracket,
            has_start_arrow: false,
            has_end_arrow: false,
            scale_y: s.scale_y,
        }),
        GeometryKind::Brace => {
            // Brace has an empty <Path> in Seewo XML. Render it through the
            // `draw_shape` pipeline as `Element::Shape(ShapeType::Brace)` — the same
            // proven path used by Bracket (and by format_enbx's plugin importer.rs).
            // This keeps both format_enbx converters consistent and avoids the
            // earlier SvgShape brace path that failed to render in display mode.
            // `scale_y` (Seewo Adjusts) drives the brace curvature in `draw_brace`.
            Element::Shape(ShapeElement {
                base: make_base(),
                shape_type: ShapeType::Brace,
                has_start_arrow: false,
                has_end_arrow: false,
                scale_y: s.scale_y,
            })
        }
        GeometryKind::LineArrowEnd
        | GeometryKind::LineArrowStart
        | GeometryKind::LineArrowStartEnd
        | GeometryKind::Line => {
            let is_arrow = s.has_start_arrow || s.has_end_arrow;
            Element::Shape(ShapeElement {
                base: make_base(),
                shape_type: if is_arrow { ShapeType::Arrow } else { ShapeType::Line },
                has_start_arrow: s.has_start_arrow,
                has_end_arrow: s.has_end_arrow,
                scale_y: 0.0,
            })
        }
        GeometryKind::Other(_) => {
            if !s.svg_path.is_empty() {
                Element::SvgShape(SvgShapeElement {
                    base: make_base(),
                    svg_path: s.svg_path.clone(),
                    is_closed: false,
                    has_end_arrow: s.has_end_arrow,
                    has_start_arrow: s.has_start_arrow,
                })
            } else {
                Element::Shape(ShapeElement {
                    base: make_base(),
                    shape_type: ShapeType::Line,
                    has_start_arrow: s.has_start_arrow,
                    has_end_arrow: s.has_end_arrow,
                    scale_y: 0.0,
                })
            }
        }
    }
}
