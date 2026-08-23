//! # JCTB 通用基础信息数据子集
//!
//! 实现 JY/T 1002-2012 表 1 通用子集的五个数据类：
//!
//! | 数据类 | 标识符 | 说明 |
//! |--------|--------|------|
//! | 通用通讯 | `JCTB0101` | 电子信箱/电话/地址等（作为**引用格式源**） |
//! | 通用时间 | `JCTB0102` | 学年/学期/起止日期 |
//! | 单位基本 | `JCTB0103` | 单位名称/代码/地址 |
//! | 通用教学 | `JCTB0104` | 学科/年级/课程 |
//! | 人员基本 | `JCTB0201` | 姓名/性别/出生日期/证件（作为**取用源**） |
//!
//! - [`CommContact`] 为「通用通讯」格式，被学校/校区等数据类**引用**（格式复用）；
//! - [`PersonBasic`] 为「人员基本」数据，被学生/教职工等数据类**取用**（数据项复用）。

use serde::{Deserialize, Serialize};

use super::types::{DataType, EmgiRecordable, FieldDef, Obligation};

// ════════════════════════════════════════════════════════════════════════════
//  JCTB0101 通用通讯类（引用格式源）
// ════════════════════════════════════════════════════════════════════════════

/// 通用通讯格式定义（JCTB0101）。可被其他数据类**引用**复用其字段结构。
pub const JCTB0101_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCTB010101", name: "电子信箱", data_type: DataType::C, length: 40, obligation: Obligation::O, code_ref: None, source: None, note: "电子邮件地址" },
    FieldDef { id: "JCTB010102", name: "固定电话号码", data_type: DataType::C, length: 24, obligation: Obligation::O, code_ref: None, source: None, note: "含区号" },
    FieldDef { id: "JCTB010103", name: "移动电话号码", data_type: DataType::C, length: 20, obligation: Obligation::O, code_ref: None, source: None, note: "手机号码" },
    FieldDef { id: "JCTB010104", name: "通信地址", data_type: DataType::C, length: 90, obligation: Obligation::O, code_ref: None, source: None, note: "详细通信地址" },
    FieldDef { id: "JCTB010105", name: "邮政编码", data_type: DataType::C, length: 6, obligation: Obligation::O, code_ref: Some("GBT_POSTAL_CODE"), source: None, note: "6 位邮政编码" },
    FieldDef { id: "JCTB010106", name: "传真号码", data_type: DataType::C, length: 24, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCTB010107", name: "主页地址", data_type: DataType::C, length: 60, obligation: Obligation::O, code_ref: None, source: None, note: "网址" },
];

/// 通用通讯数据结构（JCTB0101）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommContact {
    /// 电子信箱（JCTB010101）
    pub email: Option<String>,
    /// 固定电话号码（JCTB010102）
    pub telephone: Option<String>,
    /// 移动电话号码（JCTB010103）
    pub mobile: Option<String>,
    /// 通信地址（JCTB010104）
    pub address: Option<String>,
    /// 邮政编码（JCTB010105）
    pub postal_code: Option<String>,
    /// 传真号码（JCTB010106）
    pub fax: Option<String>,
    /// 主页地址（JCTB010107）
    pub homepage: Option<String>,
}

impl CommContact {
    /// 返回本结构承载的通讯字段（按 JCTB0101 标识符）。用于被引用类复用。
    pub fn as_fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCTB0101_FIELDS[0], self.email.clone()),
            (&JCTB0101_FIELDS[1], self.telephone.clone()),
            (&JCTB0101_FIELDS[2], self.mobile.clone()),
            (&JCTB0101_FIELDS[3], self.address.clone()),
            (&JCTB0101_FIELDS[4], self.postal_code.clone()),
            (&JCTB0101_FIELDS[5], self.fax.clone()),
            (&JCTB0101_FIELDS[6], self.homepage.clone()),
        ]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCTB0102 通用时间类
// ════════════════════════════════════════════════════════════════════════════

const JCTB0102_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCTB010201", name: "年份", data_type: DataType::N, length: 4, obligation: Obligation::M, code_ref: None, source: None, note: "4 位年份" },
    FieldDef { id: "JCTB010202", name: "学期代码", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_TERM"), source: None, note: "1 第一学期/2 第二学期" },
    FieldDef { id: "JCTB010203", name: "起始日期", data_type: DataType::D, length: 8, obligation: Obligation::M, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCTB010204", name: "结束日期", data_type: DataType::D, length: 8, obligation: Obligation::M, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCTB010205", name: "日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCTB010206", name: "时间", data_type: DataType::T, length: 6, obligation: Obligation::O, code_ref: None, source: None, note: "hhmmss" },
];

