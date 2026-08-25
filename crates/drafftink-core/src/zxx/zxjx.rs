//! ZXJX — 教学管理数据子集（优先级最高，对接备课/授课/作业流）
//!
//! - ZXJX01 课程
//! - ZXJX02 教材
//! - ZXJX03 教学计划
//! - ZXJX04 排课

use crate::emgi::types::{DataType, EmgiRecordable, FieldDef, Obligation};

pub const ZXJX010101: FieldDef = FieldDef {
    id: "ZXJX010101",
    name: "课程标识",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX010102: FieldDef = FieldDef {
    id: "ZXJX010102",
    name: "课程号",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX010103: FieldDef = FieldDef {
    id: "ZXJX010103",
    name: "课程名称",
    data_type: DataType::C,
    length: 60,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX010104: FieldDef = FieldDef {
    id: "ZXJX010104",
    name: "课程类型",
    data_type: DataType::C,
    length: 1,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_COURSE_TYPE"),
    source: None,
    note: "",
};
pub const ZXJX010105: FieldDef = FieldDef {
    id: "ZXJX010105",
    name: "教材编码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("ZXJX0202"),
    note: "取用 ZXJX0202 教材编码",
};
pub const ZXJX010106: FieldDef = FieldDef {
    id: "ZXJX010106",
    name: "教材名称",
    data_type: DataType::C,
    length: 120,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX010107: FieldDef = FieldDef {
    id: "ZXJX010107",
    name: "学科",
    data_type: DataType::C,
    length: 2,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_SUBJECT"),
    source: None,
    note: "",
};
pub const ZXJX010108: FieldDef = FieldDef {
    id: "ZXJX010108",
    name: "学分",
    data_type: DataType::N,
    length: 4,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX010109: FieldDef = FieldDef {
    id: "ZXJX010109",
    name: "总学时",
    data_type: DataType::N,
    length: 5,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX010110: FieldDef = FieldDef {
    id: "ZXJX010110",
    name: "学校标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXX0101"),
    note: "取用 JCXX0101 学校标识码",
};

/// 课程（ZXJX01）——drftx 文件头元数据映射目标。
pub struct Course {
    pub course_id: String,
    pub course_code: String,
    pub course_name: String,
    pub course_type: String,
    pub textbook_code: String,
    pub textbook_name: String,
    pub subject: String,
    pub credit: String,
    pub hours: String,
    pub school_id: String,
}

impl EmgiRecordable for Course {
    const SUBSET: &'static str = "ZXJX";
    const CLASS_ID: &'static str = "ZXJX0101";
    const CLASS_NAME: &'static str = "课程";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXJX010101, Some(self.course_id.clone())),
            (&ZXJX010102, Some(self.course_code.clone())),
            (&ZXJX010103, Some(self.course_name.clone())),
            (&ZXJX010104, Some(self.course_type.clone())),
            (&ZXJX010105, Some(self.textbook_code.clone())),
            (&ZXJX010106, Some(self.textbook_name.clone())),
            (&ZXJX010107, Some(self.subject.clone())),
            (&ZXJX010108, Some(self.credit.clone())),
            (&ZXJX010109, Some(self.hours.clone())),
            (&ZXJX010110, Some(self.school_id.clone())),
        ]
    }
}

pub const ZXJX020101: FieldDef = FieldDef {
    id: "ZXJX020101",
    name: "教材标识",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX020102: FieldDef = FieldDef {
    id: "ZXJX020102",
    name: "教材编码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX020103: FieldDef = FieldDef {
    id: "ZXJX020103",
    name: "教材名称",
    data_type: DataType::C,
    length: 120,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX020104: FieldDef = FieldDef {
    id: "ZXJX020104",
    name: "教材类型",
    data_type: DataType::C,
    length: 1,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_TEXTBOOK_TYPE"),
    source: None,
    note: "",
};
pub const ZXJX020105: FieldDef = FieldDef {
    id: "ZXJX020105",
    name: "书号(ISBN)",
    data_type: DataType::C,
    length: 17,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "ISBN-13",
};
pub const ZXJX020106: FieldDef = FieldDef {
    id: "ZXJX020106",
    name: "出版社",
    data_type: DataType::C,
    length: 60,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX020107: FieldDef = FieldDef {
    id: "ZXJX020107",
    name: "主编",
    data_type: DataType::C,
    length: 30,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX020108: FieldDef = FieldDef {
    id: "ZXJX020108",
    name: "版本",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX020109: FieldDef = FieldDef {
    id: "ZXJX020109",
    name: "学科",
    data_type: DataType::C,
    length: 2,
    obligation: Obligation::O,
    code_ref: Some("JYT_1001_SUBJECT"),
    source: None,
    note: "",
};

/// 教材（ZXJX02）。
pub struct Textbook {
    pub textbook_id: String,
    pub textbook_code: String,
    pub textbook_name: String,
    pub textbook_type: String,
    pub isbn: String,
    pub publisher: String,
    pub editor: String,
    pub version: String,
    pub subject: String,
}

impl EmgiRecordable for Textbook {
    const SUBSET: &'static str = "ZXJX";
    const CLASS_ID: &'static str = "ZXJX0201";
    const CLASS_NAME: &'static str = "教材";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXJX020101, Some(self.textbook_id.clone())),
            (&ZXJX020102, Some(self.textbook_code.clone())),
            (&ZXJX020103, Some(self.textbook_name.clone())),
            (&ZXJX020104, Some(self.textbook_type.clone())),
            (&ZXJX020105, Some(self.isbn.clone())),
            (&ZXJX020106, Some(self.publisher.clone())),
            (&ZXJX020107, Some(self.editor.clone())),
            (&ZXJX020108, Some(self.version.clone())),
            (&ZXJX020109, Some(self.subject.clone())),
        ]
    }
}

pub const ZXJX030101: FieldDef = FieldDef {
    id: "ZXJX030101",
    name: "计划标识",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX030102: FieldDef = FieldDef {
    id: "ZXJX030102",
    name: "课程号",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("ZXJX0102"),
    note: "取用 ZXJX0102 课程号",
};
pub const ZXJX030103: FieldDef = FieldDef {
    id: "ZXJX030103",
    name: "教职工标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCJG0101"),
    note: "取用 JCJG0101 教职工标识码",
};
pub const ZXJX030104: FieldDef = FieldDef {
    id: "ZXJX030104",
    name: "学校标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXX0101"),
    note: "取用 JCXX0101 学校标识码",
};
pub const ZXJX030105: FieldDef = FieldDef {
    id: "ZXJX030105",
    name: "计划类型",
    data_type: DataType::C,
    length: 1,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_TEACH_PLAN_TYPE"),
    source: None,
    note: "",
};
pub const ZXJX030106: FieldDef = FieldDef {
    id: "ZXJX030106",
    name: "学年",
    data_type: DataType::C,
    length: 9,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "如 2023-2024",
};
pub const ZXJX030107: FieldDef = FieldDef {
    id: "ZXJX030107",
    name: "学期",
    data_type: DataType::C,
    length: 1,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_TERM"),
    source: None,
    note: "",
};
pub const ZXJX030108: FieldDef = FieldDef {
    id: "ZXJX030108",
    name: "计划内容",
    data_type: DataType::C,
    length: 1000,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX030109: FieldDef = FieldDef {
    id: "ZXJX030109",
    name: "计划状态",
    data_type: DataType::L,
    length: 1,
    obligation: Obligation::O,
    code_ref: Some("JYT_1001_YES_NO"),
    source: None,
    note: "1 已执行 / 0 未执行",
};

/// 教学计划（ZXJX03）——对接备课流程。
pub struct TeachPlan {
    pub plan_id: String,
    pub course_id: String,
    pub teacher_id: String,
    pub school_id: String,
    pub plan_type: String,
    pub academic_year: String,
    pub term: String,
    pub content: String,
    pub executed: String,
}

impl EmgiRecordable for TeachPlan {
    const SUBSET: &'static str = "ZXJX";
    const CLASS_ID: &'static str = "ZXJX0301";
    const CLASS_NAME: &'static str = "教学计划";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXJX030101, Some(self.plan_id.clone())),
            (&ZXJX030102, Some(self.course_id.clone())),
            (&ZXJX030103, Some(self.teacher_id.clone())),
            (&ZXJX030104, Some(self.school_id.clone())),
            (&ZXJX030105, Some(self.plan_type.clone())),
            (&ZXJX030106, Some(self.academic_year.clone())),
            (&ZXJX030107, Some(self.term.clone())),
            (&ZXJX030108, Some(self.content.clone())),
            (&ZXJX030109, Some(self.executed.clone())),
        ]
    }
}

pub const ZXJX040101: FieldDef = FieldDef {
    id: "ZXJX040101",
    name: "排课标识",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX040102: FieldDef = FieldDef {
    id: "ZXJX040102",
    name: "课程号",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("ZXJX0102"),
    note: "取用 ZXJX0102 课程号",
};
pub const ZXJX040103: FieldDef = FieldDef {
    id: "ZXJX040103",
    name: "班级标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXX0201"),
    note: "取用 JCXX0201 班级标识码",
};
pub const ZXJX040104: FieldDef = FieldDef {
    id: "ZXJX040104",
    name: "教职工标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCJG0101"),
    note: "取用 JCJG0101 教职工标识码",
};
pub const ZXJX040105: FieldDef = FieldDef {
    id: "ZXJX040105",
    name: "学校标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXX0101"),
    note: "取用 JCXX0101 学校标识码",
};
pub const ZXJX040106: FieldDef = FieldDef {
    id: "ZXJX040106",
    name: "星期",
    data_type: DataType::C,
    length: 1,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_WEEK"),
    source: None,
    note: "",
};
pub const ZXJX040107: FieldDef = FieldDef {
    id: "ZXJX040107",
    name: "节次",
    data_type: DataType::C,
    length: 1,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_PERIOD"),
    source: None,
    note: "",
};
pub const ZXJX040108: FieldDef = FieldDef {
    id: "ZXJX040108",
    name: "教学楼",
    data_type: DataType::C,
    length: 30,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX040109: FieldDef = FieldDef {
    id: "ZXJX040109",
    name: "教室",
    data_type: DataType::C,
    length: 30,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXJX040110: FieldDef = FieldDef {
    id: "ZXJX040110",
    name: "学期",
    data_type: DataType::C,
    length: 1,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_TERM"),
    source: None,
    note: "",
};

/// 排课（ZXJX04）——对接授课流程。
pub struct Schedule {
    pub sched_id: String,
    pub course_id: String,
    pub class_id: String,
    pub teacher_id: String,
    pub school_id: String,
    pub week: String,
    pub period: String,
    pub building: String,
    pub room: String,
    pub term: String,
}

impl EmgiRecordable for Schedule {
    const SUBSET: &'static str = "ZXJX";
    const CLASS_ID: &'static str = "ZXJX0401";
    const CLASS_NAME: &'static str = "排课";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXJX040101, Some(self.sched_id.clone())),
            (&ZXJX040102, Some(self.course_id.clone())),
            (&ZXJX040103, Some(self.class_id.clone())),
            (&ZXJX040104, Some(self.teacher_id.clone())),
            (&ZXJX040105, Some(self.school_id.clone())),
            (&ZXJX040106, Some(self.week.clone())),
            (&ZXJX040107, Some(self.period.clone())),
            (&ZXJX040108, Some(self.building.clone())),
            (&ZXJX040109, Some(self.room.clone())),
            (&ZXJX040110, Some(self.term.clone())),
        ]
    }
}
