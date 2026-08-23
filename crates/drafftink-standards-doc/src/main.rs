//! # drafftink-standards-doc — 校本数据标准手册生成器（方向三）
//!
//! 直接读取 `drafftink-core` 中硬编码的国标代码 `const` 数组（代码即唯一真相源），
//! 生成一份**自包含、可离线打开**的 HTML 手册，供老师与学生查阅。
//!
//! 用法：
//! ```text
//! drafftink-standards-doc [--output docs/standards_manual.html]
//! ```
//!
//! 说明：本生成器不依赖 mdbook / rustdoc（离线环境无法联网安装），但产出物与
//! mdbook 静态站点等价——单文件 HTML，含目录、状态徽章与逐表过滤搜索。

use std::io::Write;
use std::path::PathBuf;

use drafftink_core::{
    DEGREE_CODE, EDUCATION_LEVEL_CODE, ETHNIC_CODE, GenderCode, INDUSTRY_CLASS, INDUSTRY_DIVISION,
    INDUSTRY_SECTION, LANGUAGE_CODE, MARITAL_STATUS_CODE, PROVINCE_CODE, SCHOOL_TYPE_CODE,
    TECH_POSITION_CODE, URBAN_RURAL_CODE, YesNoCode,
};

/// 数据来源状态。
#[derive(Clone, Copy)]
enum Status {
    /// 机器从标准 PDF 提取。
    PdfExtracted,
    /// 取自已发布标准代码表（非 PDF 机器提取）。
    PublicDomain,
    /// 代码内定义（枚举 / 内部约定）。
    DefinedInCode,
    /// 已冻结，待教育部目录校核。
    Frozen,
}

impl Status {
    fn label(&self) -> &'static str {
        match self {
            Status::PdfExtracted => "PDF_EXTRACTED",
            Status::PublicDomain => "PUBLIC_DOMAIN_REFERENCE",
            Status::DefinedInCode => "DEFINED_IN_CODE",
            Status::Frozen => "FROZEN",
        }
    }
    fn badge_class(&self) -> &'static str {
        match self {
            Status::PdfExtracted => "ok",
            Status::PublicDomain => "warn",
            Status::DefinedInCode => "info",
            Status::Frozen => "frozen",
        }
    }
}

/// 单个代码表章节。
struct Section {
    title: &'static str,
    standard: &'static str,
    status: Status,
    note: &'static str,
    /// 每行：`[代码, 名称, 备注]`。
    rows: Vec<[String; 3]>,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn render_section(sec: &Section, idx: usize) -> String {
    let mut rows_html = String::new();
    for r in &sec.rows {
        rows_html.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td></tr>\n",
            esc(&r[0]),
            esc(&r[1]),
            esc(&r[2])
        ));
    }
    let anchor = format!("sec-{idx}");
    format!(
        r#"<section class="tbl" id="{anchor}">
  <h2>{title} <span class="badge {badge}">{status}</span></h2>
  <p class="meta">标准：{standard} ｜ {note}</p>
  <input class="filter" data-target="{anchor}" placeholder="过滤本表…" />
  <table>
    <thead><tr><th>代码</th><th>名称</th><th>备注</th></tr></thead>
    <tbody>
{rows}</tbody>
  </table>
</section>
"#,
        title = sec.title,
        badge = sec.status.badge_class(),
        status = sec.status.label(),
        standard = sec.standard,
        note = sec.note,
        rows = rows_html,
        anchor = anchor,
    )
}

