//! # ZXX — JY/T 1004-2012 普通中小学校管理信息
//!
//! 实现普通中小学校管理信息的 **6 个子集（首批）**：
//!
//! | 子集 | 名称 | 主要数据类 |
//! |------|------|-----------|
//! | ZXXX | 学校概况 | 学校基本 / 年级 / 班级 / 机构 / 达标 |
//! | ZXXS | 学生管理 | 学生基本 / 学籍 / 在校考试 / 毕结业 / 综合素质评价 |
//! | ZXDY | 德育管理 | 德育基本 / 关注数据 |
//! | ZXJX | 教学管理 | 课程 / 教材 / 教学计划 / 排课 |
//! | ZXTW | 体育卫生 | 学生体育运动 / 医疗保健 |
//! | ZXJZ | 教职工管理 | 教职工基本 / 资质 / 岗位职务 |
//! | ZXBG | 办公管理 | 公文 / 通知公告 / 日程安排 |
//!
//! ## 「取用」机制
//!
//! 凡与学生 / 教职工 / 学校基础信息相同的字段，直接复用 [`crate::emgi`] 中的对应结构体
//! （如 [`StudentBasic`]、[`StaffBasic`]、[`SchoolBasic`]、[`ClassInfo`]、[`OrgBasic`]），
//! 通过 `as_fields()` 复用其数据元素定义（保持原 JY/T 1002 标识符），**不重复定义**；
//! 本子集仅补充 JY/T 1004 特有的数据元素，并在 [`EmgiRecordable::references`] 中声明来源。
//!
//! [`StudentBasic`]: crate::emgi::StudentBasic
//! [`StaffBasic`]: crate::emgi::StaffBasic
//! [`SchoolBasic`]: crate::emgi::SchoolBasic
//! [`ClassInfo`]: crate::emgi::ClassInfo
//! [`OrgBasic`]: crate::emgi::OrgBasic

pub mod integration;
pub mod xml;
pub mod zxbg;
pub mod zxdy;
pub mod zxjx;
pub mod zxjz;
pub mod zxtw;
pub mod zxxs;
pub mod zxxx;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::emgi::types::{EmgiError, EmgiRecord, EmgiRecordable};

/// 各子集标识与中文名称。
pub const SUBSET_NAMES: &[(&str, &str)] = &[
    ("ZXXX", "学校概况数据子集"),
    ("ZXXS", "学生管理数据子集"),
    ("ZXDY", "德育管理数据子集"),
    ("ZXJX", "教学管理数据子集"),
    ("ZXTW", "体育卫生数据子集"),
    ("ZXJZ", "教职工管理数据子集"),
    ("ZXBG", "办公管理数据子集"),
];

/// JY/T 1004-2012 数据集——可序列化、可导出 XML/JSON。
///
/// 结构与 [`crate::emgi::EmgiDataset`] 对齐（复用 [`EmgiRecord`]/`EmgiField`]），
/// 标准标识固定为 `JY/T 1004-2012`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ZxxDataset {
    /// 标准标识（固定为 JY/T 1004-2012）。
    pub standard: String,
    /// 生成时间戳（RFC3339）。
    pub generated_at: String,
    /// 全部记录（扁平化后的 [`EmgiRecord`]）。
    pub records: Vec<EmgiRecord>,
    /// 子集清单（标识 → 名称）。
    pub subsets: BTreeMap<String, String>,
}

