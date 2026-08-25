//! # 作业接口
//!
//! - `POST /api/homework/create` — 老师创建作业
//! - `GET /api/homework/list` — 列出作业
//! - `GET /api/homework/:id` — 获取作业详情
//! - `POST /api/homework/submit` — 学生提交 drftx 文件
//! - `POST /api/homework/grade` — 老师批改提交

use axum::extract::{Multipart, Path, State};
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use drafftink_core::{
    DrftxFile, Homework, HomeworkStatus, HomeworkSubmission, SubmissionStatus, TeacherAnnotation,
};

use drafftink_core::Role;

use crate::auth::rbac;
use crate::auth::{require_role, AuthUser};
use crate::error::AppError;
use crate::state::AppState;

// ════════════════════════════════════════════════════════════════════════════
//  创建作业
// ════════════════════════════════════════════════════════════════════════════

/// 创建作业请求
#[derive(Debug, Deserialize)]
pub struct CreateHomeworkRequest {
    pub title: String,
    pub description: String,
    pub class_id: Uuid,
    /// 作业内容（Base64 编码）
    pub content: String,
    /// 截止时间（ISO 8601）
    pub deadline: String,
}

/// 创建作业响应
#[derive(Debug, Serialize)]
pub struct CreateHomeworkResponse {
    pub id: Uuid,
    pub title: String,
    pub status: String,
}

/// POST /api/homework/create
pub async fn create(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateHomeworkRequest>,
) -> Result<Json<CreateHomeworkResponse>, AppError> {
    require_role(&auth, &[Role::Teacher, Role::Admin])?;

    let teacher_id = Uuid::parse_str(&auth.0.sub)
        .map_err(|_| AppError::Internal("JWT sub 不是有效的 UUID".to_string()))?;

    // 验证老师拥有该班级，且该班级属于老师的租户（数据隔离）
    let teacher_tenant = Uuid::parse_str(&auth.0.tenant_id)
        .map_err(|_| AppError::Internal("JWT tenant_id 不是有效的 UUID".to_string()))?;
    rbac::check_teacher_owns_class_in_tenant(
        state.db.as_ref(),
        teacher_id,
        req.class_id,
        teacher_tenant,
    )?;

    // 解码 Base64 内容
    let content = drafftink_core::utils::base64_decode(&req.content)
        .map_err(|e| AppError::BadRequest(format!("Base64 解码失败: {e}")))?;

    // 解析截止时间
    let deadline = chrono::DateTime::parse_from_rfc3339(&req.deadline)
        .map_err(|e| AppError::BadRequest(format!("截止时间格式错误: {e}")))?
        .with_timezone(&Utc);

    let hw = Homework {
        id: Uuid::new_v4(),
        title: req.title.clone(),
        description: req.description,
        teacher_id,
        class_id: req.class_id,
        content,
        created_at: Utc::now(),
        deadline,
        status: HomeworkStatus::Published,
        attachment_ids: Vec::new(),
    };

    state.db.save_homework(&hw)?;

    Ok(Json(CreateHomeworkResponse {
        id: hw.id,
        title: hw.title,
        status: "published".to_string(),
    }))
}

// ════════════════════════════════════════════════════════════════════════════
//  列出作业
// ════════════════════════════════════════════════════════════════════════════

/// 作业列表项
#[derive(Debug, Serialize)]
pub struct HomeworkListItem {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub class_id: Uuid,
    pub deadline: chrono::DateTime<Utc>,
    pub status: String,
}

impl From<Homework> for HomeworkListItem {
    fn from(hw: Homework) -> Self {
        Self {
            id: hw.id,
            title: hw.title,
            description: hw.description,
            class_id: hw.class_id,
            deadline: hw.deadline,
            status: format!("{:?}", hw.status).to_lowercase(),
        }
    }
}

