//! 后台视频解码，为 `drafftink-desktop` 提供叠加层视频播放能力。
//!
//! 直接驱动本地预编译的 `ffmpeg.exe`（位于 `third_party/ffmpeg/bin/`）在独立 OS
//! 线程上把视频流逐帧解码为 RGBA，并通过 `mpsc` 通道把最新帧送往 UI 线程。UI 永不
//! 阻塞在解码上——它只是 `try_recv` 取最新一帧。
//!
//! **为什么不用 `ez-ffmpeg` / `ffmpeg-next` 这类 Rust 绑定？**
//! 本仓库内置的 FFmpeg 是 2026-07 的 git master 开发版（avcodec-63 / avutil-61），
//! 其 `AVCodec` 结构体字段与 side-data 枚举均已超出 `ffmpeg-next 8.1.0` 的认知范围，
//! 编译期即报 `no field supported_samplerates` 等 ABI 不匹配错误，根本无法链接。
//! 直接调用 `ffmpeg.exe` 则由 FFmpeg 自身保证 ABI 一致，最为稳妥，同时满足
//! 「后台线程解码 + 优雅退出 + 零 panic」的全部硬性约束。
//!
//! 解码端默认开启「智能降维」：源分辨率较长边超过 1920px 时按比例缩小后再输出
//! RGBA，从源头压低单帧内存（4K≈33MB → 1080p≈8MB）并减轻 GPU 纹理上传/拉伸压力；
//! 帧通道仅缓冲 1 帧，杜绝解码堆积导致的内存爆炸。
//!
//! 硬件加速按优先级 `qsv → d3d11va → dxva2 → 软件` 探测（仅尝试本机 `ffmpeg
//! -hwaccels` 实际列出的后端）；任一能成功产出首帧即采用，全部失败则回退纯软件解码。
//!
//! ## 音频
//!
//! 音轨由 [`AudioPipeline`] 承载：**独立的第二个 `ffmpeg` 子进程**把音频重采样为
//! 声卡原生 PCM 写入自己的 stdout，搬运线程推入无锁环形缓冲区（`ringbuf`），
//! `cpal` 回调直接从中取走送进声卡。播放进度以「声卡实际消耗字节数」为准
//! （[`AudioClock`]），UI 每 500ms 上报视频进度做漂移修正。
//!
//! 为何不把音频复用进视频那条 stdout？一个 ffmpeg 输出只能是一种格式，
//! rawvideo 与 PCM 无法共存于同一管道；若改用容器（nut/matroska）复用则需在
//! Rust 侧自行解复用，复杂度与风险远高于多开一个进程（音频解码 CPU/内存开销极小）。
//! 详见 `docs/audio_integration.md`。
//!
//! 音频设备不可用（无声卡 / 虚拟机 / 独占冲突）时一律**优雅降级**为静默播放：
//! 只记录 `warn!`，视频照常播放，绝不 panic、绝不影响 UI。

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// 预加载缓冲区容量（帧）。
///
/// 解码线程在 UI 取走前最多可向前预解码这么多帧，作为「双缓冲 / 预加载」队列，
/// 吸收 GC 抖动与帧调度毛刺，使播放更顺滑。解码端默认开启「智能降维」（较长边 ≤
/// 1920 → 单帧 ≤ ~8MB），故 `PRELOAD_FRAMES × 8MB ≈ 48MB`，远在 500MB 内存防线之内。
/// 通道是有界 `sync_channel`，缓冲区满时解码线程自然阻塞（背压），**不会**因预解码
/// 而堆积数十帧导致内存爆炸——这正是把容量锁死为常量的原因。
const PRELOAD_FRAMES: usize = 6;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};

/// 一帧已解码的 RGBA 视频帧。
///
/// `rgba` 用 `Arc<[u8]>` 而非 `Vec<u8>`：解码线程把读满的缓冲直接 `Arc::from` 复用其
/// 堆分配（零拷贝）后交出去，UI 线程渲染时再 `Arc::clone` 持有，全程**不额外拷贝 8MB**。
/// 这是播放流畅度的关键——原始的 `Vec<u8>` + `ColorImage::from_rgba_unmultiplied` 会在
/// UI 线程每帧重新分配并拷贝一整帧，直接拖垮帧率。
pub struct Frame {
    pub rgba: std::sync::Arc<[u8]>,
    pub width: u32,
    pub height: u32,
}

/// 视频源/目标尺寸集合，用于降维计算与 ffmpeg 参数构造（避免函数参数过多）。
struct Dims {
    src_w: u32,
    src_h: u32,
    out_w: u32,
    out_h: u32,
}

/// 源视频的基本参数（尺寸 + 帧率），由 `ffprobe` 一次性解析。
struct VideoInfo {
    width: u32,
    height: u32,
    /// 平均帧率；用于把「已交付帧数」换算成视频播放进度（音视频同步的基准时钟）。
    fps: f32,
}

// ── 音频管线 ──────────────────────────────────────────────────────────────

/// 环形缓冲区容量：1 MiB。
///
/// 该容量是**解码提前量的上限**，不是播放延迟——声卡从 t=0 起就以恒定实时速率
/// 抽取数据，缓冲区里积压的只是「已解码但还没轮到播放」的音频。
/// 48kHz/立体声/f32 时 1 MiB ≈ 2.7s，s16 时 ≈ 5.5s；相对 8MB 级的单张视频帧，
/// 这点内存可以忽略，却能吃掉 GC 抖动/线程调度延迟造成的欠载（underrun）。
const AUDIO_RING_BYTES: usize = 1 << 20;

/// 搬运线程单次读取的块大小（栈上固定缓冲，稳态零分配）。
const AUDIO_CHUNK_BYTES: usize = 8 * 1024;

/// 漂移容忍阈值：小于此值不修正。人耳对音画不同步的察觉阈约 ±100ms，
/// 留出余量可避免在阈值附近来回抖动（hunting）。
const SYNC_THRESHOLD_MS: i64 = 150;

/// 单次修正上限，防止异常时间戳导致一次性跳过大段音频。
const SYNC_MAX_CORRECT_MS: i64 = 1000;

/// 声卡实际协商到的 PCM 采样格式。
///
/// 只支持 `I16` / `F32` 两种——它们覆盖了 WASAPI / CoreAudio / ALSA 上几乎全部
/// 默认输出配置，且「全 0 字节 == 静音」，使静音/欠载填充可以直接 `fill(0)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFmt {
    I16,
    F32,
}

impl SampleFmt {
    /// 单个样本占用字节数。
    fn bytes(self) -> u32 {
        match self {
            SampleFmt::I16 => 2,
            SampleFmt::F32 => 4,
        }
    }

    /// 对应的 ffmpeg 裸 PCM 封装名（小端）。
    fn ffmpeg_fmt(self) -> &'static str {
        match self {
            SampleFmt::I16 => "s16le",
            SampleFmt::F32 => "f32le",
        }
    }
}

/// 音频输出格式：由**声卡**决定，再让 ffmpeg 重采样对齐。
///
/// 方向很关键：不是「按文件的采样率去配置声卡」（设备常常不支持任意采样率），
/// 而是「读出声卡的默认配置，命令 ffmpeg 用 `-ar/-ac` 重采样到该配置」。
/// 这样 PCM 字节流与 cpal 回调期望的格式天然一致，无需在 Rust 侧做任何重采样。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample: SampleFmt,
}

impl AudioFormat {
    /// 每秒字节数（用于字节数 ↔ 播放时长换算）。
    fn bytes_per_sec(&self) -> u32 {
        self.sample_rate * self.channels as u32 * self.sample.bytes()
    }

    /// 一个「音频帧」（含全部声道的一组样本）的字节数。
    ///
    /// 所有丢弃/填充操作都必须对齐到该边界，否则会造成左右声道错位。
    fn frame_bytes(&self) -> u32 {
        self.channels as u32 * self.sample.bytes()
    }
}

/// 以「声卡已消耗字节数」为唯一真相的播放时钟 + 漂移修正指令。
///
/// 之所以不用 wall-clock：声卡的实际时钟与系统时钟存在 ppm 级偏差，
/// 而字节计数直接反映「真正播出去了多少音频」，是音视频同步的黄金基准。
struct AudioClock {
    /// 已送入声卡的字节数（仅在 cpal 回调中累加）。
    played: AtomicU64,
    /// 待丢弃字节数：音频落后于视频时快进追赶。
    skip: AtomicU64,
    /// 待填充静音字节数：音频超前于视频时原地等待。
    pad: AtomicU64,
    bytes_per_sec: u32,
    frame_bytes: u32,
}

impl AudioClock {
    fn new(fmt: &AudioFormat) -> Self {
        Self {
            played: AtomicU64::new(0),
            skip: AtomicU64::new(0),
            pad: AtomicU64::new(0),
            bytes_per_sec: fmt.bytes_per_sec().max(1),
            frame_bytes: fmt.frame_bytes().max(1),
        }
    }

    /// 当前音频播放位置。
    fn position(&self) -> Duration {
        let played = self.played.load(Ordering::Relaxed);
        Duration::from_micros(played.saturating_mul(1_000_000) / self.bytes_per_sec as u64)
    }

    /// 复位播放字节数为指定值。
    ///
    /// seek 后用于让音频时钟从目标位置开始计数，使 `position()` 直接反映跳转后的
    /// 位置，避免 seek 后音画回归 0（随后由字节级漂移修正收敛到视频进度）。
    pub(crate) fn reset_to_byte(&self, bytes: u64) {
        self.played.store(bytes, Ordering::Relaxed);
    }
}

/// 音频回调与后台线程共享的全部控制状态——全程 lock-free。
///
/// 打包成一个结构体而非多个 `Arc<AtomicBool>`，既减少参数数量，也保证
/// 回调里只需一次指针解引用（实时音频回调中不允许加锁 / 分配 / 阻塞）。
struct AudioShared {
    stop: AtomicBool,
    paused: AtomicBool,
    muted: AtomicBool,
    clock: AudioClock,
}

/// 音频播放管线：ffmpeg 子进程 → 无锁环形缓冲区 → cpal 声卡回调。
///
/// 生命周期与 [`VideoPlayer`] 绑定；`Drop` 时显式停流、杀进程、join 线程，
/// 确保不残留僵尸进程或占用声卡句柄。
pub struct AudioPipeline {
    /// 实际协商到的输出格式。
    pub format: AudioFormat,
    /// cpal 输出流。持有即播放，`Drop` 即归还设备。
    stream: Option<cpal::Stream>,
    /// 专用于音频解码的 ffmpeg 子进程。
    child: Option<Child>,
    /// stdout → 环形缓冲区搬运线程。
    reader: Option<JoinHandle<()>>,
    /// stderr 排空 + 日志线程（**必须存在**，否则管道写满会让子进程永久阻塞）。
    logger: Option<JoinHandle<()>>,
    /// 漂移修正线程。
    syncer: Option<JoinHandle<()>>,
    /// UI → 漂移修正线程的时钟上报端；`Drop` 时置空以唤醒并结束该线程。
    sync_tx: Option<mpsc::Sender<Duration>>,
    shared: Arc<AudioShared>,
}

