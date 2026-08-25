//! # 审批工作流引擎
//!
//! 纯函数式的状态机：根据审批类型提供流程模板（节点 + 绑定角色 + 会签/或签），
//! 并负责在每次审批操作后推进/驳回实例。
//!
//! 引擎不持有状态，状态由 [`crate::workflow::store::WorkflowStore`] 持久化；
//! Handler 负责加载实例 → 调用 [`WorkflowEngine::apply_decision`] 推进 → 回写存储。
//!
//! 可选 AI 顾问：在 [`WorkflowEngine::advice`] 提供基于规则的审批建议（演示用，
//! 不依赖外部模型服务，保证离线可用、数据不出校）。

use chrono::Utc;
use uuid::Uuid;

use drafftink_core::{AuditAction, EmgiRecordable, OfficialDoc, Role};

use super::types::{
    ApprovalDecision, ApprovalMode, ApprovalRecord, WorkflowInstance, WorkflowNode, WorkflowStatus,
    WorkflowType,
};
use crate::error::AppError;

/// 引擎错误（映射到 HTTP 语义）
#[derive(Debug)]
pub enum WorkflowError {
    /// 实例状态不允许该操作（如已结束）
    InvalidState(WorkflowStatus),
    /// 当前角色无权审批当前节点
    Forbidden,
    /// 审批人重复审批同一节点
    Duplicate,
    /// ZXBG01 公文数据校验失败
    DocInvalid(String),
}

impl std::fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidState(s) => write!(f, "审批流当前状态不允许此操作: {s:?}"),
            Self::Forbidden => write!(f, "当前角色无权审批该节点"),
            Self::Duplicate => write!(f, "您已对该节点作出过审批，不能重复提交"),
            Self::DocInvalid(m) => write!(f, "公文数据校验失败: {m}"),
        }
    }
}

impl std::error::Error for WorkflowError {}

impl From<WorkflowError> for AppError {
    fn from(e: WorkflowError) -> Self {
        match e {
            WorkflowError::InvalidState(_) => AppError::BadRequest(e.to_string()),
            WorkflowError::Forbidden => AppError::Forbidden(e.to_string()),
            WorkflowError::Duplicate => AppError::BadRequest(e.to_string()),
            WorkflowError::DocInvalid(m) => AppError::BadRequest(m),
        }
    }
}

/// 审批工作流引擎
pub struct WorkflowEngine;

impl WorkflowEngine {
    /// 获取某审批类型的默认流程模板（节点顺序即审批顺序）。
    ///
    /// 设计依据（RBAC 角色绑定）：
    /// - 公文流转：部门负责人 → 分管校领导（会签：校领导集体须全部同意）。
    /// - 用印申请：部门负责人 → 校办（或签：校办任一人员可盖）。
    /// - 车辆申请：部门负责人 → 后勤（或签）。
    pub fn template(t: WorkflowType) -> Vec<WorkflowNode> {
        match t {
            WorkflowType::OfficialDoc => vec![
                WorkflowNode {
                    name: "部门负责人审核".into(),
                    roles: vec![Role::Admin, Role::Teacher],
                    mode: ApprovalMode::OrSign,
                },
                WorkflowNode {
                    name: "分管校领导审批".into(),
                    roles: vec![Role::Admin],
                    mode: ApprovalMode::CounterSign,
                },
            ],
            WorkflowType::Seal => vec![
                WorkflowNode {
                    name: "部门负责人审核".into(),
                    roles: vec![Role::Admin, Role::Teacher],
                    mode: ApprovalMode::OrSign,
                },
                WorkflowNode {
                    name: "校办用印审批".into(),
                    roles: vec![Role::Admin],
                    mode: ApprovalMode::OrSign,
                },
            ],
            WorkflowType::Vehicle => vec![
                WorkflowNode {
                    name: "部门负责人审核".into(),
                    roles: vec![Role::Admin, Role::Teacher],
                    mode: ApprovalMode::OrSign,
                },
                WorkflowNode {
                    name: "后勤调度审批".into(),
                    roles: vec![Role::Admin],
                    mode: ApprovalMode::OrSign,
                },
            ],
        }
    }

