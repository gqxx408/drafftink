# 音频集成设计（drafftink-desktop）

> 适用范围：`crates/drafftink-desktop/src/video_player.rs` 中的 `AudioPipeline`
> 相关依赖：`cpal 0.15.3`（声卡输出）、`ringbuf 0.3.3`（无锁 SPSC 环形缓冲）
> 前置文档：[`video_integration_report.md`](./video_integration_report.md)（shell-out ffmpeg 视频通路）

---

## 1. 结论摘要

在不改动既有视频通路的前提下，为播放器补齐音频。核心取舍如下：

| 维度 | 决策 | 一句话理由 |
|---|---|---|
| 音频来源 | **独立的第二个 ffmpeg 进程** | rawvideo 与 PCM 无法共用同一条 stdout |
| 格式协商 | **以声卡为准，命令 ffmpeg 重采样对齐** | Rust 侧零格式转换代码 |
| 主时钟 | **视频帧计数（UI 侧）** | 与既有 `try_recv` 帧驱动模型一致 |
| 同步手段 | **字节级 skip / pad 微修正**（非 `av_seek_frame`） | 修正量通常仅几十 ms，重启进程代价过高 |
| 暂停 | **`stream.pause()` + 共享标志 + 管道背压**（不重启进程） | 恢复后音视频天然对齐 |
| 静音 | **单个原子布尔**（仍消耗数据、仍推进时钟） | 静音是「不出声」而非「暂停」 |
| 无音频 / 无声卡 | **返回 `None`，静默播放** | 永不 panic、永不阻断视频 |

---

## 2. 架构总览

```
                    ┌──────────────────────── UI / eframe 线程 ────────────────────────┐
                    │  每帧: try_recv() → frames_shown += 1                            │
                    │        sync_tick() → 每 500ms 非阻塞 send(video_time)            │
                    └───────┬──────────────────────────────────┬──────────────────────┘
                            │                                  │ mpsc::Sender<Duration>
   ┌────────────────────────┴──────────┐              ┌────────┴──────────────────┐
   │  视频进程 ffmpeg #1                │              │  audio-sync 线程           │
   │  -f rawvideo → stdout             │              │  drift = video - audio     │
   │        ↓ decode 线程               │              │  → skip / pad (原子)       │
   │  sync_channel(1) 背压              │              └────────┬──────────────────┘
   └───────────────────────────────────┘                       │ AudioClock
                                                               │
   ┌───────────────────────────────────┐              ┌────────┴──────────────────┐
   │  音频进程 ffmpeg #2                │              │  cpal 回调线程（硬实时）    │
   │  -vn -sn -dn                      │              │  fill_pcm():               │
   │  -ar/-ac/-f <声卡格式> → stdout    │              │   无锁 / 无分配 / 无阻塞   │
   └──────┬────────────────────────────┘              └────────▲──────────────────┘
          │ read 8KiB                                          │ pop_slice (memcpy)
   ┌──────┴──────────────┐    ringbuf::HeapRb<u8> 1MiB   ┌─────┴─────┐
   │  audio-reader 线程   │ ──── Producer ══════════════▶ │ Consumer  │
   └─────────────────────┘                               └───────────┘
          │
   ┌──────┴──────────────┐
   │  audio-stderr 线程   │  ← 非可选！不排空 stderr 必然死锁（见 §9）
   └─────────────────────┘
```

**线程清单（每个带音轨的播放实例）**：UI 线程、视频 decode 线程、`audio-reader`、`audio-stderr`、`audio-sync`、cpal 内部回调线程；外加 2 个 ffmpeg 子进程。

---

## 3. 决策记录：为什么是「第二个 ffmpeg 进程」

原始需求建议复用现有进程的 stdout（或退化为一条额外管道）。实测后否决，理由：

| 方案 | 结论 | 原因 |
|---|---|---|
| A. 单进程、单 stdout 混合 | ❌ 不可行 | `-f rawvideo` 是**独占的输出封装格式**，无法在同一 muxer 里再塞一路 PCM；裸流也没有帧边界可供解复用 |
| B. 单进程 + `pipe:3` 第二输出 | ⚠️ 可行但脆弱 | Windows 下 `std::process` 无法稳定传递 fd 3；需 `CreateProcess` + 句柄继承的平台特化代码 |
| C. 单进程输出 nut/matroska 容器 | ❌ 否决 | 需要在 Rust 侧自己写解复用器，等于重新实现 ffmpeg 已有能力 |
| **D. 第二个独立进程** | ✅ 采用 | 零平台特化、故障隔离（音频挂掉不影响视频）、参数可独立调优 |