impl AudioPipeline {
    /// 搭建音频管线：**跳过音轨探测**（ffprobe 子进程 + `.output()` 同步等待，
    /// Windows 上常需 50~300ms），直接复用调用方缓存的探测结果。
    ///
    /// 供 `VideoPlayer::new` / `VideoPlayer::seek` / 纯音频元素（`AudioInstance`）使用：
    /// 源文件不变、音轨不变，每次 seek 都重新 ffprobe 只会在 UI 线程白白阻塞——
    /// 那是拖动进度条卡顿的主因之一。
    pub(crate) fn try_new_with_src(
        ffmpeg: &Path,
        path: &str,
        is_loop: bool,
        ss_sec: Option<f64>,
        src: &SrcAudio,
    ) -> Option<Self> {
        Self::build(ffmpeg, path, is_loop, ss_sec, src)
    }

    /// 用已知的源音轨信息搭建整条音频管线（设备协商 → 环形缓冲 → 子进程 → 声卡流）。
    ///
    /// 返回 `None` 表示「本次播放无音频」，这是**完全正常**的降级路径，调用方
    /// 应当继续正常播放视频。触发 `None` 的情形：
    /// 1. 系统无可用输出设备（无声卡 / 虚拟机 / 被独占）；
    /// 2. 声卡默认格式非 I16/F32，或建流/启流失败。
    fn build(
        ffmpeg: &Path,
        path: &str,
        is_loop: bool,
        ss_sec: Option<f64>,
        src: &SrcAudio,
    ) -> Option<Self> {
        log::info!(
            "[audio] source stream: {} Hz, {} ch (codec={})",
            src.sample_rate,
            src.channels,
            src.codec
        );

        // 2) 读取声卡默认输出配置，作为整条管线的格式基准。
        let device = cpal::default_host().default_output_device()?;
        let dev_name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
        let default_cfg = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[audio] 无法获取默认输出配置（{dev_name}）: {e} — 静默播放");
                return None;
            }
        };
        let sample = match default_cfg.sample_format() {
            cpal::SampleFormat::I16 => SampleFmt::I16,
            // f32 是 WASAPI/CoreAudio 的常见默认；其余格式（I32/U16/...）统一
            // 尝试以 f32 建流——绝大多数设备都接受，失败则走降级分支。
            _ => SampleFmt::F32,
        };
        let format = AudioFormat {
            sample_rate: default_cfg.sample_rate().0,
            channels: default_cfg.channels(),
            sample,
        };
        log::info!(
            "[audio] device \"{}\" -> {} Hz, {} ch, {:?} (ffmpeg 重采样对齐)",
            dev_name,
            format.sample_rate,
            format.channels,
            format.sample
        );

        // 3) 环形缓冲区：容量按音频帧边界向下取整，保证任何时候都不会半帧错位。
        let cap = AUDIO_RING_BYTES - (AUDIO_RING_BYTES % format.frame_bytes() as usize);
        let (producer, consumer) = HeapRb::<u8>::new(cap).split();

        let shared = Arc::new(AudioShared {
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            clock: AudioClock::new(&format),
        });

        // 4) 启动专用音频进程。stderr 必须 piped + 有人读，否则会死锁。
        let args = ffmpeg_audio_args(path, &format, is_loop, ss_sec);
        let mut child = match Command::new(ffmpeg)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[audio] 启动音频 ffmpeg 失败: {e} — 静默播放");
                return None;
            }
        };
        let stdout = child.stdout.take()?;
        let stderr = child.stderr.take();

        // 5) 搬运线程：ffmpeg stdout → 环形缓冲区（背压见 `audio_reader_loop`）。
        let reader = std::thread::Builder::new()
            .name("audio-reader".to_string())
            .spawn({
                let shared = shared.clone();
                move || audio_reader_loop(stdout, producer, shared)
            })
            .ok()?;

        // 6) stderr 排空线程：既暴露真实错误，又防止管道写满导致子进程卡死。
        let logger = stderr.and_then(|err| {
            std::thread::Builder::new()
                .name("audio-stderr".to_string())
                .spawn(move || drain_stderr(err))
                .ok()
        });

        // 7) 建流并启动。任一步失败都走降级：杀掉子进程、返回 None。
        let stream = match build_output_stream(&device, &default_cfg, consumer, shared.clone()) {
            Some(s) => s,
            None => {
                shared.stop.store(true, Ordering::SeqCst);
                let _ = child.kill();
                let _ = child.wait();
                if let Some(h) = reader.join().err() {
                    log::debug!("[audio] reader 线程退出异常: {h:?}");
                }
                return None;
            }
        };
        if let Err(e) = stream.play() {
            log::warn!("[audio] 启动播放失败: {e} — 静默播放");
            shared.stop.store(true, Ordering::SeqCst);
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return None;
        }

        // 8) 漂移修正线程：接收 UI 上报的视频进度，与音频时钟比对后下发修正量。
        let (sync_tx, sync_rx) = mpsc::channel::<Duration>();
        let syncer = std::thread::Builder::new()
            .name("audio-sync".to_string())
            .spawn({
                let shared = shared.clone();
                move || sync_loop(sync_rx, shared)
            })
            .ok();

        log::info!(
            "[audio] pipeline ready (ring={} KiB ≈ {:.1}s, chunk={} B)",
            cap / 1024,
            cap as f32 / format.bytes_per_sec() as f32,
            AUDIO_CHUNK_BYTES
        );

        Some(Self {
            format,
            stream: Some(stream),
            child: Some(child),
            reader: Some(reader),
            logger,
            syncer,
            sync_tx: Some(sync_tx),
            shared,
        })
    }

    /// 暂停 / 恢复音频。
    ///
    /// 双保险：既停掉 cpal 流（设备层面不再拉取数据），又置位共享标志
    /// （回调若仍被调用则输出静音且**不推进时钟**）。后者保证了即使某些后端
    /// 不支持 `pause()` 也一定静音，同时让恢复后音视频仍然对齐。
    pub(crate) fn set_paused(&self, paused: bool) {
        self.shared.paused.store(paused, Ordering::SeqCst);
        if let Some(s) = &self.stream {
            // pause()/play() 返回不同的错误类型，分别处理；二者失败都不致命——
            // 共享的 `paused` 标志已保证回调输出静音。
            let err: Option<String> = if paused {
                s.pause().err().map(|e| e.to_string())
            } else {
                s.play().err().map(|e| e.to_string())
            };
            if let Some(e) = err {
                log::debug!("[audio] stream pause/play 不受支持: {e}");
            }
        }
    }

    /// 静音切换：只改一个原子布尔，回调下一次触发即生效——无需重启进程或重建流。
    ///
    /// 注意：静音**仍然消耗**环形缓冲区数据并推进时钟，这样取消静音时音频位置
    /// 与视频依然对齐（符合「静音只是不出声，不是暂停」的直觉）。
    pub(crate) fn set_muted(&self, muted: bool) {
        self.shared.muted.store(muted, Ordering::SeqCst);
    }

    /// 当前音频播放位置（由声卡消耗量推算）。
    pub(crate) fn position(&self) -> Duration {
        self.shared.clock.position()
    }

    /// 当前音频播放位置（毫秒），供纯音频元素的控制条显示进度。
    pub(crate) fn position_ms(&self) -> u64 {
        self.position().as_millis() as u64
    }

    /// seek 后把音频时钟直接对齐到目标播放位置（毫秒）。
    ///
    /// 新进程以 `-ss` 输入级定位重启后，字节时钟从 0 开始；调用方用本方法让它「看起来」
    /// 已从 `target_ms` 处播放，使音视频时钟在 seek 后即刻对齐（随后由漂移修正兜底收敛）。
    pub(crate) fn reset_to_time(&self, target_ms: u64) {
        let bytes = target_ms * self.format.bytes_per_sec() as u64 / 1000;
        self.shared.clock.reset_to_byte(bytes);
    }

    /// 向漂移修正线程上报 UI 观测到的视频进度。
    fn report_video_time(&self, t: Duration) {
        if let Some(tx) = &self.sync_tx {
            // 通道断开（修正线程已退出）不是错误，忽略即可。
            let _ = tx.send(t);
        }
    }
}

impl Drop for AudioPipeline {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);

        // 1) 显式停流并释放设备：先 pause 让回调停止，再 drop 归还句柄。
        //    不依赖字段析构顺序，避免「回调仍在跑而 consumer 已被回收」。
        if let Some(s) = self.stream.take() {
            let _ = s.pause();
            drop(s);
        }

        // 2) 杀子进程 → 关闭管道 → 搬运/日志线程的 read 立即返回，得以退出。
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }

        // 3) 丢弃发送端唤醒阻塞在 recv 的修正线程。
        self.sync_tx.take();

        for h in [self.reader.take(), self.logger.take(), self.syncer.take()]
            .into_iter()
            .flatten()
        {
            let _ = h.join();
        }
    }
}

/// 建立 cpal 输出流。按协商到的采样格式分派到 `i16` / `f32` 两条回调。
///
/// 返回 `None` 表示建流失败（走静默降级），不会 panic。
fn build_output_stream(
    device: &cpal::Device,
    default_cfg: &cpal::SupportedStreamConfig,
    consumer: HeapConsumer<u8>,
    shared: Arc<AudioShared>,
) -> Option<cpal::Stream> {
    let cfg: cpal::StreamConfig = default_cfg.config();
    let err_shared = shared.clone();
    let on_err = move |e: cpal::StreamError| {
        // 设备被拔出/独占抢占等：记录并停掉管线，绝不 panic。
        log::warn!("[audio] 输出流错误: {e}");
        err_shared.stop.store(true, Ordering::SeqCst);
    };

    let built = match default_cfg.sample_format() {
        cpal::SampleFormat::I16 => {
            let mut cons = consumer;
            device.build_output_stream(
                &cfg,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    let n = std::mem::size_of_val(data);
                    // Safety: i16 无内部不变量（任意位模式合法），`data` 是独占可变
                    // 借用，长度按 size_of_val 精确换算 → 字节视图与原切片同生命周期。
                    // 直接让 pop_slice 填进声卡缓冲，实现真正的零拷贝、零分配。
                    let bytes =
                        unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr().cast::<u8>(), n) };
                    fill_pcm(bytes, &mut cons, &shared);
                },
                on_err,
                None,
            )
        }
        _ => {
            let mut cons = consumer;
            device.build_output_stream(
                &cfg,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let n = std::mem::size_of_val(data);
                    // Safety: 同上。f32 的全 0 位模式即 0.0（静音），故欠载填 0 安全。
                    let bytes =
                        unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr().cast::<u8>(), n) };
                    fill_pcm(bytes, &mut cons, &shared);
                },
                on_err,
                None,
            )
        }
    };

    match built {
        Ok(s) => Some(s),
        Err(e) => {
            log::warn!("[audio] 建立输出流失败: {e} — 静默播放");
            None
        }
    }
}

