//! # JCBX 办学条件管理信息数据子集
//!
//! 实现 JY/T 1002-2012 表 5 办学条件子集的两个数据类：
//!
//! | 数据类 | 标识符 | 说明 |
//! |--------|--------|------|
//! | 校舍场所 | `JCBX0201` | 校舍标识/名称/面积，**引用** `JCXX0101` 学校 |
//! | 仪器设备 | `JCBX0202` | 设备标识/分类/数量，**引用** `JCXX0101` 学校 |

use serde::{Deserialize, Serialize};

use super::types::{DataType, EmgiRecordable, FieldDef, Obligation};

// ════════════════════════════════════════════════════════════════════════════
//  JCBX0201 校舍场所数据类
// ════════════════════════════════════════════════════════════════════════════

const JCBX0201_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCBX020101", name: "校舍场所标识", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCBX020102", name: "校舍名称", data_type: DataType::C, length: 60, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCBX020103", name: "校舍类型代码", data_type: DataType::C, length: 1, obligation: Obligation::M, code_ref: Some("JYT_1001_HOUSE_TYPE"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCBX020104", name: "建筑面积", data_type: DataType::N, length: 12, obligation: Obligation::O, code_ref: None, source: None, note: "平方米" },
    FieldDef { id: "JCBX020105", name: "占地面积", data_type: DataType::N, length: 12, obligation: Obligation::O, code_ref: None, source: None, note: "平方米" },
    FieldDef { id: "JCBX020106", name: "学校标识码", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "引用 JCXX0101" },
];

/// 校舍场所数据结构（JCBX0201）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Schoolhouse {
    pub house_id: Option<String>,
    pub house_name: Option<String>,
    pub house_type: Option<String>,
    pub build_area: Option<String>,
    pub land_area: Option<String>,
    pub school_id: Option<String>,
}

impl EmgiRecordable for Schoolhouse {
    const SUBSET: &'static str = "JCBX";
    const CLASS_ID: &'static str = "JCBX0201";
    const CLASS_NAME: &'static str = "校舍场所";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCBX0201_FIELDS[0], self.house_id.clone()),
            (&JCBX0201_FIELDS[1], self.house_name.clone()),
            (&JCBX0201_FIELDS[2], self.house_type.clone()),
            (&JCBX0201_FIELDS[3], self.build_area.clone()),
            (&JCBX0201_FIELDS[4], self.land_area.clone()),
            (&JCBX0201_FIELDS[5], self.school_id.clone()),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCXX0101"]
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  JCBX0202 仪器设备数据类
// ════════════════════════════════════════════════════════════════════════════

const JCBX0202_FIELDS: &[FieldDef] = &[
    FieldDef { id: "JCBX020201", name: "仪器设备标识", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCBX020202", name: "仪器设备名称", data_type: DataType::C, length: 60, obligation: Obligation::M, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCBX020203", name: "仪器设备分类代码", data_type: DataType::C, length: 2, obligation: Obligation::M, code_ref: Some("JYT_1001_EQUIP_TYPE"), source: None, note: "JY/T 1001" },
    FieldDef { id: "JCBX020204", name: "数量", data_type: DataType::N, length: 8, obligation: Obligation::O, code_ref: None, source: None, note: "" },
    FieldDef { id: "JCBX020205", name: "单价", data_type: DataType::N, length: 10, obligation: Obligation::O, code_ref: None, source: None, note: "元" },
    FieldDef { id: "JCBX020206", name: "学校标识码", data_type: DataType::C, length: 19, obligation: Obligation::M, code_ref: None, source: None, note: "引用 JCXX0101" },
];

/// 仪器设备数据结构（JCBX0202）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Equipment {
    pub equip_id: Option<String>,
    pub equip_name: Option<String>,
    pub equip_type: Option<String>,
    pub quantity: Option<String>,
    pub unit_price: Option<String>,
    pub school_id: Option<String>,
}

impl EmgiRecordable for Equipment {
    const SUBSET: &'static str = "JCBX";
    const CLASS_ID: &'static str = "JCBX0202";
    const CLASS_NAME: &'static str = "仪器设备";

    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)> {
        vec![
            (&JCBX0202_FIELDS[0], self.equip_id.clone()),
            (&JCBX0202_FIELDS[1], self.equip_name.clone()),
            (&JCBX0202_FIELDS[2], self.equip_type.clone()),
            (&JCBX0202_FIELDS[3], self.quantity.clone()),
            (&JCBX0202_FIELDS[4], self.unit_price.clone()),
            (&JCBX0202_FIELDS[5], self.school_id.clone()),
        ]
    }

    fn references(&self) -> &'static [&'static str] {
        &["JCXX0101"]
    }
}
