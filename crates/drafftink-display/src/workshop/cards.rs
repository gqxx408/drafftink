//! 多学科工坊卡片数据模型。
//!
//! 用 Rust Enum 替代希沃的万能 ViewModel，类型安全、零冗余。
//! 每种卡片有自己的数据结构，编译器保证你不会访问不存在的字段。

use egui::Pos2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::workshop::experiment::ExperimentCardData;
use crate::workshop::quiz::QuizCardData;

// ─── 学科分类 ──────────────────────────────────────────────────────────────

/// 学科分类（用于卡片分类标签筛选）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Subject {
    /// 语文
    Chinese,
    /// 数学
    Math,
    /// 英语
    English,
    /// 物理
    Physics,
    /// 化学
    Chemistry,
    /// 生物
    Biology,
    /// 历史
    History,
    /// 地理
    Geography,
    /// 政治
    Politics,
    /// 其他 / 综合
    Other,
}

impl Subject {
    /// 学科中文名。
    pub fn label(&self) -> &'static str {
        match self {
            Subject::Chinese => "语文",
            Subject::Math => "数学",
            Subject::English => "英语",
            Subject::Physics => "物理",
            Subject::Chemistry => "化学",
            Subject::Biology => "生物",
            Subject::History => "历史",
            Subject::Geography => "地理",
            Subject::Politics => "政治",
            Subject::Other => "综合",
        }
    }

    /// 学科对应的 emoji 图标。
    pub fn emoji(&self) -> &'static str {
        match self {
            Subject::Chinese => "📖",
            Subject::Math => "📐",
            Subject::English => "🔤",
            Subject::Physics => "⚡",
            Subject::Chemistry => "🧪",
            Subject::Biology => "🧬",
            Subject::History => "🏛️",
            Subject::Geography => "🌍",
            Subject::Politics => "⚖️",
            Subject::Other => "📦",
        }
    }

    /// 所有学科列表。
    pub fn all() -> &'static [Subject] {
        &[
            Subject::Chinese,
            Subject::Math,
            Subject::English,
            Subject::Physics,
            Subject::Chemistry,
            Subject::Biology,
            Subject::History,
            Subject::Geography,
            Subject::Politics,
            Subject::Other,
        ]
    }
}

// ─── 卡片类型枚举 ──────────────────────────────────────────────────────────

/// 学科卡片的具体类型和数据。
///
/// 用 Enum 替代希沃的万能 ViewModel：
/// - 类型安全：编译器保证你只能访问对应类型的字段
/// - 内存紧凑：每种变体只存自己需要的数据
/// - 可扩展：新增类型只需加一个枚举变体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubjectCard {
    /// 题库/小测卡片
    Quiz(QuizCardData),
    /// 作业卡片
    Homework(HomeworkCardData),
    /// 视频卡片
    Video(VideoCardData),
    /// 虚拟实验卡片
    Experiment(ExperimentCardData),
    /// 画板卡片
    DrawingBoard(DrawingBoardData),
}

impl SubjectCard {
    /// 卡片类型的中文名。
    pub fn type_label(&self) -> &'static str {
        match self {
            SubjectCard::Quiz(_) => "题目",
            SubjectCard::Homework(_) => "作业",
            SubjectCard::Video(_) => "视频",
            SubjectCard::Experiment(_) => "实验",
            SubjectCard::DrawingBoard(_) => "画板",
        }
    }

    /// 卡片类型的 emoji 图标。
    pub fn type_emoji(&self) -> &'static str {
        match self {
            SubjectCard::Quiz(_) => "📝",
            SubjectCard::Homework(_) => "📋",
            SubjectCard::Video(_) => "🎬",
            SubjectCard::Experiment(_) => "🔬",
            SubjectCard::DrawingBoard(_) => "🎨",
        }
    }
}

// ─── 作业卡片 ──────────────────────────────────────────────────────────────

/// 作业卡片数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeworkCardData {
    /// 作业标题
    pub title: String,
    /// 作业描述/说明
    pub description: String,
    /// 截止日期（时间戳，秒）
    pub deadline: Option<i64>,
    /// 题目数量
    pub question_count: usize,
    /// 是否已完成
    pub completed: bool,
    /// 得分（满分 100）
    pub score: Option<f32>,
}

// ─── 视频卡片 ──────────────────────────────────────────────────────────────

/// 视频卡片数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoCardData {
    /// 视频标题
    pub title: String,
    /// 视频描述
    pub description: String,
    /// 本地文件路径（已缓存）
    pub local_path: Option<String>,
    /// 在线 URL（未缓存时使用）
    pub online_url: Option<String>,
    /// 视频时长（秒）
    pub duration_sec: Option<u32>,
    /// 是否已下载
    pub downloaded: bool,
    /// 上次播放进度（秒）
    pub last_position_sec: f32,
}

// ─── 画板卡片 ──────────────────────────────────────────────────────────────

/// 画板卡片数据。
///
/// 本质上就是一个空白页面 + 可选的背景图/网格，
/// 点击后直接在当前白板环境下新建页面。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawingBoardData {
    /// 画板标题
    pub title: String,
    /// 画板描述
    pub description: String,
    /// 背景类型
    pub background: BoardBackground,
    /// 预设的元素数量（用于展示）
    pub element_count: usize,
}

/// 画板背景类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoardBackground {
    /// 空白
    Blank,
    /// 网格
    Grid,
    /// 横线
    Lined,
    /// 坐标纸
    GraphPaper,
}

// ─── 卡片容器 ──────────────────────────────────────────────────────────────

/// 通用卡片元素——把卡片数据和显示信息打包在一起。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct CardElement {
    /// 卡片唯一 ID
    pub id: Uuid,
    /// 卡片标题（显示在卡片顶部）
    pub title: String,
    /// 所属学科
    pub subject: Subject,
    /// 卡片具体数据
    pub data: SubjectCard,
    /// 卡片在工坊中的位置（网格布局时自动计算）
    #[serde(skip, default)]
    pub position: Pos2,
    /// 卡片缩放比例
    #[serde(default = "default_scale")]
    pub scale: f32,
    /// 难度等级（1-5），可选
    pub difficulty: Option<u8>,
    /// 标签（用于搜索和筛选）
    pub tags: Vec<String>,
}

fn default_scale() -> f32 {
    1.0
}

impl CardElement {
    /// 创建一个新的卡片元素。
    pub fn new(title: impl Into<String>, subject: Subject, data: SubjectCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            subject,
            data,
            position: Pos2::ZERO,
            scale: 1.0,
            difficulty: None,
            tags: Vec::new(),
        }
    }

    /// 设置难度等级。
    pub fn with_difficulty(mut self, level: u8) -> Self {
        self.difficulty = Some(level.clamp(1, 5));
        self
    }

    /// 添加一个标签。
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}
