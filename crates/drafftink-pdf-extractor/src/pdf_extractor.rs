//! `pdf_extractor` —— 纯 Rust 的国标 PDF 文本提取器（基于 lopdf）。
//!
//! 设计目标：在不调用任何外部系统命令、完全内存中解析的前提下，
//! 通过解析字体自带的 **ToUnicode CMap**（`bfchar` / `bfrange`），将 PDF 内
//! 容流中的字形码（CID）还原为正确的 UTF-8 文本，解决 CID 字体直接按字节
//! 解码产生乱码的问题。
//!
//! 仅依赖 `lopdf`（纯 Rust，零系统调用）。对不含 ToUnicode CMap 的字体，
//! 退化为基于 Differences/AGL 的拉丁回退；对确实无任何 Unicode 映射的 PDF
//! （如文字被轮廓化、或子集字体无映射），`analyze()` 会如实报告，不会伪造中文。

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object};
use std::collections::BTreeMap;

/// 判断一个字符是否属于中日韩统一表意文字（含扩展 A、兼容区）。
pub fn is_cjk(c: char) -> bool {
    let u = c as u32;
    (0x2E80..=0x9FFF).contains(&u)
        || (0x3400..=0x4DBF).contains(&u)
        || (0xF900..=0xFAFF).contains(&u)
        || (0x20000..=0x2A6DF).contains(&u)
}

// ---------------------------------------------------------------------------
// ToUnicode CMap 解析
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Tok {
    Kw(String),
    Hex(Vec<u8>),
}

/// 将 CMap 文本切分为关键字与十六进制 token。
fn tokenize_cmap(text: &str) -> Vec<Tok> {
    let b = text.as_bytes();
    let mut i = 0;
    let mut toks = Vec::new();
    while i < b.len() {
        match b[i] {
            b'<' => {
                i += 1;
                let mut h = String::new();
                while i < b.len() && b[i] != b'>' {
                    h.push(b[i] as char);
                    i += 1;
                }
                i += 1; // 跳过 '>'
                if h.len().is_multiple_of(2) && !h.is_empty() {
                    if let Ok(bytes) = hex_decode(&h) {
                        toks.push(Tok::Hex(bytes));
                    }
                }
            }
            b'[' | b']' => {
                toks.push(Tok::Kw(text[i..=i].to_string()));
                i += 1;
            }
            c if c.is_ascii_alphabetic() || c == b'/' => {
                let start = i;
                while i < b.len()
                    && !b[i].is_ascii_whitespace()
                    && b[i] != b'<'
                    && b[i] != b'>'
                    && b[i] != b'['
                    && b[i] != b']'
                {
                    i += 1;
                }
                toks.push(Tok::Kw(text[start..i].to_string()));
            }
            _ => i += 1,
        }
    }
    toks
}

fn hex_decode(s: &str) -> Result<Vec<u8>, std::num::ParseIntError> {
    // 按 2 字符一组解析；奇数长度时末位补 0，避免越界 panic（CMap 中 hex 恒为偶长）。
    s.as_bytes()
        .chunks(2)
        .map(|c| {
            let hi = c[0] as char;
            let lo = if c.len() == 2 { c[1] as char } else { '0' };
            u8::from_str_radix(&format!("{hi}{lo}"), 16)
        })
        .collect()
}

/// 将 UTF-16BE 字节（可能为代理对）解码为 Rust 字符串。
fn utf16be_to_string(bytes: &[u8]) -> String {
    let mut units = Vec::with_capacity(bytes.len() / 2 + 1);
    let mut i = 0;
    while i + 1 < bytes.len() {
        units.push(u16::from_be_bytes([bytes[i], bytes[i + 1]]));
        i += 2;
    }
    char::decode_utf16(units)
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect()
}

fn bytes_be_to_u64(b: &[u8]) -> u64 {
    let mut v = 0u64;
    for &x in b {
        v = (v << 8) | x as u64;
    }
    v
}

fn u64_to_be_bytes(v: u64, width: usize) -> Vec<u8> {
    let mut out = vec![0u8; width];
    for i in 0..width {
        out[width - 1 - i] = ((v >> (8 * i)) & 0xff) as u8;
    }
    out
}

