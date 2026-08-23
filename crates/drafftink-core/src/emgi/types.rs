//! # emgi 核心类型系统
//!
//! 定义 JY/T 1002-2012《教育管理信息 教育管理基础信息》的数据元素抽象：
//!
//! - [`Obligation`]：约束级别（必备 M / 可选 O / 条件必选 C）
//! - [`DataType`]：数据类型（字符 C / 数值 N / 日期 D / 时间 T / 逻辑 L / 二进制 B）
//! - [`FieldDef`]：单个数据元素定义（标准表中的一个数据项）
//! - [`EmgiRecordable`]：数据类 trait，提供取值、校验、溯源能力
//! - [`EmgiRecord`]/[`EmgiField`]：可序列化、可导出 XML/JSON 的扁平记录
//!
//! 日期统一为 `YYYYMMDD`，时间统一为 `hhmmss`（见 [`is_valid_date8`]/[`is_valid_time6`]）。

use std::fmt;

use serde::{Deserialize, Serialize};

use super::codes;

/// 数据元素约束级别（标准表头「约束/条件」列）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Obligation {
    /// 必备（Mandatory）——缺省即视为不合规。
    M,
    /// 可选（Optional）。
    O,
    /// 条件必选（Conditional）——满足某条件时必备。
    C,
}

impl Obligation {
    /// 标准代码字符。
    pub const fn as_code(self) -> &'static str {
        match self {
            Obligation::M => "M",
            Obligation::O => "O",
            Obligation::C => "C",
        }
    }
}

impl fmt::Display for Obligation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_code())
    }
}

/// 数据元素数据类型（标准表头「数据类型」列）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    /// 字符型（Char）。
    C,
    /// 数值型（Number）。
    N,
    /// 日期型，格式 `YYYYMMDD`。
    D,
    /// 时间型，格式 `hhmmss`。
    T,
    /// 逻辑型（`0`/`1`）。
    L,
    /// 二进制（以 Base64 文本承载）。
    B,
}

impl DataType {
    /// 标准类型字符。
    pub const fn as_code(self) -> &'static str {
        match self {
            DataType::C => "C",
            DataType::N => "N",
            DataType::D => "D",
            DataType::T => "T",
            DataType::L => "L",
            DataType::B => "B",
        }
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_code())
    }
}

/// 单个数据元素定义（标准表中的一行）。
///
/// 所有字段均为 `'static`，可直接编译进二进制，无需运行时配置。
#[derive(Debug, Clone, Copy)]
pub struct FieldDef {
    /// 数据元素标识符，如 `JCTB020101`。
    pub id: &'static str,
    /// 中文名称，如「姓名」。
    pub name: &'static str,
    /// 数据类型。
    pub data_type: DataType,
    /// 最大长度（字符数）。`0` 表示依标准另行说明或不限制。
    pub length: usize,
    /// 约束级别。
    pub obligation: Obligation,
    /// 引用的代码表标识符（见 [`codes`]），`None` 表示自由文本或裸数值。
    pub code_ref: Option<&'static str>,
    /// 取用来源：本元素若「取用」自其他数据子集的某数据元素，记录其标识符。
    ///
    /// 例如 `JCXS010102`（学生.姓名）取用 `JCTB020101`（人员.姓名）。
    pub source: Option<&'static str>,
    /// 备注/说明。
    pub note: &'static str,
}

