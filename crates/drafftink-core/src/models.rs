//! # 公共数据模型
//!
//! 作业、课件、用户、班级等核心业务数据结构。
//! 与 `model.rs`（画板元素模型）区分：此模块专注于业务实体。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ════════════════════════════════════════════════════════════════════════════
//  用户与权限
// ════════════════════════════════════════════════════════════════════════════

/// 用户角色（RBAC）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// 校长 — 全校数据访问权限
    Admin,
    /// 老师 — 仅访问自己班级的数据
    Teacher,
    /// 学生 — 仅访问自己的作业
    #[default]
    Student,
}

impl Role {
    /// 是否有管理权限
    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Admin)
    }

    /// 是否有老师权限（含校长）
    pub fn is_teacher(&self) -> bool {
        matches!(self, Self::Admin | Self::Teacher)
    }

    /// 转为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Teacher => "teacher",
            Self::Student => "student",
        }
    }

    /// 从字符串解析角色（大小写不敏感），遵循 `FromStr` 约定。
    pub fn from_str_insensitive(s: &str) -> Option<Role> {
        s.to_lowercase().parse::<Role>().ok()
    }
}

impl std::str::FromStr for Role {
    type Err = ();

    /// 从字符串解析角色（大小写敏感）。
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(Role::Admin),
            "teacher" => Ok(Role::Teacher),
            "student" => Ok(Role::Student),
            _ => Err(()),
        }
    }
}

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// 用户唯一 ID
    pub id: Uuid,
    /// 用户名（登录用）
    pub username: String,
    /// 显示名称
    pub display_name: String,
    /// 角色
    pub role: Role,
    /// 所属班级 ID（学生专用）
    pub class_id: Option<Uuid>,
    /// 租户 ID（学校 ID），用于多租户数据隔离
    pub tenant_id: Uuid,
    /// 密码哈希（Argon2，不存储明文）
    #[serde(skip)]
    pub password_hash: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 是否启用
    pub active: bool,
}

impl Default for User {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            username: String::new(),
            display_name: String::new(),
            role: Role::Student,
            class_id: None,
            tenant_id: Uuid::nil(),
            password_hash: String::new(),
            created_at: Utc::now(),
            active: true,
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  班级
// ════════════════════════════════════════════════════════════════════════════

/// 班级信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Class {
    /// 班级唯一 ID
    pub id: Uuid,
    /// 班级名称（如 "三年二班"）
    pub name: String,
    /// 年级
    pub grade: String,
    /// 班主任 ID
    pub teacher_id: Option<Uuid>,
    /// 学校 ID
    pub school_id: Uuid,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

impl Default for Class {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            grade: String::new(),
            teacher_id: None,
            school_id: Uuid::nil(),
            created_at: Utc::now(),
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  作业
// ════════════════════════════════════════════════════════════════════════════

/// 作业状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HomeworkStatus {
    /// 未发布（草稿）
    #[default]
    Draft,
    /// 已发布，学生可提交
    Published,
    /// 已截止，不再接受提交
    Closed,
    /// 已归档
    Archived,
}

/// 作业定义（老师布置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Homework {
    /// 作业唯一 ID
    pub id: Uuid,
    /// 标题
    pub title: String,
    /// 描述/要求
    pub description: String,
    /// 布置老师 ID
    pub teacher_id: Uuid,
    /// 班级 ID
    pub class_id: Uuid,
    /// 作业内容（drft 课件数据或自定义题目）
    pub content: Vec<u8>,
    /// 布置时间
    pub created_at: DateTime<Utc>,
    /// 截止时间
    pub deadline: DateTime<Utc>,
    /// 状态
    pub status: HomeworkStatus,
    /// 附件资源 ID 列表
    pub attachment_ids: Vec<Uuid>,
}

impl Default for Homework {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            title: String::new(),
            description: String::new(),
            teacher_id: Uuid::nil(),
            class_id: Uuid::nil(),
            content: Vec::new(),
            created_at: Utc::now(),
            deadline: Utc::now() + chrono::Duration::days(7),
            status: HomeworkStatus::Draft,
            attachment_ids: Vec::new(),
        }
    }
}

/// 作业提交状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionStatus {
    /// 未提交
    #[default]
    NotSubmitted,
    /// 已提交（等待批改）
    Submitted,
    /// 已批改
    Graded,
    /// 已退回（需重做）
    Returned,
}

