//! # RBAC 权限检查
//!
//! 基于角色的访问控制辅助函数，验证用户对资源的访问权限。

use anyhow::Result;
use uuid::Uuid;

use crate::db::Database;
use crate::error::AppError;

/// 验证老师拥有指定班级
pub fn check_teacher_owns_class(
    db: &dyn Database,
    teacher_id: Uuid,
    class_id: Uuid,
) -> Result<(), AppError> {
    let class = db
        .get_class(class_id)?
        .ok_or_else(|| AppError::NotFound(format!("班级不存在: {class_id}")))?;

    match class.teacher_id {
        Some(tid) if tid == teacher_id => Ok(()),
        _ => Err(AppError::Forbidden(
            "您不是该班级的任课老师".to_string(),
        )),
    }
}

/// 验证学生在指定班级中
pub fn check_student_in_class(
    db: &dyn Database,
    student_id: Uuid,
    class_id: Uuid,
) -> Result<(), AppError> {
    let student = db
        .get_user(student_id)?
        .ok_or_else(|| AppError::NotFound(format!("学生不存在: {student_id}")))?;

    match student.class_id {
        Some(cid) if cid == class_id => Ok(()),
        _ => Err(AppError::Forbidden(
            "该学生不属于此班级".to_string(),
        )),
    }
}

/// 验证学生拥有指定作业的提交权限（学生所在班级 = 作业所属班级）
pub fn check_student_owns_homework(
    db: &dyn Database,
    student_id: Uuid,
    hw_id: Uuid,
) -> Result<(), AppError> {
    let homework = db
        .get_homework(hw_id)?
        .ok_or_else(|| AppError::NotFound(format!("作业不存在: {hw_id}")))?;

    check_student_in_class(db, student_id, homework.class_id)
}

use drafftink_core::Role;

/// 多租户数据隔离校验。
///
/// - 管理员（Admin / 校长）可跨校访问，不受租户限制；
/// - 其余角色必须 `claims_tenant == resource_tenant`，否则视为越权访问。
///
/// 注：访问令牌由服务端签名，客户端无法篡改 `tenant_id`；此函数用于
/// 在资源层面二次兜底，确保即使逻辑层误用令牌也不会泄露跨校数据。
pub fn ensure_tenant_access(
    claims_tenant: &str,
    resource_tenant: &str,
    role: Role,
) -> Result<(), AppError> {
    if role.is_admin() {
        return Ok(());
    }
    if claims_tenant == resource_tenant {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "无权访问其他学校（租户）的数据".to_string(),
        ))
    }
}

/// 判断角色是否在允许列表中（RBAC 校验辅助）。
pub fn role_matches(role: Role, allowed: &[Role]) -> bool {
    allowed.contains(&role)
}

/// 校验老师拥有指定班级，且该班级属于老师的租户（多租户隔离）。
pub fn check_teacher_owns_class_in_tenant(
    db: &dyn Database,
    teacher_id: Uuid,
    class_id: Uuid,
    teacher_tenant: Uuid,
) -> Result<(), AppError> {
    check_teacher_owns_class(db, teacher_id, class_id)?;
    let class = db
        .get_class(class_id)?
        .ok_or_else(|| AppError::NotFound(format!("班级不存在: {class_id}")))?;
    if class.school_id != teacher_tenant {
        return Err(AppError::Forbidden(
            "该班级不属于您所在的学校（租户）".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_tenant_access() {
        let admin = Role::Admin;
        let teacher = Role::Teacher;
        // 管理员可跨租户
        assert!(ensure_tenant_access("A", "B", admin).is_ok());
        // 同租户放行
        assert!(ensure_tenant_access("A", "A", teacher).is_ok());
        // 跨租户拒绝
        assert!(ensure_tenant_access("A", "B", teacher).is_err());
    }

    #[test]
    fn test_role_matches() {
        assert!(role_matches(Role::Teacher, &[Role::Teacher, Role::Admin]));
        assert!(!role_matches(Role::Student, &[Role::Teacher, Role::Admin]));
    }
}

