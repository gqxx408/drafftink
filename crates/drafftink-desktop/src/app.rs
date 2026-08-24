//! Drafftink Desktop — 备授一体宿主（单一 eframe 应用）。
//!
//! 完全复用 `drafftink-edit` 的 `EditApp`（备课）与 `drafftink-display` 的
//! `DisplayApp`（授课）：不重写二者核心逻辑，仅在 `IntegratedApp` 上层做
//! 模式切换与状态共享。
//!
//! - **F5**：备课 → 授课（全屏）
//! - **Esc**：授课 → 备课（不再退出进程，由 `DisplayApp` 通知宿主）
//!
//! 共享状态统一存放在 [`drafftink_core::integration::SharedContext`]
//! （`Arc<Mutex<_>>`），两模式通过它安全读写，无数据竞争。
//!
//! **Phase 2 关键点**：
//! 1. 插件管理器在启动期创建并加载**一次**，存入共享上下文；授课端复用同一实例，
//!    彻底避免 cdylib 双加载与符号冲突。
//! 2. 授课端批注（板书 / 标注 / 小测）仅写入课件 `annotations_data` 批注层，
//!    绝不触碰内容层 `elements`（学生原始作答快照），符合「作业防篡改」红线。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use drafftink_core::document::StrokeData as CoreStroke;
use drafftink_core::integration::{SharedAppContext, SharedContext};
use drafftink_core::model::{BaseElement, Element, ShapeKind, SvgShapeElement};
use drafftink_core::plugin::api::DummyContext;
use drafftink_core::plugin::PluginManager;
use drafftink_display::DisplayApp;
use drafftink_edit::{EditApp, TeachingToolKind};
use eframe::App;

use crate::audio_player::AudioInstance;
use crate::interactive_rect::{HitZone, RectInteraction};
use crate::shape_renderer::draw_shape;
use crate::stroke_conv::{core_vec_to_ink, ink_vec_to_core};
use crate::tools::{
    angle_of, closest_point_on_line, dist_to_segment, draw_active_tool, find_nearest_edge,
    line_draw_result, protractor_to_unified, set_square_centroid, set_square_edges,
    snap_angle, snap_angle_grid15, snap_dir_axis, snap_dir_grid45, unified_to_protractor,
    ActiveTool, CompassMode, CompassTool, CountdownTool, FunctionPlotTool, NumberLineTool,
    PolygonTool, ProtractorMode, ProtractorTool, RulerTool, SetSquareKind, SetSquareTool,
    WhichEnd,
};
use crate::undo::{UndoCmd, UndoHistory};
use crate::video_player::VideoPlayer;
use drafftink_core::element::{Element as ElementTrait, ElementData};
use uuid::Uuid;

/// 当前运行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum AppMode {
    #[default]
    Prepare,
    Teach,
}

/// 放大镜（Magnifier）授课工具：**纯 UI 覆盖层**，不序列化、不进任何文档数据模型。
///
/// - `center`：放大镜圆心（屏幕坐标，跟随鼠标 `hover_pos`）。
/// - `radius`：圆圈半径（px，默认 120）。
/// - `zoom_factor`：放大倍数（默认 2.0，鼠标滚轮可在 1.0 ~ 4.0 之间调节）。
/// - `active`：是否激活（点「🔍 放大镜」按钮或 Esc 切换）。
///
/// 与其它虚拟教具（需提交为 `ShapeInstance` 才可持久化）不同，放大镜仅作实时预览，
/// 不产生持久元素、不进 Undo 栈，因此无需任何序列化字段。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MagnifierTool {
    /// 放大镜中心（屏幕坐标，跟随鼠标）。
    pub center: egui::Pos2,
    /// 放大镜半径（px）。
    pub radius: f32,
    /// 放大倍数（1.0 ~ 4.0）。
    pub zoom_factor: f32,
    /// 是否激活。
    pub active: bool,
}

impl Default for MagnifierTool {
    fn default() -> Self {
        Self {
            center: egui::Pos2::new(200.0, 200.0),
            radius: 120.0,
            zoom_factor: 2.0,
            active: false,
        }
    }
}

/// 随机点名器（Name Picker）：老师临时输入学生名单，滚动暂停后选中一名。
///
/// **纯 UI / 临时数据**：名单仅存在本工具内、每次授课临时录入，**不序列化到 ENBX**；
/// 关闭窗口（✕ / Esc / 关闭按钮）后名单保留，下次打开仍在。
///
/// - `names`：学生名单；`input_text`：当前输入框内容。
/// - `display_name`：滚动显示区当前名字；`is_rolling`：是否正在滚动。
/// - `roll_speed`：滚动切换间隔（秒）；`elapsed`：当前间隔累计时间。
/// - `position`：窗口位置（可拖拽）；`selected_name`：最终选中的名字。
/// - `visible`：窗口可见性（本工具独立维护，关闭 ≠ 清空名单）。
#[derive(Debug, Clone)]
pub struct NamePickerTool {
    /// 学生名单。
    pub names: Vec<String>,
    /// 当前输入框内容。
    pub input_text: String,
    /// 当前滚动显示的名字。
    pub display_name: String,
    /// 是否正在滚动。
    pub is_rolling: bool,
    /// 滚动速度（名字切换间隔，秒）。
    pub roll_speed: f32,
    /// 累计时间（用于计时切换）。
    pub elapsed: f32,
    /// 窗口位置。
    pub position: egui::Pos2,
    /// 最终选中的名字。
    pub selected_name: Option<String>,
    /// 窗口是否可见（关闭时名单保留，下次打开仍在）。
    pub visible: bool,
}

impl Default for NamePickerTool {
    fn default() -> Self {
        Self {
            names: Vec::new(),
            input_text: String::new(),
            display_name: String::new(),
            is_rolling: false,
            roll_speed: 0.08,
            elapsed: 0.0,
            position: egui::Pos2::new(320.0, 180.0),
            selected_name: None,
            visible: false,
        }
    }
}

/// 解析名单输入：英文逗号 / 中文逗号 / 分号 / 换行分隔，去空白、跳过空项。
///
/// 独立成纯函数便于单测（`name_picker_add_names`）。
pub(crate) fn parse_names(input: &str) -> Vec<String> {
    input
        .split([',', '，', ';', '；', '\n', '\r'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

impl NamePickerTool {
    /// 从输入框解析并追加名单（去重）；把已加入的名字去除后清空输入框。
    pub fn add_from_input(&mut self) {
        for n in parse_names(&self.input_text) {
            if !self.names.contains(&n) {
                self.names.push(n);
            }
        }
        self.input_text.clear();
    }

    /// 清空名单并复位滚动 / 选中状态。
    pub fn clear_names(&mut self) {
        self.names.clear();
        self.display_name.clear();
        self.selected_name = None;
        self.is_rolling = false;
        self.elapsed = 0.0;
    }

    /// 停止滚动：锁定当前显示的名字为选中项。
    /// 若显示区为空（尚未开始/名单空），则回退到名单第一项，保证永远选中一名。
    pub fn stop_rolling(&mut self) {
        self.is_rolling = false;
        self.elapsed = 0.0;
        if self.display_name.is_empty() {
            if let Some(first) = self.names.first() {
                self.display_name = first.clone();
            }
        }
        self.selected_name = Some(self.display_name.clone());
    }
}

/// 备授一体宿主：在同一 eframe 窗口内切换备课 / 授课。
pub struct IntegratedApp {
    mode: AppMode,
    edit: EditApp,
    display: Option<DisplayApp>,
    shared: SharedContext,
    /// F12 呼出的性能 / 内存监控面板可见性（仅诊断用，不参与业务逻辑）。
    show_profiler: bool,
    /// 视频播放实例叠加层：key = 资源 id（内嵌）或元素 id（本地文件视频）。
    video_instances: std::collections::HashMap<String, VideoInstance>,
    /// 图片叠加层实例（宿主层跟踪，与视频同构）。key = 图片实例 id。
    image_instances: std::collections::HashMap<String, ImageInstance>,
    /// 全局唯一拖拽守卫（视频 + 图片共用）：当前正在拖动哪个矩形实例、命中哪个区域。
    /// `None` 表示无拖拽；仅当守卫空闲时实例方可认领，保证跨实例互斥。
    active_drag: Option<(egui::Id, HitZone)>,
    /// 后台线程弹出的文件选择框选中的视频路径，由主循环每帧取出并插入当前幻灯片。
    pending_videos: Arc<Mutex<Vec<PathBuf>>>,
    /// 后台线程弹出的文件选择框选中的图片路径，由主循环每帧取出并插入当前幻灯片。
    pending_images: Arc<Mutex<Vec<PathBuf>>>,
    /// 备课端插入的本地视频记录（文档层无法容纳 `ElementData::Video`，故在宿主层跟踪）。
    inserted_videos: Vec<InsertedVideo>,
    /// 图片纹理缓存：路径 → egui 纹理句柄。懒加载，避免每帧重新解码。
    image_textures: std::collections::HashMap<PathBuf, egui::TextureHandle>,
    /// 形状叠加层实例（宿主层跟踪，与图片同构）。key = 形状实例 id。
    shape_instances: std::collections::HashMap<String, ShapeInstance>,
    /// 音频叠加层实例（宿主层跟踪，与视频/图片/形状同构）。key = 音频实例 id。
    audio_instances: std::collections::HashMap<String, AudioInstance>,
    /// 函数绘图叠加层实例（宿主层跟踪，与形状同构）。key = 函数实例 id。
    function_instances: std::collections::HashMap<String, FunctionPlotInstance>,
    /// 备课端插入的本地音频记录（文档层无法容纳 `ElementData::Audio`，故在宿主层跟踪）。
    inserted_audios: Vec<InsertedAudio>,
    /// 后台线程弹出的文件选择框选中的音频路径，由主循环每帧取出并插入当前幻灯片。
    pending_audios: Arc<Mutex<Vec<PathBuf>>>,
    /// 点击放置：选定音频文件后进入，幽灵音频框跟随光标，单击画布固定到该处。
    pending_audio: Option<std::path::PathBuf>,
    /// 撤销 / 重做历史（命令模式），覆盖文档层文本与宿主叠加层实例。
    history: UndoHistory,
    /// 当前激活的虚拟教具（圆规/三角尺/量角器）；`None` 表示无教具。
    active_tool: ActiveTool,
    /// 教具双击检测：上次单击的时刻（用于 350ms 内两次单击视为双击提交）。
    last_tool_click: Option<std::time::Instant>,
    /// 拖拽（移动/缩放）进行中的起始快照：拖拽开始帧记录旧几何，结束帧据此 push Modify。
    drag_snapshot: Option<DragSnapshot>,
    /// 文本内容编辑的起始快照：编辑开始前记录旧元素，失焦/取消选中时据此 push ModifyText。
    text_edit_undo: Option<TextEditSession>,
    /// 「点击放置」模式：点「插入」后进入，幽灵形状跟随光标，单击画布固定到该处。
    pending_shape: Option<drafftink_core::model::ShapeKind>,
    /// 点击放置：选定视频文件后进入，幽灵视频框跟随光标，单击画布固定到该处。
    pending_video: Option<std::path::PathBuf>,
    /// 点击放置：选定图片文件后进入，幽灵图片框跟随光标，单击画布固定到该处。
    pending_image: Option<std::path::PathBuf>,
    /// 放置模式武装标志：跳过「打开文件对话框 / 点插入按钮」那一下的点击，
    /// 下一帧起才允许放置，避免误放到工具栏或文件对话框上。
    pending_armed: bool,
    /// 「插入图形」拖拽绘制中的临时状态（仅 UI 层，不进入持久化数据）。
    /// 按下左键 → `Some`，拖动实时更新 `current_screen`，松开时提交为 `ShapeInstance`
    /// 并清除；`None` 表示当前没有正在拖拽绘制的矩形。
    shape_draw: Option<ShapeDrawState>,
    /// 放大镜工具状态（纯 UI 覆盖层：跟随鼠标、滚轮调倍数，不序列化、不进文档数据模型）。
    magnifier: MagnifierTool,
    /// 随机点名器（授课工具：浮动窗口 + 临时名单，不序列化、闭环后名单保留）。
    name_picker: NamePickerTool,
    /// 当前选中的画布元素（形状 / 视频 / 图片之一，仅备课模式有效）。
    /// `None` 表示无选中——元素以「固定背景元素」呈现（无边框 / 抓手），
    /// 老师主动单击该元素才选中进入微调（「先隐身后选中」范式）。
    /// 三个叠加层共享这一个选中态，保证同一时刻只有单个元素可被微调，
    /// 取代原先各叠加层各自维护的 `selected_*_id` 字段。
    selected_element_id: Option<SelectedElement>,
    /// 全局递增的插入序号，用于区分元素的前后（z-order）。后插入的元素 z_index 更大，
    /// 命中检测时优先被选中，渲染时也绘制在更上层，从而保证大图形内部的
    /// 小图形仍可被正确选中。
    next_z_index: u64,
    /// 上一帧渲染时所处的画布页面索引，用于检测「翻页」并清空跨页残留的选中态，
    /// 避免翻到新页面后旧页选中边框残留。
    last_page: usize,
    /// 框选（marquee）起点：在画布空白处按下并拖拽时记录；`None` 表示非框选拖拽。
    marquee_start: Option<egui::Pos2>,
    /// 当前正在显示 / 拖拽的选框矩形（屏幕空间，仅备课模式）。
    marquee_rect: Option<egui::Rect>,
    /// 框选命中单个文本元素后待弹出的「函数绘图」菜单状态。
    function_menu: Option<FunctionMenuState>,
}

/// 框选命中单个文本后的函数绘图菜单状态。
#[derive(Clone)]
struct FunctionMenuState {
    /// 菜单锚点矩形（框选矩形下限，菜单在其下方弹出）。
    anchor: egui::Rect,
    /// 命中的文本元素内容（去掉首尾空白用以解析）。
    text: String,
    /// 点击「📈 函数绘图」后解析失败的错误信息（`Some` 时菜单内显示红字并保持打开）。
    error: Option<String>,
}

/// 「插入图形」拖拽绘制的临时 UI 状态（橡皮筋 / 拖拽绘制）。
///
/// 只存在于 UI 层（本集成宿主），**不混入**文档业务数据（`PageData`）——
/// 按下左键记录 `start_screen`，拖动把当前鼠标位置写进 `current_screen`，
/// 松开时一次性转为 `ShapeInstance` 落库并清除本状态。因此绘制过程中的
/// 矩形既不是持久化元素，也不进 Undo 栈，直到提交才作为正式实例存在。
struct ShapeDrawState {
    /// 正在绘制的形状种类（来自「➕ 插入」按钮）。
    #[allow(dead_code)]
    kind: drafftink_core::model::ShapeKind,
    /// 按下左键时的画布起点（屏幕坐标 `Pos2`）。
    start_screen: egui::Pos2,
    /// 当前鼠标拖动位置（屏幕坐标 `Pos2`）；`None` 表示尚未开始移动。
    /// 宽高 = `start_screen` 到 `current_screen` 的差，随鼠标实时变化。
    current_screen: Option<egui::Pos2>,
}

/// 画布上可被单击选中的叠加层元素种类。
///
/// 形状 / 视频 / 图片在宿主层分别用独立的 `HashMap` 跟踪，key 互不冲突，
/// 故用枚举把「类型 + 实例 id」打包为单一选中态，取代原先三套 `*_id` 字段，
/// 避免多头维护、保证全局唯一选中。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SelectedElement {
    /// 形状叠加层实例（key = `shape_*`）。
    Shape(String),
    /// 视频叠加层实例（key = 视频实例 id）。
    Video(String),
    /// 图片叠加层实例（key = 图片实例 id）。
    Image(String),
    /// 音频叠加层实例（key = 音频实例 id）。
    Audio(String),
    /// 文档层文本元素（key = 文本元素 `base.id` 的 Uuid 字符串）。
    Text(String),
    /// 函数绘图叠加层实例（key = 函数实例 id）。
    Function(String),
}

/// 拖拽（移动 / 缩放）进行中的起始快照，用于拖拽结束后构造 `UndoCmd::Modify*`。
enum DragSnapshot {
    /// 宿主叠加层（形状/图片/视频/音频）：旧 `user_rect`。
    Overlay {
        sel: SelectedElement,
        old_rect: Option<egui::Rect>,
    },
    /// 文档层文本：旧完整元素（含 base + 文本内容）。
    Text {
        page: usize,
        elem_id: Uuid,
        old: Element,
    },
}

/// 文本内容编辑会话：记录「开始编辑前」的完整元素，失焦 / 取消选中时据此提交
/// `UndoCmd::ModifyText`（文本内容修改的撤销）。
struct TextEditSession {
    page: usize,
    elem_id: Uuid,
    old: Element,
}

/// 单个视频叠加层实例。
struct VideoInstance {
    /// 内嵌视频的世界坐标矩形；测试视频为 `None`（绘制在屏幕中心）。
    /// 仅在 `user_rect` 为 `None` 时作为「默认矩形」参与相机变换（跟随画布平移/缩放）；
    /// 一旦用户拖拽/缩放（写入 `user_rect`），即以屏幕空间矩形为唯一真相来源。
    world_rect: Option<egui::Rect>,
    /// 后台解码器；`None` 表示初始化失败，绘制红色占位矩形。
    player: Option<VideoPlayer>,
    /// 上一帧成功解码后缓存的纹理，用于无新帧时持续绘制。
    last_tex: Option<egui::TextureHandle>,
    /// 用户在屏幕上看到的完整视频矩形（已包含拖拽缩放和位移）。
    /// `None` 时表示「尚未被用户修改过」，渲染用默认矩形（居中 + 原始解码尺寸 / 相机锚定）。
    user_rect: Option<egui::Rect>,
    /// 视频总时长（毫秒），由 VideoPlayer 经 ffprobe 探测得到；0 表示探测失败。
    duration_ms: u64,
    /// 当前播放位置（毫秒），每帧由后台 stderr 进度线程解析的 `out_time_ms` 刷新。
    current_ms: u64,
    /// 是否正在拖动进度条：期间冻结画面（不取帧）、不刷新 current_ms（进度条跟手）、
    /// **不发起任何真实 seek**——整个拖动过程零进程重启，松手时才一次性精确定位。
    seeking: bool,
    /// 拖动进度条的目标时间（毫秒），释放时作为唯一一次 seek 参数。
    seek_target_ms: u64,
    /// 插入序号（z-order）。值越大越靠上，命中检测时优先被选中，渲染时也后绘制。
    z_index: u64,
    /// 所属页面索引：叠加层实例只在「所属页 == 当前页」时才渲染 / 参与命中检测，
    /// 从而新建页面不会再显示上一页的本地视频、图片或形状（解决「新页面残留旧内容」）。
    page: usize,
}

/// 单个图片叠加层实例（宿主层跟踪，与 `VideoInstance` 同构，但无解码器）。
#[derive(Clone)]
pub(crate) struct ImageInstance {
    /// 图片本地文件路径（绝对路径；`image` crate 据此懒加载纹理）。
    path: PathBuf,
    /// 图片在世界坐标中的默认矩形；`user_rect` 为 `None` 时参与相机变换
    /// （跟随画布平移/缩放）。一旦用户拖拽/缩放（写入 `user_rect`），即以屏幕空间
    /// 矩形为唯一真相来源，与视频一致。
    world_rect: Option<egui::Rect>,
    /// 用户在屏幕上看到的完整图片矩形（已包含拖拽缩放和位移）。
    /// `None` 时表示「尚未被用户修改过」，渲染用 `world_rect` 相机锚定的矩形。
    user_rect: Option<egui::Rect>,
    /// 插入序号（z-order）。值越大越靠上，命中检测时优先被选中，渲染时也后绘制。
    z_index: u64,
    /// 所属页面索引：仅当「所属页 == 当前页」时渲染 / 命中（与形状 / 视频一致）。
    page: usize,
}

/// 单个形状叠加层实例（宿主层跟踪，与 `ImageInstance` 同构，但携带样式字段）。
#[derive(Clone)]
pub(crate) struct ShapeInstance {
    /// 形状种类（圆/方/括号/箭头等）。对应顶边栏选择器的 [`drafftink_core::model::ShapeKind`]。
    kind: drafftink_core::model::ShapeKind,
    /// 形状在世界坐标中的默认矩形；`user_rect` 为 `None` 时参与相机变换
    /// （跟随画布平移/缩放）。一旦用户拖拽/缩放（写入 `user_rect`），即以屏幕空间
    /// 矩形为唯一真相来源，与图片/视频一致。
    world_rect: Option<egui::Rect>,
    /// 用户在屏幕上看到的完整形状矩形（已包含拖拽缩放和位移）。
    /// `None` 时表示「尚未被用户修改过」，渲染用 `world_rect` 相机锚定的矩形。
    user_rect: Option<egui::Rect>,
    /// 描边线宽（世界单位；绘制时乘以相机 zoom）。
    stroke_width: f32,
    /// 描边颜色 RGBA（0–255）。
    stroke_color: (u8, u8, u8, u8),
    /// 填充颜色；`None` 表示不填充（仅描边）。
    fill_color: Option<(u8, u8, u8, u8)>,
    /// 弧 / 扇 / 角的起止角（度，屏幕空间，0°=正右、逆时针为正）；仅
    /// `Arc` / `Sector` / `Angle` 使用，其余为 `None`。
    arc_degrees: Option<(f32, f32)>,
    /// 线段方向（仅 `Line` 使用）：`false` = 左上→右下对角线，`true` = 右上→左下。
    line_flipped: bool,
    /// 插入序号（z-order）。值越大越靠上，命中检测时优先被选中，渲染时也后绘制。
    z_index: u64,
    /// 所属页面索引：仅当「所属页 == 当前页」时渲染 / 命中（与视频 / 图片一致）。
    page: usize,
}

/// 测试视频 / 内嵌视频在 `user_rect` 尚未被用户写入时的默认屏幕矩形：
/// 画布中心、宽度占屏 60%、16:9。
fn default_overlay_rect(screen: egui::Rect) -> egui::Rect {
    let w = screen.width() * 0.6;
    let h = w * 9.0 / 16.0;
    egui::Rect::from_center_size(screen.center(), egui::vec2(w, h))
}

/// 坐标系在画布上的默认矩形：中心、边长 20×scale（x ∈ [-scale, scale] px，±10 单位）。
fn function_default_rect(screen: egui::Rect, scale: f32) -> egui::Rect {
    let half = 10.0 * scale;
    egui::Rect::from_center_size(screen.center(), egui::vec2(half * 2.0, half * 2.0))
}

/// 放大镜坐标变换：把「原屏幕坐标点」映射为放大后的屏幕坐标。
///
/// 数学推导：先把屏幕坐标经 `canvas_offset` / `canvas_zoom` 映射到画布（世界）坐标，
/// 再在画布空间中围绕放大镜圆心放大 `zoom_factor` 倍，最后映射回屏幕坐标。
/// 由于缩放围绕圆心居中，`canvas_offset` 与 `canvas_zoom` 会在两步折算中互相抵消，
/// 最终**等价于**以放大镜圆心为基准的纯屏幕缩放：
///
/// ```text
/// result = center + (screen - center) * zoom_factor
/// ```
///
/// 这是放大镜在圈内「以其圆心为中心放大重绘」内容的核心变换；独立成纯函数便于单测。
pub(crate) fn magnifier_transform(
    screen: egui::Pos2,
    center: egui::Pos2,
    canvas_offset: egui::Vec2,
    canvas_zoom: f32,
    zoom_factor: f32,
) -> egui::Pos2 {
    // 屏幕 → 画布坐标（用 Vec2 做偏移/缩放运算，Pos2 仅到最后再转回）。
    let scr = screen.to_vec2();
    let cen = center.to_vec2();
    let w = (scr - canvas_offset) / canvas_zoom;
    // 放大镜圆心对应的画布坐标。
    let cw = (cen - canvas_offset) / canvas_zoom;
    // 画布空间中围绕圆心放大。
    let mw = cw + (w - cw) * zoom_factor;
    // 画布 → 屏幕。
    (canvas_offset + mw * canvas_zoom).to_pos2()
}

/// 构造一个「带洞圆环」三角形网格（外圆 - 内圆），用于把放大镜圆外区域压暗的同时
/// 保持圆内内容可见（egui 0.29 不支持圆形 clip / PathShape 的 even-odd，故用 Mesh 手拼环）。
fn annulus_mesh(center: egui::Pos2, r_in: f32, r_out: f32, color: egui::Color32) -> egui::Mesh {
    use egui::epaint::Vertex;
    let mut mesh = egui::Mesh::default();
    let n = 96usize;
    // 内、外两圈顶点；每圈首尾闭合，作为带状三角形带。
    let ring = |mesh: &mut egui::Mesh, r: f32| -> u32 {
        let start = mesh.vertices.len() as u32;
        for k in 0..n {
            let a = std::f32::consts::TAU * k as f32 / n as f32;
            mesh.vertices.push(Vertex {
                pos: egui::pos2(center.x + r * a.cos(), center.y + r * a.sin()),
                uv: egui::pos2(0.0, 0.0),
                color,
            });
        }
        start
    };
    let i0 = ring(&mut mesh, r_in);
    let i1 = ring(&mut mesh, r_out);
    for k in 0..n {
        let k2 = (k + 1) % n;
        let a = i0 + k as u32;
        let b = i0 + k2 as u32;
        let c = i1 + k as u32;
        let d = i1 + k2 as u32;
        mesh.indices.extend_from_slice(&[a, c, b, b, c, d]);
    }
    mesh
}

/// 在一个坐标系矩形内绘制：网格 + 坐标轴（含箭头）+ 刻度 + 函数曲线 + 表达式标签。
///
/// - 数学 y 向上，屏幕 y 向下 → 屏幕 y = origin.y - y·scale（Y 轴翻转）。
/// - 曲线采样 400 点，绘制时用 `with_clip_rect` 裁剪到坐标系矩形内（超出自动裁剪）。
/// - 除零 / 非有限值由 `sample_points` 跳过，曲线在断点处自然断开。
fn draw_function_plot(
    painter: &egui::Painter,
    rect: egui::Rect,
    scale: f32,
    expr: &crate::function_parser::Expr,
    label: &str,
) {
    let origin = rect.center();
    let grid = egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 42, 48));
    let axis = egui::Stroke::new(1.6, egui::Color32::from_rgb(170, 175, 185));
    let tick_font = egui::FontId::proportional(11.0);
    let tick_col = egui::Color32::from_rgb(150, 155, 165);

    // 整数单位网格（跳过节/轴）。
    for k in -10..=10 {
        if k == 0 {
            continue;
        }
        let gx = origin.x + k as f32 * scale;
        let gy = origin.y - k as f32 * scale;
        painter.line_segment(
            [egui::pos2(gx, rect.top()), egui::pos2(gx, rect.bottom())],
            grid,
        );
        painter.line_segment(
            [egui::pos2(rect.left(), gy), egui::pos2(rect.right(), gy)],
            grid,
        );
    }

    // 坐标轴：穿过原点（矩形中心）的横轴（X）与纵轴（Y）。
    painter.line_segment(
        [egui::pos2(rect.left(), origin.y), egui::pos2(rect.right(), origin.y)],
        axis,
    );
    painter.line_segment(
        [egui::pos2(origin.x, rect.top()), egui::pos2(origin.x, rect.bottom())],
        axis,
    );

    // 轴端箭头：X 指向右侧、Y 指向顶部。
    let ah = 6.0; // 箭头半高（px）
    let spear = 13.0; // 箭头长度（px）
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(rect.right(), origin.y),
            egui::pos2(rect.right() - spear, origin.y - ah),
            egui::pos2(rect.right() - spear, origin.y + ah),
        ],
        egui::Color32::from_rgb(170, 175, 185),
        egui::Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(origin.x, rect.top()),
            egui::pos2(origin.x - ah, rect.top() + spear),
            egui::pos2(origin.x + ah, rect.top() + spear),
        ],
        egui::Color32::from_rgb(170, 175, 185),
        egui::Stroke::NONE,
    ));

    // 刻度数字：X 轴 -10,-5,0,5,10 标在轴下，Y 轴标在轴右（0 只标一次）。
    for v in [-10.0, -5.0, 0.0, 5.0, 10.0] {
        let sx = origin.x + v * scale;
        painter.text(
            egui::pos2(sx, origin.y + 3.0),
            egui::Align2::CENTER_TOP,
            format!("{v:.0}"),
            tick_font.clone(),
            tick_col,
        );
        if v != 0.0 {
            let sy = origin.y - v * scale;
            painter.text(
                egui::pos2(origin.x + 4.0, sy),
                egui::Align2::LEFT_CENTER,
                format!("{v:.0}"),
                tick_font.clone(),
                tick_col,
            );
        }
    }

    // 函数曲线：采样后映射到屏幕，裁剪在坐标系矩形内。
    let pts = crate::function_parser::sample_points(expr, -10.0, 10.0, 400);
    let curve = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 100, 0));
    let mut line = Vec::with_capacity(pts.len());
    for (x, y) in &pts {
        line.push(egui::pos2(origin.x + x * scale, origin.y - y * scale));
    }
    if line.len() >= 2 {
        let clipped = painter.with_clip_rect(rect);
        for w in line.windows(2) {
            clipped.line_segment([w[0], w[1]], curve);
        }
    }

    // 表达式标签（左上角）。
    painter.text(
        egui::pos2(rect.left() + 8.0, rect.top() + 6.0),
        egui::Align2::LEFT_TOP,
        format!("f(x) = {label}"),
        egui::FontId::proportional(13.0),
        egui::Color32::from_rgb(255, 160, 80),
    );
}

