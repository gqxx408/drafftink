//! 答题会话 Actor（核心）
//!
//! Session Actor 是唯一有权修改 QuizSession 状态的实体。
//! 它接收来自 IM、USB、UI 的消息，通过 mpsc 单线程调度，
//! 不存在数据竞争，不需要 Mutex。
//!
//! # 架构
//! ```text
//! IM Actor ──→ Session Actor ←── USB Actor
//!                   │
//!                   ↓
//!              UI Proxy Actor → egui
//! ```

use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::error::QuizError;
use crate::messages::*;
use crate::types::*;

// ── 内部状态 ────────────────────────────────────────────────────

/// 答题会话内部状态
///
/// 所有字段私有，外部只能通过消息与之交互。
struct QuizSession {
    /// 会话 ID
    id: SessionId,
    /// 会话状态
    status: SessionStatus,
    /// 学生列表（ID → 信息）
    students: HashMap<StudentId, StudentInfo>,
    /// 在线学生 ID 集合
    online_students: Vec<StudentId>,
    /// 题目列表
    questions: Vec<Question>,
    /// 当前题目索引
    current_question_idx: Option<usize>,
    /// 答题记录：题目 ID → (学生 ID → 答案)
    answers: HashMap<QuestionId, HashMap<StudentId, AnswerRecord>>,
    /// 题目统计：题目 ID → 统计
    stats: HashMap<QuestionId, QuestionStats>,
    /// 抢答胜利者：题目 ID → 结果（第一个抢到的获胜）
    quick_answer_winners: HashMap<QuestionId, QuickAnswerResult>,
    /// 题目开始时间（用于计算答题耗时）
    question_start_time: Option<Instant>,
    /// 题目时间限制（秒）
    question_time_limit: u32,
}

