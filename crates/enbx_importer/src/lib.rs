//! ENBX/ENPX 希沃课件导入器
//!
//! Streams a ZIP-based .enbx file, validates security, parses Board/Document/
//! Reference/Slides via quick-xml, and builds a `CoursewareDoc`.

mod error;
mod parser;
mod security;
mod converter;
pub mod animation_parser;

pub use error::EnbxError;
pub use converter::ProgressFn;

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use drafftink_core::model::CoursewareDoc;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct ImportReport {
    pub pages_ok: usize,
    pub pages_failed: usize,
    pub resources_extracted: usize,
    pub warnings: Vec<String>,
    pub title: Option<String>,
}

/// Import a .enbx / .enpx file.  Optionally accepts a progress callback:
/// `callback(current, total, "description")`.
pub fn import_enbx(path: &Path, progress: Option<ProgressFn>) -> Result<(CoursewareDoc, ImportReport), EnbxError> {
    log::info!("=== ENBX import: {} ===", path.display());

    let file = File::open(path)?;
    let file_size = file.metadata()?.len();
    log::info!("File: {:.1} MB", file_size as f64 / 1_048_576.0);

    let mut archive = zip::ZipArchive::new(BufReader::new(file))?;
    log::info!("Entries: {}", archive.len());

    // Security
    security::check_zip_bomb(&mut archive)?;
    log::info!("Security OK");

    // --- 0.5  Dump ZIP layout for diagnostics ---
    let mut entry_names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();
    entry_names.sort();
    log::debug!("ZIP entries ({}):", entry_names.len());
    for name in &entry_names {
        log::debug!("  {name}");
    }

    // Board — read into vec, then parse
    let board_bytes = if let Ok(mut f) = archive.by_name("Board.xml") {
        let sz = f.size();
        let mut v = Vec::with_capacity(sz as usize);
        f.read_to_end(&mut v).ok();
        log::debug!("Board.xml: {} bytes", v.len());
        v
    } else {
        log::debug!("Board.xml: not found");
        Vec::new()
    };
    let (canvas_w, canvas_h, bg) = if board_bytes.is_empty() {
        log::warn!("Board.xml missing, using defaults");
        (1920.0, 1080.0, [255u8, 255, 255, 255])
    } else {
        parser::parse_board(board_bytes.as_slice())?
    };
    log::info!("Board: {canvas_w:.0}x{canvas_h:.0}");

    // Document metadata
    let doc_meta = if let Ok(mut f) = archive.by_name("Document.xml") {
        let mut v = Vec::new();
        f.read_to_end(&mut v).ok();
        log::debug!("Document.xml: {} bytes", v.len());
        parser::parse_document(v.as_slice()).ok().unwrap_or_default()
    } else {
        log::debug!("Document.xml: not found");
        parser::DocMeta::default()
    };
    log::info!("Title: {:?}", doc_meta.title);

    // Reference
    let ref_map = if let Ok(mut f) = archive.by_name("Reference.xml") {
        let sz = f.size();
        let mut v = Vec::new();
        f.read_to_end(&mut v).ok();
        log::debug!("Reference.xml: {} bytes (pre-size {})", v.len(), sz);
        parser::parse_reference(v.as_slice()).unwrap_or_default()
    } else {
        log::debug!("Reference.xml: not found");
        HashMap::new()
    };
    log::info!("Resources: {} refs", ref_map.len());

    // Slides
    let slide_indices = parser::list_slides(&mut archive);
    log::info!("Slides: {slide_indices:?}");
    let total = slide_indices.len();

    if let Some(ref cb) = progress {
        cb(0, total, "parsing slides");
    }

    let (mut doc, mut report) = converter::convert_slides(
        &mut archive,
        &slide_indices,
        &ref_map,
        canvas_w,
        canvas_h,
        bg,
        progress.as_ref(),
    )?;

    // ── Parse animations for each slide ─────────────────────────────────
    let package_root = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    for (i, &idx) in slide_indices.iter().enumerate() {
        let entry = format!("Slides/Slide_{idx}.xml");
        if let Ok(mut f) = archive.by_name(&entry) {
            let mut xml = String::new();
            f.read_to_string(&mut xml).ok();
            if !xml.is_empty() {
                let (anim_map, seq) = animation_parser::parse_slide_animations(
                    &xml,
                    package_root,
                    [canvas_w, canvas_h],
                );
                if let Some(page) = doc.pages.get_mut(i) {
                    page.animations = anim_map;
                    page.animation_sequence = seq;
                }
            }
        }
    }

    report.title = doc_meta.title.clone();

    if let Some(ref cb) = progress {
        cb(total, total, "done");
    }

    log::info!("=== Done: {} ok, {} failed ===", report.pages_ok, report.pages_failed);
    Ok((doc, report))
}

