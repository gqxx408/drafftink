//! 多学科工坊（Workshop）—— 轻量级、本地优先的多学科资源管理系统。
//!
//! 替代希沃 MultiSubject 模块的 WebView + PNG 方案，
//! 使用纯 Rust + egui 实现数据驱动的渲染。
//!
//! # 核心优势
//!
//! - **内存极低**：结构化数据存储，10 道题 < 20MB（希沃 > 100MB）
//! - **矢量清晰**：egui 实时绘制，缩放无锯齿
//! - **离线可用**：全部本地渲染，断网照常使用
//! - **类型安全**：Enum 区分卡片类型，编译器保证字段正确
//!
//! # 模块结构
//!
//! - `cards` — 卡片数据模型（SubjectCard 枚举 + CardElement 容器）
//! - `quiz` — 题库卡片的数据结构和渲染引擎
//! - `experiment` — 虚拟实验卡片（电路、光学等）
//! - `workshop` — 工坊主界面（卡片网格 + 分类 + 详情弹窗）

pub mod cards;
pub mod experiment;
pub mod quiz;
pub mod workshop;

pub use workshop::Workshop;