/// cpal 回调核心：把环形缓冲区中的 PCM 直接填入声卡缓冲的字节视图。
///
/// **实时约束**：本函数运行在音频线程的硬实时上下文中，因此全程
/// 无锁、无分配、无系统调用、无阻塞——只有原子读写与 `memcpy`。
fn fill_pcm(out: &mut [u8], cons: &mut HeapConsumer<u8>, shared: &AudioShared) {
    // 停止或暂停：输出静音且**不推进时钟**，使恢复后音视频仍然对齐。
    if shared.stop.load(Ordering::Relaxed) || shared.paused.load(Ordering::Relaxed) {
        out.fill(0);
        return;
    }

    let clock = &shared.clock;

    // 漂移修正 B（音频超前）：整块回调静音「原地等待」，让视频追上来。
    // 不推进 played，因此音频进度停滞而视频继续前进。
    let pad = clock.pad.load(Ordering::Relaxed);
    if pad > 0 {
        out.fill(0);
        let dec = (out.len() as u64).min(pad);
        clock.pad.fetch_sub(dec, Ordering::Relaxed);
        return;
    }

    // 漂移修正 A（音频落后）：丢弃 skip 字节以快进追赶。
    // 复用 `out` 当作丢弃用的暂存区（随后会被真实数据覆盖），避免额外缓冲。
    let mut skip = clock.skip.swap(0, Ordering::Relaxed);
    while skip > 0 {
        let chunk = (skip as usize).min(out.len());
        let dropped = cons.pop_slice(&mut out[..chunk]);
        if dropped == 0 {
            // 缓冲区已空，剩余修正量留给后续回调继续消化。
            clock.skip.fetch_add(skip, Ordering::Relaxed);
            break;
        }
        clock.played.fetch_add(dropped as u64, Ordering::Relaxed);
        skip -= dropped as u64;
    }

    // 正常路径：一次 memcpy 直达声卡缓冲。
    let n = cons.pop_slice(out);
    if n < out.len() {
        // 欠载（underrun）：补静音而不是重复上一段，避免可听见的「咔哒」。
        out[n..].fill(0);
    }
    clock.played.fetch_add(n as u64, Ordering::Relaxed);

    // 静音：数据已消耗、时钟已推进，仅把输出清零 → 取消静音后位置依旧对齐。
    if shared.muted.load(Ordering::Relaxed) {
        out.fill(0);
    }
}

