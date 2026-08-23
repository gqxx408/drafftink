//! ZXXS — 学生管理数据子集
//!
//! - ZXXS01 学生基本（取用 [`StudentBasic`] / JCXS01）
//! - ZXXS02 学籍（取用 [`StudentStatus`] / JCXS02）
//! - ZXXS0206 在校考试数据子类（对接作业批改结果）
//! - ZXXS03 毕结业
//! - ZXXS04 综合素质评价

use crate::emgi::types::{DataType, EmgiRecordable, FieldDef, Obligation};
use crate::emgi::{StudentBasic, StudentStatus};

// ── ZXXS01 学生基本（取用 JCXS01） ─────────────────────────────
pub const ZXXS010101: FieldDef = FieldDef { id: "ZXXS010101", name: "学号", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: None, note: "本子集特有的学籍号" };

/// 学生基本（ZXXS01）——取用 JCXS01，补充学号。
pub struct StudentProfile {
    pub student: StudentBasic,
    pub student_no: String,
}

impl StudentProfile {
    /// 由 JY/T 1002 学生基本结构体「取用」构造，附加学号。
    pub fn from_student(student: StudentBasic, student_no: &str) -> Self {
        Self {
            student,
            student_no: student_no.to_string(),
        }
    }
}

impl EmgiRecordable for StudentProfile {
    const SUBSET: &'static str = "ZXXS";
    const CLASS_ID: &'static str = "ZXXS0101";
    const CLASS_NAME: &'static str = "学生基本";
    fn references(&self) -> &'static [&'static str] {
        &["JCXS0101"]
    }
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        let mut v = self.student.fields();
        v.push((&ZXXS010101, Some(self.student_no.clone())));
        v
    }
}

// ── ZXXS02 学籍（取用 JCXS02） ─────────────────────────────────
pub const ZXXS020101: FieldDef = FieldDef { id: "ZXXS020101", name: "学籍标识", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: None, note: "" };

/// 学籍（ZXXS02）——取用 JCXS02。
pub struct StudentStatusProfile {
    pub status_id: String,
    pub status: StudentStatus,
}

impl EmgiRecordable for StudentStatusProfile {
    const SUBSET: &'static str = "ZXXS";
    const CLASS_ID: &'static str = "ZXXS0201";
    const CLASS_NAME: &'static str = "学籍";
    fn references(&self) -> &'static [&'static str] {
        &["JCXS0201"]
    }
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        let mut v = self.status.fields();
        v.push((&ZXXS020101, Some(self.status_id.clone())));
        v
    }
}