方案 D 的代价是「多一个进程」与「两条流各自解码」，但音频解码开销相对视频可忽略（AAC 立体声解码 + 重采样在现代 CPU 上 < 1% 单核）。故障隔离的收益远大于此。

---

## 4. 格式协商：方向是「声卡 → ffmpeg」

关键设计：**不是**把声卡配置成文件的格式，而是读出声卡默认格式，再命令 ffmpeg 输出成那个格式。

```rust
let default_cfg = device.default_output_config()?;      // 例：48000 Hz / 2ch / F32
let sample = match default_cfg.sample_format() {
    cpal::SampleFormat::I16 => SampleFmt::I16,
    _ => SampleFmt::F32,                                 // f32 是 WASAPI/CoreAudio 常见默认
};
```

于是 ffmpeg 参数直接落成声卡参数：

```
-vn -sn -dn  -ar 48000  -ac 2  -f f32le  pipe:1
```

**收益**：Rust 侧完全没有重采样、声道混合、位深转换的代码——这些全部由 ffmpeg 的 `swresample` 承担（它的实现质量远高于手写版本）。回调里只剩一次 `memcpy`。

> `cpal::SampleFormat` 枚举为 `I8/I16/I32/I64/U8/U16/U32/U64/F32/F64`，并没有 `S16LE` 这种命名。
> 映射关系：`I16 → s16le`、其余 → `f32le`。非 I16/F32 的设备统一尝试以 f32 建流，失败则降级静默。

**刻意不加 `-re`**：音频节奏由声卡回调（真实硬件时钟）决定。环形缓冲区 + 管道背压已把 ffmpeg 约束在「刚好领先一点」的状态；加了 `-re` 会让缓冲区长期贴近空，稍有调度抖动就欠载爆音。

---

## 5. RingBuffer 容量设定

```rust
const AUDIO_RING_BYTES: usize = 1 << 20;   // 1 MiB
const AUDIO_CHUNK_BYTES: usize = 8 * 1024; // 单次 read/搬运粒度
```

实例化时按**音频帧边界向下取整**，确保任何时刻都不会出现半帧错位（否则左右声道整体错开半个样本，听感为声场翻转）：

```rust
let cap = AUDIO_RING_BYTES - (AUDIO_RING_BYTES % format.frame_bytes() as usize);
```

### 5.1 容量换算表

| 设备格式 | `frame_bytes` | `bytes_per_sec` | 对齐后容量 | **缓冲时长** |
|---|---:|---:|---:|---:|
| 48 kHz / 2ch / f32 | 8 | 384,000 | 1,048,576 | **2.73 s** |
| 48 kHz / 2ch / i16 | 4 | 192,000 | 1,048,576 | **5.46 s** |
| 44.1 kHz / 2ch / f32 | 8 | 352,800 | 1,048,576 | **2.97 s** |
| 44.1 kHz / 2ch / i16 | 4 | 176,400 | 1,048,576 | **5.94 s** |
| 48 kHz / 6ch / f32 | 24 | 1,152,000 | 1,048,560 | **0.91 s** |

### 5.2 对原始需求的一处澄清

需求原文写「1MB 左右，足够缓冲 100ms 音频」。实测下来 **1 MiB 远不止 100ms**：在 48 kHz/2ch/f32 下是 **2.73 秒**；真正只缓冲 100ms 需要约 **37.5 KiB**。

仍然保留 1 MiB，理由是**大缓冲不等于高延迟**：

- 播放位置由**声卡已消耗的字节数**定义，缓冲区里的数据是「预读」而非「排队等待播放」；
- 缓冲区一旦填满，`push_slice` 返回 0 → 搬运线程短睡 → 管道写满 → ffmpeg 自己阻塞。也就是说**满缓冲的稳态代价只是 ffmpeg 停在原地**，不产生任何附加延迟；
- 换来的是对磁盘卡顿、GC 式调度抖动、ffmpeg 突发慢速的容错余量。100ms 的缓冲在 Windows 上一次线程调度抖动就可能击穿，导致可听见的爆音。

