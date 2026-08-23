//! # 移动办公 REST 接口
//!
//! 覆盖任务要求的全部移动端端点，统一复用既有 [`crate::auth::auth_middleware`] 与 RBAC：
//!
//! | 方法 | 路径 | 说明 |
//! |------|------|------|
//! | POST | `/api/mobile/login` | 登录 + 设备指纹绑定 + 下发短信验证码（MFA 前置） |
//! | POST | `/api/mobile/mfa/verify` | 短信二次验证，通过后签发 SSO 令牌（GB/T 36342-2018） |
//! | GET  | `/api/mobile/sso/token` | 取回已签发的 SSO 令牌 |
//! | GET  | `/api/mobile/todos` | 当前角色的待办审批 |
//! | POST | `/api/mobile/workflow/start` | 发起公文/用印/车辆审批 |
//! | GET  | `/api/mobile/workflow/:id` | 审批详情 |
//! | POST | `/api/mobile/workflow/approve` | 提交审批决定（会签/或签 + RBAC） |
//! | GET  | `/api/mobile/announcements` | 通知公告（ZXBG0201） |
//! | POST | `/api/mobile/meeting/book` | 会议预约 |
//! | POST | `/api/mobile/seal/apply` | 用印申请（Seal 工作流） |
//! | GET  | `/api/mobile/messages` | 消息中心（敏感正文以 SM4 信封加密） |

use axum::extract::{Json, Path, State};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use drafftink_core::auth::{claims_role, ACCESS_TOKEN_TTL_SECS};
use drafftink_core::Role;

use crate::auth::client_ip;
use crate::auth::jwt;
use crate::auth::mobile::{encrypt_json, issue_sso_token};
use crate::auth::{require_role, AuthUser};
use crate::error::AppError;
use crate::state::AppState;
use crate::workflow::engine::{audit_action_for, WorkflowEngine};
use crate::workflow::types::{
    Announcement, ApprovalDecision, MeetingBooking, Message, WorkflowInstance, WorkflowStatus,
    WorkflowType,
};
use crate::workflow::WorkflowStore;

// ════════════════════════════════════════════════════════════════════════════
//  请求 / 响应结构
// ════════════════════════════════════════════════════════════════════════════

/// 移动端登录请求（复用既有 [`drafftink_core::auth::LoginRequest`] 字段语义）
#[derive(Debug, Deserialize)]
pub struct MobileLoginRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub device_fp: String,
}

/// 移动端登录响应
#[derive(Debug, Serialize)]
pub struct MobileLoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub mfa_required: bool,
    pub user: drafftink_core::auth::UserInfo,
}

/// MFA 短信验证请求
#[derive(Debug, Deserialize)]
pub struct MobileMfaVerifyRequest {
    pub access_token: String,
    pub sms_code: String,
}

/// MFA 验证响应（含 SSO 令牌）
#[derive(Debug, Serialize)]
pub struct MobileMfaVerifyResponse {
    pub verified: bool,
    /// 校园级单点登录令牌（GB/T 36342-2018）
    pub sso_token: String,
}

/// SSO 令牌响应
#[derive(Debug, Serialize)]
pub struct SsoTokenResponse {
    pub sso_token: String,
}

/// 发起审批请求
#[derive(Debug, Deserialize)]
pub struct WorkflowStartRequest {
    /// 审批类型：`official_doc` / `seal` / `vehicle`
    pub workflow_type: String,
    /// 标题
    pub title: String,
    /// 类型相关载荷（公文流转需含 doc_type/issue_date/issue_dept/urgency/secret_level）
    pub payload: serde_json::Value,
}

/// 审批决定请求
#[derive(Debug, Deserialize)]
pub struct WorkflowApproveRequest {
    pub workflow_id: Uuid,
    /// `approve` / `reject`
    pub decision: String,
    #[serde(default)]
    pub comment: String,
}

/// 会议预约请求
#[derive(Debug, Deserialize)]
pub struct MeetingBookRequest {
    pub title: String,
    /// RFC3339 时间
    pub start_time: String,
    pub end_time: String,
    pub location: String,
    #[serde(default)]
    pub participants: String,
}

/// 用印申请请求
#[derive(Debug, Deserialize)]
pub struct SealApplyRequest {
    pub title: String,
    pub doc_title: String,
    /// 用印类型（如：公章 / 财务章 / 合同章）
    pub seal_type: String,
    pub reason: String,
}

