//! 持久化模块
//!
//! 基于 sled 嵌入式数据库，提供答题记录的持久化存储。
//! 断电后重启可自动恢复，无需 XML 文件。

use sled::Db;
use std::path::Path;

use crate::error::QuizError;
use crate::types::AnswerRecord;

/// 持久化存储
pub struct QuizStore {
    db: Db,
}

impl QuizStore {
    /// 打开或创建数据库
    ///
    /// # 参数
    /// - `path`: 数据库文件路径，如 `./quiz_data`
    pub fn open(path: impl AsRef<Path>) -> Result<Self, QuizError> {
        let path_ref = path.as_ref();
        let db = sled::open(path_ref)?;
        log::info!("[quiz-store] 数据库已打开: {:?}", path_ref);
        Ok(Self { db })
    }

    /// 保存答题记录
    ///
    /// 键格式: `answer:{session_id}:{question_id}:{student_id}`
    pub fn save_answer(
        &self,
        session_id: &str,
        record: &AnswerRecord,
    ) -> Result<(), QuizError> {
        let key = format!(
            "answer:{}:{}:{}",
            session_id, record.question_id, record.student_id
        );
        let value = serde_json::to_vec(record)?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }

    /// 加载指定会话的所有答题记录
    pub fn load_answers(
        &self,
        session_id: &str,
    ) -> Result<Vec<AnswerRecord>, QuizError> {
        let prefix = format!("answer:{}:", session_id);
        let mut records = Vec::new();

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            let record: AnswerRecord = serde_json::from_slice(&value)?;
            records.push(record);
        }

        Ok(records)
    }

    /// 删除指定会话的答题记录
    pub fn clear_session(&self, session_id: &str) -> Result<(), QuizError> {
        let prefix = format!("answer:{}:", session_id);
        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, _) = item?;
            self.db.remove(key)?;
        }
        Ok(())
    }

    /// 获取数据库大小（字节）
    pub fn size_bytes(&self) -> u64 {
        self.db.size_on_disk().unwrap_or(0)
    }

    /// 刷新到磁盘
    pub fn flush(&self) -> Result<(), QuizError> {
        self.db.flush()?;
        Ok(())
    }
}

impl Drop for QuizStore {
    fn drop(&mut self) {
        let _ = self.db.flush();
    }
}