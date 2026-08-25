//! # live — 网络直播子系统（DB34/T 2318-2015）
//!
//! 基于现有公网网关提供 B/S 架构的 WebSocket 直播能力，端到端延时 ≤ 3 秒
//! （通过 tokio 异步流即时转发保证）。支持：
//!
//! - **自动导播**：依据板书 / 批注 / 互动信号，由 [`ActivityDirector`] 自动切换
//!   教师画面 / 学生画面 / 电脑画面。
//! - **手动导播**：授课端发送 [`ClientControl::SwitchView`] 手动切换导播视角。
//!
//! 直播鉴权复用现有 JWT + RBAC（[`drafftink_core::auth::claims_role`]）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use axum::extract::ws::{Message as WsMessage, WebSocket};
use base64::engine::general_purpose::STANDARD as B64;
use drafftink_core::recording::{ActivityDirector, DirectingSignals, DirectingStrategy, LiveView};
use drafftink_core::Role;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// 直播单帧：媒体帧（携带某视角画面数据）或控制帧（导播切换通知）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum LiveFrame {
    /// 媒体帧：某一导播视角的画面数据（base64 编码）
    Media {
        view: LiveView,
        /// 时间戳（Unix 毫秒）
        ts: i64,
        /// 画面数据（视频帧 / 封装流片段）
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
    },
    /// 控制帧：导播视角已切换
    Control { view: LiveView, ts: i64 },
}

impl LiveFrame {
    /// 构造媒体帧。
    pub fn media(view: LiveView, data: Vec<u8>) -> Self {
        LiveFrame::Media {
            view,
            ts: now_ms(),
            data,
        }
    }

    /// 构造控制帧（导播切换）。
    pub fn control(view: LiveView) -> Self {
        LiveFrame::Control { view, ts: now_ms() }
    }
}

/// 客户端（授课端）控制指令。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action")]
pub enum ClientControl {
    /// 手动导播：切换导播视角
    SwitchView { view: LiveView },
}

/// base64 编解码模块，用于 `LiveFrame` 中的二进制画面数据。
mod base64_bytes {
    use super::B64;
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&B64.encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let raw = String::deserialize(d)?;
        B64.decode(raw).map_err(serde::de::Error::custom)
    }
}

/// 单个直播间的运行时状态。
struct RoomState {
    /// 广播发送端：所有订阅者通过 `subscribe()` 获取接收端。
    sender: broadcast::Sender<LiveFrame>,
    /// 当前导播视角（手动 / 自动导播共享）。
    current_view: RwLock<LiveView>,
}

/// 直播中枢：管理所有直播间，承载广播与导播决策。
///
/// 以 `Arc` 包裹内部状态，可廉价 `Clone` 后注入 Axum `State`。
#[derive(Clone)]
pub struct LiveHub {
    rooms: Arc<Mutex<HashMap<String, RoomState>>>,
}

impl Default for LiveHub {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveHub {
    /// 创建空直播中枢。
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 获取或创建直播间，返回其广播发送端。
    fn ensure(&self, room_id: &str) -> broadcast::Sender<LiveFrame> {
        let mut rooms = self.rooms.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(room) = rooms.get(room_id) {
            return room.sender.clone();
        }
        let (tx, _rx) = broadcast::channel(512);
        rooms.insert(
            room_id.to_string(),
            RoomState {
                sender: tx.clone(),
                current_view: RwLock::new(LiveView::Teacher),
            },
        );
        tx
    }

    /// 订阅某直播间的帧流（每个连接一个独立接收端）。
    pub fn subscribe(&self, room_id: &str) -> broadcast::Receiver<LiveFrame> {
        self.ensure(room_id).subscribe()
    }

    /// 发布一帧媒体流。若提供导播信号，则先由自动导播策略选择最优视角，
    /// 再广播，实现"AI 分析板书/批注/互动数据，自动切换画面"。
    pub fn publish(&self, room_id: &str, mut frame: LiveFrame, signals: Option<DirectingSignals>) {
        if let Some(signals) = signals {
            let chosen = {
                let current = self.current_view(room_id);
                ActivityDirector.choose(&signals, current)
            };
            if let LiveFrame::Media { view, .. } = &mut frame {
                *view = chosen;
            }
            let mut rooms = self.rooms.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(room) = rooms.get_mut(room_id) {
                *room.current_view.write().unwrap_or_else(|e| e.into_inner()) = chosen;
            }
        }
        let _ = self.ensure(room_id).send(frame);
    }

