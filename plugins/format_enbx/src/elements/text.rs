//! Text-element data structures for .enbx parsing.

/// ARGB colour parsed from Seewo's `ColorBrush` hex format.
///
/// Format: `#AARRGGBB` — leading `#`, then 8 hex digits (alpha first).
#[derive(Debug, Clone, PartialEq)]
pub struct ArgbColor {
    pub a: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ArgbColor {
    /// Parse `#AARRGGBB`, e.g. `#FF000000` → black.
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        let hex = hex.trim();
        if hex.len() != 9 || !hex.starts_with('#') {
            return Err(format!("Invalid ARGB hex: '{}'", hex));
        }
        let bytes = &hex[1..];
        let a = u8::from_str_radix(&bytes[0..2], 16)
            .map_err(|e| format!("Bad alpha: {}", e))?;
        let r = u8::from_str_radix(&bytes[2..4], 16)
            .map_err(|e| format!("Bad red: {}", e))?;
        let g = u8::from_str_radix(&bytes[4..6], 16)
            .map_err(|e| format!("Bad green: {}", e))?;
        let b = u8::from_str_radix(&bytes[6..8], 16)
            .map_err(|e| format!("Bad blue: {}", e))?;
        Ok(Self { a, r, g, b })
    }

    /// Convert to drafftink-internal RGBA layout.
    pub fn to_rgba(&self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// A parsed text element from a Seewo Slide XML.
#[derive(Debug, Clone, PartialEq)]
pub struct TextElement {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub is_locked: bool,
    pub background: ArgbColor,
    pub content: String,
    pub font_size: f32,
    pub font_family: String,
    pub font_weight: String,
    pub foreground: ArgbColor,
}

impl Default for TextElement {
    fn default() -> Self {
        Self {
            id: String::new(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            rotation: 0.0,
            is_locked: false,
            background: ArgbColor { a: 0, r: 255, g: 255, b: 255 },
            content: String::new(),
            font_size: 0.0,
            font_family: String::new(),
            font_weight: String::new(),
            foreground: ArgbColor { a: 255, r: 0, g: 0, b: 0 },
        }
    }
}
