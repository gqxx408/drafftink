use egui::{Context, Rect, Sense};

use super::spatial::Quadtree;
use super::stroke::{InkStroke, ToolType};

#[derive(Default)]
pub struct AnnotationInput {
    min_distance: f32,
    last_point_time: f64,
    enabled: bool,
}

impl AnnotationInput {
    pub fn new() -> Self {
        Self {
            min_distance: 2.0,
            last_point_time: 0.0,
            enabled: true,
        }
    }

    #[allow(dead_code)]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Process one frame of input.
    /// Returns `(completed_stroke, erased_points_count)`.
    ///
    /// `spatial` 为笔迹包围盒索引，橡皮据此只对命中区域内的候选笔迹做距离测试。
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        ctx: &Context,
        screen_rect: Rect,
        current_stroke: &mut Option<InkStroke>,
        strokes: &mut Vec<InkStroke>,
        tool: &ToolType,
        color: &[u8; 4],
        thickness: f32,
        spatial: &Quadtree,
    ) -> (Option<InkStroke>, usize) {
        if !self.enabled {
            return (None, 0);
        }

        // Allocate interactive rect — exclude bottom 46px for toolbar
        let input_rect =
            egui::Rect::from_min_max(screen_rect.min, screen_rect.max - egui::vec2(0.0, 46.0));
        let response = {
            let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("annot_input"));
            let mut ui = egui::Ui::new(
                ctx.clone(),
                layer,
                egui::Id::new("annot_input_ui"),
                egui::UiBuilder::new().max_rect(input_rect),
            );
            ui.allocate_rect(input_rect, Sense::click_and_drag())
        };

        let pos = match response.hover_pos() {
            Some(p) => p,
            None => return (None, 0),
        };

        // ── Eraser: pixel-level point removal ──────────────────
        if *tool == ToolType::Eraser {
            if response.dragged() || response.clicked() {
                let erase_radius = thickness.max(5.0);
                let mut erased = 0usize;

                // 只对橡皮邻域内的候选笔迹做逐点距离测试。
                // 旧实现每帧 `drain(..)` 全表并重建 Vec，等于把整块板书搬一遍。
                let candidates = spatial.query_circle(pos, erase_radius);
                let mut shrunk = false;
                for &i in &candidates {
                    if let Some(stroke) = strokes.get_mut(i) {
                        let n = stroke.erase_near(pos.x, pos.y, erase_radius);
                        if n > 0 {
                            erased += n;
                            shrunk |= stroke.points.len() < 2;
                        }
                    }
                }
                // 仅在确有笔迹被擦到只剩不足两点时才整表压缩。
                if shrunk {
                    strokes.retain(|s| s.points.len() >= 2);
                }
                *current_stroke = None;
                return (None, erased);
            }
            return (None, 0);
        }

        // ── Pen / Highlighter ──────────────────────────────────
        let now = ctx.input(|i| i.time);

        if response.dragged() {
            let time_ok = (now - self.last_point_time) > 0.008;

            if let Some(ref mut stroke) = *current_stroke {
                if let Some(&(lx, ly)) = stroke.points.last() {
                    let dx = pos.x - lx;
                    let dy = pos.y - ly;
                    let dist = (dx * dx + dy * dy).sqrt();
                    let min_dist = match tool {
                        ToolType::Highlighter => (thickness * 0.3).max(2.0),
                        _ => self.min_distance,
                    };
                    if dist >= min_dist && time_ok {
                        stroke.points.push((pos.x, pos.y));
                        self.last_point_time = now;
                    }
                }
            } else {
                let mut stroke =
                    InkStroke::new(*tool, [color[0], color[1], color[2]], color[3], thickness);
                stroke.points.push((pos.x, pos.y));
                *current_stroke = Some(stroke);
                self.last_point_time = now;
            }
            (None, 0)
        } else if !response.dragged() && current_stroke.is_some() {
            if let Some(mut stroke) = current_stroke.take() {
                if stroke.points.len() >= 2 {
                    if let Some(p) = response.hover_pos() {
                        stroke.points.push((p.x, p.y));
                    }
                    (Some(stroke), 0)
                } else {
                    (None, 0)
                }
            } else {
                (None, 0)
            }
        } else {
            (None, 0)
        }
    }
}
