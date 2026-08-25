//! # CSV 在线清洗接口（方向一 × 方向二 衔接）
//!
//! 接收 `multipart/form-data` 上传的 CSV，**内存处理、不落盘**，复用现有
//! [`drafftink_etl`] 的 `parse_csv` / `normalize_date`（零修改）完成脏日期标准化，
//! 并复用 `standards::lookup_code`（即 `/api/v1/lookup` 背后的纯函数）对代码列做合法性校验。
//!
//! ## 路由
//!
//! `POST /api/v1/etl/clean-csv`
//!
//! 表单字段：
//! - `file`：CSV 文件本体（必填）。
//! - `date_columns`：需标准化的日期列名，逗号分隔（如 `birth_date,enroll_date`）。
//! - `code_columns`：需校验的代码列，`列名:标准表` 形式、逗号分隔
//!   （如 `gender:gender,school_type:school_type`）；标准表名同 `/api/v1/lookup/{table}`。
//!
//! 返回示例：
//! ```json
//! {
//!   "summary": { "total": 4, "success": 3, "failed": 1 },
//!   "failed_rows": [
//!     { "row": 2, "column": "birth_date", "raw": "2009/13/01", "reason": "日期格式非法" }
//!   ],
//!   "code_issues": [
//!     { "row": 3, "column": "gender", "value": "9", "table": "gender",
//!       "reason": "代码 '9' 不在标准表 'gender'（依据 GB/T 2261.1-2003）中" }
//!   ],
//!   "preview": [ { "student_id": "S001", "birth_date": "2008-09-01", ... } ]
//! }
//! ```
//!
//! ## 设计要点
//!
//! - **零新增外部依赖**：仅复用 `drafftink-etl` + 已缓存的 `axum`（`multipart` 特性已在
//!   workspace 启用）/ `serde_json`，未引入任何新 crate。
//! - **复用而非重写**：日期解析与 CSV 解析逻辑完全来自 `drafftink-etl`，此处不做任何改动，
//!   避免重复造轮子、保证与 CLI 行为一致。
//! - **代码校验复用 lookup 逻辑**：直接调用与 HTTP 接口相同的 `lookup_code` 纯函数，
//!   而非在进程内自发起 HTTP 请求，无额外 I/O 开销、`summary.success/failed` 仅反映日期清洗结果，
//!   代码合法性问题单独在 `code_issues` 中列出。

use std::collections::{BTreeMap, HashMap, HashSet};

use axum::extract::multipart::Multipart;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use drafftink_etl::{normalize_date, parse_csv};

/// 单个需要校验的代码列映射：`列名 -> 标准表名`。
#[derive(Debug, Clone)]
pub struct CodeColumn {
    /// CSV 表头中的列名。
    pub column: String,
    /// 对应的标准表名（同 `/api/v1/lookup/{table}`）。
    pub table: String,
}

/// 清洗结果汇总（行级统计）。
#[derive(Debug, Serialize)]
pub struct Summary {
    /// 数据总行数（不含表头）。
    pub total: usize,
    /// 全部日期列均成功标准化的行数。
    pub success: usize,
    /// 至少存在一个日期列无法标准化的行数。
    pub failed: usize,
}

/// 单条日期清洗失败明细。
#[derive(Debug, Serialize)]
pub struct FailedRow {
    /// 数据行号（从 1 开始，不含表头）。
    pub row: usize,
    /// 出错的列名。
    pub column: String,
    /// 无法识别的原始字符串。
    pub raw: String,
    /// 错误原因（如「日期格式非法」）。
    pub reason: String,
}

/// 单条代码合法性问题。
#[derive(Debug, Serialize)]
pub struct CodeIssue {
    /// 数据行号（从 1 开始，不含表头）。
    pub row: usize,
    /// 出错的列名。
    pub column: String,
    /// 校验未通过的原值。
    pub value: String,
    /// 对应的标准表名。
    pub table: String,
    /// 错误原因（含所依据的标准号）。
    pub reason: String,
}

/// `POST /api/v1/etl/clean-csv` 的完整响应结构。
#[derive(Debug, Serialize)]
pub struct CleanCsvResponse {
    /// 行级汇总统计。
    pub summary: Summary,
    /// 日期清洗失败明细（逐单元格）。
    pub failed_rows: Vec<FailedRow>,
    /// 代码合法性问题（逐单元格）。
    pub code_issues: Vec<CodeIssue>,
    /// 前 10 行清洗后的数据，便于老师预览。
    pub preview: Vec<BTreeMap<String, String>>,
}

