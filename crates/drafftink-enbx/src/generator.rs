//! `.enbx` file generation — writes a ZIP archive containing XML slides and
//! resource mappings.
//!
//! The generator produces files compatible with Seewo EasiNote 5 by following
//! the native ENBX XML schema discovered through reverse-engineering of real
//! `.enbx` exports.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use zip::write::FileOptions;
use zip::CompressionMethod;

use crate::mapper::xml_value_to_string;
use crate::parser::{
    Enbx3dShape, EnbxActivity, EnbxActivityItem, EnbxAudio, EnbxElement, EnbxGroup, EnbxImage,
    EnbxPath, EnbxShape, EnbxSlide, EnbxText, EnbxTopic, EnbxVideo,
};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generate an `.enbx` file from a list of slides.
///
/// Creates a ZIP archive at `output_path` containing:
/// - `Reference.xml` — resource-id → filename mappings
/// - `Slide_1.xml`, `Slide_2.xml`, … — per-slide content
/// - Resource files (images) referenced by slides
///
/// `extra_files` contains arbitrary additional entries to embed in the archive
/// (e.g. `Board.xml`, `Document.xml`, thumbnail PNG, slide thumbnails).
/// The keys are ZIP entry paths (use forward slashes); the values are the raw
/// bytes. This makes the round-trip a complete Seewo ENBX rather than a
/// bare-slides package.
///
/// # Errors
///
/// Returns an error if the output file cannot be created or written.
pub fn generate_enbx(
    slides: &[EnbxSlide],
    extra_files: &HashMap<String, Vec<u8>>,
    output_path: &Path,
) -> Result<()> {
    generate_enbx_with_resources(slides, &HashMap::new(), extra_files, output_path)
}