/// 通用时间数据结构（JCTB0102）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeneralTime {
    pub year: Option<String>,
    pub term: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub date: Option<String>,
    pub time: Option<String>,
}

impl EmgiRecordable for GeneralTime {
    const SUBSET: &'static str = "JCTB";
    const CLASS_ID: &'static str = "JCTB0102";
    const CLASS_NAME: &'static str = "通用时间";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCTB0102_FIELDS[0], self.year.clone()),
            (&JCTB0102_FIELDS[1], self.term.clone()),
            (&JCTB0102_FIELDS[2], self.start_date.clone()),
            (&JCTB0102_FIELDS[3], self.end_date.clone()),
            (&JCTB0102_FIELDS[4], self.date.clone()),
            (&JCTB0102_FIELDS[5], self.time.clone()),
        ]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCTB0103 单位基本类
// ════════════════════════════════════════════════════════════════════════════

const JCTB0103_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCTB010301", name: "单位名称", data_type: DataType::C, length: 60, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCTB010302", name: "单位代码", data_type: DataType::C, length: 18, obligation: Obligation::M, code_ref: Some("GBT_CREDIT_CODE"), source: None, note: "统一社会信用代码" },
    FieldDef { id: "JCTB010303", name: "单位地址", data_type: DataType::C, length: 90, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCTB010304", name: "单位简码", data_type: DataType::C, length: 10, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCTB010305", name: "单位英文名称", data_type: DataType::C, length: 120, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCTB010306", name: "单位网址", data_type: DataType::C, length: 60, obligation: Obligation::O, code_ref: None, source: None, note: "" },
];

/// 单位基本数据结构（JCTB0103）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrgBasic {
    pub org_name: Option<String>,
    pub org_code: Option<String>,
    pub org_address: Option<String>,
    pub org_short: Option<String>,
    pub org_name_en: Option<String>,
    pub org_url: Option<String>,
}

impl EmgiRecordable for OrgBasic {
    const SUBSET: &'static str = "JCTB";
    const CLASS_ID: &'static str = "JCTB0103";
    const CLASS_NAME: &'static str = "单位基本";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCTB0103_FIELDS[0], self.org_name.clone()),
            (&JCTB0103_FIELDS[1], self.org_code.clone()),
            (&JCTB0103_FIELDS[2], self.org_address.clone()),
            (&JCTB0103_FIELDS[3], self.org_short.clone()),
            (&JCTB0103_FIELDS[4], self.org_name_en.clone()),
            (&JCTB0103_FIELDS[5], self.org_url.clone()),
        ]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCTB0104 通用教学类
// ════════════════════════════════════════════════════════════════════════════

const JCTB0104_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCTB010401", name: "学科代码", data_type: DataType::C, length: 2, obligation: Obligation::M, code_ref: Some("JYT_1001_SUBJECT"), source: None, note: "引用 JY/T 1001 学科" },
    FieldDef { id: "JCTB010402", name: "年级代码", data_type: DataType::C, length: 2, obligation: Obligation::M, code_ref: Some("JYT_1001_GRADE"), source: None, note: "引用 JY/T 1001 年级" },
    FieldDef { id: "JCTB010403", name: "课程代码", data_type: DataType::C, length: 20, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCTB010404", name: "课程名称", data_type: DataType::C, length: 60, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCTB010405", name: "学制", data_type: DataType::N, length: 2, obligation: Obligation::O, code_ref: None, source: None, note: "年" },
    FieldDef { id: "JCTB010406", name: "教学语言", data_type: DataType::C, length: 10, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCTB010407", name: "学时", data_type: DataType::N, length: 4, obligation: Obligation::O, code_ref: None, source: None, note: "总学时" },
];

/// 通用教学数据结构（JCTB0104）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeneralTeaching {
    pub subject: Option<String>,
    pub grade: Option<String>,
    pub course_code: Option<String>,
    pub course_name: Option<String>,
    pub school_system: Option<String>,
    pub teaching_lang: Option<String>,
    pub class_hours: Option<String>,
}

