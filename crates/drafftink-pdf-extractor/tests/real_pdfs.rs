//! 真实国标 PDF 端到端验证。
//!
//! 关键结论（已在用户机器上实证）：
//! - 带 ToUnicode CMap 的国标 PDF（如 12407-2008）可被正确提取出中文；
//! - GB/T 4658-2006 等 PDF **本身不含任何 ToUnicode CMap**（字体仅含标准拉丁字形名、
//!   中文实为轮廓化/无映射子集字体），因此"解析 ToUnicode 还原中文"对此类 PDF 无解，
//!   模块会如实 report 而非伪造中文。

use drafftink_pdf_extractor::analyze;
use std::path::PathBuf;

/// 返回 PDF 目录：优先读取 `GBT_PDF_DIR` 环境变量，否则用默认 Downloads 路径。
fn pdf_dir() -> PathBuf {
    if let Ok(d) = std::env::var("GBT_PDF_DIR") {
        PathBuf::from(d)
    } else {
        PathBuf::from("C:/Users/William Guo/Downloads")
    }
}

fn pdf(name: &str) -> PathBuf {
    pdf_dir().join(name)
}

/// 正控：12407-2008 含 ToUnicode，可提取出中文。
#[test]
fn positive_control_12407_extracts_chinese() {
    let p = pdf("12407-2008-gbt-e-300.pdf");
    if !p.exists() {
        eprintln!("skip: {} not found", p.display());
        return;
    }
    let a = analyze(p.to_str().unwrap()).expect("analyze 12407");
    assert!(
        a.to_unicode_fonts > 0,
        "12407 应含 ToUnicode 字体，实际 {}",
        a.to_unicode_fonts
    );
    assert!(
        a.cjk_chars > 0,
        "12407 应能提取出中文，实际 {}",
        a.cjk_chars
    );
    println!(
        "12407 OK: to_unicode_fonts={}, cjk={}",
        a.to_unicode_fonts, a.cjk_chars
    );
}

/// 目标 PDF：GB/T 4658-2006 无 ToUnicode CMap，故无法提取中文（如实记录，而非伪造）。
#[test]
fn target_4658_has_no_tounicode() {
    let p = pdf("4658-2006-gbt-e-300.pdf");
    if !p.exists() {
        eprintln!("skip: {} not found", p.display());
        return;
    }
    let a = analyze(p.to_str().unwrap()).expect("analyze 4658");
    // 根因：该 PDF 字体全部无 ToUnicode CMap，文字为轮廓化/无映射子集字体。
    assert_eq!(
        a.to_unicode_fonts, 0,
        "GB/T 4658-2006 不应含 ToUnicode 字体（根因：无 Unicode 映射）"
    );
    assert_eq!(a.cjk_chars, 0, "无 ToUnicode 时不应提取到中文");
    assert!(!a.extractable_chinese);
    println!("4658 OK (honest): 无 ToUnicode，CJK=0 —— 符合预期限制");
}
