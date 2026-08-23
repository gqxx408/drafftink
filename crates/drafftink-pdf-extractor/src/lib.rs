//! drafftink-pdf-extractor
//!
//! 纯 Rust 的国标 PDF 文本提取库。零系统调用，完全内存解析，
//! 通过解析字体自带的 ToUnicode CMap 将字形码还原为 UTF-8 文本。

pub mod pdf_extractor;

pub use pdf_extractor::{
    analyze, extract_text, is_cjk, parse_tounicode_cmap, PdfAnalysis,
};
