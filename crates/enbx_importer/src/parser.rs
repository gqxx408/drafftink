//! XML parsers: Board, Document, Reference, Slides.
//!
//! Uses quick-xml's streaming `Reader` to avoid materialising full XML trees.

use crate::error::EnbxError;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::BufReader;

// ---------------------------------------------------------------------------
// EMU helpers
// ---------------------------------------------------------------------------

pub fn emu_to_px(emu: f64) -> f32 {
    (emu * 96.0 / 914400.0) as f32
}

pub fn parse_hex(s: &str) -> [u8; 4] {
    let s = s.trim().trim_start_matches('#');
    match s.len() {
        6 => {
            let v = u32::from_str_radix(s, 16).unwrap_or(0xFFFFFF);
            [((v >> 16) & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, (v & 0xFF) as u8, 255]
        }
        8 => {
            let v = u32::from_str_radix(s, 16).unwrap_or(0xFFFFFFFF);
            [((v >> 24) & 0xFF) as u8, ((v >> 16) & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, (v & 0xFF) as u8]
        }
        _ => [255, 255, 255, 255],
    }
}

// ---------------------------------------------------------------------------
// Board.xml
// ---------------------------------------------------------------------------

pub fn parse_board<R: std::io::Read>(reader: R) -> Result<(f32, f32, [u8; 4]), EnbxError> {
    let mut r = Reader::from_reader(BufReader::new(reader));
    let mut buf = Vec::new();
    let mut w: f32 = 1920.0;
    let mut h: f32 = 1080.0;
    let mut bg = [255u8, 255, 255, 255];

    loop {
        match r.read_event_into(&mut buf)? {
            Event::Empty(ref e) | Event::Start(ref e) => {
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref());
                    let val = String::from_utf8_lossy(&attr.value);
                    match key.as_ref() {
                        "boardWidth" | "width" => {
                            if let Ok(v) = val.parse::<f64>() { w = emu_to_px(v); }
                        }
                        "boardHeight" | "height" => {
                            if let Ok(v) = val.parse::<f64>() { h = emu_to_px(v); }
                        }
                        "bgcolor" | "bgColor" | "backgroundColor" => {
                            bg = parse_hex(&val);
                        }
                        _ => {}
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok((w, h, bg))
}

// ---------------------------------------------------------------------------
// Reference.xml — resource-id → hash-filename map
// ---------------------------------------------------------------------------

pub fn parse_reference<R: std::io::Read>(
    reader: R,
) -> Result<HashMap<String, String>, EnbxError> {
    let mut r = Reader::from_reader(BufReader::new(reader));
    let mut buf = Vec::new();
    let mut map = HashMap::new();

    let mut current_id: Option<String> = None;
    let mut current_target: Option<String> = None;

    loop {
        match r.read_event_into(&mut buf)? {
            Event::Empty(ref e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let l = name.to_lowercase();
                if l == "relationship" || l == "resource" || l == "ref" {
                    for attr in e.attributes().flatten() {
                        let k = String::from_utf8_lossy(attr.key.as_ref());
                        let v = String::from_utf8_lossy(&attr.value);
                        match k.as_ref() {
                            "Id" | "id" | "r:id" => current_id = Some(v.to_string()),
                            "Target" | "target" | "file" | "src" => current_target = Some(v.to_string()),
                            _ => {}
                        }
                    }
                    if let (Some(id), Some(target)) = (current_id.take(), current_target.take()) {
                        let name = target
                            .strip_prefix("Resources/")
                            .unwrap_or(&target)
                            .to_string();
                        map.insert(id, name);
                    }
                }
            }
            Event::Start(ref e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let l = name.to_lowercase();
                if l == "relationship" || l == "resource" || l == "ref" {
                    for attr in e.attributes().flatten() {
                        let k = String::from_utf8_lossy(attr.key.as_ref());
                        let v = String::from_utf8_lossy(&attr.value);
                        match k.as_ref() {
                            "Id" | "id" | "r:id" => current_id = Some(v.to_string()),
                            "Target" | "target" | "file" | "src" => current_target = Some(v.to_string()),
                            _ => {}
                        }
                    }
                }
            }
            Event::End(ref e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let l = name.to_lowercase();
                if l == "relationship" || l == "resource" || l == "ref" {
                    if let (Some(id), Some(target)) = (current_id.take(), current_target.take()) {
                        let name = target
                            .strip_prefix("Resources/")
                            .unwrap_or(&target)
                            .to_string();
                        map.insert(id, name);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Document.xml
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct DocMeta {
    pub title: Option<String>,
    pub author: Option<String>,
}

pub fn parse_document<R: std::io::Read>(reader: R) -> Result<DocMeta, EnbxError> {
    let mut r = Reader::from_reader(BufReader::new(reader));
    let mut buf = Vec::new();
    let mut meta = DocMeta::default();
    let mut in_title = false;
    let mut in_author = false;

    loop {
        match r.read_event_into(&mut buf)? {
            Event::Start(ref e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                match name.as_ref() {
                    "title" => in_title = true,
                    "author" | "creator" => in_author = true,
                    _ => {}
                }
            }
            Event::Text(ref e) => {
                let text = e.unescape()?.to_string();
                if in_title { meta.title = Some(text.clone()); in_title = false; }
                if in_author { meta.author = Some(text); in_author = false; }
            }
            Event::End(ref e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                match name.as_ref() {
                    "title" => in_title = false,
                    "author" | "creator" => in_author = false,
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(meta)
}

// ---------------------------------------------------------------------------
// Slide listing
// ---------------------------------------------------------------------------

pub fn list_slides(archive: &mut zip::ZipArchive<impl std::io::Read + std::io::Seek>) -> Vec<usize> {
    let mut indices: Vec<usize> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(f) = archive.by_index(i) {
            let name = f.name().to_string();
            // Match: Slides/Slide_0.xml, slides/slide_0.xml, Slide_0.xml etc.
            let lower = name.to_lowercase();
            if lower.contains("slide") && lower.ends_with(".xml") {
                // Extract digits
                let digits: String = lower
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(n) = digits.parse::<usize>() {
                    indices.push(n);
                }
            }
        }
    }
    indices.sort_unstable();
    indices.dedup();
    log::debug!("Slide indices: {indices:?}");
    indices
}
