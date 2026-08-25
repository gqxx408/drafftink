use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolType {
    Pen,
    Highlighter,
    Eraser,
}

impl Default for ToolType {
    fn default() -> Self {
        Self::Pen
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InkStroke {
    pub id: Uuid,
    pub tool: ToolType,
    pub color: [u8; 4],
    pub thickness: f32,
    pub points: Vec<(f32, f32)>,
    pub timestamp_ms: u64,
}

impl InkStroke {
    pub fn new(tool: ToolType, rgb: [u8; 3], alpha: u8, thickness: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            tool,
            color: [rgb[0], rgb[1], rgb[2], alpha],
            thickness,
            points: Vec::new(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        self.points.len() >= 2
    }

    #[allow(dead_code)]
    pub fn distance_to(&self, x: f32, y: f32) -> f32 {
        self.points
            .iter()
            .map(|(px, py)| {
                let dx = px - x;
                let dy = py - y;
                (dx * dx + dy * dy).sqrt()
            })
            .fold(f32::MAX, f32::min)
    }

    /// Remove points within `radius` of (x, y). Returns how many were removed.
    pub fn erase_near(&mut self, x: f32, y: f32, radius: f32) -> usize {
        let r2 = radius * radius;
        let before = self.points.len();
        self.points.retain(|(px, py)| {
            let dx = px - x;
            let dy = py - y;
            (dx * dx + dy * dy) > r2
        });
        before - self.points.len()
    }
}

/// Merge strokes of the same colour/thickness/tool that are nearby in space and time.
pub fn merge_adjacent_strokes(strokes: &mut Vec<InkStroke>) {
    let mut i = 0;
    while i < strokes.len().saturating_sub(1) {
        let can_merge = {
            let a = &strokes[i];
            let b = &strokes[i + 1];
            a.color == b.color
                && (a.thickness - b.thickness).abs() < 0.1
                && std::mem::discriminant(&a.tool) == std::mem::discriminant(&b.tool)
                && b.timestamp_ms.saturating_sub(a.timestamp_ms) < 500
                && {
                    if let (Some(&(ax, ay)), Some(&(bx, by))) = (a.points.last(), b.points.first())
                    {
                        let dx = ax - bx;
                        let dy = ay - by;
                        (dx * dx + dy * dy).sqrt() < 10.0
                    } else {
                        false
                    }
                }
        };
        if can_merge {
            let b = strokes.remove(i + 1);
            strokes[i].points.extend(b.points);
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_new_has_valid_id() {
        let s = InkStroke::new(ToolType::Pen, [0, 0, 0], 255, 2.0);
        assert!(!s.id.is_nil());
    }

    #[test]
    fn stroke_is_valid_requires_two_points() {
        let s = InkStroke {
            id: Uuid::new_v4(),
            tool: ToolType::Pen,
            color: [0, 0, 0, 255],
            thickness: 2.0,
            points: vec![(0.0, 0.0)],
            timestamp_ms: 0,
        };
        assert!(!s.is_valid());

        let s2 = InkStroke {
            id: Uuid::new_v4(),
            tool: ToolType::Pen,
            color: [0, 0, 0, 255],
            thickness: 2.0,
            points: vec![(0.0, 0.0), (10.0, 10.0)],
            timestamp_ms: 0,
        };
        assert!(s2.is_valid());
    }

    #[test]
    fn stroke_distance_to() {
        let s = InkStroke {
            id: Uuid::new_v4(),
            tool: ToolType::Pen,
            color: [0, 0, 0, 255],
            thickness: 2.0,
            points: vec![(0.0, 0.0), (10.0, 0.0), (5.0, 5.0)],
            timestamp_ms: 0,
        };
        let d = s.distance_to(3.0, 4.0);
        // closest point is (5,5): dx=2, dy=1 → sqrt(5) ≈ 2.236
        assert!((d - 2.236).abs() < 0.1);
    }
}
