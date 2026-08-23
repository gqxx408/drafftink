# 校本白板动画系统 — 实现方案（含 C# 源码对齐）

## 一、技术选型

| 维度 | 决策 | 理由 |
|---|---|---|
| **时间驱动** | `std::time::Instant` + egui `request_repaint_after()` | 零依赖，与 egui 帧循环天然匹配 |
| **缓动函数** | 手写 3 个函数（Linear, EaseInOut/Smoothstep, Bounce）| 总共 ~25 行 |
| **音频** | `rodio`（≈200KB 增量）| Rust 最轻量音频库，MIT/Apache2 |
| **数据结构** | 纯 `#[derive]` 结构体 + `Vec` 容器 | 零开销，与现有 model.rs 风格一致 |
| **存储** | `PageContent` 增加 `animations` + `animation_sequence` | 复用现有多页架构，`#[serde(default)]` 向后兼容 |
| **渲染** | 动画属性直接覆盖 `BaseElement.opacity/position/size` | 不改动已有渲染管线 |

### 依赖增量

```
rodio = "0.20"          # 音频播放，~200KB
```

总增量 < 300KB。

---

## 二、C# 源码对齐 — 关键语义差异

以下 6 点是从希沃 C# 源码（dotPeek 反编译）中挖掘出的与常规实现不同的细节，直接决定播放体验是否"像希沃"。

### 2.1 begin_time 的 Before/After 硬编码延迟

C# 的 `BeginTime` 是 `TimeSpan`，但 Before/After 触发中希沃常硬编码 300ms/600ms，不完全依赖 XML。

**指令**：`player.update()` 中，对 `Trigger::Before` 动画，如果 `begin_time == Duration::ZERO`，默认使用 `Duration::from_millis(300)`。对 `Trigger::After`，使用 `Duration::from_millis(600)`。

```rust
fn effective_delay(anim: &ElementAnimation) -> Duration {
    if anim.begin_time > Duration::ZERO {
        return anim.begin_time;
    }
    match anim.trigger {
        AnimationTrigger::Before => Duration::from_millis(300),
        AnimationTrigger::After => Duration::from_millis(600),
        _ => Duration::ZERO,
    }
}
```

### 2.2 HiddenOnStart（对应 C# 的 `IsHideOwnerInBeginTime`）

如果一个元素有 Before/After 动画，在动画开始播放前，该元素**默认隐藏**（opacity=0）。

**指令**：`init_page()` 时，遍历 sequence 中所有动画的 `target_element_id`，在动画还未进入 Active 状态前，强制设 `opacity = 0.0`。只有当动画进入 Active 或该元素没有动画时，才恢复可见性。

```rust
// init_page 中的处理
for anim_id in seq.all_animation_ids() {
    if let Some(anim) = anim_map.get(anim_id) {
        if let Some(elem) = elements.get_mut(&anim.target_element_id) {
            elem.base_mut().opacity = 0.0; // 预隐藏
        }
    }
}
```

这是解决"元素提前闪现"问题的关键。

### 2.3 Click 触发的作用域（背景触发）

C# 的 Click 触发不限于点击元素本身。`TriggerSource` 经常是 `SlideBackground` 或容器元素。

**指令**：`on_canvas_click` 不检查命中的具体元素。只要点击落在 Canvas 范围内，就检查 `click_map` 是否包含一个**特殊背景 ID**（或所有 trigger_source_id）。逻辑：

```rust
fn on_canvas_click(&mut self, click_pos: Pos2, camera: &Camera) {
    let world = camera.screen_to_world(click_pos);
    // 检查是否落在画布内
    if world[0] >= 0.0 && world[1] >= 0.0
        && world[0] <= self.page_size[0] && world[1] <= self.page_size[1]
    {
        // 先检查命中的具体元素，再检查全局背景触发
        for (source_id, anim_ids) in &self.sequence.click_map {
            self.trigger_click(*source_id, anim_ids);
        }
    }
}
```

### 2.4 Repeat 的循环进度计算

C# 的 `AutoReverse` 在本 MVP 阶段忽略。`repeat > 0` 简化为"循环播放"。

**指令**：计算 progress 时用模运算防跳回：

