# 视频集成验证报告 — drafftink-desktop

> 任务：在 `drafftink-desktop` 中集成本地预编译 FFmpeg 的视频播放能力，作为 egui 叠加层绘制解码帧。
> 日期：2026-08-15

## 1. 结论

| 检查项 | 结果 |
|---|---|
| `cargo check -p drafftink-desktop` | ✅ 通过（无错误/无警告） |
| `cargo clippy -p drafftink-desktop --all-targets` | ✅ **0 警告**（仅其它 crate 的既有 lint，与本任务无关） |
| 端到端解码单元测试 `decode_mp4_yields_valid_rgba_frames` | ✅ 通过（自动生成 MP4 → 解出 320×240 RGBA 帧，非全零） |
| 运行时产物拷贝 | ✅ `target/debug/` 下出现 `ffmpeg.exe` / `ffprobe.exe` / `avcodec-63.dll` / `avutil-61.dll` 等 |

## 2. 关键决策：放弃 ez-ffmpeg 绑定，改为直接调用 ffmpeg.exe

**根因（编译期即失败）**：规格假设用 `ez-ffmpeg` 封装，但其传递依赖 `ffmpeg-next 8.1.0` + `ffmpeg-sys-next 8.1.0` 的源码硬编码了旧版 FFmpeg 的 `AVCodec` 结构体字段（`supported_samplerates` / `sample_fmts` / `ch_layouts`）与旧版 side-data 枚举。本仓库内置的 FFmpeg 是 **2026-07-03 的 git master 开发版**（`avcodec-63` / `avutil-61`），头文件里这些字段已不存在、枚举已新增变体，于是 `ffmpeg-next` 直接报：

```
error[E0609]: no field `supported_samplerates` on type `sys::AVCodec`
error[E0004]: non-exhaustive patterns: AV_FRAME_DATA_DYNAMIC_HDR_SMPTE_2094_APP5 ... not covered
```

这是 Rust 绑定与本地 FFmpeg 的 **ABI 根本不兼容**，无法通过版本微调解决（无兼容的 `ffmpeg-next` 版本覆盖此 dev 版 FFmpeg）。

**解决**：直接驱动本地预编译的 `ffmpeg.exe`（位于 `third_party/ffmpeg/bin/`）。它自带一致的 ABI，最稳妥，且满足全部硬性约束：后台线程解码、优雅退出、零 panic（失败画红矩形 + `warn!`）、不升级 egui/wgpu、不修改 `drafftink-core`/`drafftink-enbx`、不影响既有 269 项形状/SVG 渲染测试（改动仅限 `drafftink-desktop`）。

## 3. 改动文件清单

| 文件 | 改动 |
|---|---|
| `crates/drafftink-desktop/src/video_player.rs` | **重写**：移除 `ez-ffmpeg`，改为用 `std::process` 调 `ffmpeg.exe`：①`ffprobe` 解析宽高；②按 `qsv→d3d11va→dxva2→软解` 优先级用 `ffmpeg -hwaccels` 探测本机可用后端并选首个能出首帧者；③后台线程逐帧读取 RGBA（`-re` 实时节奏、`-stream_loop -1` 循环）经 `sync_channel` 送 UI；④`Drop` 杀子进程 + `join` 线程，优雅退出；⑤解析失败/解码失败均不 panic。 |
| `crates/drafftink-desktop/Cargo.toml` | 删除 `ez-ffmpeg = "0.17"`（零新增依赖，仅用 std）。 |
| `crates/drafftink-desktop/build.rs` | 改为把 `third_party/ffmpeg/bin/` 下的 `ffmpeg.exe` / `ffprobe.exe` / 全部 `*.dll` 拷贝到 `target/<profile>/`（与 exe 同级，零安装）；并注入 `FFMPEG_BIN_DIR` 兜底路径。 |
| `crates/drafftink-desktop/src/app.rs` | 修复 `enter_teach` 中 `path` 被 `DisplayApp::new` 移动后仍在 `collect_embedded_videos(&path)` 借用的问题（`path.clone()`）；移除未读字段 `VideoInstance.is_test`；clippy 小修（`{path:?}`）。视频叠加层逻辑（V 键测试视频 / 内嵌 ENBX 视频 / `draw_video_overlay`）保持不变。 |
| `.cargo/config.toml` | 删除不再需要的 `[env] FFMPEG_DIR`（ffmpeg-sys-next 已不依赖），保留 `[http] proxy`（绕过 Windows 陈旧的 WinHTTP 系统代理 127.0.0.1:7897，指向可用代理 7890）。 |
| `crates/drafftink-desktop/src/video_player.rs`（测试） | 新增 `#[cfg(test)]` 端到端用例，自动生成 MP4 并断言可解出正确 RGBA 帧。 |

