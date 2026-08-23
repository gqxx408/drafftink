//! # 日期时间标准化（GB/T 7408.1-2023）
//!
//! 校本教学套件（JY/T 1007 配套）核心日期字段（学生出生日期、入学日期等）的
//! 标准化处理模块。
//!
//! 支持输入格式（按优先级）：
//!
//! - a. 基本格式 `YYYYMMDD`（无分隔符，利于数据库索引）
//! - b. 扩展格式 `YYYY-MM-DD`（符合国标扩展原则，利于人工阅读）
//! - c. 带时刻的基本格式 `YYYYMMDDTHHMMSS`（仅取日期部分）
//!
//! 容错：历史遗留的 `/` 分隔符自动归一为 `-` 后重试。
//!
//! Ref: GB/T 7408.1-2023 《日期和时间 信息交换表示法 第1部分：基本原则》
//!      （等同采用 ISO 8601-1:2019）

use chrono::{Datelike, NaiveDate, NaiveDateTime};
use std::fmt;
use std::str::FromStr;

/// 年份合理范围下界（含）。Ref: GB/T 7408.1-2023 教育管理字段领域约束。
const MIN_YEAR: i32 = 1900;
/// 年份合理范围上界（含）。
const MAX_YEAR: i32 = 2100;

/// 标准化日期解析/校验错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GbDateTimeError {
    /// 输入不符合任何受支持格式（基本/扩展/带时刻），或日历值非法
    /// （如月份 13、平年 2 月 30 日）。
    InvalidFormat,
    /// 年份超出合理范围 `[MIN_YEAR, MAX_YEAR]`，包含负数年份。
    OutOfRange,
    /// 出现了不被接受的分隔符（如 `/` 容错后仍无法解析，或其它非法分隔符）。
    InvalidSeparator,
}

impl fmt::Display for GbDateTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GbDateTimeError::InvalidFormat => write!(
                f,
                "invalid date format (expected YYYYMMDD, YYYY-MM-DD or YYYYMMDDTHHMMSS)"
            ),
            GbDateTimeError::OutOfRange => {
                write!(f, "year out of supported range [{MIN_YEAR}, {MAX_YEAR}]")
            }
            GbDateTimeError::InvalidSeparator => write!(
                f,
                "invalid date separator (standard is '-'; '/' is tolerated as legacy input)"
            ),
        }
    }
}

impl std::error::Error for GbDateTimeError {}

/// 符合 GB/T 7408.1-2023 的标准化日期。
///
/// 内部以 `NaiveDate` 存储，不含时区与时刻信息（仅日期粒度，满足出生日期、
/// 入学日期等字段需求）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GbDateTime {
    date: NaiveDate,
}

impl GbDateTime {
    /// 由 `NaiveDate` 构造，并执行年份范围校验。
    ///
    /// Ref: GB/T 7408.1-2023 领域约束（年份落在 `[MIN_YEAR, MAX_YEAR]`）。
    pub fn from_naive_date(date: NaiveDate) -> Result<Self, GbDateTimeError> {
        if date.year() < MIN_YEAR || date.year() > MAX_YEAR {
            return Err(GbDateTimeError::OutOfRange);
        }
        Ok(Self { date })
    }

    /// 由年/月/日构造，自动校验月份（1-12）与日期（含闰年）。
    ///
    /// Ref: GB/T 7408.1-2023 基本构成要素。
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Result<Self, GbDateTimeError> {
        let date =
            NaiveDate::from_ymd_opt(year, month, day).ok_or(GbDateTimeError::InvalidFormat)?;
        Self::from_naive_date(date)
    }

    /// 解析字符串为标准日期。
    ///
    /// 支持格式（按优先级 a → b → c）：
    /// a. 基本格式 `YYYYMMDD`
    /// b. 扩展格式 `YYYY-MM-DD`
    /// c. 带时刻的基本格式 `YYYYMMDDTHHMMSS`（取日期部分）
    ///
    /// 容错：遇到 `/` 分隔符时自动替换为 `-` 后重试（历史遗留数据常见）。
    ///
    /// Ref: GB/T 7408.1-2023 §4 基本格式与扩展格式。
    pub fn parse(s: &str) -> Result<Self, GbDateTimeError> {
        let raw = s.trim();
        if raw.is_empty() {
            return Err(GbDateTimeError::InvalidFormat);
        }

        // 容错：将历史遗留的 '/' 归一化为标准 '-'。
        // Ref: GB/T 7408.1-2023 扩展格式家族（'-' 为授权分隔符）。
        let had_slash = raw.contains('/');
        let normalized = raw.replace('/', "-");

        // 负数年份直接判为超范围（所有合法年份均为正，区间为 [MIN_YEAR, MAX_YEAR]）。
        // chrono 的 %Y%m%d 对前导 '-' 会整体解析失败，此处提前明确拒绝。
        // Ref: GB/T 7408.1-2023 领域约束（年份范围）。
        if normalized.starts_with('-') {
            return Err(GbDateTimeError::OutOfRange);
        }

        // 依次尝试受支持格式（优先级 a → b → c）。
        let parsed = NaiveDate::parse_from_str(&normalized, "%Y%m%d") // a. 基本格式
            .ok()
            .or_else(|| NaiveDate::parse_from_str(&normalized, "%Y-%m-%d").ok()) // b. 扩展格式
            .or_else(|| {
                // c. 带时刻的基本格式；仅取日期部分（时刻不在本结构范畴内）。
                NaiveDateTime::parse_from_str(&normalized, "%Y%m%dT%H%M%S")
                    .ok()
                    .map(|dt| dt.date())
            });

        let date = match parsed {
            Some(d) => d,
            None => {
                // 已尝试 '/' 容错仍失败，明确指向分隔符问题；否则为格式/日历值问题。
                return Err(if had_slash {
                    GbDateTimeError::InvalidSeparator
                } else {
                    GbDateTimeError::InvalidFormat
                });
            }
        };

        // 年份领域约束：拒绝负数与超范围年份。
        // Ref: GB/T 7408.1-2023 教育管理字段范围。
        if date.year() < MIN_YEAR || date.year() > MAX_YEAR {
            return Err(GbDateTimeError::OutOfRange);
        }

        Ok(Self { date })
    }