```rust
let raw_t = play_time.as_secs_f32() / anim.duration.as_secs_f32();
let progress = if anim.repeat > 0 {
    raw_t % 1.0 // 循环：小数部分永远在 [0, 1)
} else {
    raw_t.clamp(0.0, 1.0)
};
```

### 2.5 音频抢占

C# 的 `Begin()` 方法先 `Stop()` 上一个动画的音频，防止音效重叠。

**指令**：`ActiveAnimation` 记录 `audio_path`。新动画启动时，若已有音频在播，先 `sink.stop()` 再新建。在 `start_animations()` 入口处处理：

```rust
fn start_animations(&mut self, anim_ids: &[Uuid]) {
    self.sink = Some(rodio::Sink::try_new(&self.stream_handle)?);
    // 旧 sink 被 drop → 音频停止
    for anim_id in anim_ids {
        self.start_one(anim_id);
    }
}
```

### 2.6 缓动函数的"幂等性"

希沃的 `PowerEase(Power=2)` 与 `smoothstep` 有细微偏差。**不做像素级复刻**。smoothstep 在 90% 的场景下肉眼不可分辨。

```rust
fn apply_easing(t: f32, easing: Easing) -> f32 {
    match easing {
        Easing::Linear => t,
        Easing::EaseInOut => t * t * (3.0 - 2.0 * t), // smoothstep
        Easing::Bounce => bounce(t),
    }
}
```

### 2.7 初始值捕获时机 — FromCurrentState

ActiveAnimation 存储 `initial_*` 存在竞态隐患：用户在动画开始前修改了元素属性，动画会从错误的"初始值"开始插值。

**指令**：在 `start_one_animation` 将动画加入 active 队列的**那一瞬间**，立即捕获元素的当前状态。这相当于 WPF 的 `FromCurrentState` 行为。

```rust
fn start_one(&mut self, anim_id: Uuid, elements: &mut HashMap<Uuid, &mut BaseElement>) {
    let anim = self.anim_map.get(&anim_id).unwrap();
    let elem = elements.get_mut(&anim.target_element_id).unwrap();

    // 在加入 active 的瞬间捕获基准值
    let base_opacity = elem.opacity;
    let base_pos = elem.position;
    let base_size = elem.size;

    // 计算目标差值（delta），而非存绝对值 → 每帧计算更快
    let active = ActiveAnimation {
        anim_id,
        start_time: Instant::now(),
        base_opacity, base_pos, base_size,
        delta_opacity: 1.0 - base_opacity,  // FadeIn 为例
        delta_pos: [0.0, 0.0],
        delta_size: [0.0, 0.0],
        easing: anim.easing.clone(),
        duration: anim.duration,
    };
    self.active.push(active);
}
```

**优势**：delta 存储使得 update 中的计算变成 `base + delta * eased`，只需一次乘法和一次加法，比"插值两个绝对值"快约 3 条指令。

### 2.8 Translate 动画的 distance 字段

希沃的位移动画偏移量可能是像素值或百分比。不同分辨率下固定像素会导致幅度不一致。

**指令**：在 `ElementAnimation` 中增加 `distance: Option<f32>`。

- XML 有明确像素偏移 → 用像素
- XML 是百分比（如 25%） → 用 `page_size.max * percentage`
- 缺失 → `page_size.width * 0.25`（默认 1/4 画布宽度）

```rust
// 解析时
let distance = parse_optional_pixel(xml, "Distance")
    .or_else(|| parse_percentage(xml, "DistancePercent")
        .map(|pct| page_size[0].max(page_size[1]) * pct));

// 播放时
let offset = anim.distance.unwrap_or(page_size[0] * 0.25);
```

### 2.9 音频生命周期 — 让 Drop 管理

全局 `Sink` 放在 `AnimationPlayer` 里需要手动 `stop()`，容易漏掉。

**指令**：不使用常驻 `sink` 字段。在 `start_one` 中，如果动画有 `audio_path`，当场 `rodio::Sink::try_new()` → 用 `Sink::append()` 加载文件 → 播放 → 加入 `Vec<Sink>`。每帧 `retain(|s| !s.empty())` 自动清理已播完的音轨。

