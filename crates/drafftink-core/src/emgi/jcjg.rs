//! # JCJG 教职工管理信息数据子集
//!
//! 实现 JY/T 1002-2012 表 4 教职工子集的四个数据类：
//!
//! | 数据类 | 标识符 | 说明 |
//! |--------|--------|------|
//! | 教职工基本 | `JCJG0101` | 教职工标识/姓名等，**取用** `JCTB0201` 人员基本 |
//! | 学历学位 | `JCJG0102` | 学历/学位/毕业院校，**引用** `JCJG0101` |
//! | 专业技术职务 | `JCJG0103` | 职务/级别/聘任，**引用** `JCJG0101` |
//! | 党政职务 | `JCJG0104` | 党政职务/任职，**引用** `JCJG0101` |
//!
//! 教职工基本复用「人员基本」(JCTB0201) 的姓名/性别/出生日期/证件等数据元素（取用）。

use serde::{Deserialize, Serialize};

use super::jctb::PersonBasic;
use super::types::{DataType, EmgiRecordable, FieldDef, Obligation};

// ════════════════════════════════════════════════════════════════════════════
//  JCJG0101 教职工基本数据类（取用 JCTB0201）
// ════════════════════════════════════════════════════════════════════════════

const JCJG0101_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCJG010101", name: "教职工标识码", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCJG010102", name: "姓名", data_type: DataType::C, length: 50, obligation: Obligation::M, code_ref: None, source: Some("JCTB020101"), note: "取用 JCTB020101" },
    FieldDef { id: "JCJG010103", name: "性别代码", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("GBT_2261_1_GENDER"), source: Some("JCTB020102"), note: "取用 JCTB020102" },
    FieldDef { id: "JCJG010104", name: "出生日期", data_type: DataType::D, length: 8, obligation: Obligation::M, code_ref: None, source: Some("JCTB020103"), note: "取用 JCTB020103" },
    FieldDef { id: "JCJG010105", name: "身份证件类型代码", data_type: DataType::C, length: 2, obligation: Obligation::M, code_ref: Some("JYT_1001_ID_TYPE"), source: Some("JCTB020104"), note: "取用 JCTB020104" },
    FieldDef { id: "JCJG010106", name: "身份证件号", data_type: DataType::C, length: 18, obligation: Obligation::M, code_ref: None, source: Some("JCTB020105"), note: "取用 JCTB020105" },
    FieldDef { id: "JCJG010107", name: "民族代码", data_type: DataType::C, length: 2, obligation: Obligation::O, code_ref: Some("GBT_3304_ETHNICITY"), source: Some("JCTB020106"), note: "取用 JCTB020106" },
    FieldDef { id: "JCJG010108", name: "国籍/地区代码", data_type: DataType::C, length: 3, obligation: Obligation::O, code_ref: Some("GBT_2659_NATIONALITY"), source: Some("JCTB020107"), note: "取用 JCTB020107" },
    FieldDef { id: "JCJG010109", name: "政治面貌代码", data_type: DataType::C, length: 2, obligation: Obligation::O, code_ref: Some("GBT_4762_POLITICAL"), source: Some("JCTB020108"), note: "取用 JCTB020108" },
    FieldDef { id: "JCJG010110", name: "婚姻状况代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("GBT_2261_2_MARITAL"), source: Some("JCTB020110"), note: "取用 JCTB020110" },
    FieldDef { id: "JCJG010111", name: "港澳台侨代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_HMT"), source: Some("JCTB020111"), note: "取用 JCTB020111" },
    FieldDef { id: "JCJG010112", name: "教职工类别代码", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_STAFF_CATEGORY"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCJG010113", name: "当前状态代码", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_STAFF_STATE"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCJG010114", name: "从教日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCJG010115", name: "进本单位日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCJG010116", name: "教职工来源代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_STAFF_SOURCE"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCJG010117", name: "照片", data_type: DataType::B, length: 0, obligation: Obligation::O, code_ref: None, source: None, note: "Base64 编码" },
];

/// 教职工基本数据结构（JCJG0101）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StaffBasic {
    pub staff_id: Option<String>,
    pub person: PersonBasic,
    pub staff_category: Option<String>,
    pub staff_state: Option<String>,
    pub teaching_start_date: Option<String>,
    pub join_date: Option<String>,
    pub staff_source: Option<String>,
    pub photo: Option<String>,
}