/// 线性 `bfrange`：`<start> <end> <dst>` → 每个 CID 按相同偏移映射到 Unicode。
fn add_bfrange(map: &mut BTreeMap<Vec<u8>, String>, start: &[u8], end: &[u8], dst: &[u8]) {
    if start.len() != dst.len() || start.len() != end.len() {
        return; // 仅支持等宽（BMP）线性映射
    }
    let start_i = bytes_be_to_u64(start);
    let end_i = bytes_be_to_u64(end);
    let dst_i = bytes_be_to_u64(dst);
    if end_i < start_i || end_i - start_i > 0x1_0000 {
        return; // 防御异常范围
    }
    let width = start.len();
    for off in 0..=(end_i - start_i) {
        let code = u64_to_be_bytes(start_i + off, width);
        let uni = u64_to_be_bytes(dst_i + off, width);
        map.insert(code, utf16be_to_string(&uni));
    }
}

/// 数组 `bfrange`：`<start> <end> [<u1> <u2> ...]` → 逐码点映射。
fn add_bfrange_array(
    map: &mut BTreeMap<Vec<u8>, String>,
    start: &[u8],
    end: &[u8],
    arr: &[Vec<u8>],
) {
    let start_i = bytes_be_to_u64(start);
    let end_i = bytes_be_to_u64(end);
    let count = (end_i - start_i + 1) as usize;
    for (o, dst_bytes) in arr.iter().take(count).enumerate() {
        let code = u64_to_be_bytes(start_i + o as u64, start.len());
        map.insert(code, utf16be_to_string(dst_bytes));
    }
}

/// 解析 ToUnicode CMap，返回 `(字形码 -> Unicode 字符串, 代码单元字节宽度)`。
///
/// 支持 `bfchar`、`bfrange`（线性与数组两种 dst 形式）、codespacerange 推断宽度，
/// 以及 UTF-16 代理对（> U+FFFF 的字符）。
pub fn parse_tounicode_cmap(cmap: &[u8]) -> (BTreeMap<Vec<u8>, String>, usize) {
    let text = String::from_utf8_lossy(cmap);
    let toks = tokenize_cmap(&text);
    let mut map: BTreeMap<Vec<u8>, String> = BTreeMap::new();
    let mut code_width = 0usize;
    let mut k = 0;
    while k < toks.len() {
        match &toks[k] {
            Tok::Kw(w) if w.as_str() == "begincodespacerange" => {
                k += 1;
                if let Some(Tok::Hex(h)) = toks.get(k) {
                    code_width = h.len();
                }
                while k < toks.len() {
                    if let Tok::Kw(w2) = &toks[k] {
                        if w2.as_str() == "endcodespacerange" {
                            k += 1;
                            break;
                        }
                    }
                    k += 1;
                }
            }
            Tok::Kw(w) if w.as_str() == "beginbfchar" => {
                k += 1;
                while k < toks.len() {
                    match &toks[k] {
                        Tok::Kw(w2) if w2.as_str() == "endbfchar" => {
                            k += 1;
                            break;
                        }
                        Tok::Hex(src) => {
                            if let Some(Tok::Hex(dst)) = toks.get(k + 1) {
                                map.insert(src.clone(), utf16be_to_string(dst));
                                k += 2;
                            } else {
                                k += 1;
                            }
                        }
                        _ => k += 1,
                    }
                }
            }
            Tok::Kw(w) if w.as_str() == "beginbfrange" => {
                k += 1;
                while k < toks.len() {
                    match &toks[k] {
                        Tok::Kw(w2) if w2.as_str() == "endbfrange" => {
                            k += 1;
                            break;
                        }
                        Tok::Hex(start) => {
                            if let (Some(Tok::Hex(end)), Some(next)) =
                                (toks.get(k + 1), toks.get(k + 2))
                            {
                                if let Tok::Hex(dst) = next {
                                    add_bfrange(&mut map, start, end, dst);
                                    k += 3;
                                    continue;
                                } else if let Tok::Kw(arr) = next {
                                    if arr.as_str() == "[" {
                                        k += 3;
                                        let mut arr_hex = Vec::new();
                                        while k < toks.len() {
                                            match &toks[k] {
                                                Tok::Kw(w3) if w3.as_str() == "]" => {
                                                    k += 1;
                                                    break;
                                                }
                                                Tok::Hex(h) => {
                                                    arr_hex.push(h.clone());
                                                    k += 1;
                                                }
                                                _ => k += 1,
                                            }
                                        }
                                        add_bfrange_array(&mut map, start, end, &arr_hex);
                                        continue;
                                    }
                                }
                            }
                            k += 1;
                        }
                        _ => k += 1,
                    }
                }
            }
            _ => k += 1,
        }
    }
    (map, code_width)
}

