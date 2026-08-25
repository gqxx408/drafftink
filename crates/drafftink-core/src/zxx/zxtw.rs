//! ZXTW — 体育卫生数据子集
//!
//! - ZXTW01 学生体育运动
//! - ZXTW02 医疗保健

use crate::emgi::types::{DataType, EmgiRecordable, FieldDef, Obligation};

pub const ZXTW010101: FieldDef = FieldDef {
    id: "ZXTW010101",
    name: "运动标识",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXTW010102: FieldDef = FieldDef {
    id: "ZXTW010102",
    name: "学生标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXS0101"),
    note: "取用 JCXS0101 学生标识码",
};
pub const ZXTW010103: FieldDef = FieldDef {
    id: "ZXTW010103",
    name: "学校标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXX0101"),
    note: "取用 JCXX0101 学校标识码",
};
pub const ZXTW010104: FieldDef = FieldDef {
    id: "ZXTW010104",
    name: "运动类型",
    data_type: DataType::C,
    length: 1,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_SPORT_TYPE"),
    source: None,
    note: "",
};
pub const ZXTW010105: FieldDef = FieldDef {
    id: "ZXTW010105",
    name: "运动日期",
    data_type: DataType::D,
    length: 8,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "YYYYMMDD",
};
pub const ZXTW010106: FieldDef = FieldDef {
    id: "ZXTW010106",
    name: "运动时长(分钟)",
    data_type: DataType::N,
    length: 4,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXTW010107: FieldDef = FieldDef {
    id: "ZXTW010107",
    name: "运动强度",
    data_type: DataType::C,
    length: 4,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "如 中/高",
};

/// 学生体育运动（ZXTW01）。
pub struct Sport {
    pub sport_id: String,
    pub student_id: String,
    pub school_id: String,
    pub sport_type: String,
    pub sport_date: String,
    pub duration: String,
    pub intensity: String,
}

impl EmgiRecordable for Sport {
    const SUBSET: &'static str = "ZXTW";
    const CLASS_ID: &'static str = "ZXTW0101";
    const CLASS_NAME: &'static str = "学生体育运动";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXTW010101, Some(self.sport_id.clone())),
            (&ZXTW010102, Some(self.student_id.clone())),
            (&ZXTW010103, Some(self.school_id.clone())),
            (&ZXTW010104, Some(self.sport_type.clone())),
            (&ZXTW010105, Some(self.sport_date.clone())),
            (&ZXTW010106, Some(self.duration.clone())),
            (&ZXTW010107, Some(self.intensity.clone())),
        ]
    }
}

pub const ZXTW020101: FieldDef = FieldDef {
    id: "ZXTW020101",
    name: "医疗标识",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXTW020102: FieldDef = FieldDef {
    id: "ZXTW020102",
    name: "学生标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXS0101"),
    note: "取用 JCXS0101 学生标识码",
};
pub const ZXTW020103: FieldDef = FieldDef {
    id: "ZXTW020103",
    name: "学校标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXX0101"),
    note: "取用 JCXX0101 学校标识码",
};
pub const ZXTW020104: FieldDef = FieldDef {
    id: "ZXTW020104",
    name: "医疗类型",
    data_type: DataType::C,
    length: 1,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_MEDICAL_TYPE"),
    source: None,
    note: "",
};
pub const ZXTW020105: FieldDef = FieldDef {
    id: "ZXTW020105",
    name: "医疗日期",
    data_type: DataType::D,
    length: 8,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "YYYYMMDD",
};
pub const ZXTW020106: FieldDef = FieldDef {
    id: "ZXTW020106",
    name: "诊断",
    data_type: DataType::C,
    length: 200,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXTW020107: FieldDef = FieldDef {
    id: "ZXTW020107",
    name: "医疗机构",
    data_type: DataType::C,
    length: 80,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};

/// 医疗保健（ZXTW02）。
pub struct Medical {
    pub medical_id: String,
    pub student_id: String,
    pub school_id: String,
    pub medical_type: String,
    pub medical_date: String,
    pub diagnosis: String,
    pub hospital: String,
}

impl EmgiRecordable for Medical {
    const SUBSET: &'static str = "ZXTW";
    const CLASS_ID: &'static str = "ZXTW0201";
    const CLASS_NAME: &'static str = "医疗保健";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXTW020101, Some(self.medical_id.clone())),
            (&ZXTW020102, Some(self.student_id.clone())),
            (&ZXTW020103, Some(self.school_id.clone())),
            (&ZXTW020104, Some(self.medical_type.clone())),
            (&ZXTW020105, Some(self.medical_date.clone())),
            (&ZXTW020106, Some(self.diagnosis.clone())),
            (&ZXTW020107, Some(self.hospital.clone())),
        ]
    }
}