    /// 返回内部 `NaiveDate`。
    pub fn to_naive_date(&self) -> NaiveDate {
        self.date
    }

    /// 年份。
    pub fn year(&self) -> i32 {
        self.date.year()
    }

    /// 月份（1-12）。
    pub fn month(&self) -> u32 {
        self.date.month()
    }

    /// 日（1-31）。
    pub fn day(&self) -> u32 {
        self.date.day()
    }

    /// 输出基本格式 `YYYYMMDD`（紧凑，利于数据库索引与比较）。
    ///
    /// Ref: GB/T 7408.1-2023 基本格式。
    pub fn to_basic_string(&self) -> String {
        self.date.format("%Y%m%d").to_string()
    }
}

impl FromStr for GbDateTime {
    type Err = GbDateTimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for GbDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 默认输出扩展格式 YYYY-MM-DD，符合国标且为 Web 端最通用形式。
        // Ref: GB/T 7408.1-2023 扩展格式。
        write!(f, "{}", self.date.format("%Y-%m-%d"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_format() {
        // a. 基本格式 YYYYMMDD
        let d = GbDateTime::parse("20240812").unwrap();
        assert_eq!(format!("{d}"), "2024-08-12");
        assert_eq!(d.to_basic_string(), "20240812");
    }

    #[test]
    fn test_parse_extended_format() {
        // b. 扩展格式 YYYY-MM-DD
        let d = GbDateTime::parse("2024-08-12").unwrap();
        assert_eq!(format!("{d}"), "2024-08-12");
    }

    #[test]
    fn test_parse_slash_fault_tolerance() {
        // 容错：'/' 自动转为 '-'
        let d = GbDateTime::parse("2024/08/12").unwrap();
        assert_eq!(format!("{d}"), "2024-08-12");
    }

    #[test]
    fn test_parse_invalid_month() {
        // 非法月份 13
        assert!(GbDateTime::parse("2024-13-01").is_err());
    }

    #[test]
    fn test_parse_invalid_day_non_leap_feb() {
        // 平年 2 月无 30 日
        assert!(GbDateTime::parse("20240230").is_err());
    }

    #[test]
    fn test_parse_datetime_basic_keeps_date_only() {
        // c. 带时刻的基本格式，仅取日期部分
        let d = GbDateTime::parse("20240812T093000").unwrap();
        assert_eq!(format!("{d}"), "2024-08-12");
        assert_eq!(d.to_basic_string(), "20240812");
    }

    #[test]
    fn test_out_of_range_year_low() {
        assert_eq!(
            GbDateTime::parse("18000101").unwrap_err(),
            GbDateTimeError::OutOfRange
        );
    }

    #[test]
    fn test_out_of_range_year_high() {
        assert_eq!(
            GbDateTime::parse("22000101").unwrap_err(),
            GbDateTimeError::OutOfRange
        );
    }

    #[test]
    fn test_negative_year_out_of_range() {
        // 负数年份应判为超范围
        assert_eq!(
            GbDateTime::parse("-20240101").unwrap_err(),
            GbDateTimeError::OutOfRange
        );
    }

    #[test]
    fn test_invalid_separator_after_slash_tolerance() {
        // 含 '/' 但日历值非法，应明确指向分隔符/格式问题
        let err = GbDateTime::parse("2024/13/01").unwrap_err();
        assert!(matches!(
            err,
            GbDateTimeError::InvalidSeparator | GbDateTimeError::InvalidFormat
        ));
    }

    #[test]
    fn test_from_ymd_validates_leap_and_bounds() {
        assert!(GbDateTime::from_ymd(2024, 2, 29).is_ok()); // 闰年 2/29
        assert!(GbDateTime::from_ymd(2023, 2, 29).is_err()); // 平年无 2/29
        assert!(GbDateTime::from_ymd(2024, 0, 1).is_err()); // 月份下溢
        assert!(GbDateTime::from_ymd(2024, 13, 1).is_err()); // 月份上溢
    }

    #[test]
    fn test_display_and_basic_roundtrip() {
        let d = GbDateTime::parse("20240812").unwrap();
        assert_eq!(format!("{d}"), "2024-08-12");
        let reparsed = GbDateTime::parse(&d.to_basic_string()).unwrap();
        assert_eq!(reparsed, d);
    }

    #[test]
    fn test_from_str_trait() {
        let d: GbDateTime = "2024-08-12".parse().unwrap();
        assert_eq!(d.year(), 2024);
        assert_eq!(d.month(), 8);
        assert_eq!(d.day(), 12);
    }
}
