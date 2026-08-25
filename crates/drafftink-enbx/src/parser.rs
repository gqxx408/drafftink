//! `.enbx` file parsing — reads a ZIP archive and decodes the XML slide data.
//!
//! The parser uses `quick-xml`'s streaming `Reader` for memory-efficient,
//! forward-only parsing.  Unknown element types are preserved as [`XmlValue`]
//! so that round-trip fidelity is maintained.

use std::collections::HashMap;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::{bail, Context, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// EMU helpers (English Metric Units → pixels)
// ---------------------------------------------------------------------------

/// Convert EMU (914 400 per inch) to pixels at 96 DPI.
pub fn emu_to_px(emu: f64) -> f64 {
    emu * 96.0 / 914_400.0
}

/// Default slide dimensions (Seewo standard 16:9).
pub const DEFAULT_SLIDE_WIDTH: f64 = 1280.0;
pub const DEFAULT_SLIDE_HEIGHT: f64 = 720.0;

// ---------------------------------------------------------------------------
// XmlValue — opaque XML node for round-trip preservation
// ---------------------------------------------------------------------------

/// An opaque XML node used to preserve unknown elements for round-trip
/// fidelity.
///
/// When the parser encounters an element type it does not recognise, it stores
/// the full XML subtree in this structure so that the generator can write it
/// back verbatim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlValue {
    /// Element tag name (without namespace prefix).
    pub tag: String,
    /// Attribute key-value pairs.
    pub attributes: HashMap<String, String>,
    /// Concatenated text content (excluding child elements).
    #[serde(default)]
    pub content: String,
    /// Child XML nodes.
    #[serde(default)]
    pub children: Vec<XmlValue>,
}

impl XmlValue {
    /// Create a new empty `XmlValue` with the given tag.
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            attributes: HashMap::new(),
            content: String::new(),
            children: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Enbx element data structs
// ---------------------------------------------------------------------------

/// A text element in the Seewo .enbx format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxText {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub content: String,
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    /// ARGB hex string, e.g. `"FF000000"`.
    #[serde(default = "default_text_color")]
    pub font_color: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
}

fn default_font_size() -> f64 {
    18.0
}
fn default_text_color() -> String {
    "FF000000".to_string()
}

/// An image element in the Seewo .enbx format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxImage {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Resource identifier referencing `Reference.xml`.
    pub resource_id: String,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
}

fn default_opacity() -> f64 {
    1.0
}

/// A shape element in the Seewo .enbx format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxShape {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Shape type tag: `"rectangle"`, `"ellipse"`, `"triangle"`, `"line"`,
    /// `"arrow"`, etc.
    pub shape_type: String,
    /// ARGB hex fill colour, e.g. `"FFE0E0E0"`.
    #[serde(default = "default_fill")]
    pub fill_color: String,
    /// ARGB hex stroke colour, e.g. `"FF404040"`.
    #[serde(default = "default_stroke")]
    pub stroke_color: String,
    #[serde(default = "default_stroke_width")]
    pub stroke_width: f64,
    /// Geometry preset name (e.g. `"Cloud"`, `"SmileyFace"`, `"Bomb"`),
    /// from `<Geometry>/<PresetGeometry|CustomGeometry>/<GeometryType>`.
    #[serde(default)]
    pub geometry_type: Option<String>,
    /// Raw SVG path data (`M…L…C…Q…z`) extracted from a `<Path>` child element.
    /// When present the shape is rendered as a vector path rather than a
    /// primitive.  There may be several `<Path>` children; the first non-empty
    /// one wins.
    #[serde(default)]
    pub path_data: Option<String>,
    /// Line style tag from `<LineType>` (e.g. `"Solid"`, `"Dashed"`).
    #[serde(default)]
    pub line_type: Option<String>,
    /// Arrow decoration at the start of the path/line (`<Line><HeadEnd>`).
    #[serde(default)]
    pub arrow_head: Option<ArrowEnd>,
    /// Arrow decoration at the end of the path/line (`<Line><TailEnd>`).
    #[serde(default)]
    pub arrow_tail: Option<ArrowEnd>,
    /// Geometry adjustment parameters from `<Adjusts><Adjust>`.
    #[serde(default)]
    pub adjusts: Vec<Adjust>,
}

/// Arrow decoration metadata attached to a shape's start/end.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrowEnd {
    /// Arrow style, e.g. `"Triangle"`, `"Stealth"`, `"Diamond"`.
    #[serde(default)]
    pub arrow_type: String,
    /// Width qualifier, e.g. `"Narrow"`, `"Medium"`, `"Wide"`.
    #[serde(default)]
    pub width: String,
    /// Length qualifier, e.g. `"Short"`, `"Medium"`, `"Long"`.
    #[serde(default)]
    pub length: String,
}

/// A single geometry adjustment parameter (`<Adjusts><Adjust>`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Adjust {
    /// Adjustment identifier.
    #[serde(default)]
    pub id: String,
    /// Horizontal adjustment scale.
    #[serde(default)]
    pub scale_x: f64,
    /// Vertical adjustment scale.
    #[serde(default)]
    pub scale_y: f64,
}

fn default_fill() -> String {
    "FFE0E0E0".to_string()
}
fn default_stroke() -> String {
    "FF404040".to_string()
}
fn default_stroke_width() -> f64 {
    1.5
}
fn default_video_volume() -> f64 {
    1.0
}

/// A freehand path element in the Seewo .enbx format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxPath {
    /// Sequence of `(x, y)` points.
    pub points: Vec<(f64, f64)>,
    /// ARGB hex stroke colour.
    #[serde(default = "default_stroke")]
    pub stroke_color: String,
    #[serde(default = "default_stroke_width")]
    pub stroke_width: f64,
    /// Optional ARGB hex fill colour (`None` = no fill).
    #[serde(default)]
    pub fill_color: Option<String>,
}

/// A group of elements in the Seewo .enbx format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxGroup {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub elements: Vec<EnbxElement>,
}

/// A video element in the Seewo .enbx format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxVideo {
    /// Resource identifier referencing `Reference.xml` (may be `id://<id>` or a
    /// bare filename; the parser resolves it through the reference map).
    pub resource_id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub is_loop: bool,
    #[serde(default)]
    pub is_auto_play: bool,
    /// Playback volume in the range `0.0`–`1.0`. Defaults to `1.0`.
    #[serde(default = "default_video_volume")]
    pub volume: f64,
    /// Optional poster/thumbnail resource id (resolved like `resource_id`).
    #[serde(default)]
    pub thumbnail_id: Option<String>,
}

/// A pure-audio element in the Seewo .enbx format.
///
/// Mirrors [`EnbxVideo`] but carries no video track. The host renders a
/// transport-bar overlay; the saved XML only carries the resource reference
/// and control metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxAudio {
    /// Resource identifier referencing `Reference.xml` (resolved through the
    /// reference map, like `EnbxVideo::resource_id`).
    pub resource_id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub is_loop: bool,
    #[serde(default)]
    pub is_auto_play: bool,
    /// Playback volume in the range `0.0`–`1.0`. Defaults to `1.0`.
    #[serde(default = "default_audio_volume")]
    pub volume: f64,
    /// Probed total duration in milliseconds (0 = unknown).
    #[serde(default)]
    pub duration_ms: u64,
}

fn default_audio_volume() -> f64 {
    1.0
}

/// A 3D shape element (`<Cylinder>` / `<Cone>`) in the Seewo .enbx format.
///
/// 3D content cannot be represented in a 2D renderer; the mapper degrades it to
/// a placeholder shape and logs a warning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enbx3dShape {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// 3D transform matrix (16 components, comma-separated), if present.
    #[serde(default)]
    pub transform: Option<String>,
}

/// A classroom activity item (container or material) in the Seewo .enbx format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxActivityItem {
    pub resource_id: String,
    pub activity_id: String,
    /// Background resource reference (e.g. `id://<id>`), if present.
    #[serde(default)]
    pub background_source: Option<String>,
    /// First-level text content, if present.
    #[serde(default)]
    pub text_content: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    /// ARGB hex foreground colour, e.g. `"FF000000"`.
    #[serde(default = "default_text_color")]
    pub font_color: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
}

/// A single option within a classify group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxClassifyItem {
    pub id: String,
    pub name: String,
}

/// A classify group within an activity (e.g. "城市名称").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxClassify {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub items: Vec<EnbxClassifyItem>,
}

/// A classroom activity (e.g. `<Classify>`) in the Seewo .enbx format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxActivity {
    pub id: String,
    /// Activity type key, e.g. `"Classify"`.
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub classifies: Vec<EnbxClassify>,
}

/// A child node (branch) of a [`EnbxTopic`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxTopicNode {
    pub text: String,
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    /// ARGB hex foreground colour, e.g. `"FF000000"`.
    #[serde(default = "default_text_color")]
    pub color: String,
    /// ARGB hex background colour, e.g. `"FFE0E0E0"`.
    #[serde(default = "default_fill")]
    pub bg_color: String,
    /// Relative offset from the topic centre, e.g. `"290.5,-128"`.
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub content_width: f64,
    #[serde(default)]
    pub content_height: f64,
}

/// A topic (mind map / fishbone / organization chart) in the Seewo .enbx format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxTopic {
    /// Topic type: `"MindMap"` / `"FishBoneMap"` / `"Organization"`.
    #[serde(default)]
    pub topic_type: String,
    /// Centre node text.
    #[serde(default)]
    pub center_text: String,
    pub center_x: f64,
    pub center_y: f64,
    pub center_w: f64,
    pub center_h: f64,
    /// Child branch nodes.
    #[serde(default)]
    pub children: Vec<EnbxTopicNode>,
}

