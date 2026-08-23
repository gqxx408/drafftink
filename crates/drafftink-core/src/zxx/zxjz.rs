//! ZXJZ — 教职工管理数据子集
//!
//! - ZXJZ01 教职工基本（取用 [`StaffBasic`] / JCJG01）
//! - ZXJZ02 资质
//! - ZXJZ03 岗位职务

use crate::emgi::types::{DataType, EmgiRecordable, FieldDef, Obligation};
use crate::emgi::StaffBasic;

pub const ZXJZ010101: FieldDef = FieldDef { id: "ZXJZ010101", name: "教职工标识码", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: Some("JCJG0101"), note: "取用 JCJG0101 教职工标识码" };

/// 教职工基本（ZXJZ01）——取用 JCJG01。
pub struct StaffProfile {
    pub staff: StaffBasic,
}

impl StaffProfile {
    /// 由 JY/T 1002 教职工基本结构体「取用」构造。
    pub fn from_staff(staff: StaffBasic) -> Self {
        Self { staff }
    }
}

impl EmgiRecordable for StaffProfile {
    const SUBSET: &'static str = "ZXJZ";
    const CLASS_ID: &'static str = "ZXJZ0101";
    const CLASS_NAME: &'static str = "教职工基本";
    fn references(&self) -> &'static [&'static str] {
        &["JCJG0101"]
    }
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        let mut v = self.staff.fields();
        v.push((&ZXJZ010101, self.staff.staff_id.clone()));
        v
    }
}

pub const ZXJZ020101: FieldDef = FieldDef { id: "ZXJZ020101", name: "资质标识", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: None, note: "" };
pub const ZXJZ020102: FieldDef = FieldDef { id: "ZXJZ020102", name: "教职工标识码", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: Some("JCJG0101"), note: "取用 JCJG0101 教职工标识码" };
pub const ZXJZ020103: FieldDef = FieldDef { id: "ZXJZ020103", name: "学校标识码", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: Some("JCXX0101"), note: "取用 JCXX0101 学校标识码" };
pub const ZXJZ020104: FieldDef = FieldDef { id: "ZXJZ020104", name: "资格类型", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: None, note: "" };
pub const ZXJZ020105: FieldDef = FieldDef { id: "ZXJZ020105", name: "证书编号", data_type: DataType::C, length: 30, obligation: Obligation::M, code_ref: None, source: None, note: "" };
pub const ZXJZ020106: FieldDef = FieldDef { id: "ZXJZ020106", name: "发证日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" };
pub const ZXJZ020107: FieldDef = FieldDef { id: "ZXJZ020107", name: "有效期至", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" };

/// 资质（ZXJZ02）。
pub struct StaffQualification {
    pub qual_id: String,
    pub staff_id: String,
    pub school_id: String,
    pub qual_type: String,
    pub cert_no: String,
    pub issue_date: String,
    pub valid_until: String,
}

impl EmgiRecordable for StaffQualification {
    const SUBSET: &'static str = "ZXJZ";
    const CLASS_ID: &'static str = "ZXJZ0201";
    const CLASS_NAME: &'static str = "资质";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXJZ020101, Some(self.qual_id.clone())),
            (&ZXJZ020102, Some(self.staff_id.clone())),
            (&ZXJZ020103, Some(self.school_id.clone())),
            (&ZXJZ020104, Some(self.qual_type.clone())),
            (&ZXJZ020105, Some(self.cert_no.clone())),
            (&ZXJZ020106, Some(self.issue_date.clone())),
            (&ZXJZ020107, Some(self.valid_until.clone())),
        ]
    }
}

pub const ZXJZ030101: FieldDef = FieldDef { id: "ZXJZ030101", name: "职务标识", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: None, note: "" };
pub const ZXJZ030102: FieldDef = FieldDef { id: "ZXJZ030102", name: "教职工标识码", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: Some("JCJG0101"), note: "取用 JCJG0101 教职工标识码" };
pub const ZXJZ030103: FieldDef = FieldDef { id: "ZXJZ030103", name: "学校标识码", data_type: DataType::C, length: 20, obligation: Obligation::M, code_ref: None, source: Some("JCXX0101"), note: "取用 JCXX0101 学校标识码" };
pub const ZXJZ030104: FieldDef = FieldDef { id: "ZXJZ030104", name: "岗位代码", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_POST_CODE"), source: None, note: "" };
pub const ZXJZ030105: FieldDef = FieldDef { id: "ZXJZ030105", name: "职务代码", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_DUTY_CODE"), source: None, note: "" };
pub const ZXJZ030106: FieldDef = FieldDef { id: "ZXJZ030106", name: "任职开始日期", data_type: DataType::D, length: 8, obligation: Obligation::M, code_ref: None, source: None, note: "YYYYMMDD" };
pub const ZXJZ030107: FieldDef = FieldDef { id: "ZXJZ030107", name: "任职结束日期", data_type: DataType::D, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "YYYYMMDD" };

/// 岗位职务（ZXJZ03）。
pub struct StaffPost {
    pub post_id: String,
    pub staff_id: String,
    pub school_id: String,
    pub post_code: String,
    pub duty_code: String,
    pub start_date: String,
    pub end_date: String,
}

impl EmgiRecordable for StaffPost {
    const SUBSET: &'static str = "ZXJZ";
    const CLASS_ID: &'static str = "ZXJZ0301";
    const CLASS_NAME: &'static str = "岗位职务";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXJZ030101, Some(self.post_id.clone())),
            (&ZXJZ030102, Some(self.staff_id.clone())),
            (&ZXJZ030103, Some(self.school_id.clone())),
            (&ZXJZ030104, Some(self.post_code.clone())),
            (&ZXJZ030105, Some(self.duty_code.clone())),
            (&ZXJZ030106, Some(self.start_date.clone())),
            (&ZXJZ030107, Some(self.end_date.clone())),
        ]
    }
}