```rust
struct AnimationPlayer {
    // audio
    stream: Option<rodio::OutputStream>,
    stream_handle: Option<rodio::OutputStreamHandle>,
    pending_sinks: Vec<rodio::Sink>,  // 每帧 retain 清理
}

fn start_one(&mut self, anim_id: Uuid, elements: &mut ...) {
    // ... 创建 ActiveAnimation ...

    if let Some(path) = &anim.audio_path {
        if let Some(handle) = &self.stream_handle {
            if let Ok(sink) = rodio::Sink::try_new(handle) {
                if let Ok(file) = std::fs::File::open(path) {
                    let _ = sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());
                    self.pending_sinks.push(sink); // Sink 播完后自动静默
                }
            }
        }
    }
}

fn update(&mut self, ...) {
    // 清理已播完的 Sink（Drop 自动停）
    self.pending_sinks.retain(|s| !s.empty() || s.len() > 0);
}
```

### 2.10 Unsupported / 零时长动画的快速路径

`duration == 0` 或 `effect == Unsupported` 的动画不应进入 active 队列白白浪费 CPU。

**指令**：在 `start_one_animation` 中，如果 `anim.duration.is_zero()` 或 `matches!(anim.effect, EffectType::Unsupported(_))`，直接在调用处设置元素的终态（opacity=1.0, 位置归原），`return` 不推入 active。

```rust
fn start_one(&mut self, anim_id: Uuid, elements: &mut ...) {
    let anim = self.anim_map.get(&anim_id).unwrap();

    if anim.duration.is_zero() || matches!(anim.effect, EffectType::Unsupported(_)) {
        // 瞬切终态
        if let Some(elem) = elements.get_mut(&anim.target_element_id) {
            elem.opacity = 1.0;
            elem.position = elem.original_position; // 需要设计如何存储"原位"
        }
        log::info!("Animation {} skipped (unsupported/zero-duration)", anim_id);
        return;
    }
    // ... 正常加入 active ...
}
```

### 2.11 调试可视化 — tracing + debug 面板

动画时间线调试极难肉眼判断。

**指令**：
1. 状态机每次变迁打 `info!` 日志：
```
INFO Player: WaitingBefore → PlayingBefore (elapsed=301ms)
INFO Player: PlayingBefore → WaitingAfter (5 animations done)
```
2. `cfg!(debug_assertions)` 下，在 egui 右上角画一个半透明 debug 窗口，实时显示：
   ```
   State: PlayingBefore (1.23s)
   Active: 3/5  |  Elapsed: 0.45s  |  Progress: 90%
   ```
   这对对齐 300ms/600ms 节奏至关重要。

---

### 2.12 ActiveAnimation 最终结构（整合所有细节）

```rust
struct ActiveAnimation {
    anim_id: Uuid,
    start_time: Instant,

    // 基准值（start_one 时捕获）
    base_opacity: f32,
    base_pos: [f32; 2],
    base_size: [f32; 2],

    // 差值（target - base）—— 每帧一次乘加即可
    delta_opacity: f32,
    delta_pos: [f32; 2],
    delta_size: [f32; 2],

    // 缓动和时长（从配置复制，避免间接查找）
    easing: Easing,
    duration: Duration,
    repeat: u32,
    done: bool,
}

// update 中的属性计算（每条 1 行）：
// elem.opacity = (active.base_opacity + active.delta_opacity * eased).clamp(0.0, 1.0);
// elem.position[0] = active.base_pos[0] + active.delta_pos[0] * eased;
// elem.position[1] = active.base_pos[1] + active.delta_pos[1] * eased;
// elem.size[0] = active.base_size[0] + active.delta_size[0] * eased;
// elem.size[1] = active.base_size[1] + active.delta_size[1] * eased;
```

---

## 三、施工细则（v4 — 从伪代码到生产代码的关键细节）

### 3.1 base_pos 与 target_pos 的计算

`BaseElement.position` 在编辑模式下可被拖动，不一定是"动画终点"。

**指令**：
- 解析时优先读取 XML 中的 `<ToX>`/`<ToY>`（绝对终点）或 `<ByX>`/`<ByY>`（相对偏移）
- 如果都没有，以**元素当前位置**为终点
- `delta_pos = target_pos - base_pos`（base_pos 在 `start_one` 瞬间捕获）
- 瞬切终态（Unsupported / 零时长）：直接 `elem.position = base_pos + delta_pos`

