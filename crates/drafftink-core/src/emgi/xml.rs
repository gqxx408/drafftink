//! # 符合 JY/T 1002-2012 的 XML 导出
//!
//! 将 [`EmgiDataset`](super::EmgiDataset) 导出为分层 XML：
//!
//! ```xml
//! <?xml version="1.0" encoding="UTF-8"?>
//! <EMGI standard="JY/T 1002-2012" generated="20260810120000">
//!   <Subset id="JCXS" name="学生管理信息子集">
//!     <Class id="JCXS0101" name="学生基本" references="JCTB0201">
//!       <Field id="JCXS010101" name="学生标识码" obligation="M" type="C">G123456789</Field>
//!     </Class>
//!   </Subset>
//! </EMGI>
//! ```
//!
//! 仅依赖 `quick-xml`（纯 Rust，无 C 依赖），所有属性值经 XML 转义。

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use std::io::Cursor;

use super::types::EmgiRecord;
use super::EmgiDataset;

/// 将数据集导出为符合标准的 XML 字符串。
pub fn to_xml(dataset: &EmgiDataset) -> anyhow::Result<String> {
    let mut buffer = Vec::new();
    {
        let mut writer = Writer::new(Cursor::new(&mut buffer));
        writer.write_event(Event::Decl(BytesDecl::new("1.0", None, None)))?;

        let mut root = BytesStart::new("EMGI");
        root.push_attribute(("standard", dataset.standard.as_str()));
        root.push_attribute(("generated", dataset.generated_at.as_str()));
        writer.write_event(Event::Start(root))?;

        // 按子集分组（保持出现顺序）
        let mut groups: Vec<(String, Vec<&EmgiRecord>)> = Vec::new();
        for rec in &dataset.records {
            match groups.iter_mut().find(|(s, _)| *s == rec.subset) {
                Some((_, recs)) => recs.push(rec),
                None => groups.push((rec.subset.clone(), vec![rec])),
            }
        }

        for (subset, recs) in &groups {
            let mut subset_el = BytesStart::new("Subset");
            subset_el.push_attribute(("id", subset.as_str()));
            subset_el.push_attribute((
                "name",
                super::SUBSET_NAMES
                    .iter()
                    .find(|(s, _)| *s == subset.as_str())
                    .map(|(_, n)| *n)
                    .unwrap_or(""),
            ));
            writer.write_event(Event::Start(subset_el))?;

            for rec in recs {
                let mut class_el = BytesStart::new("Class");
                class_el.push_attribute(("id", rec.class_id.as_str()));
                class_el.push_attribute(("name", rec.class_name.as_str()));
                let refs = rec.references.join(",");
                if !refs.is_empty() {
                    class_el.push_attribute(("references", refs.as_str()));
                }
                writer.write_event(Event::Start(class_el))?;

                for f in &rec.fields {
                    let mut field_el = BytesStart::new("Field");
                    field_el.push_attribute(("id", f.id.as_str()));
                    field_el.push_attribute(("name", f.name.as_str()));
                    field_el.push_attribute(("obligation", f.obligation.as_code()));
                    field_el.push_attribute(("type", f.data_type.as_code()));
                    if let Some(c) = f.code_ref.as_deref() {
                        field_el.push_attribute(("codeRef", c));
                    }
                    if let Some(s) = f.source.as_deref() {
                        field_el.push_attribute(("source", s));
                    }

                    match &f.value {
                        Some(v) => {
                            writer.write_event(Event::Start(field_el))?;
                            writer.write_event(Event::Text(BytesText::new(v.as_str())))?;
                            writer.write_event(Event::End(BytesEnd::new("Field")))?;
                        }
                        None => {
                            writer.write_event(Event::Empty(field_el))?;
                        }
                    }
                }

                writer.write_event(Event::End(BytesEnd::new("Class")))?;
            }

            writer.write_event(Event::End(BytesEnd::new("Subset")))?;
        }

        writer.write_event(Event::End(BytesEnd::new("EMGI")))?;
        writer.write_event(Event::Eof)?;
    }

    String::from_utf8(buffer).map_err(|e| anyhow::anyhow!("XML 编码失败: {e}"))
}
