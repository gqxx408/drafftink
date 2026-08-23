//! ZXBG — 办公管理数据子集（JY/T 1004-2012）
//!
//! 新增的第七个数据子集，补齐办公管理类数据元素，完全对标 JY/T 1004：
//!
//! | 数据类 | 标识符 | 说明 |
//! |--------|--------|------|
//! | 公文数据 | `ZXBG0101` | 公文编号/标题/类型码/发文日期/发文部门/紧急程度码/密级/审批状态 |
//! | 通知公告 | `ZXBG0201` | 通知编号/标题/发布日期/发布人(取用JCJG01)/接收范围 |
//! | 日程安排 | `ZXBG0301` | 日程编号/内容/开始时间/结束时间/参与人/地点 |
//!
//! 所有字段均标注必备(M)级别，复用了 emgi 的取用字段：
//! - `ZXBG010105` 发文部门 → 取用 `JCXX010102`（学校名称）
//! - `ZXBG020104` 发布人   → 取用 `JCJG010102`（教职工姓名）

use crate::emgi::types::{DataType, EmgiRecordable, FieldDef, Obligation};
use serde::{Deserialize, Serialize};

// ════════════════════════════════════════════════════════════════════════════
//  ZXBG0101 公文数据类
// ════════════════════════════════════════════════════════════════════════════

