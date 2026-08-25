//! # emgi — JY/T 1002-2012 教育管理基础信息数据模型
//!
//! 提供符合国家标准《教育管理信息 教育管理基础信息》（JY/T 1002-2012）的
//! 数据底座，供上层教学应用（授课、作业、学籍等）复用合规的教育管理数据元素。
//!
//! ## 五大子集
//!
//! | 子集 | 名称 | 数据类 |
//! |------|------|--------|
//! | `JCTB` | 通用基础信息子集 | 通用通讯/通用时间/单位基本/通用教学/人员基本 |
//! | `JCXX` | 学校管理信息子集 | 学校基本/校区基本/班级数据 |
//! | `JCXS` | 学生管理信息子集 | 学生基本/学籍基本/成绩/奖励/惩处 |
//! | `JCJG` | 教职工管理信息子集 | 教职工基本/学历学位/专业技术职务/党政职务 |
//! | `JCBX` | 办学条件管理信息子集 | 校舍场所/仪器设备 |
//!
//! ## 取用与引用
//!
//! - **取用（数据项复用）**：学生的「姓名/性别/出生日期」等直接复用「人员基本」
//!   (`JCTB0201`) 的数据元素，在 [`FieldDef::source`](types::FieldDef::source) 中标注来源标识。
//! - **引用（格式复用）**：学校/校区复用「通用通讯」(`JCTB0101`) 的字段格式，
//!   由 [`CommContact`] 提供；学籍/成绩等通过 `references()` 指向其依赖的数据类。
//!
//! ## 合规要点
//!
//! - 日期统一 `YYYYMMDD`，时间统一 `hhmmss`（见 [`types::is_valid_date8`] / [`types::is_valid_time6`]）。
//! - 所有代码引用 GB/T 与 JY/T 1001 标准代码表（见 [`codes`]）。
//! - 必备(M)元素缺失即判定不合规（见 [`EmgiRecordable::validate`]）。

pub mod codes;
pub mod jcbx;
pub mod jcjg;
pub mod jctb;
pub mod jcxs;
pub mod jcxx;
pub mod types;
pub mod xml;

use serde::{Deserialize, Serialize};

pub use codes::{validate_code, CodeKind, CodeTable, ALL_CODE_TABLES, GBT_3304_ETHNICITY};
pub use jcbx::{Equipment, Schoolhouse};
pub use jcjg::{StaffBasic, StaffEducation, StaffPartyPost, StaffTitle};
pub use jctb::{CommContact, GeneralTeaching, GeneralTime, OrgBasic, PersonBasic};
pub use jcxs::{AwardInfo, PunishmentInfo, ScoreInfo, StudentBasic, StudentStatus};
pub use jcxx::{CampusBasic, ClassInfo, SchoolBasic};
pub use types::{
    is_valid_date8, is_valid_time6, DataType, EmgiError, EmgiField, EmgiRecord, EmgiRecordable,
    FieldDef, Obligation,
};

/// JY/T 1002-2012 标准标识。
pub const STANDARD_ID: &str = "JY/T 1002-2012";

/// 五大子集名称映射。
pub const SUBSET_NAMES: &[(&str, &str)] = &[
    ("JCTB", "通用基础信息子集"),
    ("JCXX", "学校管理信息子集"),
    ("JCXS", "学生管理信息子集"),
    ("JCJG", "教职工管理信息子集"),
    ("JCBX", "办学条件管理信息子集"),
];

/// 查询子集中文名称。
pub fn subset_name(id: &str) -> Option<&'static str> {
    SUBSET_NAMES.iter().find(|(s, _)| *s == id).map(|(_, n)| *n)
}

/// 合规数据集：聚合多个数据类的扁平记录，可序列化为 XML / JSON，并可持久化。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmgiDataset {
    /// 标准标识，固定为 [`STANDARD_ID`]。
    pub standard: String,
    /// 生成时间，格式 `YYYYMMDDHHMMSS`。
    pub generated_at: String,
    /// 扁平记录集合。
    pub records: Vec<EmgiRecord>,
}

impl Default for EmgiDataset {
    fn default() -> Self {
        let (d, t) = types::now_emgi_datetime();
        Self {
            standard: STANDARD_ID.to_string(),
            generated_at: format!("{d}{t}"),
            records: Vec::new(),
        }
    }
}

