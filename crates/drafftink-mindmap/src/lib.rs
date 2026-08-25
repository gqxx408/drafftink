//! drafftink-mindmap — 高性能思维导图引擎
//!
//! 支持传统树形导图与星环图（Mindly 式）双模式。
//!
//! # 架构
//! ```text
//! 数据模型层 (types) ─── 纯数据，Clone + Serialize + Deserialize
//!       │
//!       ├── 布局算法层 (layout) ─── 纯函数，策略模式，可并行计算
//!       │
//!       ├── 渲染层 (render) ─── egui 2D / wgpu 3D 双后端
//!       │
//!       ├── 交互控制器 (interaction) ─── 状态机 + 事件驱动
//!       │
//!       └── 持久化层 (persistence) ─── RON 格式，人类可读
//! ```
//!
//! # 快速开始
//! ```ignore
//! use drafftink_mindmap::*;
//!
//! // 1. 创建文档
//! let mut doc = MindMapDoc::new("中心主题");
//! doc.add_child(doc.root_id, "分支1", NodePosition::Right)?;
//! doc.add_child(doc.root_id, "分支2", NodePosition::Left)?;
//!
//! // 2. 计算布局
//! let layout = create_layout(&doc);
//! let positions = layout.layout(&doc, Vec2::new(1920.0, 1080.0));
//!
//! // 3. 渲染（在 egui 中）
//! // let renderer = MindMapRenderer::default();
//! // renderer.render(&painter, &doc, &positions, &interaction);
//!
//! // 4. 保存
//! let ron_str = save_mindmap(&doc)?;
//! ```

pub mod interaction;
pub mod layout;
pub mod persistence;
pub mod render;
pub mod rich_text;
pub mod types;

// 核心类型 re-export
pub use interaction::{DragState, KeyAction, MindMapEvent, MindMapInteraction, PanState};
pub use layout::{create_layout, FishBoneLayout, LayoutStrategy, RadialLayout, TreeLayout, Vec2};
pub use persistence::{load_from_file, load_mindmap, save_mindmap, save_to_file};
pub use render::MindMapRenderer;
pub use render::MindMapViewer;
pub use rich_text::{RichText, RichTextSpan};
pub use types::{
    ChildrenRotation, EmbeddedContent, MapType, MindMapDoc, MindNode, NodePosition, NodeStyle,
};
