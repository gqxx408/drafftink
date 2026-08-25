use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::stroke::InkStroke;

const CACHE_MAGIC: [u8; 4] = *b"DRFC";
const CACHE_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
struct CachePayload {
    magic: [u8; 4],
    version: u16,
    strokes: Vec<InkStroke>,
}

pub struct AnnotationCache {
    #[allow(dead_code)]
    cache_dir: PathBuf,
    cache_path: PathBuf,
    last_flush: std::time::Instant,
    flush_interval: std::time::Duration,
    pending_count: usize,
}

impl AnnotationCache {
    pub fn new(cache_dir: PathBuf, doc_hash: u32) -> Self {
        let _ = fs::create_dir_all(&cache_dir);
        let cache_path = cache_dir.join(format!("{:08x}.drfc", doc_hash));
        Self {
            cache_dir,
            cache_path,
            last_flush: std::time::Instant::now(),
            flush_interval: std::time::Duration::from_secs(3),
            pending_count: 0,
        }
    }

    pub fn new_with_existing(cache_dir: PathBuf, doc_hash: u32, _count: usize) -> Self {
        Self::new(cache_dir, doc_hash)
    }

    pub fn should_flush(&self, total_strokes: &[InkStroke]) -> bool {
        let time_elapsed = self.last_flush.elapsed() >= self.flush_interval;
        let has_pending = self.pending_count > 0;
        has_pending && (time_elapsed || !total_strokes.is_empty())
    }

    /// Atomic write: tmp → rename, with CRC32 integrity.
    pub fn flush(&mut self, strokes: &[InkStroke]) -> Result<(), String> {
        let payload = CachePayload {
            magic: CACHE_MAGIC,
            version: CACHE_VERSION,
            strokes: strokes.to_vec(),
        };

        let mut encoded =
            bincode::serialize(&payload).map_err(|e| format!("Cache serialize failed: {}", e))?;

        // CRC32 checksum appended at end
        let crc = crc32fast::hash(&encoded);
        encoded.extend_from_slice(&crc.to_le_bytes());

        // Atomic write
        let tmp_path = self.cache_path.with_extension("drfc.tmp");
        fs::write(&tmp_path, &encoded).map_err(|e| format!("Write tmp failed: {}", e))?;
        fs::rename(&tmp_path, &self.cache_path).map_err(|e| format!("Rename failed: {}", e))?;

        self.last_flush = std::time::Instant::now();
        self.pending_count = 0;

        Ok(())
    }

    pub fn cleanup(&self) {
        if self.cache_path.exists() {
            let _ = fs::remove_file(&self.cache_path);
        }
    }

    /// Scan cache directory and load strokes matching doc_hash. CRC-validated.
    pub fn scan_and_load(cache_dir: &Path, doc_hash: u32) -> Option<Vec<InkStroke>> {
        let cache_path = cache_dir.join(format!("{:08x}.drfc", doc_hash));
        if !cache_path.exists() {
            return None;
        }

        let data = fs::read(&cache_path).ok()?;
        if data.len() < 8 {
            let _ = fs::remove_file(&cache_path);
            return None;
        }

        let (body, crc_bytes) = data.split_at(data.len() - 4);
        let stored_crc = u32::from_le_bytes(crc_bytes.try_into().ok()?);
        let actual_crc = crc32fast::hash(body);
        if actual_crc != stored_crc {
            eprintln!("[cache] CRC mismatch for {:08x}, discarding", doc_hash);
            let _ = fs::remove_file(&cache_path);
            return None;
        }

        let payload: CachePayload = bincode::deserialize(body).ok()?;
        if payload.magic != CACHE_MAGIC || payload.version != CACHE_VERSION {
            return None;
        }

        Some(payload.strokes)
    }

    pub fn mark_pending(&mut self) {
        self.pending_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation::stroke::{InkStroke, ToolType};

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "drt_cache_test_{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ))
    }

    fn dummy_stroke() -> InkStroke {
        let mut s = InkStroke::new(ToolType::Pen, [255, 0, 0], 255, 2.0);
        s.points = vec![(0.0, 0.0), (10.0, 10.0), (20.0, 0.0)];
        s
    }

    #[test]
    fn cache_roundtrip() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let mut cache = AnnotationCache::new(dir.clone(), 0xCAFE);
        let strokes = vec![dummy_stroke()];
        cache.mark_pending();
        cache.flush(&strokes).unwrap();
        let loaded = AnnotationCache::scan_and_load(&dir, 0xCAFE);
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_crc_corruption() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let mut cache = AnnotationCache::new(dir.clone(), 0xBEEF);
        let strokes = vec![dummy_stroke()];
        cache.mark_pending();
        cache.flush(&strokes).unwrap();

        // Corrupt the last byte
        let path = dir.join("0000beef.drfc");
        let mut data = fs::read(&path).unwrap();
        let len = data.len();
        data[len - 2] ^= 0xFF;
        fs::write(&path, &data).unwrap();

        let loaded = AnnotationCache::scan_and_load(&dir, 0xBEEF);
        assert!(loaded.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_atomic_write() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let mut cache = AnnotationCache::new(dir.clone(), 0xDEAD);
        let strokes = vec![dummy_stroke()];
        cache.mark_pending();
        cache.flush(&strokes).unwrap();

        // No tmp file should remain
        let tmp = dir.join("0000dead.drfc.tmp");
        assert!(!tmp.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
