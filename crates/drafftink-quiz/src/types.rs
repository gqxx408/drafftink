//! 核心数据类型
//!
//! 无 null、无魔法数字、无隐式类型转换。
//! 编译器在编译期就检查所有状态转换的合法性。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

// ── 基础标识符 ──────────────────────────────────────────────────

/// 学生唯一标识
pub type StudentId = String;

/// 题目唯一标识
pub type QuestionId = String;

/// 答题会话唯一标识
pub type SessionId = String;

/// 选项索引（0=A, 1=B, ...）
pub type OptionIndex = u8;

// ── 题型（编译期穷举，消灭魔法数字）─────────────────────────

/// 题型枚举。
///
/// 希沃用 `int QuestionType { get; set; }` (1=单选 2=多选...)。
/// 这里用 Rust enum，编译器强制处理所有分支。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuestionType {
    /// 单选题
    SingleChoice,
    /// 多选题
    MultipleChoice,
    /// 判断题
    TrueFalse,
    /// 抢答题（毫秒级延迟）
    QuickAnswer,
    /// 主观题（文本）
    Text,
}

// ── 学生答案（强类型）─────────────────────────────────────────

/// 学生答案，根据题型不同携带不同数据。
///
/// 希沃用 `object` 或 `string` 存答案，需要运行时解析。
/// 这里用 enum + 关联数据，编译器保证类型安全。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StudentAnswer {
    /// 单选: A=0, B=1, C=2, D=3
    Single(OptionIndex),
    /// 多选: [A, C, D] = [0, 2, 3]
    Multiple(Vec<OptionIndex>),
    /// 判断: true/false
    Bool(bool),
    /// 主观题: 文本内容
    Text(String),
}

impl StudentAnswer {
    /// 与标准答案比较，判断是否正确
    pub fn is_correct(&self, correct: &CorrectAnswer) -> bool {
        match (self, correct) {
            (StudentAnswer::Single(a), CorrectAnswer::Single(b)) => a == b,
            (StudentAnswer::Multiple(a), CorrectAnswer::Multiple(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                let mut a_sorted = a.clone();
                let mut b_sorted = b.clone();
                a_sorted.sort();
                b_sorted.sort();
                a_sorted == b_sorted
            }
            (StudentAnswer::Bool(a), CorrectAnswer::Bool(b)) => a == b,
            // 主观题不自动判分，需要人工批改
            (StudentAnswer::Text(_), CorrectAnswer::Text(_)) => false,
            _ => false,
        }
    }
}

// ── 标准答案 ──────────────────────────────────────────────────

/// 标准答案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CorrectAnswer {
    Single(OptionIndex),
    Multiple(Vec<OptionIndex>),
    Bool(bool),
    Text(String),
}

// ── 题目定义 ──────────────────────────────────────────────────

/// 一道题目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    /// 题目 ID
    pub id: QuestionId,
    /// 题型
    pub question_type: QuestionType,
    /// 题干文本
    pub content: String,
    /// 选项列表（仅选择题/判断题有效）
    pub options: Vec<String>,
    /// 标准答案
    pub correct_answer: Option<CorrectAnswer>,
    /// 分值
    pub score: u32,
    /// 答题时间限制（秒），0 = 不限时
    pub time_limit_sec: u32,
}

// ── 学生信息 ──────────────────────────────────────────────────

/// 学生信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudentInfo {
    /// 学生 ID
    pub id: StudentId,
    /// 学生姓名
    pub name: String,
    /// 设备 ID（平板/手机/反馈器）
    pub device_id: Option<String>,
    /// 连接状态
    pub connected: bool,
    /// 最后心跳时间（毫秒时间戳，可序列化）
    #[serde(default)]
    pub last_heartbeat_ms: u64,
    /// 最后心跳 Instant（仅供内存使用，不序列化）
    #[serde(skip)]
    pub last_heartbeat: Option<Instant>,
}

// ── 答题记录 ──────────────────────────────────────────────────

/// 单次答题记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerRecord {
    /// 学生 ID
    pub student_id: StudentId,
    /// 题目 ID
    pub question_id: QuestionId,
    /// 学生答案
    pub answer: StudentAnswer,
    /// 是否在时限内
    pub within_time_limit: bool,
    /// 答题耗时（毫秒）
    pub response_time_ms: u64,
    /// 是否被判为正确
    pub is_correct: bool,
    /// 得分
    pub score: u32,
}

// ── 会话状态 ──────────────────────────────────────────────────

/// 答题会话状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SessionStatus {
    /// 等待学生连接
    #[default]
    Waiting,
    /// 正在答题（某道题已开启）
    Active,
    /// 已暂停（老师手动暂停）
    Paused,
    /// 已结束
    Ended,
}

// ── 主持人答案（聚合统计）──────────────────────────────────────

/// 一道题的实时统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuestionStats {
    /// 题目 ID
    pub question_id: QuestionId,
    /// 已提交答案人数
    pub answered_count: u32,
    /// 正确人数
    pub correct_count: u32,
    /// 各选项选择人数（索引 → 人数）
    pub option_distribution: HashMap<OptionIndex, u32>,
    /// 正确率 (0.0 ~ 1.0)
    pub accuracy: f32,
    /// 平均响应时间（毫秒）
    pub avg_response_time_ms: f64,
    /// 未答题人数
    pub unanswered_count: u32,
}

impl QuestionStats {
    /// 从答题记录更新统计
    pub fn update(&mut self, record: &AnswerRecord) {
        self.answered_count += 1;
        if record.is_correct {
            self.correct_count += 1;
        }
        self.accuracy = if self.answered_count > 0 {
            self.correct_count as f32 / self.answered_count as f32
        } else {
            0.0
        };
        // 更新选项分布
        match &record.answer {
            StudentAnswer::Single(idx) => {
                *self.option_distribution.entry(*idx).or_insert(0) += 1;
            }
            StudentAnswer::Multiple(indices) => {
                for idx in indices {
                    *self.option_distribution.entry(*idx).or_insert(0) += 1;
                }
            }
            StudentAnswer::Bool(b) => {
                let idx = if *b { 0 } else { 1 };
                *self.option_distribution.entry(idx).or_insert(0) += 1;
            }
            _ => {}
        }
        // 更新平均响应时间
        let n = self.answered_count as f64;
        self.avg_response_time_ms =
            (self.avg_response_time_ms * (n - 1.0) + record.response_time_ms as f64) / n;
    }

    /// 未答题人数 = 总人数 - 已答题人数
    pub fn set_unanswered(&mut self, total_students: u32) {
        self.unanswered_count = total_students.saturating_sub(self.answered_count);
    }
}

// ── 抢答结果 ──────────────────────────────────────────────────

/// 抢答结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickAnswerResult {
    /// 题目 ID
    pub question_id: QuestionId,
    /// 获胜者学生 ID
    pub winner_id: StudentId,
    /// 获胜者姓名
    pub winner_name: String,
    /// 响应时间（毫秒）
    pub response_time_ms: u64,
    /// 是否有多个学生同时抢答
    pub is_tie: bool,
}