/// GET /api/homework/list
///
/// 老师看到自己布置的作业，学生看到所在班级的作业。
pub async fn list(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<HomeworkListItem>>, AppError> {
    let user_id = Uuid::parse_str(&auth.0.sub)
        .map_err(|_| AppError::Internal("JWT sub 不是有效的 UUID".to_string()))?;

    let homeworks = if auth.0.role == "student" {
        // 学生：根据 class_id 查找
        let class_id = auth
            .0
            .class_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| AppError::BadRequest("学生未关联班级".to_string()))?;
        state.db.list_homework_by_class(class_id)?
    } else {
        // 老师/管理员：查看自己布置的作业
        state.db.list_homework_by_teacher(user_id)?
    };

    let items: Vec<HomeworkListItem> = homeworks.into_iter().map(Into::into).collect();
    Ok(Json(items))
}

// ════════════════════════════════════════════════════════════════════════════
//  获取作业详情
// ════════════════════════════════════════════════════════════════════════════

/// 作业详情响应
#[derive(Debug, Serialize)]
pub struct HomeworkDetail {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub teacher_id: Uuid,
    pub class_id: Uuid,
    pub content: String, // Base64 编码
    pub created_at: chrono::DateTime<Utc>,
    pub deadline: chrono::DateTime<Utc>,
    pub status: String,
}

/// GET /api/homework/:id
pub async fn get(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<HomeworkDetail>, AppError> {
    let hw = state
        .db
        .get_homework(id)?
        .ok_or_else(|| AppError::NotFound(format!("作业不存在: {id}")))?;

    // 权限检查：老师需拥有该作业，学生需在该班级
    let user_id = Uuid::parse_str(&auth.0.sub)
        .map_err(|_| AppError::Internal("JWT sub 不是有效的 UUID".to_string()))?;

    if auth.0.role == "student" {
        rbac::check_student_in_class(state.db.as_ref(), user_id, hw.class_id)?;
    } else if auth.0.role == "teacher" && hw.teacher_id != user_id {
        return Err(AppError::Forbidden("您不是该作业的布置老师".to_string()));
    }

    let content_b64 = drafftink_core::utils::base64_encode(&hw.content);

    Ok(Json(HomeworkDetail {
        id: hw.id,
        title: hw.title,
        description: hw.description,
        teacher_id: hw.teacher_id,
        class_id: hw.class_id,
        content: content_b64,
        created_at: hw.created_at,
        deadline: hw.deadline,
        status: format!("{:?}", hw.status).to_lowercase(),
    }))
}

// ════════════════════════════════════════════════════════════════════════════
//  提交作业
// ════════════════════════════════════════════════════════════════════════════

/// 提交作业响应
#[derive(Debug, Serialize)]
pub struct SubmitResponse {
    pub submission_id: Uuid,
    pub status: String,
}

/// POST /api/homework/submit
///
/// 学生上传 drftx 文件，验证 Ed25519 签名后存储。
pub async fn submit(
    auth: AuthUser,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<SubmitResponse>, AppError> {
    require_role(&auth, &[Role::Student, Role::Teacher])?;

    let student_id = Uuid::parse_str(&auth.0.sub)
        .map_err(|_| AppError::Internal("JWT sub 不是有效的 UUID".to_string()))?;

    let mut homework_id: Option<Uuid> = None;
    let mut file_data: Option<Vec<u8>> = None;

    // 解析 multipart 表单
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart 解析失败: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "homework_id" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("读取 homework_id 失败: {e}")))?;
                homework_id = Some(
                    Uuid::parse_str(&text)
                        .map_err(|e| AppError::BadRequest(format!("homework_id 格式错误: {e}")))?,
                );
            }
            "file" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("读取文件失败: {e}")))?;
                file_data = Some(bytes.to_vec());
            }
            _ => {
                // 忽略未知字段
            }
        }
    }

    let homework_id =
        homework_id.ok_or_else(|| AppError::BadRequest("缺少 homework_id 字段".to_string()))?;
    let file_data = file_data.ok_or_else(|| AppError::BadRequest("缺少 file 字段".to_string()))?;

    // 验证学生有权提交该作业
    rbac::check_student_owns_homework(state.db.as_ref(), student_id, homework_id)?;

    // 解析并验证 drftx 文件（包含 Ed25519 签名验证）
    let drftx = DrftxFile::from_bytes(&file_data, true)
        .map_err(|e| AppError::BadRequest(format!("drftx 文件验证失败: {e}")))?;

    // 验证快照中的作业 ID 和学生 ID 匹配
    if drftx.snapshot.homework_id != homework_id {
        return Err(AppError::BadRequest(
            "drftx 文件中的作业 ID 与请求不匹配".to_string(),
        ));
    }
    if drftx.snapshot.student_id != student_id {
        return Err(AppError::BadRequest(
            "drftx 文件中的学生 ID 与当前用户不匹配".to_string(),
        ));
    }

    // 存储文件
    let submission_id = Uuid::new_v4();
    let storage_path = format!("submissions/{homework_id}/{student_id}/{submission_id}.drftx");
    state.storage.save(&storage_path, file_data)?;

    // 计算内容哈希（十六进制）
    let content_hash = drafftink_core::utils::sha256_hex(&drftx.snapshot.answer_data);

    // 创建提交记录
    let submission = HomeworkSubmission {
        id: submission_id,
        homework_id,
        student_id,
        drftx_path: storage_path,
        submitted_at: Utc::now(),
        status: SubmissionStatus::Submitted,
        content_hash,
        score: None,
        graded_by: None,
        graded_at: None,
    };

    state.db.save_submission(&submission)?;

    Ok(Json(SubmitResponse {
        submission_id,
        status: "submitted".to_string(),
    }))
}