/// 消息视图（正文以 SM4 信封加密）
#[derive(Debug, Serialize)]
pub struct MessageView {
    pub id: Uuid,
    pub title: String,
    pub channel: String,
    pub created_at: String,
    pub read: bool,
    /// SM4（GB/T 32907-2016）加密正文（Base64）
    pub encrypted_body: String,
}

// ════════════════════════════════════════════════════════════════════════════
//  接口实现
// ════════════════════════════════════════════════════════════════════════════

/// 移动端登录：校验凭证 → 签发令牌（绑定设备指纹）→ 下发短信验证码（MFA 前置）。
pub async fn mobile_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<MobileLoginRequest>,
) -> Result<Json<MobileLoginResponse>, AppError> {
    // 速率限制（防暴力破解）
    state.login_ratelimit.check(client_ip(&headers))?;

    // 查找用户
    let user = state
        .db
        .get_user_by_username(&req.username)?
        .ok_or_else(|| AppError::Unauthorized("用户名或密码错误".to_string()))?;
    if !user.active {
        return Err(AppError::Unauthorized("账户已被禁用".to_string()));
    }
    if !crate::auth::password::verify_password(&req.password, &user.password_hash) {
        return Err(AppError::Unauthorized("用户名或密码错误".to_string()));
    }

    // 设备指纹（请求头优先，其次请求体）
    let device_fp = if req.device_fp.is_empty() {
        headers
            .get("x-device-fp")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string()
    } else {
        req.device_fp.clone()
    };

    // 签发访问 / 刷新令牌（绑定 device_fp）
    let secret = state.config.jwt.secret.clone();
    let access = jwt::generate_access_token(&user, &device_fp, &secret)?;
    let (refresh, jti, exp) = jwt::generate_refresh_token(&user, &secret)?;
    state.refresh_store.store(&jti, exp);

    // 下发短信验证码（演示：日志输出，生产经短信网关）
    let sms_code = state.mobile_auth.sms.issue(user.id);
    tracing::info!(user = %user.username, code = %sms_code, "MFA 短信验证码已下发（演示）");

    Ok(Json(MobileLoginResponse {
        access_token: access,
        refresh_token: refresh,
        token_type: "Bearer".to_string(),
        expires_in: ACCESS_TOKEN_TTL_SECS,
        mfa_required: true,
        user: drafftink_core::auth::UserInfo::from(&user),
    }))
}

/// 短信二次验证：校验成功后签发校园级 SSO 令牌（GB/T 36342-2018）。
pub async fn mfa_verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<MobileMfaVerifyRequest>,
) -> Result<Json<MobileMfaVerifyResponse>, AppError> {
    // 校验既有访问令牌
    let claims = jwt::verify_access_token(&req.access_token, &state.config.jwt.secret)?;

    // 设备指纹绑定校验
    let req_fp = headers
        .get("x-device-fp")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if !req_fp.is_empty() && req_fp != claims.device_fp {
        return Err(AppError::Forbidden(
            "设备指纹与登录设备不一致，拒绝 MFA 验证".to_string(),
        ));
    }

    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::BadRequest("无效的用户标识".to_string()))?;

    // 校验短信验证码（一次性）
    if !state.mobile_auth.sms.verify(user_id, &req.sms_code) {
        return Err(AppError::BadRequest("短信验证码错误 or 已过期".to_string()));
    }

    // 签发 SSO 令牌（GB/T 36342-2018 单点登录）
    let sso_token = issue_sso_token(
        &state.config.jwt.secret,
        &claims.sub,
        &claims.name,
        &claims.role,
        claims.class_id.as_deref(),
        &claims.device_fp,
        &claims.tenant_id,
    )?;

    // 登记 MFA 会话（与访问令牌 jti 绑定），供后续取回 SSO 令牌
    state.mobile_auth.sessions.mark_verified(
        &claims.jti,
        user_id,
        &claims.device_fp,
        sso_token.clone(),
    );

    // 审计
    state.workflow.audit(
        user_id,
        drafftink_core::AuditAction::MfaVerify,
        &client_ip(&headers).to_string(),
        &claims.device_fp,
        "移动端 MFA 短信二次验证通过",
    );

    Ok(Json(MobileMfaVerifyResponse {
        verified: true,
        sso_token,
    }))
}

