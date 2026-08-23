//! 持久化层（RON 格式）
//!
//! 使用 RON（Rusty Object Notation）序列化整个 MindMapDoc。
//! 相比希沃的二进制 SaveInfo，RON 人类可读，调试时直接看文件即可。

use crate::types::MindMapDoc;

/// 将思维导图文档保存为 RON 字符串
///
/// # 示例
/// ```ignore
/// let doc = MindMapDoc::new("中心主题");
/// let ron_str = save_mindmap(&doc)?;
/// std::fs::write("mindmap.ron", ron_str)?;
/// ```
pub fn save_mindmap(doc: &MindMapDoc) -> anyhow::Result<String> {
    let pretty = ron::ser::PrettyConfig::new()
        .indentor("  ".to_string())
        .compact_arrays(true);
    let s = ron::ser::to_string_pretty(doc, pretty)?;
    Ok(s)
}

/// 从 RON 字符串加载思维导图文档
///
/// # 示例
/// ```ignore
/// let ron_str = std::fs::read_to_string("mindmap.ron")?;
/// let doc = load_mindmap(&ron_str)?;
/// ```
pub fn load_mindmap(s: &str) -> anyhow::Result<MindMapDoc> {
    let doc: MindMapDoc = ron::from_str(s)?;
    Ok(doc)
}

/// 保存到文件
pub fn save_to_file(doc: &MindMapDoc, path: &std::path::Path) -> anyhow::Result<()> {
    let content = save_mindmap(doc)?;
    std::fs::write(path, content)?;
    log::info!("[mindmap] 已保存到: {}", path.display());
    Ok(())
}

/// 从文件加载
pub fn load_from_file(path: &std::path::Path) -> anyhow::Result<MindMapDoc> {
    let content = std::fs::read_to_string(path)?;
    let doc = load_mindmap(&content)?;
    log::info!("[mindmap] 已加载: {} ({} 个节点)", path.display(), doc.nodes.len());
    Ok(doc)
}

// ── 测试 ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MapType, NodePosition};

    #[test]
    fn test_roundtrip() {
        let mut doc = MindMapDoc::new("中心主题");
        doc.add_child(doc.root_id, "左节点", NodePosition::Left)
            .unwrap();
        doc.add_child(doc.root_id, "右节点", NodePosition::Right)
            .unwrap();
        doc.map_type = MapType::MindMap;

        let ron_str = save_mindmap(&doc).unwrap();
        let loaded = load_mindmap(&ron_str).unwrap();

        assert_eq!(loaded.id, doc.id);
        assert_eq!(loaded.root_id, doc.root_id);
        assert_eq!(loaded.nodes.len(), doc.nodes.len());
        assert_eq!(loaded.map_type, doc.map_type);
    }

    #[test]
    fn test_roundtrip_mindly() {
        let mut doc = MindMapDoc::new("中心");
        doc.map_type = MapType::Mindly;
        doc.is_3d_mode = true;
        doc.add_child(doc.root_id, "A", NodePosition::Right)
            .unwrap();
        doc.add_child(doc.root_id, "B", NodePosition::Right)
            .unwrap();

        let ron_str = save_mindmap(&doc).unwrap();
        let loaded = load_mindmap(&ron_str).unwrap();

        assert_eq!(loaded.map_type, MapType::Mindly);
        assert!(loaded.is_3d_mode);
        assert_eq!(loaded.nodes.len(), 3);
    }

    #[test]
    fn test_pretty_format() {
        let doc = MindMapDoc::new("测试");
        let ron_str = save_mindmap(&doc).unwrap();

        // RON 格式应该包含缩进
        assert!(ron_str.contains('\n'));
        assert!(ron_str.contains("MindMap"));
    }
}