/// 公文编号
pub const ZXBG010101: FieldDef = FieldDef { id: "ZXBG010101", name: "公文编号", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: None, note: "" };
/// 公文标题
pub const ZXBG010102: FieldDef = FieldDef { id: "ZXBG010102", name: "公文标题", data_type: DataType::C, length: 200, obligation: Obligation::M, code_ref: None, source: None, note: "" };
/// 公文类型码（取用 JY/T 1001 公文类型）
pub const ZXBG010103: FieldDef = FieldDef { id: "ZXBG010103", name: "公文类型码", data_type: DataType::C, length: 2, obligation: Obligation::M, code_ref: Some("JYT_1001_DOC_TYPE"), source: None, note: "JY/T 1001" };
/// 发文日期（YYYYMMDD）
pub const ZXBG010104: FieldDef = FieldDef { id: "ZXBG010104", name: "发文日期", data_type: DataType::D, length: 8, obligation: Obligation::M, code_ref: None, source: None, note: "YYYYMMDD" };
/// 发文部门（取用 JCXX0101 学校名称）
pub const ZXBG010105: FieldDef = FieldDef { id: "ZXBG010105", name: "发文部门", data_type: DataType::C, length: 60, obligation: Obligation::M, code_ref: None, source: Some("JCXX010102"), note: "取用 JCXX010102 学校名称" };
/// 紧急程度码（取用 JY/T 1001 紧急程度）
pub const ZXBG010106: FieldDef = FieldDef { id: "ZXBG010106", name: "紧急程度码", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_URGENCY"), source: None, note: "JY/T 1001" };
/// 密级（取用 GB/T 7156 密级）
pub const ZXBG010107: FieldDef = FieldDef { id: "ZXBG010107", name: "密级", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_SECRET_LEVEL"), source: None, note: "GB/T 7156" };
/// 审批状态（取用 JY/T 1004 审批状态）
pub const ZXBG010108: FieldDef = FieldDef { id: "ZXBG010108", name: "审批状态", data_type: DataType::C, length: 2, obligation: Obligation::M, code_ref: Some("JYT_1001_APPROVAL_STATUS"), source: None, note: "JY/T 1004" };

/// 公文数据结构（ZXBG0101）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OfficialDoc {
    /// 公文编号
    pub doc_id: String,
    /// 公文标题
    pub title: String,
    /// 公文类型码
    pub doc_type: String,
    /// 发文日期 YYYYMMDD
    pub issue_date: String,
    /// 发文部门
    pub issue_dept: String,
    /// 紧急程度码
    pub urgency: String,
    /// 密级
    pub secret_level: String,
    /// 审批状态
    pub approval_status: String,
}

impl EmgiRecordable for OfficialDoc {
    const SUBSET: &'static str = "ZXBG";
    const CLASS_ID: &'static str = "ZXBG0101";
    const CLASS_NAME: &'static str = "公文数据";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXBG010101, Some(self.doc_id.clone())),
            (&ZXBG010102, Some(self.title.clone())),
            (&ZXBG010103, Some(self.doc_type.clone())),
            (&ZXBG010104, Some(self.issue_date.clone())),
            (&ZXBG010105, Some(self.issue_dept.clone())),
            (&ZXBG010106, Some(self.urgency.clone())),
            (&ZXBG010107, Some(self.secret_level.clone())),
            (&ZXBG010108, Some(self.approval_status.clone())),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCXX0101"]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  ZXBG0201 通知公告数据类
// ════════════════════════════════════════════════════════════════════════════

/// 通知编号
pub const ZXBG020101: FieldDef = FieldDef { id: "ZXBG020101", name: "通知编号", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: None, note: "" };
/// 通知标题
pub const ZXBG020102: FieldDef = FieldDef { id: "ZXBG020102", name: "通知标题", data_type: DataType::C, length: 200, obligation: Obligation::M, code_ref: None, source: None, note: "" };
/// 发布日期（YYYYMMDD）
pub const ZXBG020103: FieldDef = FieldDef { id: "ZXBG020103", name: "发布日期", data_type: DataType::D, length: 8, obligation: Obligation::M, code_ref: None, source: None, note: "YYYYMMDD" };
/// 发布人（取用 JCJG0101 教职工姓名）
pub const ZXBG020104: FieldDef = FieldDef { id: "ZXBG020104", name: "发布人", data_type: DataType::C, length: 50, obligation: Obligation::M, code_ref: None, source: Some("JCJG010102"), note: "取用 JCJG010102 姓名" };
/// 接收范围
pub const ZXBG020105: FieldDef = FieldDef { id: "ZXBG020105", name: "接收范围", data_type: DataType::C, length: 200, obligation: Obligation::M, code_ref: None, source: None, note: "如：全体教职工 / 三年级组" };

/// 通知公告数据结构（ZXBG0201）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Announcement {
    /// 通知编号
    pub notice_id: String,
    /// 通知标题
    pub title: String,
    /// 发布日期 YYYYMMDD
    pub publish_date: String,
    /// 发布人
    pub publisher: String,
    /// 接收范围
    pub recv_scope: String,
}

impl EmgiRecordable for Announcement {
    const SUBSET: &'static str = "ZXBG";
    const CLASS_ID: &'static str = "ZXBG0201";
    const CLASS_NAME: &'static str = "通知公告";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXBG020101, Some(self.notice_id.clone())),
            (&ZXBG020102, Some(self.title.clone())),
            (&ZXBG020103, Some(self.publish_date.clone())),
            (&ZXBG020104, Some(self.publisher.clone())),
            (&ZXBG020105, Some(self.recv_scope.clone())),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCJG0101"]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  ZXBG0301 日程安排数据类
// ════════════════════════════════════════════════════════════════════════════

/// 日程编号
pub const ZXBG030101: FieldDef = FieldDef { id: "ZXBG030101", name: "日程编号", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: None, note: "" };
/// 日程内容
pub const ZXBG030102: FieldDef = FieldDef { id: "ZXBG030102", name: "日程内容", data_type: DataType::C, length: 500, obligation: Obligation::M, code_ref: None, source: None, note: "" };
/// 开始时间（YYYYMMDDhhmmss）
pub const ZXBG030103: FieldDef = FieldDef { id: "ZXBG030103", name: "开始时间", data_type: DataType::C, length: 14, obligation: Obligation::M, code_ref: None, source: None, note: "YYYYMMDDhhmmss" };
/// 结束时间（YYYYMMDDhhmmss）
pub const ZXBG030104: FieldDef = FieldDef { id: "ZXBG030104", name: "结束时间", data_type: DataType::C, length: 14, obligation: Obligation::M, code_ref: None, source: None, note: "YYYYMMDDhhmmss" };
/// 参与人
pub const ZXBG030105: FieldDef = FieldDef { id: "ZXBG030105", name: "参与人", data_type: DataType::C, length: 200, obligation: Obligation::M, code_ref: None, source: None, note: "逗号分隔的姓名/工号" };
/// 地点
pub const ZXBG030106: FieldDef = FieldDef { id: "ZXBG030106", name: "地点", data_type: DataType::C, length: 60, obligation: Obligation::M, code_ref: None, source: None, note: "" };

/// 日程安排数据结构（ZXBG0301）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Schedule {
    /// 日程编号
    pub sched_id: String,
    /// 日程内容
    pub content: String,
    /// 开始时间 YYYYMMDDhhmmss
    pub start_time: String,
    /// 结束时间 YYYYMMDDhhmmss
    pub end_time: String,
    /// 参与人
    pub participants: String,
    /// 地点
    pub location: String,
}

impl EmgiRecordable for Schedule {
    const SUBSET: &'static str = "ZXBG";
    const CLASS_ID: &'static str = "ZXBG0301";
    const CLASS_NAME: &'static str = "日程安排";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXBG030101, Some(self.sched_id.clone())),
            (&ZXBG030102, Some(self.content.clone())),
            (&ZXBG030103, Some(self.start_time.clone())),
            (&ZXBG030104, Some(self.end_time.clone())),
            (&ZXBG030105, Some(self.participants.clone())),
            (&ZXBG030106, Some(self.location.clone())),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &[]
    }
}