/// 取回已签发的 SSO 令牌（需先完成 MFA）。
pub async fn sso_token(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<SsoTokenResponse>, AppError> {
    let jti = auth.0.jti.clone();
    match state.mobile_auth.sessions.take_sso_ticket(&jti) {
        Some(token) => Ok(Json(SsoTokenResponse { sso_token: token })),
        None => Err(AppError::Unauthorized(
            "尚未完成 MFA 验证，无法获取 SSO 令牌".to_string(),
        )),
    }
}

/// 当前角色的待办审批。
pub async fn todos(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<WorkflowInstance>>, AppError> {
    let role = claims_role(&auth.0);
    let tenant = Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default();
    let list = state.workflow.list_todos(tenant, role);
    Ok(Json(list))
}

/// 发起审批（公文 / 用印 / 车辆）。仅教师 / 管理员可发起。
pub async fn workflow_start(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<WorkflowStartRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_role(&auth, &[Role::Admin, Role::Teacher])?;
    let wf_type = parse_workflow_type(&req.workflow_type)?;

    let applicant_id = Uuid::parse_str(&auth.0.sub).unwrap_or_default();
    let tenant = Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default();

    let instance = WorkflowEngine::start(
        wf_type,
        req.title.clone(),
        applicant_id,
        auth.0.name.clone(),
        tenant,
        req.payload.clone(),
    );
    let saved = state.workflow.create_workflow(instance);

    // 审计
    state.workflow.audit(
        applicant_id,
        drafftink_core::AuditAction::ApprovalSubmit,
        "mobile",
        &auth.0.device_fp,
        &format!("发起{}：{}", wf_type.label(), req.title),
    );
    // 通知全员有待办（演示：广播消息）
    push_broadcast(
        &state.workflow,
        tenant,
        "新的审批待办",
        &format!("「{}」已提交，等待审批", req.title),
        "approval",
    );

    let advice = WorkflowEngine::advice(&saved);
    Ok(Json(json!({
        "workflow": saved,
        "ai_advice": advice,
    })))
}

/// 审批详情。
pub async fn workflow_get(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkflowInstance>, AppError> {
    let tenant = Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default();
    let instance = state
        .workflow
        .get_workflow(id)
        .ok_or_else(|| AppError::NotFound("审批实例不存在".to_string()))?;
    if instance.tenant_id != tenant {
        return Err(AppError::Forbidden("无权访问其他租户的审批数据".to_string()));
    }
    Ok(Json(instance))
}

/// 提交审批决定（RBAC + 会签/或签）。
pub async fn workflow_approve(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<WorkflowApproveRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let role = claims_role(&auth.0);
    let user_id = Uuid::parse_str(&auth.0.sub).unwrap_or_default();
    let tenant = Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default();

    let mut instance = state
        .workflow
        .get_workflow(req.workflow_id)
        .ok_or_else(|| AppError::NotFound("审批实例不存在".to_string()))?;
    if instance.tenant_id != tenant {
        return Err(AppError::Forbidden("无权操作其他租户的审批数据".to_string()));
    }

    // RBAC：当前角色必须属于当前节点
    if !WorkflowEngine::can_approve(&instance, role) {
        return Err(AppError::Forbidden(
            "当前角色无权审批该节点".to_string(),
        ));
    }

    let decision = match req.decision.as_str() {
        "approve" => ApprovalDecision::Approve,
        "reject" => ApprovalDecision::Reject,
        _ => return Err(AppError::BadRequest("decision 必须为 approve/reject".to_string())),
    };

    WorkflowEngine::apply_decision(
        &mut instance,
        decision,
        user_id,
        auth.0.name.clone(),
        role,
        req.comment.clone(),
    )?;

    state.workflow.save_workflow(&instance);

    // 审计
    state.workflow.audit(
        user_id,
        audit_action_for(decision),
        "mobile",
        &auth.0.device_fp,
        &format!("审批「{}」→ {:?}", instance.title, decision),
    );

    // 若审批通过，通知申请人
    if instance.status == WorkflowStatus::Approved {
        push_to_user(
            &state.workflow,
            tenant,
            instance.applicant_id,
            "审批通过",
            &format!("「{}」已审批通过", instance.title),
            "approval",
        );
    }

    let advice = WorkflowEngine::advice(&instance);
    Ok(Json(json!({
        "workflow": instance,
        "ai_advice": advice,
    })))
}

/// 通知公告（ZXBG0201）。
pub async fn announcements(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<Announcement>>, AppError> {
    let tenant = Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default();
    let list = state.workflow.list_announcements(tenant);
    Ok(Json(list))
}

/// 会议预约。
pub async fn meeting_book(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<MeetingBookRequest>,
) -> Result<Json<MeetingBooking>, AppError> {
    let tenant = Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default();
    let organizer_id = Uuid::parse_str(&auth.0.sub).unwrap_or_default();

    let start = parse_dt(&req.start_time)?;
    let end = parse_dt(&req.end_time)?;
    if end <= start {
        return Err(AppError::BadRequest("结束时间须晚于开始时间".to_string()));
    }

    let booking = MeetingBooking {
        id: Uuid::new_v4(),
        title: req.title.clone(),
        organizer_id,
        organizer_name: auth.0.name.clone(),
        start_time: start,
        end_time: end,
        location: req.location.clone(),
        participants: req.participants.clone(),
        tenant_id: tenant,
        created_at: chrono::Utc::now(),
    };
    let saved = state.workflow.add_meeting(booking.clone());

    state.workflow.audit(
        organizer_id,
        drafftink_core::AuditAction::MeetingBook,
        "mobile",
        &auth.0.device_fp,
        &format!("预约会议：{} @ {}", req.title, req.location),
    );

    Ok(Json(saved))
}

/// 用印申请（Seal 工作流）。
pub async fn seal_apply(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<SealApplyRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_role(&auth, &[Role::Admin, Role::Teacher])?;
    let applicant_id = Uuid::parse_str(&auth.0.sub).unwrap_or_default();
    let tenant = Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default();

    let payload = json!({
        "doc_title": req.doc_title,
        "seal_type": req.seal_type,
        "reason": req.reason,
    });

    let instance = WorkflowEngine::start(
        WorkflowType::Seal,
        req.title.clone(),
        applicant_id,
        auth.0.name.clone(),
        tenant,
        payload,
    );
    let saved = state.workflow.create_workflow(instance);
    state.workflow.audit(
        applicant_id,
        drafftink_core::AuditAction::SealApply,
        "mobile",
        &auth.0.device_fp,
        &format!("用印申请：{}（{}）", req.title, req.seal_type),
    );
    push_broadcast(
        &state.workflow,
        tenant,
        "新的用印待办",
        &format!("「{}」提交用印申请", req.title),
        "approval",
    );

    let advice = WorkflowEngine::advice(&saved);
    Ok(Json(json!({
        "workflow": saved,
        "ai_advice": advice,
    })))
}

/// 消息中心：返回消息列表，敏感正文以 SM4 信封加密（GB/T 32907-2016）。
pub async fn messages(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<MessageView>>, AppError> {
    let tenant = Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default();
    let user_id = Uuid::parse_str(&auth.0.sub).unwrap_or_default();

    let list: Vec<MessageView> = state
        .workflow
        .list_messages(tenant, user_id)
        .into_iter()
        .map(|m| {
            let encrypted_body =
                encrypt_json(&auth.0.device_fp, &state.config.jwt.secret, &m.body)
                    .unwrap_or_default();
            MessageView {
                id: m.id,
                title: m.title,
                channel: m.channel,
                created_at: m.created_at.to_rfc3339(),
                read: m.read,
                encrypted_body,
            }
        })
        .collect();
    Ok(Json(list))
}

// ════════════════════════════════════════════════════════════════════════════
//  辅助函数
// ════════════════════════════════════════════════════════════════════════════

fn parse_workflow_type(s: &str) -> Result<WorkflowType, AppError> {
    match s {
        "official_doc" | "official" | "doc" => Ok(WorkflowType::OfficialDoc),
        "seal" | "use_seal" => Ok(WorkflowType::Seal),
        "vehicle" | "car" => Ok(WorkflowType::Vehicle),
        _ => Err(AppError::BadRequest(format!(
            "未知审批类型: {s}（应为 official_doc/seal/vehicle）"
        ))),
    }
}

fn parse_dt(s: &str) -> Result<chrono::DateTime<chrono::Utc>, AppError> {
    s.parse::<chrono::DateTime<chrono::Utc>>()
        .map_err(|_| AppError::BadRequest(format!("时间格式应为 RFC3339: {s}")))
}

fn push_broadcast(
    store: &WorkflowStore,
    tenant: Uuid,
    title: &str,
    body: &str,
    channel: &str,
) {
    store.push_message(Message {
        id: Uuid::new_v4(),
        tenant_id: tenant,
        recipient_id: None,
        title: title.to_string(),
        body: body.to_string(),
        channel: channel.to_string(),
        created_at: chrono::Utc::now(),
        read: false,
    });
}

fn push_to_user(
    store: &WorkflowStore,
    tenant: Uuid,
    user_id: Uuid,
    title: &str,
    body: &str,
    channel: &str,
) {
    store.push_message(Message {
        id: Uuid::new_v4(),
        tenant_id: tenant,
        recipient_id: Some(user_id),
        title: title.to_string(),
        body: body.to_string(),
        channel: channel.to_string(),
        created_at: chrono::Utc::now(),
        read: false,
    });
}