/// 搬运线程：ffmpeg stdout → 环形缓冲区。
///
/// 三重背压保证内存恒定：
/// 1. 环形缓冲区满 → `push_slice` 返回 0 → 本线程短睡重试；
/// 2. 本线程不读 → OS 管道缓冲写满 → ffmpeg 自身阻塞在 write；
/// 3. 暂停时直接不读 stdout，同样由 (2) 让 ffmpeg 停在原地。
fn audio_reader_loop(
    mut stdout: std::process::ChildStdout,
    mut prod: HeapProducer<u8>,
    shared: Arc<AudioShared>,
) {
    let mut buf = [0u8; AUDIO_CHUNK_BYTES];
    loop {
        if shared.stop.load(Ordering::Relaxed) {
            return;
        }
        // 暂停：不搬运。ffmpeg 会因管道写满自然停住，恢复后从原位继续。
        if shared.paused.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        }
        let n = match stdout.read(&mut buf) {
            Ok(0) => return, // 流结束
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        };
        let mut off = 0;
        while off < n {
            if shared.stop.load(Ordering::Relaxed) {
                return;
            }
            let pushed = prod.push_slice(&buf[off..n]);
            off += pushed;
            if pushed == 0 {
                // 缓冲区满 → 等声卡消耗（背压）。5ms 远小于 1MiB 的缓冲时长，
                // 不会造成欠载，也不会忙等烧 CPU。
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

/// 排空并记录子进程 stderr。
///
/// **这个线程不是可选的**：一旦把 stderr 设为 `piped` 却没人读，ffmpeg 写满
/// OS 管道缓冲（通常 4–64KB）后就会永久阻塞在 write 上 —— 表现为「音频莫名
/// 停止且进程不退出」的死锁。顺带把真实错误暴露到日志里，便于诊断。
fn drain_stderr(stderr: std::process::ChildStderr) {
    let reader = std::io::BufReader::new(stderr);
    for line in std::io::BufRead::lines(reader) {
        let Ok(line) = line else { return };
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        log::warn!("[audio][ffmpeg] {t}");
    }
}

/// 排空视频 ffmpeg 的 stderr 并解析播放进度（`out_time_ms=...`）。
///
/// 同音频的 `drain_stderr`，必须有人持续读 stderr 否则子进程会在管道写满后阻塞；
/// 这里额外把解析出的毫秒进度经通道送回 UI 线程，驱动进度条。通道断开（接收端已
/// 丢弃，如 seek 重建时）即结束，不 panic。
fn drain_progress(stderr: std::process::ChildStderr, tx: Sender<u64>) {
    let reader = BufReader::new(stderr);
    for line in reader.lines() {
        let Ok(line) = line else { return };
        if let Some(ms) = parse_out_time_ms(&line) {
            // 错过的进度不重要（UI 只取最新值），通道断开则退出。
            if tx.send(ms).is_err() {
                return;
            }
        }
    }
}

/// 从 ffmpeg 进度行中提取 `out_time_ms` 的值（毫秒）。
///
/// 走手写解析而非正则，避免引入 `regex` 依赖（约束要求零新依赖）。定位字面量
/// `out_time_ms=` 后截取连续数字即可。注意 `out_time_us` 不含该字面量，不会误匹配。
pub(crate) fn parse_out_time_ms(line: &str) -> Option<u64> {
    let marker = "out_time_ms=";
    let idx = line.find(marker)?;
    let rest = &line[idx + marker.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// 用 ffprobe 探测视频总时长（毫秒）；任意失败（ffprobe 缺失 / 文件异常 / 输出非数字）
/// 一律回退 0——进度条据此隐藏，但视频照常播放（零 panic 兜底）。
pub(crate) fn probe_duration(ffmpeg: &Path, path: &str) -> u64 {
    let out = Command::new(ffprobe_exe(ffmpeg))
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            path,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => {
            log::warn!("[video] ffprobe 时长探测失败: {e} — 进度条将不可用");
            return 0;
        }
    };
    if !out.status.success() {
        return 0;
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .map(|sec| (sec * 1000.0) as u64)
        .unwrap_or(0)
}

/// 漂移修正线程：把 UI 上报的视频进度与音频时钟比对，下发丢弃/填充量。
///
/// 放在独立线程而非 UI 线程内联计算，是为了让 UI 侧 `report_video_time`
/// 退化成一次无阻塞的 channel send —— 无论修正逻辑将来变得多复杂，
/// 都不会有任何一微秒消耗在 UI 帧循环上。
fn sync_loop(rx: mpsc::Receiver<Duration>, shared: Arc<AudioShared>) {
    // 发送端被 Drop 时 recv 返回 Err，线程自然结束。
    while let Ok(video_time) = rx.recv() {
        if shared.stop.load(Ordering::Relaxed) {
            return;
        }
        let clock = &shared.clock;
        let audio_ms = clock.position().as_millis() as i64;
        let video_ms = video_time.as_millis() as i64;
        // drift > 0：音频落后于视频，需要快进；< 0：音频超前，需要等待。
        let drift = video_ms - audio_ms;
        if drift.abs() <= SYNC_THRESHOLD_MS {
            continue;
        }
        let corr = drift.clamp(-SYNC_MAX_CORRECT_MS, SYNC_MAX_CORRECT_MS);
        let raw = corr.unsigned_abs() * clock.bytes_per_sec as u64 / 1000;
        // 必须对齐到音频帧边界，否则左右声道会整体错位半个样本。
        let bytes = raw - (raw % clock.frame_bytes as u64);
        if bytes == 0 {
            continue;
        }
        if corr > 0 {
            clock.skip.fetch_add(bytes, Ordering::Relaxed);
        } else {
            clock.pad.fetch_add(bytes, Ordering::Relaxed);
        }
        log::debug!(
            "[audio] drift {drift:+}ms (video {video_ms}ms / audio {audio_ms}ms) -> {} {} bytes",
            if corr > 0 { "skip" } else { "pad" },
            bytes
        );
    }
}

/// 构造音频解码进程参数：丢弃视频，按声卡格式重采样为裸 PCM 输出到 stdout。
///
/// 刻意**不加** `-re`：音频节奏由声卡回调（真实硬件时钟）决定，
/// 环形缓冲区与管道背压已经把 ffmpeg 限制在「刚好领先一点」的状态；
/// 加了 `-re` 反而会让缓冲区长期贴近空，稍有调度抖动就欠载爆音。
/// `ss_sec` 为 `Some` 时以输入级 `-ss` 定位（seek 重启进程用）。
fn ffmpeg_audio_args(
    path: &str,
    fmt: &AudioFormat,
    is_loop: bool,
    ss_sec: Option<f64>,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into()];
    if is_loop {
        args.push("-stream_loop".into());
        args.push("-1".into());
    }
    // 输入级定位：`-ss` 必须放在 `-i` 之前，与视频解码一致。
    if let Some(ss) = ss_sec {
        args.push("-ss".into());
        args.push(format!("{ss:.3}"));
    }
    args.push("-i".into());
    args.push(path.into());
    // 只要音频：丢弃视频/字幕/数据流。
    args.push("-vn".into());
    args.push("-sn".into());
    args.push("-dn".into());
    // 重采样到声卡原生参数——Rust 侧因此完全不需要做格式转换。
    args.push("-ar".into());
    args.push(fmt.sample_rate.to_string());
    args.push("-ac".into());
    args.push(fmt.channels.to_string());
    args.push("-f".into());
    args.push(fmt.sample.ffmpeg_fmt().into());
    args.push("pipe:1".into());
    args
}

/// 源文件音轨信息（仅用于日志/诊断；实际输出格式由声卡决定）。
pub(crate) struct SrcAudio {
    sample_rate: u32,
    channels: u16,
    codec: String,
}

/// 探测首条音轨；无音轨或探测失败返回 `None`（视为「无音频」而非错误）。
pub(crate) fn probe_audio(ffmpeg: &Path, path: &str) -> Option<SrcAudio> {
    let out = Command::new(ffprobe_exe(ffmpeg))
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=sample_rate,channels,codec_name",
            "-of",
            "json",
            path,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    // 没有音轨时 streams 为空数组 → 取不到 sample_rate，正常返回 None。
    let sample_rate = parse_u32(&s, "\"sample_rate\"")?;
    let channels = parse_u32(&s, "\"channels\"").unwrap_or(2) as u16;
    let codec = parse_json_str(&s, "\"codec_name\"").unwrap_or_else(|| "?".to_string());
    Some(SrcAudio {
        sample_rate,
        channels,
        codec,
    })
}

/// 硬件加速后端选择（优先级 + 回退）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwAccel {
    Qsv,
    D3D11Va,
    Dxva2,
    Software,
}

impl HwAccel {
    /// 探测可用后端时使用的优先级顺序。
    pub const ORDER: [HwAccel; 4] = [
        HwAccel::Qsv,
        HwAccel::D3D11Va,
        HwAccel::Dxva2,
        HwAccel::Software,
    ];

    /// 对应 FFmpeg 硬件设备名（`None` 表示软件解码）。
    pub fn ffmpeg_name(self) -> Option<&'static str> {
        match self {
            HwAccel::Qsv => Some("qsv"),
            HwAccel::D3D11Va => Some("d3d11va"),
            HwAccel::Dxva2 => Some("dxva2"),
            HwAccel::Software => None,
        }
    }

    /// 用于日志的简短标签。
    pub fn label(self) -> &'static str {
        match self {
            HwAccel::Qsv => "qsv",
            HwAccel::D3D11Va => "d3d11va",
            HwAccel::Dxva2 => "dxva2",
            HwAccel::Software => "soft",
        }
    }
}

/// 单个视频的后台解码器与帧通道。
///
/// 该结构体被丢弃时会杀掉子进程并 `join` 解码线程，在 App（或元素）销毁时实现
/// 干净、无 panic 的退出。
pub struct VideoPlayer {
    /// 实际采用的解码后端（用于日志/诊断）。
    pub hwaccel: HwAccel,
    /// 解码端自适应降维比例：源分辨率较长边压到 `max_dim`(默认 1920)以内的系数，
    /// 取值 (0,1]，等于 1.0 表示源已 ≤ 上限、未缩放。该比例在解码端直接生效
    /// （ffmpeg `-vf scale`），因此已解码帧本身即为缩小后的尺寸；渲染端无需再乘。
    pub base_scale: f32,
    /// 暂停状态（UI 侧镜像，仅用于渲染图标）；真正的解码线程读取 `paused_flag`。
    pub paused: bool,
    /// 静音状态（UI 侧镜像，仅用于渲染图标）；真正的回调读取音频管线内的原子标志。
    pub is_muted: bool,
    /// 源平均帧率，用于把「已交付帧数」换算为视频播放进度（同步基准时钟）。
    pub fps: f32,
    /// 已交付给 UI 的帧数——视频时钟的计数器。
    frames_shown: u64,
    /// 音频管线；`None` 表示无音轨或音频设备不可用（优雅降级为静默播放）。
    audio: Option<AudioPipeline>,
    /// 上次向漂移修正线程上报视频进度的时刻（`sync_tick` 内部自节流用）。
    last_sync: Instant,
    /// 与解码线程共享的暂停标志：置位时解码线程睡眠而非发送帧（见 `decode_loop`）。
    paused_flag: Arc<AtomicBool>,
    /// UI 取帧通道；`Option` 仅为在 `Drop` 中丢弃接收端以解除 reader 阻塞。
    receiver: Option<Receiver<Frame>>,
    /// 通知解码线程停止。
    stop: Arc<AtomicBool>,
    /// 解码子进程；`Drop` 时杀掉它。
    child: Option<Child>,
    /// 后台解码线程句柄；`Drop` 时 `join`。
    thread: Option<JoinHandle<()>>,
    /// 视频源路径（seek 重启进程时需要重建命令行）。
    input_path: String,
    /// 是否循环播放（seek 重启时保持一致）。
    is_loop: bool,
    /// 解码目标尺寸（降维后的输出尺寸），seek 重启时复用，避免重新探测。
    dims: Dims,
    /// 单帧 RGBA 字节数（= out_w*out_h*4），seek 重启时复用。
    frame_bytes: usize,
    /// 当前播放进度（毫秒）通道接收端：后台 stderr 进度线程解析 `out_time_ms` 后送入，
    /// UI 线程每帧 `poll_progress_ms()` 取最新值（不阻塞）。
    progress_rx: Option<Receiver<u64>>,
    /// 视频总时长（毫秒），由 ffprobe 在启动时探测；失败时回退 0（进度条不可见但不 panic）。
    duration_ms: u64,
    /// 是否正处于 seek 重启（冻结）中：置位期间画面停在上一帧，直到新位置首帧到达
    /// （`try_recv` 首次成功）才清零——实现「先冻结，后瞬切」的体验。
    /// 同时用于合并连续拖动产生的多次 seek：重启进行中再有新请求只记录最新目标。
    seeking: bool,
    /// seek 重启进行中若又收到新的跳转目标，暂存此处；首帧到达后由 `try_recv` 顺带
    /// 发起一次重启，避免过度杀/起 ffmpeg 进程（那正是「拖动卡死」的根因）。
    pending_seek: Option<u64>,
    /// 进度时间轴基准（毫秒）：输入级 `-ss` 重启后 ffmpeg 输出时间戳从 0 重新计起，
    /// `poll_progress_ms` 需加上本基准才是绝对播放位置。初始 0；seek 时置为目标位置。
    progress_base_ms: u64,
    /// 源音轨探测缓存（`new` 时 ffprobe 一次）：`None` 表示无音轨或探测失败。
    /// `seek` 重启音频管线时直接复用——源文件不变、音轨不变，每次 seek 都重新
    /// 在 UI 线程同步跑 ffprobe（Windows 上 50~300ms 的子进程等待）正是拖动
    /// 进度条卡顿的主因之一。
    audio_src: Option<SrcAudio>,
}

impl VideoPlayer {
    /// 便捷入口：使用默认上限 1920（教学场景几乎不需要 4K）创建解码器。
    pub fn new(video_path: &Path, is_loop: bool) -> Result<Self> {
        Self::new_with_max_dim(video_path, is_loop, 1920.0)
    }

    /// 打开 `video_path` 并在后台启动解码。
    ///
    /// 仅当连软件解码都无法配置时（如文件不可解码、找不到 ffmpeg）才返回 `Err`；
    /// 任何加速器探测失败都会静默回退到软件解码。
    ///
    /// `max_dim` 为解码端输出的最长边上限（像素）；超过则按比例缩小（不放大），
    /// 从源头压低单帧内存与 GPU 纹理压力。`base_scale` 反映最终采用的缩放比。
    pub fn new_with_max_dim(video_path: &Path, is_loop: bool, max_dim: f32) -> Result<Self> {
        let ffmpeg = ffmpeg_exe().ok_or_else(|| {
            anyhow!("找不到本地 ffmpeg.exe（请确认 third_party/ffmpeg/bin 已部署，或由 build.rs 拷贝到 exe 旁）")
        })?;

        let path = video_path
            .to_str()
            .ok_or_else(|| anyhow!("非 UTF-8 视频路径: {video_path:?}"))?;

        // 探测总时长（毫秒），供进度条使用；失败（ffprobe 缺失/异常）回退 0，零 panic。
        let duration_ms = probe_duration(&ffmpeg, path);

        // 先解析源尺寸与帧率（决定逐帧读取长度、降维目标与视频时钟）；失败视为不可解码。
        let info = probe(&ffmpeg, path)?;
        let (src_w, src_h) = (info.width, info.height);

        // 【核心优化：智能降维打击】
        // 教学场景几乎不需要 4K。把较长边压到 `max_dim` 以内（保持比例、绝不放大于原图），
        // 解码端直接用 `ffmpeg -vf scale` 输出缩小后的 RGBA——单帧内存从 4K 的
        // ~33MB 降到 1080p 的 ~8MB，并显著减轻 GPU 纹理上传/全尺寸拉伸的压力。
        let max_dim = max_dim.max(1.0);
        let base_scale = (max_dim / src_w as f32)
            .min(max_dim / src_h as f32)
            .min(1.0);
        let (out_w, out_h) = if base_scale < 1.0 {
            let mut ow = (src_w as f32 * base_scale).round() as i64;
            let mut oh = (src_h as f32 * base_scale).round() as i64;
            if ow % 2 != 0 {
                ow -= 1;
            }
            if oh % 2 != 0 {
                oh -= 1;
            }
            (ow.max(2) as u32, oh.max(2) as u32)
        } else {
            (src_w, src_h)
        };
        let mut frame_bytes = (out_w as usize) * (out_h as usize) * 4;
        let mut dims = Dims {
            src_w,
            src_h,
            out_w,
            out_h,
        };

        log::info!(
            "[video] source {}x{} -> decode {}x{} (base_scale={:.3}, max_dim={})",
            src_w,
            src_h,
            out_w,
            out_h,
            base_scale,
            max_dim as u32
        );

        // 候选后端：仅取本机支持的硬件加速，软件解码始终作为最终回退。
        let supported = supported_hwaccels(&ffmpeg);
        let mut candidates: Vec<HwAccel> = HwAccel::ORDER
            .iter()
            .filter(|hw| {
                hw.ffmpeg_name()
                    .map(|n| supported.iter().any(|s| s == n))
                    .unwrap_or(true)
            })
            .copied()
            .collect();
        if !candidates.contains(&HwAccel::Software) {
            candidates.push(HwAccel::Software);
        }

        // 按优先级探测，找到第一个能成功产出（降维后）首帧的后端。
        let mut chosen = pick_backend(&ffmpeg, path, &candidates, &dims, frame_bytes);
        if chosen.is_none() {
            // 降维管线在本机所有后端均失败（极少见，如某硬件加速不支持 CPU scale 滤镜），
            // 降级为原分辨率输出以保证可用性。
            log::warn!(
                "[video] downscaled pipeline failed on all backends for {path}; falling back to full-res decode"
            );
            dims.out_w = src_w;
            dims.out_h = src_h;
            frame_bytes = (src_w as usize) * (src_h as usize) * 4;
            chosen = pick_backend(&ffmpeg, path, &candidates, &dims, frame_bytes);
        }
        let chosen = chosen
            .ok_or_else(|| anyhow!("无法为 {path} 初始化任何视频后端（已尝试 {candidates:?}）"))?;

        // 用选中的后端启动真正的流式解码（含降维滤镜）。
        let args = ffmpeg_decode_args(path, chosen, &dims, is_loop, None, None, true);
        let mut child = Command::new(&ffmpeg)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("启动 ffmpeg 子进程失败: {e}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("ffmpeg 子进程未提供 stdout"))?;

        // 进度线程：持续读取 ffmpeg stderr 的 `out_time_ms=...` 并送回 UI 线程。
        // 必须有人读 stderr，否则管道写满会让子进程永久阻塞（与音频管线同理）。
        let (progress_tx, progress_rx) = mpsc::channel::<u64>();
        if let Some(stderr) = child.stderr.take() {
            std::thread::Builder::new()
                .name("video-progress".to_string())
                .spawn(move || drain_progress(stderr, progress_tx))
                .ok();
        }

        // 预加载双缓冲：有界通道容量为 PRELOAD_FRAMES，解码线程在 UI 取走前最多向前
        // 预解码数帧（吸收 GC 抖动、平滑播放），缓冲区满即背压阻塞——既保留「预加载」
        // 收益，又从源头杜绝「解码堆积数十帧 → 内存爆炸」。单帧 ≤8MB 故上限 ≈48MB。
        let (tx, rx) = mpsc::sync_channel::<Frame>(PRELOAD_FRAMES);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        // 暂停与停止**必须**是两个独立的原子量。（历史 bug：此处曾写作
        // `stop.clone()`，使暂停等价于永久停止解码——首次按下暂停后再也无法恢复。）
        let paused_flag = Arc::new(AtomicBool::new(false));
        let paused_flag_thread = paused_flag.clone();
        let reader = std::thread::Builder::new()
            .name("video-decoder".to_string())
            .spawn(move || {
                decode_loop(
                    stdout,
                    tx,
                    stop_thread,
                    paused_flag_thread,
                    out_w,
                    out_h,
                    frame_bytes,
                )
            })
            .map_err(|e| anyhow!("启动解码线程失败: {e}"))?;

        // 音频：与视频完全解耦地尝试建立。任何失败都只是「没有声音」，
        // 绝不影响视频播放，也绝不向上抛错（优雅降级硬性要求）。
        // 音轨信息只探测一次并缓存（audio_src）：seek 重启音频管线时复用，
        // 杜绝每次跳转都在 UI 线程同步等待 ffprobe 子进程。
        let audio_src = probe_audio(&ffmpeg, path);
        let audio = audio_src.as_ref().and_then(|src| {
            AudioPipeline::try_new_with_src(&ffmpeg, path, is_loop, None, src)
        });
        if audio.is_none() {
            log::warn!("[audio] 无可用音频管线（无音轨或设备不可用）— 静默播放 {path}");
        }

        Ok(Self {
            hwaccel: chosen,
            base_scale,
            paused: false,
            is_muted: false,
            fps: info.fps,
            frames_shown: 0,
            audio,
            last_sync: Instant::now(),
            paused_flag,
            receiver: Some(rx),
            stop,
            child: Some(child),
            thread: Some(reader),
            input_path: path.to_string(),
            is_loop,
            dims,
            frame_bytes,
            progress_rx: Some(progress_rx),
            duration_ms,
            seeking: false,
            pending_seek: None,
            progress_base_ms: 0,
            audio_src,
        })
    }

    /// 设置暂停状态（线程安全）：同步 UI 镜像、向解码线程广播标志、联动暂停音频。
    ///
    /// 音视频**同时**暂停/恢复，且各自都停在自己的当前位置（视频侧靠解码线程
    /// 睡眠 + 管道背压，音频侧靠停流 + 环形缓冲区保水位），因此恢复后依然对齐，
    /// 无需重建任何子进程。
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.paused_flag.store(paused, Ordering::SeqCst);
        if let Some(a) = &self.audio {
            a.set_paused(paused);
        }
        if paused {
            log::info!("[video] paused");
        } else {
            log::info!("[video] resumed");
        }
    }

    /// 在暂停/播放之间切换。
    pub fn toggle_paused(&mut self) {
        self.set_paused(!self.paused);
    }

    /// 设置静音：仅翻转一个原子布尔，声卡回调下次触发即生效。
    ///
    /// 不重启进程、不重建流；音频数据仍在正常消耗，故取消静音后位置不会跳变。
    pub fn set_muted(&mut self, muted: bool) {
        self.is_muted = muted;
        if let Some(a) = &self.audio {
            a.set_muted(muted);
        }
        log::info!("[audio] {}", if muted { "muted" } else { "unmuted" });
    }

    /// 在静音/取声之间切换。
    pub fn toggle_muted(&mut self) {
        self.set_muted(!self.is_muted);
    }

    /// 该播放器是否真的有音频输出（无音轨或设备不可用时为 `false`）。
    pub fn has_audio(&self) -> bool {
        self.audio.is_some()
    }

    /// 实际协商到的音频输出格式（供诊断面板展示）；无音频时为 `None`。
    pub fn audio_format(&self) -> Option<AudioFormat> {
        self.audio.as_ref().map(|a| a.format)
    }

    /// 当前视频播放进度 = 已交付帧数 / 帧率。
    ///
    /// 以「已真正显示的帧」为准而非墙上时钟，因此暂停期间不会继续前进，
    /// 天然就是音视频同步所需的视频基准时钟。
    pub fn video_time(&self) -> Duration {
        let fps = if self.fps > 0.1 { self.fps } else { 30.0 };
        Duration::from_secs_f64(self.frames_shown as f64 / fps as f64)
    }

    /// 当前音频播放位置（由声卡消耗字节数推算）；无音频时为 `None`。
    pub fn audio_time(&self) -> Option<Duration> {
        self.audio.as_ref().map(|a| a.position())
    }

    /// 音视频漂移（毫秒，正数=音频落后于视频）；无音频时为 `None`。仅供诊断展示。
    pub fn drift_ms(&self) -> Option<i64> {
        let a = self.audio.as_ref()?.position().as_millis() as i64;
        Some(self.video_time().as_millis() as i64 - a)
    }

    /// 同步心跳：把当前视频进度上报给漂移修正线程。
    ///
    /// UI 可以每帧无脑调用——内部自节流到 500ms 一次，且只做一次
    /// 非阻塞 channel send，不会给帧循环带来任何可测量的开销。
    pub fn sync_tick(&mut self) {
        if self.audio.is_none() || self.paused {
            return;
        }
        if self.last_sync.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.last_sync = Instant::now();
        let t = self.video_time();
        if let Some(a) = &self.audio {
            a.report_video_time(t);
        }
    }

    /// 非阻塞取最新一帧；无新帧时返回 `Err`。
    ///
    /// 取帧成功即推进视频时钟（`frames_shown`），故本方法需要 `&mut self`。
    /// `receiver` 在 `seek` 重启期间会短暂为 `None`（旧接收端已丢弃），此时返回
    /// `Disconnected` 而非 panic——符合零 panic 兜底约定。
    ///
    /// **冻结→瞬切**：若当前正处于 seek 重启（`seeking == true`），则本次取到的帧
    /// 即为「新位置」的第一帧——立即解除冻结，并把拖动期间堆积的最新跳转目标顺带
    /// 发起一次重启（合并连续 seek），避免每像素都杀/起一个 ffmpeg 进程。
    pub fn try_recv(&mut self) -> std::result::Result<Frame, TryRecvError> {
        let r = match self.receiver.as_ref() {
            Some(r) => r.try_recv(),
            None => return Err(TryRecvError::Disconnected),
        };
        if r.is_ok() {
            self.frames_shown += 1;
            if self.seeking {
                self.seeking = false;
                // 拖动/快速连点期间若又来了新目标，紧接首帧再发起一次重启，使其生效。
                if let Some(target) = self.pending_seek.take() {
                    self.seek(target);
                }
            }
        }
        r
    }

    /// 开始拖动进度条：立即丢弃预加载缓冲区里属于「旧位置」的已解码帧。
    ///
    /// 注意：本方法**只清缓冲、不置 `seeking`**。`seeking` 是 `seek()`/`try_recv()`
    /// 内部管理的「重启进行中」状态，用于在连续拖动时合并（coalesce）多次 seek。
    /// 若此处抢先置 `seeking = true`，紧随其后的 `seek()` 会因「已在 seeking」而
    /// 只记录 `pending_seek` 却不真正重启——首帧永远到不了，拖动卡死。
    /// 因此「冻结」由 `last_tex` 保留旧帧 + 新流首帧到达前不刷新纹理自然实现。
    pub fn begin_scrub(&mut self) {
        self.clear_preload();
    }

    /// 丢弃预加载缓冲区中尚未显示的帧（属于旧播放位置，seek 前应清空）。
    pub fn clear_preload(&mut self) {
        if let Some(rx) = self.receiver.as_ref() {
            while rx.try_recv().is_ok() {}
        }
    }

    /// 非阻塞取最新进度（毫秒）；通道无数据返回 `None`。
    ///
    /// 后台 stderr 进度线程解析出 `out_time_ms` 后送入通道，UI 线程每帧调用本方法
    /// 刷新进度条，绝不会阻塞帧循环。返回值已折算为**绝对时间轴**：输入级 `-ss`
    /// 重启后 ffmpeg 从 0 计时，故叠加 `progress_base_ms`（上次 seek 的目标位置）；
    /// 循环播放时对总时长取模，使进度条循环往复而非顶满不动。
    pub fn poll_progress_ms(&mut self) -> Option<u64> {
        self.progress_rx
            .as_mut()
            .and_then(|r| r.try_recv().ok())
            .map(|ms| {
                let abs = ms.saturating_add(self.progress_base_ms);
                if self.is_loop && self.duration_ms > 0 {
                    abs % self.duration_ms
                } else {
                    abs
                }
            })
    }

    /// 视频总时长（毫秒）；ffprobe 失败时为 0。
    pub fn duration_ms(&self) -> u64 {
        self.duration_ms
    }

    /// 跳转到指定播放位置（毫秒）。
    ///
    /// **非阻塞（关键性能修复）**：旧实现会在 UI 线程上 `join` 旧解码线程 + `wait` 旧
    /// ffmpeg 进程，而杀进程在 Windows 上常需 100~500ms，于是「拖动进度条」时主线程被
    /// 卡死。现改为：把旧进程/线程/通道交给后台「收割线程」去 kill+join，**本方法立即
    /// 返回**；UI 全程不阻塞。配合「预加载双缓冲」+「冻结→瞬切」实现顺滑 seek。
    ///
    /// 实现为「kill 现有视频 + 音频进程 → 以 `-ss <target>` 输入级重启双进程」——
    /// shell-out 架构下最快的定位方式：直接跳到最近关键帧，< 500ms 偏差由现有字节级
    /// 漂移修正自动收敛。重启后按 `self.paused` 重新应用暂停状态，故 seek 前的暂停/
    /// 播放状态在 seek 后保持不变。零 panic：任意步骤失败仅记日志并返回，不崩溃。
    ///
    /// **合并连续 seek**：拖动/快速连点期间若上一次重启仍在进行（`seeking == true`），
    /// 仅记录 `pending_seek` 最新目标，待首帧到达（`try_recv`）后再发起一次重启，避免
    /// 每像素都杀/起一个 ffmpeg 进程（那才是「拖动卡死」的真正根因）。
    pub fn seek(&mut self, target_ms: u64) {
        // 合并：重启进行中只暂存最新目标，首帧到达后再顺带重启。
        if self.seeking {
            self.pending_seek = Some(target_ms);
            return;
        }
        self.seeking = true;
        self.pending_seek = None;
        // 输入级 -ss 重启后 ffmpeg 从 0 计时：记录基准，poll_progress_ms 叠加回绝对位置。
        self.progress_base_ms = target_ms;

        let ffmpeg = match ffmpeg_exe() {
            Some(p) => p,
            None => {
                log::warn!("[video] seek 失败：找不到本地 ffmpeg.exe");
                self.seeking = false;
                return;
            }
        };
        let out_w = self.dims.out_w;
        let out_h = self.dims.out_h;
        let frame_bytes = self.frame_bytes;

        // 1) 旧进程/线程/通道全部摘下，交给后台「收割线程」kill+join——绝不阻塞 UI 线程。
        //    旧解码线程持有的 stop 标志独立成 `old_stop`，避免与新生进程的标志互相误停。
        let old_stop = self.stop.clone();
        let old_child = self.child.take();
        let old_thread = self.thread.take();
        let old_rx = self.receiver.take();
        let old_progress = self.progress_rx.take();
        // 旧音频管线在 UI 线程丢弃（Drop 杀音频子进程 + join 线程，耗时极短，非卡死主因）。
        drop(self.audio.take());
        std::thread::Builder::new()
            .name("video-reaper".to_string())
            .spawn(move || {
                old_stop.store(true, Ordering::SeqCst);
                drop(old_rx); // 解除旧解码线程在 send 上的阻塞，使其因通道断开而退出
                if let Some(mut c) = old_child {
                    let _ = c.kill();
                    let _ = c.wait(); // 在后台线程等待，不卡 UI
                }
                if let Some(h) = old_thread {
                    let _ = h.join();
                }
                drop(old_progress);
            })
            .ok();

        // 2) 新进程使用独立的 stop 标志（与旧线程解耦）。
        let new_stop = Arc::new(AtomicBool::new(false));
        self.stop = new_stop.clone();

        // 3) 以 -ss 输入级定位重启视频解码进程（参数复用既有 dims / hwaccel）。
        let args = ffmpeg_decode_args(
            &self.input_path,
            self.hwaccel,
            &self.dims,
            self.is_loop,
            None,
            Some(target_ms as f64 / 1000.0),
            true,
        );
        let mut child = match Command::new(&ffmpeg)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[video] seek 重启 ffmpeg 失败: {e}");
                self.seeking = false;
                return;
            }
        };
        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                log::warn!("[video] seek 重启后未提供 stdout");
                self.seeking = false;
                return;
            }
        };
        // 预加载双缓冲：容量为 PRELOAD_FRAMES 的有界通道，复用既有零拷贝（Arc<[u8]>）。
        let (tx, rx) = mpsc::sync_channel::<Frame>(PRELOAD_FRAMES);
        let stop_thread = self.stop.clone();
        let paused_flag_thread = self.paused_flag.clone();
        let reader = match std::thread::Builder::new()
            .name("video-decoder".to_string())
            .spawn(move || {
                decode_loop(
                    stdout,
                    tx,
                    stop_thread,
                    paused_flag_thread,
                    out_w,
                    out_h,
                    frame_bytes,
                )
            }) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("[video] seek 重启解码线程失败: {e}");
                self.seeking = false;
                return;
            }
        };

        // 进度线程：读取新进程的 stderr 进度并送回 UI。
        let (progress_tx, progress_rx) = mpsc::channel::<u64>();
        if let Some(stderr) = child.stderr.take() {
            std::thread::Builder::new()
                .name("video-progress".to_string())
                .spawn(move || drain_progress(stderr, progress_tx))
                .ok();
        }

        // 4) 以 -ss 重启音频管线（无音轨/无设备时优雅降级为 None）。
        //    复用缓存的音轨信息（new 时已探测一次）——此处绝不能再跑 ffprobe：
        //    那会在 UI 线程同步等待一个子进程（Windows 上 50~300ms），
        //    正是拖动进度条/跳转时 UI 卡顿的主因之一。
        let audio = self.audio_src.as_ref().and_then(|src| {
            AudioPipeline::try_new_with_src(&ffmpeg, &self.input_path, self.is_loop, None, src)
        });
        if let Some(a) = &audio {
            // 让音频时钟从目标位置开始，避免 seek 后音画回归 0。
            a.reset_to_time(target_ms);
        }

        // 5) 落库新状态。
        self.child = Some(child);
        self.thread = Some(reader);
        self.receiver = Some(rx);
        self.audio = audio;
        self.progress_rx = Some(progress_rx);

        // 6) 视频帧时钟对齐：frames_shown 重置到目标位置对应的帧数，
        //    使帧计数式 video_time() 在 seek 后仍准确（供音视频漂移修正）。
        let fps = if self.fps > 0.1 { self.fps } else { 30.0 };
        self.frames_shown = (target_ms as f64 / 1000.0 * fps as f64) as u64;

        // 7) 重新应用暂停状态（seek 重启后默认恢复播放；若原本暂停则再次暂停）。
        self.set_paused(self.paused);

        log::info!(
            "[video] seek -> {:.3}s (duration={}ms)",
            target_ms as f64 / 1000.0,
            self.duration_ms
        );
    }
}