/// Generate an `.enbx` file with explicit resource data and arbitrary extras.
///
/// `resources` maps resource-ids to their raw bytes.  Each resource is written
/// to the archive under `Resources/<filename>`.
///
/// `extra_files` are additional ZIP entries (same key/value semantics as above).
/// Files with the same ZIP path as a `Reference.xml` / `Slide_N.xml` / resource
/// entry will overwrite the standard entry — callers are expected to avoid
/// reserved paths.
pub fn generate_enbx_with_resources(
    slides: &[EnbxSlide],
    resources: &HashMap<String, Vec<u8>>,
    extra_files: &HashMap<String, Vec<u8>>,
    output_path: &Path,
) -> Result<()> {
    let file = std::fs::File::create(output_path)
        .with_context(|| format!("failed to create output file: {}", output_path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default().compression_method(CompressionMethod::Deflated);

    // ── Collect resource references ────────────────────────────────────
    let ref_map = collect_resource_refs(slides);

    // ── Write Reference.xml ────────────────────────────────────────────
    let ref_xml = generate_reference_xml(&ref_map);
    zip.start_file("Reference.xml", options)
        .context("failed to start Reference.xml")?;
    zip.write_all(ref_xml.as_bytes())
        .context("failed to write Reference.xml")?;

    // ── Write resource files ───────────────────────────────────────────
    for (id, filename) in &ref_map {
        if let Some(data) = resources.get(id).or_else(|| resources.get(filename)) {
            let entry_name = if filename.starts_with("Resources/") {
                filename.clone()
            } else {
                format!("Resources/{filename}")
            };
            zip.start_file(&entry_name, options)
                .with_context(|| format!("failed to start resource: {entry_name}"))?;
            zip.write_all(data)
                .with_context(|| format!("failed to write resource: {entry_name}"))?;
        }
    }

    // ── Write slide XML files ──────────────────────────────────────────
    for (i, slide) in slides.iter().enumerate() {
        let name = format!("Slide_{}.xml", i + 1);
        let xml = generate_slide_xml(slide);
        zip.start_file(&name, options)
            .with_context(|| format!("failed to start {name}"))?;
        zip.write_all(xml.as_bytes())
            .with_context(|| format!("failed to write {name}"))?;
    }

    // ── Write extra files (Board.xml, Document.xml, thumbnails, …) ────
    for (path, data) in extra_files {
        zip.start_file(path, options)
            .with_context(|| format!("failed to start extra file: {path}"))?;
        zip.write_all(data)
            .with_context(|| format!("failed to write extra file: {path}"))?;
    }

    zip.finish().context("failed to finalise ZIP archive")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Resource reference collection
// ---------------------------------------------------------------------------

/// Walk all slides and collect unique resource-id → filename mappings.
fn collect_resource_refs(slides: &[EnbxSlide]) -> Vec<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut refs: Vec<(String, String)> = Vec::new();

    for slide in slides {
        collect_refs_from_elements(&slide.elements, &mut seen, &mut refs);
    }

    refs
}

fn collect_refs_from_elements(
    elements: &[EnbxElement],
    seen: &mut std::collections::HashSet<String>,
    refs: &mut Vec<(String, String)>,
) {
    for elem in elements {
        match elem {
            EnbxElement::Image(img) => {
                if !img.resource_id.is_empty() && seen.insert(img.resource_id.clone()) {
                    let filename = img
                        .resource_id
                        .strip_prefix("Resources/")
                        .unwrap_or(&img.resource_id)
                        .to_string();
                    refs.push((img.resource_id.clone(), filename));
                }
            }
            EnbxElement::Video(v) => {
                if !v.resource_id.is_empty() && seen.insert(v.resource_id.clone()) {
                    let filename = v
                        .resource_id
                        .strip_prefix("Resources/")
                        .unwrap_or(&v.resource_id)
                        .to_string();
                    refs.push((v.resource_id.clone(), filename));
                }
            }
            EnbxElement::Audio(a) => {
                if !a.resource_id.is_empty() && seen.insert(a.resource_id.clone()) {
                    let filename = a
                        .resource_id
                        .strip_prefix("Resources/")
                        .unwrap_or(&a.resource_id)
                        .to_string();
                    refs.push((a.resource_id.clone(), filename));
                }
            }
            EnbxElement::ActivityItem(a) => {
                if let Some(bg) = &a.background_source {
                    if !bg.is_empty() && seen.insert(bg.clone()) {
                        let filename = bg.strip_prefix("Resources/").unwrap_or(bg).to_string();
                        refs.push((bg.clone(), filename));
                    }
                }
            }
            EnbxElement::Group(g) => {
                collect_refs_from_elements(&g.elements, seen, refs);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Reference.xml generation
// ---------------------------------------------------------------------------

/// Generate the `Reference.xml` content.
fn generate_reference_xml(refs: &[(String, String)]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<Reference>\n");
    for (i, (id, filename)) in refs.iter().enumerate() {
        let rid = if id.starts_with("rId") {
            id.clone()
        } else {
            format!("rId{}", i + 1)
        };
        out.push_str(&format!(
            "  <Relationship Id=\"{}\" Target=\"Resources/{}\"/>\n",
            xml_escape_attr(&rid),
            xml_escape_attr(filename)
        ));
    }
    out.push_str("</Reference>\n");
    out
}

// ---------------------------------------------------------------------------
// Slide XML generation
// ---------------------------------------------------------------------------

/// Generate the XML content for a single slide.
fn generate_slide_xml(slide: &EnbxSlide) -> String {
    let (w, h) = slide.size;
    let bg = slide.background.as_deref().unwrap_or("FFFFFFFF");

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<Slide width=\"{}\" height=\"{}\" bgColor=\"{}\">\n",
        fmt_num(w),
        fmt_num(h),
        xml_escape_attr(bg)
    ));
    out.push_str("  <Elements>\n");

    for elem in &slide.elements {
        out.push_str(&generate_element_xml(elem, 2));
    }

    out.push_str("  </Elements>\n");
    out.push_str("</Slide>\n");
    out
}

/// Generate XML for a single element at the given indentation level.
fn generate_element_xml(elem: &EnbxElement, indent: usize) -> String {
    match elem {
        EnbxElement::Text(t) => generate_text_xml(t, indent),
        EnbxElement::Image(i) => generate_image_xml(i, indent),
        EnbxElement::Shape(s) => generate_shape_xml(s, indent),
        EnbxElement::Path(p) => generate_path_xml(p, indent),
        EnbxElement::Group(g) => generate_group_xml(g, indent),
        EnbxElement::Video(v) => generate_video_xml(v, indent),
        EnbxElement::Audio(a) => generate_audio_xml(a, indent),
        EnbxElement::Cylinder(s) => generate_3d_shape_xml(s, "Cylinder", indent),
        EnbxElement::Cone(s) => generate_3d_shape_xml(s, "Cone", indent),
        EnbxElement::ActivityItem(a) => generate_activity_item_xml(a, indent),
        EnbxElement::Activity(a) => generate_activity_xml(a, indent),
        EnbxElement::Topic(t) => generate_topic_xml(t, indent),
        EnbxElement::Unknown(xv) => xml_value_to_string(xv, indent),
    }
}

fn generate_text_xml(t: &EnbxText, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    out.push_str(&format!("{pad}<Text>\n"));
    write_rect(&mut out, t.x, t.y, t.width, t.height, indent + 1);
    write_tag(&mut out, "FontSize", &fmt_num(t.font_size), indent + 1);
    write_tag(&mut out, "ColorBrush", &t.font_color, indent + 1);
    if t.bold {
        write_tag(&mut out, "Bold", "true", indent + 1);
    }
    if t.italic {
        write_tag(&mut out, "Italic", "true", indent + 1);
    }
    write_tag(
        &mut out,
        "Content",
        &xml_escape_text(&t.content),
        indent + 1,
    );
    out.push_str(&format!("{pad}</Text>\n"));
    out
}

fn generate_image_xml(i: &EnbxImage, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    out.push_str(&format!("{pad}<Image>\n"));
    write_rect(&mut out, i.x, i.y, i.width, i.height, indent + 1);
    write_tag(
        &mut out,
        "Source",
        &xml_escape_text(&i.resource_id),
        indent + 1,
    );
    write_tag(&mut out, "Opacity", &fmt_num(i.opacity), indent + 1);
    out.push_str(&format!("{pad}</Image>\n"));
    out
}

fn generate_shape_xml(s: &EnbxShape, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    out.push_str(&format!(
        "{}<Shape type=\"{}\">\n",
        pad,
        xml_escape_attr(&s.shape_type)
    ));
    write_rect(&mut out, s.x, s.y, s.width, s.height, indent + 1);
    write_tag(&mut out, "FillColor", &s.fill_color, indent + 1);
    write_tag(&mut out, "StrokeColor", &s.stroke_color, indent + 1);
    write_tag(
        &mut out,
        "StrokeWidth",
        &fmt_num(s.stroke_width),
        indent + 1,
    );
    out.push_str(&format!("{pad}</Shape>\n"));
    out
}

fn generate_path_xml(p: &EnbxPath, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    out.push_str(&format!(
        "{}<Path StrokeColor=\"{}\" StrokeWidth=\"{}\">\n",
        pad,
        xml_escape_attr(&p.stroke_color),
        fmt_num(p.stroke_width)
    ));
    let inner_pad = "  ".repeat(indent + 1);
    for (x, y) in &p.points {
        out.push_str(&format!(
            "{}<Point X=\"{}\" Y=\"{}\"/>\n",
            inner_pad,
            fmt_num(*x),
            fmt_num(*y)
        ));
    }
    if let Some(fill) = &p.fill_color {
        write_tag(&mut out, "FillColor", fill, indent + 1);
    }
    out.push_str(&format!("{pad}</Path>\n"));
    out
}

fn generate_group_xml(g: &EnbxGroup, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    out.push_str(&format!("{pad}<Group>\n"));
    write_rect(&mut out, g.x, g.y, g.width, g.height, indent + 1);
    for child in &g.elements {
        out.push_str(&generate_element_xml(child, indent + 1));
    }
    out.push_str(&format!("{pad}</Group>\n"));
    out
}

fn generate_video_xml(v: &EnbxVideo, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    out.push_str(&format!("{pad}<Video>\n"));
    write_rect(&mut out, v.x, v.y, v.width, v.height, indent + 1);
    write_tag(
        &mut out,
        "Source",
        &xml_escape_text(&v.resource_id),
        indent + 1,
    );
    if v.is_loop {
        write_tag(&mut out, "IsLoop", "true", indent + 1);
    }
    if v.is_auto_play {
        write_tag(&mut out, "IsAutoPlay", "true", indent + 1);
    }
    out.push_str(&format!("{pad}</Video>\n"));
    out
}

fn generate_audio_xml(a: &EnbxAudio, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    out.push_str(&format!("{pad}<Audio>\n"));
    write_rect(&mut out, a.x, a.y, a.width, a.height, indent + 1);
    write_tag(
        &mut out,
        "Source",
        &xml_escape_text(&a.resource_id),
        indent + 1,
    );
    if a.is_loop {
        write_tag(&mut out, "IsLoop", "true", indent + 1);
    }
    if a.is_auto_play {
        write_tag(&mut out, "IsAutoPlay", "true", indent + 1);
    }
    if (a.volume - 1.0).abs() > f64::EPSILON {
        write_tag(&mut out, "Volume", &fmt_num(a.volume), indent + 1);
    }
    if a.duration_ms > 0 {
        write_tag(
            &mut out,
            "DurationMs",
            &a.duration_ms.to_string(),
            indent + 1,
        );
    }
    out.push_str(&format!("{pad}</Audio>\n"));
    out
}

fn generate_3d_shape_xml(s: &Enbx3dShape, tag: &str, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    out.push_str(&format!("{pad}<{tag}>\n"));
    write_rect(&mut out, s.x, s.y, s.width, s.height, indent + 1);
    if let Some(t) = &s.transform {
        write_tag(&mut out, "Transform", &xml_escape_text(t), indent + 1);
    }
    out.push_str(&format!("{pad}</{tag}>\n"));
    out
}

fn generate_activity_item_xml(a: &EnbxActivityItem, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    out.push_str(&format!("{pad}<ActivityItem>\n"));
    write_rect(&mut out, a.x, a.y, a.width, a.height, indent + 1);
    write_tag(
        &mut out,
        "ResourceId",
        &xml_escape_text(&a.resource_id),
        indent + 1,
    );
    write_tag(
        &mut out,
        "ActivityId",
        &xml_escape_text(&a.activity_id),
        indent + 1,
    );
    if let Some(bg) = &a.background_source {
        write_tag(
            &mut out,
            "BackgroundSource",
            &xml_escape_text(bg),
            indent + 1,
        );
    }
    if let Some(t) = &a.text_content {
        write_tag(&mut out, "Text", &xml_escape_text(t), indent + 1);
    }
    write_tag(&mut out, "FontSize", &fmt_num(a.font_size), indent + 1);
    write_tag(&mut out, "ForegroundColor", &a.font_color, indent + 1);
    out.push_str(&format!("{pad}</ActivityItem>\n"));
    out
}

fn generate_activity_xml(a: &EnbxActivity, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    out.push_str(&format!(
        "{pad}<Activity type=\"{}\" id=\"{}\">\n",
        xml_escape_attr(&a.key),
        xml_escape_attr(&a.id)
    ));
    write_tag(&mut out, "Name", &xml_escape_text(&a.name), indent + 1);
    write_tag(
        &mut out,
        "Description",
        &xml_escape_text(&a.description),
        indent + 1,
    );
    let cpad = "  ".repeat(indent + 1);
    for c in &a.classifies {
        out.push_str(&format!(
            "{cpad}<Classify id=\"{}\" name=\"{}\">\n",
            xml_escape_attr(&c.id),
            xml_escape_attr(&c.name)
        ));
        let ipad = "  ".repeat(indent + 2);
        for item in &c.items {
            out.push_str(&format!(
                "{ipad}<Item id=\"{}\" name=\"{}\"/>\n",
                xml_escape_attr(&item.id),
                xml_escape_attr(&item.name)
            ));
        }
        out.push_str(&format!("{cpad}</Classify>\n"));
    }
    out.push_str(&format!("{pad}</Activity>\n"));
    out
}

fn generate_topic_xml(t: &EnbxTopic, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    out.push_str(&format!(
        "{pad}<Topic type=\"{}\">\n",
        xml_escape_attr(&t.topic_type)
    ));
    write_rect(
        &mut out,
        t.center_x,
        t.center_y,
        t.center_w,
        t.center_h,
        indent + 1,
    );
    write_tag(
        &mut out,
        "Title",
        &xml_escape_text(&t.center_text),
        indent + 1,
    );
    let npad = "  ".repeat(indent + 1);
    out.push_str(&format!("{npad}<Nodes>\n"));
    let ipad = "  ".repeat(indent + 2);
    for child in &t.children {
        out.push_str(&format!("{ipad}<Node>\n"));
        write_tag(&mut out, "Title", &xml_escape_text(&child.text), indent + 3);
        write_tag(
            &mut out,
            "Location",
            &xml_escape_text(&child.location),
            indent + 3,
        );
        write_tag(
            &mut out,
            "ContentWidth",
            &fmt_num(child.content_width),
            indent + 3,
        );
        write_tag(
            &mut out,
            "ContentHeight",
            &fmt_num(child.content_height),
            indent + 3,
        );
        write_tag(&mut out, "Color", &child.color, indent + 3);
        write_tag(&mut out, "BgColor", &child.bg_color, indent + 3);
        out.push_str(&format!("{ipad}</Node>\n"));
    }
    out.push_str(&format!("{npad}</Nodes>\n"));
    out.push_str(&format!("{pad}</Topic>\n"));
    out
}

// ---------------------------------------------------------------------------
// XML writing helpers
// ---------------------------------------------------------------------------

/// Write `<X>…</X><Y>…</Y><Width>…</Width><Height>…</Height>` block.
fn write_rect(out: &mut String, x: f64, y: f64, w: f64, h: f64, indent: usize) {
    write_tag(out, "X", &fmt_num(x), indent);
    write_tag(out, "Y", &fmt_num(y), indent);
    write_tag(out, "Width", &fmt_num(w), indent);
    write_tag(out, "Height", &fmt_num(h), indent);
}

/// Write a single `<Tag>value</Tag>` line.
fn write_tag(out: &mut String, tag: &str, value: &str, indent: usize) {
    let pad = "  ".repeat(indent);
    out.push_str(&format!("{pad}<{tag}>{value}</{tag}>\n"));
}

/// Format a number for XML output (integers without decimal part).
fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Escape special XML characters in attribute values.
fn xml_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Escape special XML characters in text content.
fn xml_escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_slide_xml;
    use std::io::Read;

    fn make_slide() -> EnbxSlide {
        EnbxSlide {
            size: (1280.0, 720.0),
            background: Some("FFFFFFFF".to_string()),
            elements: vec![
                EnbxElement::Text(EnbxText {
                    x: 100.0,
                    y: 50.0,
                    width: 300.0,
                    height: 80.0,
                    content: "Test text".to_string(),
                    font_size: 24.0,
                    font_color: "FF000000".to_string(),
                    bold: true,
                    italic: false,
                }),
                EnbxElement::Shape(EnbxShape {
                    x: 10.0,
                    y: 10.0,
                    width: 200.0,
                    height: 120.0,
                    shape_type: "rectangle".to_string(),
                    fill_color: "FFE0E0E0".to_string(),
                    stroke_color: "FF404040".to_string(),
                    stroke_width: 2.0,
                    geometry_type: None,
                    path_data: None,
                    line_type: None,
                    arrow_head: None,
                    arrow_tail: None,
                    adjusts: Vec::new(),
                }),
                EnbxElement::Image(EnbxImage {
                    x: 500.0,
                    y: 300.0,
                    width: 400.0,
                    height: 300.0,
                    resource_id: "photo.png".to_string(),
                    opacity: 1.0,
                }),
                EnbxElement::Path(EnbxPath {
                    points: vec![(10.0, 20.0), (30.0, 40.0), (50.0, 60.0)],
                    stroke_color: "FFFF0000".to_string(),
                    stroke_width: 3.0,
                    fill_color: None,
                }),
            ],
        }
    }

    #[test]
    fn generate_slide_xml_structure() {
        let slide = make_slide();
        let xml = generate_slide_xml(&slide);
        assert!(xml.contains("<?xml"));
        assert!(xml.contains("<Slide"));
        assert!(xml.contains("width=\"1280\""));
        assert!(xml.contains("height=\"720\""));
        assert!(xml.contains("<Elements>"));
        assert!(xml.contains("<Text>"));
        assert!(xml.contains("<Shape"));
        assert!(xml.contains("<Image>"));
        assert!(xml.contains("<Path"));
        assert!(xml.contains("Test text"));
    }

    #[test]
    fn generate_and_parse_round_trip() {
        let slide = make_slide();
        let xml = generate_slide_xml(&slide);

        // Parse the generated XML back
        let parsed = parse_slide_xml(&xml, &HashMap::new()).expect("parse generated XML");

        assert_eq!(parsed.size, (1280.0, 720.0));
        assert_eq!(parsed.background.as_deref(), Some("FFFFFFFF"));
        assert_eq!(parsed.elements.len(), 4);

        // Verify text
        match &parsed.elements[0] {
            EnbxElement::Text(t) => {
                assert_eq!(t.x, 100.0);
                assert_eq!(t.y, 50.0);
                assert_eq!(t.content, "Test text");
                assert_eq!(t.font_size, 24.0);
                assert!(t.bold);
            }
            other => panic!("expected Text, got {other:?}"),
        }

        // Verify shape
        match &parsed.elements[1] {
            EnbxElement::Shape(s) => {
                assert_eq!(s.shape_type, "rectangle");
                assert_eq!(s.x, 10.0);
                assert_eq!(s.stroke_width, 2.0);
            }
            other => panic!("expected Shape, got {other:?}"),
        }

        // Verify image
        match &parsed.elements[2] {
            EnbxElement::Image(i) => {
                assert_eq!(i.x, 500.0);
                assert_eq!(i.resource_id, "photo.png");
            }
            other => panic!("expected Image, got {other:?}"),
        }

        // Verify path
        match &parsed.elements[3] {
            EnbxElement::Path(p) => {
                assert_eq!(p.points.len(), 3);
                assert_eq!(p.points[0], (10.0, 20.0));
            }
            other => panic!("expected Path, got {other:?}"),
        }
    }

    #[test]
    fn generate_reference_xml_basic() {
        let refs = vec![
            ("rId1".to_string(), "image1.png".to_string()),
            ("rId2".to_string(), "image2.jpg".to_string()),
        ];
        let xml = generate_reference_xml(&refs);
        assert!(xml.contains("<Reference>"));
        assert!(xml.contains("Id=\"rId1\""));
        assert!(xml.contains("Target=\"Resources/image1.png\""));
        assert!(xml.contains("Id=\"rId2\""));
        assert!(xml.contains("Target=\"Resources/image2.jpg\""));
    }

    #[test]
    fn generate_enbx_creates_valid_zip() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_enbx_generate.enbx");

        let slides = vec![make_slide()];
        generate_enbx(&slides, &HashMap::new(), &path).expect("generate enbx");

        // Verify the file was created and is a valid ZIP
        let file = std::fs::File::open(&path).expect("open generated file");
        let mut archive = zip::ZipArchive::new(file).expect("open zip");

        // Should contain Reference.xml and Slide_1.xml
        let names: Vec<String> = (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
            .collect();
        assert!(names.iter().any(|n| n == "Reference.xml"));
        assert!(names.iter().any(|n| n == "Slide_1.xml"));

        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn generate_enbx_includes_resource_files() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_enbx_resources.enbx");

        let mut slide = make_slide();
        // Add an image with a known resource_id
        slide.elements.push(EnbxElement::Image(EnbxImage {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            resource_id: "test_image.png".to_string(),
            opacity: 1.0,
        }));

        let mut resources = HashMap::new();
        resources.insert("test_image.png".to_string(), b"fake_png_data".to_vec());

        generate_enbx_with_resources(&[slide], &resources, &HashMap::new(), &path)
            .expect("generate");

        let file = std::fs::File::open(&path).expect("open");
        let mut archive = zip::ZipArchive::new(file).expect("zip");

        // Verify resource file is in the archive
        let mut found_resource = false;
        for i in 0..archive.len() {
            let mut f = archive.by_index(i).expect("entry");
            let name = f.name().to_string();
            if name.contains("test_image.png") {
                found_resource = true;
                let mut buf = String::new();
                f.read_to_string(&mut buf).expect("read");
                assert_eq!(buf, "fake_png_data");
            }
        }
        assert!(found_resource, "resource file not found in archive");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn generate_group_xml() {
        let group = EnbxElement::Group(EnbxGroup {
            x: 10.0,
            y: 20.0,
            width: 400.0,
            height: 300.0,
            elements: vec![EnbxElement::Text(EnbxText {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 50.0,
                content: "Grouped".to_string(),
                font_size: 18.0,
                font_color: "FF000000".to_string(),
                bold: false,
                italic: false,
            })],
        });
        let xml = generate_element_xml(&group, 0);
        assert!(xml.contains("<Group>"));
        assert!(xml.contains("<Text>"));
        assert!(xml.contains("Grouped"));
        assert!(xml.contains("</Group>"));
    }

    #[test]
    fn xml_escape_functions() {
        assert_eq!(xml_escape_attr("a&b<c>d\"e"), "a&amp;b&lt;c&gt;d&quot;e");
        assert_eq!(xml_escape_text("a&b<c>d"), "a&amp;b&lt;c&gt;d");
    }

    #[test]
    fn fmt_num_formats() {
        assert_eq!(fmt_num(100.0), "100");
        assert_eq!(fmt_num(100.5), "100.5");
        assert_eq!(fmt_num(0.0), "0");
    }
}
