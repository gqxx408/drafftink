//! # 工具函数
//!
//! 哈希计算、时间格式化、ID 生成等通用工具。

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

// 日期时间标准化（GB/T 7408.1-2023）
pub mod date_time;
// 国标（GB/T）代码表硬编码（GB/T 2260 / 33782 / 2261.1 / 7408 等）
pub mod gb_standard_codes;
// 国民经济行业分类代码表（GB/T 4754-2017，数据量较大，独立文件）
pub mod gb_industry_codes;
// 语种名称代码表（GB/T 4881-1985）
pub mod gb_language_codes;

// ════════════════════════════════════════════════════════════════════════════
//  哈希工具
// ════════════════════════════════════════════════════════════════════════════

/// 计算数据的 SHA-256 哈希，返回 32 字节数组。
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// 计算数据的 SHA-256 哈希，返回十六进制字符串。
pub fn sha256_hex(data: &[u8]) -> String {
    let hash = sha256(data);
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

/// 计算文件的 SHA-256 哈希。
pub fn sha256_file(path: &std::path::Path) -> Result<[u8; 32], std::io::Error> {
    let data = std::fs::read(path)?;
    Ok(sha256(&data))
}

// ════════════════════════════════════════════════════════════════════════════
//  时间工具
// ════════════════════════════════════════════════════════════════════════════

/// 格式化时间为 "YYYY-MM-DD HH:MM:SS" 格式。
pub fn format_datetime(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// 格式化时间为 "YYYY-MM-DD" 格式。
pub fn format_date(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d").to_string()
}

/// 格式化时间为 ISO 8601 格式。
pub fn format_iso8601(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

/// 从 ISO 8601 字符串解析时间。
pub fn parse_iso8601(s: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc))
}

/// 获取当前 Unix 时间戳（秒）。
pub fn now_timestamp() -> i64 {
    Utc::now().timestamp()
}

/// 获取当前 Unix 时间戳（毫秒）。
pub fn now_timestamp_millis() -> i64 {
    Utc::now().timestamp_millis()
}

// ════════════════════════════════════════════════════════════════════════════
//  ID 生成
// ════════════════════════════════════════════════════════════════════════════

/// 生成新的 UUID v4。
pub fn new_uuid() -> Uuid {
    Uuid::new_v4()
}

/// 生成资源 ID（基于时间戳 + 随机数的短 ID）。
pub fn new_resource_id() -> String {
    let uuid = Uuid::new_v4();
    let hex = uuid.as_simple().to_string();
    // 取前 16 个字符作为短 ID
    format!("res_{}", &hex[..16])
}

// ════════════════════════════════════════════════════════════════════════════
//  Base64 工具
// ════════════════════════════════════════════════════════════════════════════

/// Base64 编码（标准，含 padding）。
pub fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Base64 解码（标准，含 padding）。
pub fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| e.to_string())
}

/// Base64 URL-safe 编码（无 padding）。
pub fn base64_url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// Base64 URL-safe 解码（无 padding）。
pub fn base64_url_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| e.to_string())
}

// ════════════════════════════════════════════════════════════════════════════
//  文件工具
// ════════════════════════════════════════════════════════════════════════════

/// 获取人类可读的文件大小。
pub fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// 确保目录存在，不存在则创建。
pub fn ensure_dir(path: &std::path::Path) -> Result<(), std::io::Error> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// 原子写入文件（先写 .tmp 再 rename）。
pub fn atomic_write(path: &std::path::Path, data: &[u8]) -> Result<(), std::io::Error> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
//  单元测试
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_known_vector() {
        // SHA-256("abc") = ba7816bf...
        let hash = sha256(b"abc");
        let expected = [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_sha256_hex() {
        let hex = sha256_hex(b"abc");
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_sha256_empty() {
        let hash = sha256(b"");
        // SHA-256("") = e3b0c442...
        assert_eq!(hash[0], 0xe3);
        assert_eq!(hash[1], 0xb0);
    }

    #[test]
    fn test_format_datetime() {
        let dt = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 1, 15, 14, 30, 45).unwrap();
        assert_eq!(format_datetime(&dt), "2026-01-15 14:30:45");
        assert_eq!(format_date(&dt), "2026-01-15");
    }

    #[test]
    fn test_iso8601_roundtrip() {
        let dt = Utc::now();
        let s = format_iso8601(&dt);
        let restored = parse_iso8601(&s).unwrap();
        assert_eq!(dt.timestamp(), restored.timestamp());
    }

    #[test]
    fn test_new_resource_id() {
        let id = new_resource_id();
        assert!(id.starts_with("res_"));
        assert_eq!(id.len(), 20); // "res_" (4) + 16 hex chars
    }

    #[test]
    fn test_base64_roundtrip() {
        let data = b"hello world";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_base64_url_roundtrip() {
        let data = b"hello+world/=";
        let encoded = base64_url_encode(data);
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
        let decoded = base64_url_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_file_size(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_atomic_write() {
        let temp = std::env::temp_dir().join("drafftink_test_atomic.bin");
        let data = b"test data";
        atomic_write(&temp, data).unwrap();
        let read = std::fs::read(&temp).unwrap();
        assert_eq!(read, data);
        let _ = std::fs::remove_file(&temp);
    }

    #[test]
    fn test_new_uuid_unique() {
        let id1 = new_uuid();
        let id2 = new_uuid();
        assert_ne!(id1, id2);
    }
}
