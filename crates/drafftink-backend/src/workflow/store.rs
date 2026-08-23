//! # 审批工作流存储（内存实现）
//!
//! 采用 `Arc<Mutex<...>>` 的内存存储，承载：
//! - 审批实例（公文/用印/车辆）
//! - 通知公告、会议预约、消息中心
//! - 审计日志（与 [`drafftink_core::AuditLog`] 对齐）
//!
//! 内存实现便于内网单机部署与单元测试；如需持久化可替换为 sled/数据库实现，
//! 接口保持不变。所有读写均按 `tenant_id` 做数据隔离。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use uuid::Uuid;

use drafftink_core::{AuditAction, AuditLog, Role};

use super::engine::WorkflowEngine;
use super::types::{
    Announcement, MeetingBooking, Message, WorkflowInstance, WorkflowStatus, WorkflowType,
};

/// 工作流与办公数据存储
#[derive(Clone, Default)]
pub struct WorkflowStore {
    workflows: Arc<Mutex<HashMap<Uuid, WorkflowInstance>>>,
    announcements: Arc<Mutex<Vec<Announcement>>>,
    meetings: Arc<Mutex<Vec<MeetingBooking>>>,
    messages: Arc<Mutex<Vec<Message>>>,
    audits: Arc<Mutex<Vec<AuditLog>>>,
}

impl WorkflowStore {
    /// 创建空存储
    pub fn new() -> Self {
        Self::default()
    }

    // ── 审批实例 ────────────────────────────────────────────────────────────

    /// 创建并持久化一个审批实例。
    pub fn create_workflow(&self, instance: WorkflowInstance) -> WorkflowInstance {
        let id = instance.id;
        self.workflows
            .lock()
            .unwrap()
            .insert(id, instance.clone());
        instance
    }

    /// 获取审批实例。
    pub fn get_workflow(&self, id: Uuid) -> Option<WorkflowInstance> {
        self.workflows.lock().unwrap().get(&id).cloned()
    }

    /// 更新审批实例（写回）。
    pub fn save_workflow(&self, instance: &WorkflowInstance) {
        self.workflows
            .lock()
            .unwrap()
            .insert(instance.id, instance.clone());
    }

    /// 列出某租户下的全部审批实例。
    pub fn list_workflows(&self, tenant_id: Uuid) -> Vec<WorkflowInstance> {
        self.workflows
            .lock()
            .unwrap()
            .values()
            .filter(|w| w.tenant_id == tenant_id)
            .cloned()
            .collect()
    }

    /// 列出某审批类型的实例（租户内）。
    pub fn list_by_type(&self, tenant_id: Uuid, t: WorkflowType) -> Vec<WorkflowInstance> {
        self.list_workflows(tenant_id)
            .into_iter()
            .filter(|w| w.workflow_type == t)
            .collect()
    }

    /// 列出待办：当前节点角色包含 `role` 且状态为流转中、且属于该租户的实例。
    pub fn list_todos(&self, tenant_id: Uuid, role: Role) -> Vec<WorkflowInstance> {
        self.workflows
            .lock()
            .unwrap()
            .values()
            .filter(|w| w.tenant_id == tenant_id && w.status == WorkflowStatus::InProgress)
            .filter(|w| WorkflowEngine::can_approve(w, role))
            .cloned()
            .collect()
    }

    // ── 通知公告 ──────────────────────────────────────────────────────────

    /// 发布通知公告。
    pub fn add_announcement(&self, a: Announcement) -> Announcement {
        self.announcements.lock().unwrap().push(a.clone());
        a
    }

    /// 列出某租户的通知公告（置顶优先，新近优先）。
    pub fn list_announcements(&self, tenant_id: Uuid) -> Vec<Announcement> {
        let mut list: Vec<Announcement> = self
            .announcements
            .lock()
            .unwrap()
            .iter()
            .filter(|a| a.tenant_id == tenant_id)
            .cloned()
            .collect();
        list.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| b.publish_date.cmp(&a.publish_date))
        });
        list
    }

    // ── 会议预约 ──────────────────────────────────────────────────────────

    /// 新增会议预约。
    pub fn add_meeting(&self, m: MeetingBooking) -> MeetingBooking {
        self.meetings.lock().unwrap().push(m.clone());
        m
    }

    /// 列出某租户的会议预约（按开始时间升序）。
    pub fn list_meetings(&self, tenant_id: Uuid) -> Vec<MeetingBooking> {
        let mut list: Vec<MeetingBooking> = self
            .meetings
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.tenant_id == tenant_id)
            .cloned()
            .collect();
        list.sort_by_key(|m| m.start_time);
        list
    }

    // ── 消息中心 ──────────────────────────────────────────────────────────

    /// 推送消息（可按 recipient_id 定向，或传 None 广播给全员）。
    pub fn push_message(&self, m: Message) -> Message {
        self.messages.lock().unwrap().push(m.clone());
        m
    }

    /// 列出某用户的消息（定向 + 广播）。
    pub fn list_messages(&self, tenant_id: Uuid, user_id: Uuid) -> Vec<Message> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.tenant_id == tenant_id)
            .filter(|m| m.recipient_id.map(|r| r == user_id).unwrap_or(true))
            .cloned()
            .collect()
    }

    /// 标记消息已读。
    pub fn mark_read(&self, id: Uuid) {
        let mut guard = self.messages.lock().unwrap();
        if let Some(m) = guard.iter_mut().find(|m| m.id == id) {
            m.read = true;
        }
    }

    // ── 审计 ────────────────────────────────────────────────────────────

    /// 写入一条审计日志。
    pub fn audit(
        &self,
        user_id: Uuid,
        action: AuditAction,
        ip: &str,
        device_fp: &str,
        details: &str,
    ) {
        let log = AuditLog {
            id: Uuid::new_v4(),
            user_id,
            action,
            timestamp: Utc::now(),
            ip_address: ip.to_string(),
            device_fp: device_fp.to_string(),
            details: details.to_string(),
        };
        self.audits.lock().unwrap().push(log);
    }

    /// 列出某租户相关的审计日志（按用户 ID 近似过滤；演示用）。
    #[allow(dead_code)]
    pub fn list_audit(&self, user_id: Uuid) -> Vec<AuditLog> {
        self.audits
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.user_id == user_id)
            .cloned()
            .collect()
    }
}