/// All possible element types in an .enbx slide.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EnbxElement {
    Text(EnbxText),
    Image(EnbxImage),
    Shape(EnbxShape),
    Path(EnbxPath),
    Group(EnbxGroup),
    /// `<Video>` element.
    Video(EnbxVideo),
    /// `<Audio>` element (pure-audio clip; no video track).
    Audio(EnbxAudio),
    /// `<Cylinder>` 3D shape.
    Cylinder(Enbx3dShape),
    /// `<Cone>` 3D shape.
    Cone(Enbx3dShape),
    /// `<ActivityItem>` classroom activity item.
    ActivityItem(EnbxActivityItem),
    /// `<Activity>` classroom activity configuration.
    Activity(EnbxActivity),
    /// `<Topic>` mind map / fishbone / organization chart.
    Topic(EnbxTopic),
    /// Unknown element type, preserved for round-trip fidelity.
    Unknown(XmlValue),
}

// ---------------------------------------------------------------------------
// Slide & file structs
// ---------------------------------------------------------------------------

/// Metadata extracted from the .enbx archive.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnbxMetadata {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
}

/// A single slide in an .enbx file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnbxSlide {
    pub elements: Vec<EnbxElement>,
    /// Background colour (ARGB hex) or image resource-id.
    #[serde(default)]
    pub background: Option<String>,
    /// Slide dimensions `(width, height)`.
    #[serde(default = "default_slide_size")]
    pub size: (f64, f64),
}

fn default_slide_size() -> (f64, f64) {
    (DEFAULT_SLIDE_WIDTH, DEFAULT_SLIDE_HEIGHT)
}

impl Default for EnbxSlide {
    fn default() -> Self {
        Self {
            elements: Vec::new(),
            background: None,
            size: default_slide_size(),
        }
    }
}

/// A fully parsed .enbx file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnbxFile {
    pub slides: Vec<EnbxSlide>,
    pub metadata: EnbxMetadata,
    /// Resource-id → raw bytes (images, etc.).
    #[serde(default)]
    pub resources: HashMap<String, Vec<u8>>,
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Parse an `.enbx` file from disk.
///
/// The file is opened as a ZIP archive and its XML contents are decoded.
/// Resource files (images, etc.) are loaded into the `resources` map for
/// downstream consumption.
///
/// # Errors
///
/// Returns an error if the file cannot be opened, the ZIP is corrupt, or the
/// XML is malformed beyond recovery.
pub fn parse_enbx(path: &Path) -> Result<EnbxFile> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("failed to open .enbx file: {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to read ZIP archive: {}", path.display()))?;

    // ── Parse Reference.xml ───────────────────────────────────────────
    let ref_map = parse_reference_entry(&mut archive).unwrap_or_default();

    // ── Parse resources ────────────────────────────────────────────────
    let mut resources: HashMap<String, Vec<u8>> = HashMap::new();
    for (id, filename) in &ref_map {
        if let Ok(mut entry) = archive.by_name(filename) {
            let mut buf = Vec::with_capacity(entry.size() as usize);
            if entry.read_to_end(&mut buf).is_ok() {
                resources.insert(id.clone(), buf);
            }
        }
    }

    // ── Discover slide entries ─────────────────────────────────────────
    let slide_entries = list_slide_entries(&mut archive);
    if slide_entries.is_empty() {
        log::warn!("no slide XML entries found in .enbx archive");
        return Ok(EnbxFile {
            slides: vec![EnbxSlide::default()],
            metadata: EnbxMetadata::default(),
            resources,
        });
    }

    // ── Parse slides ───────────────────────────────────────────────────
    let mut slides = Vec::with_capacity(slide_entries.len());
    for entry_name in &slide_entries {
        match parse_slide_entry(&mut archive, entry_name, &ref_map) {
            Ok(slide) => slides.push(slide),
            Err(e) => {
                log::warn!("failed to parse slide {entry_name}: {e}");
                slides.push(EnbxSlide::default());
            }
        }
    }

    // ── Parse metadata (optional) ──────────────────────────────────────
    let metadata = parse_metadata_entry(&mut archive).unwrap_or_default();

    Ok(EnbxFile {
        slides,
        metadata,
        resources,
    })
}

// ---------------------------------------------------------------------------
// Reference.xml parsing
// ---------------------------------------------------------------------------