impl EmgiRecordable for StaffBasic {
    const SUBSET: &'static str = "JCJG";
    const CLASS_ID: &'static str = "JCJG0101";
    const CLASS_NAME: &'static str = "教职工基本";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCJG0101_FIELDS[0], self.staff_id.clone()),
            (&JCJG0101_FIELDS[1], self.person.name.clone()),
            (&JCJG0101_FIELDS[2], self.person.gender.clone()),
            (&JCJG0101_FIELDS[3], self.person.birth_date.clone()),
            (&JCJG0101_FIELDS[4], self.person.id_type.clone()),
            (&JCJG0101_FIELDS[5], self.person.id_number.clone()),
            (&JCJG0101_FIELDS[6], self.person.ethnicity.clone()),
            (&JCJG0101_FIELDS[7], self.person.nationality.clone()),
            (&JCJG0101_FIELDS[8], self.person.political.clone()),
            (&JCJG0101_FIELDS[9], self.person.marital.clone()),
            (&JCJG0101_FIELDS[10], self.person.hmt.clone()),
            (&JCJG0101_FIELDS[11], self.staff_category.clone()),
            (&JCJG0101_FIELDS[12], self.staff_state.clone()),
            (&JCJG0101_FIELDS[13], self.teaching_start_date.clone()),
            (&JCJG0101_FIELDS[14], self.join_date.clone()),
            (&JCJG0101_FIELDS[15], self.staff_source.clone()),
            (&JCJG0101_FIELDS[16], self.photo.clone()),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCTB0201"]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCJG0102 学历学位数据类
// ════════════════════════════════════════════════════════════════════════════

const JCJG0102_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCJG010201", name: "学历代码", data_type: DataType::C, length: 2, obligation: Obligation::M, code_ref: Some("GBT_4658_EDUCATION"), source: None, note: "GB/T 4658" },
    FieldDef { id: "JCJG010202", name: "学位代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("GBT_6864_DEGREE"), source: None, note: "GB/T 6864" },
    FieldDef { id: "JCJG010203", name: "毕业院校", data_type: DataType::C, length: 60, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCJG010204", name: "所学专业", data_type: DataType::C, length: 30, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCJG010205", name: "毕业日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCJG010206", name: "教职工标识码", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "引用 JCJG0101" },
];

/// 学历学位数据结构（JCJG0102）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StaffEducation {
    pub education_code: Option<String>,
    pub degree_code: Option<String>,
    pub grad_school: Option<String>,
    pub major: Option<String>,
    pub grad_date: Option<String>,
    pub staff_id: Option<String>,
}

impl EmgiRecordable for StaffEducation {
    const SUBSET: &'static str = "JCJG";
    const CLASS_ID: &'static str = "JCJG0102";
    const CLASS_NAME: &'static str = "学历学位";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCJG0102_FIELDS[0], self.education_code.clone()),
            (&JCJG0102_FIELDS[1], self.degree_code.clone()),
            (&JCJG0102_FIELDS[2], self.grad_school.clone()),
            (&JCJG0102_FIELDS[3], self.major.clone()),
            (&JCJG0102_FIELDS[4], self.grad_date.clone()),
            (&JCJG0102_FIELDS[5], self.staff_id.clone()),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCJG0101"]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCJG0103 专业技术职务数据类
// ════════════════════════════════════════════════════════════════════════════

const JCJG0103_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCJG010301", name: "专业技术职务代码", data_type: DataType::C, length: 4, obligation: Obligation::M, code_ref: Some("GBT_8561_TITLE"), source: None, note: "GB/T 8561" },
    FieldDef { id: "JCJG010302", name: "专业技术职务级别代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_TITLE_LEVEL"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCJG010303", name: "聘任日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCJG010304", name: "聘任单位", data_type: DataType::C, length: 60, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCJG010305", name: "教职工标识码", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "引用 JCJG0101" },
];

/// 专业技术职务数据结构（JCJG0103）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StaffTitle {
    pub title_code: Option<String>,
    pub title_level: Option<String>,
    pub appoint_date: Option<String>,
    pub appoint_org: Option<String>,
    pub staff_id: Option<String>,
}

impl EmgiRecordable for StaffTitle {
    const SUBSET: &'static str = "JCJG";
    const CLASS_ID: &'static str = "JCJG0103";
    const CLASS_NAME: &'static str = "专业技术职务";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCJG0103_FIELDS[0], self.title_code.clone()),
            (&JCJG0103_FIELDS[1], self.title_level.clone()),
            (&JCJG0103_FIELDS[2], self.appoint_date.clone()),
            (&JCJG0103_FIELDS[3], self.appoint_org.clone()),
            (&JCJG0103_FIELDS[4], self.staff_id.clone()),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCJG0101"]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCJG0104 党政职务数据类
// ════════════════════════════════════════════════════════════════════════════

const JCJG0104_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCJG010401", name: "党政职务代码", data_type: DataType::C, length: 4, obligation: Obligation::M, code_ref: None, source: None, note: "党政职务分类代码" },
    FieldDef { id: "JCJG010402", name: "任职日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCJG010403", name: "任职单位", data_type: DataType::C, length: 60, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCJG010404", name: "教职工标识码", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "引用 JCJG0101" },
];

/// 党政职务数据结构（JCJG0104）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StaffPartyPost {
    pub post_code: Option<String>,
    pub post_date: Option<String>,
    pub post_org: Option<String>,
    pub staff_id: Option<String>,
}

impl EmgiRecordable for StaffPartyPost {
    const SUBSET: &'static str = "JCJG";
    const CLASS_ID: &'static str = "JCJG0104";
    const CLASS_NAME: &'static str = "党政职务";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCJG0104_FIELDS[0], self.post_code.clone()),
            (&JCJG0104_FIELDS[1], self.post_date.clone()),
            (&JCJG0104_FIELDS[2], self.post_org.clone()),
            (&JCJG0104_FIELDS[3], self.staff_id.clone()),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCJG0101"]
    }
}
