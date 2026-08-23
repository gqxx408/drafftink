//! # 审批工作流数据契约
//!
//! 定义移动办公审批流的领域模型：
//! - [`WorkflowType`]：公文 / 用印 / 车辆 三类审批。
//! - [`ApprovalMode`]：会签（全部同意）/ 或签（任一同意）。
//! - [`WorkflowNode`]：流程节点，绑定 RBAC 角色。
//! - [`WorkflowInstance`]：一次审批实例，承载节点流转状态与 ZXBG01 公文记录。
//!
//! 所有结构均可序列化，便于持久化与前端展示。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use drafftink_core::Role;

/// 审批流类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowType {
    /// 公文流转审批
    OfficialDoc,
    /// 用印（盖章）审批
    Seal,
    /// 公务用车审批
    Vehicle,
}

impl WorkflowType {
    /// 中文名称
    pub fn label(&self) -> &'static str {
        match self {
            Self::OfficialDoc => "公文流转",
            Self::Seal => "用印申请",
            Self::Vehicle => "车辆申请",
        }
    }
}

/// 节点审批模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// 会签：本节点所有绑定角色均须同意方可通过
    CounterSign,
    /// 或签：本节点任一绑定角色同意即可通过
    OrSign,
}

impl ApprovalMode {
    /// 中文名称
    pub fn label(&self) -> &'static str {
        match self {
            Self::CounterSign => "会签",
            Self::OrSign => "或签",
        }
    }
}

/// 审批流整体状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    /// 草稿（已提交申请，尚未进入流转）
    Draft,
    /// 流转中（等待当前节点审批）
    InProgress,
    /// 审批通过（全部节点完成）
    Approved,
    /// 审批驳回（任一节点驳回）
    Rejected,
    /// 已撤回
    Withdrawn,
}

/// 单条审批决定
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    /// 同意
    Approve,
    /// 驳回
    Reject,
}

/// 流程节点
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowNode {
    /// 节点名称
    pub name: String,
    /// 本节点允许审批的角色（RBAC）
    pub roles: Vec<Role>,
    /// 审批模式（会签 / 或签）
    pub mode: ApprovalMode,
}

/// 审批记录（每次节点操作追加一条）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRecord {
    /// 所属节点索引
    pub node_index: usize,
    /// 节点名称快照
    pub node_name: String,
    /// 审批人 ID
    pub approver_id: Uuid,
    /// 审批人姓名
    pub approver_name: String,
    /// 审批人角色（RBAC 快照，用于会签逐角色校验）
    pub approver_role: Role,
    /// 决定
    pub decision: ApprovalDecision,
    /// 审批意见
    pub comment: String,
    /// 操作时间
    pub at: DateTime<Utc>,
}

/// 审批实例
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInstance {
    /// 实例 ID
    pub id: Uuid,
    /// 审批类型
    pub workflow_type: WorkflowType,
    /// 标题
    pub title: String,
    /// 申请人 ID
    pub applicant_id: Uuid,
    /// 申请人姓名
    pub applicant_name: String,
    /// 租户 ID（学校），用于数据隔离
    pub tenant_id: Uuid,
    /// 当前状态
    pub status: WorkflowStatus,
    /// 流程节点（有序）
    pub nodes: Vec<WorkflowNode>,
    /// 当前所处节点索引
    pub current_node: usize,
    /// 全部审批记录
    pub approvals: Vec<ApprovalRecord>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最近更新时间
    pub updated_at: DateTime<Utc>,
    /// 公文流转审批通过后遗留的 ZXBG01 公文记录
    pub official_doc: Option<drafftink_core::OfficialDoc>,
    /// 类型相关的申请载荷（JSON，保存原始字段）
    pub payload: serde_json::Value,
}

impl WorkflowInstance {
    /// 当前节点（若存在）
    pub fn current(&self) -> Option<&WorkflowNode> {
        self.nodes.get(self.current_node)
    }

    /// 当前节点已产生的审批记录
    pub fn current_approvals(&self) -> Vec<&ApprovalRecord> {
        self.approvals
            .iter()
            .filter(|a| a.node_index == self.current_node)
            .collect()
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  办公管理附属数据（通知公告 / 会议预约 / 消息中心）
// ════════════════════════════════════════════════════════════════════════════

/// 通知公告（对齐 ZXBG0201）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announcement {
    /// 通知编号
    pub notice_id: String,
    /// 标题
    pub title: String,
    /// 发布日期 YYYYMMDD
    pub publish_date: String,
    /// 发布人
    pub publisher: String,
    /// 接收范围
    pub recv_scope: String,
    /// 正文
    pub body: String,
    /// 租户 ID
    pub tenant_id: Uuid,
    /// 是否置顶
    pub pinned: bool,
}

/// 会议预约
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingBooking {
    /// 预约 ID
    pub id: Uuid,
    /// 会议主题
    pub title: String,
    /// 发起人 ID
    pub organizer_id: Uuid,
    /// 发起人姓名
    pub organizer_name: String,
    /// 开始时间（ISO8601）
    pub start_time: DateTime<Utc>,
    /// 结束时间（ISO8601）
    pub end_time: DateTime<Utc>,
    /// 地点
    pub location: String,
    /// 参与人（姓名/工号，逗号分隔）
    pub participants: String,
    /// 租户 ID
    pub tenant_id: Uuid,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

/// 消息中心条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 消息 ID
    pub id: Uuid,
    /// 租户 ID
    pub tenant_id: Uuid,
    /// 接收人 ID（为空表示全员广播）
    pub recipient_id: Option<Uuid>,
    /// 标题
    pub title: String,
    /// 正文
    pub body: String,
    /// 渠道/分类
    pub channel: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 是否已读
    pub read: bool,
}

