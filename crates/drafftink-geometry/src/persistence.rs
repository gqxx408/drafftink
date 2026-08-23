//! 持久化 — JSON 序列化 / 反序列化
//!
//! 所有 Def 结构体已派生 `Serialize` / `Deserialize`，
//! 可直接序列化为 JSON 格式保存/加载。

use std::path::Path;

use crate::definitions::GeometryDoc;

/// 保存几何文档到 JSON 文件
pub fn save_to_json(doc: &GeometryDoc, path: impl AsRef<Path>) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(doc)?;
    std::fs::write(path.as_ref(), json)?;
    Ok(())
}

/// 从 JSON 文件加载几何文档
pub fn load_from_json(path: impl AsRef<Path>) -> anyhow::Result<GeometryDoc> {
    let json = std::fs::read_to_string(path.as_ref())?;
    let doc: GeometryDoc = serde_json::from_str(&json)?;
    Ok(doc)
}

/// 序列化为 JSON 字符串
pub fn to_json_string(doc: &GeometryDoc) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(doc)?)
}

/// 从 JSON 字符串反序列化
pub fn from_json_string(json: &str) -> anyhow::Result<GeometryDoc> {
    Ok(serde_json::from_str(json)?)
}

// ── 测试 ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definitions::Point2D;

    #[test]
    fn test_serialize_deserialize() {
        let mut doc = GeometryDoc::new();
        let p1 = doc.add_free_point(Point2D::new(10.0, 20.0));
        let p2 = doc.add_free_point(Point2D::new(30.0, 40.0));
        doc.add_line(p1, p2);
        doc.add_circle(p1, 5.0);

        let json = to_json_string(&doc).unwrap();
        let loaded = from_json_string(&json).unwrap();

        assert_eq!(loaded.points.len(), 2);
        assert_eq!(loaded.lines.len(), 1);
        assert_eq!(loaded.circles.len(), 1);
    }

    #[test]
    fn test_save_load_file() {
        let mut doc = GeometryDoc::new();
        doc.add_free_point(Point2D::new(1.0, 2.0));

        let tmp = std::env::temp_dir().join("test_geometry.json");
        save_to_json(&doc, &tmp).unwrap();
        let loaded = load_from_json(&tmp).unwrap();

        assert_eq!(loaded.points.len(), 1);

        // 清理
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_empty_doc() {
        let doc = GeometryDoc::new();
        let json = to_json_string(&doc).unwrap();
        let loaded = from_json_string(&json).unwrap();
        assert_eq!(loaded.points.len(), 0);
    }
}
