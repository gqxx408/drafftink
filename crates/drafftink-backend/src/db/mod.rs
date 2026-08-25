//! # 数据库抽象层
//!
//! `Database` trait 定义数据访问接口，`SledDb` 基于 sled 嵌入式数据库实现。
//! 使用 bincode 进行序列化，sled 键采用前缀模式（如 `user:{uuid}`）。

pub mod models;

use anyhow::{anyhow, Result};
use drafftink_core::{AuditLog, Class, Homework, HomeworkSubmission, User};
use sled::Db;
use uuid::Uuid;

use models::{
    UserCredentials, PREFIX_AUDIT, PREFIX_CLASS, PREFIX_HW, PREFIX_HW_CLASS, PREFIX_HW_TEACHER,
    PREFIX_PWD, PREFIX_SUB, PREFIX_SUB_HW, PREFIX_USER, PREFIX_USERNAME,
};

/// 数据库访问 trait
///
/// 方法为同步调用（sled 本身是同步的），在 async handler 中直接使用。
pub trait Database: Send + Sync {
    /// 保存用户
    fn save_user(&self, user: &User) -> Result<()>;
    /// 根据 ID 获取用户
    fn get_user(&self, id: Uuid) -> Result<Option<User>>;
    /// 根据用户名获取用户
    fn get_user_by_username(&self, username: &str) -> Result<Option<User>>;

    /// 保存班级
    fn save_class(&self, class: &Class) -> Result<()>;
    /// 根据 ID 获取班级
    fn get_class(&self, id: Uuid) -> Result<Option<Class>>;

    /// 保存作业
    fn save_homework(&self, hw: &Homework) -> Result<()>;
    /// 根据 ID 获取作业
    fn get_homework(&self, id: Uuid) -> Result<Option<Homework>>;
    /// 列出班级的所有作业
    fn list_homework_by_class(&self, class_id: Uuid) -> Result<Vec<Homework>>;
    /// 列出老师的所有作业
    fn list_homework_by_teacher(&self, teacher_id: Uuid) -> Result<Vec<Homework>>;

    /// 保存提交记录
    fn save_submission(&self, sub: &HomeworkSubmission) -> Result<()>;
    /// 根据 ID 获取提交记录
    fn get_submission(&self, id: Uuid) -> Result<Option<HomeworkSubmission>>;
    /// 根据作业 ID 和学生 ID 获取提交记录
    #[allow(dead_code)]
    fn get_submission_by_homework_and_student(
        &self,
        hw_id: Uuid,
        stu_id: Uuid,
    ) -> Result<Option<HomeworkSubmission>>;
    /// 列出某作业的所有提交记录
    #[allow(dead_code)]
    fn list_submissions_by_homework(&self, hw_id: Uuid) -> Result<Vec<HomeworkSubmission>>;

    /// 保存审计日志
    fn save_audit_log(&self, log: &AuditLog) -> Result<()>;

    // ---------- 课件资源索引（JY/T 1004 分类检索） ----------

    /// 保存课件资源元数据（JSON 字符串），`id` 为资源唯一标识。
    fn save_resource_meta(&self, id: &str, json: &str) -> Result<()>;

    /// 读取课件资源元数据（JSON 字符串），不存在返回 `None`。
    fn get_resource_meta(&self, id: &str) -> Result<Option<String>>;

    /// 扫描全部课件资源元数据，返回 `(资源ID, JSON)` 列表。
    fn scan_resource_meta(&self) -> Result<Vec<(String, String)>>;
}

/// 基于 sled 的数据库实现
pub struct SledDb {
    db: Db,
}

