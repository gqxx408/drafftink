//! Actor 消息定义
//!
//! 所有 Actor 之间通过 mpsc::channel 通信，消息类型在此统一声明。
//! 不共享内存，只传递消息 — 消灭数据竞争。

use tokio::sync::oneshot;

use crate::types::*;

// ── Session Actor 接收的消息 ─────────────────────────────────────

/// 发送给 Session Actor 的命令
///
/// Session Actor 是唯一有权修改 QuizSession 状态的实体。
/// 所有外部 Actor（IM、USB、UI）都通过此消息与 Session 通信。
///
/// 注意：手动实现 Debug 而非 derive，因为 oneshot::Sender 不实现 Debug。
pub enum SessionCommand {
    /// 学生上线（IM Actor 发送）
    StudentJoin {
        student_id: StudentId,
        student_name: String,
        device_id: Option<String>,
        /// 回复通道：返回是否成功加入
        reply: oneshot::Sender<Result<(), String>>,
    },

    /// 学生离线（IM Actor 发送）
    StudentLeave {
        student_id: StudentId,
    },

    /// 学生提交答案（IM Actor 发送）
    SubmitAnswer {
        student_id: StudentId,
        question_id: QuestionId,
        answer: StudentAnswer,
        /// 学生端时间戳（纳秒），用于抢答裁决
        timestamp_ns: u64,
        /// 回复通道：返回判分结果
        reply: oneshot::Sender<Result<AnswerRecord, String>>,
    },

    /// 抢答（IM Actor 发送）
    QuickAnswerBuzz {
        student_id: StudentId,
        question_id: QuestionId,
        timestamp_ns: u64,
        reply: oneshot::Sender<Result<QuickAnswerResult, String>>,
    },

    /// 开始一道新题（UI Actor 发送）
    StartQuestion {
        question: Question,
        reply: oneshot::Sender<Result<(), String>>,
    },

    /// 结束当前题目（UI Actor 发送）
    EndQuestion {
        question_id: QuestionId,
        reply: oneshot::Sender<Result<QuestionStats, String>>,
    },

    /// 结束整个会话（UI Actor 发送）
    EndSession {
        /// 回复通道：返回最终统计
        reply: oneshot::Sender<Result<Vec<AnswerRecord>, String>>,
    },

    /// 获取当前会话快照（UI Actor 定期轮询）
    GetSnapshot {
        reply: oneshot::Sender<SessionSnapshot>,
    },

    /// 获取当前题目统计（UI Actor 轮询）
    GetStats {
        question_id: QuestionId,
        reply: oneshot::Sender<Option<QuestionStats>>,
    },

    /// 暂停/恢复会话
    SetPause {
        paused: bool,
    },

    /// USB 设备插拔事件（USB Actor 发送）
    UsbEvent {
        device_id: String,
        connected: bool,
    },

    /// 心跳（IM Actor 定期发送）
    Heartbeat {
        student_id: StudentId,
    },
}

// ── UI Actor 接收的消息 ─────────────────────────────────────────

/// UI Actor 接收的事件
///
/// UI Actor 不修改任何业务状态，只负责将 Session 的状态变化
/// 转换为 egui 可渲染的数据结构。
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// 会话快照更新
    SnapshotUpdated(SessionSnapshot),
    /// 统计更新
    StatsUpdated(QuestionStats),
    /// 新学生加入
    StudentJoined {
        student_id: StudentId,
        student_name: String,
    },
    /// 学生离开
    StudentLeft {
        student_id: StudentId,
    },
    /// 抢答结果
    QuickAnswerWinner(QuickAnswerResult),
    /// 会话结束
    SessionEnded {
        total_answers: usize,
    },
    /// 错误提示
    Error(String),
    /// USB 设备状态变化
    UsbDeviceChanged {
        device_id: String,
        connected: bool,
    },
}

// ── 会话快照（只读）─────────────────────────────────────────────

/// 会话快照，UI Actor 用于渲染。
///
/// 这是一个纯数据对象，不包含任何可变引用。
/// UI Actor 只管读取它来画界面。
#[derive(Debug, Clone, Default)]
pub struct SessionSnapshot {
    /// 会话 ID
    pub session_id: String,
    /// 会话状态
    pub status: SessionStatus,
    /// 学生列表
    pub students: Vec<StudentInfo>,
    /// 当前题目
    pub current_question: Option<Question>,
    /// 当前题目统计
    pub current_stats: Option<QuestionStats>,
    /// 总答题数
    pub total_answers: usize,
    /// 学生总数
    pub total_students: usize,
    /// 在线学生数
    pub online_students: usize,
}

// ── 手动实现 Debug（因为 oneshot::Sender 不实现 Debug）────────────

impl std::fmt::Debug for SessionCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StudentJoin { student_id, student_name, device_id, .. } => f
                .debug_struct("StudentJoin")
                .field("student_id", student_id)
                .field("student_name", student_name)
                .field("device_id", device_id)
                .finish(),
            Self::StudentLeave { student_id } => f
                .debug_struct("StudentLeave")
                .field("student_id", student_id)
                .finish(),
            Self::SubmitAnswer { student_id, question_id, answer, timestamp_ns, .. } => f
                .debug_struct("SubmitAnswer")
                .field("student_id", student_id)
                .field("question_id", question_id)
                .field("answer", answer)
                .field("timestamp_ns", timestamp_ns)
                .finish(),
            Self::QuickAnswerBuzz { student_id, question_id, timestamp_ns, .. } => f
                .debug_struct("QuickAnswerBuzz")
                .field("student_id", student_id)
                .field("question_id", question_id)
                .field("timestamp_ns", timestamp_ns)
                .finish(),
            Self::StartQuestion { question, .. } => f
                .debug_struct("StartQuestion")
                .field("question", question)
                .finish(),
            Self::EndQuestion { question_id, .. } => f
                .debug_struct("EndQuestion")
                .field("question_id", question_id)
                .finish(),
            Self::EndSession { .. } => f.debug_struct("EndSession").finish(),
            Self::GetSnapshot { .. } => f.debug_struct("GetSnapshot").finish(),
            Self::GetStats { question_id, .. } => f
                .debug_struct("GetStats")
                .field("question_id", question_id)
                .finish(),
            Self::SetPause { paused } => f
                .debug_struct("SetPause")
                .field("paused", paused)
                .finish(),
            Self::UsbEvent { device_id, connected } => f
                .debug_struct("UsbEvent")
                .field("device_id", device_id)
                .field("connected", connected)
                .finish(),
            Self::Heartbeat { student_id } => f
                .debug_struct("Heartbeat")
                .field("student_id", student_id)
                .finish(),
        }
    }
}