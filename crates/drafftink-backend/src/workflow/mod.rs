//! # 移动办公审批工作流模块
//!
//! 公文 / 用印 / 车辆 三类审批的状态机引擎与存储，配合 `api/mobile.rs` 提供 REST 接口。

pub mod engine;
pub mod store;
pub mod types;

pub use engine::{audit_action_for, WorkflowEngine, WorkflowError};
pub use store::WorkflowStore;
pub use types::{
    Announcement, ApprovalDecision, ApprovalMode, ApprovalRecord, MeetingBooking, Message,
    WorkflowInstance, WorkflowNode, WorkflowStatus, WorkflowType,
};