// ════════════════════════════════════════════════════════════════════════════
//  批改作业
// ════════════════════════════════════════════════════════════════════════════

/// 批改请求
#[derive(Debug, Deserialize)]
pub struct GradeRequest {
    pub submission_id: Uuid,
    pub score: f32,
    pub comments: String,
}

/// 批改响应
#[derive(Debug, Serialize)]
pub struct GradeResponse {
    pub submission_id: Uuid,
    pub status: String,
    pub score: f32,
}

/// POST /api/homework/grade
///
/// 老师批改提交，将 TeacherAnnotation 写入 drftx 文件。
pub async fn grade(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<GradeRequest>,
) -> Result<Json<GradeResponse>, AppError> {
    require_role(&auth, &[Role::Teacher, Role::Admin])?;

    let teacher_id = Uuid::parse_str(&auth.0.sub)
        .map_err(|_| AppError::Internal("JWT sub 不是有效的 UUID".to_string()))?;

    // 获取提交记录
    let mut submission = state
        .db
        .get_submission(req.submission_id)?
        .ok_or_else(|| AppError::NotFound(format!("提交记录不存在: {}", req.submission_id)))?;

    // 验证老师拥有该作业
    let homework = state
        .db
        .get_homework(submission.homework_id)?
        .ok_or_else(|| AppError::NotFound("作业不存在".to_string()))?;

    rbac::check_teacher_owns_class(state.db.as_ref(), teacher_id, homework.class_id)?;

    // 加载 drftx 文件
    let file_data = state.storage.load(&submission.drftx_path)?;
    let drftx = DrftxFile::from_bytes(&file_data, false)
        .map_err(|e| AppError::Internal(format!("drftx 文件解析失败: {e}")))?;

    // 创建教师批注
    let annotation = TeacherAnnotation {
        teacher_id,
        score: Some(req.score),
        comments: req.comments.clone(),
        annotation_data: Vec::new(),
        annotated_at: Utc::now(),
        teacher_signature: None,
    };

    // 将批注写入 drftx 文件
    let updated_drftx = drftx.with_annotation(annotation);
    let updated_bytes = updated_drftx
        .to_bytes()
        .map_err(|e| AppError::Internal(format!("drftx 文件序列化失败: {e}")))?;

    // 存储更新后的文件
    state.storage.save(&submission.drftx_path, updated_bytes)?;

    // 更新提交记录
    submission.score = Some(req.score);
    submission.graded_by = Some(teacher_id);
    submission.graded_at = Some(Utc::now());
    submission.status = SubmissionStatus::Graded;
    state.db.save_submission(&submission)?;

    Ok(Json(GradeResponse {
        submission_id: req.submission_id,
        status: "graded".to_string(),
        score: req.score,
    }))
}
