//! EnbxImporter — FileImporter implementation for .enbx courseware.
//!
//! Blind-scan strategy: enumerate all ZIP entries, match slide files by name
//! pattern, extract page numbers from filenames, parse directly.
//! No path guessing — works with any Seewo packaging variant.

use std::io::{Cursor, Read};

use drafftink_core::model::CoursewareDoc;
use drafftink_core::plugin::api::{FileImporter, PluginContext};
use egui::Color32;

use crate::elements::{
    shape::{GeometryKind, ShapeElementData, SlideElement},
    text::TextElement as ParserText,
};
use crate::parser;
use crate::security;

/// ENBX file-importer backed by the ZIP + XML parser pipeline.
pub struct EnbxImporter;

impl FileImporter for EnbxImporter {
    fn supported_extensions(&self) -> Vec<String> {
        vec!["enbx".into(), "enbxz".into()]
    }

    fn can_import(&self, data: &[u8]) -> bool {
        if data.len() < 4 {
            return false;
        }
        if &data[..4] == b"ENBX" {
            return true;
        }
        if &data[..4] == b"PK\x03\x04" {
            return contains_slide_entry(data);
        }
        false
    }

    fn import(&self, data: &[u8], ctx: &dyn PluginContext) -> Result<CoursewareDoc, String> {
        ctx.log("info", &format!("[enbx] Importing {} bytes", data.len()));

        if &data[..4] == b"PK\x03\x04" {
            return import_enbx(data, ctx);
        }
        if &data[..4] == b"ENBX" {
            return import_enbx(data, ctx);
        }

        Err("ENBX format not recognised".into())
    }
}

// ── Blind-scan import pipeline ────────────────────────────────────

fn import_enbx(data: &[u8], _ctx: &dyn PluginContext) -> Result<CoursewareDoc, String> {
    eprintln!("🦀 [DISPLAY-ENBX BUILD: 2026-08-01-ARROWS] 🦀");

    let cursor = Cursor::new(data);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("ZIP open: {}", e))?;

    // ── 1. ZIP bomb check ────────────────────────────────────────
    security::check_zip_bomb(&mut archive, 100)?;
    eprintln!("[enbx] ✅ ZIP安全校验通过");

    // ── 2. Parse Board.xml → canvas size ─────────────────────────
    let (canvas_w, canvas_h) = if let Ok(board_xml) = read_zip_text(&mut archive, "Board.xml") {
        parser::parse_board(&board_xml).unwrap_or((1280.0, 720.0))
    } else {
        (1280.0, 720.0)
    };

    // ── 3. Parse Document.xml → title / author ──────────────────
    let (title, author) = if let Ok(doc_xml) = read_zip_text(&mut archive, "Document.xml") {
        parser::parse_document(&doc_xml)
    } else {
        (String::new(), String::new())
    };

    eprintln!(
        "[enbx] 📄 元数据: 标题={}, 作者={}, 画布={}x{}",
        title, author, canvas_w, canvas_h
    );

    // ── 4. Blind scan: enumerate ALL ZIP entries ────────────────
    eprintln!("[enbx] 🚀 盲扫导入开始，ZIP 共 {} 个条目", archive.len());

    let mut slide_entries: Vec<(usize, String)> = Vec::new();

    for i in 0..archive.len() {
        let entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.name().to_string();
        let lower = name.to_lowercase();

        // Match slide XML by name pattern (case-insensitive)
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
                0
            } else {
                digits.parse::<usize>().unwrap_or(0)
            };

            eprintln!(
                "[enbx] ✅ 发现Slide文件: {} (页码: {})",
                name, page_num
            );
            slide_entries.push((page_num, name));
        }
    }

    if slide_entries.is_empty() {
        let all_entries: Vec<String> = (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_string()))
            .collect();
        return Err(format!(
            "[enbx] ❌ 未找到Slide文件。ZIP内所有条目: {:?}",
            all_entries
        ));
    }

    slide_entries.sort_by_key(|(page, _)| *page);
    let mut seen = std::collections::HashSet::new();
    slide_entries.retain(|(page, _)| seen.insert(*page));

    eprintln!(
        "[enbx] 🔍 枚举到Slide页码: {:?}",
        slide_entries.iter().map(|(p, _)| *p).collect::<Vec<_>>()
    );

    // ── 5. Parse each slide ──────────────────────────────────────
    let mut pages: Vec<drafftink_core::model::PageContent> = Vec::new();

    for (page_num, entry_name) in &slide_entries {
        if !is_safe_path(entry_name) {
            eprintln!("[enbx] ⚠️ 跳过不安全路径: {}", entry_name);
            continue;
        }

        let xml = match read_zip_text(&mut archive, entry_name) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[enbx]  无法读取 {}: {}", entry_name, e);
                continue;
            }
        };

        match parser::parse_slide_xml(&xml) {
            Ok(slide_elems) => {
                let element_count = slide_elems.len();
                eprintln!(
                    "[enbx] ✅ 第{}页解析成功，元素数: {}",
                    page_num + 1,
                    element_count
                );
                let mut page = drafftink_core::model::PageContent::default();
                for se in &slide_elems {
                    match se {
                        SlideElement::Text(t) => page.elements.push(text_to_element(t)),
                        SlideElement::Shape(s) => page.elements.push(shape_to_element(s)),
                    }
                }
                pages.push(page);
            }
            Err(e) => {
                eprintln!("[enbx] ❌ 第{}页解析失败: {}", page_num + 1, e);
            }
        }
    }

    if pages.is_empty() {
        return Err("[enbx] 所有Slide解析失败".to_string());
    }

    let mut doc = CoursewareDoc::empty();
    doc.page_size = [canvas_w, canvas_h];
    doc.pages = pages;
    doc.elements.clear();

    eprintln!("[enbx] 🎉 导入完成，总页数: {}", doc.pages.len());
    Ok(doc)
}