/// 单个函数绘图叠加层实例（宿主层跟踪，与 `ShapeInstance` 同构）。
///
/// 一个实例 = 一个坐标系 + 一条表达式曲线，整体作为一个矩形区域存在，
/// 可拖动移动（RectInteraction）与删除，纳入 Undo 栈。为宿主叠加层，不进 ENBX
/// 文档序列化（与形状/图片/音频/视频同构）。
#[derive(Clone)]
pub(crate) struct FunctionPlotInstance {
    /// 用户看到的坐标系屏幕矩形；`None` 表示未移动过（用默认中心矩形）。
    user_rect: Option<egui::Rect>,
    /// 每单位长度像素（默认为 40px/单位 → 视界 ±10 单位、边长 800px）。
    scale: f32,
    /// 编译后的表达式（渲染采样用；宿主层绘制尚未接线时保留）。
    #[allow(dead_code)]
    expr: crate::function_parser::Expr,
    /// 原始表达式字符串（左下角标签显示用）。
    #[allow(dead_code)]
    expr_str: String,
    /// 插入序号（z-order）。
    z_index: u64,
    /// 所属页面索引：仅当「所属页 == 当前页」时渲染 / 命中。
    page: usize,
}

/// 毫秒 → `m:ss` 或 `h:mm:ss` 的时间文本（长视频超过 1 小时自动显示小时位）。
fn fmt_time(ms: u64) -> String {    let total = ms / 1000;
    let s = total % 60;
    let m = (total / 60) % 60;
    let h = total / 3600;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// 备课端通过「🎬 多媒体」插入的本地视频记录。
///
/// 模型层 `VideoElement` 只能通过 `resource_id`（内嵌资源 hex id）索引，且所在文档
/// 的 `PageContent.elements` 是 legacy `Vec<Element>`，无法容纳 `ElementData::Video`
/// （`to_legacy()` 对其返回 `None`）。受「不修改 drafftink-core」约束，视频元素不在
/// 文档层持久化，而是在宿主层用本结构记录，并据此在 `video_instances` 中重建叠加层。
/// `element` 字段保留完整的 `ElementData::Video`，便于将来文档层支持后直接落盘，
/// 叠加层重建时的 `is_loop` 等参数也直接从此读取。
#[derive(Clone)]
pub(crate) struct InsertedVideo {
    /// 唯一实例 key（= 元素 id）。
    id: String,
    /// 完整视频元素（模型层 `ElementData::Video`）。
    element: ElementData,
    /// 本地视频文件路径（`resource_id` 去掉 `file://` 前缀）。
    path: PathBuf,
    /// 画布世界坐标矩形（位置/尺寸）。
    world_rect: egui::Rect,
    /// 所属页面索引：该本地视频被插入时所在的页面，仅当「所属页 == 当前页」时
    /// 由 `sync_path_videos` 重建叠加层并参与渲染 / 命中检测。
    page: usize,
}

/// 备课端通过「🎵 音频」插入的本地音频记录（与 [`InsertedVideo`] 同构）。
///
/// 音频元素不进文档层（`to_legacy()` 对 `ElementData::Audio` 返回 `None`），
/// 在宿主层用本结构记录，并据此在 `audio_instances` 中建立可播放的控制条。
#[derive(Clone)]
pub(crate) struct InsertedAudio {
    /// 唯一实例 key（= 元素 id）。
    pub(crate) id: String,
    /// 本地音频文件路径（`resource_id` 去掉 `file://` 前缀）。
    pub(crate) path: PathBuf,
    /// 画布世界坐标矩形（控制条位置/尺寸）。
    pub(crate) world_rect: egui::Rect,
    /// 所属页面索引：仅当「所属页 == 当前页」时渲染 / 命中。
    pub(crate) page: usize,
}

impl IntegratedApp {
    pub fn new() -> Self {
        let shared: SharedContext = Arc::new(Mutex::new(SharedAppContext::default()));

        // 启动期创建并加载一次共享插件管理器（两模式复用，避免 cdylib 双加载）。
        let pm = load_shared_plugins();
        if let Ok(mut g) = shared.lock() {
            g.set_plugin_manager(Arc::new(Mutex::new(pm)));
        }

        let mut edit = EditApp::default();
        edit.set_shared(shared.clone());
        Self {
            mode: AppMode::Prepare,
            edit,
            display: None,
            shared,
            show_profiler: false,
            video_instances: std::collections::HashMap::new(),
            image_instances: std::collections::HashMap::new(),
            active_drag: None,
            pending_videos: Arc::new(Mutex::new(Vec::new())),
            pending_images: Arc::new(Mutex::new(Vec::new())),
            inserted_videos: Vec::new(),
            image_textures: std::collections::HashMap::new(),
            shape_instances: std::collections::HashMap::new(),
            audio_instances: std::collections::HashMap::new(),
            function_instances: std::collections::HashMap::new(),
            inserted_audios: Vec::new(),
            pending_audios: Arc::new(Mutex::new(Vec::new())),
            pending_audio: None,
            history: UndoHistory::new(),
            active_tool: ActiveTool::None,
            last_tool_click: None,
            drag_snapshot: None,
            text_edit_undo: None,
            pending_shape: None,
            pending_video: None,
            pending_image: None,
            pending_armed: false,
            shape_draw: None,
            magnifier: MagnifierTool::default(),
            name_picker: NamePickerTool::default(),
            selected_element_id: None,
            next_z_index: 0,
            last_page: 0,
            marquee_start: None,
            marquee_rect: None,
            function_menu: None,
        }
    }

    /// 内存 / 纹理 / 字体监控面板。
    ///
    /// egui 把这些诊断视图暴露为 `Context` 上的方法（`memory_ui` / `texture_ui` /
    /// `inspection_ui`），并没有「打开开关就自动显示」的 options 字段，需要自己
    /// 开窗调用。
    fn profiler_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_profiler;
        egui::Window::new("性能监控 (F12)")
            .open(&mut open)
            .default_width(420.0)
            .default_height(520.0)
            .vscroll(true)
            .show(ctx, |ui| {
                // 板书笔迹的绘制/剔除统计——判断四叉树剔除是否真的生效。
                if let Some(display) = self.display.as_ref() {
                    let r = &display.annotations.renderer;
                    ui.label(format!(
                        "板书笔迹: 总计 {} · 本帧绘制 {} · 剔除 {}",
                        display.annotations.strokes.len(),
                        r.last_drawn,
                        r.last_culled,
                    ));
                    ui.label(format!(
                        "四叉树已索引: {} 条",
                        display.annotations.spatial.len()
                    ));
                } else {
                    ui.label("板书统计：进入授课模式后可见");
                }
                ui.separator();

                // 视频 / 音频管线诊断：一眼看出是否有声、协商到什么格式、
                // 以及音视频漂移是否被控制在阈值内。
                if self.video_instances.is_empty() {
                    ui.label("视频叠加层：无实例（授课模式或按 V 加载测试视频）");
                } else {
                    for (key, inst) in &self.video_instances {
                        let Some(p) = inst.player.as_ref() else {
                            ui.label(format!("视频 {key}: 解码器不可用（占位）"));
                            continue;
                        };
                        ui.label(format!(
                            "视频 {key}: {} · {:.2} fps · base_scale {:.3}{}",
                            p.hwaccel.label(),
                            p.fps,
                            p.base_scale,
                            if p.paused { " · 已暂停" } else { "" }
                        ));
                        match p.audio_format() {
                            Some(f) => {
                                let drift = p.drift_ms().unwrap_or(0);
                                ui.label(format!(
                                    "  音频: {} Hz · {} ch · {:?}{} · 视频 {:.2}s / 音频 {:.2}s · 漂移 {:+} ms",
                                    f.sample_rate,
                                    f.channels,
                                    f.sample,
                                    if p.is_muted { " · 已静音" } else { "" },
                                    p.video_time().as_secs_f32(),
                                    p.audio_time().map(|t| t.as_secs_f32()).unwrap_or(0.0),
                                    drift,
                                ));
                            }
                            None => {
                                ui.label("  音频: 无（无音轨或设备不可用，已降级为静默播放）");
                            }
                        }
                    }
                }
                ui.separator();

                ui.collapsing("Memory（UI 状态占用）", |ui| {
                    ctx.memory_ui(ui);
                });
                ui.collapsing("Textures（纹理）", |ui| {
                    ctx.texture_ui(ui);
                });
                ui.collapsing("Inspection（形状/绘制统计）", |ui| {
                    ctx.inspection_ui(ui);
                });
            });
        self.show_profiler = open;
    }

    /// 进入授课模式：以当前备课课件构造 `DisplayApp` 并全屏。
    fn enter_teach(&mut self, ctx: &egui::Context) {
        // 先把备课端当前页的板书 / 编辑落盘到 doc，保证授课看到最新内容。
        self.edit.flush_to_doc();

        let shared = self.shared.clone();
        let doc = self.edit.doc.clone();
        let path = shared
            .lock()
            .map(|g| g.current_doc_path.clone())
            .unwrap_or(None)
            .map(|p| p.to_string_lossy().into_owned());

        // 复用共享插件管理器（不重新加载），避免双加载。
        let pm = shared.lock().ok().and_then(|g| g.plugin_manager.clone());
        let mut display = DisplayApp::new(doc, path.clone(), pm);
        display.set_shared(shared.clone());

        // 把备课端当前页已有批注加载进授课端（仅批注层）。
        let existing = self.edit.export_current_annotations();
        // 走 set_strokes 而非直接赋值，确保笔迹空间索引同步置脏。
        display.annotations.set_strokes(core_vec_to_ink(&existing));

        self.display = Some(display);
        self.mode = AppMode::Teach;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));

        // 解析源 .enbx 内嵌视频资源并启动解码器（叠加层）。
        self.collect_embedded_videos(&path, ctx);
        // 同时把备课端插入的本地文件视频（file://）挂回叠加层，避免授课时丢失。
        self.sync_path_videos(ctx);
    }

    /// 退出授课模式：把授课批注 / 小测、几何参数同步回备课端，再销毁授课窗口。
    ///
    /// 合规要点：批注只合并进 `annotations_data`（批注层），绝不修改 `elements`
    /// （学生原始作答快照）。
    fn exit_teach(&mut self, ctx: &egui::Context) {
        if let Some(display) = self.display.take() {
            let page = display.multi_page.current_page;

            // 1) 授课批注 → 中性格式 → 写入共享缓冲，供后续复盘 / 落盘。
            let core_strokes: Vec<CoreStroke> = ink_vec_to_core(&display.annotations.strokes);
            if let Ok(mut g) = self.shared.lock() {
                g.capture_teach_strokes(page, core_strokes.clone());
                g.doc = Some(display.doc.clone());
            }

            // 2) 几何 / 内容元素回写（仅内容层；不触碰批注层）。
            self.edit.sync_doc_elements_from(&display.doc);

            // 3) 批注合并进备课端当前页（仅批注层）。
            self.edit.import_current_annotations(core_strokes);
        }
        // 回到备课模式后，重新把本地文件视频挂回叠加层（enter_teach 会整表替换）。
        self.sync_path_videos(ctx);
        self.mode = AppMode::Prepare;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
    }

    /// 测试钩子（备课模式按 `L`）：打开文件对话框加载 `.enbx` 课件，将其元素映射为
    /// legacy `Element`（保留 `SvgShape`），注入 `edit.doc.elements`；若取消对话框，
    /// 则注入一组内置演示形状。两者均会把 `doc.page_size` 设为内容包围盒，使相机自动
    /// 适配（render_canvas_area 每帧按 page_size 重算 zoom/offset）。
    fn load_svg_shape_demo(&mut self, ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .add_filter("ENBX courseware", &["enbx"])
            .pick_file();

        let mut elements: Vec<Element> = Vec::new();

        match picked {
            Some(path) => match drafftink_enbx::parse_enbx(&path) {
                Ok(enbx_file) => {
                    for slide in &enbx_file.slides {
                        for enbx_elem in &slide.elements {
                            let ed: drafftink_core::ElementData = drafftink_enbx::map_element_from_enbx(enbx_elem);
                            if let Some(legacy) = ed.to_legacy() {
                                elements.push(legacy);
                            }
                        }
                    }
                    let svg_count = elements
                        .iter()
                        .filter(|e| matches!(e, Element::SvgShape(_)))
                        .count();
                    log::info!(
                        "[desktop] Loaded {} elements from {:?} ({} SvgShape)",
                        elements.len(),
                        path,
                        svg_count
                    );
                }
                Err(e) => {
                    log::error!("[desktop] Failed to parse ENBX {path:?}: {e}");
                }
            },
            None => {
                elements = demo_svg_shapes();
                log::info!(
                    "[desktop] No file chosen — injected {} demo SvgShapes",
                    elements.len()
                );
            }
        }

        if elements.is_empty() {
            return;
        }

        // 用内容包围盒设置页尺寸，使相机自动适配可视区域。
        let mut max_x = 1.0_f32;
        let mut max_y = 1.0_f32;
        for e in &elements {
            let b = e.base();
            max_x = max_x.max(b.position[0] + b.size[0]);
            max_y = max_y.max(b.position[1] + b.size[1]);
        }
        self.edit.doc.elements = elements;
        self.edit.doc.pages.clear();
        self.edit.doc.page_size = [max_x, max_y];
        // render_canvas_area 每帧按 page_size 重算相机 zoom/offset，无需手动设置。
        ctx.request_repaint();
    }

    // ── 视频叠加层 ────────────────────────────────────────────────────────

    /// 弹出系统文件选择框（后台线程，避免阻塞 UI），用户选定视频后将路径暂存，
    /// 由 `consume_pending_videos` 在下一帧主循环取出并插入当前幻灯片。
    ///
    /// 顶边栏「🎬 多媒体」按钮与 V 键共用此入口，保证两条路径行为一致。
    fn request_video_pick(&mut self, ctx: &egui::Context) {
        let sink = self.pending_videos.clone();
        let ctx = ctx.clone();
        // egui 0.29 的 Context 可跨线程 clone，`request_repaint` 从任一线程调用均安全。
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("选择视频文件")
                .add_filter(
                    "视频文件",
                    &["mp4", "mkv", "mov", "avi", "webm", "flv", "wmv"],
                )
                .pick_file();
            if let Some(p) = picked {
                sink.lock().unwrap().push(p);
                ctx.request_repaint();
            }
        });
    }

    /// 主循环每帧消费待插入的视频路径：构造 `ElementData::Video` 塞入当前页，
    /// 并立即启动解码叠加层（自动播放）。
    fn consume_pending_videos(&mut self, ctx: &egui::Context) {
        let pending: Vec<PathBuf> = std::mem::take(&mut *self.pending_videos.lock().unwrap());
        for path in pending {
            if self.pending_video.is_none() {
                // 进入「点击放置」模式：幽灵视频框跟随光标，单击画布固定到该处（Esc 取消）。
                self.pending_video = Some(path);
                self.pending_armed = false;
            } else {
                // 已有待放置视频（理论多选路径）：其余直接落在画布中心。
                self.insert_video_from_path(&path, ctx);
            }
        }
    }

    /// 把选定视频文件插到当前幻灯片画布中心，并启动叠加层播放。
    ///
    /// 模型层 `VideoElement` 没有 `video_path` 字段（只有 `resource_id`），且本任务
    /// 受约束不能修改 `drafftink-core`；因此把绝对路径以 `file://` 前缀写入
    /// `resource_id`，叠加层据此直接打开本地文件解码（与 enbx 内嵌资源的 hex id 不冲突）。
    ///
    /// 文档层 `PageContent.elements` 为 legacy `Vec<Element>`，无法承载 `ElementData::Video`
    /// （`to_legacy()` 对其返回 `None`），故视频元素记录在宿主层的 `inserted_videos`，
    /// 并由本方法同步在 `video_instances` 中建立可播放的叠加层实例。
    /// 在指定世界坐标中心放置一个视频叠加层实例（供「点击放置」流程调用）。
    fn insert_video_at(
        &mut self,
        path: &std::path::Path,
        world_center: [f32; 2],
        ctx: &egui::Context,
    ) {
        use drafftink_core::model::BaseElement;

        // 1) 计算默认世界尺寸（640×360，且不超过画布宽 60%）。
        let cam = &self.edit.camera;
        let canvas_world_w = cam.viewport[0] / cam.zoom;
        let mut w = 640.0_f32;
        let mut h = w * 9.0 / 16.0;
        let max_w = canvas_world_w * 0.6;
        if w > max_w {
            w = max_w;
            h = w * 9.0 / 16.0;
        }
        let center = world_center; // 由调用方给定（点击放置时为光标处的世界坐标）
        let base = BaseElement {
            id: Uuid::new_v4(),
            position: [center[0] - w / 2.0, center[1] - h / 2.0],
            size: [w, h],
            ..Default::default()
        };

        // 2) 构造视频元素，以 file:// 前缀路径写入 resource_id。
        let resource_id = format!("file://{}", path.display());
        let elem = ElementData::video(base, resource_id, false, true, 1.0, None);
        let id = ElementTrait::id(&elem).to_string();
        let world_rect = egui::Rect::from_min_size(
            egui::pos2(center[0] - w / 2.0, center[1] - h / 2.0),
            egui::vec2(w, h),
        );

        // 3) 在宿主层记录该视频元素（文档层无法容纳，故在此跟踪，供 teach 切换时重建）。
        let record = InsertedVideo {
            id: id.clone(),
            element: elem,
            path: path.to_path_buf(),
            world_rect,
            page: self.edit.multi_page.current_page,
        };
        self.inserted_videos.push(record.clone());
        // 撤销：插入视频。
        self.history.push(UndoCmd::InsertVideo { id: id.clone(), record });
        log::info!("[video] 插入视频元素: {} (id={})", path.display(), id);

        // 4) 立即启动解码叠加层，world_rect 锚定到画布中心。
        let player = VideoPlayer::new(path, false).ok();
        if player.is_none() {
            log::warn!(
                "[video] VideoPlayer 初始化失败: {}，将以红色占位呈现",
                path.display()
            );
        }
        let duration_ms = player.as_ref().map(|p| p.duration_ms()).unwrap_or(0);
        let z_index = self.next_z_index;
        self.next_z_index += 1;
        self.video_instances.insert(
            id,
            VideoInstance {
                world_rect: Some(world_rect),
                player,
                last_tex: None,
                user_rect: None,
                duration_ms,
                current_ms: 0,
                seeking: false,
                seek_target_ms: 0,
                z_index,
                page: self.edit.multi_page.current_page,
            },
        );
        ctx.request_repaint();
    }

    /// 兼容旧调用：把视频插到当前幻灯片画布中心（世界坐标 = 相机焦点）。
    fn insert_video_from_path(&mut self, path: &std::path::Path, ctx: &egui::Context) {
        let center = self.edit.camera.offset;
        self.insert_video_at(path, center, ctx);
    }

    /// 为 `inserted_videos` 中尚未建立叠加层的本地视频补建实例。
    ///
    /// 用于「进入授课 / 退出授课」时 `video_instances` 被整表替换后，把本地文件视频
    /// 重新挂回叠加层。幂等：已存在的 key 直接跳过，不会重复 spawn 解码进程。
    fn sync_path_videos(&mut self, ctx: &egui::Context) {
        for iv in &self.inserted_videos {
            if self.video_instances.contains_key(&iv.id) {
                continue;
            }
            let is_loop = match &iv.element {
                ElementData::Video(v) => v.is_loop,
                _ => false,
            };
            let player = VideoPlayer::new(&iv.path, is_loop).ok();
            if player.is_none() {
                log::warn!(
                    "[video] 重建叠加层失败: {}，将以红色占位呈现",
                    iv.path.display()
                );
            }
            let duration_ms = player.as_ref().map(|p| p.duration_ms()).unwrap_or(0);
            let z_index = self.next_z_index;
            self.next_z_index += 1;
            self.video_instances.insert(
                iv.id.clone(),
                VideoInstance {
                    world_rect: Some(iv.world_rect),
                    player,
                    last_tex: None,
                    user_rect: None,
                    duration_ms,
                    current_ms: 0,
                    seeking: false,
                    seek_target_ms: 0,
                    z_index,
                    page: iv.page,
                },
            );
            ctx.request_repaint();
        }
    }

    /// 进入授课时，解析源 `.enbx` 的内嵌视频资源并启动解码器。
    fn collect_embedded_videos(&mut self, enbx_path: &Option<String>, ctx: &egui::Context) {
        let Some(path) = enbx_path else { return; };
        let enbx = match drafftink_enbx::parse_enbx(std::path::Path::new(path)) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("[video] parse_enbx failed for embedded videos: {e}");
                return;
            }
        };
        let mut instances = std::collections::HashMap::new();
        for (slide_idx, slide) in enbx.slides.iter().enumerate() {
            for elem in &slide.elements {
                let ed: ElementData = drafftink_enbx::map_element_from_enbx(elem);
                if let ElementData::Video(v) = ed {
                    let world_rect = Some(egui::Rect::from_min_size(
                        egui::pos2(v.base.position[0], v.base.position[1]),
                        egui::vec2(v.base.size[0], v.base.size[1]),
                    ));
                    let player = enbx
                        .resources
                        .get(&v.resource_id)
                        .and_then(|bytes| make_temp_video_file(bytes, &v.resource_id))
                        .and_then(|p| VideoPlayer::new(&p, v.is_loop).ok());
                    if player.is_none() {
                        log::warn!(
                            "[video] VideoPlayer init failed for resource {}",
                            v.resource_id
                        );
                    } else {
                        log::info!(
                            "[video] embedded player started (backend: {}, base_scale={:.3}) resource={}",
                            player.as_ref().map(|p| p.hwaccel.label()).unwrap_or("?"),
                            player.as_ref().map(|p| p.base_scale).unwrap_or(1.0),
                            v.resource_id
                        );
                    }
                    let duration_ms = player.as_ref().map(|p| p.duration_ms()).unwrap_or(0);
                    let z_index = self.next_z_index;
                    self.next_z_index += 1;
                    instances.insert(
                        v.resource_id.clone(),
                        VideoInstance {
                            world_rect,
                            player,
                            last_tex: None,
                            user_rect: None,
                            duration_ms,
                            current_ms: 0,
                            seeking: false,
                            seek_target_ms: 0,
                            z_index,
                            page: slide_idx,
                        },
                    );
                }
            }
        }
        self.video_instances = instances;
        ctx.request_repaint();
    }

    // ── 图片叠加层 ────────────────────────────────────────────────────────

    /// 弹出系统文件选择框（后台线程，避免阻塞 UI），用户选定图片后将路径暂存，
    /// 由 `consume_pending_images` 在下一帧主循环取出并插入当前幻灯片。
    ///
    /// 顶边栏「🖼 图片」按钮与 I 键共用此入口，保证两条路径行为一致。
    fn request_image_pick(&mut self, ctx: &egui::Context) {
        let sink = self.pending_images.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("选择图片文件")
                .add_filter(
                    "图片文件",
                    &["png", "jpg", "jpeg", "gif", "bmp", "webp"],
                )
                .pick_file();
            if let Some(p) = picked {
                sink.lock().unwrap().push(p);
                ctx.request_repaint();
            }
        });
    }

    /// 主循环每帧消费待插入的图片路径：构造图片叠加层实例塞入当前页画布中心。
    fn consume_pending_images(&mut self, ctx: &egui::Context) {
        let pending: Vec<PathBuf> =
            std::mem::take(&mut *self.pending_images.lock().unwrap());
        for path in pending {
            if self.pending_image.is_none() {
                // 进入「点击放置」模式：幽灵图片框跟随光标，单击画布固定到该处（Esc 取消）。
                self.pending_image = Some(path);
                self.pending_armed = false;
            } else {
                // 已有待放置图片（理论多选路径）：其余直接落在画布中心。
                self.insert_image_from_path(&path, ctx);
            }
        }
    }

    /// 后台线程弹出音频文件选择框（不阻塞 UI）。
    fn request_audio_pick(&mut self, ctx: &egui::Context) {
        let sink = self.pending_audios.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("选择音频文件")
                .add_filter("音频文件", &["mp3", "wav", "m4a", "aac", "ogg", "flac", "wma"])
                .pick_file();
            if let Some(p) = picked {
                sink.lock().unwrap().push(p);
                ctx.request_repaint();
            }
        });
    }

    /// 主循环每帧消费待插入的音频路径：进入「点击放置」或直接落到画布中心。
    fn consume_pending_audios(&mut self, ctx: &egui::Context) {
        let pending: Vec<PathBuf> =
            std::mem::take(&mut *self.pending_audios.lock().unwrap());
        for path in pending {
            if self.pending_audio.is_none() {
                self.pending_audio = Some(path);
                self.pending_armed = false;
            } else {
                self.insert_audio_from_path(&path, ctx);
            }
        }
    }

    /// 在指定世界坐标中心放置一个音频叠加层实例（控制条）。
    fn insert_audio_at(
        &mut self,
        path: &std::path::Path,
        world_center: [f32; 2],
        ctx: &egui::Context,
    ) {
        // 控制条默认尺寸：300×48（世界单位），不超过画布宽 60%。
        let cam = &self.edit.camera;
        let canvas_world_w = cam.viewport[0] / cam.zoom;
        let mut w = 300.0_f32;
        let mut h = 48.0_f32;
        let max_w = canvas_world_w * 0.6;
        if w > max_w {
            let scale = max_w / w;
            w = max_w;
            h *= scale;
        }
        let world_rect = egui::Rect::from_min_size(
            egui::pos2(world_center[0] - w / 2.0, world_center[1] - h / 2.0),
            egui::vec2(w, h),
        );

        let id = format!("audio_{}", Uuid::new_v4());

        // 宿主层记录 + 建立可播放实例（音频元素仅宿主层跟踪，不进文档层）。
        let mut inst = AudioInstance::new(path, false);
        let duration_ms = inst.duration_ms;
        let z_index = self.next_z_index;
        self.next_z_index += 1;
        inst.world_rect = Some(world_rect);
        inst.z_index = z_index;
        inst.page = self.edit.multi_page.current_page;

        let record = InsertedAudio {
            id: id.clone(),
            path: path.to_path_buf(),
            world_rect,
            page: self.edit.multi_page.current_page,
        };
        self.inserted_audios.push(record.clone());
        self.audio_instances.insert(id.clone(), inst);

        // 撤销：插入。
        self.history.push(UndoCmd::InsertAudio { id, record });
        log::info!("[audio] 插入音频元素: {} (duration={}ms)", path.display(), duration_ms);
        ctx.request_repaint();
    }

    /// 把选定音频文件插到当前幻灯片画布中心。
    fn insert_audio_from_path(&mut self, path: &std::path::Path, ctx: &egui::Context) {
        let center = self.edit.camera.offset;
        self.insert_audio_at(path, center, ctx);
    }

    /// 把选定图片文件插到当前幻灯片画布中心，并建立宿主层叠加层实例。
    ///
    /// 与视频不同：`ImageElement` 的 `to_legacy()` 同样返回 `None`，文档层无法容纳，
    /// 故图片元素仅宿主层跟踪（不落盘、不进 `ElementData::Image` 持久化）；模型层已
    /// 有 `ElementData::Image`，此处不重复造轮子。叠加层由 `draw_image_overlay` 渲染。
    /// 在指定世界坐标中心放置一个图片叠加层实例（供「点击放置」流程调用）。
    fn insert_image_at(
        &mut self,
        path: &std::path::Path,
        world_center: [f32; 2],
        ctx: &egui::Context,
    ) {
        // 1) 计算默认世界尺寸（480×270，且不超过画布宽 60%）。
        let cam = &self.edit.camera;
        let canvas_world_w = cam.viewport[0] / cam.zoom;
        let mut w = 480.0_f32;
        let mut h = w * 9.0 / 16.0;
        let max_w = canvas_world_w * 0.6;
        if w > max_w {
            w = max_w;
            h = w * 9.0 / 16.0;
        }
        let center = world_center; // 由调用方给定（点击放置时为光标处的世界坐标）
        let world_rect = egui::Rect::from_min_size(
            egui::pos2(center[0] - w / 2.0, center[1] - h / 2.0),
            egui::vec2(w, h),
        );

        // 2) 生成稳定且唯一的实例 id。
        let id = format!("img_{}", Uuid::new_v4());
        log::info!("[image] 插入图片元素: {} (id={})", path.display(), id);

        let z_index = self.next_z_index;
        self.next_z_index += 1;
        let inst = ImageInstance {
            path: path.to_path_buf(),
            world_rect: Some(world_rect),
            user_rect: None,
            z_index,
            page: self.edit.multi_page.current_page,
        };
        self.image_instances.insert(id.clone(), inst.clone());
        // 撤销：插入图片。
        self.history.push(UndoCmd::InsertImage { id, inst });
        ctx.request_repaint();
    }

    /// 兼容旧调用：把图片插到当前幻灯片画布中心（世界坐标 = 相机焦点）。
    fn insert_image_from_path(&mut self, path: &std::path::Path, ctx: &egui::Context) {
        let center = self.edit.camera.offset;
        self.insert_image_at(path, center, ctx);
    }

    // ── ENBX 导出快照 ────────────────────────────────────────────────────────

    /// 收集当前画布的可序列化快照（纯数据），供 `save` 模块装配 ENBX。
    ///
    /// 放在 `app.rs` 内以便访问私有字段；不暴露字段为 `pub(crate)`。
    /// 文档层元素走 `ElementData::from_legacy` + `map_element_to_enbx`，叠加层实例
    /// 各自折算为对应 `Enbx*` 描述，资源以「页」为单位归类，保证翻页后不串页。
    pub(crate) fn save_bundle(&self) -> crate::save::SaveBundle {
        let doc = &self.edit.doc;
        let page_count = if doc.pages.is_empty() { 1 } else { doc.pages.len() };

        let mut pages = Vec::with_capacity(page_count);
        for p in 0..page_count {
            let doc_elements: Vec<Element> = if doc.pages.is_empty() {
                doc.elements.clone()
            } else {
                doc.pages[p].elements.clone()
            };

            let mut shapes = Vec::new();
            for inst in self.shape_instances.values() {
                if inst.page != p {
                    continue;
                }
                let rect = inst.user_rect.or(inst.world_rect);
                if let Some(r) = rect {
                    shapes.push(crate::save::ShapeDesc {
                        x: r.min.x as f64,
                        y: r.min.y as f64,
                        w: r.width() as f64,
                        h: r.height() as f64,
                        shape_type: Self::shape_kind_to_enbx(inst.kind),
                        fill: Self::color_tuple_to_argb(inst.fill_color),
                        stroke: Self::color_tuple_to_argb(Some(inst.stroke_color)),
                    });
                }
            }

            let mut images = Vec::new();
            for inst in self.image_instances.values() {
                if inst.page != p {
                    continue;
                }
                let rect = inst.user_rect.or(inst.world_rect);
                if let Some(r) = rect {
                    images.push(crate::save::ImageDesc {
                        x: r.min.x as f64,
                        y: r.min.y as f64,
                        w: r.width() as f64,
                        h: r.height() as f64,
                        path: inst.path.clone(),
                        opacity: 1.0,
                    });
                }
            }

            let mut videos = Vec::new();
            for iv in &self.inserted_videos {
                if iv.page != p {
                    continue;
                }
                let rect = self
                    .video_instances
                    .get(&iv.id)
                    .and_then(|v| v.user_rect.or(v.world_rect))
                    .or(Some(iv.world_rect));
                if let Some(r) = rect {
                    if let ElementData::Video(v) = &iv.element {
                        videos.push(crate::save::VideoDesc {
                            x: r.min.x as f64,
                            y: r.min.y as f64,
                            w: r.width() as f64,
                            h: r.height() as f64,
                            resource_id: v.resource_id.clone(),
                            is_loop: v.is_loop,
                            is_auto_play: v.is_auto_play,
                            volume: v.volume,
                        });
                    }
                }
            }

            // 5) 音频叠加层（`file://` 路径作为 resource_id；解析时 `embed_resource_id`
            //    会 strip 前缀并按 `md5(路径).扩展名` 写入 Resources/）。
            let mut audios = Vec::new();
            for ia in &self.inserted_audios {
                if ia.page != p {
                    continue;
                }
                let rect = self
                    .audio_instances
                    .get(&ia.id)
                    .and_then(|a| a.user_rect.or(a.world_rect))
                    .or(Some(ia.world_rect));
                if let Some(r) = rect {
                    let is_loop = self
                        .audio_instances
                        .get(&ia.id)
                        .map(|a| a.is_loop)
                        .unwrap_or(false);
                    let duration_ms = self
                        .audio_instances
                        .get(&ia.id)
                        .map(|a| a.duration_ms)
                        .unwrap_or(0);
                    audios.push(crate::save::AudioDesc {
                        x: r.min.x as f64,
                        y: r.min.y as f64,
                        w: r.width() as f64,
                        h: r.height() as f64,
                        resource_id: format!("file://{}", ia.path.display()),
                        is_loop,
                        duration_ms,
                    });
                }
            }

            pages.push(crate::save::PageElements {
                doc_elements,
                shapes,
                images,
                videos,
                audios,
            });
        }

        crate::save::SaveBundle {
            pages,
            page_size: doc.page_size,
            background: doc.background_color,
        }
    }

    /// 形状种类 → Seewo shape-type 字符串（与解析器 `shape_type_from_enbx` 对称）。
    pub(crate) fn shape_kind_to_enbx(k: ShapeKind) -> String {
        match k {
            ShapeKind::Circle => "ellipse",
            ShapeKind::Square | ShapeKind::Rectangle | ShapeKind::RoundedRect => "rectangle",
            ShapeKind::Parenthesis => "parenthesis",
            ShapeKind::Bracket => "bracket",
            ShapeKind::Brace => "brace",
            ShapeKind::Arrow | ShapeKind::DoubleArrow => "arrow",
            // 虚拟教具产物：近似映射到 Seewo 已知 shape-type（line/arc/fan）。
            ShapeKind::Line | ShapeKind::Angle => "line",
            ShapeKind::Arc => "arc",
            ShapeKind::Sector => "fan",
            ShapeKind::Polygon { .. } => "polygon",
            // 数轴降级为直线段保存（Seewo 无「数轴」原语；读取侧近似为 line）。
            ShapeKind::NumberLine(_) => "numberline",
        }
        .to_string()
    }

    /// `(r, g, b, a)` 元组 → `AARRGGBB` 字符串；`None` 表示无填充（空串）。
    pub(crate) fn color_tuple_to_argb(c: Option<(u8, u8, u8, u8)>) -> String {
        match c {
            Some((r, g, b, a)) => format!("{a:02X}{r:02X}{g:02X}{b:02X}"),
            None => String::new(),
        }
    }

    // ── 文本工具（Part B） ────────────────────────────────────────────────────

    /// 在画布中心插入一个默认文本框，并立即选中以便就地编辑。
    ///
    /// 文本以 `Element::Text` 落到文档层 `pages[current].elements`（与形状/图片/视频
    /// 叠加层不同，文本是持久化的文档元素），因此会被 `save_bundle` 一并序列化。
    fn insert_text_at(&mut self, ctx: &egui::Context) {
        use drafftink_core::model::{BaseElement, TextElement};

        let page = self.edit.multi_page.current_page;
        let center = self.edit.camera.offset;
        let id = Uuid::new_v4();
        let elem = Element::Text(TextElement {
            base: BaseElement {
                id,
                position: [center[0] - 200.0, center[1] - 50.0],
                size: [400.0, 100.0],
                fill_color: egui::Color32::from_rgb(0, 0, 0),
                ..Default::default()
            },
            text: "双击编辑文本".to_string(),
            font_size: 32.0,
            font_family: "Microsoft YaHei".to_string(),
        });
        let id_str = id.to_string();

        if self.edit.doc.pages.is_empty() {
            // 兼容 legacy 单页文档：落到 `doc.elements`。
            self.edit.doc.elements.push(elem.clone());
        } else {
            self.edit.doc.pages[page].elements.push(elem.clone());
        }

        // 撤销：插入文本。
        self.history.push(UndoCmd::InsertElement { page, elem });

        self.selected_element_id = Some(SelectedElement::Text(id_str));
        ctx.request_repaint();
    }

    /// 在内容之上绘制图片叠加层（与视频叠加层同构，但无解码器 / 无暂停静音按钮）。
    ///
    /// 交互复用同一套 `RectInteraction`（8 方向缩放 + 内部拖拽），与视频共享全局唯一
    /// 拖拽守卫，保证同一时刻仅一个矩形可被拖动。图片纹理经 `image` crate 懒加载并缓存
    /// 在 `image_textures`（路径 → 纹理句柄），避免每帧重新解码（符合零 panic 兜底约定：
    /// 解码/读取失败时绘制红色占位 + warn!，而非崩溃）。
    fn draw_image_overlay(&mut self, ctx: &egui::Context) {
        if self.image_instances.is_empty() && self.pending_image.is_none() {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("image_overlay"),
        ));
        // 备课模式需用 EditApp 相机 + 画布屏幕偏移；授课模式用 DisplayApp 相机（全屏）。
        let (cam, panel_offset) = if let Some(d) = self.display.as_ref() {
            (Some(d.camera.clone()), egui::Vec2::ZERO)
        } else {
            (
                Some(self.edit.camera.clone()),
                egui::vec2(self.edit.canvas_offset[0], self.edit.canvas_offset[1]),
            )
        };
        let screen = ctx.screen_rect();
        let prepare = self.mode == AppMode::Prepare;

        // ── 点击放置（pending）：选定图片文件后，半透明幽灵框跟随光标，
        //    单击画布任意处把图片固定到该位置（Esc 取消放置）。放置后视为「已固定」——
        //    不自动选中、不显示边框/抓手；单击图片才选中进入微调。
        if prepare {
            if let Some(path) = self.pending_image.clone() {
                let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                let pressed = ctx.input(|i| i.pointer.primary_pressed());
                let pointer = ctx.pointer_interact_pos();
                if esc {
                    self.pending_image = None;
                    self.pending_armed = false;
                } else {
                    if let Some(pos) = pointer {
                        // 幽灵预览：以光标为中心的半透明 16:9 矩形（尺寸 = 世界默认尺寸 × zoom）。
                        let cw = self.edit.camera.viewport[0] / self.edit.camera.zoom;
                        let mut gw = 480.0_f32;
                        let mut gh = gw * 9.0 / 16.0;
                        let max_w = cw * 0.6;
                        if gw > max_w {
                            gw = max_w;
                            gh = gw * 9.0 / 16.0;
                        }
                        let zoom = cam.as_ref().map(|c| c.zoom).unwrap_or(1.0);
                        let ghost_rect = egui::Rect::from_center_size(
                            pos,
                            egui::vec2(gw * zoom, gh * zoom),
                        );
                        painter.rect_filled(
                            ghost_rect,
                            4.0,
                            egui::Color32::from_rgba_unmultiplied(0, 150, 255, 40),
                        );
                        let gcol = egui::Color32::from_rgba_unmultiplied(0, 150, 255, 220);
                        let gwb = 2.0;
                        painter.line_segment(
                            [ghost_rect.left_top(), ghost_rect.right_top()],
                            egui::Stroke::new(gwb, gcol),
                        );
                        painter.line_segment(
                            [ghost_rect.right_top(), ghost_rect.right_bottom()],
                            egui::Stroke::new(gwb, gcol),
                        );
                        painter.line_segment(
                            [ghost_rect.right_bottom(), ghost_rect.left_bottom()],
                            egui::Stroke::new(gwb, gcol),
                        );
                        painter.line_segment(
                            [ghost_rect.left_bottom(), ghost_rect.left_top()],
                            egui::Stroke::new(gwb, gcol),
                        );
                        painter.text(
                            pos + egui::vec2(0.0, gh * zoom / 2.0 + 6.0),
                            egui::Align2::CENTER_TOP,
                            "点击放置图片",
                            egui::FontId::proportional(12.0),
                            egui::Color32::WHITE,
                        );
                    }
                    if self.pending_armed && pressed {
                        if let Some(pos) = pointer {
                            let local = pos - panel_offset;
                            let world = cam
                                .as_ref()
                                .map(|c| c.screen_to_world(local))
                                .unwrap_or([pos.x, pos.y]);
                            self.pending_image = None;
                            self.pending_armed = false;
                            self.insert_image_at(&path, world, ctx);
                            // 刚放置的图片以「固定」状态呈现：清空选中，避免边框/抓手
                            // 立即挡住光标或遮挡图片，影响老师插入下一个素材。
                            self.selected_element_id = None;
                        }
                    } else if !self.pending_armed {
                        // 跳过文件对话框那一下点击，下一帧起才允许放置。
                        self.pending_armed = true;
                    }
                }
            }
        }

        let keys: Vec<String> = self.image_instances.keys().cloned().collect();
        let cur_page = self.current_canvas_page();

        // 1) 先计算每个图片的屏幕矩形（用于选中 / 取消选中判定）。
        //    仅收集「所属页 == 当前页」的图片，翻页后其它页的图片不再渲染 / 命中。
        let mut rects: Vec<(String, egui::Rect, u64)> = Vec::with_capacity(keys.len());
        for key in &keys {
            let inst = match self.image_instances.get(key) {
                Some(i) => i,
                None => continue,
            };
            if inst.page != cur_page {
                continue;
            }
            let base_rect = if let Some(wr) = inst.world_rect {
                match &cam {
                    Some(c) => {
                        let tl = c.world_to_screen([wr.min.x, wr.min.y]) + panel_offset;
                        let br = c.world_to_screen([wr.max.x, wr.max.y]) + panel_offset;
                        egui::Rect::from_two_pos(tl, br)
                    }
                    None => default_overlay_rect(screen),
                }
            } else {
                default_overlay_rect(screen)
            };
            let rect = inst.user_rect.unwrap_or(base_rect);
            rects.push((key.clone(), rect, inst.z_index));
        }
        // 按 z_index 升序排序：后插入的（值大）后绘制，渲染在上层。
        rects.sort_by_key(|(_, _, z)| *z);

        // 2) 选中 / 取消选中已统一由 `handle_canvas_click`（全局单击处理器）负责：
        //    单击图片矩形 → 选中（显示边框 + 抓手供微调）；单击空白处 → 取消选中。
        //    此处不再内联判定，避免三套叠加层各自维护选中态、相互覆盖。

        // 3) 逐实例绘制。未选中 → 仅渲染图片（无边框/抓手）；选中 → 边框/抓手 + 拖拽缩放。
        for (key, rect, _z) in rects {
            let inst = match self.image_instances.get_mut(&key) {
                Some(i) => i,
                None => continue,
            };

            // 2) 懒加载纹理：首次按路径解码并缓存到 image_textures，后续直接复用。
            //    先 `contains_key` 取 bool，避免 `get` 的不可变借用跨过 `insert` 造成借用冲突。
            let exists = self.image_textures.contains_key(&inst.path);
            let tex: Option<egui::TextureHandle> = if exists {
                Some(self.image_textures.get(&inst.path).unwrap().clone())
            } else {
                let loaded = match std::fs::read(&inst.path) {
                    Ok(bytes) => match image::load_from_memory(&bytes) {
                        Ok(img) => {
                            let img = img.to_rgba8();
                            let (iw, ih) = (img.width(), img.height());
                            let color_img = egui::ColorImage::from_rgba_unmultiplied(
                                [iw as usize, ih as usize],
                                &img.into_raw(),
                            );
                            Some(ctx.load_texture(
                                format!("image_{key}"),
                                color_img,
                                egui::TextureOptions::default(),
                            ))
                        }
                        Err(e) => {
                            log::warn!("[image] 解码失败: {} — {:?}", inst.path.display(), e);
                            None
                        }
                    },
                    Err(e) => {
                        log::warn!(
                            "[image] 读取文件失败: {} — {:?}",
                            inst.path.display(),
                            e
                        );
                        None
                    }
                };
                if let Some(t) = loaded {
                    self.image_textures.insert(inst.path.clone(), t.clone());
                    Some(t)
                } else {
                    None
                }
            };

            // 4) 绘制纹理（解码失败则红色占位，符合零 panic 兜底约定）。
            //    未选中 → 直接用外层 rect；选中 → 先让 RectInteraction 解析拖拽/缩放，
            //    用更新后的 rect 绘制，使纹理与边框/抓手对齐。
            let selected = self.selected_element_id == Some(SelectedElement::Image(key.clone()));
            let (draw_rect, interact_opt) = if prepare && selected {
                let iid = egui::Id::new(format!("image_{key}"));
                let (r, interact) = Self::overlay_rect_drag(
                    &mut self.active_drag,
                    &mut self.drag_snapshot,
                    &mut self.history,
                    SelectedElement::Image(key.clone()),
                    iid,
                    rect,
                    &mut inst.user_rect,
                    ctx,
                );
                (r, Some(interact))
            } else {
                (rect, None)
            };
            match &tex {
                Some(t) => {
                    painter.image(
                        t.id(),
                        draw_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                None => {
                    painter.rect_filled(draw_rect, 0.0, egui::Color32::RED);
                    log::warn!("[image] texture unavailable for {key} — placeholder shown");
                }
            }
            // 仅选中态：边框 + 角 grip + 边高亮（悬停/拖拽变亮蓝），绘制在纹理之上。
            if let Some(interact) = interact_opt {
                interact.draw_overlay(&painter);
            }
        }

        ctx.request_repaint();
    }

    // ── 形状叠加层 ────────────────────────────────────────────────────────

    /// 在画布中心插入一个默认大小的形状叠加层实例（备课端顶边栏「➕ 插入」触发）。
    ///
    /// 形状与图片/视频同构：仅宿主层跟踪（不进文档、不落盘），由 `draw_shape_overlay`
    /// 渲染。样式默认：线宽 3.0、黑色描边、无填充；方形类默认 200×200，
    /// 长方形/箭头/括号类默认 200×100（与需求一致）。
    /// 在指定世界坐标中心放置一个默认大小的形状叠加层实例。
    ///
    /// 供「点击放置」流程调用：幽灵形状跟随光标，单击画布时把光标处屏幕坐标经相机
    /// 反变换为 `world_center` 后调用本方法固定形状。
    #[allow(dead_code)] // 仍被测试（save_enbx_creates_valid_zip 等）与旧放置路径引用
    fn insert_shape_at(
        &mut self,
        kind: drafftink_core::model::ShapeKind,
        world_center: [f32; 2],
        ctx: &egui::Context,
    ) {
        let cam = &self.edit.camera;
        let canvas_world_w = cam.viewport[0] / cam.zoom;
        // 长方形/箭头/括号类默认扁一些，方形类默认正方。
        let (mut w, mut h) = match kind {
            drafftink_core::model::ShapeKind::Parenthesis
            | drafftink_core::model::ShapeKind::Bracket
            | drafftink_core::model::ShapeKind::Brace
            | drafftink_core::model::ShapeKind::Arrow
            | drafftink_core::model::ShapeKind::DoubleArrow => (200.0, 100.0),
            _ => (200.0, 200.0),
        };
        let max_w = canvas_world_w * 0.6;
        if w > max_w {
            let scale = max_w / w;
            w = max_w;
            h *= scale;
        }
        let world_rect = egui::Rect::from_min_size(
            egui::pos2(world_center[0] - w / 2.0, world_center[1] - h / 2.0),
            egui::vec2(w, h),
        );

        let id = format!("shape_{}", Uuid::new_v4());
        log::info!("[shape] 插入形状叠加层: {kind:?} (id={id})");

        let z_index = self.next_z_index;
        self.next_z_index += 1;
        let inst = ShapeInstance {
            kind,
            world_rect: Some(world_rect),
            user_rect: None,
            stroke_width: 3.0,
            stroke_color: (0, 0, 0, 255),
            fill_color: None,
            arc_degrees: None,
            line_flipped: false,
            z_index,
            page: self.edit.multi_page.current_page,
        };
        self.shape_instances.insert(id.clone(), inst.clone());
        // 撤销：插入形状。
        self.history.push(UndoCmd::InsertShape { id, inst });
        ctx.request_repaint();
    }

    // ── 统一选中 / 命中检测 ────────────────────────────────────────────────

    /// 把一个**屏幕空间**矩形映射为**画布世界**矩形（供拖拽绘制提交用）。
    ///
    /// 屏幕坐标（`Pos2`，来自 `PointerState`）先扣除画布偏移 `canvas_offset`，
    /// 再经相机 `screen_to_world` 转世界坐标；Y 轴翻转由相机负责。若授课/无相机会退，
    /// 则退化为「屏幕坐标即世界坐标」（全屏授课时二者一致）。
    fn screen_rect_to_world_rect(&self, screen_rect: egui::Rect) -> egui::Rect {
        let (cam, panel_offset) = if let Some(d) = self.display.as_ref() {
            (Some(d.camera.clone()), egui::Vec2::ZERO)
        } else {
            (
                Some(self.edit.camera.clone()),
                egui::vec2(self.edit.canvas_offset[0], self.edit.canvas_offset[1]),
            )
        };
        let to_world = |p: egui::Pos2| -> [f32; 2] {
            let local = p - panel_offset;
            cam.as_ref()
                .map(|c| c.screen_to_world(local))
                .unwrap_or([local.x, local.y])
        };
        let wmin = to_world(screen_rect.min);
        let wmax = to_world(screen_rect.max);
        egui::Rect::from_two_pos(egui::pos2(wmin[0], wmin[1]), egui::pos2(wmax[0], wmax[1]))
    }

    /// 当前画布所处的页面索引：备课模式取 `EditApp` 的当前页，授课模式取 `DisplayApp`
    /// 的当前页（从 `display.multi_page.current_page` 读取）。叠加层渲染 / 命中检测
    /// 只用「所属页 == 当前页」的实例，保证翻页后不会残留其它页的内容。
    fn current_canvas_page(&self) -> usize {
        match self.mode {
            AppMode::Prepare => self.edit.multi_page.current_page,
            AppMode::Teach => self
                .display
                .as_ref()
                .map_or(0, |d| d.multi_page.current_page),
        }
    }

    /// 视频进度条的可点击命中带（bar 位于视频矩形下方 8px、高 6px，外扩 8px），
    /// 与 `draw_video_overlay` 绘制 / 拖拽进度条的命中区保持一致，确保「点进度条」
    /// 不会误判为「点空白处取消选中」。
    fn video_seek_hit_rect(video_rect: egui::Rect) -> egui::Rect {
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(video_rect.left(), video_rect.bottom() + 8.0),
            egui::vec2(video_rect.width(), 6.0),
        );
        bar_rect.expand(8.0)
    }

    /// 命中检测：给定画布屏幕坐标指针位置，返回其下**所有**命中的叠加层元素，
    /// 按 `z_index` 降序排列（最上层在前）。
    ///
    /// **关键约束（对应「修正 RectInteraction 检测范围」）**：只使用元素的 `user_rect`
    /// 或 `world_rect` **逻辑矩形**进行判定，绝不把边框宽度（2px）或角 grip（8px）
    /// 算进检测矩形。边框 / 抓手纯属被动渲染——鼠标碰到它们应触发缩放，而永远不应
    /// 阻挡对底层图形像素的点击。
    fn find_all_elements_at(
        &self,
        mouse_pos: egui::Pos2,
        screen: egui::Rect,
    ) -> Vec<(SelectedElement, u64)> {
        let (cam, panel_offset) = if let Some(d) = self.display.as_ref() {
            (Some(d.camera.clone()), egui::Vec2::ZERO)
        } else {
            (
                Some(self.edit.camera.clone()),
                egui::vec2(self.edit.canvas_offset[0], self.edit.canvas_offset[1]),
            )
        };

        let screen_rect_of =
            |world_rect: Option<egui::Rect>, user_rect: Option<egui::Rect>| -> egui::Rect {
                let base = if let Some(wr) = world_rect {
                    match &cam {
                        Some(c) => {
                            let tl = c.world_to_screen([wr.min.x, wr.min.y]) + panel_offset;
                            let br = c.world_to_screen([wr.max.x, wr.max.y]) + panel_offset;
                            egui::Rect::from_two_pos(tl, br)
                        }
                        None => default_overlay_rect(screen),
                    }
                } else {
                    default_overlay_rect(screen)
                };
                user_rect.unwrap_or(base)
            };

        // 收集所有命中项。后插入的（上层）元素 z_index 更大，排在前面，
        // 以便普通单击选中顶层；同时保留完整栈，供 Ctrl/Cmd+单击 向下穿透选中。
        let mut hits: Vec<(SelectedElement, u64)> = Vec::new();
        let cur_page = self.current_canvas_page();

        // 图片：仅逻辑矩形命中，且仅当所属页 == 当前页。
        for (k, inst) in &self.image_instances {
            if inst.page != cur_page {
                continue;
            }
            if screen_rect_of(inst.world_rect, inst.user_rect).contains(mouse_pos) {
                hits.push((SelectedElement::Image(k.clone()), inst.z_index));
            }
        }
        // 视频：逻辑矩形 + 进度条命中带（有时长时），且仅当所属页 == 当前页。
        for (k, inst) in &self.video_instances {
            if inst.page != cur_page {
                continue;
            }
            let r = screen_rect_of(inst.world_rect, inst.user_rect);
            let mut hit_region = r;
            if inst.duration_ms > 0 {
                hit_region = hit_region.union(Self::video_seek_hit_rect(r));
            }
            if hit_region.contains(mouse_pos) {
                hits.push((SelectedElement::Video(k.clone()), inst.z_index));
            }
        }
        // 形状：仅逻辑矩形命中，且仅当所属页 == 当前页。
        for (k, inst) in &self.shape_instances {
            if inst.page != cur_page {
                continue;
            }
            if screen_rect_of(inst.world_rect, inst.user_rect).contains(mouse_pos) {
                hits.push((SelectedElement::Shape(k.clone()), inst.z_index));
            }
        }
        // 音频：控制条逻辑矩形命中，且仅当所属页 == 当前页。
        for (k, inst) in &self.audio_instances {
            if inst.page != cur_page {
                continue;
            }
            if screen_rect_of(inst.world_rect, inst.user_rect).contains(mouse_pos) {
                hits.push((SelectedElement::Audio(k.clone()), inst.z_index));
            }
        }
        // 函数绘图：坐标系矩形命中，且仅当所属页 == 当前页。
        for (k, inst) in &self.function_instances {
            if inst.page != cur_page {
                continue;
            }
            let rect = inst
                .user_rect
                .unwrap_or_else(|| function_default_rect(screen, inst.scale));
            if rect.contains(mouse_pos) {
                hits.push((SelectedElement::Function(k.clone()), inst.z_index));
            }
        }

        // 文本（文档层）：仅逻辑矩形命中，仅当前页；选中后由 `draw_text_overlay` 就地编辑。
        let page_elements: Option<&Vec<Element>> = if self.edit.doc.pages.is_empty() {
            Some(&self.edit.doc.elements)
        } else {
            self.edit.doc.pages.get(cur_page).map(|p| &p.elements)
        };
        if let Some(elems) = page_elements {
            for e in elems {
                if let Element::Text(t) = e {
                    let wr = egui::Rect::from_min_size(
                        egui::pos2(t.base.position[0], t.base.position[1]),
                        egui::vec2(t.base.size[0], t.base.size[1]),
                    );
                    if screen_rect_of(Some(wr), None).contains(mouse_pos) {
                        hits.push((
                            SelectedElement::Text(t.base.id.to_string()),
                            t.base.z_order as u64,
                        ));
                    }
                }
            }
        }

        hits.sort_by_key(|(_, z)| std::cmp::Reverse(*z));
        hits
    }

    /// 全局画布单击处理器（左键 `primary_pressed` 触发），统一「先隐身后选中」交互：
    ///
    /// 1. 普通单击：命中某元素 → 选中最上层；命中空白处 → 取消全选（即「固定」操作）。
    /// 2. 按住 Ctrl（Windows/Linux）或 Cmd（macOS）单击：在重叠元素之间循环向下
    ///    穿透选中，解决「大矩形盖住内部小圆后无法选中小圆」的问题。
    ///
    /// 仅在备课模式、非放置态、且当前无进行中拖拽时生效。`None` 表示单击空白处，
    /// 这是统一的「取消选中」交互范式（不再绑定 Escape 取消选中）。
    fn handle_canvas_click(
        &mut self,
        mouse_pos: egui::Pos2,
        screen: egui::Rect,
        modifiers: &egui::Modifiers,
    ) {
        if self.mode != AppMode::Prepare {
            return;
        }
        if self.pending_shape.is_some()
            || self.pending_video.is_some()
            || self.pending_image.is_some()
        {
            return;
        }
        if self.active_drag.is_some() {
            return;
        }

        let hits = self.find_all_elements_at(mouse_pos, screen);
        if hits.is_empty() {
            self.selected_element_id = None;
            return;
        }

        if modifiers.command {
            // 循环穿透：若当前选中项仍在命中栈中，则选中它的下一层；否则回到最上层。
            if let Some(current) = self.selected_element_id.as_ref() {
                if let Some(pos) = hits.iter().position(|(id, _)| id == current) {
                    let next = (pos + 1) % hits.len();
                    self.selected_element_id = Some(hits[next].0.clone());
                    return;
                }
            }
        }

        // 默认：选中最上层。
        self.selected_element_id = Some(hits[0].0.clone());
    }

    // ── Delete 键删除（Part C） ─────────────────────────────────────────────────

    /// 删除当前选中的元素，并按类型分发清理各宿主层叠加层 / 文档层。
    ///
    /// - 视频：`remove` 触发 `VideoInstance` 析构 → `VideoPlayer::Drop` 杀掉 ffmpeg
    ///   子进程并停 cpal 音频流（零 panic 兜底已内置于 `video_player.rs`）。
    /// - 图片：移除 `image_instances` 并释放 `image_textures` 中缓存的纹理。
    /// - 形状：移除 `shape_instances`。
    /// - 文本：从文档层 `pages[current].elements`（或 legacy `doc.elements`）移除。
    ///
    /// 调用方需保证文本编辑框未聚焦（否则 Backspace/Delete 会误删正在输入的字符）。
    /// 无论删除是否发生，都清空全局拖拽守卫 `active_drag`。
    fn delete_selected(&mut self, ctx: &egui::Context) {
        let Some(sel) = self.selected_element_id.take() else {
            return;
        };
        // 删除前先提交未决的文本内容编辑（若刚编辑完文本就 Delete，内容修改也要入栈）。
        self.commit_text_edit();
        match sel {
            SelectedElement::Video(id) => {
                // 记录撤销（删除）：先取出记录，移除实例（触发 VideoPlayer::Drop 杀进程）。
                if let Some(idx) = self.inserted_videos.iter().position(|v| v.id == id) {
                    let record = self.inserted_videos.remove(idx);
                    self.video_instances.remove(&id);
                    self.history.push(UndoCmd::RemoveVideo { id, record });
                } else {
                    self.video_instances.remove(&id);
                }
            }
            SelectedElement::Image(id) => {
                if let Some(inst) = self.image_instances.remove(&id) {
                    self.image_textures.remove(&inst.path);
                    self.history.push(UndoCmd::RemoveImage { id, inst });
                }
            }
            SelectedElement::Shape(id) => {
                if let Some(inst) = self.shape_instances.remove(&id) {
                    self.history.push(UndoCmd::RemoveShape { id, inst });
                }
            }
            SelectedElement::Audio(id) => {
                // 记录撤销（删除）：移除实例触发 AudioPipeline::Drop 杀 ffmpeg + 停 cpal。
                if let Some(idx) = self.inserted_audios.iter().position(|a| a.id == id) {
                    let record = self.inserted_audios.remove(idx);
                    self.audio_instances.remove(&id);
                    self.history.push(UndoCmd::RemoveAudio { id, record });
                } else {
                    self.audio_instances.remove(&id);
                }
            }
            SelectedElement::Function(id) => {
                if let Some(inst) = self.function_instances.remove(&id) {
                    self.history.push(UndoCmd::RemoveFunction { id, inst });
                }
            }
            SelectedElement::Text(id) => {
                let page = self.edit.multi_page.current_page;
                let elems = if self.edit.doc.pages.is_empty() {
                    &mut self.edit.doc.elements
                } else if let Some(p) = self.edit.doc.pages.get_mut(page) {
                    &mut p.elements
                } else {
                    self.active_drag = None;
                    ctx.request_repaint();
                    return;
                };
                if let Some(index) = elems.iter().position(|e| e.base().id.to_string() == id) {
                    let elem = elems.remove(index);
                    self.history.push(UndoCmd::RemoveElement { page, index, elem });
                }
            }
        }
        self.active_drag = None;
        ctx.request_repaint();
    }

    // ── Undo / Redo ─────────────────────────────────────────────────────────

    /// 宿主叠加层的拖拽（移动 / 缩放）公共入口：调用 `RectInteraction::update`、
    /// 写回 `user_rect`，并在拖拽开始 / 结束帧分别快照旧几何、push `ModifyRect`。
    ///
    /// 设计为**关联函数**（分别借用 `active_drag` / `drag_snapshot` / `history` 三个
    /// 字段），而非 `&mut self` 方法——调用方此刻正持有 `inst`（借用 `*_instances`），
    /// 字段级分离借用可让二者共存，避免整体 `&mut self` 与 `&mut inst` 冲突。
    ///
    /// 返回 `(更新后的绘制矩形, 交互状态)`，调用方据此绘制纹理并叠加边框/抓手。
    #[allow(clippy::too_many_arguments)]
    fn overlay_rect_drag(
        active_drag: &mut Option<(egui::Id, HitZone)>,
        drag_snapshot: &mut Option<DragSnapshot>,
        history: &mut UndoHistory,
        sel: SelectedElement,
        id: egui::Id,
        rect: egui::Rect,
        user_rect: &mut Option<egui::Rect>,
        ctx: &egui::Context,
    ) -> (egui::Rect, RectInteraction) {
        let old = *user_rect;
        let was_active = active_drag.is_some();
        let mut interact = RectInteraction::new(id, rect);
        let new_rect = interact.update(ctx, active_drag);
        let r = if let Some(nr) = new_rect {
            *user_rect = Some(nr);
            nr
        } else {
            rect
        };
        // 拖拽开始：守卫由空闲 → 本实例认领，快照旧几何（旧 user_rect）。
        if !was_active && active_drag.is_some() {
            *drag_snapshot = Some(DragSnapshot::Overlay {
                sel,
                old_rect: old,
            });
        }
        // 拖拽结束：守卫释放，若几何确有变化则 push ModifyRect（撤销 = 回旧矩形）。
        if was_active && active_drag.is_none() {
            if let Some(DragSnapshot::Overlay { sel, old_rect }) = drag_snapshot.take() {
                if old_rect != *user_rect {
                    history.push(UndoCmd::ModifyRect {
                        sel,
                        old_rect,
                        new_rect: *user_rect,
                    });
                }
            }
        }
        (r, interact)
    }

    /// 按页面/索引克隆文档层元素（文本拖拽快照与比较用）。
    fn clone_text_elem(&self, page: usize, idx: usize) -> Element {
        if self.edit.doc.pages.is_empty() {
            self.edit.doc.elements[idx].clone()
        } else {
            self.edit.doc.pages[page].elements[idx].clone()
        }
    }

    /// 按元素 id 查找文档层元素（跨 `elements` / `pages[page]`）。
    fn find_text_elem(&self, page: usize, elem_id: Uuid) -> Option<Element> {
        if self.edit.doc.pages.is_empty() {
            self.edit.doc.elements.iter().find(|e| e.id() == elem_id).cloned()
        } else {
            self.edit
                .doc
                .pages
                .get(page)
                .and_then(|p| p.elements.iter().find(|e| e.id() == elem_id).cloned())
        }
    }

    /// 按元素 id 替换文档层元素（`ModifyText` 的撤销/重做）。
    fn replace_text_elem(&mut self, page: usize, elem_id: Uuid, elem: Element) {
        if self.edit.doc.pages.is_empty() {
            for e in &mut self.edit.doc.elements {
                if e.id() == elem_id {
                    *e = elem.clone();
                }
            }
        } else if let Some(p) = self.edit.doc.pages.get_mut(page) {
            for e in &mut p.elements {
                if e.id() == elem_id {
                    *e = elem.clone();
                }
            }
        }
    }

    /// 按选中元素设置叠加层 `user_rect`（`ModifyRect` 的撤销/重做）。
    fn set_overlay_user_rect(&mut self, sel: &SelectedElement, rect: Option<egui::Rect>) {
        match sel {
            SelectedElement::Shape(id) => {
                if let Some(i) = self.shape_instances.get_mut(id) {
                    i.user_rect = rect;
                }
            }
            SelectedElement::Image(id) => {
                if let Some(i) = self.image_instances.get_mut(id) {
                    i.user_rect = rect;
                }
            }
            SelectedElement::Video(id) => {
                if let Some(i) = self.video_instances.get_mut(id) {
                    i.user_rect = rect;
                }
            }
            SelectedElement::Audio(id) => {
                if let Some(i) = self.audio_instances.get_mut(id) {
                    i.user_rect = rect;
                }
            }
            SelectedElement::Function(id) => {
                if let Some(i) = self.function_instances.get_mut(id) {
                    i.user_rect = rect;
                }
            }
            SelectedElement::Text(_) => {}
        }
    }

    /// 提交未决的文本内容编辑：若编辑后内容确有变化，push `ModifyText`。
    fn commit_text_edit(&mut self) {
        let Some(sess) = self.text_edit_undo.take() else {
            return;
        };
        let Some(cur) = self.find_text_elem(sess.page, sess.elem_id) else {
            return;
        };
        let changed = match (&sess.old, &cur) {
            (Element::Text(o), Element::Text(n)) => o.text != n.text,
            _ => false,
        };
        if changed {
            self.history.push(UndoCmd::ModifyText {
                page: sess.page,
                elem_id: sess.elem_id,
                old: sess.old,
                new: cur,
            });
        }
    }

    /// 应用一次撤销：把 `cmd` 描述的**已发生操作**反向执行。
    fn apply_undo(&mut self, cmd: UndoCmd, ctx: &egui::Context) {
        match cmd {
            UndoCmd::InsertElement { page, elem } => {
                // 撤销「插入」= 删除该元素。
                let id = elem.id();
                if self.edit.doc.pages.is_empty() {
                    self.edit.doc.elements.retain(|e| e.id() != id);
                } else if let Some(p) = self.edit.doc.pages.get_mut(page) {
                    p.elements.retain(|e| e.id() != id);
                }
            }
            UndoCmd::RemoveElement { page, index, elem } => {
                // 撤销「删除」= 按索引插回。
                let elems = if self.edit.doc.pages.is_empty() {
                    &mut self.edit.doc.elements
                } else {
                    &mut self.edit.doc.pages[page].elements
                };
                let idx = index.min(elems.len());
                elems.insert(idx, elem);
            }
            UndoCmd::InsertShape { id, .. } => {
                self.shape_instances.remove(&id);
            }
            UndoCmd::RemoveShape { id, inst } => {
                self.shape_instances.insert(id, inst);
            }
            UndoCmd::InsertImage { id, .. } => {
                if let Some(inst) = self.image_instances.remove(&id) {
                    self.image_textures.remove(&inst.path);
                }
            }
            UndoCmd::RemoveImage { id, inst } => {
                self.image_instances.insert(id, inst);
            }
            UndoCmd::InsertVideo { id, .. } => {
                self.video_instances.remove(&id);
                self.inserted_videos.retain(|v| v.id != id);
            }
            UndoCmd::RemoveVideo { record, .. } => {
                self.inserted_videos.push(record.clone());
                self.rebuild_video_instance(&record, ctx);
            }
            UndoCmd::InsertAudio { id, .. } => {
                self.audio_instances.remove(&id);
                self.inserted_audios.retain(|a| a.id != id);
            }
            UndoCmd::RemoveAudio { record, .. } => {
                self.inserted_audios.push(record.clone());
                self.rebuild_audio_instance(&record);
            }
            UndoCmd::ModifyText { page, elem_id, old, .. } => {
                // 撤销「修改」= 回旧值（base 几何 + 文本内容）。
                self.replace_text_elem(page, elem_id, old);
            }
            UndoCmd::ModifyRect { sel, old_rect, .. } => {
                // 撤销「移动/缩放」= 回旧 user_rect。
                self.set_overlay_user_rect(&sel, old_rect);
            }
            UndoCmd::InsertFunction { id, .. } => {
                self.function_instances.remove(&id);
            }
            UndoCmd::RemoveFunction { id, inst } => {
                self.function_instances.insert(id, inst);
            }
        }
        self.selected_element_id = None;
        ctx.request_repaint();
    }

    /// 应用一次重做：把 `cmd` 描述的**已发生操作**正向执行。
    fn apply_redo(&mut self, cmd: UndoCmd, ctx: &egui::Context) {
        match cmd {
            UndoCmd::InsertElement { page, elem } => {
                // 重做「插入」= 重新插入。
                if self.edit.doc.pages.is_empty() {
                    self.edit.doc.elements.push(elem);
                } else {
                    self.edit.doc.pages[page].elements.push(elem);
                }
            }
            UndoCmd::RemoveElement { page, index, elem } => {
                // 重做「删除」= 再次删除。
                let id = elem.id();
                if self.edit.doc.pages.is_empty() {
                    self.edit.doc.elements.retain(|e| e.id() != id);
                } else if let Some(p) = self.edit.doc.pages.get_mut(page) {
                    p.elements.retain(|e| e.id() != id);
                }
                let _ = index;
            }
            UndoCmd::InsertShape { id, inst } => {
                self.shape_instances.insert(id, inst);
            }
            UndoCmd::RemoveShape { id, .. } => {
                self.shape_instances.remove(&id);
            }
            UndoCmd::InsertImage { id, inst } => {
                self.image_instances.insert(id, inst);
            }
            UndoCmd::RemoveImage { id, .. } => {
                if let Some(inst) = self.image_instances.remove(&id) {
                    self.image_textures.remove(&inst.path);
                }
            }
            UndoCmd::InsertVideo { record, .. } => {
                self.inserted_videos.push(record.clone());
                self.rebuild_video_instance(&record, ctx);
            }
            UndoCmd::RemoveVideo { id, .. } => {
                self.video_instances.remove(&id);
                self.inserted_videos.retain(|v| v.id != id);
            }
            UndoCmd::InsertAudio { record, .. } => {
                self.inserted_audios.push(record.clone());
                self.rebuild_audio_instance(&record);
            }
            UndoCmd::RemoveAudio { id, .. } => {
                self.audio_instances.remove(&id);
                self.inserted_audios.retain(|a| a.id != id);
            }
            UndoCmd::ModifyText { page, elem_id, new, .. } => {
                // 重做「修改」= 应用新值。
                self.replace_text_elem(page, elem_id, new);
            }
            UndoCmd::ModifyRect { sel, new_rect, .. } => {
                // 重做「移动/缩放」= 应用新 user_rect。
                self.set_overlay_user_rect(&sel, new_rect);
            }
            UndoCmd::InsertFunction { id, inst } => {
                self.function_instances.insert(id, inst);
            }
            UndoCmd::RemoveFunction { id, .. } => {
                self.function_instances.remove(&id);
            }
        }
        self.selected_element_id = None;
        ctx.request_repaint();
    }

    /// 撤销/重做时按记录重建视频叠加层（重新 spawn ffmpeg 解码进程）。
    fn rebuild_video_instance(&mut self, record: &InsertedVideo, ctx: &egui::Context) {
        if self.video_instances.contains_key(&record.id) {
            return;
        }
        let is_loop = match &record.element {
            ElementData::Video(v) => v.is_loop,
            _ => false,
        };
        let player = VideoPlayer::new(&record.path, is_loop).ok();
        let duration_ms = player.as_ref().map(|p| p.duration_ms()).unwrap_or(0);
        let z_index = self.next_z_index;
        self.next_z_index += 1;
        self.video_instances.insert(
            record.id.clone(),
            VideoInstance {
                world_rect: Some(record.world_rect),
                player,
                last_tex: None,
                user_rect: None,
                duration_ms,
                current_ms: 0,
                seeking: false,
                seek_target_ms: 0,
                z_index,
                page: record.page,
            },
        );
        ctx.request_repaint();
    }

    /// 撤销/重做时按记录重建音频叠加层（重新 spawn ffmpeg 音频子进程）。
    fn rebuild_audio_instance(&mut self, record: &InsertedAudio) {
        if self.audio_instances.contains_key(&record.id) {
            return;
        }
        let mut inst = AudioInstance::new(&record.path, false);
        inst.world_rect = Some(record.world_rect);
        inst.z_index = self.next_z_index;
        self.next_z_index += 1;
        inst.page = record.page;
        self.audio_instances.insert(record.id.clone(), inst);
    }

    /// 在内容之上绘制音频叠加层（画布上的音频控制条，非视频帧）。
    ///
    /// - 备课模式：播放/暂停按钮、进度条（点击/拖拽 seek）、时间文本；选中态启用
    ///   `RectInteraction` 移动/缩放控制条。
    /// - 授课模式：只读渲染控制条，不启用交互（与视频/图片/形状一致）。
    fn draw_audio_overlay(&mut self, ctx: &egui::Context) {
        if self.audio_instances.is_empty() && self.pending_audio.is_none() {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("audio_overlay"),
        ));
        let (cam, panel_offset) = if let Some(d) = self.display.as_ref() {
            (Some(d.camera.clone()), egui::Vec2::ZERO)
        } else {
            (
                Some(self.edit.camera.clone()),
                egui::vec2(self.edit.canvas_offset[0], self.edit.canvas_offset[1]),
            )
        };
        let screen = ctx.screen_rect();
        let prepare = self.mode == AppMode::Prepare;

        let screen_rect_of =
            |world_rect: Option<egui::Rect>, user_rect: Option<egui::Rect>| -> egui::Rect {
                let base = if let Some(wr) = world_rect {
                    match &cam {
                        Some(c) => {
                            let tl = c.world_to_screen([wr.min.x, wr.min.y]) + panel_offset;
                            let br = c.world_to_screen([wr.max.x, wr.max.y]) + panel_offset;
                            egui::Rect::from_two_pos(tl, br)
                        }
                        None => default_overlay_rect(screen),
                    }
                } else {
                    default_overlay_rect(screen)
                };
                user_rect.unwrap_or(base)
            };

        // ── 点击放置（pending）：幽灵控制条跟随光标，单击固定到该处。 ──
        if prepare {
            if let Some(path) = self.pending_audio.clone() {
                let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                let pressed = ctx.input(|i| i.pointer.primary_pressed());
                let pointer = ctx.pointer_interact_pos();
                if esc {
                    self.pending_audio = None;
                    self.pending_armed = false;
                } else if let Some(pos) = pointer {
                    let zoom = cam.as_ref().map(|c| c.zoom).unwrap_or(1.0);
                    let (gw, gh) = (300.0 * zoom, 48.0 * zoom);
                    let ghost = egui::Rect::from_center_size(pos, egui::vec2(gw, gh));
                    painter.rect_filled(
                        ghost,
                        6.0,
                        egui::Color32::from_rgba_unmultiplied(40, 40, 40, 160),
                    );
                    painter.rect_stroke(
                        ghost,
                        6.0,
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 150, 255)),
                    );
                    if pressed && self.pending_armed {
                        let wc = match &cam {
                            Some(c) => {
                                let local = pos - panel_offset;
                                c.screen_to_world(local)
                            }
                            None => [0.0, 0.0],
                        };
                        self.pending_audio = None;
                        self.pending_armed = false;
                        self.insert_audio_at(&path, wc, ctx);
                    }
                    if pressed {
                        self.pending_armed = true;
                    }
                }
            }
        }

        // 收集当前页实例矩形（按 z_index 升序，后插入的在上层）。
        let cur_page = self.current_canvas_page();
        let mut infos: Vec<(String, egui::Rect, u64)> = Vec::new();
        for (k, inst) in &self.audio_instances {
            if inst.page != cur_page {
                continue;
            }
            let r = screen_rect_of(inst.world_rect, inst.user_rect);
            infos.push((k.clone(), r, inst.z_index));
        }
        infos.sort_by_key(|(_, _, z)| *z);

        let pointer_pos = ctx.pointer_interact_pos();
        let pointer_pressed = ctx.input(|i| i.pointer.primary_pressed());
        let pointer_down = ctx.input(|i| i.pointer.primary_down());

        for (key, rect, _z) in infos {
            let inst = match self.audio_instances.get_mut(&key) {
                Some(i) => i,
                None => continue,
            };
            let selected = self.selected_element_id == Some(SelectedElement::Audio(key.clone()));

            let dur = inst.duration_ms;
            let cur = if inst.seeking {
                inst.seek_target_ms
            } else {
                inst.current_ms()
            };

            // 控制条背景。
            painter.rect_filled(rect, 6.0, egui::Color32::from_gray(40));

            // 播放/暂停按钮（左侧）。
            let play_btn = egui::Rect::from_min_size(
                rect.min + egui::vec2(6.0, (rect.height() - 24.0) / 2.0),
                egui::vec2(24.0, 24.0),
            );
            painter.circle_filled(play_btn.center(), 12.0, egui::Color32::from_rgb(0, 150, 255));
            painter.text(
                play_btn.center(),
                egui::Align2::CENTER_CENTER,
                if inst.paused { "▶" } else { "⏸" },
                egui::FontId::proportional(13.0),
                egui::Color32::WHITE,
            );

            // 进度条（中间）。
            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 36.0, rect.center().y - 3.0),
                egui::vec2((rect.width() - 72.0).max(8.0), 6.0),
            );
            painter.rect_filled(bar_rect, 3.0, egui::Color32::from_gray(80));
            if dur > 0 {
                let progress = (cur as f32 / dur as f32).clamp(0.0, 1.0);
                let played = egui::Rect::from_min_max(
                    bar_rect.min,
                    egui::pos2(bar_rect.left() + bar_rect.width() * progress, bar_rect.bottom()),
                );
                painter.rect_filled(played, 3.0, egui::Color32::from_rgb(0, 150, 255));
            }

            // 时间文本（右侧）。
            painter.text(
                egui::pos2(rect.right() - 6.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                format!("{}/{}", fmt_time(cur), fmt_time(dur)),
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );

            if prepare && selected {
                // 播放/暂停 + seek 命中（仅选中态可交互，与「先隐身后选中」一致）。
                if let Some(pos) = pointer_pos {
                    if pointer_pressed && play_btn.contains(pos) {
                        inst.toggle_paused();
                    }
                    // 进度条：点击 → 立即 seek；拖拽 → 拖动中只更新目标，松手 seek。
                    let bar_hit = bar_rect.expand(8.0);
                    if inst.seeking {
                        if pointer_down {
                            let ratio =
                                ((pos.x - bar_rect.left()) / bar_rect.width()).clamp(0.0, 1.0);
                            inst.seek_target_ms = (ratio * dur as f32) as u64;
                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                        } else {
                            inst.seeking = false;
                            let target = inst.seek_target_ms;
                            inst.seek(target);
                        }
                    } else if pointer_pressed && dur > 0 && bar_hit.contains(pos) {
                        inst.seeking = true;
                        let ratio =
                            ((pos.x - bar_rect.left()) / bar_rect.width()).clamp(0.0, 1.0);
                        inst.seek_target_ms = (ratio * dur as f32) as u64;
                    }
                }

                // 移动 / 缩放控制条（复用 RectInteraction + 全局拖拽守卫）。
                let sid = egui::Id::new(format!("audio_{key}"));
                let (_r, interact) = Self::overlay_rect_drag(
                    &mut self.active_drag,
                    &mut self.drag_snapshot,
                    &mut self.history,
                    SelectedElement::Audio(key.clone()),
                    sid,
                    rect,
                    &mut inst.user_rect,
                    ctx,
                );
                interact.draw_overlay(&painter);
            }
        }
    }

    /// 在内容之上绘制形状叠加层（与视频/图片叠加层同层、同交互组件）。
    ///
    /// - 备课模式：复用公共 `RectInteraction`（8 方向缩放 + 内部拖拽），与视频/图片
    ///   共用全局唯一拖拽守卫；拖拽后边框 + 角 grip 由 `RectInteraction::draw_overlay` 绘制。
    /// - 授课模式：仅只读渲染（`draw_shape`），不启用交互、不画边框，符合「授课只读」要求。
    ///
    /// 绘制纯粹调用 `shape_renderer::draw_shape`（egui Painter），不依赖任何字体图标。
    fn draw_shape_overlay(&mut self, ctx: &egui::Context) {
        if self.shape_instances.is_empty() && self.pending_shape.is_none() {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("shape_overlay"),
        ));
        // 备课模式用 EditApp 相机 + 画布屏幕偏移；授课模式用 DisplayApp 相机（全屏）。
        let (cam, panel_offset) = if let Some(d) = self.display.as_ref() {
            (Some(d.camera.clone()), egui::Vec2::ZERO)
        } else {
            (
                Some(self.edit.camera.clone()),
                egui::vec2(self.edit.canvas_offset[0], self.edit.canvas_offset[1]),
            )
        };
        let screen = ctx.screen_rect();
        let prepare = self.mode == AppMode::Prepare;

        // ── 拖拽绘制（pencil / rubber-band）──
        // 备课时点「🔷 形状 → ➕ 插入」后进入此模式：按下左键确定起点，拖动鼠标时
        // 实时绘制半透明亮蓝边框的矩形虚影（无填充），松开左键提交为正式形状并退出。
        // 绘制中的矩形仅作为 UI 层临时状态（`shape_draw`），不混入文档业务数据，
        // 直到提交才作为 `ShapeInstance` 落入宿主层并纳入 Undo 栈（业务/视图解耦）。
        if prepare {
            if let Some(kind) = self.pending_shape {
                let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                let (pressed, released) = ctx.input(|i| {
                    (i.pointer.primary_pressed(), i.pointer.primary_released())
                });
                let pointer = ctx.pointer_interact_pos();

                if esc {
                    // Esc 取消绘制模式，回到普通状态。
                    self.pending_shape = None;
                    self.pending_armed = false;
                    self.shape_draw = None;
                } else if self.shape_draw.is_some() {
                    // ── 正在拖拽绘制：跟随鼠标更新并实时绘制虚影。 ──
                    let mut draw = self.shape_draw.take().unwrap();
                    if let Some(p) = pointer {
                        draw.current_screen = Some(p);
                    }
                    if let Some(cur) = draw.current_screen {
                        let start = draw.start_screen;
                        // 绘制中虚影：亮蓝边框、50% 透明、无填充、线宽 2.0。
                        let preview = egui::Rect::from_two_pos(start, cur);
                        let stroke = egui::Stroke::new(
                            2.0,
                            egui::Color32::from_rgba_unmultiplied(0, 150, 255, 128),
                        );
                        painter.rect_stroke(preview, 0.0, stroke);
                    }
                    if released {
                        // 松开左键：屏幕矩形 → 画布矩形 → 提交为正式形状并退出绘制模式。
                        self.pending_shape = None;
                        self.pending_armed = false;
                        if let Some(cur) = draw.current_screen {
                            let start = draw.start_screen;
                            let screen_rect = egui::Rect::from_two_pos(start, cur);
                            // 过小的「点击」不生成形状（视为取消）。
                            if screen_rect.width() >= 8.0 && screen_rect.height() >= 8.0 {
                                let world_rect = self.screen_rect_to_world_rect(screen_rect);
                                self.commit_shape_geom(kind, world_rect, None, false, false, ctx);
                                // 刚生成的形状以「固定」状态呈现：清空选中，避免边框/抓手
                                // 立即挡住光标或遮挡图形，影响老师绘制下一个图形。
                                self.selected_element_id = None;
                            }
                        }
                    } else {
                        // 仍在拖动：把更新后的状态存回，下一帧继续。
                        self.shape_draw = Some(draw);
                    }
                } else {
                    // ── 等待第一次左键按下，确定拖拽起点。 ──
                    // 跳过「插入」按钮那一下的点击，下一帧起才允许绘制，避免误放到工具栏上。
                    if self.pending_armed && pressed {
                        if let Some(pos) = pointer {
                            self.shape_draw = Some(ShapeDrawState {
                                kind,
                                start_screen: pos,
                                current_screen: Some(pos),
                            });
                        }
                    } else if !self.pending_armed {
                        self.pending_armed = true;
                    }
                }
            }
        }

        let keys: Vec<String> = self.shape_instances.keys().cloned().collect();
        let cur_page = self.current_canvas_page();

        // 1) 先计算每个形状的屏幕矩形（用于命中检测 + 绘制）。
        //    仅收集「所属页 == 当前页」的形状，翻页后其它页的形状不再渲染 / 命中。
        let mut rects: Vec<(String, egui::Rect, u64)> = Vec::with_capacity(keys.len());
        for key in &keys {
            let inst = match self.shape_instances.get(key) {
                Some(i) => i,
                None => continue,
            };
            if inst.page != cur_page {
                continue;
            }
            let base_rect = if let Some(wr) = inst.world_rect {
                match &cam {
                    Some(c) => {
                        let tl = c.world_to_screen([wr.min.x, wr.min.y]) + panel_offset;
                        let br = c.world_to_screen([wr.max.x, wr.max.y]) + panel_offset;
                        egui::Rect::from_two_pos(tl, br)
                    }
                    None => default_overlay_rect(screen),
                }
            } else {
                default_overlay_rect(screen)
            };
            let rect = inst.user_rect.unwrap_or(base_rect);
            rects.push((key.clone(), rect, inst.z_index));
        }
        // 按 z_index 升序排序：后插入的（值大）后绘制，渲染在上层。
        rects.sort_by_key(|(_, _, z)| *z);

        // 2) 选中 / 取消选中已统一由 `handle_canvas_click`（全局单击处理器）负责：
        //    单击形状矩形 → 选中（边框 + 抓手）；单击空白处 → 取消选中。不内联判定，
        //    以保证三套叠加层共用同一选中态、互不覆盖。

        // 3) 绘制：仅选中的形状显示边框/抓手（RectInteraction）；其余以纯图形呈现。
        for (key, rect, _z) in rects {
            let inst = match self.shape_instances.get(&key) {
                Some(i) => i,
                None => continue,
            };
            let kind = inst.kind;
            let stroke_width = inst.stroke_width;
            let stroke_color = inst.stroke_color;
            let fill_color = inst.fill_color;
            let arc_degrees = inst.arc_degrees;
            let line_flipped = inst.line_flipped;

            let stroke = egui::Stroke::new(
                stroke_width,
                egui::Color32::from_rgba_unmultiplied(
                    stroke_color.0,
                    stroke_color.1,
                    stroke_color.2,
                    stroke_color.3,
                ),
            );
            let fill = fill_color.map(|c| egui::Color32::from_rgba_unmultiplied(c.0, c.1, c.2, c.3));

            let selected = self.selected_element_id == Some(SelectedElement::Shape(key.clone()));
            if prepare && selected {
                // 选中态：8 方向缩放 + 内部拖拽（复用 RectInteraction），并显示边框/抓手。
                let sid = egui::Id::new(format!("shape_{key}"));
                let (r, interact) = if let Some(inst) = self.shape_instances.get_mut(&key) {
                    Self::overlay_rect_drag(
                        &mut self.active_drag,
                        &mut self.drag_snapshot,
                        &mut self.history,
                        SelectedElement::Shape(key.clone()),
                        sid,
                        rect,
                        &mut inst.user_rect,
                        ctx,
                    )
                } else {
                    (rect, RectInteraction::new(sid, rect))
                };
                draw_shape(&painter, r, kind, stroke, fill, arc_degrees, line_flipped);
                interact.draw_overlay(&painter);
            } else {
                // 未选中 / 授课模式：纯图形渲染，无边框、无交互。
                draw_shape(&painter, rect, kind, stroke, fill, arc_degrees, line_flipped);
            }
        }

        ctx.request_repaint();
    }

    // ── 函数绘图叠加层（框选 → 菜单 → 坐标系 + 曲线） ──────────────────────

    /// 绘制函数绘图叠加层（坐标系 + 曲线 + 表达式标签）。
    ///
    /// - 与形状/图片/视频同构：仅绘制「所属页 == 当前页」的实例，按 z_index 排序。
    /// - 选中态在备课模式复用公共 `RectInteraction`（8 方向缩放 + 内部拖拽），
    ///   拖拽移动时 `user_rect` 更新 → 坐标系与曲线整体跟随，符合「拖拽整体移动」。
    /// - 授课模式仅只读渲染，不画边框、不启用交互。
    fn draw_function_overlay(&mut self, ctx: &egui::Context) {
        if self.function_instances.is_empty() {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("function_overlay"),
        ));
        let screen = ctx.screen_rect();
        let prepare = self.mode == AppMode::Prepare;
        let cur_page = self.current_canvas_page();

        // 仅收集「所属页 == 当前页」的实例并按 z_index 升序排序（后插入的绘制在上层）。
        let mut rects: Vec<(String, egui::Rect, u64)> = self
            .function_instances
            .iter()
            .filter(|(_, i)| i.page == cur_page)
            .map(|(k, i)| {
                (
                    k.clone(),
                    i.user_rect
                        .unwrap_or_else(|| function_default_rect(screen, i.scale)),
                    i.z_index,
                )
            })
            .collect();
        rects.sort_by_key(|(_, _, z)| *z);

        for (key, rect, _z) in rects {
            let inst = match self.function_instances.get(&key) {
                Some(i) => i,
                None => continue,
            };
            let scale = inst.scale;
            let expr = inst.expr.clone();
            let expr_str = inst.expr_str.clone();

            let selected = self.selected_element_id == Some(SelectedElement::Function(key.clone()));
            if prepare && selected {
                // 选中态：8 方向缩放 + 内部拖拽（复用 RectInteraction），并显示边框/抓手。
                let sid = egui::Id::new(format!("function_{key}"));
                let (r, interact) = if let Some(inst) = self.function_instances.get_mut(&key) {
                    Self::overlay_rect_drag(
                        &mut self.active_drag,
                        &mut self.drag_snapshot,
                        &mut self.history,
                        SelectedElement::Function(key.clone()),
                        sid,
                        rect,
                        &mut inst.user_rect,
                        ctx,
                    )
                } else {
                    (rect, RectInteraction::new(sid, rect))
                };
                draw_function_plot(&painter, r, scale, &expr, &expr_str);
                interact.draw_overlay(&painter);
            } else {
                draw_function_plot(&painter, rect, scale, &expr, &expr_str);
            }
        }

        ctx.request_repaint();
    }

    /// 矩形框选（marquee）检测：在画布空白处按下拖拽成框，释放时若恰好框住单个
    /// 文本元素，则在框下方弹出「函数绘图」菜单（交给 [`Self::render_function_menu`]）。
    ///
    /// - 仅在备课模式、且非教具/非放置形状时生效（与单击选中共用 selected 判空）。
    /// - 普通单击（框太小）不弹菜单，只是清空选择。
    fn update_marquee(&mut self, ctx: &egui::Context) {
        if self.mode != AppMode::Prepare
            || !matches!(self.active_tool, ActiveTool::None)
            || self.pending_shape.is_some()
        {
            self.marquee_start = None;
            self.marquee_rect = None;
            return;
        }
        let (pressed, released, down) =
            ctx.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_released(), i.pointer.primary_down()));
        let pointer = ctx.pointer_interact_pos();

        if pressed {
            // 仅当按在空白处（无选中元素）才进入框选；否则丢弃旧选框。
            self.marquee_rect = None;
            self.marquee_start = if self.selected_element_id.is_none() {
                pointer
            } else {
                None
            };
            return;
        }
        if down && self.marquee_start.is_some() {
            if let Some(p) = pointer {
                self.marquee_rect = Some(egui::Rect::from_two_pos(self.marquee_start.unwrap(), p));
            }
            self.draw_marquee_rect(ctx);
            return;
        }
        if released {
            if let Some(r) = self.marquee_rect {
                // 框太小视为普通单击，不弹菜单。
                if r.width() >= 12.0 && r.height() >= 12.0 {
                    let texts = self.find_texts_in_rect(r);
                    if texts.len() == 1 {
                        self.selected_element_id = None;
                        self.function_menu = Some(FunctionMenuState {
                            anchor: r,
                            text: texts[0].clone(),
                            error: None,
                        });
                    }
                }
            }
            self.marquee_start = None;
            self.marquee_rect = None;
        }
    }

    /// 绘制当前进行中的框选矩形（半透明填充 + 蓝色描边）。
    fn draw_marquee_rect(&self, ctx: &egui::Context) {
        if let Some(r) = self.marquee_rect {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("marquee_overlay"),
            ));
            painter.rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(0, 120, 255, 30));
            painter.rect_stroke(
                r,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 150, 255)),
            );
        }
    }

    /// 找出与给定屏幕矩形相交的文本元素内容（仅当前页文档层）。
    fn find_texts_in_rect(&self, rect: egui::Rect) -> Vec<String> {
        let (cam, panel_offset) = if let Some(d) = self.display.as_ref() {
            (Some(d.camera.clone()), egui::Vec2::ZERO)
        } else {
            (
                Some(self.edit.camera.clone()),
                egui::vec2(self.edit.canvas_offset[0], self.edit.canvas_offset[1]),
            )
        };
        let to_screen = |wr: egui::Rect| -> egui::Rect {
            match &cam {
                Some(c) => {
                    let tl = c.world_to_screen([wr.min.x, wr.min.y]) + panel_offset;
                    let br = c.world_to_screen([wr.max.x, wr.max.y]) + panel_offset;
                    egui::Rect::from_two_pos(tl, br)
                }
                None => rect, // 无相机（测试）时按输入矩形原样处理
            }
        };
        let cur_page = self.current_canvas_page();
        let elems = if self.edit.doc.pages.is_empty() {
            &self.edit.doc.elements
        } else if let Some(p) = self.edit.doc.pages.get(cur_page) {
            &p.elements
        } else {
            return Vec::new();
        };
        elems
            .iter()
            .filter_map(|e| {
                if let Element::Text(t) = e {
                    let wr = egui::Rect::from_min_size(
                        egui::pos2(t.base.position[0], t.base.position[1]),
                        egui::vec2(t.base.size[0], t.base.size[1]),
                    );
                    let sr = to_screen(wr);
                    if rect.intersects(sr) {
                        Some(t.text.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    /// 从框选菜单提交：把编译好的表达式实例化为宿主层函数绘图叠加层（可移动/缩放/删除/Undo）。
    fn insert_function_instance(
        &mut self,
        expr: crate::function_parser::Expr,
        expr_str: String,
        center: egui::Pos2,
        ctx: &egui::Context,
    ) {
        let scale = 40.0; // 每单位像素 → 视界 ±10 单位、边长 800px。
        let rect = egui::Rect::from_center_size(center, egui::vec2(20.0 * scale, 20.0 * scale));
        let id = format!("fn_{}", Uuid::new_v4());
        let z_index = self.next_z_index;
        self.next_z_index += 1;
        let inst = FunctionPlotInstance {
            user_rect: Some(rect),
            scale,
            expr,
            expr_str,
            z_index,
            page: self.edit.multi_page.current_page,
        };
        self.function_instances.insert(id.clone(), inst.clone());
        self.history.push(UndoCmd::InsertFunction { id: id.clone(), inst });
        // 自动选中新实例，便于老师立即拖动/调整。
        self.selected_element_id = Some(SelectedElement::Function(id));
        log::info!("[function-plot] 框选提交函数曲线实例");
        ctx.request_repaint();
    }

    /// 渲染「📊 检测到函数表达式」弹出菜单（框下方）。
    ///
    /// 点击「📈 函数绘图」→ 解析表达式 → 成功则生成坐标系 + 曲线实例并关闭菜单；
    /// 失败则在菜单内显示红字（不清崩、不 panic），保持菜单打开便于修改。
    fn render_function_menu(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.function_menu.clone() else {
            return;
        };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.function_menu = None;
            return;
        }
        let mut error: Option<String> = menu.error.clone();
        let mut to_plot: Option<(crate::function_parser::Expr, String, egui::Pos2)> = None;
        let res = egui::Area::new(egui::Id::new("function_menu"))
            .fixed_pos(menu.anchor.left_bottom() + egui::vec2(0.0, 5.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.group(|ui| {
                    ui.set_min_width(190.0);
                    ui.label(format!("📊 检测到函数：{}", menu.text));
                    if ui.button("📈 函数绘图").clicked() {
                        match crate::function_parser::parse(&menu.text) {
                            Ok(expr) => to_plot = Some((expr, menu.text.clone(), menu.anchor.center())),
                            Err(e) => error = Some(e),
                        }
                    }
                    if let Some(e) = &error {
                        ui.colored_label(egui::Color32::from_rgb(255, 90, 90), e);
                    }
                });
                ctx.request_repaint();
            });

        if let Some((expr, text, center)) = to_plot {
            self.insert_function_instance(expr, text, center, ctx);
            self.function_menu = None;
            return;
        }
        // 出错：保持菜单打开显示红字；否则按「点击菜单外则关闭」处理。
        if error.is_some() {
            self.function_menu = Some(FunctionMenuState {
                anchor: menu.anchor,
                text: menu.text.clone(),
                error,
            });
        } else {
            let pressed = ctx.input(|i| i.pointer.primary_pressed());
            let pointer = ctx.pointer_interact_pos();
            let clicked_outside = pressed
                && !res.response.rect.contains(pointer.unwrap_or(egui::Pos2::ZERO));
            if clicked_outside {
                self.function_menu = None;
            } else {
                self.function_menu = Some(menu);
            }
        }
    }

    // ── 虚拟教具（圆规 / 三角尺 / 量角器） ─────────────────────────────────

    /// 画布中心的屏幕坐标（教具初始位置）。
    fn canvas_screen_center(&self) -> egui::Pos2 {
        let panel_offset = egui::vec2(self.edit.canvas_offset[0], self.edit.canvas_offset[1]);
        let cam = &self.edit.camera;
        cam.world_to_screen(cam.offset) + panel_offset
    }

    /// 激活圆规（画布中心，等待第一次点击定圆心）。
    fn activate_compass(&mut self, ctx: &egui::Context) {
        let c = self.canvas_screen_center();
        self.active_tool = ActiveTool::Compass(CompassTool {
            pivot: c,
            pencil: c,
            mode: CompassMode::Circle,
            arc_start_deg: 0.0,
            arc_end_deg: 90.0,
            stage: 0,
        });
        self.selected_element_id = None;
        ctx.request_repaint();
    }

    /// 激活三角尺（画布中心，默认 0°）。
    fn activate_set_square(&mut self, kind: SetSquareKind, ctx: &egui::Context) {
        let c = self.canvas_screen_center();
        self.active_tool = ActiveTool::SetSquare(SetSquareTool {
            kind,
            origin: c,
            rotation_deg: 0.0,
            size: 200.0,
            moving: false,
            drawing: false,
            rotating: false,
            line_start: egui::Pos2::ZERO,
            line_current: egui::Pos2::ZERO,
            line_edge: None,
        });
        self.selected_element_id = None;
        ctx.request_repaint();
    }

    /// 激活量角器（画布中心）。
    fn activate_protractor(&mut self, mode: ProtractorMode, ctx: &egui::Context) {
        let c = self.canvas_screen_center();
        self.active_tool = ActiveTool::Protractor(ProtractorTool {
            center: c,
            radius: 140.0,
            rotation_deg: 0.0,
            cursor_angle_deg: 90.0,
            mode,
            first_angle_deg: None,
            dragging: false,
            last_mouse: egui::pos2(0.0, 0.0),
        });
        self.selected_element_id = None;
        ctx.request_repaint();
    }

    /// 激活直尺（画布中心，水平放置，默认 10cm 长）。
    fn activate_ruler(&mut self, ctx: &egui::Context) {
        let c = self.canvas_screen_center();
        let half = crate::tools::PIXELS_PER_CM * 5.0;
        self.active_tool = ActiveTool::Ruler(RulerTool {
            start: egui::pos2(c.x - half, c.y),
            end: egui::pos2(c.x + half, c.y),
            dragging_end: None,
            dragging_body: false,
            last_mouse: egui::Pos2::ZERO,
        });
        self.selected_element_id = None;
        ctx.request_repaint();
    }

    /// 激活正多边形（画布中心，等待第一次点击定中心）。
    fn activate_polygon(&mut self, sides: u8, ctx: &egui::Context) {
        let c = self.canvas_screen_center();
        self.active_tool = ActiveTool::Polygon(PolygonTool {
            center: c,
            radius: 0.0,
            sides: sides.clamp(3, 12),
            preview_angle: crate::shape_renderer::POLYGON_DEFAULT_START_DEG,
        });
        self.selected_element_id = None;
        ctx.request_repaint();
    }

    /// 激活函数绘图：画布中心放坐标系，弹出表达式输入框。
    fn activate_function_plot(&mut self, ctx: &egui::Context) {
        let c = self.canvas_screen_center();
        self.active_tool = ActiveTool::FunctionPlot(FunctionPlotTool {
            center: c,
            scale: 40.0,
            expr_str: String::new(),
            parsed: None,
            error: None,
        });
        self.selected_element_id = None;
        ctx.request_repaint();
    }

    /// 激活数轴：第一次点击定起点 → 拖拽预览 → 松开提交。
    fn activate_number_line(&mut self, ctx: &egui::Context) {
        self.active_tool = ActiveTool::NumberLine(NumberLineTool::default());
        self.selected_element_id = None;
        ctx.request_repaint();
    }

    /// 激活倒计时器：画布中央，等待输入时间。
    fn activate_countdown(&mut self, ctx: &egui::Context) {
        let c = self.canvas_screen_center();
        let t = CountdownTool {
            position: c - egui::vec2(100.0, 40.0), // 居中（200×80 的左上角）
            ..CountdownTool::default()
        };
        self.active_tool = ActiveTool::Countdown(t);
        self.selected_element_id = None;
        ctx.request_repaint();
    }

    /// 授课工具面板（左上角）：倒计时 / 直尺 / 三角尺 / 量角器等授课工具入口。
    fn teach_tools_panel(&mut self, ctx: &egui::Context) {
        egui::Window::new("授课工具")
            .id(egui::Id::new("teach_tools_panel"))
            .default_pos(egui::pos2(12.0, 12.0))
            .collapsible(true)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("⏱ 倒计时").clicked() {
                        self.activate_countdown(ctx);
                    }
                    if ui.button("📏 直尺").clicked() {
                        self.activate_ruler(ctx);
                    }
                    if ui.button("📐 三角尺").clicked() {
                        self.activate_set_square(SetSquareKind::Triangle30_60_90, ctx);
                    }
                    if ui.button("📐 量角器").clicked() {
                        self.activate_protractor(ProtractorMode::Measure, ctx);
                    }
                    if ui.button("📏 数轴").clicked() {
                        self.activate_number_line(ctx);
                    }
                });
                ui.horizontal(|ui| {
                    // 放大镜：点一下激活 / 再点一下退出（也可按 Esc 退出）。
                    if ui.selectable_label(self.magnifier.active, "🔍 放大镜").clicked() {
                        self.magnifier.active = !self.magnifier.active;
                    }
                    // 随机点名器：点按打开浮动窗口（名单在该工具内临时保留）。
                    if ui.button("🎲 随机点名").clicked() {
                        self.name_picker.visible = true;
                    }
                });
            });
    }

    /// 每帧渲染随机点名器浮动窗口：Esc 关闭、滚动动画推进、窗口 UI；关闭不清空名单。
    ///
    /// 名单存于 `Self::name_picker`（`NamePickerTool`），**不序列化到 ENBX**；窗口关闭
    /// （✕ / Esc / 「关闭」按钮）仅隐藏窗口，名单保留供下次打开复用。
    fn render_name_picker(&mut self, ctx: &egui::Context) {
        if !self.name_picker.visible {
            return;
        }
        // Esc 关闭窗口（名单保留）。
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.name_picker.visible = false;
            self.name_picker.is_rolling = false;
            return;
        }
        // ── 滚动动画：每 roll_speed 秒随机切换显示的名字。 ──
        if self.name_picker.is_rolling {
            self.name_picker.elapsed += ctx.input(|i| i.unstable_dt);
            if self.name_picker.elapsed >= self.name_picker.roll_speed {
                self.name_picker.elapsed = 0.0;
                if !self.name_picker.names.is_empty() {
                    let idx = (ctx.input(|i| i.time * 10.0) as usize)
                        % self.name_picker.names.len();
                    self.name_picker.display_name = self.name_picker.names[idx].clone();
                }
            }
            // 滚动中驱动每帧重绘，保证动画连续。
            ctx.request_repaint();
        }

        let mut open = self.name_picker.visible;
        let base = self.name_picker.position;
        let res = egui::Window::new("🎲 随机点名器")
            .id(egui::Id::new("name_picker_window"))
            .default_pos(base)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| self.name_picker_window_ui(ui));
        // 记录拖拽后的窗口位置，下次打开保持一致。
        if let Some(res) = res {
            self.name_picker.position = res.response.rect.min;
        }
        // 窗口右上角 ✕ 关闭 → 隐藏窗口（名单保留）。
        if !open && self.name_picker.visible {
            self.name_picker.visible = false;
            self.name_picker.is_rolling = false;
        }
    }

    /// 随机点名器窗口内容：输入区 / 名单 / 大号滚动显示区 / 控制按钮。
    fn name_picker_window_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("输入学生名单（逗号 / 换行分隔），如: 张三,李四,王五");
        ui.text_edit_singleline(&mut self.name_picker.input_text);
        ui.horizontal(|ui| {
            if ui.button("➕ 添加").clicked() {
                self.name_picker.add_from_input();
            }
            if ui.button("🧹 清空").clicked() {
                self.name_picker.clear_names();
            }
        });

        let list = if self.name_picker.names.is_empty() {
            "（空）".to_string()
        } else {
            format!("{}（{} 人）", self.name_picker.names.join("  "), self.name_picker.names.len())
        };
        ui.label(format!("当前名单: {list}"));

        // ── 滚动显示区：大号字体；选中后金色背景 + 更大字号。 ──
        let selected = self.name_picker.selected_name.is_some();
        let big = egui::FontId::proportional(if selected { 42.0 } else { 30.0 });
        let display = self.name_picker.display_name.clone();
        let frame = egui::Frame::group(ui.style())
            .fill(if selected {
                egui::Color32::from_rgb(255, 215, 0)
            } else {
                ui.visuals().extreme_bg_color
            })
            .rounding(8.0);
        frame.show(ui, |ui| {
            ui.set_min_size(egui::vec2(260.0, 76.0));
            ui.vertical_centered(|ui| {
                let name = if display.is_empty() { "　" } else { display.as_str() };
                let color = if selected {
                    egui::Color32::from_rgb(70, 40, 0)
                } else {
                    egui::Color32::WHITE
                };
                ui.label(egui::RichText::new(name).size(big.size).color(color).strong());
            });
        });

        // ── 控制按钮。 ──
        ui.horizontal(|ui| {
            if !self.name_picker.is_rolling {
                let has_names = !self.name_picker.names.is_empty();
                let disabled = ui.add_enabled(has_names, egui::Button::new("▶ 开始滚动"));
                if disabled.clicked() {
                    // 再次开始：清空选中状态，重新滚动。
                    self.name_picker.is_rolling = true;
                    self.name_picker.selected_name = None;
                    self.name_picker.elapsed = 0.0;
                    if self.name_picker.display_name.is_empty() {
                        self.name_picker.display_name = self.name_picker.names[0].clone();
                    }
                }
                if !has_names {
                    ui.label(egui::RichText::new("（请先添加名单）").weak());
                }
            } else if ui.button("⏹ 停止").clicked() {
                self.name_picker.stop_rolling();
            }
            if ui.button("✕ 关闭").clicked() {
                self.name_picker.visible = false;
                self.name_picker.is_rolling = false;
            }
        });
    }

    /// 每帧更新放大镜：Esc 退出；滚轮在 1x–4x 间调倍数；圆心跟随鼠标；随后叠加绘制。
    ///
    /// 放大镜是**纯 UI 覆盖层**：不拦截画布交互（用 `pointer.hover_pos` 而非 `response`）、
    /// 不产生持久元素、不进 Undo 栈 —— 业务数据（文档 / 叠加层实例）完全不受其影响。
    fn update_magnifier(&mut self, ctx: &egui::Context) {
        if !self.magnifier.active {
            return;
        }
        // Esc 退出放大镜（回到普通授课视图）。
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.magnifier.active = false;
            return;
        }
        // 圆心跟随鼠标；用 hover_pos 而非 response 拦截，避免吞掉底层画布/覆盖层的交互。
        if let Some(p) = ctx.input(|i| i.pointer.hover_pos()) {
            self.magnifier.center = p;
        }
        // 鼠标滚轮调节放大倍数（1.0 → 4.0），每格约 0.2x。
        let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.001 {
            self.magnifier.zoom_factor =
                (self.magnifier.zoom_factor + scroll * 0.004).clamp(1.0, 4.0);
        }
        self.draw_magnifier(ctx);
    }

    /// 叠加放大镜层（必须在所有元素/叠加层绘制完成后调用）：圈内内容放大重绘、圈外压暗、
    /// 蓝色圆圈边框 + 当前放大倍数角标。纯预览，不改变任何数据模型。
    #[allow(clippy::needless_borrows_for_generic_args)]
    fn draw_magnifier(&mut self, ctx: &egui::Context) {
        let tool = self.magnifier;
        if !tool.active {
            return;
        }
        let center = tool.center;
        let radius = tool.radius;
        let zoom = tool.zoom_factor;
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("magnifier_overlay"),
        ));
        // 相机 / 画布偏移：与形状叠加层一致（授课全屏用 DisplayApp 相机；备课用 EditApp 相机）。
        let (cam, panel_offset) = if let Some(d) = self.display.as_ref() {
            (Some(d.camera.clone()), egui::Vec2::ZERO)
        } else {
            (
                Some(self.edit.camera.clone()),
                egui::vec2(self.edit.canvas_offset[0], self.edit.canvas_offset[1]),
            )
        };
        let cur_page = self.current_canvas_page();
        // 圈内绘制裁剪到圆的外接正方形（近似圆，圆角外的角落由环形遮罩压暗）。
        let clip_rect = egui::Rect::from_center_size(center, egui::vec2(radius * 2.0, radius * 2.0));
        let inner = painter.with_clip_rect(clip_rect);

        // ── 圈内放大重绘：只重新绘制「所圈区域内的可见元素」──
        // 形状：把每个形状的屏幕矩形以圆心为基准放大 zoom 倍后，用同一渲染器重绘。
        let shape_keys: Vec<String> = self.shape_instances.keys().cloned().collect();
        for key in &shape_keys {
            let inst = match self.shape_instances.get(key) {
                Some(i) => i,
                None => continue,
            };
            if inst.page != cur_page {
                continue;
            }
            let base_rect = if let Some(wr) = inst.world_rect {
                match &cam {
                    Some(c) => {
                        let tl = c.world_to_screen([wr.min.x, wr.min.y]) + panel_offset;
                        let br = c.world_to_screen([wr.max.x, wr.max.y]) + panel_offset;
                        egui::Rect::from_two_pos(tl, br)
                    }
                    None => default_overlay_rect(screen),
                }
            } else {
                default_overlay_rect(screen)
            };
            let r = inst.user_rect.unwrap_or(base_rect);
            // 放大后的屏幕矩形（圆心不动、坐标与尺寸等比外扩）。
            let mag = egui::Rect::from_min_max(
                magnifier_transform(r.min, center, panel_offset, 1.0, zoom),
                magnifier_transform(r.max, center, panel_offset, 1.0, zoom),
            );
            let stroke = egui::Stroke::new(
                inst.stroke_width,
                egui::Color32::from_rgba_unmultiplied(
                    inst.stroke_color.0,
                    inst.stroke_color.1,
                    inst.stroke_color.2,
                    inst.stroke_color.3,
                ),
            );
            let fill = inst
                .fill_color
                .map(|c| egui::Color32::from_rgba_unmultiplied(c.0, c.1, c.2, c.3));
            draw_shape(&inner, mag, inst.kind, stroke, fill, inst.arc_degrees, inst.line_flipped);
        }

        // 函数绘图：在主画布矩形放大后，重绘完整的坐标系 + 曲线（内部自带裁剪）。
        let func_keys: Vec<String> = self.function_instances.keys().cloned().collect();
        for key in &func_keys {
            let inst = match self.function_instances.get(key) {
                Some(i) => i,
                None => continue,
            };
            if inst.page != cur_page {
                continue;
            }
            let r = inst.user_rect.unwrap_or_else(|| function_default_rect(screen, inst.scale));
            let mag = egui::Rect::from_min_max(
                magnifier_transform(r.min, center, panel_offset, 1.0, zoom),
                magnifier_transform(r.max, center, panel_offset, 1.0, zoom),
            );
            draw_function_plot(&inner, mag, inst.scale, &inst.expr, &inst.expr_str);
        }

        // ── 圈外压暗（带洞环形遮罩），只留圆内内容高亮。──
        // 外半径取足够大，覆盖整个屏幕角落（with_clip 未裁剪此层）。
        let ring_color = egui::Color32::from_rgba_unmultiplied(18, 22, 30, 120);
        painter.add(egui::Shape::mesh(annulus_mesh(center, radius, 8000.0, ring_color)));

        // 蓝色圆圈边框。
        painter.circle_stroke(
            center,
            radius,
            egui::Stroke::new(3.0, egui::Color32::from_rgb(0, 150, 255)),
        );
        // 当前放大倍数角标（圆圈下方）。
        painter.text(
            egui::pos2(center.x, center.y + radius + 6.0),
            egui::Align2::CENTER_TOP,
            format!("🔍 {zoom:.1}x"),
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(0, 170, 255),
        );

        ctx.request_repaint();
    }

    /// 函数曲线提交：采样点 → SVG polyline path → 文档层 `Element::SvgShape`（可 Undo、可序列化）。
    fn commit_function_plot(
        &mut self,
        expr: &crate::function_parser::Expr,
        center: egui::Pos2,
        scale: f32,
        expr_str: &str,
        ctx: &egui::Context,
    ) {
        // 采样点为空（如表达式恒为 NaN）则不提交。
        let pts = crate::function_parser::sample_points(expr, -10.0, 10.0, 200);
        if pts.is_empty() {
            log::warn!("[function-plot] 采样点为空，未提交: y = {expr_str}");
            return;
        }
        // 统一为宿主层函数绘图实例（与并行框选菜单的删除/Undo 路径对称）：
        // 可选中 / 移动 / 删除 / Undo，与形状/图片/视频/音频叠加层同构。
        let id = format!("func_{}", Uuid::new_v4());
        let z_index = self.next_z_index;
        self.next_z_index += 1;
        let half = 10.0 * scale;
        let inst = FunctionPlotInstance {
            user_rect: Some(egui::Rect::from_center_size(
                center,
                egui::vec2(half * 2.0, half * 2.0),
            )),
            scale,
            expr: expr.clone(),
            expr_str: expr_str.to_string(),
            z_index,
            page: self.edit.multi_page.current_page,
        };
        self.function_instances.insert(id.clone(), inst.clone());
        self.history.push(UndoCmd::InsertFunction { id, inst });
        log::info!("[function-plot] 提交函数曲线: y = {expr_str}");
        ctx.request_repaint();
    }

    /// 教具提交：以指定几何（kind + world_rect + arc_degrees + line_flipped）插入形状叠加层，
    /// 纳入 Undo 栈（与 `insert_shape_at` 同构）。
    fn commit_shape_geom(
        &mut self,
        kind: ShapeKind,
        world_rect: egui::Rect,
        arc_degrees: Option<(f32, f32)>,
        line_flipped: bool,
        fill: bool,
        ctx: &egui::Context,
    ) {
        let id = format!("shape_{}", Uuid::new_v4());
        let z_index = self.next_z_index;
        self.next_z_index += 1;
        let inst = ShapeInstance {
            kind,
            world_rect: Some(world_rect),
            user_rect: None,
            stroke_width: 3.0,
            stroke_color: (0, 0, 0, 255),
            fill_color: if fill { Some((0, 150, 255, 80)) } else { None },
            arc_degrees,
            line_flipped,
            z_index,
            page: self.edit.multi_page.current_page,
        };
        self.shape_instances.insert(id.clone(), inst.clone());
        self.history.push(UndoCmd::InsertShape { id, inst });
        log::info!("[tool] 教具提交形状: {kind:?} arc={arc_degrees:?} flipped={line_flipped}");
        ctx.request_repaint();
    }

    /// 每帧更新激活教具的交互；提交时构造形状并销毁教具。
    #[allow(clippy::type_complexity)]
    fn update_active_tool(&mut self, ctx: &egui::Context) {
        if matches!(self.active_tool, ActiveTool::None) {
            return;
        }
        let pointer = ctx.pointer_interact_pos();
        let pressed = ctx.input(|i| i.pointer.primary_pressed());
        let down = ctx.input(|i| i.pointer.primary_down());
        let released = ctx.input(|i| i.pointer.primary_released());
        // egui 0.29 无 `PointerState::double_clicked`，手动检测：350ms 内两次单击。
        let clicked = ctx.input(|i| i.pointer.primary_clicked());
        let mut double_clicked = false;
        if clicked {
            let now = std::time::Instant::now();
            if let Some(t) = self.last_tool_click {
                if now.duration_since(t) < std::time::Duration::from_millis(350) {
                    double_clicked = true;
                }
            }
            self.last_tool_click = Some(now);
        }
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        if esc {
            self.active_tool = ActiveTool::None;
            return;
        }

        // 提交参数：(kind, world_rect, arc_degrees, line_flipped, fill)。
        let mut commit: Option<(ShapeKind, egui::Rect, Option<(f32, f32)>, bool, bool)> = None;

        match self.active_tool.clone() {
            ActiveTool::Compass(mut t) => {
                if t.stage == 0 {
                    // 第一次按下：确定转轴（圆心）。
                    if pressed {
                        if let Some(p) = pointer {
                            t.pivot = p;
                            t.pencil = p;
                            t.stage = 1;
                        }
                    }
                } else {
                    // 拖动铅笔脚：实时半径。
                    if down {
                        if let Some(p) = pointer {
                            t.pencil = p;
                        }
                    }
                    // 右键循环 Circle → Arc → Sector。
                    if ctx.input(|i| i.pointer.secondary_clicked()) {
                        t.mode = match t.mode {
                            CompassMode::Circle => CompassMode::Arc,
                            CompassMode::Arc => CompassMode::Sector,
                            CompassMode::Sector => CompassMode::Circle,
                        };
                    }
                    // 双击 / Enter 提交。
                    if double_clicked || enter {
                        let r = t.radius();
                        if r > 2.0 {
                            let rect = egui::Rect::from_center_size(
                                t.pivot,
                                egui::vec2(r * 2.0, r * 2.0),
                            );
                            match t.mode {
                                CompassMode::Circle => {
                                    commit = Some((ShapeKind::Circle, rect, None, false, false));
                                }
                                CompassMode::Arc | CompassMode::Sector => {
                                    let end = angle_of(t.pivot, t.pencil);
                                    let fill = t.mode == CompassMode::Sector;
                                    commit = Some((
                                        if fill { ShapeKind::Sector } else { ShapeKind::Arc },
                                        rect,
                                        Some((0.0, end)),
                                        false,
                                        fill,
                                    ));
                                }
                            }
                        }
                    }
                }
                self.active_tool = ActiveTool::Compass(t);
            }
            ActiveTool::SetSquare(mut t) => {
                // 滚轮旋转 + 吸附 30/45/60/90。
                let scroll = ctx.input(|i| i.raw_scroll_delta.y);
                if scroll != 0.0 {
                    let raw = t.rotation_deg + scroll.signum() * 15.0;
                    let (snapped, _) = snap_angle(raw, 3.0);
                    t.rotation_deg = snapped;
                }
                // 底边方向 = 直角顶点 origin → 30° 顶点，与 set_square_points 的 rotate_vec 约定一致。
                let dir = crate::tools::rotate_vec(egui::Vec2::new(1.0, 0.0), t.rotation_deg.to_radians());
                let edge_end = t.origin + dir * t.size;
                // 重心（三顶点均值）用于判定「平移」。
                let grip = set_square_centroid(&t);

                // 按下：按「顶点旋转 → 重心平移 → 沿边画线 → 整体移动」的优先级判定。
                if pressed {
                    if let Some(p) = pointer {
                        let pts = crate::tools::set_square_points(&t);
                        // 任意顶点 → 旋转（旋转中心固定为直角顶点 origin）。
                        let near_vertex = pts.iter().any(|v| p.distance(*v) < 14.0);
                        if near_vertex {
                            t.rotating = true;
                        } else if p.distance(grip) < 14.0 {
                            t.moving = true;
                        } else if let Some((edge_idx, snap_point)) = find_nearest_edge(p, &t, 8.0) {
                            // 沿离鼠标最近的那条边画线，起点吸附到边上的最近点。
                            t.drawing = true;
                            t.line_edge = Some(edge_idx);
                            t.line_start = snap_point;
                            t.line_current = snap_point;
                        } else if self.tool_hits_set_square(&t, p) {
                            t.moving = true;
                        }
                    }
                }
                // 旋转：以直角顶点 origin 为中心。`angle_of` 屏幕逆时针为正、`rotate_vec` 屏幕
                // 顺时针为正，两者符号相反，故加负号使底边跟随鼠标；Shift 吸附 15° 网格。
                if t.rotating && down {
                    if let Some(p) = pointer {
                        let raw = -angle_of(t.origin, p);
                        let shift = ctx.input(|i| i.modifiers.shift);
                        t.rotation_deg = if shift { snap_angle_grid15(raw) } else { raw };
                    }
                }
                if t.rotating && released {
                    t.rotating = false;
                }
                if t.moving && down {
                    let delta = ctx.input(|i| i.pointer.delta());
                    t.origin += delta;
                }
                // 画线中：当前点跟随鼠标，并吸附到所选边的方向（无限直线）。
                if t.drawing && down {
                    if let Some(p) = pointer {
                        t.line_current = match t.line_edge {
                            Some(e) => {
                                let (a, b) = set_square_edges(&t)[e];
                                closest_point_on_line(p, a, b)
                            }
                            None => p,
                        };
                    }
                }
                // 松手：起止距离超过 5px 才提交线段（防误触）。
                if t.drawing && released {
                    if let Some((start, end)) = line_draw_result(t.line_start, t.line_current, 5.0)
                    {
                        let rect = egui::Rect::from_two_pos(start, end);
                        let line_flipped = (start.x > end.x) != (start.y > end.y);
                        commit = Some((ShapeKind::Line, rect, None, line_flipped, false));
                    }
                    t.drawing = false;
                    t.line_edge = None;
                }
                if t.moving && released {
                    t.moving = false;
                }
                // 双击 / Enter：提交整条底边为 Line。
                if (double_clicked || enter)
                    && !t.drawing
                    && !t.moving
                    && !t.rotating
                {
                    let rect = egui::Rect::from_two_pos(t.origin, edge_end);
                    let line_flipped = (t.origin.x > edge_end.x) != (t.origin.y > edge_end.y);
                    commit = Some((ShapeKind::Line, rect, None, line_flipped, false));
                }
                self.active_tool = ActiveTool::SetSquare(t);
            }
            ActiveTool::Protractor(mut t) => {
                // 右键循环 Measure → DrawAngle → DrawArc。
                if ctx.input(|i| i.pointer.secondary_clicked()) {
                    t.mode = match t.mode {
                        ProtractorMode::Measure => ProtractorMode::DrawAngle,
                        ProtractorMode::DrawAngle => ProtractorMode::DrawArc,
                        ProtractorMode::DrawArc => ProtractorMode::Measure,
                    };
                    t.first_angle_deg = None;
                }
                // 鼠标移动测角（量角器读数 0–180），射线跟随不受平移影响。
                if let Some(p) = pointer {
                    let unified = angle_of(t.center, p);
                    t.cursor_angle_deg = unified_to_protractor(unified);
                }

                // 拖拽移动：仅当鼠标落在量角器外圈（距离中心 > 半径）时激活；整体平移，不旋转。
                // 刻度区域内按下不响应（留给未来「画弧」等功能）。
                if pressed {
                    if let Some(p) = pointer {
                        if p.distance(t.center) > t.radius {
                            t.dragging = true;
                            t.last_mouse = p;
                        }
                    }
                }
                if t.dragging && down {
                    if let Some(p) = pointer {
                        let delta = p - t.last_mouse;
                        t.center += delta;
                        t.last_mouse = p;
                    }
                }
                if t.dragging && released {
                    t.dragging = false;
                }

                // DrawAngle：点两次确定两条边（拖拽移动中不响应点边）。角度不再带旋转偏移。
                if t.mode == ProtractorMode::DrawAngle && !t.dragging {
                    if pressed {
                        match t.first_angle_deg {
                            None => t.first_angle_deg = Some(t.cursor_angle_deg),
                            Some(first) => {
                                let a0 = protractor_to_unified(first);
                                let a1 = protractor_to_unified(t.cursor_angle_deg);
                                let rect = egui::Rect::from_center_size(
                                    t.center,
                                    egui::vec2(t.radius * 2.0, t.radius * 2.0),
                                );
                                commit = Some((ShapeKind::Angle, rect, Some((a0, a1)), false, false));
                            }
                        }
                    }
                } else if (double_clicked || enter) && !t.dragging {
                    // Measure / DrawArc：双击提交弧（0° → 当前角度），center 存入元素数据。
                    let rect = egui::Rect::from_center_size(
                        t.center,
                        egui::vec2(t.radius * 2.0, t.radius * 2.0),
                    );
                    let a0 = 0.0;
                    let a1 = protractor_to_unified(t.cursor_angle_deg);
                    commit = Some((ShapeKind::Arc, rect, Some((a0, a1)), false, false));
                }
                self.active_tool = ActiveTool::Protractor(t);
            }
            ActiveTool::Ruler(mut t) => {
                // 按下优先级：端点（<10px）→ 主体（<8px）。
                if pressed {
                    if let Some(p) = pointer {
                        if p.distance(t.start) < 10.0 {
                            t.dragging_end = Some(WhichEnd::Start);
                        } else if p.distance(t.end) < 10.0 {
                            t.dragging_end = Some(WhichEnd::End);
                        } else if dist_to_segment(p, t.start, t.end).0 < 8.0 {
                            t.dragging_body = true;
                            t.last_mouse = p;
                        }
                    }
                }
                // 拖端：更新端点；Shift 把方向吸附到 45° 网格（含水平/竖直）。
                if let Some(which) = t.dragging_end {
                    if down {
                        if let Some(p) = pointer {
                            let shift = ctx.input(|i| i.modifiers.shift);
                            match which {
                                WhichEnd::Start => {
                                    let v = p - t.end;
                                    t.start = if shift { t.end + snap_dir_grid45(v) } else { p };
                                }
                                WhichEnd::End => {
                                    let v = p - t.start;
                                    t.end = if shift { t.start + snap_dir_grid45(v) } else { p };
                                }
                            }
                        }
                    }
                    if released {
                        t.dragging_end = None;
                    }
                }
                // 拖主体：整体平移。
                if t.dragging_body {
                    if down {
                        if let Some(p) = pointer {
                            let delta = p - t.last_mouse;
                            t.start += delta;
                            t.end += delta;
                            t.last_mouse = p;
                        }
                    }
                    if released {
                        t.dragging_body = false;
                    }
                }
                // 双击 / Enter 提交为直线。
                if (double_clicked || enter)
                    && t.dragging_end.is_none()
                    && !t.dragging_body
                    && t.start.distance(t.end) > 5.0
                {
                    let rect = egui::Rect::from_two_pos(t.start, t.end);
                    let line_flipped = (t.start.x > t.end.x) != (t.start.y > t.end.y);
                    commit = Some((ShapeKind::Line, rect, None, line_flipped, false));
                }
                self.active_tool = ActiveTool::Ruler(t);
            }
            ActiveTool::Polygon(mut t) => {
                // 滚轮旋转 + Q/E 步进旋转。
                let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    t.preview_angle += scroll * 2.0;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Q)) {
                    t.preview_angle -= 15.0;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::E)) {
                    t.preview_angle += 15.0;
                }

                if t.radius <= 0.0 {
                    // 第一阶段：第一次点击确定中心。
                    if pressed {
                        if let Some(p) = pointer {
                            t.center = p;
                            t.radius = 1.0;
                        }
                    }
                } else {
                    // 第二阶段：拖拽确定半径。
                    if down {
                        if let Some(p) = pointer {
                            t.radius = t.center.distance(p);
                        }
                    }
                    // 双击 / Enter 提交为正多边形元素。
                    if (double_clicked || enter) && t.radius > 2.0 {
                        let center = [t.center.x, t.center.y];
                        let rect = egui::Rect::from_center_size(
                            t.center,
                            egui::vec2(t.radius * 2.0, t.radius * 2.0),
                        );
                        commit = Some((
                            ShapeKind::Polygon { center, radius: t.radius, sides: t.sides },
                            rect,
                            None,
                            false,
                            true,
                        ));
                    }
                }
                self.active_tool = ActiveTool::Polygon(t);
            }
            ActiveTool::FunctionPlot(mut t) => {
                // 表达式输入框 + 实时解析预览；Enter / 提交按钮 → 提交为 SvgShape。
                let mut submit = false;
                egui::Window::new("📈 函数绘图")
                    .id(egui::Id::new("function_plot_editor"))
                    .default_pos(t.center + egui::vec2(24.0, 24.0))
                    .show(ctx, |ui| {
                        ui.label("输入函数（y = 2x + 1、sin(x)、x^2）：");
                        let resp = ui.text_edit_singleline(&mut t.expr_str);
                        // 解析：去掉可能的 "y=" 前缀。
                        let raw = t.expr_str.trim();
                        let cleaned = raw
                            .strip_prefix("y=")
                            .or_else(|| raw.strip_prefix("Y="))
                            .unwrap_or(raw)
                            .trim();
                        match crate::function_parser::parse(cleaned) {
                            Ok(e) => {
                                t.parsed = Some(e);
                                t.error = None;
                            }
                            Err(err) => {
                                t.parsed = None;
                                t.error = Some(err);
                            }
                        }
                        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.button("提交到画布").clicked() || (resp.lost_focus() && enter) {
                            submit = true;
                        }
                        if let Some(err) = &t.error {
                            ui.colored_label(egui::Color32::from_rgb(220, 60, 60), err);
                        }
                        ui.small("Enter 提交 · Esc 取消 · 支持 sin/cos/tan");
                    });
                if submit {
                    if let Some(e) = t.parsed.clone() {
                        self.commit_function_plot(&e, t.center, t.scale, &t.expr_str, ctx);
                    }
                    self.active_tool = ActiveTool::None;
                    self.last_tool_click = None;
                } else {
                    self.active_tool = ActiveTool::FunctionPlot(t);
                }
            }
            ActiveTool::NumberLine(mut t) => {
                // 第一次点击定起点 → 拖拽实时预览（Shift 吸附水平/垂直）→ 松开提交。
                if pressed && t.start.is_none() {
                    if let Some(p) = pointer {
                        t.start = Some(p);
                        t.current = Some(p);
                        t.dragging = true;
                    }
                }
                if t.dragging && down {
                    if let Some(p) = pointer {
                        let shift = ctx.input(|i| i.modifiers.shift);
                        if let Some(s) = t.start {
                            let v = p - s;
                            // Shift：吸附到水平（0°）/ 垂直（90°）。
                            t.current = Some(if shift { s + snap_dir_axis(v) } else { p });
                        }
                    }
                }
                if t.dragging && released {
                    if let (Some(s), Some(c)) = (t.start, t.current) {
                        if s.distance(c) > 5.0 {
                            let rect = egui::Rect::from_two_pos(s, c);
                            let data = drafftink_core::model::NumberLineData {
                                start: [s.x, s.y],
                                end: [c.x, c.y],
                                step: t.step,
                                ..drafftink_core::model::NumberLineData::default()
                            };
                            commit = Some((
                                ShapeKind::NumberLine(data),
                                rect,
                                None,
                                false,
                                false,
                            ));
                        }
                    }
                    t.dragging = false;
                    t.start = None;
                    t.current = None;
                }
                self.active_tool = ActiveTool::NumberLine(t);
            }
            ActiveTool::Countdown(mut t) => {
                if t.total_seconds == 0 && !t.is_running && !t.is_finished {
                    // ── 输入阶段：弹窗输入时间（M:SS 或纯秒）→ 确认开始。 ──
                    let mut confirm = false;
                    egui::Window::new("⏱ 倒计时设置")
                        .id(egui::Id::new("countdown_setup_win"))
                        .default_pos(t.position)
                        .collapsible(false)
                        .resizable(false)
                        .show(ctx, |ui| {
                            ui.label("时间（如 5:30 或 330 秒）：");
                            let resp = ui.text_edit_singleline(&mut t.input_text);
                            let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.button("开始").clicked() || (resp.lost_focus() && enter) {
                                confirm = true;
                            }
                            ui.small("Enter 确认 · Esc 取消");
                        });
                    if confirm {
                        match t.parse_input() {
                            Ok(()) => {
                                t.is_running = true;
                                ctx.request_repaint_after(std::time::Duration::from_millis(200));
                            }
                            Err(e) => {
                                log::warn!("[countdown] 输入无效: {e}");
                                // 输入失败：保持设置窗口（输入框已有内容可改）。
                            }
                        }
                    }
                    self.active_tool = ActiveTool::Countdown(t);
                } else {
                    // ── 显示阶段：点击数字开始/暂停、拖拽移动、点击 ✕ 关闭、每秒递减。 ──
                    let rect = t.rect();
                    let close_rect = egui::Rect::from_min_size(
                        rect.right_top() + egui::vec2(-20.0, 4.0),
                        egui::vec2(16.0, 16.0),
                    );
                    let mut close = false;

                    if pressed {
                        if let Some(p) = pointer {
                            if rect.contains(p) {
                                t.pending_press = Some(p);
                            } else if close_rect.contains(p) {
                                close = true;
                            }
                        }
                    }
                    if let Some(pp) = t.pending_press {
                        if down {
                            if let Some(p) = pointer {
                                if p.distance(pp) > 4.0 {
                                    t.dragging = true;
                                    t.last_mouse = p;
                                }
                            }
                        }
                        if released {
                            if !t.dragging {
                                // 点击数字：开始 / 暂停；到时后点击 = 重置重开。
                                if t.is_finished {
                                    t.remaining_seconds = t.total_seconds;
                                    t.is_finished = false;
                                    t.is_running = true;
                                } else if t.total_seconds > 0 {
                                    t.is_running = !t.is_running;
                                }
                                t.last_tick = None;
                            }
                            t.pending_press = None;
                            t.dragging = false;
                        }
                    }
                    // 拖拽移动计时器。
                    if t.dragging && down {
                        if let Some(p) = pointer {
                            let delta = p - t.last_mouse;
                            t.position += delta;
                            t.last_mouse = p;
                        }
                    }
                    // 每秒递减（由 Instant 驱动，避免帧率不均）。
                    if t.is_running && !t.is_finished {
                        let now = std::time::Instant::now();
                        if let Some(last) = t.last_tick {
                            if now.duration_since(last) >= std::time::Duration::from_secs(1) {
                                t.tick();
                                t.last_tick = Some(now);
                            }
                        } else {
                            t.last_tick = Some(now);
                        }
                    }
                    // 刷新率：运行中 200ms（秒递减平滑）、到 0 后 500ms（闪烁）、否则不主动。
                    if t.is_running || t.is_finished {
                        let ms = if t.is_finished { 500 } else { 200 };
                        ctx.request_repaint_after(std::time::Duration::from_millis(ms));
                    }

                    if close {
                        self.active_tool = ActiveTool::None;
                        self.last_tool_click = None;
                    } else {
                        self.active_tool = ActiveTool::Countdown(t);
                    }
                }
            }
            ActiveTool::None => {}
        }

        if let Some((kind, rect, arc_degrees, line_flipped, fill)) = commit {
            self.active_tool = ActiveTool::None;
            self.commit_shape_geom(kind, rect, arc_degrees, line_flipped, fill, ctx);
        }
    }

    /// 命中测试：指针是否落在三角尺三角形内部（含直角边延伸）。
    fn tool_hits_set_square(&self, t: &SetSquareTool, p: egui::Pos2) -> bool {
        let pts = crate::tools::set_square_points(t);
        let [a, b, c] = pts;
        // 有符号面积法判定点是否在三角形内。
        let area = |p0: egui::Pos2, p1: egui::Pos2, p2: egui::Pos2| -> f32 {
            (p1.x - p0.x) * (p2.y - p0.y) - (p1.y - p0.y) * (p2.x - p0.x)
        };
        let d1 = area(p, a, b);
        let d2 = area(p, b, c);
        let d3 = area(p, c, a);
        let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        !(has_neg && has_pos)
    }

    /// 选中态文本就地编辑（备课模式）：`TextEdit` 改字 + `RectInteraction` 移动/缩放。
    ///
    /// 非选中 / 授课模式不进入此函数——文本由文档渲染器 `render.rs::draw_text` 呈现，
    /// 只读。仅在当前选中的是 `SelectedElement::Text` 时，叠加一个 `egui::Window`
    /// 承载 `TextEdit`（就地编辑内容），并用公共 `RectInteraction` 做移动 / 缩放，
    /// 缩放后的世界坐标写回文档层元素的 `base.position` / `base.size`，保证持久化。
    fn draw_text_overlay(&mut self, ctx: &egui::Context) {
        if self.mode == AppMode::Teach {
            return;
        }
        let id = match &self.selected_element_id {
            Some(SelectedElement::Text(id)) => id.clone(),
            _ => {
                // 取消选中（点空白/点其它元素）：提交未决的文本内容编辑。
                self.commit_text_edit();
                return;
            }
        };

        let page = self.edit.multi_page.current_page;
        // 定位选中文本元素。
        let idx = if self.edit.doc.pages.is_empty() {
            self.edit
                .doc
                .elements
                .iter()
                .position(|e| e.base().id.to_string() == id)
        } else {
            self.edit
                .doc
                .pages
                .get(page)
                .and_then(|p| p.elements.iter().position(|e| e.base().id.to_string() == id))
        };
        let Some(idx) = idx else {
            return;
        };
        let elem_id = if self.edit.doc.pages.is_empty() {
            self.edit.doc.elements[idx].base().id
        } else {
            self.edit.doc.pages[page].elements[idx].base().id
        };

        let cam = self.edit.camera.clone();
        let panel_offset = egui::vec2(self.edit.canvas_offset[0], self.edit.canvas_offset[1]);

        // 当前世界矩形 → 屏幕矩形。
        let (pos, size) = if self.edit.doc.pages.is_empty() {
            let e = &self.edit.doc.elements[idx];
            (e.base().position, e.base().size)
        } else {
            let e = &self.edit.doc.pages[page].elements[idx];
            (e.base().position, e.base().size)
        };
        let tl = cam.world_to_screen([pos[0], pos[1]]) + panel_offset;
        let br = cam.world_to_screen([pos[0] + size[0], pos[1] + size[1]]) + panel_offset;
        let rect = egui::Rect::from_two_pos(tl, br);

        // RectInteraction 移动 / 缩放（复用全局拖拽守卫）。
        let sid = egui::Id::new(format!("text_{id}"));
        let was_active = self.active_drag.is_some();
        let old_elem = self.clone_text_elem(page, idx); // 拖拽前的确切旧值。
        let mut interact = RectInteraction::new(sid, rect);
        let new_rect = interact.update(ctx, &mut self.active_drag);

        // 写回世界坐标到文档元素（缩放 / 拖拽结果持久化）。
        if let Some(nr) = new_rect {
            let min_w = cam.screen_to_world(nr.min - panel_offset);
            let max_w = cam.screen_to_world(nr.max - panel_offset);
            let elem = if self.edit.doc.pages.is_empty() {
                &mut self.edit.doc.elements[idx]
            } else {
                &mut self.edit.doc.pages[page].elements[idx]
            };
            if let Element::Text(t) = elem {
                t.base.position = [min_w[0], min_w[1]];
                t.base.size = [max_w[0] - min_w[0], max_w[1] - min_w[1]];
            }
        }

        // 拖拽开始：快照旧元素（base 几何 + 文本内容）。
        if !was_active && self.active_drag.is_some() {
            self.drag_snapshot = Some(DragSnapshot::Text {
                page,
                elem_id,
                old: old_elem,
            });
        }
        // 拖拽结束：若 base 几何确有变化，push ModifyText。
        if was_active && self.active_drag.is_none() {
            if let Some(DragSnapshot::Text { page, elem_id, old }) = self.drag_snapshot.take() {
                let new = self.clone_text_elem(page, idx);
                let changed = {
                    let ob = old.base();
                    let nb = new.base();
                    ob.position != nb.position || ob.size != nb.size
                };
                if changed {
                    self.history.push(UndoCmd::ModifyText { page, elem_id, old, new });
                }
            }
        }

        // TextEdit 就地编辑内容（承载于固定位置的 Window，覆盖在文档文本之上）。
        // 用块作用域隔离 `text_ref`（&mut doc）与后续 `&mut self` 调用，避免借用冲突。
        let resp = {
            let text_ref: &mut String = if self.edit.doc.pages.is_empty() {
                match &mut self.edit.doc.elements[idx] {
                    Element::Text(t) => &mut t.text,
                    _ => return,
                }
            } else {
                match &mut self.edit.doc.pages[page].elements[idx] {
                    Element::Text(t) => &mut t.text,
                    _ => return,
                }
            };
            egui::Window::new("text_edit")
                .id(egui::Id::new(format!("text_win_{id}")))
                .fixed_rect(rect)
                .title_bar(false)
                .resizable(false)
                .show(ctx, |ui| ui.text_edit_multiline(text_ref))
                // egui 0.29 的 Window::show 返回 Option<InnerResponse<Option<R>>>：
                // and_then 展平两层 Option，得到 Response（或 None）。
                .and_then(|ir| ir.inner)
        };

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("text_overlay"),
        ));
        if let Some(r) = resp {
            // 开始编辑（首次获得焦点）：快照旧元素。
            if r.gained_focus() && self.text_edit_undo.is_none() {
                self.text_edit_undo = Some(TextEditSession {
                    page,
                    elem_id,
                    old: self.clone_text_elem(page, idx),
                });
            }
            // 结束编辑（失去焦点）：提交内容修改。
            if r.lost_focus() {
                self.commit_text_edit();
            }
        }
        interact.draw_overlay(&painter);
        ctx.request_repaint();
    }

    /// 在内容之上绘制视频叠加层（授课画布 + 测试视频），并集成暂停/播放、边框缩放
    /// grip 与内部拖拽移动交互。不阻塞 UI 线程。
    ///
    /// 交互采用 egui 指针输入（`ctx.input().pointer`）手动实现：egui 0.29 已移除
    /// `Context::interact`，叠加层无需依赖 `Ui` 即可获得完整的拖拽/光标反馈。
    fn draw_video_overlay(&mut self, ctx: &egui::Context) {
        if self.video_instances.is_empty() && self.pending_video.is_none() {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("video_overlay"),
        ));
        // 拷贝相机用于坐标变换（避免与下方可变借用冲突）。
        // 授课模式用 DisplayApp 相机（全屏，无需偏移）；备课模式改用 EditApp 相机，
        // 并补上画布在窗口中的屏幕偏移（顶栏/侧栏），否则视频会整体偏上偏左。
        let (cam, panel_offset) = if let Some(d) = self.display.as_ref() {
            (Some(d.camera.clone()), egui::Vec2::ZERO)
        } else {
            (
                Some(self.edit.camera.clone()),
                egui::vec2(self.edit.canvas_offset[0], self.edit.canvas_offset[1]),
            )
        };
        let screen = ctx.screen_rect();
        let prepare = self.mode == AppMode::Prepare;

        // ── 点击放置（pending）：选定视频文件后，半透明幽灵框跟随光标，
        //    单击画布任意处把视频固定到该位置（Esc 取消放置）。放置后视为「已固定」——
        //    不自动选中、不显示边框/抓手/控件；单击视频（含进度条区域）才选中进入微调。
        if prepare {
            if let Some(path) = self.pending_video.clone() {
                let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                let pressed = ctx.input(|i| i.pointer.primary_pressed());
                let pointer = ctx.pointer_interact_pos();
                if esc {
                    self.pending_video = None;
                    self.pending_armed = false;
                } else {
                    if let Some(pos) = pointer {
                        // 幽灵预览：以光标为中心的半透明 16:9 矩形（尺寸 = 世界默认尺寸 × zoom）。
                        let cw = self.edit.camera.viewport[0] / self.edit.camera.zoom;
                        let mut gw = 640.0_f32;
                        let mut gh = gw * 9.0 / 16.0;
                        let max_w = cw * 0.6;
                        if gw > max_w {
                            gw = max_w;
                            gh = gw * 9.0 / 16.0;
                        }
                        let zoom = cam.as_ref().map(|c| c.zoom).unwrap_or(1.0);
                        let ghost_rect = egui::Rect::from_center_size(
                            pos,
                            egui::vec2(gw * zoom, gh * zoom),
                        );
                        painter.rect_filled(
                            ghost_rect,
                            4.0,
                            egui::Color32::from_rgba_unmultiplied(0, 150, 255, 40),
                        );
                        let gcol = egui::Color32::from_rgba_unmultiplied(0, 150, 255, 220);
                        let gwb = 2.0;
                        painter.line_segment(
                            [ghost_rect.left_top(), ghost_rect.right_top()],
                            egui::Stroke::new(gwb, gcol),
                        );
                        painter.line_segment(
                            [ghost_rect.right_top(), ghost_rect.right_bottom()],
                            egui::Stroke::new(gwb, gcol),
                        );
                        painter.line_segment(
                            [ghost_rect.right_bottom(), ghost_rect.left_bottom()],
                            egui::Stroke::new(gwb, gcol),
                        );
                        painter.line_segment(
                            [ghost_rect.left_bottom(), ghost_rect.left_top()],
                            egui::Stroke::new(gwb, gcol),
                        );
                        painter.text(
                            pos + egui::vec2(0.0, gh * zoom / 2.0 + 6.0),
                            egui::Align2::CENTER_TOP,
                            "点击放置视频",
                            egui::FontId::proportional(12.0),
                            egui::Color32::WHITE,
                        );
                    }
                    if self.pending_armed && pressed {
                        if let Some(pos) = pointer {
                            let local = pos - panel_offset;
                            let world = cam
                                .as_ref()
                                .map(|c| c.screen_to_world(local))
                                .unwrap_or([pos.x, pos.y]);
                            self.pending_video = None;
                            self.pending_armed = false;
                            self.insert_video_at(&path, world, ctx);
                            // 刚放置的视频以「固定」状态呈现：清空选中，避免边框/抓手
                            // 立即挡住光标或遮挡视频，影响老师插入下一个素材。
                            self.selected_element_id = None;
                        }
                    } else if !self.pending_armed {
                        // 跳过文件对话框那一下点击，下一帧起才允许放置。
                        self.pending_armed = true;
                    }
                }
            }
        }

        // 指针状态：仅暂停/静音按钮 + 进度条需要（缩放/移动由 RectInteraction 内部读取 ctx）。
        let pointer_pos = ctx.pointer_interact_pos();
        let pointer_pressed = ctx.input(|i| i.pointer.primary_pressed());
        let pointer_down = ctx.input(|i| i.pointer.primary_down());
        let space_toggle = ctx.input(|i| i.key_pressed(egui::Key::Space));
        let mute_toggle = ctx.input(|i| i.key_pressed(egui::Key::M));

        const BTN_R: f32 = 12.0; // 圆形按钮半径 → 24×24

        let keys: Vec<String> = self.video_instances.keys().cloned().collect();
        let cur_page = self.current_canvas_page();

        // 1) 先计算每个视频的屏幕矩形与进度条命中区（用于选中 / 取消选中判定）。
        //    仅收集「所属页 == 当前页」的视频，翻页后其它页的视频不再渲染 / 命中。
        let mut infos: Vec<(String, egui::Rect, Option<egui::Rect>, u64)> =
            Vec::with_capacity(keys.len());
        for key in &keys {
            let inst = match self.video_instances.get(key) {
                Some(i) => i,
                None => continue,
            };
            if inst.page != cur_page {
                continue;
            }
            let base_rect = if let Some(wr) = inst.world_rect {
                match &cam {
                    Some(c) => {
                        let tl = c.world_to_screen([wr.min.x, wr.min.y]) + panel_offset;
                        let br = c.world_to_screen([wr.max.x, wr.max.y]) + panel_offset;
                        egui::Rect::from_two_pos(tl, br)
                    }
                    None => default_overlay_rect(screen),
                }
            } else {
                default_overlay_rect(screen)
            };
            let rect = inst.user_rect.unwrap_or(base_rect);
            // 进度条位于视频矩形下方 8px（高 6px），命中区竖直放宽到 ±8px；
            // 选中判定把进度条区域算作「视频本体」，避免点击进度条被误判为取消选中。
            let bar_hit = if inst.duration_ms > 0 {
                Some(Self::video_seek_hit_rect(rect))
            } else {
                None
            };
            infos.push((key.clone(), rect, bar_hit, inst.z_index));
        }
        // 按 z_index 升序排序：后插入的（值大）后绘制，渲染在上层。
        infos.sort_by_key(|(_, _, _, z)| *z);

        // 2) 选中 / 取消选中已统一由 `handle_canvas_click`（全局单击处理器）负责：
        //    单击视频矩形（或进度条命中带）→ 选中（边框/抓手 + 控件）；单击空白处 → 取消选中。
        //    此处不再内联判定，保证三套叠加层共用同一选中态。

        // 3) 逐实例绘制。未选中 → 仅渲染视频帧（无边框/抓手、无控件）；选中 → 完整交互。
        for (key, rect, _bar_hit, _z) in infos {
            let inst = match self.video_instances.get_mut(&key) {
                Some(i) => i,
                None => continue,
            };
            let selected = self.selected_element_id == Some(SelectedElement::Video(key.clone()));

            // 3.0) 进度刷新（拖动期间不覆盖 current_ms，使手柄稳定贴住拖动目标）。
            if !inst.seeking {
                if let Some(p) = inst.player.as_mut() {
                    if let Some(ms) = p.poll_progress_ms() {
                        inst.current_ms = ms;
                    }
                }
            }
            // 3.1) 音视频同步心跳（内部自节流 500ms）。
            if let Some(p) = inst.player.as_mut() {
                p.sync_tick();
            }

            // 3.2) 取最新帧（暂停时通道无新帧 → 保留上一帧纹理，不闪烁）。
            //      拖动进度条期间冻结画面：不取帧、不更新纹理——进度条/时间戳仍跟手，
            //      画面停在拖动开始时的帧，直到松手 seek 后新位置首帧到达瞬切。
            let is_paused = inst.player.as_ref().map(|p| p.paused).unwrap_or(false);
            let is_muted = inst.player.as_ref().map(|p| p.is_muted).unwrap_or(false);
            if !inst.seeking {
                if let Some(player) = inst.player.as_mut() {
                    if let Ok(frame) = player.try_recv() {
                        // 复用已有的 TextureHandle（仅标脏重新上传，不再每帧分配新 GPU 纹理）。
                        // 旧实现每帧都调 ctx.load_texture，会反复分配 8MB 显存纹理并触发
                        // 旧纹理的垃圾回收，是「视频播放卡顿」的主因；改用 .set() 复用同一
                        // 张纹理后，每帧仅做一次 GPU 上传，流畅度显著回升。
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [frame.width as usize, frame.height as usize],
                            &frame.rgba,
                        );
                        match &mut inst.last_tex {
                            Some(tex) => tex.set(image, egui::TextureOptions::default()),
                            None => {
                                inst.last_tex = Some(ctx.load_texture(
                                    format!("video_{key}"),
                                    image,
                                    egui::TextureOptions::default(),
                                ));
                            }
                        }
                    }
                }
            }

            // 3.3) 绘制视频帧（解码失败 → 红色占位，零 panic 兜底）。
            if let Some(tex) = &inst.last_tex {
                painter.image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            } else {
                painter.rect_filled(rect, 0.0, egui::Color32::RED);
                log::warn!("[video] VideoPlayer unavailable for {key} — placeholder shown");
            }

            // 3.4) 仅选中态：边框 + 抓手 + 暂停/静音按钮 + 进度条。
            if prepare && selected {
                let pause_center = egui::pos2(rect.max.x - 16.0, rect.min.y + 16.0);
                let has_audio = inst.player.as_ref().map(|p| p.has_audio()).unwrap_or(false);
                let mute_center = egui::pos2(pause_center.x - 36.0, pause_center.y);
                let btn_size = egui::vec2(BTN_R * 2.0, BTN_R * 2.0);
                let pause_rect = egui::Rect::from_center_size(pause_center, btn_size);
                let mute_rect = egui::Rect::from_center_size(mute_center, btn_size);
                let on_button = pointer_pos.is_some_and(|p| {
                    pause_rect.contains(p) || (has_audio && mute_rect.contains(p))
                });

                // 8 方向缩放 + 内部拖拽（复用 RectInteraction）；命中按钮时跳过避免冲突。
                let vid = egui::Id::new(format!("video_{key}"));
                let (r, interact) = if on_button {
                    (rect, RectInteraction::new(vid, rect))
                } else {
                    Self::overlay_rect_drag(
                        &mut self.active_drag,
                        &mut self.drag_snapshot,
                        &mut self.history,
                        SelectedElement::Video(key.clone()),
                        vid,
                        rect,
                        &mut inst.user_rect,
                        ctx,
                    )
                };
                // 拖拽后让 rect 跟随新位置，使视频帧、边框、按钮三者对齐。
                let rect = r;

                // 暂停/静音按钮点击（仅选中态可见、可点，与「先隐身后选中」一致）。
                if pointer_pressed {
                    if let Some(pos) = pointer_pos {
                        if pause_rect.contains(pos) {
                            if let Some(p) = inst.player.as_mut() {
                                p.toggle_paused();
                            }
                        } else if has_audio && mute_rect.contains(pos) {
                            if let Some(p) = inst.player.as_mut() {
                                p.toggle_muted();
                            }
                        }
                    }
                }

                // ── 进度条 ────────────────────────────────────────────────────
                let dur = inst.duration_ms;
                if dur > 0 {
                    let bar_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.left(), rect.bottom() + 8.0),
                        egui::vec2(rect.width(), 6.0),
                    );
                    let bar_hit_rect = bar_rect.expand(8.0);

                    if let Some(pos) = pointer_pos {
                        if inst.seeking {
                            if pointer_down {
                                // 拖动中：只做廉价 UI 反馈（进度条/时间戳跟手），绝不重启
                                // 解码进程。旧实现按 16ms 节流发真实 seek，叠加播放器内部
                                // 「首帧到达→pending 再重启」的链式循环，整个拖动期间以首帧
                                // 延迟为周期不停杀/起 ffmpeg（每次还伴随 UI 线程同步等待），
                                // 进程风暴把 CPU 打满——这才是「拖动还是很卡」的根因。
                                let ratio =
                                    ((pos.x - bar_rect.left()) / bar_rect.width()).clamp(0.0, 1.0);
                                inst.seek_target_ms = (ratio * dur as f32) as u64;
                                inst.current_ms = inst.seek_target_ms; // 立即 UI 反馈：仅移动进度条，廉价
                                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                            } else {
                                // 释放：一次性精确定位（唯一一次进程重启；音频管线复用
                                // new 时缓存的音轨信息，UI 线程零 ffprobe 等待）。
                                // 单击进度条同样走此路径，跳转依然即时。
                                inst.seeking = false;
                                if let Some(p) = inst.player.as_mut() {
                                    p.seek(inst.seek_target_ms);
                                }
                            }
                        } else if pointer_pressed && bar_hit_rect.contains(pos) {
                            // 拖动开始：清空预加载缓冲并冻结画面（配合 3.2 的取帧门控），
                            // 整个拖动期间零进程重启、零 ffprobe——拖动全程 60fps 跟手。
                            inst.seeking = true;
                            let ratio =
                                ((pos.x - bar_rect.left()) / bar_rect.width()).clamp(0.0, 1.0);
                            inst.seek_target_ms = (ratio * dur as f32) as u64;
                            inst.current_ms = inst.seek_target_ms;
                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                            if let Some(p) = inst.player.as_mut() {
                                p.begin_scrub();
                            }
                        }
                    }

                    let progress = (inst.current_ms as f32 / dur as f32).clamp(0.0, 1.0);
                    painter.rect_filled(
                        bar_rect,
                        3.0,
                        egui::Color32::from_rgba_unmultiplied(80, 80, 80, 150),
                    );
                    let played_rect = egui::Rect::from_min_max(
                        bar_rect.min,
                        egui::pos2(bar_rect.left() + bar_rect.width() * progress, bar_rect.bottom()),
                    );
                    painter.rect_filled(played_rect, 3.0, egui::Color32::from_rgb(0, 150, 255));
                    let handle_x = bar_rect.left() + bar_rect.width() * progress;
                    let handle_pos = egui::pos2(handle_x, bar_rect.center().y);
                    let handle_color = if inst.seeking {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_gray(200)
                    };
                    painter.circle_filled(handle_pos, 6.0, handle_color);
                    painter.text(
                        bar_rect.min - egui::vec2(0.0, 16.0),
                        egui::Align2::LEFT_BOTTOM,
                        format!("{}/{}", fmt_time(inst.current_ms), fmt_time(dur)),
                        egui::FontId::proportional(12.0),
                        egui::Color32::WHITE,
                    );
                }

                // 暂停/播放 + 静音按钮绘制。
                let disc = egui::Color32::from_rgba_unmultiplied(20, 20, 20, 180);
                let white = egui::Color32::WHITE;
                painter.circle_filled(pause_center, BTN_R, disc);
                if is_paused {
                    let tri = vec![
                        pause_center + egui::vec2(-3.0, -5.0),
                        pause_center + egui::vec2(-3.0, 5.0),
                        pause_center + egui::vec2(4.0, 0.0),
                    ];
                    painter.add(egui::Shape::convex_polygon(tri, white, egui::Stroke::NONE));
                } else {
                    for dx in [-3.0_f32, 3.0] {
                        painter.rect_filled(
                            egui::Rect::from_center_size(
                                pause_center + egui::vec2(dx, 0.0),
                                egui::vec2(3.0, 10.0),
                            ),
                            1.0,
                            white,
                        );
                    }
                }
                if has_audio {
                    painter.circle_filled(mute_center, BTN_R, disc);
                    painter.rect_filled(
                        egui::Rect::from_center_size(
                            mute_center + egui::vec2(-4.0, 0.0),
                            egui::vec2(4.0, 6.0),
                        ),
                        0.0,
                        white,
                    );
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            mute_center + egui::vec2(-2.0, -5.0),
                            mute_center + egui::vec2(-2.0, 5.0),
                            mute_center + egui::vec2(2.0, 0.0),
                        ],
                        white,
                        egui::Stroke::NONE,
                    ));
                    if is_muted {
                        painter.line_segment(
                            [
                                mute_center + egui::vec2(-6.0, -6.0),
                                mute_center + egui::vec2(6.0, 6.0),
                            ],
                            egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 80, 80)),
                        );
                    } else {
                        for (i, len) in [(3.0_f32, 3.0_f32), (5.5, 5.0)] {
                            painter.line_segment(
                                [
                                    mute_center + egui::vec2(i, -len * 0.5),
                                    mute_center + egui::vec2(i, len * 0.5),
                                ],
                                egui::Stroke::new(1.5, white),
                            );
                        }
                    }
                }

                // 边框 + 角 grip + 边高亮（悬停/拖拽变亮蓝），由公共组件绘制。
                interact.draw_overlay(&painter);
            }

            // 3.5) 键盘：空格暂停/播放、M 静音（作用于全部实例，保持全局便捷性）。
            if space_toggle {
                if let Some(p) = inst.player.as_mut() {
                    p.toggle_paused();
                }
            }
            if mute_toggle {
                if let Some(p) = inst.player.as_mut() {
                    p.toggle_muted();
                }
            }
        }

        ctx.request_repaint();
    }
}

