//! # 存储抽象层
//!
//! `Storage` trait 定义文件存储接口，`LocalStorage` 基于本地文件系统实现。

pub mod local;

pub use local::LocalStorage;

use anyhow::Result;

/// 文件存储 trait
///
/// 方法为同步调用（本地文件系统操作），在 async handler 中直接使用。
pub trait Storage: Send + Sync {
    /// 保存数据到指定路径
    fn save(&self, path: &str, data: Vec<u8>) -> Result<()>;
    /// 从指定路径加载数据
    fn load(&self, path: &str) -> Result<Vec<u8>>;
    /// 删除指定路径的文件
    #[allow(dead_code)]
    fn delete(&self, path: &str) -> Result<()>;
    /// 检查文件是否存在
    #[allow(dead_code)]
    fn exists(&self, path: &str) -> Result<bool>;
}