impl SledDb {
    /// 打开或创建 sled 数据库
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }

    /// 从已打开的 sled::Db 创建
    #[allow(dead_code)]
    pub fn from_db(db: Db) -> Self {
        Self { db }
    }

    /// 简单的 put + bincode 序列化
    fn put_bincode<T: serde::Serialize>(&self, key: &[u8], value: &T) -> Result<()> {
        let bytes = bincode::serialize(value)?;
        self.db.insert(key, bytes)?;
        Ok(())
    }

    /// 简单的 get + bincode 反序列化
    fn get_bincode<T: serde::de::DeserializeOwned>(&self, key: &[u8]) -> Result<Option<T>> {
        match self.db.get(key)? {
            None => Ok(None),
            Some(bytes) => {
                let value = bincode::deserialize(&bytes)?;
                Ok(Some(value))
            }
        }
    }

    /// 构造用户键 `user:{uuid}`
    fn user_key(id: Uuid) -> Vec<u8> {
        format!("{PREFIX_USER}{id}").into_bytes()
    }

    /// 构造密码键 `pwd:{uuid}`
    fn pwd_key(id: Uuid) -> Vec<u8> {
        format!("{PREFIX_PWD}{id}").into_bytes()
    }

    /// 构造用户名索引键 `username:{username}`
    fn username_key(username: &str) -> Vec<u8> {
        format!("{PREFIX_USERNAME}{username}").into_bytes()
    }

    /// 构造班级键 `class:{uuid}`
    fn class_key(id: Uuid) -> Vec<u8> {
        format!("{PREFIX_CLASS}{id}").into_bytes()
    }

    /// 构造作业键 `hw:{uuid}`
    fn hw_key(id: Uuid) -> Vec<u8> {
        format!("{PREFIX_HW}{id}").into_bytes()
    }

    /// 构造作业-班级索引键 `hw_class:{class_id}:{hw_id}`
    fn hw_class_key(class_id: Uuid, hw_id: Uuid) -> Vec<u8> {
        format!("{PREFIX_HW_CLASS}{class_id}:{hw_id}").into_bytes()
    }

    /// 构造作业-老师索引键前缀 `hw_teacher:{teacher_id}:`
    fn hw_teacher_prefix(teacher_id: Uuid) -> Vec<u8> {
        format!("{PREFIX_HW_TEACHER}{teacher_id}:").into_bytes()
    }

    /// 构造作业-班级索引键前缀 `hw_class:{class_id}:`
    fn hw_class_prefix(class_id: Uuid) -> Vec<u8> {
        format!("{PREFIX_HW_CLASS}{class_id}:").into_bytes()
    }

    /// 构造作业-老师索引键 `hw_teacher:{teacher_id}:{hw_id}`
    fn hw_teacher_key(teacher_id: Uuid, hw_id: Uuid) -> Vec<u8> {
        format!("{PREFIX_HW_TEACHER}{teacher_id}:{hw_id}").into_bytes()
    }

    /// 构造提交记录键 `sub:{uuid}`
    fn sub_key(id: Uuid) -> Vec<u8> {
        format!("{PREFIX_SUB}{id}").into_bytes()
    }

    /// 构造提交-作业索引键前缀 `sub_hw:{hw_id}:`
    fn sub_hw_prefix(hw_id: Uuid) -> Vec<u8> {
        format!("{PREFIX_SUB_HW}{hw_id}:").into_bytes()
    }

    /// 构造提交-作业索引键 `sub_hw:{hw_id}:{sub_id}`
    fn sub_hw_key(hw_id: Uuid, sub_id: Uuid) -> Vec<u8> {
        format!("{PREFIX_SUB_HW}{hw_id}:{sub_id}").into_bytes()
    }

    /// 构造审计日志键 `audit:{uuid}`
    fn audit_key(id: Uuid) -> Vec<u8> {
        format!("{PREFIX_AUDIT}{id}").into_bytes()
    }

    /// 构造课件资源索引键 `resource:{id}`
    fn resource_key(id: &str) -> Vec<u8> {
        format!("resource:{id}").into_bytes()
    }

    /// 保存课件资源元数据（JSON 字符串），用于资源管理平台检索索引。
    pub fn save_resource_meta(&self, id: &str, json: &str) -> Result<()> {
        self.db.insert(Self::resource_key(id), json.as_bytes())?;
        self.db.flush()?;
        Ok(())
    }

    /// 读取课件资源元数据（JSON 字符串）；不存在返回 `None`。
    pub fn get_resource_meta(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .db
            .get(Self::resource_key(id))?
            .map(|b| String::from_utf8_lossy(&b).into_owned()))
    }

    /// 扫描全部课件资源元数据，返回 `(资源ID, JSON)` 列表。
    pub fn scan_resource_meta(&self) -> Result<Vec<(String, String)>> {
        const PREFIX: &[u8] = b"resource:";
        let mut out = Vec::new();
        for item in self.db.scan_prefix(PREFIX) {
            let (k, v) = item?;
            let id = String::from_utf8_lossy(&k)
                .strip_prefix("resource:")
                .unwrap_or("")
                .to_string();
            let json = String::from_utf8_lossy(&v).into_owned();
            out.push((id, json));
        }
        Ok(out)
    }
}

