//! Ink cache subsystem — atomic, CRC-validated annotation stroke caching.
//!
//! ## On-disk layout
//!
//! Each cache file is named `<source_hash>.cache` and lives in
//! `%TEMP%/drafftink/cache/` (or a configurable directory).
//!
//! ```text
//! ┌──────────┬──────────────────────────────────────────┐
//! │  Header  │  bincode-encoded Vec<InkStroke>          │
//! │  10 B    │  variable length                          │
//! └──────────┴──────────────────────────────────────────┘
//!
//! Header:
//!   [0..4]  MAGIC  = b"CACH"
//!   [4..6]  VERSION = u16 LE (currently 1)
//!   [6..10] CRC32   = u32 LE (of payload only)
//! ```
//!
//! ## Atomic writes
//!
//! `flush()` writes to `<hash>.cache.tmp` first, flushes the OS buffer,
//! then atomically renames to `<hash>.cache`.  If the process crashes
//! mid-write, only the `.tmp` file is left behind (cleaned up on next
//! `scan_recoverable()`).

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// InkStroke — lightweight on-disk representation
// ---------------------------------------------------------------------------

/// A single annotation stroke suitable for caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InkStroke {
    /// Flattened segments: `segments[i]` is a contiguous polyline.
    /// Stored as `[f32; 2]` for bincode efficiency (avoiding Pos2 serialisation overhead).
    pub segments: Vec<Vec<[f32; 2]>>,
    /// RGBA color.
    pub color: [u8; 4],
    /// Stroke width in screen pixels.
    pub thickness: f32,
    /// 0 = pen, 1 = highlighter, 2 = eraser.
    pub tool: u8,
}

impl InkStroke {
    /// Total point count across all segments.
    pub fn point_count(&self) -> usize {
        self.segments.iter().map(|s| s.len()).sum()
    }
}

// ---------------------------------------------------------------------------
// Cache file I/O
// ---------------------------------------------------------------------------

const CACHE_MAGIC: &[u8; 4] = b"CACH";
const CACHE_VERSION: u16 = 1;
const CACHE_HEADER_SIZE: usize = 10;
const CACHE_EXT: &str = "cache";
const CACHE_TMP_EXT: &str = "cache.tmp";

/// Default cache directory under OS temp.
pub fn default_cache_dir() -> PathBuf {
    std::env::temp_dir().join("drafftink").join("cache")
}

/// Compute a stable document hash for cache file naming.
/// Uses the first 8 characters of the SHA-256 of the file path as a simple discriminator.
/// For proper identity tracking, pass the CRC32 of the .drft payload.
pub fn source_hash_from_crc(crc: u32) -> String {
    format!("{crc:08x}")
}

// ---------------------------------------------------------------------------
// Cache header
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CacheHeader {
    crc32: u32,
}