/// 纯函数：在内存中对 CSV 文本做清洗与校验，返回结构化结果。
///
/// 复用 [`drafftink_etl::parse_csv`] 与 [`drafftink_etl::normalize_date`]（不做任何修改），
/// 并通过 [`crate::api::standards::lookup_code`] 对指定代码列做合法性校验。
/// 所有逻辑均为无 I/O 纯计算，便于单测。
pub fn clean_csv_in_memory(
    csv_text: &str,
    date_columns: &[String],
    code_columns: &[CodeColumn],
) -> CleanCsvResponse {
    let (headers, rows) = parse_csv(csv_text);

    // 预计算需要处理的列索引，避免内层循环反复线性查找。
    let date_set: HashSet<usize> = date_columns
        .iter()
        .filter_map(|c| headers.iter().position(|h| h == c))
        .collect();
    let code_map: HashMap<usize, String> = code_columns
        .iter()
        .filter_map(|cc| {
            headers
                .iter()
                .position(|h| h == &cc.column)
                .map(|i| (i, cc.table.clone()))
        })
        .collect();

    let mut cleaned_rows: Vec<BTreeMap<String, String>> = Vec::with_capacity(rows.len());
    let mut failed_rows: Vec<FailedRow> = Vec::new();
    let mut code_issues: Vec<CodeIssue> = Vec::new();
    let mut success: usize = 0;
    let mut failed: usize = 0;

    for (i, row) in rows.iter().enumerate() {
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        let mut row_bad = false;

        for (j, h) in headers.iter().enumerate() {
            let val = row.get(j).cloned().unwrap_or_default();
            if date_set.contains(&j) {
                if val.trim().is_empty() {
                    // 空值不参与标准化，原样保留。
                    map.insert(h.clone(), val);
                } else {
                    match normalize_date(&val) {
                        Some(norm) => {
                            map.insert(h.clone(), norm);
                        }
                        None => {
                            row_bad = true;
                            failed_rows.push(FailedRow {
                                row: i + 1,
                                column: h.clone(),
                                raw: val.clone(),
                                reason: "日期格式非法".to_string(),
                            });
                            map.insert(h.clone(), val);
                        }
                    }
                }
            } else {
                map.insert(h.clone(), val);
            }
        }

        // 代码列合法性校验：复用 standards::lookup_code（与 /api/v1/lookup 同源）。
        for (j, table) in &code_map {
            if let Some(v) = row.get(*j) {
                let v = v.trim();
                if !v.is_empty() {
                    let resp = crate::api::standards::lookup_code(table, v);
                    if !resp.found {
                        code_issues.push(CodeIssue {
                            row: i + 1,
                            column: headers.get(*j).cloned().unwrap_or_default(),
                            value: v.to_string(),
                            table: table.clone(),
                            reason: if resp.standard.is_empty() {
                                format!("标准表 '{table}' 不存在")
                            } else {
                                format!(
                                    "代码 '{v}' 不在标准表 '{table}'（依据 {}）中",
                                    resp.standard
                                )
                            },
                        });
                    }
                }
            }
        }

        if row_bad {
            failed += 1;
        } else {
            success += 1;
        }
        cleaned_rows.push(map);
    }

    let total = rows.len();
    let preview = cleaned_rows.iter().take(10).cloned().collect();

    CleanCsvResponse {
        summary: Summary {
            total,
            success,
            failed,
        },
        failed_rows,
        code_issues,
        preview,
    }
}