/// 后台解码循环：从 ffmpeg 的 stdout 中逐帧读取恰好 `frame_bytes` 字节的 RGBA 帧，
/// 通过通道发往 UI。循环在收到停止信号、接收端断开或流结束时退出——绝不 panic。
///
/// 暂停处理：当 `paused` 置位时，循环睡眠（而非忙等）且不读取/发送任何帧，因此
/// 解码线程在暂停期间几乎不占 CPU，UI 侧保留并显示最后一帧。
fn decode_loop(
    mut stdout: std::process::ChildStdout,
    tx: mpsc::SyncSender<Frame>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    w: u32,
    h: u32,
    frame_bytes: usize,
) {
    let mut buf = vec![0u8; frame_bytes];
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        // 暂停：睡眠让出 CPU（非忙等），保留通道内最后一帧供 UI 持续显示。
        if paused.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(33));
            continue;
        }
        // 精确读取一帧；EOF 或读错误则结束。
        let mut filled = 0;
        while filled < frame_bytes {
            match stdout.read(&mut buf[filled..]) {
                Ok(0) => return, // 流结束
                Ok(n) => filled += n,
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return,
            }
        }
        // 把已读满的缓冲直接转为 Arc（复用底层堆分配，零拷贝），随后为下一帧重新分配。
        let rgba = std::sync::Arc::from(std::mem::take(&mut buf));
        let frame = Frame {
            rgba,
            width: w,
            height: h,
        };
        // 接收端（UI）已丢弃 → 退出。通道容量=1，UI 未取走时此处阻塞形成背压。
        if tx.send(frame).is_err() {
            return;
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }
        buf = vec![0u8; frame_bytes];
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // 先显式销毁音频管线：停流 → 归还声卡句柄 → 杀音频子进程 → join 线程。
        // 显式 take 而不依赖字段析构顺序，确保「设备与麦克风/扬声器权限」被
        // 第一时间释放，不留僵尸进程。
        self.audio.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
        // 丢弃接收端以解除 reader 在 `send` 上可能的阻塞，使其因通道断开而退出。
        self.receiver.take();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// 按优先级挑选第一个能成功产出（降维后）首帧的后端。