/// Extract images from `Resources/` using SHA256 dedup.
pub fn extract_resources(
    archive: &mut zip::ZipArchive<impl Read + std::io::Seek>,
    ref_map: &HashMap<String, String>,
    dest: &Path,
) -> Result<usize, EnbxError> {
    std::fs::create_dir_all(dest).ok();
    let mut hashes: HashMap<String, String> = HashMap::new(); // hash → fname
    let mut count = 0usize;

    for fname in ref_map.values() {
        // P0-4 (ZipSlip): the filename comes straight from Reference.xml.
        // Validate it BEFORE joining onto `dest` so a malicious
        // `Target="../../../../tmp/evil"` or `C:\windows\...\evil.dll` can
        // never produce a path outside the destination directory.
        security::check_path(fname)?;

        let entry_name = format!("Resources/{fname}");
        let mut f = match archive.by_name(&entry_name) {
            Ok(f) => f,
            Err(_) => continue,
        };

        // Streamed read with a hard byte cap (zip-bomb defence).
        let mut buf = Vec::with_capacity((f.size() as usize).min(1 << 16));
        let mut chunk = [0u8; 8192];
        let mut extracted: u64 = 0;
        loop {
            let n = f.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            extracted += n as u64;
            if extracted > security::MAX_EXTRACT_BYTES {
                return Err(EnbxError::ZipBomb);
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        if buf.is_empty() {
            continue;
        }

        // SHA256 for dedup
        let hash = hex::encode(Sha256::digest(&buf));
        if let Some(existing) = hashes.get(&hash) {
            log::debug!("Dedup: {fname} = {existing}");
            continue;
        }
        hashes.insert(hash, fname.clone());

        let out_path = dest.join(fname);
        // Canonicalization guard (defence-in-depth): re-derive the target from
        // the *canonical* destination and a clean, already-validated `fname`,
        // so it can never resolve outside `dest` even if `check_path` is ever
        // weakened. We join onto the canonical dest (not the raw `out_path`,
        // which may not exist yet and would fail to canonicalize on Windows).
        let dest_canon = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
        let out_canon = dest_canon.join(fname);
        if !out_canon.starts_with(&dest_canon) {
            return Err(EnbxError::Security(format!("path escape: {fname}")));
        }
        if !out_path.exists() {
            std::fs::write(&out_path, &buf).ok();
            count += 1;
        }
    }
    Ok(count)
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::check_path;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    // ---- check_path: ZipSlip / drive-letter rejection -------------------

    #[test]
    fn check_path_rejects_dotdot_traversal() {
        assert!(check_path("../evil").is_err());
        assert!(check_path("../../etc/passwd").is_err());
        assert!(check_path("a/../../b").is_err());
    }

    #[test]
    fn check_path_rejects_windows_drive_letter() {
        // The classic ZipSlip-on-Windows bypass that `starts_with('/')` misses.
        assert!(check_path("C:\\windows\\system32\\evil.dll").is_err());
        assert!(check_path("D:/evil").is_err());
        assert!(check_path("c:/users/x/payload.exe").is_err());
    }

    #[test]
    fn check_path_allows_relative_resource_names() {
        assert!(check_path("Resources/abc.jpg").is_ok());
        assert!(check_path("abc.jpg").is_ok());
        assert!(check_path("sub/dir/img.png").is_ok());
    }

    #[test]
    fn malicious_reference_target_is_caught_by_check_path() {
        // Reference.xml can carry a relative path that escapes via `..`.
        let xml = br#"<Relationships>
            <Relationship Id="rId1" Target="../../../../tmp/evil.bin"/>
        </Relationships>"#;
        let map = crate::parser::parse_reference(&xml[..]).unwrap();
        let fname = map.get("rId1").expect("rId1 present");
        assert!(check_path(fname).is_err(), "fname = {fname:?} should be rejected");
    }

    // ---- extract_resources: traversal + zip-bomb ------------------------

    /// Build an in-memory zip with the given (name, uncompressed size) entries.
    /// Zero-filled data deflates to almost nothing, so we can ask for huge
    /// uncompressed sizes cheaply and let the streaming reader catch the bomb.
    fn make_archive(entries: &[(&str, usize)]) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut w = ZipWriter::new(cursor);
            let opts = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, size) in entries {
                w.start_file(*name, opts).unwrap();
                let zeros = [0u8; 8192];
                let mut remaining = *size;
                while remaining > 0 {
                    let n = remaining.min(zeros.len());
                    w.write_all(&zeros[..n]).unwrap();
                    remaining -= n;
                }
            }
            w.finish().unwrap();
        }
        buf
    }

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("enbx_test_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn extract_resources_rejects_traversal_fname() {
        let buf = make_archive(&[("Resources/ok.jpg", 10)]);
        let mut archive =
            zip::ZipArchive::new(Cursor::new(buf)).unwrap();
        let mut ref_map: HashMap<String, String> = HashMap::new();
        ref_map.insert("rId1".to_string(), "../../../../tmp/evil.jpg".to_string());

        let dest = tmp_dir("traversal");
        let res = extract_resources(&mut archive, &ref_map, &dest);
        assert!(
            matches!(res, Err(EnbxError::Security(_))),
            "expected Security error, got {:?}",
            res
        );
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn extract_resources_writes_clean_resource() {
        let buf = make_archive(&[("Resources/ok.jpg", 10)]);
        let mut archive = zip::ZipArchive::new(Cursor::new(buf)).unwrap();
        let mut ref_map: HashMap<String, String> = HashMap::new();
        ref_map.insert("rId1".to_string(), "ok.jpg".to_string());

        let dest = tmp_dir("clean");
        let count = extract_resources(&mut archive, &ref_map, &dest).unwrap();
        assert_eq!(count, 1);
        assert!(dest.join("ok.jpg").exists());
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn extract_resources_detects_zip_bomb() {
        // 512 MB uncompressed (> MAX_EXTRACT_BYTES) but a tiny on-disk file.
        let buf = make_archive(&[("Resources/big.bin", 512 * 1024 * 1024)]);
        let mut archive = zip::ZipArchive::new(Cursor::new(buf)).unwrap();
        let mut ref_map: HashMap<String, String> = HashMap::new();
        ref_map.insert("rId1".to_string(), "big.bin".to_string());

        let dest = tmp_dir("bomb");
        let res = extract_resources(&mut archive, &ref_map, &dest);
        assert!(
            matches!(res, Err(EnbxError::ZipBomb)),
            "expected ZipBomb, got {:?}",
            res
        );
        let _ = std::fs::remove_dir_all(&dest);
    }
}