**内存代价**：每个播放实例固定 1 MiB，与视频分辨率无关。10 路同时播放 = 10 MiB，可接受。

### 5.3 三重背压

内存恒定不靠约定，靠三层机械保证：

1. 环形缓冲区满 → `push_slice` 返回 0 → 搬运线程 `sleep(5ms)` 重试；
2. 搬运线程不读 → OS 管道缓冲（4–64 KB）写满 → ffmpeg 阻塞在 `write`；
3. 暂停时干脆不读 stdout → 同样由 (2) 让 ffmpeg 停在原地。

`sleep(5ms)` 的选取：远小于最小缓冲时长（0.91 s），不会造成欠载；又远大于 0，不会忙等烧 CPU。

---

## 6. 时钟与 Seek / 漂移修正策略

### 6.1 为什么不用 `av_seek_frame`

需求原文建议「音频线程通过 `av_seek_frame` 对齐」。这在本架构下不可用：`av_seek_frame` 是 libav\* 的 **C API**，需要进程内持有 `AVFormatContext`。我们是 shell-out 架构，没有任何 in-process 解复用器句柄。

等价能力只有「带 `-ss` 重启进程」，而典型修正量只有几十毫秒——为了 50ms 的偏差重启一个进程（含容器解析、解码器初始化，约 100–300ms）显然本末倒置，且重启期间必然静音。

### 6.2 采用的方案：字节级微修正

```
主时钟（video）= frames_shown / fps            ← UI 线程，帧驱动，与渲染天然同步
从时钟（audio）= (played - skipped) / bytes_per_sec  ← 声卡实际消耗字节数
drift = video_ms - audio_ms
```

`AudioClock` 用三个 `AtomicU64` 表达状态，无锁：

| 字段 | 含义 |
|---|---|
| `played` | 回调已消耗的字节总数（推进播放位置） |
| `skip` | 待丢弃字节数（音频落后 → 快进追赶） |
| `pad` | 待填充静音字节数（音频超前 → 原地等待） |

修正阈值与上限：

```rust
const SYNC_THRESHOLD_MS: i64 = 150;    // 死区：|drift| ≤ 150ms 不动作
const SYNC_MAX_CORRECT_MS: i64 = 1000; // 单次修正上限，避免异常值造成大跳
```

`sync_loop` 每收到一次上报即计算，且**修正量必须对齐到音频帧边界**：

```rust
let corr  = drift.clamp(-SYNC_MAX_CORRECT_MS, SYNC_MAX_CORRECT_MS);
let raw   = corr.unsigned_abs() * clock.bytes_per_sec as u64 / 1000;
let bytes = raw - (raw % clock.frame_bytes as u64);   // ← 不对齐会导致声道错位
if corr > 0 { clock.skip.fetch_add(bytes, ..) } else { clock.pad.fetch_add(bytes, ..) }
```

150ms 死区的依据：人耳对音视频不同步的察觉阈值约为「音频超前 45ms / 滞后 125ms」（ITU-R BT.1359 给出的可察觉界限更宽）。取 150ms 意味着只在**已经接近可察觉**时才动作，避免频繁微调本身引入听感瑕疵。

### 6.3 为什么修正逻辑放在独立线程

`report_video_time` 在 UI 线程里必须退化成一次**无阻塞 channel send**。把计算放进 `audio-sync` 线程后，无论未来修正算法变得多复杂（例如引入 PID 控制、变速重采样），都不会有任何一微秒消耗在 UI 帧循环上。

### 6.4 真正的 Seek（尚未实现，接口预留）

`VideoPlayer` 目前不暴露 `seek()`。若要实现，正确做法是**双进程整体重启**：

1. `self.audio.take()`（触发 `Drop`：停流 → kill → join 线程）；
2. 视频进程以 `-ss <t> -accurate_seek -i <path>` 重启（`-ss` 置于 `-i` **之前**为输入级快速定位；置于之后虽精确但需从头解码，对长视频不可接受）；
3. 音频进程用**完全相同**的 `-ss` 值重启；
4. `frames_shown = 0`，并记录 `base_offset = t`，使 `video_time() = base + frames_shown / fps`；`AudioClock` 全部计数器归零。