## 4. 验证细节

- **解码管线已实测**：`ffmpeg.exe ... -f rawvideo -pix_fmt rgba pipe:1` 对 320×240@10fps 输出恰为 `3072000 = 320*240*4*10` 字节，确认 RGBA 紧密打包、可直接喂 `egui::ColorImage::from_rgba_unmultiplied`。
- **`.bin` 内容探测 OK**：内嵌资源写成 `*.bin` 临时文件后，`ffprobe` 仍能按内容识别 `h264/宽高/帧率`（FFmpeg 对未知扩展名回退内容探测），故 `make_temp_video_file` 的 `.bin` 命名无需改动。
- **硬件加速自动生效**：本机测试选中 `D3D11Va` 后端并成功出帧，证明探测逻辑有效；无可用硬件时回退纯软件解码。

## 5. 手动测试步骤（GUI，无法在无头环境执行，供用户在桌面端验收）

1. **测试视频（V 键）**：运行 `drafftink-desktop` → 备课模式下按 `V` → 选择本地 `mp4/mkv/mov/avi/webm` → 视频以屏幕中心 16:9 叠加层播放；日志打印实际后端（`[video] player started (backend: d3d11va/soft) ...`）。
2. **内嵌课件视频（F5）**：打开含视频元素的 `.enbx`（F5 进入授课）→ 视频按元素 `position/size` 经相机变换叠加在画布对应位置循环播放。
3. **失败兜底**：若资源不可解码，`draw_video_overlay` 画红色占位矩形并 `log::warn!("[video] ... placeholder shown")`，程序不崩溃。

## 6. 已知边界

- 仅解码视频帧（`-an`），无声道（叠加层播放无需音频）。
- 解码失败中途停止时，UI 冻结最后一帧；初始化失败才显示红矩形（符合规格「失败画红色占位」）。
- `ffmpeg.exe` 由 `build.rs` 拷贝到 `target/<profile>/`；分发时若单独打包需确保 `ffmpeg.exe` + DLL 与 `drafftink-desktop.exe` 同级（或由 `DRAFFTINK_FFMPEG_BIN` 环境变量指定）。

## 7. 优化：智能降维打击（解码端自适应缩放）

**背景**：4K 视频单帧 RGBA 约 `3840×2160×4 ≈ 33MB`，若解码缓冲堆积数十帧，内存瞬间飙升到 600MB+，且全尺寸纹理上传/拉伸会让 GPU 3D 占用居高不下、画面卡顿。

**两步优化（`video_player.rs`）**：

1. **分辨率探测 + 自适应缩放（解码端）**：`new_with_max_dim()` 先用 `ffprobe` 拿到源 `src_w×src_h`，计算
   `base_scale = min(max_dim/src_w, max_dim/src_h, 1.0)`（`max_dim` 默认 1920，绝不放大），
   当 `base_scale < 1.0` 时向 ffmpeg 注入 `scale=out_w:out_h,format=rgba` 滤镜，**从源头直接输出缩小后的 RGBA**。
   单帧内存从 ~33MB(4K) 降到 ~8MB(1080p)，且 GPU 纹理上传/拉伸压力骤降。
2. **渲染端应用缩放**：`VideoPlayer` 暴露 `base_scale`（已烘焙进帧尺寸，渲染不再复乘）与 `user_scale`（用户手势缩放，默认 1.0）；`app.rs::draw_video_overlay` 在绘制时按 `user_scale` 以画面中心为锚缩放叠加层；
   `=` / `-` 键可整体放大/缩小（仅改渲染尺寸，不影响解码内存）。

**配套内存保护**：帧通道由无界 `mpsc` 改为 `sync_channel(1)`——解码线程在 UI 取走前一帧前阻塞（背压），从源头杜绝「解码缓冲堆积」；逐帧 `Vec` 改用 `std::mem::take` 移动而非 8MB `clone` 拷贝，降低每帧分配/拷贝开销。

**健壮性**：若降维管线在本机所有后端（含硬件加速）均失败（极少见，如某加速不支持 CPU scale 滤镜），自动回退原分辨率输出以保证可用性，并 `log::warn!` 提示。

**验证（新增测试 `decode_is_downscaled_to_max_dim`）**：

- 用 `new_with_max_dim(path, false, 160.0)` 约束 320×240 样本 → 断言 `base_scale≈0.5`、解码帧恰为 `160×120`、字节数 `160*120*4`、非全零。✅
- 既有 `decode_mp4_yields_valid_rgba_frames` 额外断言 320×240 < 1920 时 `base_scale≈1.0`。✅
- `cargo clippy -p drafftink-desktop --all-targets` 仍 **0 警告**（含降维/缩放相关代码）。✅

