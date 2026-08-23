//! UI 代理 Actor
//!
//! 接收 Session Actor 推送的 UiEvent，维护一份只读状态快照，
//! 供 egui 渲染线程安全读取。
//!
//! 使用 Arc<Mutex<>> 保护共享状态，因为 egui 在单线程中运行，
//! 锁争用极低。

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use crate::messages::{SessionSnapshot, UiEvent};
use crate::types::QuickAnswerResult;

/// UI 代理的内部状态（线程安全）
#[derive(Clone, Default)]
pub struct UiState {
    /// 最新会话快照
    pub snapshot: Option<SessionSnapshot>,
    /// 最新抢答结果
    pub last_quick_answer: Option<QuickAnswerResult>,
    /// 最近错误消息
    pub last_error: Option<String>,
    /// 事件计数（用于 UI 刷新检测）
    pub event_count: u64,
    /// 上次读取的事件计数
    last_read_count: u64,
}

/// 启动 UI 代理 Actor
///
/// 在 tokio 运行时中消费 UiEvent 流，更新共享的 UiState。
/// egui 线程通过 `UiState` 的 Arc 引用读取最新数据。
pub fn start_ui_proxy(
    mut ui_rx: mpsc::Receiver<UiEvent>,
    state: Arc<Mutex<UiState>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = ui_rx.recv().await {
            let mut s = state.lock().unwrap();
            s.event_count += 1;

            match event {
                UiEvent::SnapshotUpdated(snapshot) => {
                    s.snapshot = Some(snapshot);
                }
                UiEvent::StatsUpdated(stats) => {
                    if let Some(ref mut snap) = s.snapshot {
                        snap.current_stats = Some(stats);
                    }
                }
                UiEvent::StudentJoined { student_id, student_name } => {
                    log::info!("[quiz-ui] 学生加入: {} ({})", student_name, student_id);
                }
                UiEvent::StudentLeft { student_id } => {
                    log::info!("[quiz-ui] 学生离开: {}", student_id);
                }
                UiEvent::QuickAnswerWinner(result) => {
                    log::info!(
                        "[quiz-ui] 抢答结果: {} 获胜 ({}ms)",
                        result.winner_name,
                        result.response_time_ms
                    );
                    s.last_quick_answer = Some(result);
                }
                UiEvent::SessionEnded { total_answers } => {
                    log::info!("[quiz-ui] 会话结束，共 {} 条答题记录", total_answers);
                }
                UiEvent::Error(msg) => {
                    log::error!("[quiz-ui] 错误: {}", msg);
                    s.last_error = Some(msg);
                }
                UiEvent::UsbDeviceChanged { device_id, connected } => {
                    log::info!(
                        "[quiz-ui] USB 设备 {}: {}",
                        if connected { "连接" } else { "断开" },
                        device_id
                    );
                }
            }
        }
    })
}

impl UiState {
    /// 检查是否有新事件（自上次读取后）
    pub fn has_new_events(&mut self) -> bool {
        let changed = self.event_count != self.last_read_count;
        self.last_read_count = self.event_count;
        changed
    }

    /// 获取快照引用
    pub fn snapshot(&self) -> Option<&SessionSnapshot> {
        self.snapshot.as_ref()
    }

    /// 获取并消费最新的错误消息
    pub fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }
}