impl QuizSession {
    fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            status: SessionStatus::Waiting,
            students: HashMap::new(),
            online_students: Vec::new(),
            questions: Vec::new(),
            current_question_idx: None,
            answers: HashMap::new(),
            stats: HashMap::new(),
            quick_answer_winners: HashMap::new(),
            question_start_time: None,
            question_time_limit: 0,
        }
    }

    // ── 学生管理 ──────────────────────────────────────────────

    fn student_join(&mut self, id: StudentId, name: String, device_id: Option<String>) {
        let now = Instant::now();
        let now_ms = now.elapsed().as_millis() as u64; // 近似，实际应用中使用 SystemTime
        if !self.students.contains_key(&id) {
            self.students.insert(
                id.clone(),
                StudentInfo {
                    id: id.clone(),
                    name,
                    device_id,
                    connected: true,
                    last_heartbeat_ms: now_ms,
                    last_heartbeat: Some(now),
                },
            );
        }
        if !self.online_students.contains(&id) {
            self.online_students.push(id.clone());
        }
        if let Some(s) = self.students.get_mut(&id) {
            s.connected = true;
            s.last_heartbeat = Some(Instant::now());
            s.last_heartbeat_ms = Instant::now().elapsed().as_millis() as u64;
        }
    }

    fn student_leave(&mut self, id: &StudentId) {
        self.online_students.retain(|s| s != id);
        if let Some(s) = self.students.get_mut(id) {
            s.connected = false;
        }
    }

    fn heartbeat(&mut self, id: &StudentId) {
        if let Some(s) = self.students.get_mut(id) {
            s.last_heartbeat = Some(Instant::now());
            s.last_heartbeat_ms = Instant::now().elapsed().as_millis() as u64;
        }
    }

    // ── 题目管理 ──────────────────────────────────────────────

    fn start_question(&mut self, question: Question) -> Result<(), QuizError> {
        if matches!(self.status, SessionStatus::Ended) {
            return Err(QuizError::Session("会话已结束".into()));
        }
        self.status = SessionStatus::Active;
        self.question_start_time = Some(Instant::now());
        self.question_time_limit = question.time_limit_sec;
        self.current_question_idx = Some(self.questions.len());
        self.questions.push(question.clone());
        // 初始化统计
        self.stats.insert(
            question.id.clone(),
            QuestionStats {
                question_id: question.id.clone(),
                unanswered_count: self.students.len() as u32,
                ..Default::default()
            },
        );
        self.answers.insert(question.id.clone(), HashMap::new());
        Ok(())
    }

    fn end_question(&mut self, question_id: &QuestionId) -> Result<QuestionStats, QuizError> {
        let mut stats = self.stats.remove(question_id).unwrap_or_default();
        stats.set_unanswered(self.students.len() as u32);
        self.status = SessionStatus::Waiting;
        self.question_start_time = None;
        self.current_question_idx = None;
        Ok(stats)
    }

    // ── 答题处理 ──────────────────────────────────────────────

    fn submit_answer(
        &mut self,
        student_id: StudentId,
        question_id: QuestionId,
        answer: StudentAnswer,
        timestamp_ns: u64,
    ) -> Result<AnswerRecord, QuizError> {
        // 会话检查
        if self.status != SessionStatus::Active {
            return Err(QuizError::Session("当前没有活跃的题目".into()));
        }

        // 题目检查
        let question = self
            .questions
            .iter()
            .find(|q| q.id == question_id)
            .ok_or_else(|| QuizError::Session("题目不存在".into()))?;

        // 重复答题检查
        if let Some(answers) = self.answers.get(&question_id) {
            if answers.contains_key(&student_id) {
                return Err(QuizError::Session("该学生已提交答案".into()));
            }
        }

        // 学生存在性检查
        if !self.students.contains_key(&student_id) {
            return Err(QuizError::Session("学生未加入会话".into()));
        }

        // 时间限制检查
        let within_time_limit = if let Some(start) = self.question_start_time {
            if self.question_time_limit > 0 {
                let elapsed = start.elapsed().as_secs();
                elapsed <= self.question_time_limit as u64
            } else {
                true
            }
        } else {
            true
        };

        // 计算响应时间
        let response_time_ms = self
            .question_start_time
            .map(|start| start.elapsed().as_millis() as u64)
            .unwrap_or(0);

        // 判分
        let is_correct = question
            .correct_answer
            .as_ref()
            .map(|correct| answer.is_correct(correct))
            .unwrap_or(false);

        let score = if is_correct { question.score } else { 0 };

        let record = AnswerRecord {
            student_id: student_id.clone(),
            question_id: question_id.clone(),
            answer: answer.clone(),
            within_time_limit,
            response_time_ms,
            is_correct,
            score,
        };

        // 存入答题记录
        self.answers
            .entry(question_id.clone())
            .or_default()
            .insert(student_id.clone(), record.clone());

        // 更新统计
        if let Some(stats) = self.stats.get_mut(&question_id) {
            stats.update(&record);
        }

        let _ = timestamp_ns; // 非抢答题不使用时间戳

        Ok(record)
    }

    // ── 抢答 ──────────────────────────────────────────────────

    fn quick_answer_buzz(
        &mut self,
        student_id: StudentId,
        question_id: QuestionId,
        _timestamp_ns: u64,
    ) -> Result<QuickAnswerResult, QuizError> {
        if self.status != SessionStatus::Active {
            return Err(QuizError::Session("当前没有活跃的题目".into()));
        }

        let student = self
            .students
            .get(&student_id)
            .ok_or_else(|| QuizError::Session("学生不存在".into()))?;

        // 第一个收到的就是赢家（mpsc 保证顺序）
        if self.quick_answer_winners.contains_key(&question_id) {
            return Err(QuizError::Session("已有学生抢答成功".into()));
        }

        let response_time_ms = self
            .question_start_time
            .map(|start| start.elapsed().as_millis() as u64)
            .unwrap_or(0);

        let result = QuickAnswerResult {
            question_id: question_id.clone(),
            winner_id: student_id.clone(),
            winner_name: student.name.clone(),
            response_time_ms,
            is_tie: false,
        };

        self.quick_answer_winners
            .insert(question_id, result.clone());

        Ok(result)
    }

    // ── 快照 ──────────────────────────────────────────────────

    fn snapshot(&self) -> SessionSnapshot {
        let current_question = self
            .current_question_idx
            .and_then(|idx| self.questions.get(idx).cloned());

        let current_stats = current_question
            .as_ref()
            .and_then(|q| self.stats.get(&q.id).cloned());

        let students: Vec<StudentInfo> = self.students.values().cloned().collect();

        SessionSnapshot {
            session_id: self.id.clone(),
            status: self.status,
            students,
            current_question,
            current_stats,
            total_answers: self.answers.values().map(|m| m.len()).sum(),
            total_students: self.students.len(),
            online_students: self.online_students.len(),
        }
    }
}