```rust
// 解析阶段（animation_parser.rs）
let target_pos = (|| {
    if let (Some(tx), Some(ty)) = (xml_val("ToX"), xml_val("ToY")) {
        return Some([tx, ty]);
    }
    if let (Some(bx), Some(by)) = (xml_val("ByX"), xml_val("ByY")) {
        // ByX/ByY 是相对偏移，与当前元素位置无关
        return Some([bx, by]); // 播放时 base_pos + [bx, by]
    }
    None
})();

// start_one 阶段（animation_player.rs）
let delta_pos = match target_pos {
    Some(t) => [t[0] - base_pos[0], t[1] - base_pos[1]],
    None => [0.0, 0.0], // 无 ToX/ByX → 不位移
};
```

### 3.2 page_size 必须是逻辑画布尺寸

`distance` 字段依赖 `page_size`，但窗口缩放不能影响动画幅度。

**指令**：
- `AnimationPlayer::init_page()` 接收的 `page_size` 是**逻辑画布尺寸**（如 Board.xml 中的 1920×1080）
- 不使用物理窗口尺寸或 `Camera::viewport`
- `Camera` 的投影矩阵与逻辑尺寸匹配：`world_to_screen` 中 `viewport` 已作为物理→逻辑的桥梁

### 3.3 rodio 音频加载的正确姿势

```rust
fn start_one(&mut self, anim_id: Uuid, elements: &mut ...) {
    // ... 创建 ActiveAnimation ...

    if let Some(path) = &anim.audio_path {
        if let Some(handle) = &self.stream_handle {
            // 1. 用 BufReader 提高大文件效率
            // 2. if let Ok 防 panic（文件损坏/格式不支持）
            if let Ok(file) = std::fs::File::open(path) {
                let reader = std::io::BufReader::new(file);
                if let Ok(decoder) = rodio::Decoder::new(reader) {
                    if let Ok(sink) = rodio::Sink::try_new(handle) {
                        sink.append(decoder);
                        self.pending_sinks.push(sink);
                    }
                }
            }
        }
    }
}
```

**路径处理**：XML 中的 `audio_path` 是相对路径（如 `Resources/audio.mp3`）。`animation_parser.rs` 在解析时需基于 ENBX 包根拼接为绝对路径，否则 `File::open` 找不到文件。

### 3.4 Debug 面板实现模式

```rust
// animation_player.rs
#[cfg(debug_assertions)]
pub fn debug_ui(&self, ui: &mut egui::Ui) {
    egui::Window::new("Animation Debug")
        .anchor(egui::Align2::RIGHT_TOP, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            ui.label(format!("State: {:?} ({:.2}s)", self.state, self.state_elapsed.as_secs_f32()));
            ui.label(format!("Active: {}", self.active.len()));
            for a in &self.active {
                let id = a.anim_id.to_string();
                let short = &id[..id.len().min(4)];
                let elapsed = Instant::now().duration_since(a.start_time);
                let pct = (elapsed.as_secs_f32() / a.duration.as_secs_f32() * 100.0).min(100.0);
                ui.label(format!("  {} — {:3.0}%", short, pct));
            }
        });
}

// app.rs — 仅在 debug_assertions 下显示
#[cfg(debug_assertions)]
if self.show_animation_debug {
    self.animation_player.debug_ui(ui);
}
```

### 3.5 Unsupported 日志标准化

```rust
// 在 start_one 的快速路径中
if matches!(anim.effect, EffectType::Unsupported(_)) {
    log::info!(
        "Unsupported animation effect: {:?}, falling back to instant cut.",
        anim.effect
    );
    // 瞬切终态
    elem.opacity = (base_opacity + delta_opacity).clamp(0.0, 1.0);
    elem.position = [base_pos[0] + delta_pos[0], base_pos[1] + delta_pos[1]];
    return;
}
```

格式统一后，`grep "Unsupported animation effect"` 即可列出所有需要后续支持的效果。

### 3.6 Click 触发的背景 ID

希沃 XML 中，背景通常有固定 ID：全零 GUID `00000000-0000-0000-0000-000000000000`。

**指令**：