fn pick_backend(
    ffmpeg: &Path,
    path: &str,
    candidates: &[HwAccel],
    dims: &Dims,
    frame_bytes: usize,
) -> Option<HwAccel> {
    candidates
        .iter()
        .copied()
        .find(|hw| backend_works(ffmpeg, path, *hw, dims, frame_bytes))
}

/// 探测指定后端能否成功解码出（降维后的）首帧（输出恰好 `frame_bytes` 字节即视为可用）。
fn backend_works(ffmpeg: &Path, path: &str, hw: HwAccel, dims: &Dims, frame_bytes: usize) -> bool {
    let args = ffmpeg_decode_args(path, hw, dims, false, Some(1), None, false);
    match Command::new(ffmpeg)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        Ok(out) => out.status.success() && out.stdout.len() == frame_bytes,
        Err(_) => false,
    }
}

/// 构造 ffmpeg 解码参数。当 `dims.out_*` 小于源尺寸（降维）时插入 `scale` 滤镜，
/// 保持画面比例并显式输出 RGBA；`limit_frames` 用于首帧探测；`ss_sec` 为 `Some`
/// 时以输入级 `-ss` 定位（seek 重启进程用）；`realtime` 控制是否加 `-re`（实时限速）。
///
/// **为何探测时不加 `-re`**：`-re` 限制 ffmpeg 以输入帧率读取，仅对「实时播放」有意义。
/// 首帧探测（`backend_works`）只需尽快解出一帧来判定后端是否可用，加 `-re` 会让
/// 硬件加速后端初始化/首帧出现被人为限速而**阻塞数秒**——而这发生在 `VideoPlayer::new`
/// 内部、直接卡住 UI 线程（表现为「加载视频时非常卡」）。故探测传 `realtime=false`，
/// 真正播放/seek 重启进程才传 `realtime=true`。
fn ffmpeg_decode_args(
    path: &str,
    hw: HwAccel,
    dims: &Dims,
    is_loop: bool,
    limit_frames: Option<usize>,
    ss_sec: Option<f64>,
    realtime: bool,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
    ];
    // 仅实时播放路径需要 `-re`；探测首帧不要限速。
    if realtime {
        args.push("-re".into());
        // 进度上报：`-progress pipe:2` 让 ffmpeg 周期性把 `out_time_ms=` 等键值对
        // 写到 stderr，由后台 drain_progress 线程解析回 UI（进度条数据源）。
        // 注意：仅靠 `-loglevel error` 下的 stats 行永远等不到 `out_time_ms`——
        // 该字面量只会出现在 `-progress` 输出里（这正是「进度条不动」的根因）。
        // `-nostats` 关掉人类可读的 stats 行，避免与键值对混流。
        args.push("-nostats".into());
        args.push("-progress".into());
        args.push("pipe:2".into());
    }
    if let Some(name) = hw.ffmpeg_name() {
        args.push("-hwaccel".into());
        args.push(name.into());
    }
    if is_loop {
        args.push("-stream_loop".into());
        args.push("-1".into());
    }
    // 输入级定位：`-ss` 必须放在 `-i` 之前，才能跳过解码从头快速跳转最近关键帧。
    if let Some(ss) = ss_sec {
        args.push("-ss".into());
        args.push(format!("{ss:.3}"));
    }
    args.push("-i".into());
    args.push(path.into());
    args.push("-an".into()); // 只要视频帧
    // 降维滤镜：仅当目标尺寸小于源尺寸时启用，保持比例、输出 RGBA。
    if dims.out_w != dims.src_w || dims.out_h != dims.src_h {
        args.push("-vf".into());
        args.push(format!("scale={}:{},format=rgba", dims.out_w, dims.out_h));
    }
    if let Some(n) = limit_frames {
        args.push("-frames:v".into());
        args.push(n.to_string());
    }
    args.push("-f".into());
    args.push("rawvideo".into());
    args.push("-pix_fmt".into());
    args.push("rgba".into());
    args.push("pipe:1".into());
    args
}

/// 与 `ffmpeg.exe` 同目录的 `ffprobe.exe`。
fn ffprobe_exe(ffmpeg: &Path) -> PathBuf {
    ffmpeg
        .parent()
        .map(|d| d.join("ffprobe.exe"))
        .unwrap_or_else(|| PathBuf::from("ffprobe.exe"))
}

/// 用 ffprobe 解析视频尺寸与帧率；失败返回 `Err`（视为不可解码）。
fn probe(ffmpeg: &Path, path: &str) -> Result<VideoInfo> {
    let out = Command::new(ffprobe_exe(ffmpeg))
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,avg_frame_rate,r_frame_rate",
            "-of",
            "json",
            path,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| anyhow!("ffprobe 执行失败: {e}"))?;

    if !out.status.success() {
        return Err(anyhow!("ffprobe 无法解析 {path}（文件可能不可解码）"));
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let width =
        parse_u32(&s, "\"width\"").ok_or_else(|| anyhow!("ffprobe 未在 {path} 中找到视频宽度"))?;
    let height =
        parse_u32(&s, "\"height\"").ok_or_else(|| anyhow!("ffprobe 未在 {path} 中找到视频高度"))?;
    // 帧率是 `"30000/1001"` 这类分数字符串；取不到时退化为 30fps（仅影响同步基准）。
    let fps = parse_fps(&s, "\"avg_frame_rate\"")
        .or_else(|| parse_fps(&s, "\"r_frame_rate\""))
        .unwrap_or(30.0);
    Ok(VideoInfo {
        width,
        height,
        fps,
    })
}