impl FieldDef {
    /// 校验单个取值。
    ///
    /// 依次检查：必填长度、类型格式、代码表合法性。
    pub fn validate(&self, value: &str) -> Result<(), EmgiError> {
        let char_len = value.chars().count();
        if self.length != 0 && char_len > self.length {
            return Err(EmgiError::ExceedsLength {
                id: self.id.to_string(),
                name: self.name.to_string(),
                max: self.length,
                got: char_len,
            });
        }

        match self.data_type {
            DataType::C => {}
            DataType::N => {
                if value.parse::<f64>().is_err() {
                    return Err(EmgiError::InvalidType {
                        id: self.id.to_string(),
                        name: self.name.to_string(),
                        expected: "N(数值)",
                        got: value.to_string(),
                    });
                }
            }
            DataType::D => {
                if !is_valid_date8(value) {
                    return Err(EmgiError::InvalidType {
                        id: self.id.to_string(),
                        name: self.name.to_string(),
                        expected: "D(YYYYMMDD)",
                        got: value.to_string(),
                    });
                }
            }
            DataType::T => {
                if !is_valid_time6(value) {
                    return Err(EmgiError::InvalidType {
                        id: self.id.to_string(),
                        name: self.name.to_string(),
                        expected: "T(hhmmss)",
                        got: value.to_string(),
                    });
                }
            }
            DataType::L => {
                if value != "0" && value != "1" {
                    return Err(EmgiError::InvalidType {
                        id: self.id.to_string(),
                        name: self.name.to_string(),
                        expected: "L(0/1)",
                        got: value.to_string(),
                    });
                }
            }
            DataType::B => {
                // 二进制以 Base64 文本承载，仅做非空与长度合理性检查。
                if value.is_empty() {
                    return Err(EmgiError::InvalidType {
                        id: self.id.to_string(),
                        name: self.name.to_string(),
                        expected: "B(Base64)",
                        got: "<empty>".to_string(),
                    });
                }
            }
        }

        if let Some(table) = self.code_ref {
            if let Some(reason) = codes::validate_code(table, value) {
                return Err(EmgiError::InvalidCode {
                    id: self.id.to_string(),
                    name: self.name.to_string(),
                    code: value.to_string(),
                    table: table.to_string(),
                    reason,
                });
            }
        }

        Ok(())
    }
}

/// 校验错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmgiError {
    /// 必备数据元素缺失。
    MissingMandatory { id: String, name: String },
    /// 数据类型不符。
    InvalidType {
        id: String,
        name: String,
        expected: &'static str,
        got: String,
    },
    /// 超出最大长度。
    ExceedsLength {
        id: String,
        name: String,
        max: usize,
        got: usize,
    },
    /// 代码不在引用代码表内。
    InvalidCode {
        id: String,
        name: String,
        code: String,
        table: String,
        reason: &'static str,
    },
}

impl fmt::Display for EmgiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmgiError::MissingMandatory { id, name } => {
                write!(f, "必备数据元素缺失: {name} ({id})")
            }
            EmgiError::InvalidType {
                id,
                name,
                expected,
                got,
            } => write!(f, "数据类型不符: {name} ({id}) 期望 {expected} 实际 {got}"),
            EmgiError::ExceedsLength {
                id,
                name,
                max,
                got,
            } => write!(f, "超出最大长度: {name} ({id}) 上限 {max} 实际 {got}"),
            EmgiError::InvalidCode {
                id,
                name,
                code,
                table,
                reason,
            } => write!(
                f,
                "代码非法: {name} ({id})={code} 代码表 {table} 校验失败: {reason}"
            ),
        }
    }
}

impl std::error::Error for EmgiError {}

