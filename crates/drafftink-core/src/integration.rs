//! 备授一体上层整合共享上下文。
//!
//! 仅承载跨「备课 / 授课」两个模式共享的轻量状态与配置，不触碰任何核心逻辑
//! （几何编辑、板书渲染、enbx/drftx 编解码等仍在各自 crate 内实现）。
//! 所有共享状态均通过 [`SharedContext`]（`Arc<Mutex<_>>`）包装，确保线程安全、无数据竞争。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::document::StrokeData;
use crate::model::CoursewareDoc;
use crate::plugin::PluginManager;

/// 教师授课批注层缓冲（对应 drftx 的 `TeacherAnnotation` 层）。
///
/// 仅承载授课模式产生的板书 / 标注 / 小测记录，结构上与课件内容层
/// (`CoursewareDoc.pages[i].elements`，即学生原始作答快照）完全隔离。
/// 合并回课件时只写入 `annotations_data`，绝不触碰 `elements`，
/// 符合「作业防篡改」红线。
#[derive(Clone, Default)]
pub struct TeachAnnotationBuffer {
    /// page_index -> 该页教师批注（中性格式 `StrokeData`，无 egui 依赖）。
    pub per_page: HashMap<usize, Vec<StrokeData>>,
}

/// 备授一体共享的应用上下文。
///
/// 备课模式打开的课件路径、用户账号、后端连接、统一设置、当前文档快照、
/// 教师批注缓冲与**共享插件管理器**集中存放于此，两个模式通过
/// [`SharedContext`]（`Arc<Mutex<SharedAppContext>>`）安全读写。
#[derive(Default, Clone)]
pub struct SharedAppContext {
    /// 当前打开的课件路径（备课模式打开文件时写入，授课模式直接读取）。
    pub current_doc_path: Option<PathBuf>,
    /// 当前课件文档快照（授课端加载 / 回写用）。仅做状态传递，不修改核心序列化。
    pub doc: Option<CoursewareDoc>,
    /// 教师授课批注（仅批注层）。切回备课时合并进课件 `annotations_data`。
    pub teach_annotations: TeachAnnotationBuffer,
    /// 共享插件管理器：两模式复用同一实例，禁止各自独立加载（避免 cdylib 双加载）。
    pub plugin_manager: Option<Arc<Mutex<PluginManager>>>,
    /// 已加载插件清单（名称, 版本），供 UI / 预览展示，避免重复探测。
    pub loaded_plugins: Vec<(String, String)>,
    /// 备课模式布置的作业 ID 列表，授课模式直接调取学生提交记录。
    pub homework_ids: Vec<String>,
    /// 内网后端基础地址。
    pub backend_url: String,
    /// 登录 JWT（如有）。
    pub jwt_token: Option<String>,
    /// 当前登录账号。
    pub account: Option<String>,
    /// 统一主题：黑底白工具栏。
    pub theme_dark: bool,
    /// 统一笔刷大小（两个模式快捷键一致）。
    pub brush_size: f32,
    /// 统一输出分辨率。
    pub resolution: [f32; 2],
    /// 统一资源缓存目录（课件 / 纹理 / 音视频，两模式共享，避免重复拉取）。
    pub resource_cache_dir: PathBuf,
}

impl SharedAppContext {
    /// 注入共享插件管理器，并记录已加载清单（只读，不重复加载）。
    pub fn set_plugin_manager(&mut self, pm: Arc<Mutex<PluginManager>>) {
        if let Ok(g) = pm.lock() {
            self.loaded_plugins = g.list_loaded();
        }
        self.plugin_manager = Some(pm);
    }

    /// 把授课模式某页的批注写入缓冲（仅批注层）。
    pub fn capture_teach_strokes(&mut self, page: usize, strokes: Vec<StrokeData>) {
        self.teach_annotations.per_page.insert(page, strokes);
    }

    /// 取出某页的授课批注（用于回灌课件 / 加载进授课端）。
    pub fn take_teach_strokes(&self, page: usize) -> Vec<StrokeData> {
        self.teach_annotations
            .per_page
            .get(&page)
            .cloned()
            .unwrap_or_default()
    }
}

/// 线程安全的共享上下文句柄，供备课 / 授课两个模块读写。
pub type SharedContext = Arc<Mutex<SharedAppContext>>;