fn build_sections() -> Vec<Section> {
    vec![
        Section {
            title: "省 / 自治区 / 直辖市 / 特别行政区",
            standard: "GB/T 2260-2007",
            status: Status::PdfExtracted,
            note: "机器从标准 PDF 提取",
            rows: PROVINCE_CODE
                .iter()
                .map(|(name, code, abbr)| [code.to_string(), name.to_string(), abbr.to_string()])
                .collect(),
        },
        Section {
            title: "办学类型",
            standard: "GB/T 33782-2017",
            status: Status::PdfExtracted,
            note: "机器从标准 PDF 提取",
            rows: SCHOOL_TYPE_CODE
                .iter()
                .map(|(name, code, abbr)| [code.to_string(), name.to_string(), abbr.to_string()])
                .collect(),
        },
        Section {
            title: "所在地城乡类型",
            standard: "GB/T 33782-2017",
            status: Status::PdfExtracted,
            note: "机器从标准 PDF 提取",
            rows: URBAN_RURAL_CODE
                .iter()
                .map(|(code, name)| [code.to_string(), name.to_string(), String::new()])
                .collect(),
        },
        Section {
            title: "学历",
            standard: "GB/T 4658-2006",
            status: Status::PublicDomain,
            note: "取自已发布标准代码表，非 PDF 机器提取，待校核",
            rows: EDUCATION_LEVEL_CODE
                .iter()
                .map(|(code, name)| [code.to_string(), name.to_string(), String::new()])
                .collect(),
        },
        Section {
            title: "学位",
            standard: "GB/T 6864-2003",
            status: Status::PublicDomain,
            note: "取自已发布标准代码表，非 PDF 机器提取，待校核",
            rows: DEGREE_CODE
                .iter()
                .map(|(code, name)| [code.to_string(), name.to_string(), String::new()])
                .collect(),
        },
        Section {
            title: "专业技术职务（系列）",
            standard: "GB/T 8561-2001",
            status: Status::PublicDomain,
            note: "取自已发布标准代码表，非 PDF 机器提取，待校核",
            rows: TECH_POSITION_CODE
                .iter()
                .map(|(code, name)| [code.to_string(), name.to_string(), String::new()])
                .collect(),
        },
        Section {
            title: "中国各民族名称",
            standard: "GB/T 3304-1991",
            status: Status::PublicDomain,
            note: "取自已发布标准代码表，非 PDF 机器提取，待校核",
            rows: ETHNIC_CODE
                .iter()
                .map(|(code, name)| [code.to_string(), name.to_string(), String::new()])
                .collect(),
        },
        Section {
            title: "婚姻状况",
            standard: "GB/T 2261.2-2003",
            status: Status::PublicDomain,
            note: "取自已发布标准代码表，非 PDF 机器提取，待校核",
            rows: MARITAL_STATUS_CODE
                .iter()
                .map(|(code, name)| [code.to_string(), name.to_string(), String::new()])
                .collect(),
        },
        Section {
            title: "国民经济行业 — 门类",
            standard: "GB/T 4754-2017",
            status: Status::PdfExtracted,
            note: "机器从标准 PDF 提取",
            rows: INDUSTRY_SECTION
                .iter()
                .map(|(code, name)| [code.to_string(), name.to_string(), String::new()])
                .collect(),
        },
        Section {
            title: "国民经济行业 — 大类",
            standard: "GB/T 4754-2017",
            status: Status::PdfExtracted,
            note: "机器从标准 PDF 提取",
            rows: INDUSTRY_DIVISION
                .iter()
                .map(|(code, name)| [code.to_string(), name.to_string(), String::new()])
                .collect(),
        },
        Section {
            title: "国民经济行业 — 小类（1638 条）",
            standard: "GB/T 4754-2017",
            status: Status::PdfExtracted,
            note: "机器从标准 PDF 提取；可用上方过滤框检索",
            rows: INDUSTRY_CLASS
                .iter()
                .map(|(code, name)| [code.to_string(), name.to_string(), String::new()])
                .collect(),
        },
        Section {
            title: "语种代码",
            standard: "GB/T 4881-1985",
            status: Status::Frozen,
            note: "1985 老旧标准，数字码与现代 zh/CN 混用，已冻结待教育部目录校核",
            rows: LANGUAGE_CODE
                .iter()
                .map(|(code, name)| [code.to_string(), name.to_string(), String::new()])
                .collect(),
        },
        Section {
            title: "人的性别",
            standard: "GB/T 2261.1-2003",
            status: Status::DefinedInCode,
            note: "代码内 repr(u8) 枚举定义",
            rows: vec![
                [GenderCode::Unknown.code().to_string(), GenderCode::Unknown.name().to_string(), "0".into()],
                [GenderCode::Male.code().to_string(), GenderCode::Male.name().to_string(), "1".into()],
                [GenderCode::Female.code().to_string(), GenderCode::Female.name().to_string(), "2".into()],
                [GenderCode::Unspecified.code().to_string(), GenderCode::Unspecified.name().to_string(), "9".into()],
            ],
        },
        Section {
            title: "是否标志（内部约定）",
            standard: "—",
            status: Status::DefinedInCode,
            note: "0=否, 1=是；系统内部约定，非某特定 GB/T 标准",
            rows: vec![
                [YesNoCode::No.code().to_string(), YesNoCode::No.name().to_string(), "0".into()],
                [YesNoCode::Yes.code().to_string(), YesNoCode::Yes.name().to_string(), "1".into()],
            ],
        },
    ]
}