// ── Actor 入口 ──────────────────────────────────────────────────

/// 启动 Session Actor，返回命令发送端和 UI 事件接收端。
///
/// # 用法
/// ```ignore
/// let (session_tx, ui_rx) = start_session_actor();
/// // 将 session_tx 传给 IM Actor 和 USB Actor
/// // 在 egui 线程中读取 ui_rx
/// ```
pub fn start_session_actor() -> (mpsc::Sender<SessionCommand>, mpsc::Receiver<UiEvent>) {
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<SessionCommand>(1024);
    let (ui_tx, ui_rx) = mpsc::channel::<UiEvent>(256);

    tokio::spawn(async move {
        let mut session = QuizSession::new();

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                SessionCommand::StudentJoin {
                    student_id,
                    student_name,
                    device_id,
                    reply,
                } => {
                    session.student_join(student_id.clone(), student_name.clone(), device_id);
                    let _ = reply.send(Ok(()));
                    let _ = ui_tx
                        .send(UiEvent::StudentJoined {
                            student_id,
                            student_name,
                        })
                        .await;
                }

                SessionCommand::StudentLeave { student_id } => {
                    session.student_leave(&student_id);
                    let _ = ui_tx.send(UiEvent::StudentLeft { student_id }).await;
                }

                SessionCommand::SubmitAnswer {
                    student_id,
                    question_id,
                    answer,
                    timestamp_ns,
                    reply,
                } => {
                    let result =
                        session.submit_answer(student_id, question_id, answer, timestamp_ns);
                    let _ = reply.send(result.map_err(|e| e.to_string()));
                    // 推送统计更新
                    let snapshot = session.snapshot();
                    let _ = ui_tx.send(UiEvent::SnapshotUpdated(snapshot)).await;
                }

                SessionCommand::QuickAnswerBuzz {
                    student_id,
                    question_id,
                    timestamp_ns,
                    reply,
                } => {
                    let result = session.quick_answer_buzz(student_id, question_id, timestamp_ns);
                    // 抢答成功 → 推送 UI 事件
                    if let Ok(ref winner) = result {
                        let _ = ui_tx.send(UiEvent::QuickAnswerWinner(winner.clone())).await;
                    }
                    // 通知客户端结果（无论成功失败）
                    let _ = reply.send(result.map_err(|e| e.to_string()));
                }

                SessionCommand::StartQuestion { question, reply } => {
                    let result = session.start_question(question);
                    let _ = reply.send(result.map_err(|e| e.to_string()));
                    let snapshot = session.snapshot();
                    let _ = ui_tx.send(UiEvent::SnapshotUpdated(snapshot)).await;
                }

                SessionCommand::EndQuestion { question_id, reply } => {
                    let result = session.end_question(&question_id);
                    match &result {
                        Ok(stats) => {
                            let _ = ui_tx.send(UiEvent::StatsUpdated(stats.clone())).await;
                        }
                        Err(_) => {}
                    }
                    let _ = reply.send(result.map_err(|e| e.to_string()));
                }

                SessionCommand::EndSession { reply } => {
                    session.status = SessionStatus::Ended;
                    let all_answers: Vec<AnswerRecord> = session
                        .answers
                        .values()
                        .flat_map(|m| m.values().cloned())
                        .collect();
                    let total = all_answers.len();
                    let _ = reply.send(Ok(all_answers));
                    let _ = ui_tx
                        .send(UiEvent::SessionEnded {
                            total_answers: total,
                        })
                        .await;
                }

                SessionCommand::GetSnapshot { reply } => {
                    let _ = reply.send(session.snapshot());
                }

                SessionCommand::GetStats { question_id, reply } => {
                    let _ = reply.send(session.stats.get(&question_id).cloned());
                }

                SessionCommand::SetPause { paused } => {
                    session.status = if paused {
                        SessionStatus::Paused
                    } else {
                        SessionStatus::Active
                    };
                }

                SessionCommand::UsbEvent {
                    device_id,
                    connected,
                } => {
                    let _ = ui_tx
                        .send(UiEvent::UsbDeviceChanged {
                            device_id,
                            connected,
                        })
                        .await;
                }

                SessionCommand::Heartbeat { student_id } => {
                    session.heartbeat(&student_id);
                }
            }
        }
    });

    (cmd_tx, ui_rx)
}