impl EmgiRecordable for GeneralTeaching {
    const SUBSET: &'static str = "JCTB";
    const CLASS_ID: &'static str = "JCTB0104";
    const CLASS_NAME: &'static str = "通用教学";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCTB0104_FIELDS[0], self.subject.clone()),
            (&JCTB0104_FIELDS[1], self.grade.clone()),
            (&JCTB0104_FIELDS[2], self.course_code.clone()),
            (&JCTB0104_FIELDS[3], self.course_name.clone()),
            (&JCTB0104_FIELDS[4], self.school_system.clone()),
            (&JCTB0104_FIELDS[5], self.teaching_lang.clone()),
            (&JCTB0104_FIELDS[6], self.class_hours.clone()),
        ]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCTB0201 人员基本类（取用源）
// ════════════════════════════════════════════════════════════════════════════

const JCTB0201_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCTB020101", name: "姓名", data_type: DataType::C, length: 50, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCTB020102", name: "性别代码", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("GBT_2261_1_GENDER"), source: None, note: "GB/T 2261.1" },
    FieldDef { id: "JCTB020103", name: "出生日期", data_type: DataType::D, length: 8, obligation: Obligation::M, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCTB020104", name: "身份证件类型代码", data_type: DataType::C, length: 2, obligation: Obligation::M, code_ref: Some("JYT_1001_ID_TYPE"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCTB020105", name: "身份证件号", data_type: DataType::C, length: 18, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCTB020106", name: "民族代码", data_type: DataType::C, length: 2, obligation: Obligation::O, code_ref: Some("GBT_3304_ETHNICITY"), source: None, note: "GB/T 3304" },
    FieldDef { id: "JCTB020107", name: "国籍/地区代码", data_type: DataType::C, length: 3, obligation: Obligation::O, code_ref: Some("GBT_2659_NATIONALITY"), source: None, note: "GB/T 2659" },
    FieldDef { id: "JCTB020108", name: "政治面貌代码", data_type: DataType::C, length: 2, obligation: Obligation::O, code_ref: Some("GBT_4762_POLITICAL"), source: None, note: "GB/T 4762" },
    FieldDef { id: "JCTB020109", name: "健康状况代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_HEALTH"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCTB020110", name: "婚姻状况代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("GBT_2261_2_MARITAL"), source: None, note: "GB/T 2261.2" },
    FieldDef { id: "JCTB020111", name: "港澳台侨代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_HMT"), source: None, note: "JY/T 1001" },
];

/// 人员基本数据结构（JCTB0201）。
///
/// 作为**取用源**：学生基本(`JCXS0101`)、教职工基本(`JCJG0101`) 等通过
/// `source: Some("JCTB0201xx")` 复用其中数据元素，避免重复定义。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersonBasic {
    pub name: Option<String>,
    pub gender: Option<String>,
    pub birth_date: Option<String>,
    pub id_type: Option<String>,
    pub id_number: Option<String>,
    pub ethnicity: Option<String>,
    pub nationality: Option<String>,
    pub political: Option<String>,
    pub health: Option<String>,
    pub marital: Option<String>,
    pub hmt: Option<String>,
}

impl PersonBasic {
    /// 返回本结构的人员基本字段（按 JCTB0201 标识符）。供取用类复用。
    pub fn as_fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCTB0201_FIELDS[0], self.name.clone()),
            (&JCTB0201_FIELDS[1], self.gender.clone()),
            (&JCTB0201_FIELDS[2], self.birth_date.clone()),
            (&JCTB0201_FIELDS[3], self.id_type.clone()),
            (&JCTB0201_FIELDS[4], self.id_number.clone()),
            (&JCTB0201_FIELDS[5], self.ethnicity.clone()),
            (&JCTB0201_FIELDS[6], self.nationality.clone()),
            (&JCTB0201_FIELDS[7], self.political.clone()),
            (&JCTB0201_FIELDS[8], self.health.clone()),
            (&JCTB0201_FIELDS[9], self.marital.clone()),
            (&JCTB0201_FIELDS[10], self.hmt.clone()),
        ]
    }
}

impl EmgiRecordable for PersonBasic {
    const SUBSET: &'static str = "JCTB";
    const CLASS_ID: &'static str = "JCTB0201";
    const CLASS_NAME: &'static str = "人员基本";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        self.as_fields()
    }
}
