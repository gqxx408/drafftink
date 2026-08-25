//! 错误类型
//!
//! 不使用 panic!，所有错误都通过 Result<T, QuizError> 传播。
//! 即使 USB 断开、网络超时，系统也继续运行，只在 UI 中提示。

use std::fmt;

/// 答题系统错误类型
#[derive(Debug)]
pub enum QuizError {
    /// 网络错误（WebSocket 断开、连接超时等）
    Network(String),
    /// USB 设备错误（断开、驱动故障等）
    UsbDevice(String),
    /// 会话逻辑错误（重复答题、无效题目等）
    Session(String),
    /// 持久化错误（数据库读写失败）
    Persistence(String),
    /// 序列化错误
    Serialization(String),
    /// 内部通信错误（Actor 通道断开）
    Internal(String),
}

impl fmt::Display for QuizError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuizError::Network(msg) => write!(f, "网络错误: {}", msg),
            QuizError::UsbDevice(msg) => write!(f, "USB 设备错误: {}", msg),
            QuizError::Session(msg) => write!(f, "会话错误: {}", msg),
            QuizError::Persistence(msg) => write!(f, "持久化错误: {}", msg),
            QuizError::Serialization(msg) => write!(f, "序列化错误: {}", msg),
            QuizError::Internal(msg) => write!(f, "内部错误: {}", msg),
        }
    }
}

impl std::error::Error for QuizError {}

impl From<std::io::Error> for QuizError {
    fn from(e: std::io::Error) -> Self {
        QuizError::Internal(e.to_string())
    }
}

impl From<serde_json::Error> for QuizError {
    fn from(e: serde_json::Error) -> Self {
        QuizError::Serialization(e.to_string())
    }
}

impl From<sled::Error> for QuizError {
    fn from(e: sled::Error) -> Self {
        QuizError::Persistence(e.to_string())
    }
}