/// 把内嵌视频字节写成临时文件，供 FFmpeg 按内容探测封装格式后解码。
fn make_temp_video_file(bytes: &[u8], resource_id: &str) -> Option<std::path::PathBuf> {
    let tmp = std::env::temp_dir().join(format!("drafftink_video_{resource_id}.bin"));
    std::fs::write(&tmp, bytes).ok()?;
    Some(tmp)
}

impl App for IntegratedApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.set_theme(egui::ThemePreference::Dark);

        // 翻页检测：当前页与上一帧记录的页不同 → 清空跨页选中态，
        // 否则旧页残留的选中边框 / 抓手仍叠加在新页画布上（表现为「新页残留旧内容」）。
        let cp = self.current_canvas_page();
        if cp != self.last_page {
            self.selected_element_id = None;
            self.last_page = cp;
        }

        // 全局快捷键：F5 备课 → 授课
        if self.mode == AppMode::Prepare && ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            self.enter_teach(ctx);
        }

        // 全局快捷键：F12 呼出性能 / 内存监控面板
        if ctx.input(|i| i.key_pressed(egui::Key::F12)) {
            self.show_profiler = !self.show_profiler;
        }

        // 全局快捷键（备课模式）：L 加载 ENBX 课件并渲染其中的 SvgShape，
        // 或（取消对话框时）注入一组内置演示形状，便于验证 SvgShape 渲染器。
        if self.mode == AppMode::Prepare && ctx.input(|i| i.key_pressed(egui::Key::L)) {
            self.load_svg_shape_demo(ctx);
        }

        // 全局快捷键：V 弹出视频文件选择框（与顶边栏「🎬 多媒体」按钮共用同一入口）。
        if ctx.input(|i| i.key_pressed(egui::Key::V)) {
            self.request_video_pick(ctx);
        }
        // 全局快捷键：I 弹出图片文件选择框（与顶边栏「🖼 图片」按钮共用同一入口）。
        if ctx.input(|i| i.key_pressed(egui::Key::I)) {
            self.request_image_pick(ctx);
        }

        // 全局快捷键：Ctrl+S 导出 ENBX（与顶边栏「💾 保存」同一入口）。
        if self.mode == AppMode::Prepare
            && ctx.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.ctrl && !i.modifiers.shift)
        {
            self.edit.enbx_save_requested = true;
        }

        // 视频叠加层键盘缩放：= 放大 / - 缩小（围绕当前矩形中心调整 user_rect，
        // 宽高独立不锁比例；与拖拽缩放共用最小尺寸保护）。不影响解码内存。
        if !self.video_instances.is_empty() {
            let factor = if ctx.input(|i| i.key_pressed(egui::Key::Equals)) {
                1.1
            } else if ctx.input(|i| i.key_pressed(egui::Key::Minus)) {
                1.0 / 1.1
            } else {
                1.0
            };
            if factor != 1.0 {
                let screen = ctx.screen_rect();
                for inst in self.video_instances.values_mut() {
                    let cur = inst.user_rect.unwrap_or_else(|| default_overlay_rect(screen));
                    let new_size = cur.size() * factor;
                    let new_rect = egui::Rect::from_center_size(cur.center(), new_size);
                    if new_rect.width() >= 40.0 && new_rect.height() >= 30.0 {
                        inst.user_rect = Some(new_rect);
                    }
                }
                ctx.request_repaint();
            }
        }

        match self.mode {
            AppMode::Prepare => {
                self.edit.update(ctx, frame);
                if self.edit.teach_requested {
                    self.edit.teach_requested = false;
                    self.enter_teach(ctx);
                }
                // 用户主动保存 → 把批注层（及内容层）落盘，避免频繁 IO。
                if self.edit.save_requested {
                    self.edit.save_requested = false;
                    self.edit.flush_annotations_to_doc();
                }

                // 顶边栏「🎬 多媒体」按钮 → 后台线程弹文件选择框（不阻塞 UI）。
                if self.edit.media_pick_requested {
                    self.edit.media_pick_requested = false;
                    self.request_video_pick(ctx);
                }
                // 顶边栏「🖼 图片」按钮 → 后台线程弹文件选择框（不阻塞 UI）。
                if self.edit.image_pick_requested {
                    self.edit.image_pick_requested = false;
                    self.request_image_pick(ctx);
                }
                // 顶边栏「🔷 形状」→「➕ 插入」按钮：进入「点击放置」模式——
                // 半透明幽灵形状跟随光标，单击画布任意处将其固定到该位置（Esc 取消）。
                if let Some(kind) = self.edit.shape_insert_requested.take() {
                    self.pending_shape = Some(kind);
                    self.pending_armed = false;
                }

                // 顶边栏「💾 保存」按钮 或 Ctrl+S → 弹出 .enbx 保存对话框并导出课件。
                let ctrl_s = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S));
                if self.edit.enbx_save_requested || ctrl_s {
                    self.edit.enbx_save_requested = false;
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("ENBX 课件", &["enbx"])
                        .save_file()
                    {
                        match crate::save::save_enbx(self, &path) {
                            Ok(()) => log::info!("[enbx] 已保存: {}", path.display()),
                            Err(e) => log::error!("[enbx] 保存失败: {e}"),
                        }
                    }
                }

                // 顶边栏「T 文本」按钮 → 画布中心插入默认文本框并选中。
                if let Some(()) = self.edit.text_insert_requested.take() {
                    self.insert_text_at(ctx);
                }
                // 顶边栏「🎵 音频」按钮 → 后台线程弹文件选择框（不阻塞 UI）。
                if self.edit.audio_pick_requested {
                    self.edit.audio_pick_requested = false;
                    self.request_audio_pick(ctx);
                }
                // 顶边栏「📐 教具」下拉 → 激活对应虚拟教具覆盖层。
                if let Some(kind) = self.edit.tool_requested.take() {
                    match kind {
                        TeachingToolKind::Compass => self.activate_compass(ctx),
                        TeachingToolKind::SetSquare30 => {
                            self.activate_set_square(SetSquareKind::Triangle30_60_90, ctx)
                        }
                        TeachingToolKind::SetSquare45 => {
                            self.activate_set_square(SetSquareKind::Triangle45_45_90, ctx)
                        }
                        TeachingToolKind::Protractor => {
                            self.activate_protractor(ProtractorMode::Measure, ctx)
                        }
                        TeachingToolKind::Ruler => self.activate_ruler(ctx),
                        TeachingToolKind::Polygon(sides) => self.activate_polygon(sides, ctx),
                        TeachingToolKind::FunctionPlot => self.activate_function_plot(ctx),
                        TeachingToolKind::NumberLine => self.activate_number_line(ctx),
                    }
                }
            }
            AppMode::Teach => {
                if let Some(display) = self.display.as_mut() {
                    display.update(ctx, frame);
                    if display.exit_to_prepare {
                        display.exit_to_prepare = false;
                        self.exit_teach(ctx);
                    }
                }
                // 授课模式教具交互（倒计时器 / 直尺 / 圆规等均可交互，不只读）。
                self.update_active_tool(ctx);
                // 左上角授课工具面板。
                self.teach_tools_panel(ctx);
                // 随机点名器浮动窗口（授课模式，名单临时保留、不序列化）。
                self.render_name_picker(ctx);
            }
        }

        // 消费后台文件选择框的结果：把选定视频插入当前页并启动叠加层。
        self.consume_pending_videos(ctx);
        // 消费后台图片文件选择框的结果：把选定图片插入当前页并建立叠加层。
        self.consume_pending_images(ctx);
        // 消费后台音频文件选择框的结果：把选定音频插入当前页并建立控制条。
        self.consume_pending_audios(ctx);

        // 统一画布单击处理：单击元素 → 选中；单击空白处 → 取消选中（「先隐身后选中」范式）。
        // 在叠加层绘制前求值，使本帧选中态立即被叠加层读取，从而显示边框 / 抓手。
        if self.mode == AppMode::Prepare {
            let (pressed, modifiers) =
                ctx.input(|i| (i.pointer.primary_pressed(), i.modifiers));
            if pressed {
                if let Some(pos) = ctx.pointer_interact_pos() {
                    let screen = ctx.screen_rect();
                    self.handle_canvas_click(pos, screen, &modifiers);
                }
            }
        }

        // 矩形框选 → 命中单个文本 → 弹出「函数绘图」菜单（与单击选中逻辑共存：
        // 单击空白处取消选中并开启框选，框住单个文本后在框下方弹菜单）。
        if self.mode == AppMode::Prepare {
            self.update_marquee(ctx);
            self.render_function_menu(ctx);
        }

        // Delete / Backspace 删除选中元素（文本编辑框聚焦时不触发，避免误删正在输入的字符）。
        if self.mode == AppMode::Prepare {
            let focused = ctx.memory(|m| m.focused().is_some());
            let del = ctx.input(|i| i.key_pressed(egui::Key::Delete));
            let bksp = ctx.input(|i| i.key_pressed(egui::Key::Backspace));
            if (del || bksp) && !focused {
                self.delete_selected(ctx);
            }
        }

        // Undo / Redo：Ctrl+Z / Ctrl+Y（文本编辑框聚焦时忽略，避免与输入撤销冲突）。
        if self.mode == AppMode::Prepare {
            let focused = ctx.memory(|m| m.focused().is_some());
            if !focused {
                let (undo, redo) = ctx.input(|i| {
                    (
                        i.modifiers.ctrl && i.key_pressed(egui::Key::Z) && !i.modifiers.shift,
                        i.modifiers.ctrl && i.key_pressed(egui::Key::Y),
                    )
                });
                if undo {
                    if let Some(cmd) = self.history.undo() {
                        self.apply_undo(cmd, ctx);
                    }
                } else if redo {
                    if let Some(cmd) = self.history.redo() {
                        self.apply_redo(cmd, ctx);
                    }
                }
            }
        }

        // 虚拟教具：快捷键（C=圆规 / T=三角尺 / P=量角器 / R=直尺）+ 每帧交互（拖拽/提交）。
        if self.mode == AppMode::Prepare {
            let focused = ctx.memory(|m| m.focused().is_some());
            if !focused {
                let (c, t, p, r) = ctx.input(|i| {
                    (
                        i.key_pressed(egui::Key::C),
                        i.key_pressed(egui::Key::T),
                        i.key_pressed(egui::Key::P),
                        i.key_pressed(egui::Key::R),
                    )
                });
                if c {
                    self.activate_compass(ctx);
                } else if t {
                    self.activate_set_square(SetSquareKind::Triangle30_60_90, ctx);
                } else if p {
                    self.activate_protractor(ProtractorMode::Measure, ctx);
                } else if r {
                    self.activate_ruler(ctx);
                }
            }
            self.update_active_tool(ctx);
        }

        // 视频叠加层（授课画布 / 测试视频）绘制在内容之上，不阻塞 UI。
        self.draw_video_overlay(ctx);
        // 图片叠加层绘制在内容之上（与视频同层、同交互组件），不阻塞 UI。
        self.draw_image_overlay(ctx);
        // 形状叠加层绘制在内容之上（与视频/图片同层、同交互组件），不阻塞 UI。
        self.draw_shape_overlay(ctx);
        // 函数绘图叠加层绘制在内容之上（与形状/视频/图片同层、同交互组件）。
        self.draw_function_overlay(ctx);
        // 音频叠加层（控制条）绘制在内容之上，与视频/图片/形状同层。
        self.draw_audio_overlay(ctx);
        // 选中文本的就地编辑叠加层（备课模式；授课模式在 update 中已早退）。
        self.draw_text_overlay(ctx);
        // 虚拟教具覆盖层（最上层，覆盖在所有元素之上）。
        if !matches!(self.active_tool, ActiveTool::None) {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("active_tool_overlay"),
            ));
            let time = ctx.input(|i| i.time);
            draw_active_tool(&painter, &self.active_tool, time);
        }

        // 放大镜叠加层（置于所有元素 / 教具之上，但低于监控面板）；仅激活时生效。
        self.update_magnifier(ctx);

        // 监控面板置于最上层，两种模式下都可用。
        if self.show_profiler {
            self.profiler_window(ctx);
        }
    }
}

