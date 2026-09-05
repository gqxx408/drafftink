//! # hub_sync — school-hub 校本资源库同步
//!
//! 把白板当前文档 + 注解笔迹序列化为一个可读的 `WhiteboardSnapshot`，
//! 通过 `hub_sdk::HubClient` 异步保存到校本资源库（`POST /api/events`）。
//!
//! 设计约定：
//! - **不阻塞 UI**：所有网络调用在 tokio 运行时上以 `spawn` 异步执行，
//!   结果通过 `oneshot` 回传，主循环每帧 `poll` 一次。
//! - **失败静默**：同步失败只写 `log::error!`/`log::warn!` 并在按钮旁显示
//!   ❌，绝不弹错误对话框打扰老师。
//! - **author 用 user_id**：`HubClient` 内部已用登录后的用户 id（JWT `sub`）
//!   填充事件的 `author` 字段，而非 token（避免把敏感凭据写入事件）。

use std::sync::Arc;

use anyhow::Result;
use drafftink_core::document::StrokeData;
use drafftink_core::model::CoursewareDoc;
use hub_sdk::HubClient;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

// ---------------------------------------------------------------------------
// 可序列化的白板快照（serde_json，可读）
// ---------------------------------------------------------------------------

/// 一条可发送的笔迹：字段与 `StrokeData` 对齐，便于资源库侧解析/回放。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableStroke {
    pub points: Vec<[f32; 2]>,
    pub color: [u8; 4],
    pub thickness: f32,
    /// 0 = 画笔，1 = 荧光笔，2 = 橡皮（与 drafftink 注解层一致）。
    pub tool: u8,
}

impl From<&StrokeData> for SerializableStroke {
    fn from(s: &StrokeData) -> Self {
        Self {
            points: s.points.clone(),
            color: s.color,
            thickness: s.thickness,
            tool: s.tool,
        }
    }
}

/// 一个页面：元素 JSON + 笔迹列表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializablePage {
    /// 元素以懒解析的 JSON 值保留（资源库无需理解白板内部元素类型）。
    pub elements: Vec<serde_json::Value>,
    pub strokes: Vec<SerializableStroke>,
}

/// 白板快照，整体作为事件正文发送。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhiteboardSnapshot {
    pub format: String,
    pub version: String,
    pub page_size: [f32; 2],
    pub background_color: [u8; 4],
    pub pages: Vec<SerializablePage>,
    pub created_at_ms: u64,
}

impl WhiteboardSnapshot {
    /// 从真实白板文档构造快照。
    ///
    /// - `doc`：当前编辑/授课的 `CoursewareDoc`。
    /// - `live_strokes`：注解层尚未落盘的实时笔迹（来自 `EditApp::export_current_annotations`）。
    /// - `current_page`：`live_strokes` 所属的页面索引（0-based），用于并入对应页。
    pub fn from_doc(
        doc: &CoursewareDoc,
        live_strokes: Vec<StrokeData>,
        current_page: usize,
    ) -> Self {
        let pages = if !doc.pages.is_empty() {
            doc.pages
                .iter()
                .enumerate()
                .map(|(i, p)| SerializablePage {
                    elements: p.elements.iter().map(element_to_json).collect(),
                    strokes: {
                        let mut s = decode_annotations(&p.annotations_data);
                        if i == current_page && !live_strokes.is_empty() {
                            s.extend(live_strokes.iter().map(SerializableStroke::from));
                        }
                        s
                    },
                })
                .collect()
        } else {
            // 旧版单页文档：把顶层 elements 当作第 0 页。
            vec![SerializablePage {
                elements: doc.elements.iter().map(element_to_json).collect(),
                strokes: live_strokes.iter().map(SerializableStroke::from).collect(),
            }]
        };

        Self {
            format: "drafftink-whiteboard".to_string(),
            version: doc.version.clone(),
            page_size: doc.page_size,
            background_color: doc.background_color,
            pages,
            created_at_ms: now_ms(),
        }
    }
}

/// 单页注解层 `annotations_data`（bincode 编码的 `Vec<StrokeData>`）→ 可发送笔迹。
fn decode_annotations(bytes: &[u8]) -> Vec<SerializableStroke> {
    if bytes.is_empty() {
        return Vec::new();
    }
    match bincode::deserialize::<Vec<StrokeData>>(bytes) {
        Ok(v) => v.iter().map(SerializableStroke::from).collect(),
        Err(e) => {
            log::warn!("[hub] 解析注解层失败（按空处理）: {e}");
            Vec::new()
        }
    }
}