    /// 创建审批实例（草稿 → 流转中，当前节点指向首个节点）。
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        workflow_type: WorkflowType,
        title: String,
        applicant_id: Uuid,
        applicant_name: String,
        tenant_id: Uuid,
        payload: serde_json::Value,
    ) -> WorkflowInstance {
        let now = Utc::now();
        WorkflowInstance {
            id: Uuid::new_v4(),
            workflow_type,
            title,
            applicant_id,
            applicant_name,
            tenant_id,
            status: WorkflowStatus::InProgress,
            nodes: Self::template(workflow_type),
            current_node: 0,
            approvals: Vec::new(),
            created_at: now,
            updated_at: now,
            official_doc: None,
            payload,
        }
    }

    /// 校验调用者角色是否有权审批当前节点（RBAC）。
    pub fn can_approve(instance: &WorkflowInstance, role: Role) -> bool {
        match instance.current() {
            Some(node) => node.roles.contains(&role),
            None => false,
        }
    }

    /// 应用一次审批决定，原地推进实例状态。
    ///
    /// 推进规则：
    /// - 状态必须为 `InProgress`，否则 [`WorkflowError::InvalidState`]。
    /// - `approver_role` 须属于当前节点绑定角色，否则 [`WorkflowError::Forbidden`]。
    /// - 同一审批人对同一节点不可重复，否则 [`WorkflowError::Duplicate`]。
    /// - 追加审批记录后按当前节点模式判定：
    ///   - 任一 `Reject` → 整体 `Rejected`；
    ///   - `或签`：存在任一 `Approve` 即视为节点通过；
    ///   - `会签`：节点绑定角色每个至少一条 `Approve` 方通过；
    ///   - 节点通过且非末节点 → `current_node += 1`，仍为 `InProgress`；
    ///   - 节点通过且为末节点 → `Approved`，若为公文流转则生成 ZXBG01 记录。
    pub fn apply_decision(
        instance: &mut WorkflowInstance,
        decision: ApprovalDecision,
        approver_id: Uuid,
        approver_name: String,
        approver_role: Role,
        comment: String,
    ) -> Result<(), WorkflowError> {
        if instance.status != WorkflowStatus::InProgress {
            return Err(WorkflowError::InvalidState(instance.status));
        }
        let node = instance
            .current()
            .ok_or(WorkflowError::InvalidState(instance.status))?
            .clone();

        // RBAC：审批人角色必须属于当前节点
        if !node.roles.contains(&approver_role) {
            return Err(WorkflowError::Forbidden);
        }

        // 防止同一审批人重复审批同一节点
        if instance
            .current_approvals()
            .iter()
            .any(|a| a.approver_id == approver_id)
        {
            return Err(WorkflowError::Duplicate);
        }

        instance.approvals.push(ApprovalRecord {
            node_index: instance.current_node,
            node_name: node.name.clone(),
            approver_id,
            approver_name,
            approver_role,
            decision,
            comment,
            at: Utc::now(),
        });

        // 若本次为驳回，整体直接驳回
        if decision == ApprovalDecision::Reject {
            instance.status = WorkflowStatus::Rejected;
            instance.updated_at = Utc::now();
            return Ok(());
        }

        // 判定当前节点是否满足通过条件
        let node_approvals = instance.current_approvals();
        let satisfied = match node.mode {
            ApprovalMode::OrSign => node_approvals
                .iter()
                .any(|a| a.decision == ApprovalDecision::Approve),
            ApprovalMode::CounterSign => node.roles.iter().all(|r| {
                node_approvals
                    .iter()
                    .any(|a| a.approver_role == *r && a.decision == ApprovalDecision::Approve)
            }),
        };

        if satisfied {
            if instance.current_node + 1 >= instance.nodes.len() {
                instance.status = WorkflowStatus::Approved;
                // 公文流转：生成 ZXBG01 公文记录
                if instance.workflow_type == WorkflowType::OfficialDoc {
                    instance.official_doc = Some(build_official_doc(instance)?);
                }
            } else {
                instance.current_node += 1;
            }
        }

        instance.updated_at = Utc::now();
        Ok(())
    }

    /// 可选的 AI 审批顾问（演示：基于规则生成建议，不依赖外部服务，数据不出校）。
    pub fn advice(instance: &WorkflowInstance) -> String {
        let urgency = instance
            .payload
            .get("urgency")
            .and_then(|v| v.as_str())
            .unwrap_or("2");
        let secret = instance
            .payload
            .get("secret_level")
            .and_then(|v| v.as_str())
            .unwrap_or("0");
        let mut tips = Vec::new();
        match urgency {
            "1" => tips.push("紧急程度为特急，建议优先处理并电话确认。"),
            "2" => tips.push("加急件，请在一个工作日内完成审批。"),
            _ => tips.push("普通件，可按常规流程处理。"),
        }
        if secret != "0" {
            tips.push("涉及涉密内容，请于涉密计算机环境办理并核对知悉范围。");
        }
        if instance.workflow_type == WorkflowType::Seal {
            tips.push("用印前请核验用印文件与审批内容一致性，落实监印人责任。");
        }
        if instance.workflow_type == WorkflowType::Vehicle {
            tips.push("车辆申请请确认乘车人、目的地与返回时间，做好出行安全告知。");
        }
        tips.join(" ")
    }
}

/// 由审批实例载荷构造 ZXBG01 公文数据类（审批通过时调用）。
fn build_official_doc(instance: &WorkflowInstance) -> Result<OfficialDoc, WorkflowError> {
    let p = &instance.payload;
    let get = |k: &str| -> String {
        p.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let doc = OfficialDoc {
        doc_id: format!("ZW{}", &instance.id.simple().to_string()[..8]),
        title: instance.title.clone(),
        doc_type: get("doc_type"),
        issue_date: get("issue_date"),
        issue_dept: get("issue_dept"),
        urgency: get("urgency"),
        secret_level: get("secret_level"),
        approval_status: "20".to_string(), // 审批通过
    };
    doc.validate()
        .map_err(|e| WorkflowError::DocInvalid(format!("{e:?}")))?;
    Ok(doc)
}

/// 审计动作映射（供 Handler 记录日志）。
pub fn audit_action_for(decision: ApprovalDecision) -> AuditAction {
    match decision {
        ApprovalDecision::Approve => AuditAction::ApprovalApprove,
        ApprovalDecision::Reject => AuditAction::ApprovalReject,
    }
}
