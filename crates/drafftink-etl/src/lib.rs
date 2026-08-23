//! # drafftink-etl — 校本脏数据清洗管道（方向二）
//!
//! 处理学校实际收到的“脏数据”：Excel 导出 / 手填 CSV 中五花八门的日期写法
//! （如 `2024.8.12`、`2024/8/12`、`2024年8月12日`、`20100305` 等），统一标准化为
//! GB/T 7408.1-2023 基本格式 `YYYY-MM-DD`，并输出结构化 JSON 清洗报告。
//!
//! 设计原则：
//! - **零新增外部依赖**：仅复用 `drafftink-core`（含 `GbDateTime` 与 `validate_yyyymmdd`）
//!   与已缓存的 `serde_json` / `anyhow`，避免离线环境联网拉包。
//! - **可复用**：`normalize_date` / `run_etl` 均为纯函数，可被其他模块直接调用。
//! - **可审计**：输出报告同时包含成功数、失败数与每条失败明细（`行号 / 列名 / 原始值`）。

use std::collections::BTreeMap;
use std::path::PathBuf;

use drafftink_core::validate_yyyymm;
use drafftink_core::GbDateTime;
use serde::Serialize;

/// ETL 运行配置。
pub struct EtlConfig {
    /// 输入 CSV 路径。
    pub input: PathBuf,
    /// 输出 JSON 路径；为 `None` 时默认写到 `<input>.clean.json`。
    pub output: Option<PathBuf>,
    /// 需要标准化的日期列名（必须与 CSV 表头一致）。
    pub date_columns: Vec<String>,
}

/// 单条日期清洗失败明细。
#[derive(Debug, Clone, Serialize)]
pub struct DateFailure {
    /// 数据行号（从 1 开始，不含表头）。
    pub row: usize,
    /// 出错的列名。
    pub column: String,
    /// 无法识别的原始字符串。
    pub raw: String,
}

/// ETL 清洗报告（即最终输出的 JSON 结构）。
#[derive(Debug, Serialize)]
pub struct EtlReport {
    /// 源文件路径。
    pub source: String,
    /// 数据总行数（不含表头）。
    pub total_rows: usize,
    /// 参与标准化的日期列。
    pub date_columns: Vec<String>,
    /// 成功标准化的日期单元格数。
    pub normalized: usize,
    /// 无法识别的日期单元格数。
    pub failed: usize,
    /// 失败明细。
    pub failures: Vec<DateFailure>,
    /// 清洗后的逐行记录（日期列已标准化）。
    pub rows: Vec<BTreeMap<String, String>>,
}

