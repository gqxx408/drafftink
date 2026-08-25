//! ZXXX — 学校概况数据子集
//!
//! - ZXXX01 学校基本（取用 [`SchoolBasic`] / JCXX01）
//! - ZXXX02 年级
//! - ZXXX03 班级（取用 [`ClassInfo`] / JCXX02）
//! - ZXXX04 机构（取用 [`OrgBasic`] / JCTB0103）
//! - ZXXX05 达标

use crate::emgi::types::{DataType, EmgiRecordable, FieldDef, Obligation};
use crate::emgi::{ClassInfo, OrgBasic, SchoolBasic};

// ── ZXXX01 学校基本（取用 JCXX01） ──────────────────────────────
pub const ZXXX0103: FieldDef = FieldDef {
    id: "ZXXX0103",
    name: "学校办别代码",
    data_type: DataType::C,
    length: 2,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_SCHOOL_RUN_TYPE"),
    source: None,
    note: "取自 JY/T 1001 学校办别代码",
};
pub const ZXXX0104: FieldDef = FieldDef {
    id: "ZXXX0104",
    name: "主管部门",
    data_type: DataType::C,
    length: 80,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};

/// 学校基本（ZXXX01）——取用 JCXX01，补充办别与主管部门。
pub struct SchoolProfile {
    pub school: SchoolBasic,
    pub run_type: String,
    pub authority: String,
}

impl EmgiRecordable for SchoolProfile {
    const SUBSET: &'static str = "ZXXX";
    const CLASS_ID: &'static str = "ZXXX0101";
    const CLASS_NAME: &'static str = "学校基本";
    fn references(&self) -> &'static [&'static str] {
        &["JCXX0101"]
    }
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        let mut v = self.school.fields();
        v.push((&ZXXX0103, Some(self.run_type.clone())));
        v.push((&ZXXX0104, Some(self.authority.clone())));
        v
    }
}

// ── ZXXX02 年级 ──────────────────────────────────────────────────
pub const ZXXX020101: FieldDef = FieldDef {
    id: "ZXXX020101",
    name: "年级标识",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXXX020102: FieldDef = FieldDef {
    id: "ZXXX020102",
    name: "学校标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXX0101"),
    note: "取用 JCXX0101 学校标识码",
};
pub const ZXXX020103: FieldDef = FieldDef {
    id: "ZXXX020103",
    name: "年级代码",
    data_type: DataType::C,
    length: 2,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_GRADE"),
    source: None,
    note: "",
};
pub const ZXXX020104: FieldDef = FieldDef {
    id: "ZXXX020104",
    name: "年级名称",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXXX020105: FieldDef = FieldDef {
    id: "ZXXX020105",
    name: "入学年份",
    data_type: DataType::N,
    length: 4,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "",
};

/// 年级（ZXXX02）。
pub struct Grade {
    pub grade_id: String,
    pub school_id: String,
    pub grade_code: String,
    pub grade_name: String,
    pub enroll_year: String,
}

impl EmgiRecordable for Grade {
    const SUBSET: &'static str = "ZXXX";
    const CLASS_ID: &'static str = "ZXXX0201";
    const CLASS_NAME: &'static str = "年级";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXXX020101, Some(self.grade_id.clone())),
            (&ZXXX020102, Some(self.school_id.clone())),
            (&ZXXX020103, Some(self.grade_code.clone())),
            (&ZXXX020104, Some(self.grade_name.clone())),
            (&ZXXX020105, Some(self.enroll_year.clone())),
        ]
    }
}

// ── ZXXX03 班级（取用 JCXX02） ──────────────────────────────────
pub const ZXXX030101: FieldDef = FieldDef {
    id: "ZXXX030101",
    name: "班级标识",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};

/// 班级（ZXXX03）——取用 JCXX02。
pub struct ClassProfile {
    pub class_id: String,
    pub class: ClassInfo,
}

impl EmgiRecordable for ClassProfile {
    const SUBSET: &'static str = "ZXXX";
    const CLASS_ID: &'static str = "ZXXX0301";
    const CLASS_NAME: &'static str = "班级";
    fn references(&self) -> &'static [&'static str] {
        &["JCXX0201"]
    }
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        let mut v = self.class.fields();
        v.push((&ZXXX030101, Some(self.class_id.clone())));
        v
    }
}

// ── ZXXX04 机构（取用 JCTB0103） ───────────────────────────────
pub const ZXXX040101: FieldDef = FieldDef {
    id: "ZXXX040101",
    name: "机构标识",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};

/// 机构（ZXXX04）——取用 JCTB0103。
pub struct Organization {
    pub org_id: String,
    pub org: OrgBasic,
}

impl EmgiRecordable for Organization {
    const SUBSET: &'static str = "ZXXX";
    const CLASS_ID: &'static str = "ZXXX0401";
    const CLASS_NAME: &'static str = "机构";
    fn references(&self) -> &'static [&'static str] {
        &["JCTB0103"]
    }
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        let mut v = self.org.fields();
        v.push((&ZXXX040101, Some(self.org_id.clone())));
        v
    }
}

// ── ZXXX05 达标 ─────────────────────────────────────────────────
pub const ZXXX050101: FieldDef = FieldDef {
    id: "ZXXX050101",
    name: "学校标识码",
    data_type: DataType::C,
    length: 20,
    obligation: Obligation::M,
    code_ref: None,
    source: Some("JCXX0101"),
    note: "取用 JCXX0101 学校标识码",
};
pub const ZXXX050102: FieldDef = FieldDef {
    id: "ZXXX050102",
    name: "达标项目代码",
    data_type: DataType::C,
    length: 10,
    obligation: Obligation::M,
    code_ref: None,
    source: None,
    note: "",
};
pub const ZXXX050103: FieldDef = FieldDef {
    id: "ZXXX050103",
    name: "是否达标",
    data_type: DataType::L,
    length: 1,
    obligation: Obligation::M,
    code_ref: Some("JYT_1001_YES_NO"),
    source: None,
    note: "",
};
pub const ZXXX050104: FieldDef = FieldDef {
    id: "ZXXX050104",
    name: "达标日期",
    data_type: DataType::D,
    length: 8,
    obligation: Obligation::O,
    code_ref: None,
    source: None,
    note: "YYYYMMDD",
};

/// 学校达标情况（ZXXX05）。
pub struct SchoolStandard {
    pub school_id: String,
    pub standard_code: String,
    pub reached: String,
    pub reached_date: String,
}

impl EmgiRecordable for SchoolStandard {
    const SUBSET: &'static str = "ZXXX";
    const CLASS_ID: &'static str = "ZXXX0501";
    const CLASS_NAME: &'static str = "达标";
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&ZXXX050101, Some(self.school_id.clone())),
            (&ZXXX050102, Some(self.standard_code.clone())),
            (&ZXXX050103, Some(self.reached.clone())),
            (&ZXXX050104, Some(self.reached_date.clone())),
        ]
    }
}
