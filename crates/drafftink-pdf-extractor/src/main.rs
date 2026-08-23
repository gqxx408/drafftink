use drafftink_pdf_extractor::{analyze, extract_text, is_cjk};

fn main() {
    for path in std::env::args().skip(1) {
        println!("══════════ {path} ══════════");
        match analyze(&path) {
            Ok(a) => {
                println!(
                    "  页={} 字体={} ToUnicode字体={} 中文={} 可提取中文={}",
                    a.pages, a.font_count, a.to_unicode_fonts, a.cjk_chars, a.extractable_chinese
                );
                println!("  诊断: {}", a.note);
            }
            Err(e) => {
                println!("  analyze 失败: {e}");
                continue;
            }
        }

        match extract_text(&path) {
            Ok(text) => {
                println!("  --- 前 10 行（仅显示非空行）---");
                let mut shown = 0;
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let cjk = trimmed.chars().filter(|c| is_cjk(*c)).count();
                    println!(
                        "  [中{}/总{}] {}",
                        cjk,
                        trimmed.chars().count(),
                        if trimmed.chars().count() > 60 {
                            format!("{}…", &trimmed.chars().take(60).collect::<String>())
                        } else {
                            trimmed.to_string()
                        }
                    );
                    shown += 1;
                    if shown >= 10 {
                        break;
                    }
                }
            }
            Err(e) => println!("  extract_text 失败: {e}"),
        }
    }
}