/// Parse the `Reference.xml` entry into a resource-id → filename map.
fn parse_reference_entry(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> Result<HashMap<String, String>> {
    // Try common names for the reference file.
    let candidates = ["Reference.xml", "reference.xml", "_rels/reference.xml"];
    let mut found: Option<String> = None;
    for c in &candidates {
        if archive.by_name(c).is_ok() {
            found = Some(c.to_string());
            break;
        }
    }
    let name = match found {
        Some(n) => n,
        None => bail!("Reference.xml not found in archive"),
    };

    let entry = archive.by_name(&name)?;
    let mut xml = String::new();
    BufReader::new(entry).read_to_string(&mut xml)?;

    Ok(parse_reference_xml(&xml))
}

/// Parse a `Reference.xml` document string into a map.
fn parse_reference_xml(xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                let lower = local.to_lowercase();
                if matches!(lower.as_str(), "relationship" | "resource" | "ref" | "item") {
                    let mut id = None;
                    let mut target = None;
                    for attr in e.attributes().flatten() {
                        let key = local_name(attr.key.as_ref()).to_lowercase();
                        let val = attr.unescape_value().unwrap_or_default().to_string();
                        match key.as_str() {
                            "id" | "r:id" => id = Some(val),
                            "target" | "file" | "src" | "href" => target = Some(val),
                            _ => {}
                        }
                    }
                    if let (Some(id), Some(target)) = (id, target) {
                        let clean = target
                            .strip_prefix("Resources/")
                            .unwrap_or(&target)
                            .to_string();
                        map.insert(id, clean);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                log::debug!("reference parse error: {e}");
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    map
}

// ---------------------------------------------------------------------------
// Slide XML parsing
// ---------------------------------------------------------------------------

/// Discover slide XML entries in the archive, sorted by index.
fn list_slide_entries(archive: &mut zip::ZipArchive<std::fs::File>) -> Vec<String> {
    let mut entries: Vec<(usize, String)> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(f) = archive.by_index(i) {
            let name = f.name().to_string();
            let lower = name.to_lowercase();
            if lower.contains("slide") && lower.ends_with(".xml") && !lower.contains("reference") {
                let digits: String = lower.chars().filter(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = digits.parse::<usize>() {
                    entries.push((n, name));
                }
            }
        }
    }
    entries.sort_by_key(|(n, _)| *n);
    entries.into_iter().map(|(_, name)| name).collect()
}

/// Parse a single slide entry from the archive.
fn parse_slide_entry(
    archive: &mut zip::ZipArchive<std::fs::File>,
    entry_name: &str,
    ref_map: &HashMap<String, String>,
) -> Result<EnbxSlide> {
    let entry = archive
        .by_name(entry_name)
        .with_context(|| format!("cannot read slide entry: {entry_name}"))?;
    let mut xml = String::new();
    BufReader::new(entry).read_to_string(&mut xml)?;

    parse_slide_xml(&xml, ref_map)
}

/// Parse slide XML content into an [`EnbxSlide`].
pub fn parse_slide_xml(xml: &str, ref_map: &HashMap<String, String>) -> Result<EnbxSlide> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    let mut elements: Vec<EnbxElement> = Vec::new();
    let mut background: Option<String> = None;
    let mut width = DEFAULT_SLIDE_WIDTH;
    let mut height = DEFAULT_SLIDE_HEIGHT;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                let lower = local.to_lowercase();

                match lower.as_str() {
                    // Slide / board dimensions
                    "slide" | "board" | "page" => {
                        for attr in e.attributes().flatten() {
                            let key = local_name(attr.key.as_ref()).to_lowercase();
                            let val = attr.unescape_value().unwrap_or_default().to_string();
                            match key.as_str() {
                                "width" | "boardwidth" => {
                                    if let Ok(v) = val.parse::<f64>() {
                                        // EMU values for screen dimensions are in the
                                        // millions (e.g. 12,192,000 for 1280 px).
                                        // Use 100,000 as a safe threshold — larger than
                                        // any pixel dimension (8K = 7680 px) but far
                                        // below any EMU value.
                                        width = if v > 100_000.0 { emu_to_px(v) } else { v };
                                    }
                                }
                                "height" | "boardheight" => {
                                    if let Ok(v) = val.parse::<f64>() {
                                        height = if v > 100_000.0 { emu_to_px(v) } else { v };
                                    }
                                }
                                "bgcolor" | "backgroundcolor" | "background" => {
                                    background = Some(val);
                                }
                                _ => {}
                            }
                        }
                    }
                    "elements" | "content" => {
                        // Container — children will be parsed individually.
                    }
                    "text" => {
                        if let Ok(elem) = parse_text_element(&mut reader, e) {
                            elements.push(EnbxElement::Text(elem));
                        }
                    }
                    "image" | "picture" | "pic" => {
                        if let Ok(elem) = parse_image_element(&mut reader, e, ref_map) {
                            elements.push(EnbxElement::Image(elem));
                        }
                    }
                    "shape" => {
                        if let Ok(elem) = parse_shape_element(&mut reader, e) {
                            elements.push(EnbxElement::Shape(elem));
                        }
                    }
                    "path" | "freeline" | "ink" => {
                        if let Ok(elem) = parse_path_element(&mut reader, e) {
                            elements.push(EnbxElement::Path(elem));
                        }
                    }
                    "group" => {
                        if let Ok(elem) = parse_group_element(&mut reader, e, ref_map) {
                            elements.push(EnbxElement::Group(elem));
                        }
                    }
                    "video" => {
                        if let Ok(xv) = parse_xml_value(&mut reader, e) {
                            match parse_video(&xv, ref_map) {
                                Ok(v) => elements.push(EnbxElement::Video(v)),
                                Err(err) => log::warn!("failed to parse <Video>: {err}"),
                            }
                        }
                    }
                    "audio" => {
                        if let Ok(xv) = parse_xml_value(&mut reader, e) {
                            match parse_audio(&xv, ref_map) {
                                Ok(a) => elements.push(EnbxElement::Audio(a)),
                                Err(err) => log::warn!("failed to parse <Audio>: {err}"),
                            }
                        }
                    }
                    "cylinder" => {
                        if let Ok(xv) = parse_xml_value(&mut reader, e) {
                            match parse_3d_shape(&xv) {
                                Ok(s) => elements.push(EnbxElement::Cylinder(s)),
                                Err(err) => log::warn!("failed to parse <Cylinder>: {err}"),
                            }
                        }
                    }
                    "cone" => {
                        if let Ok(xv) = parse_xml_value(&mut reader, e) {
                            match parse_3d_shape(&xv) {
                                Ok(s) => elements.push(EnbxElement::Cone(s)),
                                Err(err) => log::warn!("failed to parse <Cone>: {err}"),
                            }
                        }
                    }
                    "activityitem" => {
                        if let Ok(xv) = parse_xml_value(&mut reader, e) {
                            match parse_activity_item(&xv, ref_map) {
                                Ok(a) => elements.push(EnbxElement::ActivityItem(a)),
                                Err(err) => log::warn!("failed to parse <ActivityItem>: {err}"),
                            }
                        }
                    }
                    "activity" => {
                        if let Ok(xv) = parse_xml_value(&mut reader, e) {
                            match parse_activity(&xv) {
                                Ok(a) => elements.push(EnbxElement::Activity(a)),
                                Err(err) => log::warn!("failed to parse <Activity>: {err}"),
                            }
                        }
                    }
                    "topic" => {
                        if let Ok(xv) = parse_xml_value(&mut reader, e) {
                            match parse_topic(&xv) {
                                Ok(t) => elements.push(EnbxElement::Topic(t)),
                                Err(err) => log::warn!("failed to parse <Topic>: {err}"),
                            }
                        }
                    }
                    _ => {
                        // Unknown element — preserve as XmlValue.
                        if let Ok(xv) = parse_xml_value(&mut reader, e) {
                            elements.push(EnbxElement::Unknown(xv));
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                log::debug!("slide parse error: {e}");
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(EnbxSlide {
        elements,
        background,
        size: (width, height),
    })
}

// ---------------------------------------------------------------------------
// Individual element parsers
// ---------------------------------------------------------------------------

/// Parse a `<Text>` element.  The opening tag `e` has already been consumed;
/// this function reads child elements until the matching `</Text>`.
fn parse_text_element(reader: &mut Reader<&[u8]>, start: &BytesStart<'_>) -> Result<EnbxText> {
    let mut text = EnbxText {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 60.0,
        content: String::new(),
        font_size: default_font_size(),
        font_color: default_text_color(),
        bold: false,
        italic: false,
    };

    // Attributes on the opening tag (e.g. <Text Left="100" Top="50" …/>)
    for attr in start.attributes().flatten() {
        let key = local_name(attr.key.as_ref()).to_lowercase();
        let val = attr.unescape_value().unwrap_or_default().to_string();
        set_rect_field(
            &mut text.x,
            &mut text.y,
            &mut text.width,
            &mut text.height,
            &key,
            &val,
        );
    }

    let mut buf = Vec::new();
    let depth = 1u32;
    let mut depth = depth;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                match local.as_str() {
                    "x" | "left" => text.x = read_num(reader, e).unwrap_or(text.x),
                    "y" | "top" => text.y = read_num(reader, e).unwrap_or(text.y),
                    "width" => text.width = read_num(reader, e).unwrap_or(text.width),
                    "height" => text.height = read_num(reader, e).unwrap_or(text.height),
                    "fontsize" | "fontsizeinpt" => {
                        text.font_size = read_num(reader, e).unwrap_or(text.font_size);
                    }
                    "bold" => text.bold = read_bool(reader, e),
                    "italic" => text.italic = read_bool(reader, e),
                    "colorbrush" | "textcolor" | "foreground" | "color" => {
                        if let Some(s) = read_str(reader, e) {
                            text.font_color = normalise_hex(&s);
                        }
                    }
                    "content" | "text" | "richtext" => {
                        if let Some(s) = read_str(reader, e) {
                            text.content = s;
                        }
                    }
                    _ => {
                        // Recurse into unknown children to consume them.
                        let _ = consume_element(reader, e);
                    }
                }
                depth += 1;
            }
            Ok(Event::Empty(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                match local.as_str() {
                    "x" | "left" => {
                        if let Some(v) = attr_num(e, "val").or_else(|| attr_num(e, "value")) {
                            text.x = v;
                        }
                    }
                    "y" | "top" => {
                        if let Some(v) = attr_num(e, "val").or_else(|| attr_num(e, "value")) {
                            text.y = v;
                        }
                    }
                    "width" => {
                        if let Some(v) = attr_num(e, "val").or_else(|| attr_num(e, "value")) {
                            text.width = v;
                        }
                    }
                    "height" => {
                        if let Some(v) = attr_num(e, "val").or_else(|| attr_num(e, "value")) {
                            text.height = v;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                if local == "text" || depth == 0 {
                    break;
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Text(ref t)) => {
                let s = t.unescape().unwrap_or_default().to_string();
                if !s.trim().is_empty() && text.content.is_empty() {
                    text.content = s;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(text)
}

/// Parse an `<Image>` element.
fn parse_image_element(
    reader: &mut Reader<&[u8]>,
    start: &BytesStart<'_>,
    ref_map: &HashMap<String, String>,
) -> Result<EnbxImage> {
    let mut img = EnbxImage {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        resource_id: String::new(),
        opacity: default_opacity(),
    };

    for attr in start.attributes().flatten() {
        let key = local_name(attr.key.as_ref()).to_lowercase();
        let val = attr.unescape_value().unwrap_or_default().to_string();
        set_rect_field(
            &mut img.x,
            &mut img.y,
            &mut img.width,
            &mut img.height,
            &key,
            &val,
        );
        if key == "r:id" || key == "r:embed" || key == "resource" || key == "source" {
            img.resource_id = resolve_resource(&val, ref_map);
        }
    }

    let mut buf = Vec::new();
    let mut depth = 1u32;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                match local.as_str() {
                    "x" | "left" => img.x = read_num(reader, e).unwrap_or(img.x),
                    "y" | "top" => img.y = read_num(reader, e).unwrap_or(img.y),
                    "width" => img.width = read_num(reader, e).unwrap_or(img.width),
                    "height" => img.height = read_num(reader, e).unwrap_or(img.height),
                    "source" | "filename" | "src" | "path" => {
                        if let Some(s) = read_str(reader, e) {
                            img.resource_id = resolve_resource(&s, ref_map);
                        }
                    }
                    "opacity" => img.opacity = read_num(reader, e).unwrap_or(img.opacity),
                    _ => {
                        let _ = consume_element(reader, e);
                    }
                }
                depth += 1;
            }
            Ok(Event::End(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                if local == "image" || local == "picture" || local == "pic" || depth == 0 {
                    break;
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(img)
}

/// Parse a `<Shape>` element.
fn parse_shape_element(reader: &mut Reader<&[u8]>, start: &BytesStart<'_>) -> Result<EnbxShape> {
    let mut shape = EnbxShape {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        shape_type: String::from("rectangle"),
        fill_color: default_fill(),
        stroke_color: default_stroke(),
        stroke_width: default_stroke_width(),
        geometry_type: None,
        path_data: None,
        line_type: None,
        arrow_head: None,
        arrow_tail: None,
        adjusts: Vec::new(),
    };

    for attr in start.attributes().flatten() {
        let key = local_name(attr.key.as_ref()).to_lowercase();
        let val = attr.unescape_value().unwrap_or_default().to_string();
        set_rect_field(
            &mut shape.x,
            &mut shape.y,
            &mut shape.width,
            &mut shape.height,
            &key,
            &val,
        );
        match key.as_str() {
            "type" | "shapetype" => shape.shape_type = val.to_lowercase(),
            "fill" | "fillcolor" => shape.fill_color = normalise_hex(&val),
            "stroke" | "strokecolor" | "linecolor" => shape.stroke_color = normalise_hex(&val),
            "strokewidth" | "linewidth" => {
                if let Ok(v) = val.parse::<f64>() {
                    shape.stroke_width = v;
                }
            }
            _ => {}
        }
    }

    let mut buf = Vec::new();
    let mut depth = 1u32;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                match local.as_str() {
                    "x" | "left" => shape.x = read_num(reader, e).unwrap_or(shape.x),
                    "y" | "top" => shape.y = read_num(reader, e).unwrap_or(shape.y),
                    "width" => shape.width = read_num(reader, e).unwrap_or(shape.width),
                    "height" => shape.height = read_num(reader, e).unwrap_or(shape.height),
                    "type" | "shapetype" => {
                        if let Some(s) = read_str(reader, e) {
                            shape.shape_type = s.to_lowercase();
                        }
                    }
                    "fillcolor" | "fill" => {
                        if let Some(s) = read_str(reader, e) {
                            shape.fill_color = normalise_hex(&s);
                        }
                    }
                    "strokecolor" | "linecolor" | "stroke" => {
                        if let Some(s) = read_str(reader, e) {
                            shape.stroke_color = normalise_hex(&s);
                        }
                    }
                    "strokewidth" | "linewidth" => {
                        shape.stroke_width = read_num(reader, e).unwrap_or(shape.stroke_width);
                    }
                    // --- V4/V5 rich shape geometry (nested containers) ---
                    // Descend into these; their children are parsed below.
                    "geometry" | "presetgeometry" | "customgeometry" | "adjusts" | "line" => {}
                    "geometrytype" => {
                        if let Some(s) = read_str(reader, e) {
                            if shape.geometry_type.is_none() {
                                shape.geometry_type = Some(s);
                            }
                        }
                    }
                    "linetype" => {
                        if let Some(s) = read_str(reader, e) {
                            shape.line_type = Some(s);
                        }
                    }
                    "path" => {
                        // Prefer a `Data`/`d` attribute, fall back to element text.
                        let from_attr = attr_str(e, "data")
                            .or_else(|| attr_str(e, "d"))
                            .or_else(|| attr_str(e, "path"));
                        let from_text = read_str(reader, e);
                        if let Some(v) = from_attr.or(from_text) {
                            if shape.path_data.is_none() {
                                shape.path_data = Some(v);
                            }
                        }
                    }
                    "adjust" => {
                        shape.adjusts.push(parse_adjust(e));
                    }
                    "headend" => {
                        shape.arrow_head = Some(parse_arrow_end(e));
                    }
                    "tailend" => {
                        shape.arrow_tail = Some(parse_arrow_end(e));
                    }
                    _ => {
                        let _ = consume_element(reader, e);
                    }
                }
                depth += 1;
            }
            // Self-closing elements (e.g. `<Adjust id=".." scale-x=".."/>`,
            // `<HeadEnd type="Triangle"/>`) arrive as `Event::Empty` and carry
            // their data purely in attributes — no matching `</…>` End event.
            Ok(Event::Empty(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                match local.as_str() {
                    "adjust" => shape.adjusts.push(parse_adjust(e)),
                    "headend" => shape.arrow_head = Some(parse_arrow_end(e)),
                    "tailend" => shape.arrow_tail = Some(parse_arrow_end(e)),
                    "path" => {
                        if shape.path_data.is_none() {
                            if let Some(v) = attr_str(e, "data")
                                .or_else(|| attr_str(e, "d"))
                                .or_else(|| attr_str(e, "path"))
                            {
                                shape.path_data = Some(v);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                if local == "shape" || depth == 0 {
                    break;
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(shape)
}

/// Parse a `<Path>` / `<FreeLine>` / `<Ink>` element.
fn parse_path_element(reader: &mut Reader<&[u8]>, start: &BytesStart<'_>) -> Result<EnbxPath> {
    let mut path = EnbxPath {
        points: Vec::new(),
        stroke_color: default_stroke(),
        stroke_width: default_stroke_width(),
        fill_color: None,
    };

    for attr in start.attributes().flatten() {
        let key = local_name(attr.key.as_ref()).to_lowercase();
        let val = attr.unescape_value().unwrap_or_default().to_string();
        match key.as_str() {
            "stroke" | "strokecolor" | "linecolor" => path.stroke_color = normalise_hex(&val),
            "strokewidth" | "linewidth" => {
                if let Ok(v) = val.parse::<f64>() {
                    path.stroke_width = v;
                }
            }
            "fill" | "fillcolor" => path.fill_color = Some(normalise_hex(&val)),
            "points" | "data" | "d" => {
                path.points = parse_point_list(&val);
            }
            _ => {}
        }
    }

    let mut buf = Vec::new();
    let mut depth = 1u32;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                match local.as_str() {
                    "point" | "p" => {
                        let mut px = 0.0f64;
                        let mut py = 0.0f64;
                        for attr in e.attributes().flatten() {
                            let k = local_name(attr.key.as_ref()).to_lowercase();
                            let v = attr.unescape_value().unwrap_or_default().to_string();
                            match k.as_str() {
                                "x" => px = v.parse::<f64>().unwrap_or(0.0),
                                "y" => py = v.parse::<f64>().unwrap_or(0.0),
                                _ => {}
                            }
                        }
                        if let Some(s) = read_str(reader, e) {
                            let parts: Vec<&str> = s
                                .split(|c: char| c == ',' || c.is_whitespace())
                                .filter(|s| !s.is_empty())
                                .collect();
                            if parts.len() >= 2 {
                                px = parts[0].parse::<f64>().unwrap_or(px);
                                py = parts[1].parse::<f64>().unwrap_or(py);
                            }
                        }
                        path.points.push((px, py));
                    }
                    "points" | "data" | "d" => {
                        if let Some(s) = read_str(reader, e) {
                            path.points = parse_point_list(&s);
                        }
                    }
                    "strokecolor" | "linecolor" | "stroke" => {
                        if let Some(s) = read_str(reader, e) {
                            path.stroke_color = normalise_hex(&s);
                        }
                    }
                    "strokewidth" | "linewidth" => {
                        path.stroke_width = read_num(reader, e).unwrap_or(path.stroke_width);
                    }
                    "fillcolor" | "fill" => {
                        if let Some(s) = read_str(reader, e) {
                            path.fill_color = Some(normalise_hex(&s));
                        }
                    }
                    _ => {
                        let _ = consume_element(reader, e);
                    }
                }
                depth += 1;
            }
            Ok(Event::Empty(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                match local.as_str() {
                    "point" | "p" => {
                        let px = attr_num(e, "x").unwrap_or(0.0);
                        let py = attr_num(e, "y").unwrap_or(0.0);
                        path.points.push((px, py));
                    }
                    "strokecolor" | "linecolor" | "stroke" => {
                        if let Some(v) = attr_str(e, "val").or_else(|| attr_str(e, "value")) {
                            path.stroke_color = normalise_hex(&v);
                        }
                    }
                    "strokewidth" | "linewidth" => {
                        if let Some(v) = attr_num(e, "val").or_else(|| attr_num(e, "value")) {
                            path.stroke_width = v;
                        }
                    }
                    "fillcolor" | "fill" => {
                        if let Some(v) = attr_str(e, "val").or_else(|| attr_str(e, "value")) {
                            path.fill_color = Some(normalise_hex(&v));
                        }
                    }
                    _ => {}
                }
                // Do NOT increment depth for Empty events — there is no
                // corresponding End event.
            }
            Ok(Event::End(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                if matches!(local.as_str(), "path" | "freeline" | "ink") || depth == 0 {
                    break;
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(path)
}

/// Parse a `<Group>` element.
fn parse_group_element(
    reader: &mut Reader<&[u8]>,
    start: &BytesStart<'_>,
    ref_map: &HashMap<String, String>,
) -> Result<EnbxGroup> {
    let mut group = EnbxGroup {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 200.0,
        elements: Vec::new(),
    };

    for attr in start.attributes().flatten() {
        let key = local_name(attr.key.as_ref()).to_lowercase();
        let val = attr.unescape_value().unwrap_or_default().to_string();
        set_rect_field(
            &mut group.x,
            &mut group.y,
            &mut group.width,
            &mut group.height,
            &key,
            &val,
        );
    }

    let mut buf = Vec::new();
    let mut depth = 1u32;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                match local.as_str() {
                    "x" | "left" => group.x = read_num(reader, e).unwrap_or(group.x),
                    "y" | "top" => group.y = read_num(reader, e).unwrap_or(group.y),
                    "width" => group.width = read_num(reader, e).unwrap_or(group.width),
                    "height" => group.height = read_num(reader, e).unwrap_or(group.height),
                    "text" => {
                        if let Ok(elem) = parse_text_element(reader, e) {
                            group.elements.push(EnbxElement::Text(elem));
                        }
                    }
                    "image" | "picture" | "pic" => {
                        if let Ok(elem) = parse_image_element(reader, e, ref_map) {
                            group.elements.push(EnbxElement::Image(elem));
                        }
                    }
                    "shape" => {
                        if let Ok(elem) = parse_shape_element(reader, e) {
                            group.elements.push(EnbxElement::Shape(elem));
                        }
                    }
                    "path" | "freeline" | "ink" => {
                        if let Ok(elem) = parse_path_element(reader, e) {
                            group.elements.push(EnbxElement::Path(elem));
                        }
                    }
                    "group" => {
                        if let Ok(elem) = parse_group_element(reader, e, ref_map) {
                            group.elements.push(EnbxElement::Group(elem));
                        }
                    }
                    "audio" => {
                        if let Ok(xv) = parse_xml_value(reader, e) {
                            match parse_audio(&xv, ref_map) {
                                Ok(a) => group.elements.push(EnbxElement::Audio(a)),
                                Err(err) => log::warn!("failed to parse <Audio>: {err}"),
                            }
                        }
                    }
                    _ => {
                        if let Ok(xv) = parse_xml_value(reader, e) {
                            group.elements.push(EnbxElement::Unknown(xv));
                        }
                    }
                }
                depth += 1;
            }
            Ok(Event::End(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                if local == "group" || depth == 0 {
                    break;
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(group)
}

// ---------------------------------------------------------------------------
// XmlValue field-access helpers (used by the Video/3D/Activity/Topic parsers)
// ---------------------------------------------------------------------------

/// Case-insensitive attribute lookup on an [`XmlValue`].
fn xml_attr(xv: &XmlValue, key: &str) -> Option<String> {
    xv.attributes
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.clone())
}

/// Find a direct child element by (case-insensitive) tag name.
fn xml_child<'a>(xv: &'a XmlValue, tag: &str) -> Option<&'a XmlValue> {
    xv.children.iter().find(|c| c.tag.eq_ignore_ascii_case(tag))
}

/// Extract a numeric field from an attribute or a direct child's text content.
fn xml_num(xv: &XmlValue, key: &str) -> Option<f64> {
    if let Some(v) = xml_attr(xv, key) {
        if let Ok(n) = v.trim().parse::<f64>() {
            return Some(n);
        }
    }
    if let Some(c) = xml_child(xv, key) {
        if let Ok(n) = c.content.trim().parse::<f64>() {
            return Some(n);
        }
    }
    None
}

/// Extract a string field from an attribute or a direct child's text.
///
/// If the child element has no direct text but a single child (e.g.
/// `<Title><Text>..</Text></Title>`), the nested text is returned.
fn xml_str(xv: &XmlValue, key: &str) -> Option<String> {
    if let Some(v) = xml_attr(xv, key) {
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    if let Some(c) = xml_child(xv, key) {
        let t = c.content.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
        if let Some(gc) = c.children.first() {
            let gt = gc.content.trim();
            if !gt.is_empty() {
                return Some(gt.to_string());
            }
        }
    }
    None
}

/// Extract a boolean field (`true` / `1` / `yes`).
fn xml_bool(xv: &XmlValue, key: &str) -> bool {
    xml_str(xv, key)
        .map(|s| matches!(s.to_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

/// Convert a coordinate value that may be in EMU (very large) to pixels.
fn emu_or_raw(n: f64) -> f64 {
    if n > 100_000.0 {
        emu_to_px(n)
    } else {
        n
    }
}

/// Resolve a media source reference (`id://<id>` or bare filename) through the
/// reference map.  Falls back to the bare id/filename if unresolved.
fn resolve_media_source(val: &str, ref_map: &HashMap<String, String>) -> String {
    let bare = val.strip_prefix("id://").unwrap_or(val);
    resolve_resource(bare, ref_map)
}

// ---------------------------------------------------------------------------
// Video / 3D / Activity / Topic parsers
// ---------------------------------------------------------------------------

/// Parse a `<Video>` element subtree into [`EnbxVideo`].
fn parse_video(xv: &XmlValue, ref_map: &HashMap<String, String>) -> Result<EnbxVideo> {
    let mut v = EnbxVideo {
        resource_id: String::new(),
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 150.0,
        is_loop: false,
        is_auto_play: false,
        volume: default_video_volume(),
        thumbnail_id: None,
    };
    if let Some(s) = xml_str(xv, "Source").or_else(|| xml_str(xv, "MediaName")) {
        v.resource_id = resolve_media_source(&s, ref_map);
    }
    if let Some(n) = xml_num(xv, "X").or_else(|| xml_num(xv, "Left")) {
        v.x = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "Y").or_else(|| xml_num(xv, "Top")) {
        v.y = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "Width").or_else(|| xml_num(xv, "Cx")) {
        v.width = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "Height").or_else(|| xml_num(xv, "Cy")) {
        v.height = emu_or_raw(n);
    }
    v.is_loop = xml_bool(xv, "IsLoop") || xml_bool(xv, "Loop");
    v.is_auto_play = xml_bool(xv, "IsAutoPlay") || xml_bool(xv, "AutoPlay");

    // Volume is expressed 0.0–1.0 in the internal model; clamp defensively.
    if let Some(vol) = xml_num(xv, "Volume").or_else(|| xml_num(xv, "Vol")) {
        v.volume = vol.clamp(0.0, 1.0);
    }

    // Thumbnail / poster may be a bare id or an `id://<id>` reference.
    if let Some(t) = xml_str(xv, "ThumbnailId")
        .or_else(|| xml_str(xv, "Thumbnail"))
        .or_else(|| xml_str(xv, "Poster"))
    {
        let resolved = resolve_media_source(&t, ref_map);
        if !resolved.is_empty() {
            v.thumbnail_id = Some(resolved);
        }
    }
    Ok(v)
}

/// Parse an `<Audio>` subtree into [`EnbxAudio`].
///
/// Mirrors [`parse_video`] but drops video-only fields (no thumbnail, no
/// play-points). The `DurationMs` element is optional — many exported files
/// omit it.
fn parse_audio(xv: &XmlValue, ref_map: &HashMap<String, String>) -> Result<EnbxAudio> {
    let mut a = EnbxAudio {
        resource_id: String::new(),
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 48.0,
        is_loop: false,
        is_auto_play: false,
        volume: default_audio_volume(),
        duration_ms: 0,
    };
    if let Some(s) = xml_str(xv, "Source").or_else(|| xml_str(xv, "MediaName")) {
        a.resource_id = resolve_media_source(&s, ref_map);
    }
    if let Some(n) = xml_num(xv, "X").or_else(|| xml_num(xv, "Left")) {
        a.x = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "Y").or_else(|| xml_num(xv, "Top")) {
        a.y = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "Width").or_else(|| xml_num(xv, "Cx")) {
        a.width = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "Height").or_else(|| xml_num(xv, "Cy")) {
        a.height = emu_or_raw(n);
    }
    a.is_loop = xml_bool(xv, "IsLoop") || xml_bool(xv, "Loop");
    a.is_auto_play = xml_bool(xv, "IsAutoPlay") || xml_bool(xv, "AutoPlay");
    if let Some(vol) = xml_num(xv, "Volume").or_else(|| xml_num(xv, "Vol")) {
        a.volume = vol.clamp(0.0, 1.0);
    }
    if let Some(d) = xml_num(xv, "DurationMs") {
        a.duration_ms = d.max(0.0) as u64;
    }
    Ok(a)
}

/// Parse a `<Cylinder>` / `<Cone>` 3D shape subtree into [`Enbx3dShape`].
fn parse_3d_shape(xv: &XmlValue) -> Result<Enbx3dShape> {
    let mut s = Enbx3dShape {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        transform: None,
    };
    if let Some(n) = xml_num(xv, "X").or_else(|| xml_num(xv, "Left")) {
        s.x = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "Y").or_else(|| xml_num(xv, "Top")) {
        s.y = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "Width").or_else(|| xml_num(xv, "Cx")) {
        s.width = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "Height").or_else(|| xml_num(xv, "Cy")) {
        s.height = emu_or_raw(n);
    }
    s.transform = xml_str(xv, "Transform");
    Ok(s)
}

/// Parse an `<ActivityItem>` subtree into [`EnbxActivityItem`].
fn parse_activity_item(
    xv: &XmlValue,
    ref_map: &HashMap<String, String>,
) -> Result<EnbxActivityItem> {
    let mut a = EnbxActivityItem {
        resource_id: String::new(),
        activity_id: String::new(),
        background_source: None,
        text_content: None,
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 100.0,
        font_size: default_font_size(),
        font_color: default_text_color(),
        bold: false,
        italic: false,
    };
    a.resource_id = xml_str(xv, "ResourceId").unwrap_or_default();
    a.activity_id = xml_str(xv, "ActivityId").unwrap_or_default();
    a.background_source =
        xml_str(xv, "BackgroundSource").map(|s| resolve_media_source(&s, ref_map));
    a.text_content = xml_str(xv, "Text")
        .or_else(|| xml_str(xv, "Content"))
        .or_else(|| xml_str(xv, "RichText"));
    if let Some(n) = xml_num(xv, "X").or_else(|| xml_num(xv, "Left")) {
        a.x = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "Y").or_else(|| xml_num(xv, "Top")) {
        a.y = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "Width").or_else(|| xml_num(xv, "Cx")) {
        a.width = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "Height").or_else(|| xml_num(xv, "Cy")) {
        a.height = emu_or_raw(n);
    }
    if let Some(n) = xml_num(xv, "FontSize") {
        a.font_size = n;
    }
    if let Some(c) = xml_str(xv, "ForegroundColor").or_else(|| xml_str(xv, "Color")) {
        a.font_color = normalise_hex(&c);
    }
    a.bold = xml_bool(xv, "Bold");
    a.italic = xml_bool(xv, "Italic");
    Ok(a)
}

/// Parse an `<Activity>` (e.g. `<Classify>`) subtree into [`EnbxActivity`].
fn parse_activity(xv: &XmlValue) -> Result<EnbxActivity> {
    let key = xml_attr(xv, "type")
        .or_else(|| xml_attr(xv, "key"))
        .or_else(|| xml_str(xv, "Type"))
        .unwrap_or_else(|| xv.tag.clone())
        .to_string();
    let id = xml_attr(xv, "id")
        .or_else(|| xml_str(xv, "Id"))
        .unwrap_or_default()
        .to_string();
    let name = xml_str(xv, "Name").unwrap_or_default();
    let description = xml_str(xv, "Description").unwrap_or_default();

    let mut classifies: Vec<EnbxClassify> = Vec::new();
    for classify in xv
        .children
        .iter()
        .filter(|c| c.tag.eq_ignore_ascii_case("Classify"))
    {
        let cid = xml_attr(classify, "id")
            .or_else(|| xml_str(classify, "Id"))
            .unwrap_or_default()
            .to_string();
        let cname = xml_str(classify, "Name").unwrap_or_default();

        let mut items: Vec<EnbxClassifyItem> = Vec::new();
        // Options may be nested in <Items> or listed directly under <Classify>.
        for item in classify
            .children
            .iter()
            .filter(|c| c.tag.eq_ignore_ascii_case("Item"))
        {
            items.push(EnbxClassifyItem {
                id: xml_attr(item, "id")
                    .or_else(|| xml_str(item, "Id"))
                    .unwrap_or_default()
                    .to_string(),
                name: xml_str(item, "Name").unwrap_or_default(),
            });
        }
        for container in classify
            .children
            .iter()
            .filter(|c| c.tag.eq_ignore_ascii_case("Items"))
        {
            for item in container
                .children
                .iter()
                .filter(|c| c.tag.eq_ignore_ascii_case("Item"))
            {
                items.push(EnbxClassifyItem {
                    id: xml_attr(item, "id")
                        .or_else(|| xml_str(item, "Id"))
                        .unwrap_or_default()
                        .to_string(),
                    name: xml_str(item, "Name").unwrap_or_default(),
                });
            }
        }
        classifies.push(EnbxClassify {
            id: cid,
            name: cname,
            items,
        });
    }

    Ok(EnbxActivity {
        id,
        key,
        name,
        description,
        classifies,
    })
}

/// Parse a `<Topic>` subtree into [`EnbxTopic`].
fn parse_topic(xv: &XmlValue) -> Result<EnbxTopic> {
    let topic_type = xml_attr(xv, "type")
        .or_else(|| xml_str(xv, "Type"))
        .unwrap_or_default()
        .to_string();

    // Centre text: <Title>text</Title> or <Title><Text>text</Text></Title>.
    let center_text = xml_child(xv, "Title")
        .map(|title| {
            if !title.content.trim().is_empty() {
                title.content.trim().to_string()
            } else if let Some(t) = xml_child(title, "Text") {
                t.content.trim().to_string()
            } else {
                String::new()
            }
        })
        .unwrap_or_default();

    let center_x = xml_num(xv, "X")
        .or_else(|| xml_num(xv, "Left"))
        .map(emu_or_raw)
        .unwrap_or(0.0);
    let center_y = xml_num(xv, "Y")
        .or_else(|| xml_num(xv, "Top"))
        .map(emu_or_raw)
        .unwrap_or(0.0);
    let center_w = xml_num(xv, "Width")
        .or_else(|| xml_num(xv, "Cx"))
        .map(emu_or_raw)
        .unwrap_or(300.0);
    let center_h = xml_num(xv, "Height")
        .or_else(|| xml_num(xv, "Cy"))
        .map(emu_or_raw)
        .unwrap_or(200.0);

    // Child nodes live under <Nodes> or directly under <Topic>.
    let mut child_sources: Vec<&XmlValue> = Vec::new();
    if let Some(nodes) = xml_child(xv, "Nodes") {
        child_sources.extend(
            nodes
                .children
                .iter()
                .filter(|c| c.tag.eq_ignore_ascii_case("Node")),
        );
    }
    child_sources.extend(
        xv.children
            .iter()
            .filter(|c| c.tag.eq_ignore_ascii_case("Node")),
    );

    let mut children: Vec<EnbxTopicNode> = Vec::new();
    for node in child_sources {
        let text = xml_child(node, "Title")
            .map(|title| {
                if !title.content.trim().is_empty() {
                    title.content.trim().to_string()
                } else if let Some(t) = xml_child(title, "Text") {
                    t.content.trim().to_string()
                } else {
                    String::new()
                }
            })
            .unwrap_or_default();
        let font_size = xml_num(node, "FontSize").unwrap_or_else(default_font_size);
        let color = xml_str(node, "Color")
            .or_else(|| xml_str(node, "ForegroundColor"))
            .map(|s| normalise_hex(&s))
            .unwrap_or_else(default_text_color);
        let bg_color = xml_str(node, "BgColor")
            .or_else(|| xml_str(node, "BackgroundColor"))
            .or_else(|| xml_str(node, "FillColor"))
            .map(|s| normalise_hex(&s))
            .unwrap_or_else(default_fill);
        let location = xml_str(node, "Location").unwrap_or_default();
        let content_width = xml_num(node, "ContentWidth").unwrap_or(0.0);
        let content_height = xml_num(node, "ContentHeight").unwrap_or(0.0);
        children.push(EnbxTopicNode {
            text,
            font_size,
            color,
            bg_color,
            location,
            content_width,
            content_height,
        });
    }

    Ok(EnbxTopic {
        topic_type,
        center_text,
        center_x,
        center_y,
        center_w,
        center_h,
        children,
    })
}

// ---------------------------------------------------------------------------
// XmlValue recursive parser (for unknown elements)
// ---------------------------------------------------------------------------

/// Recursively parse an unknown XML element into an [`XmlValue`].
fn parse_xml_value(reader: &mut Reader<&[u8]>, start: &BytesStart<'_>) -> Result<XmlValue> {
    let tag = local_name(start.name().as_ref()).to_string();
    let mut attributes = HashMap::new();
    for attr in start.attributes().flatten() {
        let key = local_name(attr.key.as_ref()).to_string();
        let val = attr.unescape_value().unwrap_or_default().to_string();
        attributes.insert(key, val);
    }

    let mut content = String::new();
    let mut children = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let child = parse_xml_value(reader, e)?;
                children.push(child);
            }
            Ok(Event::Empty(ref e)) => {
                let child_tag = local_name(e.name().as_ref()).to_string();
                let mut child_attrs = HashMap::new();
                for attr in e.attributes().flatten() {
                    let k = local_name(attr.key.as_ref()).to_string();
                    let v = attr.unescape_value().unwrap_or_default().to_string();
                    child_attrs.insert(k, v);
                }
                children.push(XmlValue {
                    tag: child_tag,
                    attributes: child_attrs,
                    content: String::new(),
                    children: Vec::new(),
                });
            }
            Ok(Event::Text(ref t)) => {
                let s = t.unescape().unwrap_or_default().to_string();
                if !s.trim().is_empty() {
                    content.push_str(&s);
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local.eq_ignore_ascii_case(&tag) {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(XmlValue {
        tag,
        attributes,
        content,
        children,
    })
}

// ---------------------------------------------------------------------------
// Metadata parsing
// ---------------------------------------------------------------------------

/// Parse optional metadata from the archive (e.g. `Document.xml`).
fn parse_metadata_entry(archive: &mut zip::ZipArchive<std::fs::File>) -> Result<EnbxMetadata> {
    let candidates = [
        "Document.xml",
        "document.xml",
        "Metadata.xml",
        "metadata.xml",
    ];
    let mut found: Option<String> = None;
    for c in &candidates {
        if archive.by_name(c).is_ok() {
            found = Some(c.to_string());
            break;
        }
    }
    let name = match found {
        Some(n) => n,
        None => bail!("metadata entry not found"),
    };

    let entry = archive.by_name(&name)?;
    let mut xml = String::new();
    BufReader::new(entry).read_to_string(&mut xml)?;

    let mut meta = EnbxMetadata::default();
    let mut reader = Reader::from_str(&xml);
    let mut buf = Vec::new();
    let mut in_title = false;
    let mut in_author = false;
    let mut in_version = false;
    let mut in_created = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                match local.as_str() {
                    "title" => in_title = true,
                    "author" | "creator" => in_author = true,
                    "version" => in_version = true,
                    "created" | "creationdate" | "date" => in_created = true,
                    _ => {}
                }
            }
            Ok(Event::Text(ref t)) => {
                let s = t.unescape().unwrap_or_default().to_string();
                if in_title {
                    meta.title = Some(s);
                    in_title = false;
                } else if in_author {
                    meta.author = Some(s);
                    in_author = false;
                } else if in_version {
                    meta.version = Some(s);
                    in_version = false;
                } else if in_created {
                    meta.created = Some(s);
                    in_created = false;
                }
            }
            Ok(Event::End(ref e)) => {
                let local = local_name(e.name().as_ref()).to_lowercase();
                match local.as_str() {
                    "title" => in_title = false,
                    "author" | "creator" => in_author = false,
                    "version" => in_version = false,
                    "created" | "creationdate" | "date" => in_created = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(meta)
}

// ---------------------------------------------------------------------------
// Low-level helpers
// ---------------------------------------------------------------------------

/// Extract the local part of an XML name, stripping any namespace prefix.
fn local_name(bytes: &[u8]) -> &str {
    let s = std::str::from_utf8(bytes).unwrap_or("");
    // Strip namespace prefix: "p:sp" → "sp"
    match s.rfind(':') {
        Some(pos) => &s[pos + 1..],
        None => s,
    }
}

/// Read the text content of a child element as an `f64`.
fn read_num(reader: &mut Reader<&[u8]>, start: &BytesStart<'_>) -> Option<f64> {
    read_str(reader, start).and_then(|s| s.trim().parse::<f64>().ok())
}

/// Read the text content of a child element as a `bool`.
fn read_bool(reader: &mut Reader<&[u8]>, start: &BytesStart<'_>) -> bool {
    read_str(reader, start)
        .map(|s| matches!(s.trim().to_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

/// Read the text content of a child element as a `String`.
fn read_str(reader: &mut Reader<&[u8]>, start: &BytesStart<'_>) -> Option<String> {
    let tag = local_name(start.name().as_ref()).to_string();
    let mut result = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(ref t)) => {
                result.push_str(&t.unescape().unwrap_or_default());
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local.eq_ignore_ascii_case(&tag) {
                    break;
                }
            }
            Ok(Event::Start(_)) => {
                // Nested element — skip it but keep collecting text.
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    let trimmed = result.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Consume a child element and all its descendants without storing anything.
fn consume_element(reader: &mut Reader<&[u8]>, start: &BytesStart<'_>) -> Result<()> {
    let tag = local_name(start.name().as_ref()).to_string();
    let mut buf = Vec::new();
    let mut depth = 1u32;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                depth = depth.saturating_sub(1);
                if depth == 0 || local.eq_ignore_ascii_case(&tag) {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(())
}

/// Extract a numeric attribute value from an element.
fn attr_num(e: &BytesStart<'_>, name: &str) -> Option<f64> {
    for attr in e.attributes().flatten() {
        if local_name(attr.key.as_ref()).eq_ignore_ascii_case(name) {
            let v = attr.unescape_value().ok()?.to_string();
            return v.parse::<f64>().ok();
        }
    }
    None
}

/// Extract a string attribute value from an element.
fn attr_str(e: &BytesStart<'_>, name: &str) -> Option<String> {
    for attr in e.attributes().flatten() {
        if local_name(attr.key.as_ref()).eq_ignore_ascii_case(name) {
            return attr.unescape_value().ok().map(|s| s.to_string());
        }
    }
    None
}

/// Parse arrow-decoration attributes from a `<HeadEnd>`/`<TailEnd>` element.
///
/// Tolerates a few common attribute spellings (e.g. `Type` / `ArrowType` /
/// `arrowtype`) so that differently-namespaced ENBX exports still parse.
fn parse_arrow_end(e: &BytesStart<'_>) -> ArrowEnd {
    ArrowEnd {
        arrow_type: attr_str(e, "type")
            .or_else(|| attr_str(e, "arrowtype"))
            .or_else(|| attr_str(e, "arrow_type"))
            .unwrap_or_default(),
        width: attr_str(e, "width")
            .or_else(|| attr_str(e, "w"))
            .or_else(|| attr_str(e, "arrowwidth"))
            .unwrap_or_default(),
        length: attr_str(e, "length")
            .or_else(|| attr_str(e, "len"))
            .or_else(|| attr_str(e, "arrowlength"))
            .unwrap_or_default(),
    }
}

/// Parse a single `<Adjust>` element's attributes into an [`Adjust`].
fn parse_adjust(e: &BytesStart<'_>) -> Adjust {
    let scale_x = attr_num(e, "scale-x")
        .or_else(|| attr_num(e, "scale_x"))
        .or_else(|| attr_num(e, "scalex"))
        .unwrap_or(0.0);
    let scale_y = attr_num(e, "scale-y")
        .or_else(|| attr_num(e, "scale_y"))
        .or_else(|| attr_num(e, "scaley"))
        .unwrap_or(0.0);
    Adjust {
        id: attr_str(e, "id").unwrap_or_default(),
        scale_x,
        scale_y,
    }
}

/// Set one of the `x / y / width / height` fields from an attribute.
fn set_rect_field(x: &mut f64, y: &mut f64, w: &mut f64, h: &mut f64, key: &str, val: &str) {
    match key {
        "x" | "left" => {
            if let Ok(v) = val.parse::<f64>() {
                *x = if v > 100_000.0 { emu_to_px(v) } else { v };
            }
        }
        "y" | "top" => {
            if let Ok(v) = val.parse::<f64>() {
                *y = if v > 100_000.0 { emu_to_px(v) } else { v };
            }
        }
        "width" | "cx" => {
            if let Ok(v) = val.parse::<f64>() {
                *w = if v > 100_000.0 { emu_to_px(v) } else { v };
            }
        }
        "height" | "cy" => {
            if let Ok(v) = val.parse::<f64>() {
                *h = if v > 100_000.0 { emu_to_px(v) } else { v };
            }
        }
        _ => {}
    }
}

/// Parse a point list string like `"10,20 30,40 50,60"` or `"10 20 30 40"`.
fn parse_point_list(s: &str) -> Vec<(f64, f64)> {
    let nums: Vec<f64> = s
        .split(|c: char| c == ',' || c.is_whitespace() || c == ';')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    nums.chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| (c[0], c[1]))
        .collect()
}

/// Normalise a hex colour string to 8-digit ARGB (no `#` prefix).
fn normalise_hex(s: &str) -> String {
    let s = s.trim().trim_start_matches('#');
    match s.len() {
        6 => format!("FF{}", s.to_uppercase()),
        8 => s.to_uppercase(),
        _ => "FF000000".to_string(),
    }
}

/// Resolve a resource reference through the reference map.
fn resolve_resource(val: &str, ref_map: &HashMap<String, String>) -> String {
    if let Some(filename) = ref_map.get(val) {
        return filename.clone();
    }
    val.to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_slide_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Slide width="1280" height="720" bgColor="FFFFFFFF">
  <Elements>
    <Text>
      <X>100</X>
      <Y>50</Y>
      <Width>300</Width>
      <Height>80</Height>
      <FontSize>24</FontSize>
      <ColorBrush>FF0000FF</ColorBrush>
      <Content>Hello, Seewo!</Content>
    </Text>
    <Shape type="rectangle">
      <X>10</X>
      <Y>10</Y>
      <Width>200</Width>
      <Height>120</Height>
      <FillColor>FFE0E0E0</FillColor>
      <StrokeColor>FF404040</StrokeColor>
      <StrokeWidth>2</StrokeWidth>
    </Shape>
    <Image>
      <X>500</X>
      <Y>300</Y>
      <Width>400</Width>
      <Height>300</Height>
      <Source>img/photo.png</Source>
    </Image>
  </Elements>
</Slide>"#;

        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        assert_eq!(slide.size, (1280.0, 720.0));
        assert_eq!(slide.background.as_deref(), Some("FFFFFFFF"));
        assert_eq!(slide.elements.len(), 3);

        // Text element
        match &slide.elements[0] {
            EnbxElement::Text(t) => {
                assert_eq!(t.x, 100.0);
                assert_eq!(t.y, 50.0);
                assert_eq!(t.width, 300.0);
                assert_eq!(t.height, 80.0);
                assert_eq!(t.content, "Hello, Seewo!");
                assert_eq!(t.font_size, 24.0);
                assert_eq!(t.font_color, "FF0000FF");
            }
            other => panic!("expected Text, got {other:?}"),
        }

        // Shape element
        match &slide.elements[1] {
            EnbxElement::Shape(s) => {
                assert_eq!(s.shape_type, "rectangle");
                assert_eq!(s.x, 10.0);
                assert_eq!(s.width, 200.0);
                assert_eq!(s.fill_color, "FFE0E0E0");
                assert_eq!(s.stroke_width, 2.0);
            }
            other => panic!("expected Shape, got {other:?}"),
        }

        // Image element
        match &slide.elements[2] {
            EnbxElement::Image(i) => {
                assert_eq!(i.x, 500.0);
                assert_eq!(i.width, 400.0);
                assert_eq!(i.resource_id, "img/photo.png");
            }
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[test]
    fn parse_unknown_element_preserved() {
        let xml = r#"<Slide>
  <Elements>
    <CustomWidget foo="bar" baz="42">
      <Inner>text content</Inner>
    </CustomWidget>
  </Elements>
</Slide>"#;

        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        assert_eq!(slide.elements.len(), 1);
        match &slide.elements[0] {
            EnbxElement::Unknown(xv) => {
                assert_eq!(xv.tag, "CustomWidget");
                assert_eq!(xv.attributes.get("foo").map(|s| s.as_str()), Some("bar"));
                assert_eq!(xv.attributes.get("baz").map(|s| s.as_str()), Some("42"));
                assert_eq!(xv.children.len(), 1);
                assert_eq!(xv.children[0].tag, "Inner");
                assert_eq!(xv.children[0].content, "text content");
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn parse_group_element() {
        let xml = r#"<Slide>
  <Elements>
    <Group X="10" Y="20" Width="400" Height="300">
      <Text>
        <X>0</X><Y>0</Y><Width>100</Width><Height>50</Height>
        <Content>Grouped text</Content>
      </Text>
      <Shape type="ellipse">
        <X>50</X><Y>50</Y><Width>80</Width><Height>80</Height>
      </Shape>
    </Group>
  </Elements>
</Slide>"#;

        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        assert_eq!(slide.elements.len(), 1);
        match &slide.elements[0] {
            EnbxElement::Group(g) => {
                assert_eq!(g.x, 10.0);
                assert_eq!(g.y, 20.0);
                assert_eq!(g.elements.len(), 2);
                assert!(matches!(g.elements[0], EnbxElement::Text(_)));
                assert!(matches!(g.elements[1], EnbxElement::Shape(_)));
            }
            other => panic!("expected Group, got {other:?}"),
        }
    }

    #[test]
    fn parse_path_element() {
        let xml = r#"<Slide>
  <Elements>
    <Path StrokeColor="FFFF0000" StrokeWidth="3">
      <Point X="10" Y="20"/>
      <Point X="30" Y="40"/>
      <Point X="50" Y="60"/>
    </Path>
  </Elements>
</Slide>"#;

        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        assert_eq!(slide.elements.len(), 1);
        match &slide.elements[0] {
            EnbxElement::Path(p) => {
                assert_eq!(p.points.len(), 3);
                assert_eq!(p.points[0], (10.0, 20.0));
                assert_eq!(p.points[2], (50.0, 60.0));
                assert_eq!(p.stroke_color, "FFFF0000");
                assert_eq!(p.stroke_width, 3.0);
            }
            other => panic!("expected Path, got {other:?}"),
        }
    }

    #[test]
    fn parse_shape_extracts_path_data_and_geometry() {
        let xml = r#"<Slide>
  <Elements>
    <Shape type="cloud" FillColor="FFFF0000" StrokeColor="FF000000" StrokeWidth="2">
      <Geometry>
        <PresetGeometry>
          <GeometryType>Cloud</GeometryType>
          <Adjusts>
            <Adjust id="adj1" scale-x="0.25" scale-y="0.75"/>
            <Adjust id="adj2" scale-x="0.5" scale-y="0.5"/>
          </Adjusts>
        </PresetGeometry>
      </Geometry>
      <Path>M0,0 L100,0 C150,50 150,150 100,100 Z</Path>
      <Path></Path>
      <LineType>Dashed</LineType>
      <Line>
        <HeadEnd Type="Triangle" Width="Medium" Length="Medium"/>
        <TailEnd Type="Stealth" Width="Narrow" Length="Long"/>
      </Line>
    </Shape>
  </Elements>
</Slide>"#;

        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        assert_eq!(slide.elements.len(), 1);
        match &slide.elements[0] {
            EnbxElement::Shape(s) => {
                // First non-empty <Path> wins.
                assert_eq!(
                    s.path_data.as_deref(),
                    Some("M0,0 L100,0 C150,50 150,150 100,100 Z")
                );
                assert_eq!(s.geometry_type.as_deref(), Some("Cloud"));
                assert_eq!(s.line_type.as_deref(), Some("Dashed"));
                assert_eq!(s.adjusts.len(), 2);
                assert_eq!(s.adjusts[0].id, "adj1");
                assert_eq!(s.adjusts[0].scale_x, 0.25);
                assert_eq!(s.adjusts[0].scale_y, 0.75);
                let head = s.arrow_head.as_ref().expect("head arrow");
                assert_eq!(head.arrow_type, "Triangle");
                assert_eq!(head.width, "Medium");
                let tail = s.arrow_tail.as_ref().expect("tail arrow");
                assert_eq!(tail.arrow_type, "Stealth");
                assert_eq!(tail.length, "Long");
            }
            other => panic!("expected Shape, got {other:?}"),
        }
    }

    #[test]
    fn parse_shape_without_path_falls_back_gracefully() {
        let xml = r#"<Slide>
  <Elements>
    <Shape type="rectangle" FillColor="FFE0E0E0">
      <Geometry><CustomGeometry><GeometryType>Rectangle</GeometryType></CustomGeometry></Geometry>
    </Shape>
  </Elements>
</Slide>"#;

        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        match &slide.elements[0] {
            EnbxElement::Shape(s) => {
                assert!(s.path_data.is_none());
                assert_eq!(s.geometry_type.as_deref(), Some("Rectangle"));
            }
            other => panic!("expected Shape, got {other:?}"),
        }
    }

    #[test]
    fn parse_reference_xml_basic() {
        let xml = r#"<?xml version="1.0"?>
<Reference>
  <Relationship Id="rId1" Target="Resources/image1.png"/>
  <Relationship Id="rId2" Target="Resources/image2.jpg"/>
</Reference>"#;

        let map = parse_reference_xml(xml);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("rId1").map(|s| s.as_str()), Some("image1.png"));
        assert_eq!(map.get("rId2").map(|s| s.as_str()), Some("image2.jpg"));
    }

    #[test]
    fn parse_malformed_xml_does_not_panic() {
        let xml = r#"<Slide><Elements><Text><X>not_a_number</X></Text></Slide>"#;
        // Should not panic — graceful degradation.
        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        assert!(slide.elements.len() <= 1);
    }

    #[test]
    fn parse_empty_elements() {
        let xml = r#"<Slide width="1920" height="1080"><Elements></Elements></Slide>"#;
        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        assert!(slide.elements.is_empty());
        assert_eq!(slide.size, (1920.0, 1080.0));
    }

    #[test]
    fn emu_conversion() {
        // 1 inch = 914400 EMU = 96 px
        assert!((emu_to_px(914_400.0) - 96.0).abs() < 0.001);
        assert!((emu_to_px(0.0)).abs() < 0.001);
    }

    #[test]
    fn parse_point_list_various() {
        let pts = parse_point_list("10,20 30,40");
        assert_eq!(pts, vec![(10.0, 20.0), (30.0, 40.0)]);

        let pts2 = parse_point_list("10 20 30 40");
        assert_eq!(pts2, vec![(10.0, 20.0), (30.0, 40.0)]);

        let pts3 = parse_point_list("");
        assert!(pts3.is_empty());
    }

    #[test]
    fn normalise_hex_variants() {
        assert_eq!(normalise_hex("#FF0000"), "FFFF0000");
        assert_eq!(normalise_hex("FF0000"), "FFFF0000");
        assert_eq!(normalise_hex("#AABBCCDD"), "AABBCCDD");
        assert_eq!(normalise_hex("aabbccdd"), "AABBCCDD");
        assert_eq!(normalise_hex("xyz"), "FF000000");
    }

    #[test]
    fn default_slide() {
        let s = EnbxSlide::default();
        assert_eq!(s.size, (1280.0, 720.0));
        assert!(s.elements.is_empty());
        assert!(s.background.is_none());
    }

    // ── V4/V5 backport: richer element coverage ──────────────────────────

    #[test]
    fn parse_video_element() {
        let xml = r#"<Slide>
  <Elements>
    <Video>
      <X>120</X><Y>80</Y><Width>320</Width><Height>180</Height>
      <Source>id://vid1</Source>
      <IsLoop>true</IsLoop>
      <IsAutoPlay>false</IsAutoPlay>
      <Volume>0.5</Volume>
      <ThumbnailId>id://poster1</ThumbnailId>
    </Video>
  </Elements>
</Slide>"#;
        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        assert_eq!(slide.elements.len(), 1);
        match &slide.elements[0] {
            EnbxElement::Video(v) => {
                assert_eq!(v.x, 120.0);
                assert_eq!(v.width, 320.0);
                // `id://` prefix is stripped; bare id remains when unresolved.
                assert_eq!(v.resource_id, "vid1");
                assert!(v.is_loop);
                assert!(!v.is_auto_play);
                // New backport fields.
                assert_eq!(v.volume, 0.5);
                assert_eq!(v.thumbnail_id.as_deref(), Some("poster1"));
            }
            other => panic!("expected Video, got {other:?}"),
        }
    }

    #[test]
    fn parse_topic_element() {
        let xml = r#"<Slide>
  <Elements>
    <Topic type="MindMap">
      <X>200</X><Y>150</Y><Width>400</Width><Height>300</Height>
      <Title><Text>中心主题</Text></Title>
      <Nodes>
        <Node>
          <Title><Text>分支A</Text></Title>
          <Location>290.5,-128</Location>
          <ContentWidth>120</ContentWidth>
          <ContentHeight>40</ContentHeight>
        </Node>
        <Node>
          <Title><Text>分支B</Text></Title>
          <Location>-322,141</Location>
        </Node>
      </Nodes>
    </Topic>
  </Elements>
</Slide>"#;
        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        assert_eq!(slide.elements.len(), 1);
        match &slide.elements[0] {
            EnbxElement::Topic(t) => {
                assert_eq!(t.topic_type, "MindMap");
                assert_eq!(t.center_text, "中心主题");
                assert_eq!(t.center_x, 200.0);
                assert_eq!(t.center_w, 400.0);
                assert_eq!(t.children.len(), 2);
                assert_eq!(t.children[0].text, "分支A");
                assert_eq!(t.children[0].location, "290.5,-128");
                assert_eq!(t.children[1].text, "分支B");
            }
            other => panic!("expected Topic, got {other:?}"),
        }
    }

    #[test]
    fn parse_activity_and_3d_elements() {
        let xml = r#"<Slide>
  <Elements>
    <Cylinder>
      <X>10</X><Y>20</Y><Width>100</Width><Height>200</Height>
      <Transform>1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1</Transform>
    </Cylinder>
    <Activity type="Classify" id="act1">
      <Name>分类活动</Name>
      <Classify id="c1" name="城市名称">
        <Items>
          <Item id="i1" name="北京"/>
          <Item id="i2" name="上海"/>
        </Items>
      </Classify>
    </Activity>
  </Elements>
</Slide>"#;
        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        assert_eq!(slide.elements.len(), 2);

        match &slide.elements[0] {
            EnbxElement::Cylinder(s) => {
                assert_eq!(s.x, 10.0);
                assert_eq!(s.height, 200.0);
                assert!(s.transform.is_some());
            }
            other => panic!("expected Cylinder, got {other:?}"),
        }

        match &slide.elements[1] {
            EnbxElement::Activity(a) => {
                assert_eq!(a.key, "Classify");
                assert_eq!(a.name, "分类活动");
                assert_eq!(a.classifies.len(), 1);
                assert_eq!(a.classifies[0].name, "城市名称");
                assert_eq!(a.classifies[0].items.len(), 2);
                assert_eq!(a.classifies[0].items[0].name, "北京");
                assert_eq!(a.classifies[0].items[1].name, "上海");
            }
            other => panic!("expected Activity, got {other:?}"),
        }
    }

    #[test]
    fn parse_activity_item_element() {
        let xml = r#"<Slide>
  <Elements>
    <ActivityItem>
      <X>50</X><Y>60</Y><Width>140</Width><Height>70</Height>
      <ResourceId>res1</ResourceId>
      <ActivityId>act9</ActivityId>
      <Text>拖拽卡片</Text>
      <FontSize>20</FontSize>
      <ForegroundColor>FF0000FF</ForegroundColor>
    </ActivityItem>
  </Elements>
</Slide>"#;
        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        assert_eq!(slide.elements.len(), 1);
        match &slide.elements[0] {
            EnbxElement::ActivityItem(a) => {
                assert_eq!(a.activity_id, "act9");
                assert_eq!(a.text_content.as_deref(), Some("拖拽卡片"));
                assert_eq!(a.font_size, 20.0);
                assert_eq!(a.font_color, "FF0000FF");
            }
            other => panic!("expected ActivityItem, got {other:?}"),
        }
    }

    /// Mirrors a realistic "梦幻岛屿"-style courseware page: a single slide that
    /// mixes every backported V4/V5 element type together with a legacy unknown
    /// widget.  Guards against regressions in the tag dispatch where several new
    /// branches sit adjacent to one another.
    #[test]
    fn parse_compound_courseware_slide() {
        let xml = r#"<Slide width="1280" height="720">
  <Elements>
    <Video>
      <X>40</X><Y>40</Y><Width>480</Width><Height>270</Height>
      <Source>id://intro</Source>
      <IsLoop>true</IsLoop>
      <IsAutoPlay>true</IsAutoPlay>
    </Video>
    <Cylinder>
      <X>560</X><Y>40</Y><Width>120</Width><Height>240</Height>
    </Cylinder>
    <Cone>
      <X>700</X><Y>40</Y><Width>120</Width><Height>240</Height>
      <Transform>1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1</Transform>
    </Cone>
    <ActivityItem>
      <X>40</X><Y>340</Y><Width>160</Width><Height>90</Height>
      <ResourceId>card1</ResourceId>
      <ActivityId>act_drag</ActivityId>
      <Text>海星卡片</Text>
      <FontSize>22</FontSize>
      <ForegroundColor>FF00AA00</ForegroundColor>
      <Bold>true</Bold>
    </ActivityItem>
    <Activity type="Classify" id="act_drag">
      <Name>海洋生物分类</Name>
      <Classify id="sea" name="海洋">
        <Items>
          <Item id="x1" name="鲸鱼"/>
          <Item id="x2" name="海豚"/>
        </Items>
      </Classify>
    </Activity>
    <Topic type="MindMap">
      <X>840</X><Y>340</Y><Width>380</Width><Height>320</Height>
      <Title><Text>岛屿生态</Text></Title>
      <Nodes>
        <Node>
          <Title><Text>植物</Text></Title>
          <Location>200,-100</Location>
          <ContentWidth>120</ContentWidth>
          <ContentHeight>40</ContentHeight>
        </Node>
        <Node>
          <Title><Text>动物</Text></Title>
          <Location>-210,120</Location>
        </Node>
      </Nodes>
    </Topic>
    <CustomWidget mode="legacy">
      <Payload>opaque</Payload>
    </CustomWidget>
  </Elements>
</Slide>"#;

        let slide = parse_slide_xml(xml, &HashMap::new()).expect("parse slide");
        assert_eq!(slide.size, (1280.0, 720.0));
        assert_eq!(slide.elements.len(), 7);

        // Order is preserved from the source document.
        assert!(matches!(slide.elements[0], EnbxElement::Video(_)));
        assert!(matches!(slide.elements[1], EnbxElement::Cylinder(_)));
        assert!(matches!(slide.elements[2], EnbxElement::Cone(_)));
        assert!(matches!(slide.elements[3], EnbxElement::ActivityItem(_)));
        assert!(matches!(slide.elements[4], EnbxElement::Activity(_)));
        assert!(matches!(slide.elements[5], EnbxElement::Topic(_)));
        assert!(matches!(slide.elements[6], EnbxElement::Unknown(_)));

        // Spot-check one nested structure end-to-end.
        match &slide.elements[5] {
            EnbxElement::Topic(t) => {
                assert_eq!(t.center_text, "岛屿生态");
                assert_eq!(t.children.len(), 2);
                assert_eq!(t.children[1].text, "动物");
                assert_eq!(t.children[1].location, "-210,120");
            }
            other => panic!("expected Topic, got {other:?}"),
        }

        // The unknown widget is preserved (not silently dropped) — this is the
        // core bug from the backport brief.
        match &slide.elements[6] {
            EnbxElement::Unknown(xv) => {
                assert_eq!(xv.tag, "CustomWidget");
                assert_eq!(
                    xv.attributes.get("mode").map(|s| s.as_str()),
                    Some("legacy")
                );
                assert_eq!(xv.children.len(), 1);
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }
}
