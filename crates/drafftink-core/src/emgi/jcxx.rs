//! # JCXX 学校管理信息数据子集
//!
//! 实现 JY/T 1002-2012 表 2 学校子集的三个数据类：
//!
//! | 数据类 | 标识符 | 说明 |
//! |--------|--------|------|
//! | 学校基本 | `JCXX0101` | 学校标识/名称/性质，**引用** `JCTB0101` 通讯格式 |
//! | 校区基本 | `JCXX0102` | 校区标识/名称/面积，**引用** `JCTB0101` 通讯格式 |
//! | 班级数据 | `JCXX0201` | 班级标识/类型/班主任，**引用** `JCXX0101` 学校 |
//!
//! 学校与校区通过 [`CommContact`] **引用**「通用通讯」格式，避免重复定义通讯字段。

use serde::{Deserialize, Serialize};

use super::jctb::CommContact;
use super::types::{DataType, EmgiRecordable, FieldDef, Obligation};

// ════════════════════════════════════════════════════════════════════════════
//  JCXX0101 学校基本数据类
// ════════════════════════════════════════════════════════════════════════════

const JCXX0101_FIELDS: &[FieldDef] = &[
    FieldDef {
        id: "JCXX010101",
        name: "学校标识码",
        data_type: DataType::C,
        length: 19,
        obligation: Obligation::M,
        code_ref: None,
        source: None,
        note: "全国学校（机构）代码",
    },
    FieldDef {
        id: "JCXX010102",
        name: "学校名称",
        data_type: DataType::C,
        length: 60,
        obligation: Obligation::M,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010103",
        name: "学校英文名称",
        data_type: DataType::C,
        length: 120,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010104",
        name: "学校简称",
        data_type: DataType::C,
        length: 30,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010105",
        name: "学校代码",
        data_type: DataType::C,
        length: 10,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "校内自编代码",
    },
    FieldDef {
        id: "JCXX010106",
        name: "学校性质代码",
        data_type: DataType::C,
        length: 1,
        obligation: Obligation::M,
        code_ref: Some("JYT_1001_SCHOOL_NATURE"),
        source: None,
        note: "JY/T 1001",
    },
    FieldDef {
        id: "JCXX010107",
        name: "学校举办者代码",
        data_type: DataType::C,
        length: 1,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010108",
        name: "学校主管部门代码",
        data_type: DataType::C,
        length: 1,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010115",
        name: "校长姓名",
        data_type: DataType::C,
        length: 50,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010116",
        name: "校长身份证件号",
        data_type: DataType::C,
        length: 18,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010117",
        name: "建校年月",
        data_type: DataType::D,
        length: 8,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "YYYYMMDD",
    },
    FieldDef {
        id: "JCXX010118",
        name: "校区个数",
        data_type: DataType::N,
        length: 2,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010119",
        name: "班级数",
        data_type: DataType::N,
        length: 4,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010120",
        name: "学生数",
        data_type: DataType::N,
        length: 8,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010121",
        name: "教职工数",
        data_type: DataType::N,
        length: 6,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
];

/// 学校基本数据结构（JCXX0101）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchoolBasic {
    pub school_id: Option<String>,
    pub school_name: Option<String>,
    pub school_name_en: Option<String>,
    pub school_short: Option<String>,
    pub school_code: Option<String>,
    pub school_nature: Option<String>,
    pub school_sponsor: Option<String>,
    pub school_authority: Option<String>,
    pub principal_name: Option<String>,
    pub principal_id: Option<String>,
    pub founding_date: Option<String>,
    pub campus_count: Option<String>,
    pub class_count: Option<String>,
    pub student_count: Option<String>,
    pub staff_count: Option<String>,
    /// 通讯信息（**引用** JCTB0101 通用通讯格式）。
    pub contact: CommContact,
}

impl EmgiRecordable for SchoolBasic {
    const SUBSET: &'static str = "JCXX";
    const CLASS_ID: &'static str = "JCXX0101";
    const CLASS_NAME: &'static str = "学校基本";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        let mut v = vec![
            (&JCXX0101_FIELDS[0], self.school_id.clone()),
            (&JCXX0101_FIELDS[1], self.school_name.clone()),
            (&JCXX0101_FIELDS[2], self.school_name_en.clone()),
            (&JCXX0101_FIELDS[3], self.school_short.clone()),
            (&JCXX0101_FIELDS[4], self.school_code.clone()),
            (&JCXX0101_FIELDS[5], self.school_nature.clone()),
            (&JCXX0101_FIELDS[6], self.school_sponsor.clone()),
            (&JCXX0101_FIELDS[7], self.school_authority.clone()),
            (&JCXX0101_FIELDS[8], self.principal_name.clone()),
            (&JCXX0101_FIELDS[9], self.principal_id.clone()),
            (&JCXX0101_FIELDS[10], self.founding_date.clone()),
            (&JCXX0101_FIELDS[11], self.campus_count.clone()),
            (&JCXX0101_FIELDS[12], self.class_count.clone()),
            (&JCXX0101_FIELDS[13], self.student_count.clone()),
            (&JCXX0101_FIELDS[14], self.staff_count.clone()),
        ];
        v.extend(self.contact.as_fields());
        v
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCTB0101"]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCXX0102 校区基本数据类
// ════════════════════════════════════════════════════════════════════════════

