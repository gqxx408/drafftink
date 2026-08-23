//! Quiz UI 模块
//!
//! 基于 egui 的课堂互动界面，包含：
//! - 教师端主面板（题目控制、实时统计、学生列表）
//! - 实时柱状图（选项分布可视化）
//! - 抢答动画（胜者高亮）
//! - 双屏学生视图

pub mod bar_chart;
pub mod quiz_panel;
pub mod student_view;

pub use quiz_panel::QuizPanel;
pub use student_view::StudentScreen;