impl EmgiDataset {
    /// 创建空数据集（自动填入标准标识与当前生成时间）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一个数据类记录（任意 [`EmgiRecordable`] 实现）。
    pub fn with_record<R: EmgiRecordable>(&mut self, record: &R) -> &mut Self {
        self.records.push(record.to_record());
        self
    }

    /// 直接追加已扁平化的记录。
    pub fn push_record(&mut self, record: EmgiRecord) -> &mut Self {
        self.records.push(record);
        self
    }

    /// 校验全部记录，返回 `(数据类标识, 字段标识, 错误)` 列表；空表示全部合规。
    pub fn validate(&self) -> Vec<(String, String, EmgiError)> {
        let mut errs = Vec::new();
        for rec in &self.records {
            for (fid, e) in rec.validate() {
                errs.push((rec.class_id.clone(), fid, e));
            }
        }
        errs
    }

    /// 导出为符合标准的 XML 字符串。
    pub fn to_xml(&self) -> anyhow::Result<String> {
        xml::to_xml(self)
    }

    /// 导出为 JSON 字符串（供程序化处理与存储）。
    pub fn to_json(&self) -> anyhow::Result<String> {
        serde_json::to_string_pretty(self).map_err(Into::into)
    }

    /// 数据类数量。
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emgi::jctb::{CommContact, PersonBasic};

    #[test]
    fn test_person_basic_mandatory_validation() {
        let p = PersonBasic::default();
        let errs = p.validate().expect_err("空人员基本应不合规");
        // 5 个必备项：姓名/性别/出生日期/证件类型/证件号
        assert_eq!(errs.len(), 5);
        assert!(errs
            .iter()
            .all(|e| matches!(e, EmgiError::MissingMandatory { .. })));
    }

    #[test]
    fn test_person_basic_full_validation() {
        let p = PersonBasic {
            name: Some("张三".into()),
            gender: Some("1".into()),
            birth_date: Some("20080115".into()),
            id_type: Some("01".into()),
            id_number: Some("110108200801150012".into()),
            ethnicity: Some("01".into()),
            ..Default::default()
        };
        assert!(p.validate().is_ok(), "完整人员基本应合规");
    }

    #[test]
    fn test_code_table_rejects_bad_gender() {
        let p = PersonBasic {
            name: Some("李四".into()),
            gender: Some("7".into()),
            birth_date: Some("20080115".into()),
            id_type: Some("01".into()),
            id_number: Some("110108200801150012".into()),
            ethnicity: Some("01".into()),
            ..Default::default()
        };
        let errs = p.validate().expect_err("性别非法应报错");
        assert!(errs.iter().any(|e| matches!(
            e,
            EmgiError::InvalidCode { id, .. } if id == "JCTB020102"
        )));
    }

    #[test]
    fn test_student_take_from_person() {
        // 学生基本取用人员基本（JCTB0201），其字段须含 source 标注
        let s = StudentBasic {
            student_id: Some("G2011000001".into()),
            person: PersonBasic {
                name: Some("王五".into()),
                gender: Some("2".into()),
                birth_date: Some("20070909".into()),
                id_type: Some("01".into()),
                id_number: Some("440108200709090023".into()),
                ethnicity: Some("01".into()),
                nationality: Some("CHN".into()),
                political: Some("13".into()),
                health: Some("1".into()),
                marital: Some("1".into()),
                hmt: Some("0".into()),
            },
            student_source: Some("1".into()),
            blood: Some("O".into()),
            photo: None,
        };
        assert!(s.validate().is_ok());
        // 取用来源标注正确
        let rec = s.to_record();
        let name_field = rec.fields.iter().find(|f| f.id == "JCXS010102").unwrap();
        assert_eq!(name_field.source.as_deref(), Some("JCTB020101"));
        assert_eq!(s.references(), &["JCTB0201"]);
    }

