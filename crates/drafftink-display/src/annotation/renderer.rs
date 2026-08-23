//! 板书笔迹渲染。
//!
//! ## 提交策略
//!
//! 早期实现对每条笔迹逐段调用 `painter.line_segment()`，并在**每个点**再补一个
//! `circle_filled()` 来伪造圆角接头。一条 N 点笔迹因此每帧产生 `2N` 个独立
//! `Shape`——每个 Shape 都要单独走一次网格化并占用一段顶点缓冲。板书写满一屏时
//! 这个数量级会直接压垮 CPU 端的 tessellation。
//!
//! 现在改为：整条折线交给 `Shape::line` 一次性提交（1 个 Shape），仅在粗线时额外
//! 补两个端点圆做圆头收尾。单条笔迹的 Shape 数从 `2N` 降到 `1~3`，与点数解耦。
//!
//! 另外，所有 Shape 先攒进复用缓冲，末尾一次 `painter.extend()` 批量提交，
//! 避免逐个 `add()` 反复加锁 layer 的形状列表。

use egui::{Color32, Context, Id, LayerId, Order, Pos2, Rect, Shape, Stroke};

use super::spatial::stroke_bounds;
use super::stroke::{InkStroke, ToolType};

/// 超过该线宽才补端点圆做圆头，细线看不出差别，省掉这两个 Shape。
const ROUND_CAP_MIN_THICKNESS: f32 = 4.0;

pub struct AnnotationRenderer {
    layer_id: LayerId,
    /// 预分配的屏幕点缓冲，逐笔迹复用。
    point_buf: Vec<Pos2>,
    /// 一帧内所有笔迹的 Shape 攒齐后一次性提交。
    shape_buf: Vec<Shape>,
    /// 上一帧实际绘制的笔迹数（诊断用）。
    pub last_drawn: usize,
    /// 上一帧被视口剔除 / 跳过的笔迹数（诊断用）。
    pub last_culled: usize,
}

impl Default for AnnotationRenderer {
    fn default() -> Self {
        Self {
            layer_id: LayerId::new(Order::Middle, Id::new("annotation_layer")),
            point_buf: Vec::new(),
            shape_buf: Vec::new(),
            last_drawn: 0,
            last_culled: 0,
        }
    }
}

impl AnnotationRenderer {
    /// 渲染已完成笔迹与进行中的笔迹。
    ///
    /// `visible` 为四叉树查询出的可见笔迹下标；传 `None` 表示回退到全量遍历
    /// （索引尚未建立时的兜底路径）。
    pub fn render(
        &mut self,
        ctx: &Context,
        strokes: &[InkStroke],
        current: Option<&InkStroke>,
        visible: Option<&[usize]>,
    ) {
        let painter = ctx.layer_painter(self.layer_id);
        let clip = painter.clip_rect();

        self.shape_buf.clear();
        let mut drawn = 0usize;

        match visible {
            Some(indices) => {
                for &i in indices {
                    if let Some(s) = strokes.get(i) {
                        if self.push_stroke(s, clip) {
                            drawn += 1;
                        }
                    }
                }
            }
            None => {
                for s in strokes {
                    if self.push_stroke(s, clip) {
                        drawn += 1;
                    }
                }
            }
        }

        // 进行中的笔迹不在索引里，单独提交。
        if let Some(s) = current {
            self.push_stroke(s, clip);
        }

        self.last_drawn = drawn;
        self.last_culled = strokes.len().saturating_sub(drawn);

        painter.extend(self.shape_buf.drain(..));
    }

    /// 把一条笔迹转成 Shape 推入提交缓冲。返回是否真的绘制了。
    fn push_stroke(&mut self, stroke: &InkStroke, clip: Rect) -> bool {
        if stroke.points.len() < 2 {
            return false;
        }

        let (color, thickness) = match stroke.tool {
            ToolType::Highlighter => {
                let c = stroke.color;
                (
                    Color32::from_rgba_premultiplied(c[0], c[1], c[2], c[3]),
                    stroke.thickness.max(10.0),
                )
            }
            ToolType::Pen => {
                let c = stroke.color;
                (
                    Color32::from_rgba_premultiplied(c[0], c[1], c[2], c[3]),
                    stroke.thickness,
                )
            }
            // 橡皮不是可绘制笔迹，只是擦除动作。
            ToolType::Eraser => return false,
        };

        // 视口剔除：完全落在裁剪区之外的笔迹直接跳过，不做任何顶点计算。
        if let Some(bbox) = stroke_bounds(stroke) {
            if !clip.intersects(bbox) {
                return false;
            }
        }

        self.point_buf.clear();
        self.point_buf.reserve(stroke.points.len());
        for &(x, y) in &stroke.points {
            self.point_buf.push(Pos2::new(x, y));
        }

        let stk = Stroke::new(thickness, color);

        // 粗线补端点圆，保留原先的圆头观感；细线省略（肉眼无差）。
        if thickness >= ROUND_CAP_MIN_THICKNESS {
            let r = thickness * 0.5;
            if let (Some(&first), Some(&last)) =
                (self.point_buf.first(), self.point_buf.last())
            {
                self.shape_buf.push(Shape::circle_filled(first, r, color));
                self.shape_buf.push(Shape::circle_filled(last, r, color));
            }
        }

        self.shape_buf
            .push(Shape::line(self.point_buf.clone(), stk));
        true
    }

    /// 橡皮光标预览（灰色圆圈标示擦除半径）。
    pub fn render_cursor_preview(&self, ctx: &Context, tool: &ToolType, thickness: f32) {
        if *tool != ToolType::Eraser {
            return;
        }
        let pos = ctx.input(|i| i.pointer.hover_pos());
        if let Some(p) = pos {
            let preview = ctx.layer_painter(LayerId::new(
                Order::Foreground,
                Id::new("cursor_preview"),
            ));
            let radius = thickness.max(5.0);
            preview.circle_stroke(
                p,
                radius,
                Stroke::new(1.5, Color32::from_rgba_premultiplied(100, 100, 100, 180)),
            );
        }
    }
}
