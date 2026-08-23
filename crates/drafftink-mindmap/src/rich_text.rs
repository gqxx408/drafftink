//! 自定义富文本引擎
//!
//! 提供比 egui::RichText 更丰富的文本表示能力，
//! 支持多段不同样式的文本合并为一个段落。

use serde::{Deserialize, Serialize};

/// 文本样式片段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichTextSpan {
    /// 文本内容
    pub text: String,
    /// 字体大小
    pub font_size: f32,
    /// 是否加粗
    pub bold: bool,
    /// 是否斜体
    pub italic: bool,
    /// 是否下划线
    pub underline: bool,
    /// 文字颜色 RGBA（None 表示使用默认颜色）
    pub color: Option<[u8; 4]>,
    /// 背景色 RGBA（None 表示透明）
    pub background: Option<[u8; 4]>,
}

impl RichTextSpan {
    /// 创建普通文本片段
    pub fn plain(text: impl Into<String>, font_size: f32) -> Self {
        Self {
            text: text.into(),
            font_size,
            bold: false,
            italic: false,
            underline: false,
            color: None,
            background: None,
        }
    }

    /// 创建加粗文本片段
    pub fn bold(text: impl Into<String>, font_size: f32) -> Self {
        Self {
            text: text.into(),
            font_size,
            bold: true,
            italic: false,
            underline: false,
            color: None,
            background: None,
        }
    }
}

/// 富文本段落（由多个样式片段组成）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichText {
    /// 文本片段列表
    pub spans: Vec<RichTextSpan>,
    /// 默认字体大小
    pub default_font_size: f32,
}

impl RichText {
    /// 创建纯文本
    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            spans: vec![RichTextSpan::plain(text, 14.0)],
            default_font_size: 14.0,
        }
    }

    /// 创建空富文本
    pub fn empty() -> Self {
        Self {
            spans: Vec::new(),
            default_font_size: 14.0,
        }
    }

    /// 追加文本片段
    pub fn push(&mut self, span: RichTextSpan) {
        self.spans.push(span);
    }

    /// 获取纯文本表示（用于搜索、导出）
    pub fn to_plain_text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty() || self.spans.iter().all(|s| s.text.is_empty())
    }

    /// 转换为 egui::RichText 用于渲染
    pub fn to_egui_richtext(&self) -> egui::RichText {
        let plain = self.to_plain_text();
        egui::RichText::new(plain).size(self.default_font_size)
    }
}

// ── 测试 ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let rt = RichText::plain("Hello");
        assert_eq!(rt.to_plain_text(), "Hello");
        assert_eq!(rt.spans.len(), 1);
    }

    #[test]
    fn test_mixed_spans() {
        let mut rt = RichText::empty();
        rt.push(RichTextSpan::plain("Hello ", 14.0));
        rt.push(RichTextSpan::bold("World", 16.0));
        assert_eq!(rt.to_plain_text(), "Hello World");
        assert_eq!(rt.spans.len(), 2);
    }
}