/// 作业提交记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeworkSubmission {
    /// 提交记录唯一 ID
    pub id: Uuid,
    /// 作业 ID
    pub homework_id: Uuid,
    /// 学生 ID
    pub student_id: Uuid,
    /// drftx 文件路径（存储在 MinIO/本地）
    pub drftx_path: String,
    /// 提交时间
    pub submitted_at: DateTime<Utc>,
    /// 提交状态
    pub status: SubmissionStatus,
    /// 快照内容哈希（用于查重和完整性验证）
    pub content_hash: String,
    /// 分数（批改后填入）
    pub score: Option<f32>,
    /// 批改老师 ID
    pub graded_by: Option<Uuid>,
    /// 批改时间
    pub graded_at: Option<DateTime<Utc>>,
}

impl Default for HomeworkSubmission {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            homework_id: Uuid::nil(),
            student_id: Uuid::nil(),
            drftx_path: String::new(),
            submitted_at: Utc::now(),
            status: SubmissionStatus::NotSubmitted,
            content_hash: String::new(),
            score: None,
            graded_by: None,
            graded_at: None,
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  学校
// ════════════════════════════════════════════════════════════════════════════

/// 学校信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct School {
    /// 学校唯一 ID
    pub id: Uuid,
    /// 学校名称
    pub name: String,
    /// 学校代码
    pub code: String,
    /// 联系人
    pub contact: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

impl Default for School {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            code: String::new(),
            contact: String::new(),
            created_at: Utc::now(),
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  审计日志
// ════════════════════════════════════════════════════════════════════════════

/// 操作类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    Login,
    Logout,
    HomeworkCreate,
    HomeworkSubmit,
    HomeworkGrade,
    ResourceUpload,
    ResourceDownload,
    ConfigChange,
    Backup,
    Export,
    Import,
    /// 移动办公：提交审批申请（公文/用印/车辆）
    ApprovalSubmit,
    /// 移动办公：审批通过
    ApprovalApprove,
    /// 移动办公：审批驳回
    ApprovalReject,
    /// 移动办公：MFA 短信二次验证通过
    MfaVerify,
    /// 移动办公：用印申请
    SealApply,
    /// 移动办公：会议预约
    MeetingBook,
    /// 移动办公：通知公告发布
    AnnouncePublish,
}

/// 审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    /// 日志 ID
    pub id: Uuid,
    /// 操作者 ID
    pub user_id: Uuid,
    /// 操作类型
    pub action: AuditAction,
    /// 操作时间
    pub timestamp: DateTime<Utc>,
    /// IP 地址
    pub ip_address: String,
    /// 设备指纹
    pub device_fp: String,
    /// 操作详情（JSON）
    pub details: String,
}

// ════════════════════════════════════════════════════════════════════════════
//  单元测试
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_permissions() {
        assert!(Role::Admin.is_admin());
        assert!(Role::Admin.is_teacher());
        assert!(!Role::Teacher.is_admin());
        assert!(Role::Teacher.is_teacher());
        assert!(!Role::Student.is_admin());
        assert!(!Role::Student.is_teacher());
    }

    #[test]
    fn test_role_serde() {
        let json = serde_json::to_string(&Role::Teacher).unwrap();
        assert_eq!(json, "\"teacher\"");

        let role: Role = serde_json::from_str("\"admin\"").unwrap();
        assert_eq!(role, Role::Admin);
    }

    #[test]
    fn test_homework_default() {
        let hw = Homework::default();
        assert_eq!(hw.status, HomeworkStatus::Draft);
        assert!(hw.content.is_empty());
        assert!(hw.deadline > hw.created_at);
    }

    #[test]
    fn test_submission_default() {
        let sub = HomeworkSubmission::default();
        assert_eq!(sub.status, SubmissionStatus::NotSubmitted);
        assert!(sub.score.is_none());
    }

    #[test]
    fn test_user_serde_roundtrip() {
        let user = User {
            id: Uuid::new_v4(),
            username: "teacher01".to_string(),
            display_name: "王老师".to_string(),
            role: Role::Teacher,
            class_id: Some(Uuid::new_v4()),
            tenant_id: Uuid::nil(),
            password_hash: "hashed_password".to_string(),
            created_at: Utc::now(),
            active: true,
        };

        let json = serde_json::to_string(&user).unwrap();
        let restored: User = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.username, user.username);
        assert_eq!(restored.role, user.role);
        assert_eq!(restored.active, user.active);
        // password_hash should not be serialized
        assert!(!json.contains("hashed_password"));
    }

    #[test]
    fn test_audit_log_creation() {
        let log = AuditLog {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            action: AuditAction::HomeworkSubmit,
            timestamp: Utc::now(),
            ip_address: "192.168.1.100".to_string(),
            device_fp: "abc123".to_string(),
            details: r#"{"homework_id":"..."}"#.to_string(),
        };

        let json = serde_json::to_string(&log).unwrap();
        assert!(json.contains("homework_submit"));
    }
}