残留误差来自两条流各自的关键帧对齐差异，交由 §6.2 的漂移修正在数百毫秒内自动收敛——这正是保留该机制的额外价值。

---

## 7. 状态联动矩阵

| 操作 | ffmpeg 进程 | cpal 流 | 环形缓冲 | `AudioClock` | 效果 |
|---|---|---|---|---|---|
| **暂停** | 保持存活，因管道写满自然停在原地 | `stream.pause()` | 冻结（内容保留） | **不推进** | 恢复后音视频仍对齐 |
| **恢复** | 自动继续（管道被读空） | `stream.play()` | 继续填充 | 继续推进 | 无爆音、无跳跃 |
| **静音** | 不受影响 | 不受影响 | **继续消耗** | **继续推进** | 只是不出声；取消静音后位置仍对齐 |
| **销毁** | `kill()` + `wait()` | `pause()` → `drop()` | 随 Drop 释放 | — | 无泄漏、无僵尸进程 |

### 7.1 暂停：为什么不重启进程

需求原文建议「暂停时关闭 ffmpeg，恢复时重启新进程以释放混音器句柄」。本实现不这么做：

- **混音器句柄的持有者是 cpal，不是 ffmpeg**。ffmpeg 只往 stdout 写字节，从不接触音频设备。要释放设备句柄，`stream.pause()` 才是正确操作（WASAPI 下会 `IAudioClient::Stop`）；
- 重启进程必然丢失「已解码但未播放」的数据，且需要重新定位到暂停点，反而引入不同步；
- 现方案的暂停是**双保险**：既停 cpal 流（设备层不再拉取），又置位共享标志（回调若仍被调用则输出静音且不推进时钟）。即使某些后端的 `pause()` 不被支持，也一定静音。

```rust
fn set_paused(&self, paused: bool) {
    self.shared.paused.store(paused, Ordering::SeqCst);
    if let Some(s) = &self.stream {
        // pause()/play() 返回不同的错误类型（PauseStreamError / PlayStreamError），
        // 必须分别处理再统一成 String——直接写 if/else 会得到 E0308。
        let err: Option<String> = if paused {
            s.pause().err().map(|e| e.to_string())
        } else {
            s.play().err().map(|e| e.to_string())
        };
        if let Some(e) = err { log::debug!("[audio] stream pause/play 不受支持: {e}"); }
    }
}
```

### 7.2 静音：为什么仍要消耗数据

`set_muted` 只写一个原子布尔。回调中的清零动作发生在 `pop_slice` **之后**：

```rust
let n = cons.pop_slice(out);
if n < out.len() { out[n..].fill(0); }        // 欠载补静音（不重复上段，避免「咔哒」）
clock.played.fetch_add(n as u64, ..);
if shared.muted.load(Ordering::Relaxed) { out.fill(0); }   // ← 数据已消耗、时钟已推进
```

这样「静音 30 秒后取消静音」得到的是视频第 30 秒对应的声音，而不是从静音起点接着播——符合直觉，也让静音不需要任何同步补偿。

### 7.3 销毁顺序（`Drop`）

不依赖字段析构顺序，显式编排，避免「回调仍在运行而 `Consumer` 已被回收」：

```rust
fn drop(&mut self) {
    self.shared.stop.store(true, Ordering::SeqCst);   // 1) 所有循环看到退出信号
    if let Some(s) = self.stream.take() {              // 2) 先停回调，再归还设备
        let _ = s.pause();
        drop(s);
    }
    if let Some(mut c) = self.child.take() {            // 3) kill → 管道关闭 → read 立即返回
        let _ = c.kill();
        let _ = c.wait();                               //    wait 必须调用，否则留下僵尸进程
    }
    self.sync_tx.take();                                // 4) 丢发送端 → 唤醒阻塞在 recv 的线程
    for h in [self.reader.take(), self.logger.take(), self.syncer.take()]
        .into_iter().flatten() { let _ = h.join(); }    // 5) 汇合全部线程
}
```

---

## 8. 零拷贝与实时安全

`fill_pcm` 运行在音频线程的**硬实时上下文**中：全程无锁、无分配、无系统调用、无阻塞，只有原子读写与 `memcpy`。