// ---------------------------------------------------------------------------
// 字体解码器
// ---------------------------------------------------------------------------

struct FontDecoder {
    /// ToUnicode 映射（CID -> Unicode）。无则为 None。
    to_unicode: Option<BTreeMap<Vec<u8>, String>>,
    /// 字形码单元字节宽度（Type0/Identity-H 为 2，简单字体为 1）。
    code_width: usize,
    /// Differences 中的 `码点 -> 字形名` 映射（用于无 ToUnicode 的拉丁回退）。
    code_to_name: Option<BTreeMap<u8, String>>,
}

/// Adobe Glyph List（精简版）：将标准字形名映射为 Unicode 码点。
fn agl_to_char(name: &str) -> Option<char> {
    Some(match name {
        "space" => ' ',
        "exclam" => '!',
        "quotedbl" => '"',
        "numbersign" => '#',
        "dollar" => '$',
        "percent" => '%',
        "ampersand" => '&',
        "quotesingle" => '\'',
        "quoteright" => '\'',
        "parenleft" => '(',
        "parenright" => ')',
        "asterisk" => '*',
        "plus" => '+',
        "comma" => ',',
        "hyphen" => '-',
        "period" => '.',
        "slash" => '/',
        "zero" => '0',
        "one" => '1',
        "two" => '2',
        "three" => '3',
        "four" => '4',
        "five" => '5',
        "six" => '6',
        "seven" => '7',
        "eight" => '8',
        "nine" => '9',
        "colon" => ':',
        "semicolon" => ';',
        "less" => '<',
        "equal" => '=',
        "greater" => '>',
        "question" => '?',
        "at" => '@',
        "bracketleft" => '[',
        "backslash" => '\\',
        "bracketright" => ']',
        "asciicircum" => '^',
        "underscore" => '_',
        "grave" => '`',
        "quoteleft" => '`',
        "braceleft" => '{',
        "bar" => '|',
        "braceright" => '}',
        "asciitilde" => '~',
        _ => {
            // 单字符字形名（如 A-Z、a-z）直接作为码点
            if name.len() == 1 {
                name.chars().next().unwrap()
            } else {
                return None;
            }
        }
    })
}

/// 从字体的 /Encoding /Differences 解析 `码点 -> 字形名`。
fn parse_differences(fdict: &Dictionary, doc: &Document) -> Option<BTreeMap<u8, String>> {
    let enc = fdict.get(b"Encoding").ok()?;
    let enc_dict = match enc {
        Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };
    let diff = enc_dict.get(b"Differences").ok()?.as_array().ok()?;
    let mut map = BTreeMap::new();
    let mut code: i64 = 0;
    for o in diff {
        match o {
            Object::Integer(i) => code = *i,
            Object::Name(n) => {
                if (0..=255).contains(&code) {
                    map.insert(code as u8, String::from_utf8_lossy(n).to_string());
                }
                code += 1;
            }
            _ => {}
        }
    }
    Some(map)
}

