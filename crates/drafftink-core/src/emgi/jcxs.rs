//! # JCXS 学生管理信息数据子集
//!
//! 实现 JY/T 1002-2012 表 3 学生子集的五个数据类：
//!
//! | 数据类 | 标识符 | 说明 |
//! |--------|--------|------|
//! | 学生基本 | `JCXS0101` | 学生标识/姓名/性别等，**取用** `JCTB0201` 人员基本 |
//! | 学籍基本 | `JCXS0201` | 学籍号/学号/状态，**引用** `JCXS0101` 与 `JCXX0201` |
//! | 成绩 | `JCXS0203` | 成绩/学分，**引用** `JCXS0101`、`JCTB0104` |
//! | 奖励 | `JCXS0204` | 奖励信息，**引用** `JCXS0101` |
//! | 惩处 | `JCXS0205` | 惩处信息，**引用** `JCXS0101` |
//!
//! 学生基本的姓名/性别/出生日期等数据元素均标注 `source: JCTB0201xx`，表示
//! **取用**「人员基本」数据，避免重复定义。

use serde::{Deserialize, Serialize};

use super::jctb::PersonBasic;
use super::types::{DataType, EmgiRecordable, FieldDef, Obligation};

// ════════════════════════════════════════════════════════════════════════════
//  JCXS0101 学生基本数据类（取用 JCTB0201）
// ════════════════════════════════════════════════════════════════════════════

const JCXS0101_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCXS010101", name: "学生标识码", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "全国学籍号" },
    FieldDef { id: "JCXS010102", name: "姓名", data_type: DataType::C, length: 50, obligation: Obligation::M, code_ref: None, source: Some("JCTB020101"), note: "取用 JCTB020101" },
    FieldDef { id: "JCXS010103", name: "性别代码", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("GBT_2261_1_GENDER"), source: Some("JCTB020102"), note: "取用 JCTB020102" },
    FieldDef { id: "JCXS010104", name: "出生日期", data_type: DataType::D, length: 8, obligation: Obligation::M, code_ref: None, source: Some("JCTB020103"), note: "取用 JCTB020103" },
    FieldDef { id: "JCXS010105", name: "身份证件类型代码", data_type: DataType::C, length: 2, obligation: Obligation::M, code_ref: Some("JYT_1001_ID_TYPE"), source: Some("JCTB020104"), note: "取用 JCTB020104" },
    FieldDef { id: "JCXS010106", name: "身份证件号", data_type: DataType::C, length: 18, obligation: Obligation::M, code_ref: None, source: Some("JCTB020105"), note: "取用 JCTB020105" },
    FieldDef { id: "JCXS010107", name: "民族代码", data_type: DataType::C, length: 2, obligation: Obligation::O, code_ref: Some("GBT_3304_ETHNICITY"), source: Some("JCTB020106"), note: "取用 JCTB020106" },
    FieldDef { id: "JCXS010108", name: "国籍/地区代码", data_type: DataType::C, length: 3, obligation: Obligation::O, code_ref: Some("GBT_2659_NATIONALITY"), source: Some("JCTB020107"), note: "取用 JCTB020107" },
    FieldDef { id: "JCXS010109", name: "港澳台侨代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_HMT"), source: Some("JCTB020111"), note: "取用 JCTB020111" },
    FieldDef { id: "JCXS010110", name: "政治面貌代码", data_type: DataType::C, length: 2, obligation: Obligation::O, code_ref: Some("GBT_4762_POLITICAL"), source: Some("JCTB020108"), note: "取用 JCTB020108" },
    FieldDef { id: "JCXS010111", name: "学生来源", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCXS010112", name: "健康状况代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_HEALTH"), source: Some("JCTB020109"), note: "取用 JCTB020109" },
    FieldDef { id: "JCXS010113", name: "血型代码", data_type: DataType::C, length: 2, obligation: Obligation::O, code_ref: Some("JYT_1001_BLOOD"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCXS010114", name: "照片", data_type: DataType::B, length: 0, obligation: Obligation::O, code_ref: None, source: None, note: "Base64 编码" },
];

/// 学生基本数据结构（JCXS0101）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StudentBasic {
    /// 学生标识码（JCXS010101，本类自有）
    pub student_id: Option<String>,
    /// 人员基本数据（**取用** JCTB0201 的姓名/性别/出生日期/证件等）
    pub person: PersonBasic,
    /// 学生来源（JCXS010111）
    pub student_source: Option<String>,
    /// 血型代码（JCXS010113）
    pub blood: Option<String>,
    /// 照片 Base64（JCXS010114）
    pub photo: Option<String>,
}

impl EmgiRecordable for StudentBasic {
    const SUBSET: &'static str = "JCXS";
    const CLASS_ID: &'static str = "JCXS0101";
    const CLASS_NAME: &'static str = "学生基本";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCXS0101_FIELDS[0], self.student_id.clone()),
            (&JCXS0101_FIELDS[1], self.person.name.clone()),
            (&JCXS0101_FIELDS[2], self.person.gender.clone()),
            (&JCXS0101_FIELDS[3], self.person.birth_date.clone()),
            (&JCXS0101_FIELDS[4], self.person.id_type.clone()),
            (&JCXS0101_FIELDS[5], self.person.id_number.clone()),
            (&JCXS0101_FIELDS[6], self.person.ethnicity.clone()),
            (&JCXS0101_FIELDS[7], self.person.nationality.clone()),
            (&JCXS0101_FIELDS[8], self.person.hmt.clone()),
            (&JCXS0101_FIELDS[9], self.person.political.clone()),
            (&JCXS0101_FIELDS[10], self.student_source.clone()),
            (&JCXS0101_FIELDS[11], self.person.health.clone()),
            (&JCXS0101_FIELDS[12], self.blood.clone()),
            (&JCXS0101_FIELDS[13], self.photo.clone()),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCTB0201"]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCXS0201 学籍基本数据类
// ════════════════════════════════════════════════════════════════════════════

const JCXS0201_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCXS020101", name: "学籍号", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCXS020102", name: "学号", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: None, note: "校内学号" },
    FieldDef { id: "JCXS020103", name: "入学日期", data_type: DataType::D, length: 8, obligation: Obligation::M, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCXS020104", name: "入学方式代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_ENROLL_TYPE"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCXS020105", name: "就读方式代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_STUDY_MODE"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCXS020106", name: "学生类别代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_STUDENT_CATEGORY"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCXS020107", name: "学籍状态代码", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_STATUS_STATE"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCXS020108", name: "所在班级标识码", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "引用 JCXX0201" },
    FieldDef { id: "JCXS020109", name: "年级代码", data_type: DataType::C, length: 2, obligation: Obligation::O, code_ref: Some("JYT_1001_GRADE"), source: None, note: "取用 JCTB0104" },
    FieldDef { id: "JCXS020110", name: "专业代码", data_type: DataType::C, length: 6, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCXS020111", name: "预计毕业日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCXS020112", name: "学生标识码", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "引用 JCXS0101" },
];

/// 学籍基本数据结构（JCXS0201）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StudentStatus {
    pub status_id: Option<String>,
    pub student_no: Option<String>,
    pub enroll_date: Option<String>,
    pub enroll_type: Option<String>,
    pub study_mode: Option<String>,
    pub student_category: Option<String>,
    pub status_state: Option<String>,
    pub class_id: Option<String>,
    pub grade_code: Option<String>,
    pub major_code: Option<String>,
    pub graduate_date: Option<String>,
    pub student_id: Option<String>,
}