/// `POST /api/v1/etl/clean-csv` 处理器。
///
/// 解析 `multipart/form-data`：提取 `file` / `date_columns` / `code_columns` 字段，
/// 调用 [`clean_csv_in_memory`] 完成内存清洗后返回 JSON。不读写任何磁盘文件。
pub async fn clean_csv(
    mut multipart: Multipart,
) -> Result<Json<CleanCsvResponse>, (StatusCode, String)> {
    let mut csv_text: Option<String> = None;
    let mut date_columns: Vec<String> = Vec::new();
    let mut code_columns: Vec<CodeColumn> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("multipart 解析失败: {e}")))?
    {
        let name = field.name().map(str::to_string);
        match name.as_deref() {
            Some("file") => {
                let data = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, format!("读取文件字段失败: {e}")))?;
                csv_text = Some(data);
            }
            Some("date_columns") => {
                let v = field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("读取 date_columns 失败: {e}"),
                    )
                })?;
                date_columns = v
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            Some("code_columns") => {
                let v = field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("读取 code_columns 失败: {e}"),
                    )
                })?;
                // 多个映射以逗号分隔，单个映射形如 `列名:标准表`。
                for part in v.split(',') {
                    if let Some((col, table)) = part.split_once(':') {
                        let col = col.trim().to_string();
                        let table = table.trim().to_string();
                        if !col.is_empty() && !table.is_empty() {
                            code_columns.push(CodeColumn { column: col, table });
                        }
                    }
                }
            }
            _ => { /* 忽略未识别字段 */ }
        }
    }

    let csv_text = csv_text.ok_or((
        StatusCode::BAD_REQUEST,
        "缺少上传的 CSV 文件字段（字段名应为 'file'）".to_string(),
    ))?;

    Ok(Json(clean_csv_in_memory(
        &csv_text,
        &date_columns,
        &code_columns,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_csv() -> &'static str {
        "student_id,gender,birth_date,enroll_date,school_type\n\
         S001,1,2008.9.1,2024/9/1,211\n\
         S002,2,2009/13/01,2024.8.31,311\n\
         S003,3,2010-03-05,2019-09-01,999\n\
         S004,,2010/1/1,2019/9/1,211\n"
    }

    #[test]
    fn test_clean_normalizes_dates_and_counts() {
        let resp = clean_csv_in_memory(
            sample_csv(),
            &["birth_date".into(), "enroll_date".into()],
            &[],
        );
        assert_eq!(resp.summary.total, 4);
        // S002 的 birth_date 非法 → 仅 1 行失败
        assert_eq!(resp.summary.failed, 1);
        assert_eq!(resp.summary.success, 3);
        assert_eq!(resp.failed_rows.len(), 1);
        assert_eq!(resp.failed_rows[0].row, 2);
        assert_eq!(resp.failed_rows[0].column, "birth_date");
        assert_eq!(resp.failed_rows[0].raw, "2009/13/01");
        // 首行日期已标准化
        assert_eq!(resp.preview[0].get("birth_date").unwrap(), "2008-09-01");
        assert_eq!(resp.preview[0].get("enroll_date").unwrap(), "2024-09-01");
    }

    #[test]
    fn test_code_validation_flags_invalid() {
        let resp = clean_csv_in_memory(
            sample_csv(),
            &[],
            &[
                CodeColumn {
                    column: "gender".into(),
                    table: "gender".into(),
                },
                CodeColumn {
                    column: "school_type".into(),
                    table: "school_type".into(),
                },
            ],
        );
        // S003：gender=3 与 school_type=999 均非法
        assert_eq!(resp.code_issues.len(), 2);
        assert!(resp
            .code_issues
            .iter()
            .any(|c| c.row == 3 && c.column == "gender" && c.value == "3"));
        assert!(resp
            .code_issues
            .iter()
            .any(|c| c.row == 3 && c.column == "school_type" && c.value == "999"));
        // 合法代码不应出现在 code_issues（S001 gender=1 / S002 gender=2 / S004 空值）
        assert!(!resp.code_issues.iter().any(|c| c.row == 1));
        assert!(!resp.code_issues.iter().any(|c| c.row == 2));
    }

    #[test]
    fn test_preview_limited_to_ten() {
        // 构造 12 行，验证 preview 最多 10 行
        let mut csv = String::from("id,birth_date\n");
        for n in 1..=12 {
            csv.push_str(&format!("R{n},2008.9.{n}\n"));
        }
        let resp = clean_csv_in_memory(&csv, &["birth_date".into()], &[]);
        assert_eq!(resp.summary.total, 12);
        assert_eq!(resp.preview.len(), 10);
    }

    #[test]
    fn test_missing_columns_are_noops() {
        // date_columns 指向不存在的列 → 不报错，原样返回
        let resp = clean_csv_in_memory(sample_csv(), &["nonexistent".into()], &[]);
        assert_eq!(resp.summary.total, 4);
        assert_eq!(resp.summary.failed, 0);
        assert!(resp.failed_rows.is_empty());
    }
}