impl Database for SledDb {
    fn save_user(&self, user: &User) -> Result<()> {
        // 存储用户数据（password_hash 被 serde skip，不会序列化）
        self.put_bincode(&Self::user_key(user.id), user)?;

        // 单独存储密码哈希
        let creds = UserCredentials {
            password_hash: user.password_hash.clone(),
        };
        self.put_bincode(&Self::pwd_key(user.id), &creds)?;

        // 维护用户名 -> uuid 索引
        self.db
            .insert(Self::username_key(&user.username), user.id.as_bytes())?;

        self.db.flush()?;
        Ok(())
    }

    fn get_user(&self, id: Uuid) -> Result<Option<User>> {
        match self.get_bincode::<User>(&Self::user_key(id))? {
            None => Ok(None),
            Some(mut user) => {
                // 加载密码哈希
                if let Some(creds) = self.get_bincode::<UserCredentials>(&Self::pwd_key(id))? {
                    user.password_hash = creds.password_hash;
                }
                Ok(Some(user))
            }
        }
    }

    fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        match self.db.get(Self::username_key(username))? {
            None => Ok(None),
            Some(id_bytes) => {
                let id =
                    Uuid::from_slice(&id_bytes).map_err(|e| anyhow!("无效的用户 ID 字节: {e}"))?;
                self.get_user(id)
            }
        }
    }

    fn save_class(&self, class: &Class) -> Result<()> {
        self.put_bincode(&Self::class_key(class.id), class)?;
        self.db.flush()?;
        Ok(())
    }

    fn get_class(&self, id: Uuid) -> Result<Option<Class>> {
        self.get_bincode(&Self::class_key(id))
    }

    fn save_homework(&self, hw: &Homework) -> Result<()> {
        self.put_bincode(&Self::hw_key(hw.id), hw)?;
        // 维护班级索引
        self.db
            .insert(Self::hw_class_key(hw.class_id, hw.id), &[])?;
        // 维护老师索引
        self.db
            .insert(Self::hw_teacher_key(hw.teacher_id, hw.id), &[])?;
        self.db.flush()?;
        Ok(())
    }

    fn get_homework(&self, id: Uuid) -> Result<Option<Homework>> {
        self.get_bincode(&Self::hw_key(id))
    }

    fn list_homework_by_class(&self, class_id: Uuid) -> Result<Vec<Homework>> {
        let prefix = Self::hw_class_prefix(class_id);
        let mut result = Vec::new();
        for item in self.db.scan_prefix(prefix) {
            let (key, _) = item?;
            // 键格式: hw_class:{class_id}:{hw_id}
            let key_str = String::from_utf8_lossy(&key);
            if let Some(hw_id_str) = key_str.rsplit(':').next() {
                if let Ok(hw_id) = Uuid::parse_str(hw_id_str) {
                    if let Some(hw) = self.get_homework(hw_id)? {
                        result.push(hw);
                    }
                }
            }
        }
        Ok(result)
    }

    fn list_homework_by_teacher(&self, teacher_id: Uuid) -> Result<Vec<Homework>> {
        let prefix = Self::hw_teacher_prefix(teacher_id);
        let mut result = Vec::new();
        for item in self.db.scan_prefix(prefix) {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some(hw_id_str) = key_str.rsplit(':').next() {
                if let Ok(hw_id) = Uuid::parse_str(hw_id_str) {
                    if let Some(hw) = self.get_homework(hw_id)? {
                        result.push(hw);
                    }
                }
            }
        }
        Ok(result)
    }

    fn save_submission(&self, sub: &HomeworkSubmission) -> Result<()> {
        self.put_bincode(&Self::sub_key(sub.id), sub)?;
        // 维护作业索引
        self.db
            .insert(Self::sub_hw_key(sub.homework_id, sub.id), &[])?;
        self.db.flush()?;
        Ok(())
    }

    fn get_submission(&self, id: Uuid) -> Result<Option<HomeworkSubmission>> {
        self.get_bincode(&Self::sub_key(id))
    }

    fn get_submission_by_homework_and_student(
        &self,
        hw_id: Uuid,
        stu_id: Uuid,
    ) -> Result<Option<HomeworkSubmission>> {
        let prefix = Self::sub_hw_prefix(hw_id);
        for item in self.db.scan_prefix(prefix) {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some(sub_id_str) = key_str.rsplit(':').next() {
                if let Ok(sub_id) = Uuid::parse_str(sub_id_str) {
                    if let Some(sub) = self.get_submission(sub_id)? {
                        if sub.student_id == stu_id {
                            return Ok(Some(sub));
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    fn list_submissions_by_homework(&self, hw_id: Uuid) -> Result<Vec<HomeworkSubmission>> {
        let prefix = Self::sub_hw_prefix(hw_id);
        let mut result = Vec::new();
        for item in self.db.scan_prefix(prefix) {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some(sub_id_str) = key_str.rsplit(':').next() {
                if let Ok(sub_id) = Uuid::parse_str(sub_id_str) {
                    if let Some(sub) = self.get_submission(sub_id)? {
                        result.push(sub);
                    }
                }
            }
        }
        Ok(result)
    }

    fn save_audit_log(&self, log: &AuditLog) -> Result<()> {
        self.put_bincode(&Self::audit_key(log.id), log)?;
        self.db.flush()?;
        Ok(())
    }

    fn save_resource_meta(&self, id: &str, json: &str) -> Result<()> {
        SledDb::save_resource_meta(self, id, json)
    }

    fn get_resource_meta(&self, id: &str) -> Result<Option<String>> {
        SledDb::get_resource_meta(self, id)
    }

    fn scan_resource_meta(&self) -> Result<Vec<(String, String)>> {
        SledDb::scan_resource_meta(self)
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  单元测试
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use drafftink_core::{
        AuditAction, Class, Homework, HomeworkStatus, HomeworkSubmission, Role, SubmissionStatus,
        User,
    };

    fn temp_db() -> SledDb {
        let dir = std::env::temp_dir().join(format!(
            "drafftink_test_db_{}_{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        SledDb::open(&dir).expect("打开测试数据库失败")
    }

    #[allow(dead_code)]
    fn cleanup(dir: &std::path::Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_save_and_get_user() {
        let db = temp_db();
        let user = User {
            id: Uuid::new_v4(),
            username: "teacher01".to_string(),
            display_name: "王老师".to_string(),
            role: Role::Teacher,
            class_id: None,
            tenant_id: Uuid::nil(),
            password_hash: "mypassword".to_string(),
            created_at: Utc::now(),
            active: true,
        };

        db.save_user(&user).unwrap();
        let loaded = db.get_user(user.id).unwrap().expect("用户应存在");
        assert_eq!(loaded.username, "teacher01");
        assert_eq!(loaded.password_hash, "mypassword");
        assert_eq!(loaded.role, Role::Teacher);
    }

    #[test]
    fn test_get_user_by_username() {
        let db = temp_db();
        let user = User {
            id: Uuid::new_v4(),
            username: "student01".to_string(),
            display_name: "李同学".to_string(),
            role: Role::Student,
            class_id: Some(Uuid::new_v4()),
            tenant_id: Uuid::nil(),
            password_hash: "stu_pass".to_string(),
            created_at: Utc::now(),
            active: true,
        };

        db.save_user(&user).unwrap();
        let loaded = db
            .get_user_by_username("student01")
            .unwrap()
            .expect("用户应存在");
        assert_eq!(loaded.id, user.id);
        assert_eq!(loaded.password_hash, "stu_pass");
    }

    #[test]
    fn test_get_nonexistent_user() {
        let db = temp_db();
        let result = db.get_user(Uuid::new_v4()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_save_and_get_class() {
        let db = temp_db();
        let class = Class {
            id: Uuid::new_v4(),
            name: "三年二班".to_string(),
            grade: "三年级".to_string(),
            teacher_id: Some(Uuid::new_v4()),
            school_id: Uuid::new_v4(),
            created_at: Utc::now(),
        };

        db.save_class(&class).unwrap();
        let loaded = db.get_class(class.id).unwrap().expect("班级应存在");
        assert_eq!(loaded.name, "三年二班");
    }

    #[test]
    fn test_save_and_list_homework() {
        let db = temp_db();
        let teacher_id = Uuid::new_v4();
        let class_id = Uuid::new_v4();

        for i in 0..3 {
            let hw = Homework {
                id: Uuid::new_v4(),
                title: format!("作业{i}"),
                description: String::new(),
                teacher_id,
                class_id,
                content: Vec::new(),
                created_at: Utc::now(),
                deadline: Utc::now(),
                status: HomeworkStatus::Published,
                attachment_ids: Vec::new(),
            };
            db.save_homework(&hw).unwrap();
        }

        let by_class = db.list_homework_by_class(class_id).unwrap();
        assert_eq!(by_class.len(), 3);

        let by_teacher = db.list_homework_by_teacher(teacher_id).unwrap();
        assert_eq!(by_teacher.len(), 3);
    }

    #[test]
    fn test_save_and_get_submission() {
        let db = temp_db();
        let hw_id = Uuid::new_v4();
        let stu_id = Uuid::new_v4();
        let sub = HomeworkSubmission {
            id: Uuid::new_v4(),
            homework_id: hw_id,
            student_id: stu_id,
            drftx_path: "submissions/test.drftx".to_string(),
            submitted_at: Utc::now(),
            status: SubmissionStatus::Submitted,
            content_hash: "abc123".to_string(),
            score: None,
            graded_by: None,
            graded_at: None,
        };

        db.save_submission(&sub).unwrap();

        let loaded = db.get_submission(sub.id).unwrap().expect("提交应存在");
        assert_eq!(loaded.student_id, stu_id);

        let by_hw_stu = db
            .get_submission_by_homework_and_student(hw_id, stu_id)
            .unwrap()
            .expect("应找到提交");
        assert_eq!(by_hw_stu.id, sub.id);

        let list = db.list_submissions_by_homework(hw_id).unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_save_audit_log() {
        let db = temp_db();
        let log = AuditLog {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            action: AuditAction::Login,
            timestamp: Utc::now(),
            ip_address: "192.168.1.1".to_string(),
            device_fp: "fp123".to_string(),
            details: "{}".to_string(),
        };

        db.save_audit_log(&log).unwrap();
        // 审计日志只需确保不报错即可
    }

    #[test]
    fn test_password_hash_persistence() {
        let db = temp_db();
        let user = User {
            id: Uuid::new_v4(),
            username: "admin".to_string(),
            display_name: "管理员".to_string(),
            role: Role::Admin,
            class_id: None,
            tenant_id: Uuid::nil(),
            password_hash: "secret123".to_string(),
            created_at: Utc::now(),
            active: true,
        };

        db.save_user(&user).unwrap();

        // 通过用户名查找，验证密码哈希正确加载
        let loaded = db.get_user_by_username("admin").unwrap().unwrap();
        assert_eq!(loaded.password_hash, "secret123");
    }
}
