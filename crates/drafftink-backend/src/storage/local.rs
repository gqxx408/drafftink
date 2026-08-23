//! # 本地文件系统存储
//!
//! 将文件存储在指定基础目录下，对路径进行安全检查以防止目录遍历攻击。

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use super::Storage;

/// 本地文件系统存储
pub struct LocalStorage {
    /// 基础目录
    base_dir: PathBuf,
}

impl LocalStorage {
    /// 创建 LocalStorage，确保基础目录存在
    pub fn new(base_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_dir)?;
        Ok(Self {
            base_dir: base_dir.to_path_buf(),
        })
    }

    /// 将相对路径解析为绝对路径，同时检查路径安全。
    ///
    /// 拒绝包含 `..` 的路径，防止目录遍历攻击。
    fn resolve_path(&self, path: &str) -> Result<PathBuf> {
        // 拒绝包含 .. 的路径
        if path.contains("..") {
            return Err(anyhow!("路径包含非法字符 '..': {path}"));
        }

        // 拒绝绝对路径（Windows 和 Unix）
        if path.starts_with('/') || path.starts_with('\\') {
            return Err(anyhow!("路径不能为绝对路径: {path}"));
        }

        // 拒绝以驱动器号开头 (如 C:)
        if path.len() >= 2 {
            let bytes = path.as_bytes();
            if bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
                return Err(anyhow!("路径包含非法驱动器号: {path}"));
            }
        }

        // 路径安全：已拒绝 `..`、绝对路径和驱动器号，
        // join 后的路径必然在 base_dir 之下。
        let full = self.base_dir.join(path);

        Ok(full)
    }
}

impl Storage for LocalStorage {
    fn save(&self, path: &str, data: Vec<u8>) -> Result<()> {
        let full = self.resolve_path(path)?;
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full, data)?;
        Ok(())
    }

    fn load(&self, path: &str) -> Result<Vec<u8>> {
        let full = self.resolve_path(path)?;
        let data = std::fs::read(&full)
            .map_err(|e| anyhow!("读取文件失败 {path}: {e}"))?;
        Ok(data)
    }

    fn delete(&self, path: &str) -> Result<()> {
        let full = self.resolve_path(path)?;
        if full.exists() {
            std::fs::remove_file(&full)?;
        }
        Ok(())
    }

    fn exists(&self, path: &str) -> Result<bool> {
        let full = self.resolve_path(path)?;
        Ok(full.exists())
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  单元测试
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_storage() -> (LocalStorage, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "drafftink_test_storage_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let storage = LocalStorage::new(&dir).expect("创建测试存储失败");
        (storage, dir)
    }

    #[test]
    fn test_save_and_load() {
        let (storage, _dir) = temp_storage();
        let data = b"hello world".to_vec();
        storage.save("test/file.txt", data.clone()).unwrap();

        let loaded = storage.load("test/file.txt").unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_exists() {
        let (storage, _dir) = temp_storage();
        assert!(!storage.exists("missing.txt").unwrap());

        storage.save("exists.txt", b"data".to_vec()).unwrap();
        assert!(storage.exists("exists.txt").unwrap());
    }

    #[test]
    fn test_delete() {
        let (storage, _dir) = temp_storage();
        storage.save("del.txt", b"data".to_vec()).unwrap();
        assert!(storage.exists("del.txt").unwrap());

        storage.delete("del.txt").unwrap();
        assert!(!storage.exists("del.txt").unwrap());
    }

    #[test]
    fn test_delete_nonexistent() {
        let (storage, _dir) = temp_storage();
        // 删除不存在的文件不应报错
        storage.delete("nonexistent.txt").unwrap();
    }

    #[test]
    fn test_nested_dirs() {
        let (storage, _dir) = temp_storage();
        let data = b"nested".to_vec();
        storage
            .save("a/b/c/deep.txt", data.clone())
            .unwrap();

        let loaded = storage.load("a/b/c/deep.txt").unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_directory_traversal_blocked() {
        let (storage, _dir) = temp_storage();
        let result = storage.save("../escape.txt", b"data".to_vec());
        assert!(result.is_err(), "目录遍历路径应被拒绝");
    }

    #[test]
    fn test_drive_letter_blocked() {
        let (storage, _dir) = temp_storage();
        let result = storage.save("C:/Windows/System32/evil.txt", b"data".to_vec());
        assert!(result.is_err(), "驱动器号路径应被拒绝");
    }
}
