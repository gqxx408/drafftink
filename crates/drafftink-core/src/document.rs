//! Binary document format — .drft file I/O with magic header, version, CRC32.
//!
//! Layout:
//! ┌──────────┬──────────────────────────────────────────┐
//! │  Header  │  bincode-encoded CoursewareDoc payload   │
//! │  16 B    │  variable length                          │
//! └──────────┴──────────────────────────────────────────┘

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::CoursewareDoc;

const MAGIC: &[u8; 4] = b"DRFT";
const VERSION: u16 = 1;
const HEADER_SIZE: usize = 16;

#[derive(Debug)]
struct Header {
    magic: [u8; 4],
    version: u16,
    crc32: u32,
    payload_len: u32,
}

impl Header {
    fn read(mut r: impl Read) -> io::Result<Self> {
        let mut buf = [0u8; HEADER_SIZE];
        r.read_exact(&mut buf)?;
        let magic = [buf[0], buf[1], buf[2], buf[3]];
        if &magic != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Not a DRFT file",
            ));
        }
        let version = u16::from_le_bytes([buf[4], buf[5]]);
        if version > VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unsupported version {version}"),
            ));
        }
        Ok(Self {
            magic,
            version,
            crc32: u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]),
            payload_len: u32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]),
        })
    }

    fn write(&self, mut w: impl Write) -> io::Result<()> {
        w.write_all(&self.magic)?;
        w.write_all(&self.version.to_le_bytes())?;
        w.write_all(&self.crc32.to_le_bytes())?;
        w.write_all(&self.payload_len.to_le_bytes())?;
        w.write_all(&[0u8; 2])?;
        Ok(())
    }
}

/// Save a CoursewareDoc to a .drft file. Returns payload byte count.
pub fn save_document<P: AsRef<Path>>(path: P, doc: &CoursewareDoc) -> io::Result<usize> {
    let payload =
        bincode::serialize(doc).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let crc = crc32fast::hash(&payload);
    let header = Header {
        magic: *MAGIC,
        version: VERSION,
        crc32: crc,
        payload_len: payload.len() as u32,
    };
    let mut f = fs::File::create(path)?;
    header.write(&mut f)?;
    f.write_all(&payload)?;
    f.flush()?;
    Ok(payload.len())
}

/// Load a CoursewareDoc from a .drft file. Verifies magic, version, CRC32.
///
/// For backward compatibility with old-format files, if bincode deserialization
/// fails (e.g. missing new fields), returns `CoursewareDoc::empty()` with a warning.
pub fn load_document<P: AsRef<Path>>(path: P) -> io::Result<CoursewareDoc> {
    let mut f = fs::File::open(&path)?;
    let meta = f.metadata()?;
    if meta.len() < HEADER_SIZE as u64 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "File too small",
        ));
    }
    let header = Header::read(&mut f)?;
    let expected = HEADER_SIZE as u64 + header.payload_len as u64;
    if meta.len() < expected {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Truncated file",
        ));
    }
    let mut payload = vec![0u8; header.payload_len as usize];
    f.read_exact(&mut payload)?;
    let actual_crc = crc32fast::hash(&payload);
    if actual_crc != header.crc32 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "CRC mismatch"));
    }
    // Backward-compat: bincode does NOT support #[serde(default)] for EOF.
    // If deserialization fails (missing new fields), return an empty doc
    // so the user can still open and re-save the file.
    match bincode::deserialize::<CoursewareDoc>(&payload) {
        Ok(doc) => Ok(doc),
        Err(e) => {
            log::warn!("[document] File format upgrade needed: {e}. Opening as empty canvas.");
            Ok(CoursewareDoc::empty())
        }
    }
}

/// Load a CoursewareDoc from raw bytes (already read into memory).
/// Used by FormatRegistry to handle files opened by the system.
pub fn load_document_slice(data: &[u8]) -> io::Result<CoursewareDoc> {
    let mut cursor = std::io::Cursor::new(data);
    let header = Header::read(&mut cursor)?;
    // Seek past header already done by Header::read
    let mut payload = vec![0u8; header.payload_len as usize];
    cursor.read_exact(&mut payload)?;
    let actual_crc = crc32fast::hash(&payload);
    if actual_crc != header.crc32 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "CRC mismatch"));
    }
    bincode::deserialize(&payload).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

// Patch format (.drfp) — annotation strokes only
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationPatch {
    pub source_crc32: u32,
    pub strokes: Vec<StrokeData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrokeData {
    pub points: Vec<[f32; 2]>,
    pub color: [u8; 4],
    pub thickness: f32,
    pub tool: u8,
}

pub fn save_patch<P: AsRef<Path>>(path: P, patch: &AnnotationPatch) -> io::Result<()> {
    let data =
        bincode::serialize(patch).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, &data)
}

pub fn load_patch<P: AsRef<Path>>(path: P) -> io::Result<AnnotationPatch> {
    let data = fs::read(path)?;
    bincode::deserialize(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn simple_bincode_roundtrip() {
        let doc = CoursewareDoc::default();
        let encoded = bincode::serialize(&doc).expect("serialize");
        let _: CoursewareDoc = bincode::deserialize(&encoded).expect("deserialize");
    }

    #[test]
    fn file_roundtrip() {
        let doc = CoursewareDoc::default();
        let path = std::env::temp_dir().join(format!("_drft_{}.drft", Uuid::new_v4()));
        save_document(&path, &doc).unwrap();
        let _loaded = load_document(&path).unwrap();
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn corrupted_file_rejected() {
        let path = std::env::temp_dir().join(format!("_drft_bad_{}.bin", Uuid::new_v4()));
        fs::write(&path, b"DRFT\x01\x00BADDDATA").unwrap();
        assert!(load_document(&path).is_err());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn patch_roundtrip() {
        let patch = AnnotationPatch {
            source_crc32: 0,
            strokes: vec![StrokeData {
                points: vec![[0.0, 0.0], [10.0, 10.0]],
                color: [255, 0, 0, 255],
                thickness: 3.0,
                tool: 0,
            }],
        };
        let path = std::env::temp_dir().join(format!("_drfp_{}.drfp", Uuid::new_v4()));
        save_patch(&path, &patch).unwrap();
        let loaded = load_patch(&path).unwrap();
        assert_eq!(loaded.strokes.len(), 1);
        let _ = fs::remove_file(&path);
    }
}