```rust
// animation_parser.rs
const SLIDE_BACKGROUND_ID: Uuid = Uuid::from_bytes([0; 16]);

// 解析 TriggerSource 时，如果是背景 ID，存入 click_map[SLIDE_BACKGROUND_ID]
// app.rs 的 on_canvas_click：
pub fn on_canvas_click(&mut self, world_pos: [f32; 2]) {
    // 1. 先检查命中的具体元素
    if let Some(hit_id) = self.hit_test(world_pos) {
        if let Some(anim_ids) = self.sequence.click_map.get(&hit_id) {
            self.start_animations(anim_ids);
            return;
        }
    }
    // 2. 全局背景点击（任何画布内点击都触发）
    if let Some(anim_ids) = self.sequence.click_map.get(&SLIDE_BACKGROUND_ID) {
        self.start_animations(anim_ids);
    }
}
```

### 3.7 摘要 — ActiveAnimation::update 完整循环

```rust
// animation_player.rs — update 核心
for active in &mut self.active {
    let elapsed = now.duration_since(active.start_time);
    let raw_t = elapsed.as_secs_f32() / active.duration.as_secs_f32();
    let progress = if active.repeat > 0 { raw_t % 1.0 } else { raw_t.clamp(0.0, 1.0) };
    let eased = apply_easing(progress, active.easing);

    if let Some(elem) = elements.get_mut(&active.target_element_id) {
        elem.opacity = (active.base_opacity + active.delta_opacity * eased).clamp(0.0, 1.0);
        elem.position[0] = active.base_pos[0] + active.delta_pos[0] * eased;
        elem.position[1] = active.base_pos[1] + active.delta_pos[1] * eased;
        elem.size[0] = active.base_size[0] + active.delta_size[0] * eased;
        elem.size[1] = active.base_size[1] + active.delta_size[1] * eased;
    }
    active.done = raw_t >= 1.0 && active.repeat == 0;
}
self.active.retain(|a| !a.done);
```

---

## 四、模块拆分

```
crates/
├── drafftink-core/src/
│   └── animation.rs          ← NEW: 数据结构 + 缓动函数
│
├── drafftink-app/src/
│   ├── animation_player.rs   ← NEW: Timeline 调度器
│   └── app.rs                ← 集成：切页 init / update / click / shutdown
│
└── enbx_importer/src/
    └── animation_parser.rs   ← NEW: Slide XML → ElementAnimation
```