    #[test]
    fn test_school_references_comm_format() {
        let school = SchoolBasic {
            school_id: Some("S1101080001".into()),
            school_name: Some("示范学校".into()),
            school_nature: Some("3".into()),
            contact: CommContact {
                email: Some("a@b.com".into()),
                postal_code: Some("100084".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(school.validate().is_ok());
        // 引用通用通讯格式
        assert_eq!(school.references(), &["JCTB0101"]);
        let rec = school.to_record();
        assert!(rec
            .fields
            .iter()
            .any(|f| f.id == "JCTB010101" && f.value.as_deref() == Some("a@b.com")));
    }

    #[test]
    fn test_dataset_xml_and_json_export() {
        let mut ds = EmgiDataset::new();
        ds.with_record(&SchoolBasic {
            school_id: Some("S1101080001".into()),
            school_name: Some("示范学校".into()),
            school_nature: Some("3".into()),
            ..Default::default()
        });
        ds.with_record(&StudentBasic {
            student_id: Some("G2011000001".into()),
            person: PersonBasic {
                name: Some("王五".into()),
                gender: Some("1".into()),
                birth_date: Some("20070909".into()),
                id_type: Some("01".into()),
                id_number: Some("440108200709090023".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(ds.validate().is_empty(), "应为空（全部合规）");

        let xml = ds.to_xml().expect("XML 导出失败");
        assert!(xml.contains("<EMGI standard=\"JY/T 1002-2012\""));
        assert!(xml.contains("JCXX0101"));
        assert!(xml.contains("JCXS0101"));

        let json = ds.to_json().expect("JSON 导出失败");
        assert!(json.contains("G2011000001") || json.contains("S1101080001"));
    }

    #[test]
    fn test_full_coverage_mandatory() {
        // 构造一个最小合规的跨子集数据集，验证必备项 100% 可覆盖
        use crate::emgi::jcbx::{Equipment, Schoolhouse};
        use crate::emgi::jcjg::{StaffBasic, StaffEducation, StaffTitle};
        use crate::emgi::jcxs::{AwardInfo, PunishmentInfo, ScoreInfo, StudentStatus};

        let mut ds = EmgiDataset::new();
        ds.with_record(&SchoolBasic {
            school_id: Some("S1".into()),
            school_name: Some("X".into()),
            school_nature: Some("3".into()),
            ..Default::default()
        });
        ds.with_record(&ClassInfo {
            class_id: Some("C1".into()),
            class_name: Some("一班".into()),
            class_type: Some("1".into()),
            school_id: Some("S1".into()),
            ..Default::default()
        });
        ds.with_record(&StudentBasic {
            student_id: Some("G1".into()),
            person: PersonBasic {
                name: Some("甲".into()),
                gender: Some("1".into()),
                birth_date: Some("20080101".into()),
                id_type: Some("01".into()),
                id_number: Some("11010820080101001X".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        ds.with_record(&StudentStatus {
            status_id: Some("ST1".into()),
            student_no: Some("2021001".into()),
            enroll_date: Some("20210901".into()),
            status_state: Some("1".into()),
            class_id: Some("C1".into()),
            student_id: Some("G1".into()),
            ..Default::default()
        });
        ds.with_record(&ScoreInfo {
            score_id: Some("SC1".into()),
            student_id: Some("G1".into()),
            subject_code: Some("02".into()),
            score: Some("95".into()),
            ..Default::default()
        });
        ds.with_record(&AwardInfo {
            award_id: Some("AW1".into()),
            student_id: Some("G1".into()),
            award_name: Some("三好学生".into()),
            ..Default::default()
        });
        ds.with_record(&PunishmentInfo {
            punish_id: Some("PU1".into()),
            student_id: Some("G1".into()),
            punish_name: Some("警告".into()),
            ..Default::default()
        });
        ds.with_record(&StaffBasic {
            staff_id: Some("F1".into()),
            person: PersonBasic {
                name: Some("乙".into()),
                gender: Some("2".into()),
                birth_date: Some("19800101".into()),
                id_type: Some("01".into()),
                id_number: Some("11010819800101002X".into()),
                ..Default::default()
            },
            staff_category: Some("1".into()),
            staff_state: Some("1".into()),
            ..Default::default()
        });
        ds.with_record(&StaffEducation {
            education_code: Some("30".into()),
            staff_id: Some("F1".into()),
            ..Default::default()
        });
        ds.with_record(&StaffTitle {
            title_code: Some("0102".into()),
            staff_id: Some("F1".into()),
            ..Default::default()
        });
        ds.with_record(&Schoolhouse {
            house_id: Some("H1".into()),
            house_name: Some("教学楼".into()),
            house_type: Some("1".into()),
            school_id: Some("S1".into()),
            ..Default::default()
        });
        ds.with_record(&Equipment {
            equip_id: Some("E1".into()),
            equip_name: Some("投影仪".into()),
            equip_type: Some("2".into()),
            school_id: Some("S1".into()),
            ..Default::default()
        });

        let errs = ds.validate();
        assert!(
            errs.is_empty(),
            "跨子集必备项应全部通过，错误: {:?}",
            errs.iter()
                .map(|(_, f, e)| (f, e.to_string()))
                .collect::<Vec<_>>()
        );
    }
}
