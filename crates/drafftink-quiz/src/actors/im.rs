//! 网络通信 Actor（IM Actor）
//!
//! 管理 WebSocket 服务器，接收学生端连接、答案、抢答。
//! 协议使用 JSON（兼容性好，后续可切 Protobuf）。
//!
//! 每收到一条消息，立即通过 mpsc 转发给 Session Actor。

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::messages::SessionCommand;
use crate::types::StudentAnswer;

// ── 客户端消息 JSON 格式 ────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    #[serde(rename = "hello")]
    Hello {
        student_id: String,
        name: String,
        device_id: Option<String>,
    },
    #[serde(rename = "answer")]
    Answer {
        student_id: String,
        question_id: String,
        answer: AnswerPayload,
        #[serde(default)]
        timestamp_ns: u64,
    },
    #[serde(rename = "buzz")]
    Buzz {
        student_id: String,
        question_id: String,
        #[serde(default)]
        timestamp_ns: u64,
    },
    #[serde(rename = "heartbeat")]
    Heartbeat { student_id: String },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AnswerPayload {
    Single { single: u8 },
    Multiple { multiple: Vec<u8> },
    Bool { bool: bool },
    Text { text: String },
}

// ── 服务端消息 JSON 格式 ────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ServerMessage {
    #[serde(rename = "welcome")]
    Welcome { session_id: String },
    #[serde(rename = "question_start")]
    #[allow(dead_code)]
    QuestionStart {
        question_id: String,
        question_type: String,
        content: String,
        options: Vec<String>,
        time_limit_sec: u32,
    },
    #[serde(rename = "answer_result")]
    AnswerResult {
        question_id: String,
        is_correct: bool,
        score: u32,
    },
    #[serde(rename = "error")]
    Error { message: String },
}

// ── Actor 启动 ──────────────────────────────────────────────────

/// 启动 IM Actor，监听 WebSocket 端口。
///
/// 返回：(广播发送端, 任务句柄)
///
/// # 参数
/// - `addr`: 监听地址，如 "0.0.0.0:9000"
/// - `session_tx`: Session Actor 的命令通道
pub fn start_im_actor(
    addr: SocketAddr,
    session_tx: mpsc::Sender<SessionCommand>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let listener = match TcpListener::bind(addr).await {
            Ok(l) => {
                log::info!("[quiz-im] WebSocket 服务器启动: {}", addr);
                l
            }
            Err(e) => {
                log::error!("[quiz-im] 无法绑定端口 {}: {}", addr, e);
                return;
            }
        };

        while let Ok((stream, peer_addr)) = listener.accept().await {
            log::info!("[quiz-im] 新连接: {}", peer_addr);
            let tx = session_tx.clone();
            tokio::spawn(handle_connection(stream, peer_addr, tx));
        }
    })
}

/// 处理单个 WebSocket 连接
async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    session_tx: mpsc::Sender<SessionCommand>,
) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            log::error!("[quiz-im] WebSocket 握手失败 {}: {}", peer_addr, e);
            return;
        }
    };

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // 发送欢迎消息
    let welcome = ServerMessage::Welcome {
        session_id: "quiz-1".into(),
    };
    if let Ok(json) = serde_json::to_string(&welcome) {
        let _ = ws_sender.send(Message::Text(json)).await;
    }

    // 接收消息循环
    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(text) => {
                let response = handle_client_message(&text, &session_tx).await;

                // 发送回复（如果有）
                if let Some(reply_json) = response {
                    let _ = ws_sender.send(Message::Text(reply_json)).await;
                }
            }
            Message::Close(_) => {
                log::info!("[quiz-im] 客户端断开: {}", peer_addr);
                break;
            }
            Message::Ping(data) => {
                let _ = ws_sender.send(Message::Pong(data)).await;
            }
            _ => {}
        }
    }
}

/// 处理一条客户端消息，返回可选的回复 JSON
async fn handle_client_message(
    text: &str,
    session_tx: &mpsc::Sender<SessionCommand>,
) -> Option<String> {
    let msg: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            let err = ServerMessage::Error {
                message: format!("消息格式错误: {}", e),
            };
            return serde_json::to_string(&err).ok();
        }
    };

    match msg {
        ClientMessage::Hello {
            student_id,
            name,
            device_id,
        } => {
            let (reply_tx, reply_rx) = oneshot::channel();
            let _ = session_tx
                .send(SessionCommand::StudentJoin {
                    student_id: student_id.clone(),
                    student_name: name,
                    device_id,
                    reply: reply_tx,
                })
                .await;

            if let Ok(Ok(())) = reply_rx.await {
                log::info!("[quiz-im] 学生加入: {}", student_id);
            }
            None
        }

        ClientMessage::Answer {
            student_id,
            question_id,
            answer,
            timestamp_ns,
        } => {
            let parsed = match answer {
                AnswerPayload::Single { single } => StudentAnswer::Single(single),
                AnswerPayload::Multiple { multiple } => StudentAnswer::Multiple(multiple),
                AnswerPayload::Bool { bool: b } => StudentAnswer::Bool(b),
                AnswerPayload::Text { text } => StudentAnswer::Text(text),
            };

            let (reply_tx, reply_rx) = oneshot::channel();
            let _ = session_tx
                .send(SessionCommand::SubmitAnswer {
                    student_id: student_id.clone(),
                    question_id: question_id.clone(),
                    answer: parsed,
                    timestamp_ns,
                    reply: reply_tx,
                })
                .await;

            match reply_rx.await {
                Ok(Ok(record)) => {
                    let result = ServerMessage::AnswerResult {
                        question_id,
                        is_correct: record.is_correct,
                        score: record.score,
                    };
                    serde_json::to_string(&result).ok()
                }
                Ok(Err(e)) => {
                    let err = ServerMessage::Error { message: e };
                    serde_json::to_string(&err).ok()
                }
                Err(_) => None,
            }
        }

        ClientMessage::Buzz {
            student_id,
            question_id,
            timestamp_ns,
        } => {
            let (reply_tx, reply_rx) = oneshot::channel();
            let _ = session_tx
                .send(SessionCommand::QuickAnswerBuzz {
                    student_id,
                    question_id,
                    timestamp_ns,
                    reply: reply_tx,
                })
                .await;

            // 抢答结果 — 成功或失败都告知客户端
            match reply_rx.await {
                Ok(Ok(winner)) => {
                    let result = ServerMessage::AnswerResult {
                        question_id: winner.question_id,
                        is_correct: true,
                        score: 0,
                    };
                    serde_json::to_string(&result).ok()
                }
                _ => {
                    let err = ServerMessage::Error {
                        message: "抢答失败".into(),
                    };
                    serde_json::to_string(&err).ok()
                }
            }
        }

        ClientMessage::Heartbeat { student_id } => {
            let _ = session_tx
                .send(SessionCommand::Heartbeat { student_id })
                .await;
            None
        }
    }
}