/// 从 ffprobe 的 JSON 文本中提取 `"key": <int>` 或 `"key": "<int>"` 形式的整数。
///
/// ffprobe 对不同字段的类型并不一致——`width`/`channels` 是裸整数，而
/// `sample_rate` 是**带引号的字符串**，故需跳过可选的引号。
fn parse_u32(s: &str, key: &str) -> Option<u32> {
    let after = json_value_after(s, key)?;
    let after = after.trim_start_matches('"');
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// 提取 `"key": "<string>"` 形式的字符串值。
fn parse_json_str(s: &str, key: &str) -> Option<String> {
    let after = json_value_after(s, key)?.trim_start();
    let rest = after.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// 解析 `"key": "num/den"` 形式的帧率分数；`den` 为 0 视为无效。
fn parse_fps(s: &str, key: &str) -> Option<f32> {
    let v = parse_json_str(s, key)?;
    let (num, den) = v.split_once('/')?;
    let num: f32 = num.trim().parse().ok()?;
    let den: f32 = den.trim().parse().ok()?;
    if den.abs() < f32::EPSILON || num <= 0.0 {
        return None;
    }
    Some(num / den)
}

/// 定位 `key` 之后紧跟的值文本（跳过冒号与缩进空白）。
fn json_value_after<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let idx = s.find(key)?;
    let rest = &s[idx + key.len()..];
    let colon = rest.find(':')?;
    Some(rest[colon + 1..].trim_start())
}

/// 本机 `ffmpeg -hwaccels` 支持的后端名（缓存，仅探测一次）。
fn supported_hwaccels(ffmpeg: &Path) -> Vec<String> {
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let mut list = Vec::new();
            if let Ok(out) = Command::new(ffmpeg)
                .args(["-hide_banner", "-hwaccels"])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
            {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    let t = line.trim();
                    // 跳过标题行（含空格）与空行，仅保留单 token 的后端名。
                    if t.is_empty() || t.contains(' ') {
                        continue;
                    }
                    list.push(t.to_string());
                }
            }
            list
        })
        .clone()
}