/// 为单个字体构建解码器。
fn build_decoder(fdict: &Dictionary, doc: &Document) -> FontDecoder {
    let is_type0 = fdict.get(b"Subtype").and_then(Object::as_name).ok() == Some(&b"Type0"[..]);

    let mut code_width = if is_type0 { 2 } else { 1 };
    let mut to_unicode = None;

    if let Ok(stream) = fdict
        .get_deref(b"ToUnicode", doc)
        .and_then(Object::as_stream)
    {
        if let Ok(data) = stream.decompressed_content() {
            let (map, w) = parse_tounicode_cmap(&data);
            if !map.is_empty() {
                to_unicode = Some(map);
            }
            if w > 0 {
                code_width = w;
            }
        }
    }

    let code_to_name = parse_differences(fdict, doc);
    FontDecoder {
        to_unicode,
        code_width,
        code_to_name,
    }
}

/// 将一个文本操作数序列解码为字符串（处理 Tj/TJ 中的字符串与数组嵌套）。
fn decode_operands(ops: &[Object], dec: &FontDecoder) -> String {
    let mut out = String::new();
    for o in ops {
        match o {
            Object::String(b, _) => {
                if let Some(map) = &dec.to_unicode {
                    out.push_str(&decode_run(b, dec.code_width, map));
                } else {
                    for &byte in b {
                        let ch = match &dec.code_to_name {
                            Some(m) => match m.get(&byte) {
                                Some(name) => agl_to_char(name).unwrap_or(byte as char),
                                None => byte as char,
                            },
                            None => byte as char,
                        };
                        out.push(ch);
                    }
                }
            }
            Object::Array(arr) => out.push_str(&decode_operands(arr, dec)),
            _ => {}
        }
    }
    out
}

/// 按 code_width 将字节流切分为字形码并查 ToUnicode 表。
fn decode_run(bytes: &[u8], width: usize, map: &BTreeMap<Vec<u8>, String>) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i + width <= bytes.len() {
        let key = bytes[i..i + width].to_vec();
        match map.get(&key) {
            Some(s) => out.push_str(s),
            None => out.push('\u{FFFD}'),
        }
        i += width;
    }
    out
}

// ---------------------------------------------------------------------------
// 对外 API
// ---------------------------------------------------------------------------

/// 提取整个 PDF 的纯文本（UTF-8）。零系统调用，纯内存解析。
pub fn extract_text(path: &str) -> Result<String, String> {
    let doc = Document::load(path).map_err(|e| e.to_string())?;
    let pages: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();
    let mut out = String::new();

    for (_n, page_id) in pages {
        let fonts = doc.get_page_fonts(page_id).map_err(|e| e.to_string())?;
        let mut decoders: BTreeMap<Vec<u8>, FontDecoder> = BTreeMap::new();
        for (fname, fdict) in &fonts {
            decoders.insert(fname.clone(), build_decoder(fdict, &doc));
        }

        let content_data = doc.get_page_content(page_id);
        let content = Content::decode(&content_data).map_err(|e| e.to_string())?;

        let mut cur_font: Option<Vec<u8>> = None;
        for op in &content.operations {
            match op.operator.as_str() {
                "Tf" => {
                    if let Some(Object::Name(n)) = op.operands.first() {
                        cur_font = Some(n.clone());
                    }
                }
                "Tj" | "TJ" => {
                    if let Some(fname) = &cur_font {
                        if let Some(dec) = decoders.get(fname) {
                            out.push_str(&decode_operands(&op.operands, dec));
                        }
                    }
                }
                "'" | "\"" => {
                    out.push('\n');
                    if let Some(fname) = &cur_font {
                        if let Some(dec) = decoders.get(fname) {
                            out.push_str(&decode_operands(&op.operands, dec));
                        }
                    }
                }
                "T*" | "ET" => out.push('\n'),
                _ => {}
            }
        }
        out.push('\n'); // 页间分隔
    }
    Ok(out)
}

/// PDF 文本可提取性诊断结果。
#[derive(Debug, Clone)]
pub struct PdfAnalysis {
    pub pages: usize,
    pub font_count: usize,
    pub to_unicode_fonts: usize,
    pub cjk_chars: usize,
    pub extractable_chinese: bool,
    pub note: String,
}