const JCXX0102_FIELDS: &[FieldDef] = &[
    FieldDef {
        id: "JCXX010201",
        name: "校区标识码",
        data_type: DataType::C,
        length: 19,
        obligation: Obligation::M,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010202",
        name: "校区名称",
        data_type: DataType::C,
        length: 60,
        obligation: Obligation::M,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010207",
        name: "校区设施状况",
        data_type: DataType::C,
        length: 200,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX010208",
        name: "校区占地面积",
        data_type: DataType::N,
        length: 12,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "平方米",
    },
];

/// 校区基本数据结构（JCXX0102）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CampusBasic {
    pub campus_id: Option<String>,
    pub campus_name: Option<String>,
    pub facility: Option<String>,
    pub area: Option<String>,
    /// 通讯信息（**引用** JCTB0101 通用通讯格式）。
    pub contact: CommContact,
}

impl EmgiRecordable for CampusBasic {
    const SUBSET: &'static str = "JCXX";
    const CLASS_ID: &'static str = "JCXX0102";
    const CLASS_NAME: &'static str = "校区基本";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        let mut v = vec![
            (&JCXX0102_FIELDS[0], self.campus_id.clone()),
            (&JCXX0102_FIELDS[1], self.campus_name.clone()),
            (&JCXX0102_FIELDS[2], self.facility.clone()),
            (&JCXX0102_FIELDS[3], self.area.clone()),
        ];
        v.extend(self.contact.as_fields());
        v
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCTB0101"]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCXX0201 班级数据类
// ════════════════════════════════════════════════════════════════════════════

const JCXX0201_FIELDS: &[FieldDef] = &[
    FieldDef {
        id: "JCXX020101",
        name: "班级标识码",
        data_type: DataType::C,
        length: 19,
        obligation: Obligation::M,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX020102",
        name: "班级名称",
        data_type: DataType::C,
        length: 30,
        obligation: Obligation::M,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX020103",
        name: "班级类型代码",
        data_type: DataType::C,
        length: 1,
        obligation: Obligation::M,
        code_ref: Some("JYT_1001_CLASS_TYPE"),
        source: None,
        note: "JY/T 1001",
    },
    FieldDef {
        id: "JCXX020104",
        name: "年级代码",
        data_type: DataType::C,
        length: 2,
        obligation: Obligation::O,
        code_ref: Some("JYT_1001_GRADE"),
        source: None,
        note: "取用 JCTB0104 年级",
    },
    FieldDef {
        id: "JCXX020105",
        name: "班主任姓名",
        data_type: DataType::C,
        length: 50,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX020106",
        name: "班主任身份证件号",
        data_type: DataType::C,
        length: 18,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX020107",
        name: "班级人数",
        data_type: DataType::N,
        length: 4,
        obligation: Obligation::O,
        code_ref: None,
        source: None,
        note: "",
    },
    FieldDef {
        id: "JCXX020108",
        name: "所属学校标识码",
        data_type: DataType::C,
        length: 19,
        obligation: Obligation::M,
        code_ref: None,
        source: None,
        note: "引用 JCXX0101",
    },
];

/// 班级数据结构（JCXX0201）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassInfo {
    pub class_id: Option<String>,
    pub class_name: Option<String>,
    pub class_type: Option<String>,
    pub grade_code: Option<String>,
    pub head_teacher_name: Option<String>,
    pub head_teacher_id: Option<String>,
    pub class_size: Option<String>,
    pub school_id: Option<String>,
}

impl EmgiRecordable for ClassInfo {
    const SUBSET: &'static str = "JCXX";
    const CLASS_ID: &'static str = "JCXX0201";
    const CLASS_NAME: &'static str = "班级数据";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCXX0201_FIELDS[0], self.class_id.clone()),
            (&JCXX0201_FIELDS[1], self.class_name.clone()),
            (&JCXX0201_FIELDS[2], self.class_type.clone()),
            (&JCXX0201_FIELDS[3], self.grade_code.clone()),
            (&JCXX0201_FIELDS[4], self.head_teacher_name.clone()),
            (&JCXX0201_FIELDS[5], self.head_teacher_id.clone()),
            (&JCXX0201_FIELDS[6], self.class_size.clone()),
            (&JCXX0201_FIELDS[7], self.school_id.clone()),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCXX0101"]
    }
}
