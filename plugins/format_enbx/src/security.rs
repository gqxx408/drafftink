//! ZIP security checks — bomb detection, path traversal prevention.

use std::io::{Read, Seek};

/// Check for ZIP bomb attacks by comparing compressed vs uncompressed sizes.
///
/// Scans all entries; if any entry's uncompressed/compressed ratio exceeds
/// `max_ratio`, the archive is rejected.
///
/// Threshold: 100:1 (aligned with 备课端).
pub fn check_zip_bomb<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    max_ratio: u64,
) -> Result<(), String> {
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("ZIP read error: {}", e))?;
        let compressed = entry.compressed_size();
        let uncompressed = entry.size();
        if compressed > 0 && uncompressed / compressed > max_ratio {
            return Err(format!(
                "ZIP bomb detected: entry '{}' ratio {}:1 exceeds limit {}:1",
                entry.name(),
                uncompressed / compressed,
                max_ratio
            ));
        }
    }
    Ok(())
}