/// 校验 `YYYYMMDD` 格式的合法日期。
pub fn is_valid_date8(s: &str) -> bool {
    if s.len() != 8 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let year: u32 = s[0..4].parse().unwrap_or(0);
    let month: u32 = s[4..6].parse().unwrap_or(0);
    let day: u32 = s[6..8].parse().unwrap_or(0);
    if month == 0 || month > 12 || !(1900..=9999).contains(&year) {
        return false;
    }
    let max_day = match month {
        2 => {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=max_day).contains(&day)
}

/// 校验 `hhmmss` 格式的合法时间。
pub fn is_valid_time6(s: &str) -> bool {
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let hour: u32 = s[0..2].parse().unwrap_or(99);
    let minute: u32 = s[2..4].parse().unwrap_or(99);
    let second: u32 = s[4..6].parse().unwrap_or(99);
    (0..=23).contains(&hour) && (0..=59).contains(&minute) && (0..=59).contains(&second)
}

/// 由 `chrono` 当前时间生成 `YYYYMMDD` + `hhmmss` 字符串。
pub fn now_emgi_datetime() -> (String, String) {
    use chrono::Utc;
    let now = Utc::now();
    (now.format("%Y%m%d").to_string(), now.format("%H%M%S").to_string())
}

/// 数据类 trait：任何符合 JY/T 1002-2012 的数据类都应实现。
pub trait EmgiRecordable {
    /// 所属数据子集，如 `JCTB`/`JCXS`。
    const SUBSET: &'static str;
    /// 数据类标识符，如 `JCXS0101`。
    const CLASS_ID: &'static str;
    /// 数据类中文名称，如「学生基本」。
    const CLASS_NAME: &'static str;

    /// 返回本类所有数据元素（含取用/引用的源字段）及其当前取值。
    ///
    /// 取值为 `None` 表示该项未填写——可选(O)项允许为空，必备(M)项为空则校验失败。
    fn fields(&self) -> Vec<(&'static FieldDef, Option<String>)>;

    /// 本类「引用」的源数据类标识符，用于跨类溯源。
    ///
    /// 例如学生学籍(`JCXS0201`)引用班级(`JCXX0201`)与学生(`JCXS0101`)。
    fn references(&self) -> &'static [&'static str] {
        &[]
    }

    /// 转换为扁平可序列化记录。
    fn to_record(&self) -> EmgiRecord {
        EmgiRecord {
            subset: Self::SUBSET.to_string(),
            class_id: Self::CLASS_ID.to_string(),
            class_name: Self::CLASS_NAME.to_string(),
            references: self.references().iter().map(|s| s.to_string()).collect(),
            fields: self
                .fields()
                .into_iter()
                .map(|(d, v)| EmgiField {
                    id: d.id.to_string(),
                    name: d.name.to_string(),
                    obligation: d.obligation,
                    data_type: d.data_type,
                    length: d.length,
                    code_ref: d.code_ref.map(str::to_string),
                    source: d.source.map(str::to_string),
                    value: v,
                })
                .collect(),
        }
    }

    /// 校验必备项与类型/代码合法性。
    ///
    /// 返回首个遇到的错误集合（全部错误一次性给出，便于一次性整改）。
    fn validate(&self) -> Result<(), Vec<EmgiError>> {
        let mut errs = Vec::new();
        for (def, val) in self.fields() {
            match val {
                None => {
                    if def.obligation == Obligation::M {
                        errs.push(EmgiError::MissingMandatory {
                            id: def.id.to_string(),
                            name: def.name.to_string(),
                        });
                    }
                }
                Some(v) => {
                    if let Err(e) = def.validate(&v) {
                        errs.push(e);
                    }
                }
            }
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }
}

/// 扁平数据字段（可序列化，可持久化）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmgiField {
    /// 数据元素标识符。
    pub id: String,
    /// 中文名称。
    pub name: String,
    /// 约束级别。
    pub obligation: Obligation,
    /// 数据类型。
    pub data_type: DataType,
    /// 最大长度（字符数）。
    pub length: usize,
    /// 引用的代码表（若有）。
    pub code_ref: Option<String>,
    /// 取用来源元素标识符（若有）。
    pub source: Option<String>,
    /// 当前取值；`None` 表示未填写。
    pub value: Option<String>,
}

impl EmgiField {
    /// 基于字段自身元数据进行校验（供数据集级批量校验复用）。
    pub fn validate(&self) -> Result<(), EmgiError> {
        match &self.value {
            None => {
                if self.obligation == Obligation::M {
                    Err(EmgiError::MissingMandatory {
                        id: self.id.to_string(),
                        name: self.name.to_string(),
                    })
                } else {
                    Ok(())
                }
            }
            Some(v) => {
                let char_len = v.chars().count();
                if self.length != 0 && char_len > self.length {
                    return Err(EmgiError::ExceedsLength {
                        id: self.id.to_string(),
                        name: self.name.to_string(),
                        max: self.length,
                        got: char_len,
                    });
                }
                match self.data_type {
                    DataType::N => {
                        if v.parse::<f64>().is_err() {
                            return Err(EmgiError::InvalidType {
                                id: self.id.to_string(),
                                name: self.name.to_string(),
                                expected: "N(数值)",
                                got: v.clone(),
                            });
                        }
                    }
                    DataType::D => {
                        if !is_valid_date8(v) {
                            return Err(EmgiError::InvalidType {
                                id: self.id.to_string(),
                                name: self.name.to_string(),
                                expected: "D(YYYYMMDD)",
                                got: v.clone(),
                            });
                        }
                    }
                    DataType::T => {
                        if !is_valid_time6(v) {
                            return Err(EmgiError::InvalidType {
                                id: self.id.to_string(),
                                name: self.name.to_string(),
                                expected: "T(hhmmss)",
                                got: v.clone(),
                            });
                        }
                    }
                    DataType::L => {
                        if v != "0" && v != "1" {
                            return Err(EmgiError::InvalidType {
                                id: self.id.to_string(),
                                name: self.name.to_string(),
                                expected: "L(0/1)",
                                got: v.clone(),
                            });
                        }
                    }
                    DataType::B => {
                        if v.is_empty() {
                            return Err(EmgiError::InvalidType {
                                id: self.id.to_string(),
                                name: self.name.to_string(),
                                expected: "B(Base64)",
                                got: "<empty>".to_string(),
                            });
                        }
                    }
                    DataType::C => {}
                }
                if let Some(table) = self.code_ref.as_deref() {
                    if let Some(reason) = codes::validate_code(table, v) {
                        return Err(EmgiError::InvalidCode {
                            id: self.id.to_string(),
                            name: self.name.to_string(),
                            code: v.clone(),
                            table: table.to_string(),
                            reason,
                        });
                    }
                }
                Ok(())
            }
        }
    }
}

/// 扁平数据记录（可序列化，可持久化）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmgiRecord {
    /// 所属数据子集。
    pub subset: String,
    /// 数据类标识符。
    pub class_id: String,
    /// 数据类名称。
    pub class_name: String,
    /// 引用源数据类。
    pub references: Vec<String>,
    /// 字段列表。
    pub fields: Vec<EmgiField>,
}

