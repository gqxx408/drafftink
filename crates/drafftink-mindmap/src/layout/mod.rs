//! 布局算法层 — 纯函数，无 UI 依赖
//!
//! 所有布局策略都实现 `LayoutStrategy` trait，
//! 输入节点树 + 视口参数，输出每个节点的目标位置。
//!
//! # 架构优势
//! - 纯函数式，无副作用，可并行计算
//! - 策略模式实现类型切换（树形 ↔ 鱼骨 ↔ 星环）
//! - 可通过 rayon 并行计算大规模导图

use std::collections::HashMap;
use std::ops::{Add, Div, Mul, Sub};
use uuid::Uuid;

use crate::types::{MapType, MindMapDoc};

// ── 2D 向量（UI 无关） ────────────────────────────────────────────

/// 布局计算用的 2D 向量，独立于任何 UI 库。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn distance(&self, other: &Vec2) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// 转换为 egui::Pos2
    pub fn to_egui_pos2(self) -> egui::Pos2 {
        egui::pos2(self.x, self.y)
    }

    /// 从 egui::Pos2 创建
    pub fn from_egui_pos2(p: egui::Pos2) -> Self {
        Self { x: p.x, y: p.y }
    }

    /// 从 egui::Vec2 创建
    pub fn from_egui_vec2(v: egui::Vec2) -> Self {
        Self { x: v.x, y: v.y }
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Div<f32> for Vec2 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

// ── 布局策略 trait ────────────────────────────────────────────────

/// 布局策略 trait（策略模式）
///
/// 所有布局算法都实现此 trait，
/// 输入文档和视口，输出每个节点的目标位置。
pub trait LayoutStrategy {
    /// 计算布局
    ///
    /// # 参数
    /// - `doc`: 思维导图文档
    /// - `viewport`: 视口尺寸（用于根节点居中）
    ///
    /// # 返回
    /// 节点 ID → 目标位置的映射
    fn layout(&self, doc: &MindMapDoc, viewport: Vec2) -> HashMap<Uuid, Vec2>;
}

/// 根据导图类型创建对应的布局策略
pub fn create_layout(doc: &MindMapDoc) -> Box<dyn LayoutStrategy> {
    match doc.map_type {
        MapType::MindMap | MapType::Organization => Box::new(TreeLayout {
            node_offset: doc.node_offset,
            tree_offset: doc.tree_offset,
            root_distance: doc.root_distance,
        }),
        MapType::FishBone => Box::new(FishBoneLayout::default()),
        MapType::Mindly => Box::new(RadialLayout {
            ring_radius: 120.0,
            node_angle_spread: std::f32::consts::TAU / 8.0,
        }),
    }
}

// ── 树形布局（思维导图 / 组织架构图） ──────────────────────────────

pub mod tree;
pub use tree::TreeLayout;

// ── 鱼骨图布局 ────────────────────────────────────────────────────

pub mod fishbone;
pub use fishbone::FishBoneLayout;

// ── 星环图布局（放射状） ──────────────────────────────────────────

pub mod radial;
pub use radial::RadialLayout;