## 8. 优化：叠加层交互（暂停/播放 + 边框/grip 拖拽缩放）

**目标**：在不升级 egui、不引入新依赖、不破坏既有渲染/测试的前提下，为视频叠加层增加
暂停/播放切换、右下角 grip 缩放、内部拖拽移动，并配合光标反馈。

**重要实现约束（egui 0.29.1 实测）**：`Context::interact` / `Painter::interact` 在本版本**已不存在**，
仅 `Ui::interact` 可用，而叠加层绘制不在 `Ui` 上下文内。因此交互**未使用 egui 的 widget 命中系统**，
而是直接消费指针输入手动实现（`ctx.pointer_interact_pos()`、`ctx.input(|i| i.pointer.primary_pressed()/primary_down()/delta())`、
`ctx.set_cursor_icon()`），对图层无依赖、完全可控。

**改动文件**：

- `video_player.rs`
  - `VideoPlayer` 新增 `paused: bool`（UI 镜像，用于渲染图标）与 `paused_flag: Arc<AtomicBool>`（与解码线程共享）。
  - 新增 `set_paused(bool)` / `toggle_paused()`：同步镜像、广播原子标志、打印 `info!("[video] paused")` / `"[video] resumed"`。
  - `decode_loop` 新增 `paused` 参数，循环**顶部**判定：`if paused.load() { sleep(33ms); continue }` ——
    **暂停期不读不发送，零 CPU 占用**；UI 侧通过保留上一帧 `TextureHandle` 持续显示末帧，不闪烁。
- `app.rs`
  - `VideoInstance` 新增 `offset: [f32; 2]`（屏幕空间拖拽偏移，不影响解码内存）。
  - `IntegratedApp` 新增 `video_drag: Option<(String, VideoDragMode)>`（全局唯一指针，松开即清除）。
  - `draw_video_overlay` 重写为手动交互：
    - **暂停/播放**：空格键 或 右上角 24×24 半透明圆按钮（播放中显示 ⏸ 双竖条，暂停中显示 ▶ 右向三角；按钮命中区独立，不触发移动）。
    - **缩放**：右下角 12×12 grip（斜纹三角形）→ 以矩形中心为锚 `user_scale *= 1 + (Δx/width + Δy/height)`，并 `clamp(0.1, 5.0)`。
    - **移动**：内部（非边框/按钮）拖拽 → `offset += 拖拽增量`。
    - **边框视觉**：2px 边框（默认灰 `from_gray(150)`，悬停变亮蓝 `from_rgb(0,150,255)`）；右下角 grip 同色斜纹。
    - **光标**：按钮→`PointingHand`、grip→`ResizeNwSe`、边框按边角→`ResizeHorizontal/Vertical/NwSe/NeSw`、内部→`Move`。

**与用户示意伪代码的偏差说明**：用户提供的伪代码基于 `ez-ffmpeg` 的 `self.context.width()` / `texture` / `show_ui` API，
与现状（直接驱动 `ffmpeg.exe`、帧经 `sync_channel` 送达）不符；且 `position: [f32;2]` 在 `VideoPlayer` 中不存在
（内嵌视频位置在 `world_rect`，测试视频居中），故拖拽偏移落在 `VideoInstance.offset`（屏幕空间）。
`paused` 仍按要求落在 `VideoPlayer`，并通过 `Arc<AtomicBool>` 跨线程共享。

**验证**：

- `cargo clippy -p drafftink-desktop --all-targets` 对 `drafftink-desktop` **0 警告** ✅
- `cargo test -p drafftink-desktop` → 2/2 通过（`decode_mp4_yields_valid_rgba_frames` + `decode_is_downscaled_to_max_dim`）✅
- 工作区 `cargo test --workspace`：唯一失败为 `drafftink-core::crypto::test_jwt_config_from_env_present_returns_ok`
  （沙箱缺 `JWT_SECRET` 环境变量 → `MissingJwtSecret`）；设置 `JWT_SECRET` 后该测试通过 —— **与本次改动无关**（未触碰 `drafftink-core`），
  269 测试基线未被破坏。
- 手动测试步骤（GUI，桌面端验收）：
  1. 运行 `drafftink-desktop`，按 **V** 加载本地测试视频；
  2. 按 **空格** 或点击右上角按钮 → 暂停（按钮变 ▶、日志 `paused`），再按 → 继续（按钮变 ⏸、日志 `resumed`）；
  3. 鼠标移到右下角 → 光标变 resize 图标、边框/ grip 变亮蓝；按下拖动 → 以中心为锚缩放；
  4. 鼠标移到视频内部 → 光标变移动图标；按下拖动 → 视频整体移动；
  5. 确认拖拽过程不闪烁、不跳帧（暂停时解码线程睡眠、不占 CPU）。