/// 内置演示形状：当 `L` 快捷键的文件对话框被取消时注入，便于不依赖外部 `.enbx`
/// 文件即可验证 SvgShape 渲染器（箭头 / 椭圆 / 星形 / 三角 / 矩形，含闭合与描边）。
// 演示数据用 push 初始化更直观；该 lint 仅样式层面，对本演示函数放宽。
#[allow(clippy::vec_init_then_push)]
fn demo_svg_shapes() -> Vec<Element> {
    use egui::Color32;

    let mut shapes: Vec<Element> = Vec::new();

    // 1) 带端箭头的直线（开放路径，仅描边）
    shapes.push(Element::SvgShape(SvgShapeElement {
        base: BaseElement {
            position: [80.0, 60.0],
            size: [300.0, 60.0],
            stroke_color: Color32::BLACK,
            stroke_width: 3.0,
            ..Default::default()
        },
        svg_path: "M0,30 L300,30".to_string(),
        is_closed: false,
        has_end_arrow: true,
        has_start_arrow: false,
    }));

    // 2) 填充椭圆（闭合贝塞尔路径）
    shapes.push(Element::SvgShape(SvgShapeElement {
        base: BaseElement {
            position: [80.0, 180.0],
            size: [200.0, 140.0],
            fill_color: Color32::from_rgb(220, 80, 80),
            stroke_color: Color32::BLACK,
            stroke_width: 3.0,
            ..Default::default()
        },
        svg_path: "M100,0 C155,0 200,31 200,70 C200,109 155,140 100,140 C45,140 0,109 0,70 C0,31 45,0 100,0 Z".to_string(),
        is_closed: true,
        has_end_arrow: false,
        has_start_arrow: false,
    }));

    // 3) 填充五角星（闭合多边形）
    shapes.push(Element::SvgShape(SvgShapeElement {
        base: BaseElement {
            position: [340.0, 180.0],
            size: [200.0, 200.0],
            fill_color: Color32::from_rgb(240, 200, 60),
            stroke_color: Color32::BLACK,
            stroke_width: 3.0,
            ..Default::default()
        },
        svg_path: "M100,0 L124,76 L200,76 L138,120 L162,196 L100,152 L38,196 L62,120 L0,76 L76,76 Z".to_string(),
        is_closed: true,
        has_end_arrow: false,
        has_start_arrow: false,
    }));

    // 4) 填充三角形（闭合多边形）
    shapes.push(Element::SvgShape(SvgShapeElement {
        base: BaseElement {
            position: [340.0, 60.0],
            size: [100.0, 100.0],
            fill_color: Color32::from_rgb(80, 200, 120),
            stroke_color: Color32::BLACK,
            stroke_width: 3.0,
            ..Default::default()
        },
        svg_path: "M0,100 L50,0 L100,100 Z".to_string(),
        is_closed: true,
        has_end_arrow: false,
        has_start_arrow: false,
    }));

    // 5) 填充矩形（闭合多边形）
    shapes.push(Element::SvgShape(SvgShapeElement {
        base: BaseElement {
            position: [80.0, 380.0],
            size: [200.0, 100.0],
            fill_color: Color32::from_rgb(80, 140, 240),
            stroke_color: Color32::BLACK,
            stroke_width: 3.0,
            ..Default::default()
        },
        svg_path: "M0,0 L200,0 L200,100 L0,100 Z".to_string(),
        is_closed: true,
        has_end_arrow: false,
        has_start_arrow: false,
    }));

    shapes
}