/// ZXBG 子集全部数据类的必备(M)数据元素总数（用于合规自检）。
pub const MANDATORY_COUNT: usize = 8 + 5 + 6;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_doc() -> OfficialDoc {
        OfficialDoc {
            doc_id: "ZW20260001".into(),
            title: "关于2026年春季开学工作的通知".into(),
            doc_type: "80".into(), // 通知
            issue_date: "20260210".into(),
            issue_dept: "教务处".into(),
            urgency: "2".into(), // 加急
            secret_level: "0".into(), // 非涉密
            approval_status: "20".into(), // 审批通过
        }
    }

    fn sample_notice() -> Announcement {
        Announcement {
            notice_id: "NT20260001".into(),
            title: "全体教职工大会".into(),
            publish_date: "20260212".into(),
            publisher: "李校长".into(),
            recv_scope: "全体教职工".into(),
        }
    }

    fn sample_schedule() -> Schedule {
        Schedule {
            sched_id: "SC20260001".into(),
            content: "初三一模考务会".into(),
            start_time: "20260301090000".into(),
            end_time: "20260301100000".into(),
            participants: "王老师,李老师".into(),
            location: "行政楼301".into(),
        }
    }

    #[test]
    fn test_zxbg01_mandatory_100pct() {
        let doc = sample_doc();
        assert!(doc.validate().is_ok(), "公文数据校验应全过: {:?}", doc.validate());
        let ids: Vec<&str> = doc.fields().iter().map(|(d, _)| d.id).collect();
        assert_eq!(ids.len(), 8, "ZXBG01 必备数据元素应全覆盖");
        // 全部为必备(M)
        assert!(doc.fields().iter().all(|(d, _)| d.obligation == Obligation::M));
        // 代码表引用正确
        let type_field = doc.fields().into_iter().find(|(d, _)| d.id == "ZXBG010103").unwrap();
        assert_eq!(type_field.0.code_ref, Some("JYT_1001_DOC_TYPE"));
        // 取用来源正确
        let dept = doc.fields().into_iter().find(|(d, _)| d.id == "ZXBG010105").unwrap();
        assert_eq!(dept.0.source, Some("JCXX010102"));
    }

    #[test]
    fn test_zxbg02_mandatory_100pct() {
        let n = sample_notice();
        assert!(n.validate().is_ok(), "通知公告校验应全过: {:?}", n.validate());
        assert_eq!(n.fields().len(), 5);
        assert!(n.fields().iter().all(|(d, _)| d.obligation == Obligation::M));
        let pubr = n.fields().into_iter().find(|(d, _)| d.id == "ZXBG020104").unwrap();
        assert_eq!(pubr.0.source, Some("JCJG010102"));
    }

    #[test]
    fn test_zxbg03_mandatory_100pct() {
        let s = sample_schedule();
        assert!(s.validate().is_ok(), "日程安排校验应全过: {:?}", s.validate());
        assert_eq!(s.fields().len(), 6);
        assert!(s.fields().iter().all(|(d, _)| d.obligation == Obligation::M));
    }

    #[test]
    fn test_invalid_code_rejected() {
        let mut doc = sample_doc();
        doc.doc_type = "00".into(); // 不在 JY/T 1001 公文类型代码表中
        assert!(doc.validate().is_err());
        let mut doc2 = sample_doc();
        doc2.issue_date = "2026-02-10".into(); // 非法日期格式
        assert!(doc2.validate().is_err());
    }
}
