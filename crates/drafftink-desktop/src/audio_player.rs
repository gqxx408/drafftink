//! 纯音频叠加层实例（画布上的音频控制条）。
//!
//! 复用 `video_player::AudioPipeline`（ffmpeg 子进程 → 无锁环形缓冲 → cpal 声卡），
//! 因此**不引入任何新音频库**。音频元素不需要视频解码进程，只有一条音频管线。
//!
//! 生命周期：`AudioInstance` 持有 `Option<AudioPipeline>`；实例被移除（Delete / 撤销）
//! 时 `AudioPipeline` 的 `Drop` 会停流 + 杀 ffmpeg 子进程 + join 线程，杜绝僵尸进程。

use std::path::{Path, PathBuf};

use crate::video_player::{ffmpeg_exe, probe_audio, probe_duration, AudioPipeline, SrcAudio};

/// 单个音频叠加层实例（宿主层跟踪，与视频/图片/形状同构）。
pub(crate) struct AudioInstance {
    /// 音频播放管线；`None` 表示无音轨 / 无设备（静默占位，控制条仍显示但不可播）。
    pub(crate) pipeline: Option<AudioPipeline>,
    /// 音轨探测缓存（`new` 时 ffprobe 一次）：`None` 表示无音轨或探测失败。
    /// seek 重建管线时复用——源文件不变、音轨不变，每次 seek 都重新 ffprobe
    /// 会在 UI 线程同步等待 50~300ms，正是拖动/点按进度条卡顿的主因。
    audio_src: Option<SrcAudio>,
    /// 本地音频文件路径（`resource_id` 去掉 `file://` 前缀），seek 重建管线时用。
    pub(crate) path: PathBuf,
    /// 是否循环播放（seek 重建时保持一致）。
    pub(crate) is_loop: bool,
    /// 音频在世界坐标中的默认矩形；`user_rect` 为 `None` 时参与相机变换。
    pub(crate) world_rect: Option<egui::Rect>,
    /// 用户在屏幕上看到的完整控制条矩形（已包含拖拽缩放和位移）。
    pub(crate) user_rect: Option<egui::Rect>,
    /// 音频总时长（毫秒），由 ffprobe 探测；0 表示失败（控制条隐藏时间文本）。
    pub(crate) duration_ms: u64,
    /// 暂停状态（UI 侧镜像）。
    pub(crate) paused: bool,
    /// 是否正在拖动进度条：期间不覆盖显示进度，松手时才一次性 seek。
    pub(crate) seeking: bool,
    /// 拖动进度条的目标时间（毫秒），释放时作为 seek 参数。
    pub(crate) seek_target_ms: u64,
    /// seek 基准（毫秒）：输入级 `-ss` 重建后音频时钟从 0 计起，需叠加回绝对位置。
    pub(crate) seek_base_ms: u64,
    /// 插入序号（z-order）。值越大越靠上。
    pub(crate) z_index: u64,
    /// 所属页面索引：仅当「所属页 == 当前页」时渲染 / 命中。
    pub(crate) page: usize,
}

impl AudioInstance {
    /// 建立音频实例：ffprobe 探测时长 + 音轨信息（各一次并缓存）+ 启动音频 ffmpeg。
    ///
    /// 任何失败都走零 panic 回退（`pipeline = None`，控制条仍渲染但不可播）。
    pub(crate) fn new(path: &Path, is_loop: bool) -> Self {
        let ffmpeg = ffmpeg_exe();
        let path_str = path.to_string_lossy().to_string();
        let duration_ms = ffmpeg
            .as_ref()
            .map(|f| probe_duration(f, &path_str))
            .unwrap_or(0);
        // 音轨信息只探测一次并缓存：seek 时复用，杜绝 UI 线程反复同步 ffprobe。
        let audio_src = ffmpeg.as_ref().and_then(|f| probe_audio(f, &path_str));
        let pipeline = ffmpeg.as_ref().and_then(|f| {
            audio_src
                .as_ref()
                .and_then(|src| AudioPipeline::try_new_with_src(f, &path_str, is_loop, None, src))
        });
        if pipeline.is_none() {
            log::warn!("[audio] 音频管线初始化失败（无音轨或设备不可用）: {}", path.display());
        }
        let mut inst = Self {
            pipeline,
            audio_src,
            path: path.to_path_buf(),
            is_loop,
            world_rect: None,
            user_rect: None,
            duration_ms,
            paused: true, // 插入默认暂停，避免立刻外放。
            seeking: false,
            seek_target_ms: 0,
            seek_base_ms: 0,
            z_index: 0,
            page: 0,
        };
        // 初始即暂停：停流避免插入瞬间出声（pipeline 已建、可播放）。
        inst.set_paused(true);
        inst
    }

    /// 切换播放 / 暂停。
    pub(crate) fn toggle_paused(&mut self) {
        self.set_paused(!self.paused);
    }

    /// 设置暂停状态（联动音频管线）。
    pub(crate) fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if let Some(p) = &self.pipeline {
            p.set_paused(paused);
        }
    }

    /// 当前播放位置（毫秒）：seek 基准 + 管线时钟（输入级 `-ss` 后从 0 计起）。
    pub(crate) fn current_ms(&self) -> u64 {
        let pos = self.pipeline.as_ref().map(|p| p.position_ms()).unwrap_or(0);
        self.seek_base_ms.saturating_add(pos)
    }

    /// 跳转到指定位置（毫秒）：以 `-ss` 重建音频管线，并记录基准。
    ///
    /// 重建复用 `new` 时缓存的音轨信息（`audio_src`），**不再在 UI 线程跑 ffprobe**
    /// ——音频 seek 因此可高频触发（拖动预览），不会同步阻塞。
    pub(crate) fn seek(&mut self, target_ms: u64) {
        self.seek_base_ms = target_ms;
        let was_paused = self.paused;
        let ffmpeg = match ffmpeg_exe() {
            Some(f) => f,
            None => {
                log::warn!("[audio] seek 失败：找不到本地 ffmpeg.exe");
                return;
            }
        };
        let path_str = self.path.to_string_lossy().to_string();
        let new_pipe = self.audio_src.as_ref().and_then(|src| {
            AudioPipeline::try_new_with_src(
                &ffmpeg,
                &path_str,
                self.is_loop,
                Some(target_ms as f64 / 1000.0),
                src,
            )
        });
        if new_pipe.is_some() {
            self.pipeline = new_pipe; // 旧管线在此被 Drop（杀 ffmpeg + 停流）。
        } else {
            log::warn!("[audio] seek 重建音频管线失败（无音轨/设备不可用）");
        }
        // 恢复原播放/暂停状态。
        self.set_paused(was_paused);
    }
}