// ── ZXXS0206 在校考试数据子类（对接作业批改） ───────────────────
pub const ZXXS020601: FieldDef = FieldDef { id: "ZXXS020601", name: "考试标识", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: None, note: "作业/考试唯一标识" };
pub const ZXXS020602: FieldDef = FieldDef { id: "ZXXS020602", name: "学生标识码", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: Some("JCXS0101"), note: "取用 JCXS0101 学生标识码" };
pub const ZXXS020603: FieldDef = FieldDef { id: "ZXXS020603", name: "学校标识码", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: Some("JCXX0101"), note: "取用 JCXX0101 学校标识码" };
pub const ZXXS020604: FieldDef = FieldDef { id: "ZXXS020604", name: "课程号", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: Some("ZXJX0101"), note: "取用 ZXJX0101 课程号" };
pub const ZXXS020605: FieldDef = FieldDef { id: "ZXXS020605", name: "学年", data_type: DataType::C, length: 9, obligation: Obligation::M, code_ref: None, source: None, note: "如 2023-2024" };
pub const ZXXS020606: FieldDef = FieldDef { id: "ZXXS020606", name: "学期", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_TERM"), source: None, note: "1 上 / 2 下" };
pub const ZXXS020607: FieldDef = FieldDef { id: "ZXXS020607", name: "考试方式", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_EXAM_METHOD"), source: None, note: "" };
pub const ZXXS020608: FieldDef = FieldDef { id: "ZXXS020608", name: "考试日期", data_type: DataType::D, length: 8, obligation: Obligation::M, code_ref: None, source: None, note: "YYYYMMDD" };
pub const ZXXS020609: FieldDef = FieldDef { id: "ZXXS020609", name: "分数类成绩", data_type: DataType::N, length: 6, obligation: Obligation::M, code_ref: None, source: None, note: "最多两位小数" };
pub const ZXXS020610: FieldDef = FieldDef { id: "ZXXS020610", name: "成绩类型", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_SCORE_TYPE"), source: None, note: "" };

/// 在校考试数据子类（ZXXS0206）——作业批改结果映射目标。
pub struct ExamRecord {
    pub exam_id: String,
    pub student_id: String,
    pub school_id: String,
    pub course_id: String,
    pub academic_year: String,
    pub term: String,
    pub exam_method: String,
    pub exam_date: String,
    pub score: String,
    pub score_type: String,
}

impl EmgiRecordable for ExamRecord {
    const SUBSET: &'static str = "ZXXS";
    const CLASS_ID: &'static str = "ZXXS0206";
    const CLASS_NAME: &'static str = "在校考试";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXXS020601, Some(self.exam_id.clone())),
            (&ZXXS020602, Some(self.student_id.clone())),
            (&ZXXS020603, Some(self.school_id.clone())),
            (&ZXXS020604, Some(self.course_id.clone())),
            (&ZXXS020605, Some(self.academic_year.clone())),
            (&ZXXS020606, Some(self.term.clone())),
            (&ZXXS020607, Some(self.exam_method.clone())),
            (&ZXXS020608, Some(self.exam_date.clone())),
            (&ZXXS020609, Some(self.score.clone())),
            (&ZXXS020610, Some(self.score_type.clone())),
        ]
    }
}

// ── ZXXS03 毕结业 ──────────────────────────────────────────────
pub const ZXXS030101: FieldDef = FieldDef { id: "ZXXS030101", name: "毕结业标识", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: None, note: "" };
pub const ZXXS030102: FieldDef = FieldDef { id: "ZXXS030102", name: "学生标识码", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: Some("JCXS0101"), note: "取用 JCXS0101 学生标识码" };
pub const ZXXS030103: FieldDef = FieldDef { id: "ZXXS030103", name: "学校标识码", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: Some("JCXX0101"), note: "取用 JCXX0101 学校标识码" };
pub const ZXXS030104: FieldDef = FieldDef { id: "ZXXS030104", name: "毕结业代码", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_GRAD_CODE"), source: None, note: "" };
pub const ZXXS030105: FieldDef = FieldDef { id: "ZXXS030105", name: "毕结业日期", data_type: DataType::D, length: 8, obligation: Obligation::M, code_ref: None, source: None, note: "YYYYMMDD" };
pub const ZXXS030106: FieldDef = FieldDef { id: "ZXXS030106", name: "证书编号", data_type: DataType::C, length: 30, obligation: Obligation::O, code_ref: None, source: None, note: "" };

/// 毕结业（ZXXS03）。
pub struct Graduation {
    pub grad_id: String,
    pub student_id: String,
    pub school_id: String,
    pub grad_code: String,
    pub grad_date: String,
    pub cert_no: String,
}

impl EmgiRecordable for Graduation {
    const SUBSET: &'static str = "ZXXS";
    const CLASS_ID: &'static str = "ZXXS0301";
    const CLASS_NAME: &'static str = "毕结业";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXXS030101, Some(self.grad_id.clone())),
            (&ZXXS030102, Some(self.student_id.clone())),
            (&ZXXS030103, Some(self.school_id.clone())),
            (&ZXXS030104, Some(self.grad_code.clone())),
            (&ZXXS030105, Some(self.grad_date.clone())),
            (&ZXXS030106, Some(self.cert_no.clone())),
        ]
    }
}

// ── ZXXS04 综合素质评价 ────────────────────────────────────────
pub const ZXXS040101: FieldDef = FieldDef { id: "ZXXS040101", name: "评价标识", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: None, note: "" };
pub const ZXXS040102: FieldDef = FieldDef { id: "ZXXS040102", name: "学生标识码", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: Some("JCXS0101"), note: "取用 JCXS0101 学生标识码" };
pub const ZXXS040103: FieldDef = FieldDef { id: "ZXXS040103", name: "学校标识码", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: Some("JCXX0101"), note: "取用 JCXX0101 学校标识码" };
pub const ZXXS040104: FieldDef = FieldDef { id: "ZXXS040104", name: "评价类型", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_EVAL_TYPE"), source: None, note: "" };
pub const ZXXS040105: FieldDef = FieldDef { id: "ZXXS040105", name: "学年", data_type: DataType::C, length: 9, obligation: Obligation::M, code_ref: None, source: None, note: "如 2023-2024" };
pub const ZXXS040106: FieldDef = FieldDef { id: "ZXXS040106", name: "评价等级", data_type: DataType::C, length: 4, obligation: Obligation::O, code_ref: None, source: None, note: "如 优秀/良好" };
pub const ZXXS040107: FieldDef = FieldDef { id: "ZXXS040107", name: "评价内容", data_type: DataType::C, length: 500, obligation: Obligation::O, code_ref: None, source: None, note: "" };

/// 综合素质评价（ZXXS04）。
pub struct Evaluation {
    pub eval_id: String,
    pub student_id: String,
    pub school_id: String,
    pub eval_type: String,
    pub academic_year: String,
    pub eval_level: String,
    pub content: String,
}

impl EmgiRecordable for Evaluation {
    const SUBSET: &'static str = "ZXXS";
    const CLASS_ID: &'static str = "ZXXS0401";
    const CLASS_NAME: &'static str = "综合素质评价";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXXS040101, Some(self.eval_id.clone())),
            (&ZXXS040102, Some(self.student_id.clone())),
            (&ZXXS040103, Some(self.school_id.clone())),
            (&ZXXS040104, Some(self.eval_type.clone())),
            (&ZXXS040105, Some(self.academic_year.clone())),
            (&ZXXS040106, Some(self.eval_level.clone())),
            (&ZXXS040107, Some(self.content.clone())),
        ]
    }
}