/// 启动期创建并加载一次插件管理器（备授一体全局唯一实例）。
fn load_shared_plugins() -> PluginManager {
    let plugin_dir = resolve_plugin_dir();
    log::info!("[desktop] Plugin dir: {plugin_dir:?}");
    let mut pm = PluginManager::new(plugin_dir);
    let discovered = pm.discover();
    log::info!("[desktop] Found {} plugin(s)", discovered.len());
    // Safety: plugins are trusted cdylibs compiled in the same workspace.
    unsafe {
        for path in discovered {
            if let Err(e) = pm.load(&path, &DummyContext) {
                log::error!("[desktop] Plugin load failed: {path:?} — {e}");
            }
        }
    }
    pm
}

/// 解析插件目录（exe 上溯 / cwd / APPDATA），与 display 既有逻辑一致。
fn resolve_plugin_dir() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let mut candidate = exe;
        for _ in 0..4 {
            candidate.pop();
            let p = candidate.join("plugins");
            if p.exists() {
                return p;
            }
        }
    }
    let cwd = std::path::PathBuf::from("./plugins");
    if cwd.exists() {
        return cwd;
    }
    std::env::var_os("APPDATA")
        .map(|v| std::path::PathBuf::from(v).join("drafftink").join("plugins"))
        .unwrap_or_else(|| std::path::PathBuf::from("plugins"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use drafftink_core::model::ShapeKind;
    use egui::{pos2, vec2, Rect};

    /// 随机点名器：从输入框解析逗号 / 换行分隔的名字并入名单（去重、去空）。
    #[test]
    fn name_picker_add_names() {
        let mut t = NamePickerTool {
            input_text: "张三,李四，王五\n赵六".to_string(),
            ..NamePickerTool::default()
        };
        t.add_from_input();
        assert_eq!(t.names, ["张三", "李四", "王五", "赵六"], "英/中/换行分隔皆可解析");

        // 重复名字不重复加入；输入框在加入后应清空。
        t.input_text = "张三,小七".to_string();
        t.add_from_input();
        assert_eq!(t.names, ["张三", "李四", "王五", "赵六", "小七"], "重复去重");
        assert!(t.input_text.is_empty(), "加入后清空输入框");

        // 纯函数 parse_names：包围空白、分号、回车一并处理。
        assert_eq!(
            parse_names(" a , b; c，d;E\r\n f "),
            ["a", "b", "c", "d", "E", "f"]
        );
        assert!(parse_names(" , ; \n").is_empty(), "全分隔符输入为空名单");
    }

    /// 随机点名器：停止滚动后锁定选中一个名字（selected_name 非空），并复位滚动态。
    #[test]
    fn name_picker_stop_selects_one() {
        // 显示区已有名字：停止后锁死为当前显示的名字。
        let mut t = NamePickerTool {
            names: vec!["张三".into(), "李四".into(), "王五".into()],
            display_name: "王五".into(),
            is_rolling: true,
            ..NamePickerTool::default()
        };
        t.stop_rolling();
        assert!(!t.is_rolling, "停止后不再滚动");
        assert_eq!(t.selected_name.as_deref(), Some("王五"), "选中当前显示的名字");

        // 显示区为空（仍未开始滚动）：回退到名单第一项，保证永远选中一名。
        let mut t2 = NamePickerTool {
            names: vec!["张三".into(), "李四".into()],
            ..NamePickerTool::default()
        };
        t2.stop_rolling();
        assert_eq!(t2.selected_name.as_deref(), Some("张三"), "空显示区回退到名单首项");
    }

    /// 放大镜坐标变换正确性：圆心保持不动，周围点以圆心为基准等比外扩；
    /// 且任意 canvas_offset / canvas_zoom 下，通用公式退化为纯屏幕缩放（二者抵消）。
    #[test]
    fn magnifier_transform_correct() {
        // 恒等画布（offset=0、zoom=1）、放大 2x：圆心不动，边角点向外翻倍。
        let c = pos2(100.0, 100.0);
        let f = |p| magnifier_transform(p, c, vec2(0.0, 0.0), 1.0, 2.0);
        assert_eq!(f(pos2(100.0, 100.0)), pos2(100.0, 100.0), "圆心保持不动");
        assert_eq!(f(pos2(150.0, 150.0)), pos2(200.0, 200.0), "右下点放大 2x");
        assert_eq!(f(pos2(50.0, 80.0)), pos2(0.0, 60.0), "左上点放大 2x");

        // 非恒等画布（offset/zoom 改变、放大 3x）：通用公式手算校验。
        // 圆心屏幕 (250,230) → 画布 (100,100)；屏幕点 (270,240) → 画布 (110,105)
        // → 画布放大 3x 后 (130,115) → 映射回屏幕 (310,260)。
        let out = magnifier_transform(
            pos2(270.0, 240.0),
            pos2(250.0, 230.0),
            vec2(50.0, 30.0),
            2.0,
            3.0,
        );
        assert_eq!(out, pos2(310.0, 260.0), "通用画布参数 → 世界放大后映射回屏幕");

        // 关键不变量：**任何**画布参数与放大倍数下，放大镜圆心都保持固定。
        for zf in [1.0, 2.0, 4.0] {
            assert_eq!(
                magnifier_transform(
                    pos2(250.0, 230.0),
                    pos2(250.0, 230.0),
                    vec2(50.0, 30.0),
                    2.0,
                    zf,
                ),
                pos2(250.0, 230.0),
                "圆心在不同倍数下均应保持不动 (zf={zf})"
            );
        }
    }

    /// 构造一个已固定（user_rect 已知）的形状实例，便于在测试中做命中检测，
    /// 无需真实相机 / 画布坐标变换。
    fn sample_shape(rect: Rect, z_index: u64) -> ShapeInstance {
        ShapeInstance {
            kind: ShapeKind::Circle,
            world_rect: None,
            user_rect: Some(rect),
            stroke_width: 3.0,
            stroke_color: (0, 0, 0, 255),
            fill_color: None,
            arc_degrees: None,
            line_flipped: false,
            z_index,
            page: 0,
        }
    }

    /// 模拟一次「画布单击」：传入指针屏幕坐标与屏幕矩形。
    fn click(app: &mut IntegratedApp, x: f32, y: f32) {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(4000.0, 4000.0));
        app.handle_canvas_click(pos2(x, y), screen, &egui::Modifiers::NONE);
    }

    /// 模拟一次「Ctrl/Cmd + 画布单击」。
    fn ctrl_click(app: &mut IntegratedApp, x: f32, y: f32) {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(4000.0, 4000.0));
        let mut m = egui::Modifiers::NONE;
        m.command = true;
        app.handle_canvas_click(pos2(x, y), screen, &m);
    }

    /// 插入圆形 → 单击圆形中心 → 蓝色边框出现（选中成功）。
    #[test]
    fn click_shape_selects() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let rect = Rect::from_min_max(pos2(100.0, 100.0), pos2(300.0, 300.0));
        app.shape_instances
            .insert("s1".to_string(), sample_shape(rect, 0));

        // 初始应为未选中（固定）状态。
        assert!(app.selected_element_id.is_none());

        // 单击圆形中心 → 应选中该形状。
        click(&mut app, 200.0, 200.0);
        assert!(
            matches!(app.selected_element_id, Some(SelectedElement::Shape(ref id)) if id == "s1"),
            "clicking shape center should select it"
        );

        // 再次单击同一形状（已选中）应保持不变（保持选中，等待下一次单击空白处）。
        click(&mut app, 200.0, 200.0);
        assert!(
            matches!(app.selected_element_id, Some(SelectedElement::Shape(ref id)) if id == "s1"),
            "clicking an already-selected shape should keep it selected"
        );
    }

    /// 固定状态下 → 单击空白处 → 蓝色边框消失（固定成功 / 取消选中）。
    #[test]
    fn click_blank_space_deselects() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let rect = Rect::from_min_max(pos2(100.0, 100.0), pos2(300.0, 300.0));
        app.shape_instances
            .insert("s1".to_string(), sample_shape(rect, 0));

        // 先主动选中该固定形状（模拟老师单击形状进入微调）。
        app.selected_element_id = Some(SelectedElement::Shape("s1".to_string()));
        assert!(matches!(
            app.selected_element_id,
            Some(SelectedElement::Shape(ref id)) if id == "s1"
        ));

        // 单击空白处（矩形之外）→ 应取消选中。
        click(&mut app, 500.0, 500.0);
        assert!(
            app.selected_element_id.is_none(),
            "clicking blank space should deselect the fixed shape"
        );

        // 反向验证：空白处再单击圆形中心仍应重新选中。
        click(&mut app, 200.0, 200.0);
        assert!(
            matches!(app.selected_element_id, Some(SelectedElement::Shape(ref id)) if id == "s1"),
            "clicking shape after blank deselect should re-select it"
        );
    }

    /// 命中检测只认 user_rect（逻辑矩形），边框 / 抓手不应扩大或平移命中区。
    #[test]
    fn hit_test_uses_user_rect_only() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        // 紧贴边角的点（刚好在矩形内 1px）应命中；矩形外 1px 不应命中。
        let rect = Rect::from_min_max(pos2(100.0, 100.0), pos2(300.0, 300.0));
        app.shape_instances
            .insert("s1".to_string(), sample_shape(rect, 0));

        // 内部点命中。
        click(&mut app, 101.0, 101.0);
        assert!(matches!(
            app.selected_element_id,
            Some(SelectedElement::Shape(ref id)) if id == "s1"
        ));

        // 恰好在矩形外 1px（左侧）→ 不命中（取消选中）。
        click(&mut app, 99.0, 200.0);
        assert!(app.selected_element_id.is_none());
    }

    /// 大矩形内部有小圆时，点击圆心应选中上层的小圆，而不是被下方的大矩形挡住。
    #[test]
    fn overlapping_shapes_selects_topmost() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;

        // 底层大矩形（z_index=0）。
        let big = Rect::from_min_max(pos2(100.0, 100.0), pos2(300.0, 300.0));
        app.shape_instances
            .insert("big".to_string(), sample_shape(big, 0));

        // 上层小圆（z_index=1），完全位于大矩形内部。
        let small = Rect::from_min_max(pos2(180.0, 180.0), pos2(220.0, 220.0));
        app.shape_instances
            .insert("small".to_string(), sample_shape(small, 1));

        // 点击小圆中心 → 必须选中上层 small，而非底层 big。
        click(&mut app, 200.0, 200.0);
        assert!(
            matches!(app.selected_element_id, Some(SelectedElement::Shape(ref id)) if id == "small"),
            "clicking inside the smaller top shape should select it, not the larger bottom shape"
        );

        // 点击大矩形中但不在小圆内的区域 → 应选中底层 big。
        click(&mut app, 150.0, 150.0);
        assert!(
            matches!(app.selected_element_id, Some(SelectedElement::Shape(ref id)) if id == "big"),
            "clicking inside the larger bottom shape but outside the top shape should select the larger one"
        );
    }

    /// 大矩形盖住内部小圆时，按住 Ctrl/Cmd 单击可向下穿透循环选中内层图形。
    #[test]
    fn ctrl_click_cycles_through_overlapping_shapes() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;

        // 底层大矩形（z_index=0），后画在上层；普通单击会选中它。
        let big = Rect::from_min_max(pos2(100.0, 100.0), pos2(300.0, 300.0));
        app.shape_instances
            .insert("big".to_string(), sample_shape(big, 0));

        // 上层小圆（z_index=1），但本场景模拟「大矩形后被插入」的覆盖情况：
        // 普通点击圆心选中的是 big，Ctrl+Click 可穿透到 small。
        let small = Rect::from_min_max(pos2(180.0, 180.0), pos2(220.0, 220.0));
        app.shape_instances
            .insert("small".to_string(), sample_shape(small, 1));

        // 普通单击圆心：按 z-order 选中上层 small（与上一个测试一致）。
        click(&mut app, 200.0, 200.0);
        assert!(
            matches!(app.selected_element_id, Some(SelectedElement::Shape(ref id)) if id == "small"),
            "normal click should still select the topmost shape"
        );

        // 按住 Ctrl 再单击同一点：命中栈 [small, big] → 当前 small → 下一个 big。
        ctrl_click(&mut app, 200.0, 200.0);
        assert!(
            matches!(app.selected_element_id, Some(SelectedElement::Shape(ref id)) if id == "big"),
            "first Ctrl+click should drill down to the shape behind"
        );

        // 再按一次 Ctrl 单击：命中栈 [small, big] → 当前 big → 循环回到 small。
        ctrl_click(&mut app, 200.0, 200.0);
        assert!(
            matches!(app.selected_element_id, Some(SelectedElement::Shape(ref id)) if id == "small"),
            "second Ctrl+click should cycle back to the topmost shape"
        );
    }

    // ── Part A / B / C 验证 ─────────────────────────────────────────────────

    /// Part A：保存导出的 .enbx 应为合法 ZIP，且能被对称解析器 `parse_enbx` 回解。
    #[test]
    fn save_enbx_creates_valid_zip() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let ctx = egui::Context::default();

        // 文档层插一个文本框 + 宿主层插一个形状，确保两路元素都被序列化。
        app.insert_text_at(&ctx);
        app.insert_shape_at(ShapeKind::Rectangle, [960.0, 540.0], &ctx);

        let dir = std::env::temp_dir();
        let path = dir.join("drafftink_save_test.enbx");

        let res = crate::save::save_enbx(&app, &path);
        assert!(res.is_ok(), "save_enbx should succeed: {:?}", res.err());
        assert!(path.exists(), "exported .enbx should exist on disk");

        // 用 zip 校验 ZIP 结构（zip 是 dev-dependency，测试内可用）。
        let file = std::fs::File::open(&path).expect("open exported file");
        let mut archive = zip::ZipArchive::new(file).expect("valid zip archive");
        let names: Vec<String> = (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
            .collect();
        assert!(
            names.iter().any(|n| n == "Reference.xml"),
            "ZIP must contain Reference.xml, got: {names:?}"
        );
        assert!(
            names.iter().any(|n| n == "Slide_1.xml"),
            "ZIP must contain Slide_1.xml, got: {names:?}"
        );

        // 与解析器对称回解：幻灯片数与元素数应非零。
        let parsed = drafftink_enbx::parse_enbx(&path).expect("parse round-trip");
        assert_eq!(parsed.slides.len(), 1, "should produce exactly one slide");
        assert!(
            !parsed.slides[0].elements.is_empty(),
            "slide should contain the serialized elements"
        );
        // 验证元素类型覆盖：文档层 Text + 宿主层 Shape 都被序列化并回解。
        use drafftink_enbx::EnbxElement;
        let slide_elems = &parsed.slides[0].elements;
        assert!(
            slide_elems
                .iter()
                .any(|e| matches!(e, EnbxElement::Text(_))),
            "slide should contain a Text element (round-tripped from insert_text_at)"
        );
        assert!(
            slide_elems
                .iter()
                .any(|e| matches!(e, EnbxElement::Shape(_))),
            "slide should contain a Shape element (round-tripped from insert_shape_at)"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// Part ENBX 完整保存：插入音频 → 断言 Resources/ 下有 MD5 命名的 .wav 文件。
    #[test]
    fn save_enbx_resource_hash() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let ctx = egui::Context::default();
        let audio_path = make_sample_audio();

        // 插入一个音频控制条（资源按 `md5(file://<path>).wav` 命名内嵌）。
        app.insert_audio_at(&audio_path, [960.0, 540.0], &ctx);

        let out = std::env::temp_dir().join("drafftink_audio_resource_hash.enbx");
        crate::save::save_enbx(&app, &out).expect("save_enbx");
        assert!(out.exists(), ".enbx should exist after save");

        // 预期资源文件名：md5(裸路径).wav（embed_resource_id 先 strip "file://" 再 MD5）。
        let md5_hex = format!("{:x}", md5::compute(audio_path.to_string_lossy().as_bytes()));
        let expected_name = format!("{md5_hex}.wav");

        let file = std::fs::File::open(&out).expect("open enbx");
        let mut archive = zip::ZipArchive::new(file).expect("valid zip");
        let names: Vec<String> = (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
            .collect();
        let resource_path = format!("Resources/{expected_name}");
        assert!(
            names.contains(&resource_path),
            "ZIP must contain {resource_path} (md5-of-file:// path), got: {names:?}"
        );

        // 验证：解析后的 ENBX 包含一个 Audio 元素，resource_id 指向该资源。
        let parsed = drafftink_enbx::parse_enbx(&out).expect("parse");
        use drafftink_enbx::EnbxElement;
        let audio_elems: Vec<_> = parsed.slides[0]
            .elements
            .iter()
            .filter_map(|e| {
                if let EnbxElement::Audio(a) = e {
                    Some(a.resource_id.clone())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            audio_elems.len(),
            1,
            "exactly one Audio element expected, got {audio_elems:?}"
        );
        assert_eq!(audio_elems[0], expected_name, "Audio resource_id should be the MD5-named file");

        let _ = std::fs::remove_file(&audio_path);
        let _ = std::fs::remove_file(&out);
    }

    /// Part B：点击「T 文本」插入默认文本框并选中，且内容可被就地修改。
    #[test]
    fn text_element_insert_and_edit() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let ctx = egui::Context::default();

        app.insert_text_at(&ctx);

        // 当前页应恰好多出一个文本元素，且已被选中。
        let count = if app.edit.doc.pages.is_empty() {
            app.edit
                .doc
                .elements
                .iter()
                .filter(|e| matches!(e, Element::Text(_)))
                .count()
        } else {
            let p = app.edit.multi_page.current_page;
            app.edit
                .doc
                .pages[p]
                .elements
                .iter()
                .filter(|e| matches!(e, Element::Text(_)))
                .count()
        };
        assert_eq!(count, 1, "should insert exactly one text element");
        assert!(
            matches!(app.selected_element_id, Some(SelectedElement::Text(_))),
            "inserted text should be auto-selected"
        );

        // 模拟就地编辑：定位并修改文本内容与尺寸（与 draw_text_overlay 写回路径一致）。
        let id = match &app.selected_element_id {
            Some(SelectedElement::Text(id)) => id.clone(),
            _ => panic!("expected text selected"),
        };
        let page = app.edit.multi_page.current_page;
        let text_ref: &mut String = if app.edit.doc.pages.is_empty() {
            match &mut app.edit.doc.elements[0] {
                Element::Text(t) => &mut t.text,
                _ => panic!("first element should be text"),
            }
        } else {
            match &mut app.edit.doc.pages[page].elements[0] {
                Element::Text(t) => &mut t.text,
                _ => panic!("first element should be text"),
            }
        };
        text_ref.clear();
        text_ref.push_str("Hello ENBX");
        assert_eq!(text_ref, "Hello ENBX");

        // id 仍为同一文本元素，删除验证前清理选中态。
        assert!(matches!(app.selected_element_id, Some(SelectedElement::Text(ref x)) if x == &id));
    }

    /// Part C：Delete 删除选中视频时，必须移除叠加层实例（VideoPlayer::Drop 杀掉
    /// ffmpeg 子进程 + 停 cpal 音频流）并清空选中态与拖拽守卫。
    #[test]
    fn delete_video_kills_process() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let ctx = egui::Context::default();

        // 插入一个本地视频（文件不存在时 VideoPlayer 初始化失败、player=None，
        // 但仍建立宿主层叠加层；Drop 在实例被移除时运行，确保进程被回收）。
        let path = std::env::temp_dir().join("does_not_exist_clip.mp4");
        app.insert_video_from_path(&path, &ctx);
        assert!(
            !app.video_instances.is_empty(),
            "video overlay instance should be inserted"
        );
        assert!(
            !app.inserted_videos.is_empty(),
            "inserted video record should be tracked for re-sync"
        );

        let vid = app.video_instances.keys().next().unwrap().clone();
        app.selected_element_id = Some(SelectedElement::Video(vid));
        app.active_drag = Some((egui::Id::new("x"), crate::interactive_rect::HitZone::Move));

        app.delete_selected(&ctx);

        assert!(
            app.video_instances.is_empty(),
            "video instance must be removed on delete (triggers VideoPlayer::Drop)"
        );
        assert!(
            app.inserted_videos.is_empty(),
            "inserted video record must be removed on delete"
        );
        assert!(app.selected_element_id.is_none(), "selection must be cleared");
        assert!(app.active_drag.is_none(), "drag guard must be cleared");
    }

    // ── Part B / C 新增测试 ───────────────────────────────────────────────

    /// 当前页文档层里文本元素的数量（用于 undo/redo 断言）。
    fn count_text(app: &IntegratedApp) -> usize {
        let page = app.edit.multi_page.current_page;
        let elems = if app.edit.doc.pages.is_empty() {
            &app.edit.doc.elements
        } else {
            &app.edit.doc.pages[page].elements
        };
        elems.iter().filter(|e| matches!(e, Element::Text(_))).count()
    }

    /// 用本地 ffmpeg 生成一段 1 秒、440Hz 正弦的 WAV（纯音频测试样本）。
    fn make_sample_audio() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("drafftink_audio_test_{}.wav", Uuid::new_v4()));
        let ffmpeg = crate::video_player::ffmpeg_exe().expect("本地 ffmpeg.exe 应存在");
        let status = std::process::Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-y",
                "-c:a",
                "pcm_s16le",
                p.to_str().unwrap(),
            ])
            .status()
            .expect("运行 ffmpeg 生成样本");
        assert!(status.success(), "ffmpeg 应成功生成音频样本");
        p
    }

    /// Part B：插入文本 → 撤销 → 元素消失。
    #[test]
    fn undo_insert_restores() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let ctx = egui::Context::default();

        app.insert_text_at(&ctx);
        assert_eq!(count_text(&app), 1, "插入后应有 1 个文本元素");

        let cmd = app.history.undo().expect("应有可撤销命令");
        app.apply_undo(cmd, &ctx);
        assert_eq!(count_text(&app), 0, "撤销插入后文本应消失");
    }

    /// Part B：插入 → 撤销 → 重做 → 元素恢复。
    #[test]
    fn redo_restore_element() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let ctx = egui::Context::default();

        app.insert_text_at(&ctx);
        let cmd = app.history.undo().expect("undo");
        app.apply_undo(cmd, &ctx);
        assert_eq!(count_text(&app), 0);

        let cmd = app.history.redo().expect("redo");
        app.apply_redo(cmd, &ctx);
        assert_eq!(count_text(&app), 1, "重做后文本应恢复");
    }

    /// Part C：插入音频 → seek → 进度基准更新（无设备时管线为 None，仍验证 seek 语义）。
    #[test]
    fn audio_insert_and_seek() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let ctx = egui::Context::default();
        let path = make_sample_audio();

        app.insert_audio_at(&path, [960.0, 540.0], &ctx);
        assert_eq!(app.audio_instances.len(), 1, "应插入 1 个音频实例");
        assert_eq!(app.inserted_audios.len(), 1, "应记录 1 条音频记录");

        let id = app.audio_instances.keys().next().unwrap().clone();
        if let Some(inst) = app.audio_instances.get_mut(&id) {
            inst.seek(500);
            assert_eq!(inst.seek_base_ms, 500, "seek 应记录基准 500ms");
            assert!(inst.current_ms() >= 500, "seek 后进度应 ≥ 500ms");
        }

        let _ = std::fs::remove_file(&path);
    }

    /// Part D：删除选中音频 → 实例 / 记录 / 选中态 / 拖拽守卫全清空
    /// （实例被移除触发 AudioPipeline::Drop 杀 ffmpeg + 停 cpal）。
    #[test]
    fn delete_audio_removes_instance() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let ctx = egui::Context::default();
        let path = make_sample_audio();

        app.insert_audio_at(&path, [960.0, 540.0], &ctx);
        let id = app.audio_instances.keys().next().unwrap().clone();
        app.selected_element_id = Some(SelectedElement::Audio(id));
        app.active_drag = Some((egui::Id::new("x"), crate::interactive_rect::HitZone::Move));

        app.delete_selected(&ctx);

        assert!(app.audio_instances.is_empty(), "删除后音频实例应被移除");
        assert!(app.inserted_audios.is_empty(), "删除后音频记录应被移除");
        assert!(app.selected_element_id.is_none(), "选中态应清空");
        assert!(app.active_drag.is_none(), "拖拽守卫应清空");

        let _ = std::fs::remove_file(&path);
    }

    /// Part B：叠加层移动/缩放（ModifyRect）的撤销与重做。
    #[test]
    fn modify_shape_rect_undo_redo() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let ctx = egui::Context::default();

        app.insert_shape_at(ShapeKind::Rectangle, [960.0, 540.0], &ctx);
        let id = app.shape_instances.keys().next().unwrap().clone();

        let old_rect = app.shape_instances[&id].user_rect; // 初始 None
        let new_rect = Some(egui::Rect::from_min_size(
            egui::pos2(100.0, 100.0),
            egui::vec2(300.0, 200.0),
        ));

        // 模拟拖拽结束：push ModifyRect 并应用新 user_rect。
        app.history.push(UndoCmd::ModifyRect {
            sel: SelectedElement::Shape(id.clone()),
            old_rect,
            new_rect,
        });
        app.shape_instances.get_mut(&id).unwrap().user_rect = new_rect;
        assert_eq!(app.shape_instances[&id].user_rect, new_rect);

        // 撤销 → user_rect 回旧值。
        let cmd = app.history.undo().expect("undo");
        app.apply_undo(cmd, &ctx);
        assert_eq!(app.shape_instances[&id].user_rect, old_rect, "撤销后应回旧矩形");

        // 重做 → user_rect 回新值。
        let cmd = app.history.redo().expect("redo");
        app.apply_redo(cmd, &ctx);
        assert_eq!(app.shape_instances[&id].user_rect, new_rect, "重做后应回新矩形");
    }

    /// Part B：文本内容修改（ModifyText）的撤销与重做（走 commit_text_edit 提交路径）。
    #[test]
    fn modify_text_content_undo_redo() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let ctx = egui::Context::default();

        app.insert_text_at(&ctx);
        let page = app.edit.multi_page.current_page;
        let elem_id = if app.edit.doc.pages.is_empty() {
            app.edit.doc.elements[0].id()
        } else {
            app.edit.doc.pages[page].elements[0].id()
        };

        // 编辑前快照旧元素。
        let old = app.clone_text_elem(page, 0);
        app.text_edit_undo = Some(TextEditSession { page, elem_id, old: old.clone() });

        // 修改文本内容（模拟 TextEdit 输入）。
        let new_text = "Hello Modify".to_string();
        if app.edit.doc.pages.is_empty() {
            if let Element::Text(t) = &mut app.edit.doc.elements[0] {
                t.text = new_text.clone();
            }
        } else if let Element::Text(t) = &mut app.edit.doc.pages[page].elements[0] {
            t.text = new_text.clone();
        }

        // 提交（失焦）→ push ModifyText。
        app.commit_text_edit();
        assert_eq!(
            match &app.clone_text_elem(page, 0) {
                Element::Text(t) => t.text.clone(),
                _ => String::new(),
            },
            new_text,
            "提交后文本应为新值"
        );

        // 撤销 → 文本回旧值。
        let cmd = app.history.undo().expect("undo");
        app.apply_undo(cmd, &ctx);
        let cur_text = match &app.clone_text_elem(page, 0) {
            Element::Text(t) => t.text.clone(),
            _ => String::new(),
        };
        assert_eq!(cur_text, "双击编辑文本", "撤销后文本应回旧值");

        // 重做 → 文本回新值。
        let cmd = app.history.redo().expect("redo");
        app.apply_redo(cmd, &ctx);
        let cur_text = match &app.clone_text_elem(page, 0) {
            Element::Text(t) => t.text.clone(),
            _ => String::new(),
        };
        assert_eq!(cur_text, new_text, "重做后文本应回新值");
    }

    /// Part 教具：圆规提交圆 → 生成标准 Circle 形状叠加层（可 Undo）。
    #[test]
    fn compass_draw_circle() {
        let mut app = IntegratedApp::new();
        app.mode = AppMode::Prepare;
        let ctx = egui::Context::default();

        // 激活圆规并模拟「拖拽确定圆心 + 半径」。
        app.activate_compass(&ctx);
        if let ActiveTool::Compass(t) = &mut app.active_tool {
            t.stage = 1;
            t.pivot = egui::pos2(100.0, 100.0);
            t.pencil = egui::pos2(150.0, 100.0); // r = 50
        }
        // 提交（模拟双击 / Enter）。
        app.commit_shape_geom(
            ShapeKind::Circle,
            egui::Rect::from_center_size(egui::pos2(100.0, 100.0), egui::vec2(100.0, 100.0)),
            None,
            false,
            false,
            &ctx,
        );

        assert_eq!(app.shape_instances.len(), 1, "应提交 1 个圆元素");
        let inst = app.shape_instances.values().next().unwrap();
        assert_eq!(inst.kind, ShapeKind::Circle);
        // 纳入 Undo：撤销后形状消失。
        let cmd = app.history.undo().expect("undo");
        app.apply_undo(cmd, &ctx);
        assert!(app.shape_instances.is_empty(), "撤销后圆应被移除");
    }
}