fn element_to_json(e: &drafftink_core::model::Element) -> serde_json::Value {
    serde_json::to_value(e).unwrap_or(serde_json::Value::Null)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// 同步状态
// ---------------------------------------------------------------------------

/// 按钮旁的状态指示。
#[derive(Debug, Clone, Default, PartialEq)]
pub enum SyncStatus {
    #[default]
    Idle,
    Syncing,
    Success,
    Failed(String),
}

// ---------------------------------------------------------------------------
// 应用内集成状态
// ---------------------------------------------------------------------------

/// 资源库同步的 UI + 异步状态机。由 `IntegratedApp` 持有并每帧驱动。
pub struct HubSyncState {
    /// 登录成功后持有的客户端（由后台任务回传）。
    client: Option<Arc<HubClient>>,
    /// 独立 tokio 运行时：从 UI 线程 `handle().spawn`，不依赖 eframe 自带运行时。
    rt: tokio::runtime::Runtime,
    /// 登录客户端（后台异步中持有）；成功后被移入 `client`。
    login_tx: Option<oneshot::Receiver<Result<Arc<HubClient>, String>>>,
    /// 保存结果回传通道。
    save_tx: Option<oneshot::Receiver<Result<(), String>>>,
    busy: bool,
    /// 登录对话框字段。
    pub show_login: bool,
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub logged_in: bool,
    pub status: SyncStatus,
}

impl Default for HubSyncState {
    fn default() -> Self {
        Self::new()
    }
}

impl HubSyncState {
    /// 创建同步状态机，并初始化一个用于后台网络调用的 tokio 运行时。
    pub fn new() -> Self {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("[hub] 创建 tokio 运行时失败: {e}");
                panic!("hub tokio runtime: {e}");
            }
        };
        Self {
            client: None,
            rt,
            login_tx: None,
            save_tx: None,
            busy: false,
            show_login: false,
            base_url: "http://127.0.0.1:8080".to_string(),
            username: String::new(),
            password: String::new(),
            logged_in: false,
            status: SyncStatus::Idle,
        }
    }

    /// 当前登录后的客户端（`None` 表示未登录）。
    pub fn client(&self) -> Option<Arc<HubClient>> {
        self.client.clone()
    }

    /// 顶栏按钮 + 状态指示。返回是否点击了「保存」。
    pub fn show_in_toolbar(&mut self, ui: &mut egui::Ui) -> bool {
        let mut clicked = false;
        ui.separator();
        if self.logged_in {
            if ui.button("☁ 保存到资源库").clicked() {
                clicked = true;
            }
            match &self.status {
                SyncStatus::Idle => {}
                SyncStatus::Syncing => {
                    ui.colored_label(egui::Color32::from_rgb(200, 160, 0), "⏳");
                }
                SyncStatus::Success => {
                    ui.colored_label(egui::Color32::from_rgb(60, 180, 80), "✅");
                }
                SyncStatus::Failed(_) => {
                    ui.colored_label(egui::Color32::from_rgb(220, 60, 60), "❌");
                }
            }
        } else {
            if ui.button("☁ 登录资源库").clicked() {
                self.show_login = true;
            }
        }
        clicked
    }

    /// 每帧调用：回收异步结果并渲染登录对话框。
    pub fn update(&mut self, ctx: &egui::Context) {
        self.poll();
        if self.show_login {
            self.login_window(ctx);
        }
    }

    /// 触发异步保存。`client` 需为登录后的客户端，`payload` 为序列化快照字节。
    pub fn save_snapshot(&mut self, client: Arc<HubClient>, doc_id: &str, payload: Vec<u8>) {
        if self.busy {
            log::info!("[hub] 前一次保存仍在进行，忽略本次请求");
            return;
        }
        let (tx, rx) = oneshot::channel();
        self.save_tx = Some(rx);
        self.busy = true;
        self.status = SyncStatus::Syncing;

        let doc_id = doc_id.to_string();
        self.rt.handle().spawn(async move {
            let res = client.save_whiteboard(&doc_id, &payload).await;
            let _ = tx.send(res.map_err(|e| e.to_string()));
        });
    }

    /// 触发异步登录。成功后 `client` 就绪，`logged_in` 置位。
    pub fn start_login(&mut self) {
        if self.busy || self.base_url.trim().is_empty() || self.username.trim().is_empty() {
            self.status = SyncStatus::Failed("登录信息不完整".to_string());
            return;
        }
        let (tx, rx) = oneshot::channel();
        self.login_tx = Some(rx);
        self.busy = true;
        self.status = SyncStatus::Syncing;

        let base = self.base_url.trim().to_string();
        let user = self.username.trim().to_string();
        let pass = self.password.clone();
        let client = HubClient::new(&base, "");
        self.rt.handle().spawn(async move {
            let res = client.login(&user, &pass).await.map(|_| Arc::new(client));
            let _ = tx.send(res.map_err(|e| e.to_string()));
        });
    }

    /// 回收异步结果，更新状态。
    fn poll(&mut self) {
        if let Some(rx) = self.login_tx.as_mut() {
            if let Ok(res) = rx.try_recv() {
                self.login_tx = None;
                self.busy = false;
                match res {
                    Ok(c) => {
                        self.client = Some(c);
                        self.logged_in = true;
                        self.show_login = false;
                        self.status = SyncStatus::Idle;
                        log::info!("[hub] 已登录校本资源库");
                    }
                    Err(e) => {
                        self.logged_in = false;
                        self.client = None;
                        self.status = SyncStatus::Failed(e.clone());
                        log::warn!("[hub] 登录失败（仅记录，不打扰老师）: {e}");
                    }
                }
            }
        }

        if let Some(rx) = self.save_tx.as_mut() {
            if let Ok(res) = rx.try_recv() {
                self.save_tx = None;
                self.busy = false;
                match res {
                    Ok(()) => {
                        self.status = SyncStatus::Success;
                        log::info!("[hub] 白板快照保存成功");
                    }
                    Err(e) => {
                        self.status = SyncStatus::Failed(e.clone());
                        log::error!("[hub] 白板快照保存失败: {e}");
                    }
                }
            }
        }
    }

    /// 登录对话框（资源库服务地址 / 用户名 / 密码）。
    fn login_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_login;
        let mut want_login = false;

        egui::Window::new("登录校本资源库")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.base_url)
                        .desired_width(260.0)
                        .hint_text("服务地址，如 http://127.0.0.1:8080"),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.username)
                        .desired_width(260.0)
                        .hint_text("用户名"),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.password)
                        .desired_width(260.0)
                        .password(true)
                        .hint_text("密码"),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let busy = self.busy;
                    if ui.add_enabled(!busy, egui::Button::new("登录")).clicked() {
                        want_login = true;
                    }
                    if ui.button("取消").clicked() {
                        open = false;
                    }
                });
            });

        self.show_login = open;
        if want_login {
            self.start_login();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drafftink_core::model::PageContent;

    #[test]
    fn stroke_to_serializable_roundtrip() {
        let s = StrokeData {
            points: vec![[0.0, 0.0], [10.0, 10.0]],
            color: [255, 0, 0, 255],
            thickness: 3.0,
            tool: 0,
        };
        let ss = SerializableStroke::from(&s);
        let json = serde_json::to_string(&ss).unwrap();
        assert!(json.contains("[10.0,10.0]"));
    }

    #[test]
    fn snapshot_from_doc_single_page() {
        let mut doc = CoursewareDoc::default();
        doc.elements.push(drafftink_core::model::Element::Text(
            drafftink_core::model::TextElement {
                base: Default::default(),
                text: "hello".into(),
                font_size: 24.0,
                font_family: "sans-serif".into(),
            },
        ));
        let stroke = vec![StrokeData {
            points: vec![[1.0, 2.0]],
            color: [0, 0, 0, 255],
            thickness: 2.0,
            tool: 0,
        }];
        let snap = WhiteboardSnapshot::from_doc(&doc, stroke, 0);
        assert_eq!(snap.pages.len(), 1);
        assert_eq!(snap.pages[0].elements.len(), 1);
        assert_eq!(snap.pages[0].strokes.len(), 1);
    }

    #[test]
    fn snapshot_from_doc_multipage_merges_live() {
        let mut doc = CoursewareDoc::default();
        doc.pages = vec![PageContent::default(), PageContent::default()];
        let live = vec![StrokeData {
            points: vec![[5.0, 5.0]],
            color: [0, 0, 255, 255],
            thickness: 1.5,
            tool: 1,
        }];
        let snap = WhiteboardSnapshot::from_doc(&doc, live, 1);
        assert_eq!(snap.pages.len(), 2);
        assert_eq!(snap.pages[0].strokes.len(), 0);
        assert_eq!(snap.pages[1].strokes.len(), 1);
    }
}