impl EmgiRecordable for StudentStatus {
    const SUBSET: &'static str = "JCXS";
    const CLASS_ID: &'static str = "JCXS0201";
    const CLASS_NAME: &'static str = "学籍基本";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCXS0201_FIELDS[0], self.status_id.clone()),
            (&JCXS0201_FIELDS[1], self.student_no.clone()),
            (&JCXS0201_FIELDS[2], self.enroll_date.clone()),
            (&JCXS0201_FIELDS[3], self.enroll_type.clone()),
            (&JCXS0201_FIELDS[4], self.study_mode.clone()),
            (&JCXS0201_FIELDS[5], self.student_category.clone()),
            (&JCXS0201_FIELDS[6], self.status_state.clone()),
            (&JCXS0201_FIELDS[7], self.class_id.clone()),
            (&JCXS0201_FIELDS[8], self.grade_code.clone()),
            (&JCXS0201_FIELDS[9], self.major_code.clone()),
            (&JCXS0201_FIELDS[10], self.graduate_date.clone()),
            (&JCXS0201_FIELDS[11], self.student_id.clone()),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCXS0101", "JCXX0201"]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCXS0203 成绩数据类
// ════════════════════════════════════════════════════════════════════════════

const JCXS0203_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCXS020301", name: "成绩信息标识", data_type: DataType::C, length: 36, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCXS020302", name: "学生标识码", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "引用 JCXS0101" },
    FieldDef { id: "JCXS020303", name: "考试/考查科目代码", data_type: DataType::C, length: 10, obligation: Obligation::M, code_ref: Some("JYT_1001_SUBJECT"), source: None, note: "引用 JCTB0104" },
    FieldDef { id: "JCXS020304", name: "成绩", data_type: DataType::N, length: 6, obligation: Obligation::M, code_ref: None, source: None, note: "百分制" },
    FieldDef { id: "JCXS020305", name: "学分", data_type: DataType::N, length: 4, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCXS020306", name: "成绩类型代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_SCORE_TYPE"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCXS020307", name: "考试日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCXS020308", name: "学期", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_TERM"), source: None, note: "取用 JCTB0102" },
    FieldDef { id: "JCXS020309", name: "年级代码", data_type: DataType::C, length: 2, obligation: Obligation::O, code_ref: Some("JYT_1001_GRADE"), source: None, note: "取用 JCTB0104" },
];

