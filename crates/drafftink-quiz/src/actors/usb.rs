//! USB 设备 Actor
//!
//! 监听 USB 硬件反馈器（HID 设备）的插拔事件。
//! 使用 rusb 库直接读写，无 DLL 依赖，即插即用。
//!
//! 当前为骨架实现 — rusb 依赖在需要时启用。

use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;

use crate::messages::SessionCommand;

/// 启动 USB 设备监控 Actor
///
/// 定期轮询 USB 设备列表（rusb 无原生热插拔事件），
/// 检测到反馈器设备时通知 Session Actor。
///
/// # 参数
/// - `session_tx`: Session Actor 命令通道
/// - `poll_interval`: 轮询间隔（默认 2 秒）
pub fn start_usb_actor(
    session_tx: mpsc::Sender<SessionCommand>,
    poll_interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        log::info!("[quiz-usb] USB 设备监控启动，轮询间隔 {:?}", poll_interval);

        let mut known_devices: Vec<String> = Vec::new();

        loop {
            time::sleep(poll_interval).await;

            // ── 检测 USB 设备（骨架实现）──
            // 实际实现中使用 rusb::DeviceList 枚举 HID 设备
            let current_devices = detect_quiz_keypads();

            // 新插入的设备
            for dev in &current_devices {
                if !known_devices.contains(dev) {
                    log::info!("[quiz-usb] 检测到新设备: {}", dev);
                    let _ = session_tx
                        .send(SessionCommand::UsbEvent {
                            device_id: dev.clone(),
                            connected: true,
                        })
                        .await;
                }
            }

            // 拔出的设备
            for dev in &known_devices {
                if !current_devices.contains(dev) {
                    log::info!("[quiz-usb] 设备已断开: {}", dev);
                    let _ = session_tx
                        .send(SessionCommand::UsbEvent {
                            device_id: dev.clone(),
                            connected: false,
                        })
                        .await;
                }
            }

            known_devices = current_devices;
        }
    })
}

/// 检测已连接的答题反馈器（骨架实现）
///
/// 实际实现中：
/// 1. 调用 `rusb::open_devices()` 枚举所有 USB 设备
/// 2. 按 VID/PID 过滤（常见反馈器 VID=0x1234, PID=0x5678）
/// 3. 返回设备序列号列表
fn detect_quiz_keypads() -> Vec<String> {
    // 骨架：返回空列表
    // 实际实现参考：
    // ```
    // let mut devices = Vec::new();
    // for device in rusb::DeviceList::new()?.iter() {
    //     let desc = device.device_descriptor()?;
    //     if desc.vendor_id() == 0x1234 && desc.product_id() == 0x5678 {
    //         if let Some(serial) = device.serial_number_string() {
    //             devices.push(serial);
    //         }
    //     }
    // }
    // ```
    Vec::new()
}

#[allow(dead_code)]
/// 读取 HID 输入报告（每次按键触发）
///
/// 实际实现需要 rusb 依赖：
/// ```toml
/// rusb = "0.9"
/// ```
/// 然后:
/// ```ignore
/// let mut buf = [0u8; 64];
/// let timeout = std::time::Duration::from_millis(100);
/// device_handle.read_interrupt(endpoint, &mut buf, timeout)?;
/// Ok(buf.to_vec())
/// ```
fn read_hid_report(
    _device_handle: &(),
    _endpoint: u8,
    _timeout: Duration,
) -> Result<Vec<u8>, String> {
    // 骨架实现 — 需要 rusb 依赖时替换
    let _ = (_device_handle, _endpoint, _timeout);
    Ok(vec![0u8; 64])
}
