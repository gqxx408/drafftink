//! ZXDY — 德育管理数据子集
//!
//! - ZXDY01 德育基本
//! - ZXDY02 关注数据

use crate::emgi::types::{DataType, EmgiRecordable, FieldDef, Obligation};

pub const ZXDY010101: FieldDef = FieldDef {
    id: "ZXDY010101",
    name: "德育标识",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXDY010102: FieldDef = FieldDef {
    id: "ZXDY010102",
    name: "学生标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXS0101"),
    note: "取用 JCXS0101 学生标识码",
};
pub const ZXDY010103: FieldDef = FieldDef {
    id: "ZXDY010103",
    name: "学校标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXX0101"),
    note: "取用 JCXX0101 学校标识码",
};
pub const ZXDY010104: FieldDef = FieldDef {
    id: "ZXDY010104",
    name: "德育类型",
    data_type: DataType::C,
    length: 1,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_DEED_TYPE"),
    source: None,
    note: "",
};
pub const ZXDY010105: FieldDef = FieldDef {
    id: "ZXDY010105",
    name: "德育日期",
    data_type: DataType::D,
    length: 8,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "YYYYMMDD",
};
pub const ZXDY010106: FieldDef = FieldDef {
    id: "ZXDY010106",
    name: "德育内容",
    data_type: DataType::C,
    length: 500,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};

/// 德育基本（ZXDY01）。
pub struct Deed {
    pub deed_id: String,
    pub student_id: String,
    pub school_id: String,
    pub deed_type: String,
    pub deed_date: String,
    pub content: String,
}

impl EmgiRecordable for Deed {
    const SUBSET: &'static str = "ZXDY";
    const CLASS_ID: &'static str = "ZXDY0101";
    const CLASS_NAME: &'static str = "德育基本";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXDY010101, Some(self.deed_id.clone())),
            (&ZXDY010102, Some(self.student_id.clone())),
            (&ZXDY010103, Some(self.school_id.clone())),
            (&ZXDY010104, Some(self.deed_type.clone())),
            (&ZXDY010105, Some(self.deed_date.clone())),
            (&ZXDY010106, Some(self.content.clone())),
        ]
    }
}

pub const ZXDY020101: FieldDef = FieldDef {
    id: "ZXDY020101",
    name: "关注标识",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXDY020102: FieldDef = FieldDef {
    id: "ZXDY020102",
    name: "学生标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXS0101"),
    note: "取用 JCXS0101 学生标识码",
};
pub const ZXDY020103: FieldDef = FieldDef {
    id: "ZXDY020103",
    name: "关注类型",
    data_type: DataType::C,
    length: 2,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXDY020104: FieldDef = FieldDef {
    id: "ZXDY020104",
    name: "关注日期",
    data_type: DataType::D,
    length: 8,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "YYYYMMDD",
};
pub const ZXDY020105: FieldDef = FieldDef {
    id: "ZXDY020105",
    name: "关注原因",
    data_type: DataType::C,
    length: 500,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};

/// 关注数据（ZXDY02）。
pub struct Attention {
    pub attn_id: String,
    pub student_id: String,
    pub attn_type: String,
    pub attn_date: String,
    pub reason: String,
}

impl EmgiRecordable for Attention {
    const SUBSET: &'static str = "ZXDY";
    const CLASS_ID: &'static str = "ZXDY0201";
    const CLASS_NAME: &'static str = "关注数据";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXDY020101, Some(self.attn_id.clone())),
            (&ZXDY020102, Some(self.student_id.clone())),
            (&ZXDY020103, Some(self.attn_type.clone())),
            (&ZXDY020104, Some(self.attn_date.clone())),
            (&ZXDY020105, Some(self.reason.clone())),
        ]
    }
}
