//! Security: path traversal & zip bomb defence.

use crate::error::EnbxError;
use std::io::Read;

const MAX_UNCOMPRESSED: u64 = 2_147_483_648; // 2 GB
const MAX_SINGLE_FILE: u64 = 524_288_000; // 500 MB
const MAX_RATIO: u64 = 100;

/// Hard cap on *actually streamed* extracted bytes in `extract_resources`.
///
/// `check_zip_bomb` validates the *declared* `size()` of each entry before
/// extraction, but a zip bomb can lie about its compressed/declared size.
/// This cap is enforced while the bytes are read off the wire, so a 600 MB
/// decompression bomb is aborted after ~500 MB instead of exhausting memory.
pub(crate) const MAX_EXTRACT_BYTES: u64 = 524_288_000; // 500 MB

/// Reject path-traversal and absolute-path filenames.
///
/// Mirrors `LocalStorage::resolve_path`: refuses
/// - `..` path segments (climb out of the destination),
/// - entries starting with `/` or `\` (Unix/Windows absolute paths),
/// - Windows drive-letter absolute paths such as `C:\...\evil.dll` or `D:/evil`
///   (the `^[A-Za-z]:[\\/]` pattern), which `starts_with('/')` would miss.
pub fn check_path(name: &str) -> Result<(), EnbxError> {
    if name.contains("..")
        || name.starts_with('/')
        || name.starts_with('\\')
        || is_windows_drive_path(name)
    {
        return Err(EnbxError::Security(format!("path traversal: {name}")));
    }
    Ok(())
}

/// True for Windows drive-letter absolute paths like `C:\` or `D:/`.
///
/// Implemented with a manual byte scan instead of pulling in the `regex`
/// crate — the pattern is fixed and tiny, and this avoids a new dependency.
fn is_windows_drive_path(name: &str) -> bool {
    let b = name.as_bytes();
    if b.len() < 3 {
        return false;
    }
    b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

pub fn check_zip_bomb(
    archive: &mut zip::ZipArchive<impl Read + std::io::Seek>,
) -> Result<u64, EnbxError> {
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let f = archive.by_index(i)?;
        let name = f.name().to_string();
        let uncomp = f.size();

        check_path(name.as_str())?;

        if uncomp > MAX_SINGLE_FILE {
            return Err(EnbxError::Security(format!(
                "file too large: {name} ({uncomp} bytes)"
            )));
        }
        let comp = f.compressed_size();
        if comp > 0 && uncomp / comp > MAX_RATIO {
            return Err(EnbxError::Security(format!("suspicious ratio: {name}")));
        }
        total += uncomp;
        if total > MAX_UNCOMPRESSED {
            return Err(EnbxError::Security("total > 2 GB".into()));
        }
    }
    Ok(total)
}
