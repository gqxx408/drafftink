//! # 语种名称代码表（GB/T 4881-1985）
//!
//! 校本教学套件（JY/T 1007 配套）所需的语种代码表。
//!
//! ## ⚠️ 数据来源与时效性声明（重要）
//!
//! 源文件 `4881-1985-gbt-e-300.pdf` 为图像 / CID 字体，文本层**零中文**
//! （pypdf/pymupdf 提取均为 0 字符），本沙箱无 OCR 引擎，无法从 PDF 逐字节提取。
//!
//! **本表代码/名称取自 GB/T 4881-1985 已发布的语种代码表常见语种，并非从该 PDF
//! 机器提取；具体数字代码的映射关系请务必与官方标准文本核对后再用于生产。**
//! 若提供含 ToUnicode 映射 / 可复制文本的标准文件，我将重新提取并校正。

/// 语种名称代码表。元素：`(代码, 名称)`。
///
/// Source: GB/T 4881-1985（取值自已发布标准代码表，需与官方文本核对）。
/// SourceStatus: [PUBLIC_DOMAIN_REFERENCE] — 取值自已发布标准代码表，非 PDF 机器提取。
/// Version: PublicDomainSnapshot-2026-08
/// **TODO: Verify against Ministry of Education latest directory before production use.**
pub const LANGUAGE_CODE: [(&str, &str); 21] = [
    ("1", "汉语"),
    ("2", "英语"),
    ("3", "法语"),
    ("4", "德语"),
    ("5", "日语"),
    ("6", "俄语"),
    ("7", "西班牙语"),
    ("8", "阿拉伯语"),
    ("9", "葡萄牙语"),
    ("10", "意大利语"),
    ("11", "朝鲜语"),
    ("12", "蒙古语"),
    ("13", "维吾尔语"),
    ("14", "藏语"),
    ("15", "壮语"),
    ("16", "印尼语"),
    ("17", "印地语"),
    ("18", "泰语"),
    ("19", "越南语"),
    ("20", "土耳其语"),
    ("21", "其他语言"),
];

/// 按代码查询语种名称。Source: GB/T 4881-1985。
///
/// ⚠️ 本表为 [PUBLIC_DOMAIN_REFERENCE]，数字↔名称映射未经官方 PDF 逐字节核对，生产前须验证。
#[inline(always)]
pub fn get_language_name(code: &str) -> Option<&'static str> {
    LANGUAGE_CODE.iter().find(|&&(c, _)| c == code).map(|&(_, n)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_first_last() {
        assert_eq!(LANGUAGE_CODE[0], ("1", "汉语"));
        assert_eq!(LANGUAGE_CODE[20], ("21", "其他语言"));
    }

    #[test]
    fn test_language_lookup() {
        assert_eq!(get_language_name("2"), Some("英语"));
        assert_eq!(get_language_name("5"), Some("日语"));
        assert_eq!(get_language_name("99"), None);
    }
}