设备回调给的是 `&mut [i16]` / `&mut [f32]`，而环形缓冲里是 `u8`。用切片重解释直达目标缓冲，避免任何中间 `Vec`：

```rust
let bytes = unsafe {
    std::slice::from_raw_parts_mut(data.as_mut_ptr().cast::<u8>(), std::mem::size_of_val(data))
};
fill_pcm(bytes, &mut cons, &shared);
```

安全性论证：`i16`/`f32` 均无内部不变量（任意位模式都是合法值），对齐要求（2/4 字节）严于 `u8`（1 字节）故必然满足，长度用 `size_of_val` 精确计算不越界，且 ffmpeg 输出的 `s16le`/`f32le` 与 x86/ARM 小端内存布局逐字节一致。全零字节对两种类型都恰好是静音。

> 用 `slice::from_raw_parts_mut` 而非 `mem::transmute`：后者对不同长度的切片类型是 UB（胖指针的长度字段语义会被错误保留）。

`skip` 分支复用 `out` 作为丢弃暂存区（随后会被真实数据覆盖），因此连「丢弃」也不需要额外缓冲。

---

## 9. 已规避的死锁与陷阱清单

| 陷阱 | 后果 | 规避方式 |
|---|---|---|
| `stderr` 设为 `piped` 却无人读 | ffmpeg 写满 4–64KB 管道缓冲后**永久阻塞**，表现为「音频莫名停止且进程不退出」 | `audio-stderr` 线程持续排空（**非可选**） |
| `kill()` 后不 `wait()` | 留下僵尸进程 | `Drop` 中成对调用 |
| 回调中加锁 / 分配 | 优先级反转 → 爆音 | 全原子 + `memcpy`，无 `Mutex`、无 `Vec` |
| 修正量未对齐帧边界 | 左右声道错位半样本，声场异常 | `raw - (raw % frame_bytes)` |
| 欠载时重复上一段 | 可听见的「咔哒」声 | 补静音 `out[n..].fill(0)` |
| 暂停时推进时钟 | 恢复后音视频错位（错位量 = 暂停时长） | `paused` 分支直接 return，不动 `played` |
| 环形缓冲容量非帧整数倍 | 环绕时半帧错位 | 实例化时向下取整对齐 |
| 音频线程持有 `Arc<Mutex<..>>` | 同上优先级反转 | 单个 `Arc<AudioShared>`，内部全为原子 |

---

## 10. 优雅降级

`AudioPipeline::try_new` 返回 `Option<Self>`；`None` **不是错误**，而是「本次播放无音频」的正常状态。触发路径：

1. 文件本身没有音轨（`ffprobe -select_streams a:0` 返回空 `streams`）；
2. 无可用输出设备（无声卡 / 虚拟机 / 设备被独占）；
3. `default_output_config()` 失败；
4. ffmpeg 音频进程启动失败；
5. `build_output_stream()` 或 `stream.play()` 失败。

调用方仅记一条 `log::warn!` 后继续正常播放视频：

```rust
let audio = AudioPipeline::try_new(&ffmpeg, path, is_loop);
if audio.is_none() {
    log::warn!("[audio] 无音频轨道或无可用输出设备 — 静默播放");
}
```

路径 3/4/5 会先把 `stop` 置位、kill 子进程、join 已启动的线程，再返回 `None`——不留半启动状态。

---

## 11. 关键实测数据

在开发机（Windows）上验证：

| 项目 | 实测结果 |
|---|---|
| 默认输出设备 | `扬声器 (Realtek(R) Audio)` |
| `default_output_config` | 48000 Hz / 2 ch / **F32** → 走 `f32le` 通路 |
| PCM 字节数精度 | 2.0 s 正弦 → 48k/2ch/f32 输出 **768,000 B**，即 `768000 / 384000 = 2.000 s`，零误差 |
| **`ffprobe` 陷阱** | `sample_rate` 是**带引号的字符串** `"44100"`，而 `width`/`channels` 是裸数字 |

最后一项曾直接击穿原有的 `parse_u32`（只会解析裸数字，遇引号返回 `None`，导致「明明有音轨却判定无音频」）。修复方式是让 `parse_u32` 容忍可选引号，并补了针对性测试：