/// 将五花八门的日期写法标准化为 `YYYY-MM-DD`。
///
/// 支持：`20240812`(8 位)、`2024.8.12`、`2024/8/12`、`2024-8-12`、
/// `2024年8月12日`、`2024.08.12`、`2024/08/12` 等。
/// 末尾的时间部分（`2024/8/12 09:30`）会被忽略，仅取日期。
/// 非法日期（如 `2024-13-01`、`2024/2/30`）返回 `None`。
///
/// 最终合法性校验交由 [`GbDateTime`]（GB/T 7408.1-2023，年份 1900–2100）。
pub fn normalize_date(raw: &str) -> Option<String> {
    // 去除首尾空白，并仅取首个空白之前的日期部分（忽略时间）。
    let s = raw.trim();
    let s = s.split_whitespace().next().unwrap_or(s);
    if s.is_empty() {
        return None;
    }

    // 1) 纯数字 8 位基本格式：YYYYMMDD
    if s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) {
        return GbDateTime::parse(s).ok().map(|d| d.to_string());
    }
    // 2) 纯数字 6 位基本格式：YYYYMM（补齐为当月 1 号）
    if s.len() == 6 && s.bytes().all(|b| b.is_ascii_digit()) && validate_yyyymm(s) {
        return Some(format!("{}-{}-01", &s[0..4], &s[4..6]));
    }

    // 3) 分隔符写法：提取至多 3 个整数段（年/月/日）
    let mut nums: Vec<u32> = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Ok(n) = cur.parse::<u32>() {
                nums.push(n);
            }
            cur.clear();
        }
    }
    if !cur.is_empty() {
        if let Ok(n) = cur.parse::<u32>() {
            nums.push(n);
        }
    }

    match nums.len() {
        // 年月日三段 → YYYY-MM-DD
        3 => {
            let candidate = format!("{:04}-{:02}-{:02}", nums[0], nums[1], nums[2]);
            GbDateTime::parse(&candidate).ok().map(|d| d.to_string())
        }
        // 年月两段 → YYYY-MM（按年月校验）
        2 => {
            let candidate = format!("{:04}{:02}", nums[0], nums[1]);
            if validate_yyyymm(&candidate) {
                Some(format!("{:04}-{:02}", nums[0], nums[1]))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// 极简 CSV 解析器（支持双引号字段、字段内逗号与换行、双引号转义 `""`）。
///
/// 返回 `(表头, 数据行)`。足以覆盖校本场景常见的 Excel 导出 CSV。
pub fn parse_csv(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    // 剥离 UTF-8 BOM（Excel 导出的 CSV 常在首行前带 \u{feff}，会导致首列表头含不可见字符）
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut field = String::new();
    let mut record: Vec<String> = Vec::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => {
                    record.push(std::mem::take(&mut field));
                }
                '\r' => { /* 忽略，交由 \n 断行 */ }
                '\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    // 丢弃末尾可能存在的空行
    if let Some(last) = records.last() {
        if last.iter().all(|f| f.trim().is_empty()) {
            records.pop();
        }
    }

    if records.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let headers = records.remove(0);
    (headers, records)
}

/// 执行 ETL：读取 CSV → 标准化日期列 → 写出 JSON 报告。
///
/// 当 `config.output` 为 `None` 时，默认写入 `<input>.clean.json`。
pub fn run_etl(config: &EtlConfig) -> anyhow::Result<EtlReport> {
    let text = std::fs::read_to_string(&config.input)
        .map_err(|e| anyhow::anyhow!("读取输入文件失败 {}: {e}", config.input.display()))?;
    let (headers, rows) = parse_csv(&text);

    // 定位日期列的索引（表头中可能不存在的列会被静默跳过）
    let date_idx: Vec<usize> = config
        .date_columns
        .iter()
        .filter_map(|c| headers.iter().position(|h| h == c))
        .collect();

    let mut out_rows: Vec<BTreeMap<String, String>> = Vec::with_capacity(rows.len());
    let mut failures: Vec<DateFailure> = Vec::new();
    let mut normalized: usize = 0;
    let mut failed: usize = 0;

    for (i, row) in rows.iter().enumerate() {
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        for (j, h) in headers.iter().enumerate() {
            let val = row.get(j).cloned().unwrap_or_default();
            if date_idx.contains(&j) {
                match normalize_date(&val) {
                    Some(norm) => {
                        normalized += 1;
                        map.insert(h.clone(), norm);
                    }
                    None => {
                        failed += 1;
                        failures.push(DateFailure {
                            row: i + 1,
                            column: h.clone(),
                            raw: val.clone(),
                        });
                        map.insert(h.clone(), val);
                    }
                }
            } else {
                map.insert(h.clone(), val);
            }
        }
        out_rows.push(map);
    }

    let report = EtlReport {
        source: config.input.display().to_string(),
        total_rows: rows.len(),
        date_columns: config.date_columns.clone(),
        normalized,
        failed,
        failures,
        rows: out_rows,
    };

    let json = serde_json::to_string_pretty(&report)?;
    let out_path = match &config.output {
        Some(p) => p.clone(),
        None => {
            let mut p = config.input.clone();
            p.set_extension("clean.json");
            p
        }
    };
    std::fs::write(&out_path, json)
        .map_err(|e| anyhow::anyhow!("写出报告失败 {}: {e}", out_path.display()))?;
    eprintln!("✅ 清洗完成 → {}", out_path.display());
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_variants() {
        assert_eq!(normalize_date("20240812"), Some("2024-08-12".into()));
        assert_eq!(normalize_date("2024.8.12"), Some("2024-08-12".into()));
        assert_eq!(normalize_date("2024/8/12"), Some("2024-08-12".into()));
        assert_eq!(normalize_date("2024-08-12"), Some("2024-08-12".into()));
        assert_eq!(normalize_date("2024年8月12日"), Some("2024-08-12".into()));
        assert_eq!(normalize_date("2024.08.12"), Some("2024-08-12".into()));
        // 带时间部分：仅取日期
        assert_eq!(normalize_date("2024/8/12 09:30"), Some("2024-08-12".into()));
        // 年月
        assert_eq!(normalize_date("202408"), Some("2024-08-01".into()));
        assert_eq!(normalize_date("2024.8"), Some("2024-08".into()));
    }

    #[test]
    fn test_normalize_invalid() {
        assert_eq!(normalize_date("2024-13-01"), None); // 非法月份
        assert_eq!(normalize_date("2024/2/30"), None); // 平年 2 月无 30 日
        assert_eq!(normalize_date(""), None);
        assert_eq!(normalize_date("hello"), None);
    }

    #[test]
    fn test_parse_csv_quoted() {
        let text = "id,name,note\n1,张三,\"含,逗号\"\n2,李四,\"换\n行\"\n";
        let (h, rows) = parse_csv(text);
        assert_eq!(h, vec!["id", "name", "note"]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][2], "含,逗号");
        assert_eq!(rows[1][2], "换\n行");
    }

    #[test]
    fn test_run_etl_end_to_end() {
        let csv = "student_id,birth_date,enroll_date\nS001,2008.9.1,2024/9/1\nS002,2009/13/01,2024.8.31\n";
        let dir = std::env::temp_dir();
        let in_path = dir.join("drafftink_etl_test_in.csv");
        let out_path = dir.join("drafftink_etl_test_in.clean.json");
        std::fs::write(&in_path, csv).unwrap();
        let cfg = EtlConfig {
            input: in_path.clone(),
            output: Some(out_path.clone()),
            date_columns: vec!["birth_date".into(), "enroll_date".into()],
        };
        let report = run_etl(&cfg).unwrap();
        assert_eq!(report.total_rows, 2);
        // S001 两个日期均成功；S002 的 birth_date 非法
        assert_eq!(report.normalized, 3);
        assert_eq!(report.failed, 1);
        assert_eq!(report.failures[0].row, 2);
        assert_eq!(report.rows[0].get("birth_date").unwrap(), "2008-09-01");
        std::fs::remove_file(&in_path).ok();
        std::fs::remove_file(&out_path).ok();
    }
}
