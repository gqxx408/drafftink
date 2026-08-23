//! ZXX 数据集的 XML 导出（结构对齐 [`crate::emgi::xml::to_xml`]）。

use crate::zxx::{SUBSET_NAMES, ZxxDataset};

/// 转义 XML 特殊字符。
fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// 将 [`ZxxDataset`] 导出为符合标准的 XML 文本。
///
/// 根元素为 `<ZXX>`，每个数据类导出为 `<RECORD>`，其字段为 `<DATA-ELEMENT>`，
/// 取用渊源通过 `<REFERENCES>` 表达。
pub fn to_xml(ds: &ZxxDataset) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<ZXX standard=\"{}\" generated_at=\"{}\" record_count=\"{}\">\n",
        escape_xml(&ds.standard),
        escape_xml(&ds.generated_at),
        ds.record_count()
    ));

    for (id, name) in SUBSET_NAMES {
        out.push_str(&format!(
            "  <SUBSET id=\"{}\" name=\"{}\"/>\n",
            escape_xml(id),
            escape_xml(name)
        ));
    }

    for rec in &ds.records {
        out.push_str(&format!(
            "  <RECORD class=\"{}\" name=\"{}\" subset=\"{}\">\n",
            escape_xml(&rec.class_id),
            escape_xml(&rec.class_name),
            escape_xml(&rec.subset)
        ));
        for f in &rec.fields {
            let value = f.value.as_deref().unwrap_or("");
            out.push_str(&format!(
                "    <DATA-ELEMENT id=\"{}\" name=\"{}\" obligation=\"{}\" type=\"{}\">{}</DATA-ELEMENT>\n",
                escape_xml(&f.id),
                escape_xml(&f.name),
                f.obligation,
                f.data_type,
                escape_xml(value)
            ));
        }
        if !rec.references.is_empty() {
            let refs: String = rec
                .references
                .iter()
                .map(|r| format!("<REF>{}</REF>", escape_xml(r)))
                .collect();
            out.push_str(&format!("    <REFERENCES>{refs}</REFERENCES>\n"));
        }
        out.push_str("  </RECORD>\n");
    }

    out.push_str("</ZXX>\n");
    out
}