```rust
#[test]
fn parse_u32_handles_quoted_and_bare_numbers() {
    assert_eq!(parse_u32(r#"{"sample_rate": "44100"}"#, "\"sample_rate\""), Some(44100));
    assert_eq!(parse_u32(r#"{"channels": 2}"#, "\"channels\""), Some(2));
}
```

同时 `probe()` 现在返回 `VideoInfo { width, height, fps }`，`fps` 从 `avg_frame_rate`/`r_frame_rate` 的有理数字符串（如 `"30000/1001"`）解析，取不到时退化为 30fps（仅影响同步基准，不影响播放）。

---

## 12. 测试矩阵

`cargo test -p drafftink-desktop` → **10 passed**（2 项原有 + 8 项新增）。

| 测试 | 覆盖点 |
|---|---|
| `decode_mp4_yields_valid_rgba_frames` | （原有）视频通路 |
| `decode_is_downscaled_to_max_dim` | （原有）降维滤镜 |
| `probe_audio_detects_track_presence` | 有音轨 → `Some`；无音轨 → `None` |
| `parse_u32_handles_quoted_and_bare_numbers` | ffprobe 引号陷阱回归 |
| `parse_fps_handles_rational` | `"30000/1001"` → 29.97 |
| `audio_format_byte_math_is_exact` | `frame_bytes` / `bytes_per_sec` 换算 |
| `audio_clock_position_tracks_bytes` | 字节数 → 时长映射精度 |
| `audio_args_target_device_format` | 参数含 `-vn`、目标 `-ar/-ac/-f`、且**不含 `-re`** |
| `audio_pipeline_degrades_gracefully` | 无音轨 / 无设备时不 panic |
| `pause_then_resume_keeps_decoding` | **`paused_flag` bug 回归**（见附录） |

`cargo clippy -p drafftink-desktop --all-targets` → **0 warnings**。

---

## 13. 附录：顺带修掉的两个既有缺陷

### 13.1 致命：暂停一次即永久停止解码

接入音频时发现的**既有 bug**（与音频无关，但会让暂停功能完全失效）：

```diff
- let paused_flag = stop.clone();          // ← paused 与 stop 共用同一个 Arc<AtomicBool>！
+ let paused_flag = Arc::new(AtomicBool::new(false));
```

原代码中 `paused` 与 `stop` 指向同一个原子量，因此**第一次暂停就等同于停止**：解码线程看到 `stop == true` 直接退出，恢复播放后再无新帧，画面永久冻结在暂停帧。已由 `pause_then_resume_keeps_decoding` 锁定该行为。

### 13.2 UI：暂停按钮与缩放手柄重叠

暂停按钮圆心原本落在 `rect.max`（右**下**角），与右下角的 resize grip 命中区重叠，导致点击行为不确定。已移至顶边，并在其左侧新增静音按钮：

```rust
let pause_center = pos2(rect.max.x - BTN_R - 2.0, rect.min.y + BTN_R + 2.0);
let mute_center  = pos2(pause_center.x - BTN_R * 2.0 - 4.0, pause_center.y);
```

同时把 `Hit::Button` 拆分为 `Hit::Pause` / `Hit::Mute`，静音按钮仅在 `has_audio()` 为真时绘制与命中。快捷键：`Space` 暂停，`M` 静音。

---

## 14. 已知限制与后续方向

| 限制 | 说明 / 后续 |
|---|---|
| 无 `seek()` API | 设计已就绪（§6.4），需双进程带 `-ss` 重启 |
| 无音量调节 | 目前仅静音开关。加音量需在回调内做乘法（f32 通路简单，i16 需注意溢出饱和） |
| 单音轨 | 只取 `a:0`。多语言轨需扩展 `-map 0:a:<n>` |
| 变速播放不支持 | 需 `atempo` 滤镜 + 同步基准整体缩放 |
| 修正为跳跃式 | skip/pad 是硬切。若对音质要求更高，可换成 `asetrate`/变速重采样做平滑拉伸 |
| 每实例 1 MiB 常驻 | 多路播放时如需压缩内存，可按 `bytes_per_sec` 动态定容（例如固定 0.5 s） |

---

*文档版本：2026-08-15 ｜ 对应实现：`video_player.rs`（1682 行）、`app.rs`（931 行）*