impl CacheHeader {
    fn read(mut r: impl Read) -> io::Result<Self> {
        let mut buf = [0u8; CACHE_HEADER_SIZE];
        r.read_exact(&mut buf)?;
        let magic = [buf[0], buf[1], buf[2], buf[3]];
        if &magic != CACHE_MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Not a CACH file"));
        }
        let version = u16::from_le_bytes([buf[4], buf[5]]);
        if version != CACHE_VERSION {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("Unsupported version {version}")));
        }
        Ok(Self {
            crc32: u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]),
        })
    }

    fn write(&self, mut w: impl Write) -> io::Result<()> {
        w.write_all(CACHE_MAGIC)?;
        w.write_all(&CACHE_VERSION.to_le_bytes())?;
        w.write_all(&self.crc32.to_le_bytes())?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Read / write helpers
// ---------------------------------------------------------------------------

/// Load strokes from a cache file. Returns `None` if the file is missing or
/// corrupted (logs a warning in the latter case).
pub fn load_cache_file(path: &Path) -> Option<Vec<InkStroke>> {
    let mut f = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return None,
    };

    let meta = match f.metadata() {
        Ok(m) => m,
        Err(_) => return None,
    };
    if meta.len() < CACHE_HEADER_SIZE as u64 {
        return None;
    }

    let header = match CacheHeader::read(&mut f) {
        Ok(h) => h,
        Err(_) => {
            log::warn!("[cache] Bad header in {}", path.display());
            return None;
        }
    };

    let payload_len = meta.len() as usize - CACHE_HEADER_SIZE;
    let mut payload = vec![0u8; payload_len];
    if f.read_exact(&mut payload).is_err() {
        log::warn!("[cache] Truncated payload in {}", path.display());
        return None;
    }

    let actual_crc = crc32fast::hash(&payload);
    if actual_crc != header.crc32 {
        log::warn!(
            "[cache] CRC mismatch in {} (expected {:08x}, got {:08x}) — discarding",
            path.display(),
            header.crc32,
            actual_crc
        );
        let _ = fs::remove_file(path);
        return None;
    }

    match bincode::deserialize::<Vec<InkStroke>>(&payload) {
        Ok(strokes) => {
            log::debug!("[cache] Loaded {} strokes from {}", strokes.len(), path.display());
            Some(strokes)
        }
        Err(e) => {
            log::warn!("[cache] Deserialization failed for {}: {e}", path.display());
            None
        }
    }
}

