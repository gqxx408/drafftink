//! # drafftink-etl 命令行入口
//!
//! 用法：
//! ```text
//! drafftink-etl --input dirty.csv --dates birth_date,enroll_date [--output clean.json]
//! ```
//! - `--input, -i`    输入 CSV 路径（必填）
//! - `--dates, -d`    需要标准化的日期列名，逗号分隔（必填）
//! - `--output, -o`   输出 JSON 路径（可选，默认 `<input>.clean.json`）

use std::path::PathBuf;

use drafftink_etl::{run_etl, EtlConfig};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut dates: Vec<String> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--input" | "-i" => {
                i += 1;
                input = args.get(i).map(PathBuf::from);
            }
            "--output" | "-o" => {
                i += 1;
                output = args.get(i).map(PathBuf::from);
            }
            "--dates" | "-d" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    dates = v
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
            other => {
                anyhow::bail!("未知参数: {other}\n用法: drafftink-etl --input <csv> --dates <col1,col2> [--output <json>]");
            }
        }
        i += 1;
    }

    let input = match input {
        Some(p) => p,
        None => anyhow::bail!("缺少必填参数 --input <csv>"),
    };
    if dates.is_empty() {
        anyhow::bail!("缺少必填参数 --dates <col1,col2,...>");
    }

    let cfg = EtlConfig {
        input,
        output,
        date_columns: dates,
    };

    let report = run_etl(&cfg)?;
    println!(
        "📊 总行数 {} · 标准化成功 {} · 失败 {}",
        report.total_rows, report.normalized, report.failed
    );
    if !report.failures.is_empty() {
        println!("⚠️ 以下日期单元格无法识别：");
        for f in &report.failures {
            println!("   第 {} 行 [{}]: \"{}\"", f.row, f.column, f.raw);
        }
    }
    Ok(())
}