/// 分析 PDF 是否可通过 ToUnicode 解析还原中文。
pub fn analyze(path: &str) -> Result<PdfAnalysis, String> {
    let text = extract_text(path)?;
    let doc = Document::load(path).map_err(|e| e.to_string())?;
    let pages = doc.get_pages().len();
    let mut font_count = 0;
    let mut to_unicode_fonts = 0;
    for (_n, pid) in doc.get_pages() {
        if let Ok(fonts) = doc.get_page_fonts(pid) {
            for f in fonts.values() {
                font_count += 1;
                if f.get(b"ToUnicode").is_ok() {
                    to_unicode_fonts += 1;
                }
            }
        }
    }
    let cjk = text.chars().filter(|c| is_cjk(*c)).count();
    let note = if to_unicode_fonts == 0 {
        "无 ToUnicode CMap：无法通过解析 ToUnicode 还原中文。该 PDF 文字可能已轮廓化\
         （text-as-outline）或采用无 Unicode 映射的子集字体。"
            .to_string()
    } else {
        "含 ToUnicode CMap，可解析还原中文。".to_string()
    };
    Ok(PdfAnalysis {
        pages,
        font_count,
        to_unicode_fonts,
        cjk_chars: cjk,
        extractable_chinese: cjk > 0,
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CMAP: &str = "\
/CIDInit /ProcSet findresource begin 12 dict begin begincmap
/CIDSystemInfo <<>> def
1 begincodespacerange
<0000> <FFFF> endcodespacerange
2 beginbfchar
<0001> <0041>
<0002> <4E2D>
endbfchar
1 beginbfrange
<1581> <1583> <4E66>
endbfrange
1 beginbfrange
<2000> <2001> [<4E00> <4E01>]
endbfrange
endcmap";

    #[test]
    fn test_parse_bfchar_and_bfrange() {
        let (map, w) = parse_tounicode_cmap(SAMPLE_CMAP.as_bytes());
        assert_eq!(w, 2, "codespacerange 应推断 2 字节宽度");
        // bfchar
        assert_eq!(map.get(&vec![0x00, 0x01]), Some(&"A".to_string()));
        assert_eq!(map.get(&vec![0x00, 0x02]), Some(&"中".to_string()));
        // 线性 bfrange: 0x1581..0x1583 -> U+4E66..U+4E68（连续码点，偏移保持）
        let a = map.get(&vec![0x15, 0x81]).unwrap();
        let b = map.get(&vec![0x15, 0x82]).unwrap();
        let c = map.get(&vec![0x15, 0x83]).unwrap();
        assert_eq!(a, &"书".to_string()); // U+4E66
        assert_ne!(a, b);
        assert_ne!(b, c);
        let ca = a.chars().next().unwrap() as u32;
        let cb = b.chars().next().unwrap() as u32;
        let cc = c.chars().next().unwrap() as u32;
        assert_eq!(cb, ca + 1);
        assert_eq!(cc, ca + 2);
        // 数组 bfrange: 0x2000 -> U+4E00(一), 0x2001 -> U+4E01(丁)
        assert_eq!(map.get(&vec![0x20, 0x00]), Some(&"一".to_string()));
        assert_eq!(map.get(&vec![0x20, 0x01]), Some(&"丁".to_string()));
    }

    #[test]
    fn test_surrogate_pair_decode() {
        // U+1F600 的 UTF-16BE 为 <D83D DE00>
        let s = utf16be_to_string(&[0xD8, 0x3D, 0xDE, 0x00]);
        assert_eq!(s, "😀");
    }

    #[test]
    fn test_decode_run_width2() {
        let mut map = BTreeMap::new();
        map.insert(vec![0x00, 0x01], "A".to_string());
        map.insert(vec![0x00, 0x02], "中".to_string());
        // 字节流按 2 字节切分
        let out = decode_run(&[0x00, 0x01, 0x00, 0x02], 2, &map);
        assert_eq!(out, "A中");
    }

    #[test]
    fn test_is_cjk() {
        assert!(is_cjk('中'));
        assert!(is_cjk('書'));
        assert!(!is_cjk('A'));
        assert!(!is_cjk('1'));
    }
}