impl EmgiRecord {
    /// 校验本记录的全部字段，返回（字段标识, 错误）列表；空表示通过。
    pub fn validate(&self) -> Vec<(String, EmgiError)> {
        let mut errs = Vec::new();
        for f in &self.fields {
            if let Err(e) = f.validate() {
                errs.push((f.id.to_string(), e));
            }
        }
        errs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_date8_valid_and_invalid() {
        assert!(is_valid_date8("20240810"));
        assert!(is_valid_date8("20240229")); // 闰年
        assert!(!is_valid_date8("20250229")); // 非闰年
        assert!(!is_valid_date8("20241301")); // 月越界
        assert!(!is_valid_date8("20240232")); // 日越界
        assert!(!is_valid_date8("2024-08-10"));
    }

    #[test]
    fn test_time6_valid_and_invalid() {
        assert!(is_valid_time6("083015"));
        assert!(is_valid_time6("235959"));
        assert!(!is_valid_time6("240000"));
        assert!(!is_valid_time6("0830"));
        assert!(!is_valid_time6("08:30:15"));
    }

    const D_NAME: FieldDef = FieldDef {
        id: "T0001",
        name: "姓名",
        data_type: DataType::C,
        length: 50,
        obligation: Obligation::M,
        code_ref: None,
        source: None,
        note: "",
    };
    const D_GENDER: FieldDef = FieldDef {
        id: "T0002",
        name: "性别",
        data_type: DataType::C,
        length: 1,
        obligation: Obligation::M,
        code_ref: Some("GBT_2261_1_GENDER"),
        source: None,
        note: "",
    };

    #[test]
    fn test_field_validate() {
        assert!(D_NAME.validate("张三").is_ok());
        // 超过 50 个汉字（每汉字计 1 字符）应被长度约束拒绝
        let long_name: String = "名".repeat(51);
        assert!(D_NAME.validate(&long_name).is_err());

        assert!(D_GENDER.validate("1").is_ok());
        assert!(D_GENDER.validate("7").is_err()); // 不在代码表
    }
}