/// 定位运行时 `ffmpeg.exe`：依次尝试（1）环境变量覆盖、（2）与当前 exe 同级、
/// （3）构建期由 build.rs 注入的 `FFMPEG_BIN_DIR`。
pub(crate) fn ffmpeg_exe() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("DRAFFTINK_FFMPEG_BIN") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("ffmpeg.exe");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    if let Some(dir) = option_env!("FFMPEG_BIN_DIR") {
        let p = PathBuf::from(dir).join("ffmpeg.exe");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{Duration, Instant};

    /// 并发测试安全：每个样本文件用唯一序号命名，避免并行测试互相覆盖同一临时文件
    /// （原先共享 `drafftink_vp_test.mp4`，并行运行时一个测试重写文件会使另一个解码失败）。
    static TEST_FILE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    /// 用本地 ffmpeg 生成一段 2 秒、**含 440Hz 正弦音轨**的 MP4，用于验证音频管线。
    fn make_sample_av_mp4() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "drafftink_vp_av_test_{}.mp4",
            TEST_FILE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let ffmpeg = ffmpeg_exe().expect("本地 ffmpeg.exe 应存在");
        let status = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=10:duration=2",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
                "-y",
            ])
            .arg(p.to_str().unwrap())
            .status()
            .expect("应能启动 ffmpeg 生成音视频测试片");
        assert!(status.success(), "ffmpeg 生成音视频测试片失败: {status:?}");
        p
    }

    /// 用本地 ffmpeg 生成一段 1 秒的小 MP4，作为解码端到端验证的输入。
    fn make_sample_mp4() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "drafftink_vp_test_{}.mp4",
            TEST_FILE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let ffmpeg = ffmpeg_exe().expect("本地 ffmpeg.exe 应存在");
        let status = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=10:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(p.to_str().unwrap())
            .status()
            .expect("应能启动 ffmpeg 生成测试片");
        assert!(status.success(), "ffmpeg 生成测试片失败: {status:?}");
        assert!(p.exists(), "测试片未生成");
        p
    }

    /// 端到端：VideoPlayer 应能从 MP4 解出 RGBA 帧，尺寸/字节数正确且非全零。
    #[test]
    fn decode_mp4_yields_valid_rgba_frames() {
        let path = make_sample_mp4();
        let mut player = VideoPlayer::new(&path, false).expect("解码器应成功启动");
        // 优先采用本机可用的硬件加速（如 d3d11va/qsv），否则回退软件解码；二者皆有效。
        assert!(
            matches!(player.hwaccel, HwAccel::Software | HwAccel::D3D11Va | HwAccel::Qsv | HwAccel::Dxva2),
            "应选定一个有效解码后端，实际: {:?}",
            player.hwaccel
        );
        // 320x240 远小于默认上限 1920，不应被缩放。
        assert!(
            (player.base_scale - 1.0).abs() < 1e-3,
            "320x240 < 1920 时 base_scale 应=1.0，实际 {}",
            player.base_scale
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut frame = None;
        while Instant::now() < deadline {
            if let Ok(f) = player.try_recv() {
                frame = Some(f);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let frame = frame.expect("5 秒内应至少收到一帧");

        assert_eq!(frame.width, 320);
        assert_eq!(frame.height, 240);
        assert_eq!(frame.rgba.len(), 320 * 240 * 4);
        assert!(frame.rgba.iter().any(|&b| b != 0), "解码帧不应全零");

        let _ = std::fs::remove_file(&path);
    }

    /// 验证「智能降维」：用 max_dim=160 约束一个 320x240 的视频，解码帧应缩到 160x120，
    /// 且单帧内存仅为原图的 1/4（base_scale≈0.5）。
    #[test]
    fn decode_is_downscaled_to_max_dim() {
        let path = make_sample_mp4(); // 320x240
        let mut player =
            VideoPlayer::new_with_max_dim(&path, false, 160.0).expect("解码器应成功启动");
        assert!(
            (player.base_scale - 0.5).abs() < 1e-3,
            "320x240 在 max_dim=160 下 base_scale 应≈0.5，实际 {}",
            player.base_scale
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut frame = None;
        while Instant::now() < deadline {
            if let Ok(f) = player.try_recv() {
                frame = Some(f);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let frame = frame.expect("5 秒内应至少收到一帧");
        assert_eq!(frame.width, 160, "降维后宽度应为 160");
        assert_eq!(frame.height, 120, "降维后高度应为 120");
        assert_eq!(frame.rgba.len(), 160 * 120 * 4);
        assert!(frame.rgba.iter().any(|&b| b != 0), "解码帧不应全零");

        let _ = std::fs::remove_file(&path);
    }

    /// 音轨探测：有音轨的文件应解析出采样率/声道/编码，无音轨的应返回 `None`。
    #[test]
    fn probe_audio_detects_track_presence() {
        let ffmpeg = ffmpeg_exe().expect("本地 ffmpeg.exe 应存在");

        let av = make_sample_av_mp4();
        let src = probe_audio(&ffmpeg, av.to_str().unwrap()).expect("含音轨文件应探测到音频");
        // sine 滤镜默认 44100Hz 单声道；关键是「能解析出合法值」而非具体数字。
        assert!(
            src.sample_rate >= 8000 && src.sample_rate <= 192_000,
            "采样率应在合理范围，实际 {}",
            src.sample_rate
        );
        assert!(src.channels >= 1, "声道数应 ≥1，实际 {}", src.channels);
        assert_eq!(src.codec, "aac", "编码应为 aac，实际 {}", src.codec);

        // 纯视频文件（-an）必须探测为「无音频」，这是走静默降级的前提。
        let vonly = make_sample_mp4();
        assert!(
            probe_audio(&ffmpeg, vonly.to_str().unwrap()).is_none(),
            "无音轨文件不应探测到音频"
        );

        let _ = std::fs::remove_file(&av);
        let _ = std::fs::remove_file(&vonly);
    }

    /// ffprobe 的 `sample_rate` 是**带引号的字符串**而 `width` 是裸整数，
    /// `parse_u32` 必须同时兼容两者（此前只支持裸整数，会把采样率解析为 None）。
    #[test]
    fn parse_u32_handles_quoted_and_bare_numbers() {
        let json = r#"{ "streams": [ { "width": 320, "sample_rate": "44100" } ] }"#;
        assert_eq!(parse_u32(json, "\"width\""), Some(320));
        assert_eq!(parse_u32(json, "\"sample_rate\""), Some(44100));
        assert_eq!(parse_u32(json, "\"missing\""), None);
    }

    /// 帧率分数解析：`"30000/1001"` → 29.97；分母为 0 或缺字段应返回 `None`。
    #[test]
    fn parse_fps_handles_rational() {
        let json = r#"{ "avg_frame_rate": "30000/1001", "r_frame_rate": "10/1", "bad": "0/0" }"#;
        let fps = parse_fps(json, "\"avg_frame_rate\"").expect("应解析出帧率");
        assert!((fps - 29.97).abs() < 0.01, "29.97fps，实际 {fps}");
        assert_eq!(parse_fps(json, "\"r_frame_rate\""), Some(10.0));
        assert_eq!(parse_fps(json, "\"bad\""), None, "分母为 0 应无效");
    }

    /// 音频格式换算：每秒字节数与音频帧字节数必须精确，
    /// 它们是时钟推算与漂移修正对齐的基础。
    #[test]
    fn audio_format_byte_math_is_exact() {
        let f = AudioFormat {
            sample_rate: 48000,
            channels: 2,
            sample: SampleFmt::F32,
        };
        assert_eq!(f.frame_bytes(), 8, "2ch × 4B = 8B 每音频帧");
        assert_eq!(f.bytes_per_sec(), 48000 * 8);
        // 1 MiB 环形缓冲区在该格式下约 2.7 秒——与设计文档的容量论证一致。
        let secs = AUDIO_RING_BYTES as f32 / f.bytes_per_sec() as f32;
        assert!((2.5..3.0).contains(&secs), "1MiB 应≈2.7s，实际 {secs}");

        let i16f = AudioFormat {
            sample_rate: 44100,
            channels: 1,
            sample: SampleFmt::I16,
        };
        assert_eq!(i16f.frame_bytes(), 2);
        assert_eq!(i16f.bytes_per_sec(), 88200);
    }

    /// 时钟换算：字节数 → 播放位置必须线性精确（1 秒的数据恰好报 1 秒）。
    #[test]
    fn audio_clock_position_tracks_bytes() {
        let f = AudioFormat {
            sample_rate: 48000,
            channels: 2,
            sample: SampleFmt::I16,
        };
        let clock = AudioClock::new(&f);
        assert_eq!(clock.position(), Duration::ZERO);
        // 正好 1 秒的数据量。
        clock
            .played
            .store(f.bytes_per_sec() as u64, Ordering::Relaxed);
        let p = clock.position();
        assert!(
            (p.as_secs_f64() - 1.0).abs() < 1e-6,
            "1 秒数据应报 1.0s，实际 {p:?}"
        );
    }

    /// 音频 ffmpeg 参数必须：重采样到声卡格式、丢弃视频、输出裸 PCM 到 stdout，
    /// 且**不含 `-re`**（节奏由声卡回调控制，见 `ffmpeg_audio_args` 文档）。
    #[test]
    fn audio_args_target_device_format() {
        let f = AudioFormat {
            sample_rate: 48000,
            channels: 2,
            sample: SampleFmt::F32,
        };
        let args = ffmpeg_audio_args("in.mp4", &f, true, None);
        let joined = args.join(" ");
        assert!(joined.contains("-vn"), "必须丢弃视频流: {joined}");
        assert!(joined.contains("-ar 48000"), "必须重采样到 48000: {joined}");
        assert!(joined.contains("-ac 2"), "必须重采样到 2 声道: {joined}");
        assert!(joined.contains("-f f32le"), "必须输出 f32le: {joined}");
        assert!(joined.contains("pipe:1"), "必须输出到 stdout: {joined}");
        assert!(joined.contains("-stream_loop -1"), "循环播放: {joined}");
        assert!(
            !args.iter().any(|a| a == "-re"),
            "音频不应加 -re（由声卡定速）: {joined}"
        );
    }

    /// 端到端：含音轨的视频应建立起音频管线，且 `AudioFormat` 合法。
    ///
    /// CI / 虚拟机上可能没有任何输出设备，此时 `has_audio()` 为 false 属于**预期
    /// 的优雅降级**——本测试只要求「视频照常出帧、进程不 panic」。
    #[test]
    fn audio_pipeline_degrades_gracefully() {
        let path = make_sample_av_mp4();
        let mut player = VideoPlayer::new(&path, false).expect("解码器应成功启动");

        // 帧率应从 ffprobe 正确解析（测试片为 10fps）。
        assert!(
            (player.fps - 10.0).abs() < 0.1,
            "帧率应为 10fps，实际 {}",
            player.fps
        );

        if let Some(f) = player.audio_format() {
            assert!(f.sample_rate >= 8000, "采样率应合法，实际 {}", f.sample_rate);
            assert!(f.channels >= 1, "声道数应 ≥1，实际 {}", f.channels);
            assert!(f.frame_bytes() >= 2, "音频帧字节数应 ≥2");
            log::info!("[test] 音频管线已建立: {f:?}");
        } else {
            log::info!("[test] 无音频设备，已降级为静默播放（预期路径）");
        }

        // 无论有无音频，视频都必须正常出帧。
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut got = None;
        while Instant::now() < deadline {
            if let Ok(f) = player.try_recv() {
                got = Some(f);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let frame = got.expect("5 秒内应至少收到一帧");
        assert_eq!(frame.width, 320);
        assert_eq!(frame.height, 240);

        // 取到帧后视频时钟必须前进（frames_shown / fps）。
        assert!(
            player.video_time() > Duration::ZERO,
            "交付帧后视频时钟应前进"
        );

        // 静音 / 暂停切换都不得 panic，且状态镜像正确。
        player.toggle_muted();
        assert!(player.is_muted);
        player.toggle_muted();
        assert!(!player.is_muted);
        player.sync_tick(); // 首次调用受 500ms 节流，不应发送也不应 panic

        let _ = std::fs::remove_file(&path);
    }

    /// 回归测试：暂停**绝不能**等价于停止。
    ///
    /// 历史 bug —— `paused_flag` 曾被写成 `stop.clone()`，导致两个标志是同一个
    /// 原子量：第一次暂停就把解码线程永久终止，恢复后再也不出帧。
    #[test]
    fn pause_then_resume_keeps_decoding() {
        let path = make_sample_mp4();
        let mut player = VideoPlayer::new(&path, true).expect("解码器应成功启动");

        // 先确认能出帧。
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut first = false;
        while Instant::now() < deadline {
            if player.try_recv().is_ok() {
                first = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(first, "暂停前应能出帧");

        // 暂停：stop 标志必须保持 false（这正是当年 bug 的判定点）。
        player.set_paused(true);
        assert!(player.paused, "暂停镜像应为 true");
        assert!(
            !player.stop.load(Ordering::SeqCst),
            "暂停绝不能置位 stop —— 否则解码线程会永久退出"
        );

        // 恢复后应重新出帧（循环播放，源必然还有数据）。
        player.set_paused(false);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut resumed = false;
        while Instant::now() < deadline {
            if player.try_recv().is_ok() {
                resumed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(resumed, "恢复播放后必须能继续出帧");

        let _ = std::fs::remove_file(&path);
    }

    /// 进度解析：ffmpeg 的 `out_time_ms=120000` 应直接解析为毫秒 120000。
    #[test]
    fn progress_parse_from_stderr() {
        assert_eq!(
            parse_out_time_ms("frame=10 fps=25 out_time_ms=120000"),
            Some(120000)
        );
        assert_eq!(parse_out_time_ms("out_time_ms=0"), Some(0));
        // `out_time_us`（微秒）不含 `out_time_ms=` 字面量，不得误匹配。
        assert_eq!(parse_out_time_ms("out_time_us=123456789"), None);
        assert_eq!(parse_out_time_ms("no progress here"), None);
    }

    /// 时长探测：ffprobe 输出 `65.5`（秒）应换算为 65500 毫秒。
    #[test]
    fn probe_duration_returns_ms() {
        let ffmpeg = ffmpeg_exe().expect("本地 ffmpeg.exe 应存在");
        let path = make_sample_mp4(); // 1 秒测试片
        let ms = probe_duration(&ffmpeg, path.to_str().unwrap());
        assert!(
            (900..=1100).contains(&ms),
            "1s 测试片时长应≈1000ms，实际 {ms}"
        );
        // 非数字 / 空输出回退 0（零 panic）。
        assert_eq!(probe_duration(&ffmpeg, "non_existent_file.mp4"), 0);
        let _ = std::fs::remove_file(&path);
    }

    /// 输入级定位：`-ss` 必须出现在 `-i` 之前，且仅在 seek 时存在。
    #[test]
    fn decode_args_include_ss_when_seeking() {
        let dims = Dims {
            src_w: 320,
            src_h: 240,
            out_w: 320,
            out_h: 240,
        };
        let none = ffmpeg_decode_args("x.mp4", HwAccel::Software, &dims, false, None, None, true);
        assert!(
            !none.iter().any(|a| a == "-ss"),
            "无 seek 时不应含 -ss: {none:?}",
        );
        let some = ffmpeg_decode_args("x.mp4", HwAccel::Software, &dims, false, None, Some(0.5), true);
        let joined = some.join(" ");
        assert!(joined.contains("-ss 0.500"), "-ss 应出现在参数中: {joined}");
        let i_pos = some.iter().position(|a| a == "-i").unwrap();
        let ss_pos = some.iter().position(|a| a == "-ss").unwrap();
        assert!(ss_pos < i_pos, "-ss 必须在 -i 之前（输入级定位）");
    }

    /// 回归测试：首帧探测参数（`realtime=false`）**不得**包含 `-re`，否则硬件加速后端
    /// 初始化会被实时限速而阻塞 `VideoPlayer::new`（卡住 UI 线程 → 「加载视频时非常卡」）。
    /// 真正播放路径（`realtime=true`）才允许 `-re`。
    #[test]
    fn decode_args_probe_excludes_re_but_playback_includes_it() {
        let dims = Dims {
            src_w: 320,
            src_h: 240,
            out_w: 320,
            out_h: 240,
        };
        let probe = ffmpeg_decode_args("x.mp4", HwAccel::Software, &dims, false, Some(1), None, false);
        assert!(
            !probe.iter().any(|a| a == "-re"),
            "探测参数不应含 -re（否则首帧探测被限速、阻塞 new）: {probe:?}",
        );
        let playback = ffmpeg_decode_args("x.mp4", HwAccel::Software, &dims, false, None, None, true);
        assert!(
            playback.iter().any(|a| a == "-re"),
            "播放参数应含 -re（实时限速，与音视频同步节奏一致）: {playback:?}",
        );
    }

    /// 音频进程同样需在 seek 时以 `-ss` 输入级定位。
    #[test]
    fn audio_args_include_ss_when_seeking() {
        let f = AudioFormat {
            sample_rate: 48000,
            channels: 2,
            sample: SampleFmt::F32,
        };
        let none = ffmpeg_audio_args("x.mp4", &f, false, None);
        assert!(!none.iter().any(|a| a == "-ss"));
        let some = ffmpeg_audio_args("x.mp4", &f, false, Some(1.25));
        let joined = some.join(" ");
        assert!(joined.contains("-ss 1.250"));
        let i_pos = some.iter().position(|a| a == "-i").unwrap();
        let ss_pos = some.iter().position(|a| a == "-ss").unwrap();
        assert!(ss_pos < i_pos);
    }

    /// 端到端：seek 必须 kill + 以 `-ss` 重启双进程，且重启后照常出帧、进度线程重新上报。
    #[test]
    fn seek_restarts_and_decodes() {
        let path = make_sample_mp4(); // 1s, 10fps
        let mut player = VideoPlayer::new(&path, false).expect("解码器应启动");
        assert!(player.duration_ms() > 0, "应探测到时长");
        // 先确认能正常出帧。
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if player.try_recv().is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        // seek 到 300ms：输入级定位，应成功不 panic 且继续出帧。
        player.seek(300);
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut got = false;
        while Instant::now() < deadline {
            if player.try_recv().is_ok() {
                got = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(got, "seek 后必须继续出帧（进程已以 -ss 重启）");
        // 进度通道应重新开始供给（seek 后新进程会再次上报 out_time_ms）。
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut saw_progress = false;
        while Instant::now() < deadline {
            if player.poll_progress_ms().is_some() {
                saw_progress = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(saw_progress, "seek 后进度线程应重新上报 out_time_ms");
        let _ = std::fs::remove_file(&path);
    }

    /// 回归（用户实测症状「播放正常但进度条恒 0:00」）：未 seek 的普通播放期间，
    /// `poll_progress_ms` 必须持续上报非零进度——`out_time_ms` 只来自 `-progress
    /// pipe:2` 输出，此前解码参数从未含该旗标（且 `-loglevel error` 压掉 stats 行），
    /// 导致进度条永远停在 0:00。
    #[test]
    fn progress_advances_during_playback() {
        let path = make_sample_mp4(); // 1s, 10fps
        let mut player = VideoPlayer::new(&path, false).expect("解码器应启动");
        // 循环里持续取帧：保持 stdout 管道流动，ffmpeg 才会继续编码并周期性上报进度
        // （管道写满会背压阻塞 ffmpeg，进度也随之停滞——与真实 UI 每帧取帧等价）。
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut latest = 0u64;
        while Instant::now() < deadline {
            let _ = player.try_recv();
            if let Some(ms) = player.poll_progress_ms() {
                latest = latest.max(ms);
                if latest > 0 {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(latest > 0, "播放期间进度应 > 0ms，实际恒为 {latest}（0:00 症状回归）");
        let _ = std::fs::remove_file(&path);
    }
}