impl ZxxDataset {
    /// 创建空数据集，自动填充标准标识、生成时间与子集清单。
    pub fn new() -> Self {
        Self {
            standard: "JY/T 1004-2012".to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            records: Vec::new(),
            subsets: SUBSET_NAMES
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    /// 由已有记录集合构建。
    pub fn from_records(records: Vec<EmgiRecord>) -> Self {
        let mut ds = Self::new();
        ds.records = records;
        ds
    }

    /// 追加一条扁平记录。
    pub fn add_record(&mut self, record: EmgiRecord) {
        self.records.push(record);
    }

    /// 追加一条可记录对象（自动扁平化为 [`EmgiRecord`]）。
    pub fn add<T: EmgiRecordable>(&mut self, item: &T) {
        self.records.push(item.to_record());
    }

    /// 记录条数。
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// 批量校验全部记录，返回 `(记录索引, 字段标识, 错误)` 列表；空表示全部通过。
    pub fn validate(&self) -> Vec<(usize, String, EmgiError)> {
        let mut errs = Vec::new();
        for (i, rec) in self.records.iter().enumerate() {
            for (fid, e) in rec.validate() {
                errs.push((i, fid, e));
            }
        }
        errs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emgi::types::DataType;
    use crate::emgi::{PersonBasic, StudentBasic};
    use crate::zxx::zxjx::Course;
    use crate::zxx::zxxs::{ExamRecord, StudentProfile};

    fn sample_student() -> StudentBasic {
        StudentBasic {
            student_id: Some("2023010001".to_string()),
            person: PersonBasic {
                name: Some("张三".to_string()),
                gender: Some("1".to_string()),
                birth_date: Some("20080115".to_string()),
                id_type: Some("01".to_string()),
                id_number: Some("11010820080115001X".to_string()),
                ethnicity: Some("01".to_string()),
                nationality: Some("CHN".to_string()),
                political: Some("01".to_string()),
                marital: Some("0".to_string()),
                hmt: Some("0".to_string()),
                health: Some("1".to_string()),
            },
            student_source: Some("1".to_string()),
            blood: None,
            photo: None,
        }
    }

    #[test]
    fn test_zxx_dataset_build_and_xml() {
        let mut ds = ZxxDataset::new();
        let profile = StudentProfile {
            student: sample_student(),
            student_no: "2023010001".to_string(),
        };
        ds.add(&profile);

        let course = Course {
            course_id: "C001".to_string(),
            course_code: "MATH01".to_string(),
            course_name: "数学".to_string(),
            course_type: "1".to_string(),
            textbook_code: "TB001".to_string(),
            textbook_name: "数学七年级上册".to_string(),
            subject: "01".to_string(),
            credit: "4".to_string(),
            hours: "72".to_string(),
            school_id: "S001".to_string(),
        };
        ds.add(&course);

        assert_eq!(ds.record_count(), 2);
        assert!(
            ds.validate().is_empty(),
            "校验应全部通过: {:?}",
            ds.validate()
        );

        let xml = crate::zxx::xml::to_xml(&ds);
        assert!(xml.contains("<ZXX standard=\"JY/T 1004-2012\""));
        assert!(xml.contains("ZXXS0101"));
        assert!(xml.contains("ZXJX0101"));
    }

    #[test]
    fn test_take_reference_no_redefinition() {
        // ZXXS01 取用 JCXS01 —— 字段中应包含 JCXS0101 标识符（来自 emgi），而非重复定义。
        let profile = StudentProfile {
            student: sample_student(),
            student_no: "2023010001".to_string(),
        };
        let ids: Vec<&str> = profile.fields().iter().map(|(d, _)| d.id).collect();
        assert!(ids.iter().any(|id| id.starts_with("JCXS0101")));
        assert!(profile.references().contains(&"JCXS0101"));
    }

    #[test]
    fn test_exam_record_links_course() {
        let exam = ExamRecord {
            exam_id: "E001".to_string(),
            student_id: "2023010001".to_string(),
            school_id: "S001".to_string(),
            course_id: "MATH01".to_string(),
            academic_year: "2023".to_string(),
            term: "1".to_string(),
            exam_method: "2".to_string(),
            exam_date: "20240110".to_string(),
            score: "88.5".to_string(),
            score_type: "1".to_string(),
        };
        assert!(exam.validate().is_ok());
        let f = exam.fields();
        // 课程号引用 ZXJX01 课程类
        let course_field = f.iter().find(|(d, _)| d.id == "ZXXS020604").unwrap();
        assert_eq!(course_field.0.source, Some("ZXJX0101"));
    }

    #[test]
    fn test_data_type_constants_used() {
        // 防御性：确保枚举型数据元素携带代码表引用
        assert_eq!(DataType::C.as_code(), "C");
    }
}