/// Atomically write strokes to a cache file.
///
/// 1. Serialise to `<path>.tmp`
/// 2. Flush OS buffers
/// 3. Rename to `<path>`
pub fn save_cache_file(path: &Path, strokes: &[InkStroke]) -> io::Result<()> {
    let payload = bincode::serialize(strokes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let crc = crc32fast::hash(&payload);

    let tmp_path = path.with_extension(CACHE_TMP_EXT);

    // Ensure parent directory exists
    if let Some(parent) = tmp_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // 1. Write to temp file
    {
        let mut f = fs::File::create(&tmp_path)?;
        let header = CacheHeader { crc32: crc };
        header.write(&mut f)?;
        f.write_all(&payload)?;
        f.flush()?;
        // Also sync to disk to ensure durability
        f.sync_all()?;
    }

    // 2. Atomic rename
    fs::rename(&tmp_path, path)?;

    log::debug!(
        "[cache] Wrote {} strokes ({:.1} KB) to {}",
        strokes.len(),
        payload.len() as f32 / 1024.0,
        path.display()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Recovery scanner
// ---------------------------------------------------------------------------

/// Result of scanning a single cache file during recovery.
#[derive(Debug, Clone)]
pub struct RecoveredCache {
    pub source_hash: String,
    pub strokes: Vec<InkStroke>,
    pub stroke_count: usize,
}

/// Scan the cache directory for recoverable `.cache` files.
///
/// - Validates header + CRC for each file.
/// - Removes corrupted files.
/// - Removes orphaned `.tmp` files.
/// - Returns valid caches.
pub fn scan_recoverable(dir: &Path) -> Vec<RecoveredCache> {
    let mut recovered = Vec::new();

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return recovered,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Clean up orphaned tmp files
        if path.extension().and_then(|e| e.to_str()) == Some("tmp") {
            log::debug!("[cache] Removing orphaned tmp: {}", path.display());
            let _ = fs::remove_file(&path);
            continue;
        }

        // Only process .cache files
        if path.extension().and_then(|e| e.to_str()) != Some(CACHE_EXT) {
            continue;
        }

        let hash = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        if let Some(strokes) = load_cache_file(&path) {
            let count = strokes.len();
            recovered.push(RecoveredCache {
                source_hash: hash,
                strokes,
                stroke_count: count,
            });
        }
    }

    if !recovered.is_empty() {
        log::info!(
            "[cache] Recovery: found {} cache(s) with {} total strokes",
            recovered.len(),
            recovered.iter().map(|r| r.stroke_count).sum::<usize>()
        );
    }

    recovered
}

// ---------------------------------------------------------------------------
// InkCache — runtime state
// ---------------------------------------------------------------------------

/// Manages runtime annotation stroke caching with atomic flush.
pub struct InkCache {
    dir: PathBuf,
    source_hash: String,
    strokes: Vec<InkStroke>,
    /// Number of strokes added since last flush.
    pending_count: usize,
    /// Whether a flush is needed (any stroke added at all).
    dirty: bool,
    /// Flush every N frames (60 = ~1 second at 60 FPS).
    flush_interval: u32,
    frame_counter: u32,
}

impl InkCache {
    /// Create a new cache for a document identified by `source_hash`.
    pub fn new(dir: PathBuf, source_hash: &str) -> Self {
        Self {
            dir,
            source_hash: source_hash.to_string(),
            strokes: Vec::new(),
            pending_count: 0,
            dirty: false,
            flush_interval: 60,
            frame_counter: 0,
        }
    }

    /// Create and check for recoverable strokes matching this source hash.
    pub fn new_with_recovery(dir: PathBuf, source_hash: &str) -> Self {
        let mut cache = Self::new(dir, source_hash);
        cache.try_recover();
        cache
    }

    /// Return the file path for this cache.
    pub fn cache_path(&self) -> PathBuf {
        self.dir.join(format!("{}.{}", self.source_hash, CACHE_EXT))
    }

    /// Add a single stroke and bump pending counter.
    pub fn push_stroke(&mut self, stroke: InkStroke) {
        self.strokes.push(stroke);
        self.pending_count += 1;
        self.dirty = true;
    }

    /// Extend with multiple strokes.
    pub fn extend_strokes(&mut self, strokes: Vec<InkStroke>) {
        self.pending_count += strokes.len();
        self.strokes.extend(strokes);
        self.dirty = true;
    }

    /// Mark a new stroke was completed (pen lifted). Increments pending.
    pub fn on_pen_up(&mut self) {
        // pending_count is already bumped by push_stroke;
        // this is a hook for future granular tracking.
    }

    /// Total number of cached strokes.
    pub fn stroke_count(&self) -> usize {
        self.strokes.len()
    }

    /// Number of strokes pending since last flush.
    pub fn pending_count(&self) -> usize {
        self.pending_count
    }

    /// Whether auto-flush should fire this frame.
    pub fn should_flush(&mut self) -> bool {
        if !self.dirty {
            return false;
        }
        self.frame_counter += 1;
        if self.frame_counter >= self.flush_interval {
            self.frame_counter = 0;
            true
        } else {
            false
        }
    }

    /// Force a flush regardless of frame count.
    pub fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        let path = self.cache_path();
        match save_cache_file(&path, &self.strokes) {
            Ok(()) => {
                self.pending_count = 0;
                self.dirty = false;
                self.frame_counter = 0;
            }
            Err(e) => log::error!("[cache] Flush failed for {}: {e}", path.display()),
        }
    }

    /// Consume all strokes and clear the cache.
    pub fn take_strokes(&mut self) -> Vec<InkStroke> {
        self.pending_count = 0;
        self.dirty = false;
        self.frame_counter = 0;
        std::mem::take(&mut self.strokes)
    }

    /// Try to load previously saved strokes for this source hash.
    fn try_recover(&mut self) {
        let path = self.cache_path();
        if let Some(strokes) = load_cache_file(&path) {
            log::info!(
                "[cache] Recovered {} strokes for hash {}",
                strokes.len(),
                self.source_hash
            );
            self.strokes = strokes;
            self.pending_count = 0;
            self.dirty = false;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("drt_cache_test_{:08x}", rand_seed()))
    }

    fn rand_seed() -> u32 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    }

    fn dummy_strokes(n: usize) -> Vec<InkStroke> {
        (0..n)
            .map(|i| InkStroke {
                segments: vec![vec![[i as f32, (i * 2) as f32], [(i + 1) as f32, (i * 3) as f32]]],
                color: [255, 0, 0, 255],
                thickness: 3.0,
                tool: 0,
            })
            .collect()
    }

    /// Write 100 strokes, read them back — count must match.
    #[test]
    fn cache_write_and_read() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_hash.cache");

        let strokes = dummy_strokes(100);
        save_cache_file(&path, &strokes).unwrap();

        let loaded = load_cache_file(&path).unwrap();
        assert_eq!(loaded.len(), 100, "stroke count mismatch");
        assert_eq!(loaded[0].tool, 0);
        assert_eq!(loaded[99].segments.len(), 1);

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }

    /// Manually corrupt the tail of a cache file — load must return None + warn.
    #[test]
    fn cache_crc_corruption() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("corrupt.cache");

        let strokes = dummy_strokes(10);
        save_cache_file(&path, &strokes).unwrap();

        // Corrupt the last byte
        let mut data = fs::read(&path).unwrap();
        let len = data.len();
        data[len - 1] ^= 0xFF;
        fs::write(&path, &data).unwrap();

        // Must return None (CRC failure)
        let result = load_cache_file(&path);
        assert!(
            result.is_none(),
            "Corrupted cache should not be loadable, got {:?} strokes",
            result.map(|v| v.len())
        );

        // File should be removed after CRC failure
        assert!(
            !path.exists(),
            "Corrupted cache file should be deleted: {}",
            path.display()
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// Simulate a crash mid-write: create a .tmp file, then verify that the
    /// main .cache file is untouched (or absent, which is also fine).
    #[test]
    fn cache_atomic_write() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("atomic.cache");
        let tmp_path = path.with_extension(CACHE_TMP_EXT);

        // Write a valid cache first
        let good_strokes = dummy_strokes(5);
        save_cache_file(&path, &good_strokes).unwrap();
        assert!(path.exists());
        assert!(!tmp_path.exists(), "tmp must be cleaned up after rename");

        // Simulate a crash: write partial data to tmp
        fs::write(&tmp_path, b"garbage_not_a_real_cache").unwrap();
        assert!(tmp_path.exists());

        // The main cache should still be intact
        let loaded = load_cache_file(&path);
        assert!(loaded.is_some(), "Main cache must survive tmp corruption");
        assert_eq!(loaded.unwrap().len(), 5);

        // scan_recoverable should clean the tmp and keep the main
        let recovered = scan_recoverable(&dir);
        assert!(!tmp_path.exists(), "scan_recoverable should remove tmp files");
        // The valid main cache should be found
        let found = recovered.iter().any(|r| r.source_hash == "atomic");
        assert!(found, "Valid cache should be found after recovery scan");

        let _ = fs::remove_dir_all(&dir);
    }

    /// Full recovery flow: write cache → simulate restart → scan finds it.
    #[test]
    fn cache_recovery_flow() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let hash = "cafe1234";
        let path = dir.join(format!("{hash}.cache"));

        // Write strokes as if a previous session saved them
        let strokes = dummy_strokes(7);
        save_cache_file(&path, &strokes).unwrap();

        // Simulate program restart — scan_recoverable
        let recovered = scan_recoverable(&dir);
        let our_cache = recovered.iter().find(|r| r.source_hash == hash);
        assert!(our_cache.is_some(), "scan_recoverable should find our cache");
        assert_eq!(our_cache.unwrap().stroke_count, 7);

        // Now test InkCache::new_with_recovery
        let mut cache = InkCache::new_with_recovery(dir.clone(), hash);
        assert_eq!(cache.stroke_count(), 7, "InkCache should load recovered strokes");

        // Add more and flush
        cache.extend_strokes(dummy_strokes(3));
        cache.flush();
        assert_eq!(cache.stroke_count(), 10);

        // Reload fresh — should see all 10
        let cache2 = InkCache::new_with_recovery(dir.clone(), hash);
        assert_eq!(cache2.stroke_count(), 10);

        let _ = fs::remove_dir_all(&dir);
    }
}