const STYLE: &str = r#"
:root { color-scheme: light; }
* { box-sizing: border-box; }
body { font-family: -apple-system, "PingFang SC", "Microsoft YaHei", system-ui, sans-serif;
       margin: 0; background: #f5f7fa; color: #1f2933; line-height: 1.6; }
header { background: linear-gradient(135deg,#1e3a8a,#2563eb); color: #fff; padding: 28px 32px; }
header h1 { margin: 0; font-size: 24px; }
header p { margin: 6px 0 0; opacity: .9; font-size: 14px; }
nav { position: sticky; top: 0; background: #fff; border-bottom: 1px solid #e5e7eb;
      padding: 10px 32px; display: flex; flex-wrap: wrap; gap: 8px; z-index: 10; }
nav a { font-size: 13px; color: #2563eb; text-decoration: none; padding: 4px 10px;
        background: #eff6ff; border-radius: 999px; }
nav a:hover { background: #dbeafe; }
main { padding: 24px 32px 64px; max-width: 1080px; margin: 0 auto; }
.tbl { background: #fff; border: 1px solid #e5e7eb; border-radius: 12px;
       padding: 20px 22px; margin: 20px 0; box-shadow: 0 1px 3px rgba(0,0,0,.04); }
.tbl h2 { margin: 0 0 4px; font-size: 18px; display: flex; align-items: center; gap: 10px; }
.meta { color: #6b7280; font-size: 13px; margin: 0 0 12px; }
.badge { font-size: 11px; font-weight: 700; padding: 2px 8px; border-radius: 6px; color: #fff; }
.badge.ok { background: #16a34a; }
.badge.warn { background: #d97706; }
.badge.info { background: #0891b2; }
.badge.frozen { background: #dc2626; }
.filter { width: 100%; padding: 8px 12px; margin-bottom: 12px; border: 1px solid #d1d5db;
          border-radius: 8px; font-size: 14px; }
table { width: 100%; border-collapse: collapse; font-size: 14px; }
th, td { text-align: left; padding: 8px 10px; border-bottom: 1px solid #f1f5f9; }
th { background: #f8fafc; font-weight: 600; color: #475569; position: sticky; top: 48px; }
tbody tr:hover { background: #f8fafc; }
code { background: #f1f5f9; padding: 1px 6px; border-radius: 5px; font-family: ui-monospace, monospace; }
footer { text-align: center; color: #9ca3af; font-size: 12px; padding: 24px; }
"#;

const SCRIPT: &str = r#"
document.querySelectorAll('input.filter').forEach(function (inp) {
  inp.addEventListener('input', function () {
    var q = inp.value.trim().toLowerCase();
    var sec = document.getElementById(inp.dataset.target);
    var rows = sec.querySelectorAll('tbody tr');
    rows.forEach(function (tr) {
      var text = tr.textContent.toLowerCase();
      tr.style.display = (q === '' || text.indexOf(q) !== -1) ? '' : 'none';
    });
  });
});
"#;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut out = PathBuf::from("docs/standards_manual.html");
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--output" || args[i] == "-o" {
            i += 1;
            if let Some(v) = args.get(i) {
                out = PathBuf::from(v);
            }
        }
        i += 1;
    }

    let sections = build_sections();
    let total_rows: usize = sections.iter().map(|s| s.rows.len()).sum();

    // 目录
    let mut toc = String::new();
    for (idx, s) in sections.iter().enumerate() {
        toc.push_str(&format!(
            r##"<a href="#sec-{idx}">{title}</a>"##,
            idx = idx,
            title = s.title
        ));
    }

    // 章节
    let mut body = String::new();
    for (idx, s) in sections.iter().enumerate() {
        body.push_str(&render_section(s, idx));
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>校本数据标准手册 · drafftink</title>
<style>{style}</style>
</head>
<body>
<header>
  <h1>校本数据标准手册</h1>
  <p>由 drafftink-core 代码自动生成 · 共 {count} 张代码表 / {rows} 条记录 · 数据来源见每表状态徽章</p>
</header>
<nav>{toc}</nav>
<main>
{body}
</main>
<footer>本手册由代码常量生成，与 GB/T 标准代码表保持同步。PDF_EXTRACTED 为机器提取，PUBLIC_DOMAIN_REFERENCE / FROZEN 待官方文本校核。</footer>
<script>{script}</script>
</body>
</html>
"#,
        style = STYLE,
        script = SCRIPT,
        count = sections.len(),
        rows = total_rows,
        toc = toc,
        body = body,
    );

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(&out)?;
    f.write_all(html.as_bytes())?;
    println!(
        "📘 标准手册已生成 → {} （{} 张表 / {} 条记录）",
        out.display(),
        sections.len(),
        total_rows
    );
    Ok(())
}