/// 成绩数据结构（JCXS0203）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoreInfo {
    pub score_id: Option<String>,
    pub student_id: Option<String>,
    pub subject_code: Option<String>,
    pub score: Option<String>,
    pub credit: Option<String>,
    pub score_type: Option<String>,
    pub exam_date: Option<String>,
    pub term: Option<String>,
    pub grade_code: Option<String>,
}

impl EmgiRecordable for ScoreInfo {
    const SUBSET: &'static str = "JCXS";
    const CLASS_ID: &'static str = "JCXS0203";
    const CLASS_NAME: &'static str = "成绩";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCXS0203_FIELDS[0], self.score_id.clone()),
            (&JCXS0203_FIELDS[1], self.student_id.clone()),
            (&JCXS0203_FIELDS[2], self.subject_code.clone()),
            (&JCXS0203_FIELDS[3], self.score.clone()),
            (&JCXS0203_FIELDS[4], self.credit.clone()),
            (&JCXS0203_FIELDS[5], self.score_type.clone()),
            (&JCXS0203_FIELDS[6], self.exam_date.clone()),
            (&JCXS0203_FIELDS[7], self.term.clone()),
            (&JCXS0203_FIELDS[8], self.grade_code.clone()),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCXS0101", "JCTB0104"]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCXS0204 奖励数据类
// ════════════════════════════════════════════════════════════════════════════

const JCXS0204_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCXS020401", name: "奖励标识", data_type: DataType::C, length: 36, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCXS020402", name: "学生标识码", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "引用 JCXS0101" },
    FieldDef { id: "JCXS020403", name: "奖励名称", data_type: DataType::C, length: 60, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCXS020404", name: "奖励类别代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_AWARD_TYPE"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCXS020405", name: "奖励级别代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_AWARD_LEVEL"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCXS020406", name: "奖励日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCXS020407", name: "颁奖单位", data_type: DataType::C, length: 60, obligation: Obligation::O, code_ref: None, source: None, note: "" },
];

/// 奖励数据结构（JCXS0204）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AwardInfo {
    pub award_id: Option<String>,
    pub student_id: Option<String>,
    pub award_name: Option<String>,
    pub award_type: Option<String>,
    pub award_level: Option<String>,
    pub award_date: Option<String>,
    pub award_org: Option<String>,
}

impl EmgiRecordable for AwardInfo {
    const SUBSET: &'static str = "JCXS";
    const CLASS_ID: &'static str = "JCXS0204";
    const CLASS_NAME: &'static str = "奖励";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCXS0204_FIELDS[0], self.award_id.clone()),
            (&JCXS0204_FIELDS[1], self.student_id.clone()),
            (&JCXS0204_FIELDS[2], self.award_name.clone()),
            (&JCXS0204_FIELDS[3], self.award_type.clone()),
            (&JCXS0204_FIELDS[4], self.award_level.clone()),
            (&JCXS0204_FIELDS[5], self.award_date.clone()),
            (&JCXS0204_FIELDS[6], self.award_org.clone()),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCXS0101"]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCXS0205 惩处数据类
// ════════════════════════════════════════════════════════════════════════════

const JCXS0205_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCXS020501", name: "惩处标识", data_type: DataType::C, length: 36, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCXS020502", name: "学生标识码", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "引用 JCXS0101" },
    FieldDef { id: "JCXS020503", name: "惩处名称", data_type: DataType::C, length: 60, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCXS020504", name: "惩处类别代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_PUNISH_TYPE"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCXS020505", name: "惩处级别代码", data_type: DataType::C, length: 1, obligation: Obligation::O, code_ref: Some("JYT_1001_PUNISH_LEVEL"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCXS020506", name: "惩处日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" },
    FieldDef { id: "JCXS020507", name: "惩处单位", data_type: DataType::C, length: 60, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCXS020508", name: "撤销日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" },
];

/// 惩处数据结构（JCXS0205）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PunishmentInfo {
    pub punish_id: Option<String>,
    pub student_id: Option<String>,
    pub punish_name: Option<String>,
    pub punish_type: Option<String>,
    pub punish_level: Option<String>,
    pub punish_date: Option<String>,
    pub punish_org: Option<String>,
    pub revoke_date: Option<String>,
}

impl EmgiRecordable for PunishmentInfo {
    const SUBSET: &'static str = "JCXS";
    const CLASS_ID: &'static str = "JCXS0205";
    const CLASS_NAME: &'static str = "惩处";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCXS0205_FIELDS[0], self.punish_id.clone()),
            (&JCXS0205_FIELDS[1], self.student_id.clone()),
            (&JCXS0205_FIELDS[2], self.punish_name.clone()),
            (&JCXS0205_FIELDS[3], self.punish_type.clone()),
            (&JCXS0205_FIELDS[4], self.punish_level.clone()),
            (&JCXS0205_FIELDS[5], self.punish_date.clone()),
            (&JCXS0205_FIELDS[6], self.punish_org.clone()),
            (&JCXS0205_FIELDS[7], self.revoke_date.clone()),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCXS0101"]
    }
}