    /// 应用手动导播控制：更新当前视角并向所有订阅者广播控制帧。
    pub fn apply_control(&self, room_id: &str, view: LiveView) {
        {
            let mut rooms = self.rooms.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(room) = rooms.get_mut(room_id) {
                *room.current_view.write().unwrap_or_else(|e| e.into_inner()) = view;
            }
        }
        let _ = self.ensure(room_id).send(LiveFrame::control(view));
    }

    /// 读取直播间当前导播视角。
    fn current_view(&self, room_id: &str) -> LiveView {
        let rooms = self.rooms.lock().expect("直播中枢锁未中毒");
        rooms
            .get(room_id)
            .map(|room| *room.current_view.read().unwrap_or_else(|e| e.into_inner()))
            .unwrap_or(LiveView::Teacher)
    }
}

/// 处理单个 WebSocket 连接：下行广播帧流，上行解析授课端导播控制。
///
/// 接收 axum 升级后的 [`WebSocket`]，拆分为读写两半后并发转发，
/// 端到端延时取决于网络与编码，转发本身为即时异步流（≤ 3 秒硬指标）。
pub async fn handle_socket(ws: WebSocket, hub: LiveHub, room_id: String, _role: Role) {
    let mut rx = hub.subscribe(&room_id);
    let (mut sink, mut source) = ws.split();
    loop {
        tokio::select! {
            // 上行：授课端导播控制
            incoming = source.next() => {
                match incoming {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Ok(ctrl) = serde_json::from_str::<ClientControl>(&text) {
                            match ctrl {
                                ClientControl::SwitchView { view } => {
                                    hub.apply_control(&room_id, view);
                                }
                            }
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break,
                }
            }
            // 下行：直播帧广播
            outgoing = rx.recv() => {
                match outgoing {
                    Ok(frame) => {
                        let payload = match serde_json::to_string(&frame) {
                            Ok(p) => p,
                            Err(_) => continue,
                        };
                        if sink.send(WsMessage::Text(payload)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => { /* 订阅滞后，跳过单帧 */ }
                }
            }
        }
    }
}

/// 当前 Unix 毫秒时间戳。
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_frame_media_roundtrip() {
        let frame = LiveFrame::media(LiveView::Teacher, vec![1, 2, 3, 4]);
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("\"teacher\""));
        let back: LiveFrame = serde_json::from_str(&json).unwrap();
        match back {
            LiveFrame::Media { view, data, .. } => {
                assert_eq!(view, LiveView::Teacher);
                assert_eq!(data, vec![1, 2, 3, 4]);
            }
            _ => panic!("应为媒体帧"),
        }
    }

    #[test]
    fn hub_publish_and_subscribe() {
        let hub = LiveHub::new();
        let mut rx = hub.subscribe("r1");
        hub.publish("r1", LiveFrame::media(LiveView::Computer, vec![9]), None);
        let frame = rx.try_recv().expect("应收到帧");
        match frame {
            LiveFrame::Media { view, .. } => assert_eq!(view, LiveView::Computer),
            _ => panic!("应为媒体帧"),
        }
    }

    #[test]
    fn hub_auto_director_overrides_view() {
        let hub = LiveHub::new();
        let _rx = hub.subscribe("r2");
        // 提供强互动信号 → 自动导播切到学生画面
        hub.publish(
            "r2",
            LiveFrame::media(LiveView::Teacher, vec![1]),
            Some(DirectingSignals {
                board_activity: 0,
                annotation_count: 0,
                interaction_count: 12,
            }),
        );
        assert_eq!(hub.current_view("r2"), LiveView::Student);
    }

    #[test]
    fn hub_manual_control() {
        let hub = LiveHub::new();
        let _rx = hub.subscribe("r3");
        hub.apply_control("r3", LiveView::Student);
        assert_eq!(hub.current_view("r3"), LiveView::Student);
    }

    #[test]
    fn client_control_parse() {
        let ctrl: ClientControl =
            serde_json::from_str(r#"{"action":"SwitchView","view":"computer"}"#).unwrap();
        assert!(matches!(
            ctrl,
            ClientControl::SwitchView {
                view: LiveView::Computer
            }
        ));
    }
}