### 3.1 `drafftink-core/src/animation.rs` — 数据层

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AnimationCategory { Enter, Exit, Emphasis }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AnimationTrigger { Click, Before, After }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EffectType {
    // -- Enter (16) --
    FadeIn, TranslateInTop, TranslateInBottom, TranslateInLeft, TranslateInRight,
    ScaleIn, WipeInLeft, WipeInRight, WipeInTop, WipeInBottom,
    FlyInTopLeft, FlyInTopRight, FlyInBottomLeft, FlyInBottomRight,
    SplitInHorizontal, SplitInVertical,

    // -- Exit (12) --
    FadeOut, TranslateOutTop, TranslateOutBottom, TranslateOutLeft, TranslateOutRight,
    ScaleOut, WipeOutLeft, WipeOutRight, WipeOutTop, WipeOutBottom,
    FlyOutTopLeft, FlyOutTopRight, FlyOutBottomLeft, FlyOutBottomRight,
    SplitOutHorizontal, SplitOutVertical,

    // -- Emphasis (12) --
    Transparency, Zoom, Heartbeat, Shake, Wave,
    Spin, Pulse, Teeter, ColorBlend, GrowShrink,
    Darken, Lighten,

    // -- Fallback --
    Unsupported(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Easing { Linear, EaseInOut, Bounce }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Direction {
    Top, Bottom, Left, Right,
    TopLeft, TopRight, BottomLeft, BottomRight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementAnimation {
    pub id: Uuid,
    pub category: AnimationCategory,
    pub trigger: AnimationTrigger,
    pub effect: EffectType,
    #[serde(default)]
    pub easing: Easing,
    pub target_element_id: Uuid,
    #[serde(default)]
    pub trigger_source_id: Option<Uuid>,
    #[serde(default)]
    pub begin_time: Duration,
    pub duration: Duration,
    #[serde(default)]
    pub orientation: Option<Direction>,
    #[serde(default = "default_magnitude")]
    pub magnitude: f32,
    #[serde(default)]
    pub repeat: u32,
    #[serde(default)]
    pub audio_path: Option<String>,
    /// Translate 偏移量（像素），缺失时用 page_size * 0.25
    #[serde(default)]
    pub distance: Option<f32>,
}

fn default_magnitude() -> f32 { 1.0 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlideAnimationSequence {
    #[serde(default)]
    pub before_queue: Vec<Uuid>,
    #[serde(default)]
    pub after_queue: Vec<Uuid>,
    #[serde(default)]
    pub click_map: HashMap<Uuid, Vec<Uuid>>,
}

impl SlideAnimationSequence {
    pub fn is_empty(&self) -> bool {
        self.before_queue.is_empty() && self.after_queue.is_empty() && self.click_map.is_empty()
    }

    pub fn all_target_ids(&self, anim_map: &HashMap<Uuid, ElementAnimation>) -> Vec<Uuid> {
        let mut ids = Vec::new();
        for q in [&self.before_queue, &self.after_queue] {
            for id in q {
                if let Some(anim) = anim_map.get(id) {
                    ids.push(anim.target_element_id);
                }
            }
        }
        ids
    }
}
```

### 3.2 `drafftink-app/src/animation_player.rs` — 播放器

```rust
pub struct AnimationPlayer {
    anim_map: HashMap<Uuid, ElementAnimation>,
    sequence: SlideAnimationSequence,
    active: Vec<ActiveAnimation>,
    state: PlayerState,
    page_size: [f32; 2],

    // Audio
    stream: Option<rodio::OutputStream>,
    stream_handle: Option<rodio::OutputStreamHandle>,
    sink: Option<rodio::Sink>,
}

enum PlayerState {
    WaitingBefore,   // 等待 300ms 后播 before
    PlayingBefore,
    WaitingAfter,    // before 播完后等待 600ms 后播 after
    PlayingAfter,
    WaitingClick,
    Done,
}

struct ActiveAnimation {
    anim_id: Uuid,
    start_time: Instant,
    initial_opacity: f32,
    initial_position: [f32; 2],
    initial_size: [f32; 2],
}
```

### 3.3 `enbx_importer/src/animation_parser.rs` — 解析

对 `Slide_*.xml` 的 `<Animations>` 节点，逐个解析 `AnimationSaveInfo`：

```xml
<Animations>
  <AnimationSaveInfo>
    <Id>guid-here</Id>
    <AnimationCategory>Enter</AnimationCategory>
    <AnimationTrigger>Before</AnimationTrigger>
    <EffectType>FadeIn</EffectType>
    <TargetElementId>element-guid</TargetElementId>
    <BeginTime>0</BeginTime>
    <Duration>500</Duration>
    <Easing>EaseInOut</Easing>
    <Repeat>0</Repeat>
    <!-- etc -->
  </AnimationSaveInfo>
</Animations>
```

同时解析 `<AnimationOrders>`：按顺序排列的 `AnimationId` 队列。

---

## 五、数据流（整合 C# 细节后）

```
enbx 文件
  │
  ▼
animation_parser.rs  →  PageContent { animations, animation_sequence }
  │
  ▼
app.rs: 切页时
  ├─ player.shutdown()           ← 音频停、active 清空
  ├─ player.init_page(seq, map)  ← HiddenOnStart: target 元素 opacity=0
  └─ 进入 WaitingBefore 状态
  │
  ▼
app.rs: 每帧 update(elapsed_since_init)
  ├─ state == WaitingBefore && elapsed >= 300ms → PlayingBefore
  ├─ state == WaitingAfter  && elapsed >= 600ms → PlayingAfter
  ├─ 遍历 active:
  │   ├─ cycle_progress = (elapsed % duration) / duration   ← 循环用模运算
  │   ├─ eased = apply_easing(progress, anim.easing)
  │   └─ apply_effect(elem, anim.effect, eased)
  ├─ 移除 progress >= 1.0 的 active
  └─ active 为空 → 状态机推进:
      PlayingBefore→WaitingAfter → PlayingAfter→WaitingClick
  │
  ▼
render.rs: 照常渲染（opacity 已被动画修改）
```

---

## 六、动画效果实现矩阵

| 效果 | 实现 | 行数 |
|---|---|---|
| FadeIn / FadeOut | `eased` / `1.0 - eased` | 2 |
| TranslateIn (4 方向) | `initial + direction * page_size * (1-eased)` | 6 |
| TranslateOut | `initial + direction * page_size * eased` | 6 |
| ScaleIn | `initial_size * eased` | 2 |
| ScaleOut | `initial_size * (1-eased)` | 2 |
| Transparency | `0.3 + 0.7 * eased`（脉冲） | 3 |
| Zoom | `initial_size * (1.0 + 0.3 * eased)` | 2 |

MVP 总计 < 50 行。

---

## 七、内存估算

| 组件 | 100 页 |
|---|---|
| ElementAnimation (×5/页) | 150 KB |
| SlideAnimationSequence | 20 KB |
| ActiveAnimation (峰值) | < 1 KB |
| rodio 缓冲 | ~2 MB |
| **总计** | **< 3 MB** |

---

## 八、降级策略

| 场景 | 策略 |
|---|---|
| 未知 EffectType | → `Unsupported(name)`，瞬切终态 |
| XML 字段缺失 | `#[serde(default)]` + `unwrap_or_default()` |
| 音频缺失 | 静音，日志 warn |
| 链式触发 | 仅保留第一个 |

---

## 九、五日实施计划

### Day 1 — Core 数据结构

`drafftink-core/src/animation.rs`：枚举、结构体（含 `distance`/`ToX`/`ToY`/`ByX`/`ByY`）、`#[serde(default)]`、缓动函数、`SLIDE_BACKGROUND_ID` 常量、`all_target_ids()`。

### Day 2 — XML 解析

`enbx_importer/src/animation_parser.rs`：逐字段对齐（重点：`ToX`/`ToY`/`ByX`/`ByY`、distance 像素/百分比、`TriggerSource`→Click 映射、音频路径拼绝对）。**最枯燥但最关键**。

### Day 3 — 播放逻辑

`animation_player.rs`：`init_page`（HiddenOnStart + 逻辑 page_size）→ `start_one`（FromCurrentState 基准捕获 + delta + Unsupported 快径 + per-Sink + `BufReader` 容错）→ `update`（300ms/600ms + cycle mod + done retain + sink retain）→ debug tracing + `cfg!(debug)` 面板。

### Day 4 — 效果实现

`apply_effect()`：Fade / Translate（distance + ToX/ToY/ByX）/ Scale / Transparency / Zoom。Unsupported 标准化日志。

### Day 5 — 集成

`app.rs`：切页 shutdown → init → 渲染 update → Click 背景 ID 检测 → 插件卸载。

`app.rs`：切页 shutdown → init → 渲染循环 update → 点击 on_canvas_click → 插件卸载 shutdown。

---

## 十、验收标准

- [ ] 含 Before FadeIn 的课件：文字淡入、无提前闪现
- [ ] 100 页课件：内存增长 < 20MB、CPU < 5%
- [ ] 插件卸载：内存回落至基线 ±5MB
- [ ] 未知效果：不崩溃、日志降级提示
- [ ] `cargo test -p drafftink-core` 动画单测全过
- [ ] 无 unsafe（除 rodio FFI）

---

## 附录：修订记录

| # | 变更 | 版本 |
|---|---|---|
| 1–6 | C# 对齐（300ms/600ms、HiddenOnStart、Click 范围、Repeat mod、音频抢占、缓动哲学）| v2 |
| 7–12 | 用户反馈（FromCurrentState、delta 存储、distance 字段、per-Sink、快径、debug 面板）| v3 |
| 13 | **ToX/ToY/ByX/ByY 解析** — delta 由 XML 目标位置驱动 | v4 |
| 14 | **page_size 为逻辑尺寸** — 窗口缩放不影响动画幅度 | v4 |
| 15 | **BufReader + `if let Ok` 容错** — 坏音频不 panic | v4 |
| 16 | **debug_ui() 方法模式** — `#[cfg(debug)]` 条件编译 | v4 |
| 17 | **Unsupported 标准日志格式** — `grep "Unsupported animation effect"` | v4 |
| 18 | **`SLIDE_BACKGROUND_ID` 背景点击** — 全零 GUID | v4 |
| 19 | **ActiveAnimation 增加 `done: bool` + `repeat: u32`** — `retain` 清理 | v4 |