// ── Conversion helpers ───────────────────────────────────────────

fn argb_to_color32(c: &crate::elements::text::ArgbColor) -> Color32 {
    // Seewo format is #AARRGGBB → egui Color32 is RGBA (unmultiplied)
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
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
        GeometryKind::FreeLine => {
            // FreeLine with SVG path → SvgShape (stroked open path, may have arrow)
            Element::SvgShape(SvgShapeElement {
                base: make_base(),
                svg_path: s.svg_path.clone(),
                is_closed: false,
                has_end_arrow: s.has_end_arrow,
                has_start_arrow: s.has_start_arrow,
            })
        }
        GeometryKind::Fan => {
            // Fan (扇形) — filled closed SVG path
            Element::SvgShape(SvgShapeElement {
                base: make_base(),
                svg_path: s.svg_path.clone(),
                is_closed: true,
                has_end_arrow: false,
                has_start_arrow: false,
            })
        }
        GeometryKind::Rectangle if s.thickness <= 0.0 => {
            // Filled rectangle with no border
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
        GeometryKind::Rectangle => {
            Element::Shape(ShapeElement {
                base: make_base(),
                shape_type: ShapeType::Rectangle,
                has_start_arrow: false,
                has_end_arrow: false,
                scale_y: s.scale_y,
            })
        }
        GeometryKind::Ellipse | GeometryKind::Circle => {
            // Seewo circles/ellipses use cubic Bezier SVG paths.
            if !s.svg_path.trim().is_empty() {
                Element::SvgShape(SvgShapeElement {
                    base: make_base(),
                    svg_path: s.svg_path.clone(),
                    is_closed: true,
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
        }
        GeometryKind::Bracket => {
            Element::Shape(ShapeElement {
                base: make_base(),
                shape_type: ShapeType::Bracket,
                has_start_arrow: false,
                has_end_arrow: false,
                scale_y: s.scale_y,
            })
        }
        GeometryKind::Brace => {
            Element::Shape(ShapeElement {
                base: make_base(),
                shape_type: ShapeType::Brace,
                has_start_arrow: false,
                has_end_arrow: false,
                scale_y: s.scale_y,
            })
        }
        GeometryKind::LineArrowEnd | GeometryKind::LineArrowStart | GeometryKind::LineArrowStartEnd | GeometryKind::Line => {
            // Straight line (optionally with arrows)
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
            // Unknown shape: treat as generic line/stroke if we have a path, otherwise skip
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

// ── Helpers ──────────────────────────────────────────────────────

fn read_zip_text(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<String, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| format!("Entry not found: {}", name))?;
    let mut s = String::new();
    entry.read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(s)
}

fn contains_slide_entry(data: &[u8]) -> bool {
    if let Ok(mut a) = zip::ZipArchive::new(Cursor::new(data)) {
        for i in 0..a.len() {
            if let Ok(e) = a.by_index(i) {
                let lower = e.name().to_lowercase();
                if lower.contains("slide") && lower.ends_with(".xml")
                    && !lower.contains("slideshow")
                    && !lower.contains("slidelayout")
                    && !lower.contains("slidemaster")
                {
                    return true;
                }
            }
        }
    }
    false
}

fn is_safe_path(name: &str) -> bool {
    !name.contains("..") && !name.contains(":\\") && !name.starts_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_safe_path_blocks_traversal() {
        assert!(is_safe_path("Slides/test.xml"));
        assert!(!is_safe_path("../etc/passwd"));
        assert!(!is_safe_path("C:\\Windows\\evil.dll"));
    }

    #[test]
    fn import_with_slides_directory() {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = zip::write::FileOptions::default();
            z.start_file("Slides/Slide_0.xml", opts).unwrap();
            z.write_all(
                br#"<?xml version="1.0"?><Slide><Id>s0</Id><Width>1280</Width><Height>720</Height><Elements><Element><Id>e1</Id><X>0</X><Y>0</Y><Width>100</Width><Height>50</Height><Text><RichText><TextRuns><TextRun><Text>SLIDES_DIR</Text><FontSize>24</FontSize><Foreground><ColorBrush>#FF000000</ColorBrush></Foreground></TextRun></TextRuns></RichText></Text></Element></Elements></Slide>"#,
            )
            .unwrap();
            z.finish().unwrap();
        }
        let importer = EnbxImporter;
        assert!(importer.can_import(&buf));
        use drafftink_core::plugin::api::DummyContext;
        let ctx = DummyContext;
        let doc = importer.import(&buf, &ctx).expect("Import should succeed");
        assert_eq!(doc.pages.len(), 1);
    }

    #[test]
    fn import_with_skip_pages() {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = zip::write::FileOptions::default();
            for page in &[0, 2] {
                z.start_file(&format!("Slide/Slide_{}.xml", page), opts).unwrap();
                let xml = format!(
                    r#"<?xml version="1.0"?><Slide><Id>s{}</Id><Width>1280</Width><Height>720</Height><Elements><Element><Id>e1</Id><X>0</X><Y>0</Y><Width>100</Width><Height>50</Height><Text><RichText><TextRuns><TextRun><Text>PAGE_{}</Text><FontSize>24</FontSize><Foreground><ColorBrush>#FF000000</ColorBrush></Foreground></TextRun></TextRuns></RichText></Text></Element></Elements></Slide>"#,
                    page, page
                );
                z.write_all(xml.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        let importer = EnbxImporter;
        assert!(importer.can_import(&buf));
        use drafftink_core::plugin::api::DummyContext;
        let ctx = DummyContext;
        let doc = importer.import(&buf, &ctx).expect("Import should succeed");
        assert_eq!(doc.pages.len(), 2, "Should have 2 pages (skip page 1)");
    }
}
