//! 公共矩形交互组件：9 宫格命中检测 + 8 方向缩放 + 内部拖拽移动。
//!
//! 视频叠加层与图片元素共用同一套指针交互逻辑，避免重复代码（DRY）。
//! 设计要点（与 egui 0.29 手动命中检测一致）：
//!
//! - 不依赖 `Ui` / `Context::interact`，全部通过 `ctx.pointer_interact_pos()`
//!   与 `ctx.input(|i| i.pointer.*)` 手动实现。
//! - 命中优先级：**角 > 边 > 内部**（与 `draw_video_overlay` 原有顺序一致）。
//! - 宽高独立变化，不锁定比例（对边锚定）。
//! - 最小尺寸保护：`width >= MIN_W` 且 `height >= MIN_H`，违规则回退到拖拽前矩形。
//! - 全局唯一拖拽守卫 `active: &mut Option<(egui::Id, HitZone)>`：同一时刻只有一个
//!   矩形（视频或图片）可被拖拽，跨实例互斥。
//!
//! `RectInteraction` 本身是无状态的「单帧」辅助结构：每帧用当前屏幕矩形
//! `RectInteraction::new(id, rect)` 创建，拖拽的跨帧持续由宿主层 `active` 守卫持有。

use egui::{Color32, Context, CursorIcon, Id, Painter, Pos2, Rect, Stroke, Vec2};

/// 9 宫格命中区域（不含视频专属的暂停/静音按钮，那些由调用方单独处理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitZone {
    /// 左上角（宽高同时变，右下角不动）。
    TopLeft,
    /// 上边（高度变，下边缘不动）。
    Top,
    /// 右上角（宽高同时变，左下角不动）。
    TopRight,
    /// 左边（宽度变，右边缘不动）。
    Left,
    /// 右边（宽度变，左边缘不动）。
    Right,
    /// 左下角（宽高同时变，右上角不动）。
    BottomLeft,
    /// 下边（高度变，上边缘不动）。
    Bottom,
    /// 右下角（宽高同时变，左上角不动）。
    BottomRight,
    /// 内部（整体移动）。
    Move,
    /// 矩形之外（不交互）。
    Outside,
}

/// 单帧矩形交互状态。
pub struct RectInteraction {
    /// 本实例的稳定标识（跨帧一致，用于全局拖拽守卫的去重）。
    pub id: Id,
    /// 当前屏幕矩形（已包含本帧拖拽增量）。
    pub rect: Rect,
    /// 悬停区域（非拖拽时用于高亮 / 光标）。
    pub hovered: Option<HitZone>,
    /// 本实例正在进行的拖拽区域（仅当它是全局守卫的持有者时 Some）。
    pub active_drag: Option<HitZone>,
}

impl RectInteraction {
    /// 边框 / 边感应宽度（px）。
    pub const EDGE: f32 = 6.0;
    /// 角命中区边长（px）。
    pub const CORNER: f32 = 12.0;
    /// 最小宽度保护（拖到此处即停止缩小）。
    pub const MIN_W: f32 = 40.0;
    /// 最小高度保护。
    pub const MIN_H: f32 = 30.0;

    /// 用稳定 id 与当前屏幕矩形创建单帧交互状态。
    pub fn new(id: Id, rect: Rect) -> Self {
        Self {
            id,
            rect,
            hovered: None,
            active_drag: None,
        }
    }

    /// 9 宫格命中检测：给定指针位置，返回命中的区域。
    ///
    /// 优先级：角 > 边 > 内部 > 外部，与 `draw_video_overlay` 原有顺序一致。
    pub fn hit_test(&self, pos: Pos2) -> HitZone {
        let r = self.rect;
        let inner = r.shrink(Self::EDGE);
        let top = Rect::from_min_max(r.left_top(), Pos2::new(r.right(), inner.top()));
        let bottom = Rect::from_min_max(Pos2::new(r.left(), inner.bottom()), r.right_bottom());
        let left = Rect::from_min_max(r.left_top(), Pos2::new(inner.left(), r.bottom()));
        let right = Rect::from_min_max(Pos2::new(inner.right(), r.top()), r.right_bottom());
        let cs = Vec2::new(Self::CORNER, Self::CORNER);
        let top_left_corner = Rect::from_min_size(r.left_top(), cs);
        let top_right_corner =
            Rect::from_min_size(r.right_top() - Vec2::new(Self::CORNER, 0.0), cs);
        let bottom_left_corner =
            Rect::from_min_size(r.left_bottom() - Vec2::new(0.0, Self::CORNER), cs);
        let bottom_right_corner = Rect::from_min_size(r.right_bottom() - cs, cs);

        if top_left_corner.contains(pos) {
            HitZone::TopLeft
        } else if bottom_right_corner.contains(pos) {
            HitZone::BottomRight
        } else if top_right_corner.contains(pos) {
            HitZone::TopRight
        } else if bottom_left_corner.contains(pos) {
            HitZone::BottomLeft
        } else if top.contains(pos) {
            HitZone::Top
        } else if bottom.contains(pos) {
            HitZone::Bottom
        } else if left.contains(pos) {
            HitZone::Left
        } else if right.contains(pos) {
            HitZone::Right
        } else if inner.contains(pos) {
            HitZone::Move
        } else {
            HitZone::Outside
        }
    }

    /// 纯函数：把某一拖拽区域 + 指针增量应用到矩形上（宽高独立，不锁比例）。
    ///
    /// 用于单元测试，也被 [`RectInteraction::update`] 调用。
    pub fn apply_drag(rect: Rect, zone: HitZone, delta: Vec2) -> Rect {
        let mut r = rect;
        match zone {
            HitZone::Right => r.max.x += delta.x,
            HitZone::Left => r.min.x += delta.x,
            HitZone::Bottom => r.max.y += delta.y,
            HitZone::Top => r.min.y += delta.y,
            HitZone::BottomRight => {
                r.max.x += delta.x;
                r.max.y += delta.y;
            }
            HitZone::TopLeft => {
                r.min.x += delta.x;
                r.min.y += delta.y;
            }
            HitZone::BottomLeft => {
                r.min.x += delta.x;
                r.max.y += delta.y;
            }
            HitZone::TopRight => {
                r.max.x += delta.x;
                r.min.y += delta.y;
            }
            HitZone::Move => r = r.translate(delta),
            HitZone::Outside => {}
        }
        r
    }

    /// 纯函数：最小尺寸保护——候选矩形若任一维不足最小尺寸，则回退到原始矩形。
    ///
    /// 用于单元测试，也被 [`RectInteraction::update`] 调用。
    pub fn revert_if_too_small(original: Rect, candidate: Rect) -> Rect {
        if candidate.width() < Self::MIN_W || candidate.height() < Self::MIN_H {
            original
        } else {
            candidate
        }
    }

    /// 单帧更新：读取当前指针状态，计算命中区域、认领 / 应用拖拽，返回新矩形。
    ///
    /// - `active` 为全局唯一拖拽守卫（`None` 表示当前无拖拽）。仅当 `pressed` 且守卫空闲时
    ///   本实例认领；若守卫已被其它实例持有，本实例不响应，保证跨实例互斥。
    /// - 返回 `Some(new_rect)` 表示本实例刚完成一帧拖拽，调用方应把它存为新的屏幕矩形
    ///   （视频存 `user_rect`，图片存 `user_rect`）；否则返回 `None`。
    pub fn update(&mut self, ctx: &Context, active: &mut Option<(Id, HitZone)>) -> Option<Rect> {
        let pos = ctx.pointer_interact_pos();
        let pressed = ctx.input(|i| i.pointer.primary_pressed());
        let down = ctx.input(|i| i.pointer.primary_down());
        let delta = ctx.input(|i| i.pointer.delta());

        let region = match pos {
            Some(p) => self.hit_test(p),
            None => HitZone::Outside,
        };

        // 按下且守卫空闲 → 认领（仅对非外部区域）。
        if pressed && active.is_none() && region != HitZone::Outside {
            *active = Some((self.id, region));
        }

        let is_mine = active.as_ref().map(|(i, _)| *i == self.id).unwrap_or(false);

        let mut result = None;
        if is_mine {
            if let Some((_, zone)) = *active {
                if down {
                    // 拖拽进行中：应用位移（受最小尺寸保护），并更新本实例拖拽区。
                    let mut r = Self::apply_drag(self.rect, zone, delta);
                    r = Self::revert_if_too_small(self.rect, r);
                    self.rect = r;
                    self.active_drag = Some(zone);
                    result = Some(r);
                } else {
                    // 指针已释放：归还全局拖拽守卫，使其它实例可在下一帧认领拖拽；
                    // 否则守卫会一直被本实例占用，导致其它元素再也无法被拖拽 / 选中。
                    *active = None;
                    self.active_drag = None;
                }
            }
        } else {
            self.active_drag = None;
            self.hovered = if region == HitZone::Outside {
                None
            } else {
                Some(region)
            };
        }

        // 光标：拖拽中跟随拖拽区，否则跟随悬停区。
        let eff = if is_mine {
            self.active_drag
        } else if region == HitZone::Outside {
            None
        } else {
            Some(region)
        };
        if let Some(c) = Self::cursor_for(eff) {
            ctx.set_cursor_icon(c);
        }

        result
    }

    /// 绘制边框 + 四角 grip + 边高亮（悬停 / 拖拽变亮蓝）。
    ///
    /// 调用方负责在 `painter` 上先绘制纹理（视频 / 图片），再调用本方法叠加交互装饰。
    pub fn draw_overlay(&self, painter: &Painter) {
        let is_hot = self.hovered.is_some() || self.active_drag.is_some();
        let border_color = if is_hot {
            Color32::from_rgb(0, 150, 255)
        } else {
            Color32::from_rgba_unmultiplied(150, 150, 150, 180)
        };
        painter.rect_stroke(self.rect, 0.0, Stroke::new(2.0_f32, border_color));

        // 四角 grip（8×8 亮蓝方块）。
        let draw_corner = |painter: &Painter, c: Pos2| {
            painter.rect_filled(
                Rect::from_center_size(c, Vec2::new(8.0, 8.0)),
                1.0,
                Color32::from_rgb(0, 150, 255),
            );
        };
        for z in [
            HitZone::TopLeft,
            HitZone::TopRight,
            HitZone::BottomLeft,
            HitZone::BottomRight,
        ] {
            if self.hovered == Some(z) || self.active_drag == Some(z) {
                let c = match z {
                    HitZone::TopLeft => self.rect.left_top(),
                    HitZone::TopRight => self.rect.right_top(),
                    HitZone::BottomLeft => self.rect.left_bottom(),
                    HitZone::BottomRight => self.rect.right_bottom(),
                    _ => unreachable!(),
                };
                draw_corner(painter, c);
            }
        }

        // 边高亮（3px 亮蓝覆盖默认 2px 灰线）。
        let draw_edge = |painter: &Painter, a: Pos2, b: Pos2| {
            painter.line_segment([a, b], Stroke::new(3.0_f32, Color32::from_rgb(0, 150, 255)));
        };
        for z in [HitZone::Top, HitZone::Bottom, HitZone::Left, HitZone::Right] {
            if self.hovered == Some(z) || self.active_drag == Some(z) {
                let (a, b) = match z {
                    HitZone::Top => (self.rect.left_top(), self.rect.right_top()),
                    HitZone::Bottom => (self.rect.left_bottom(), self.rect.right_bottom()),
                    HitZone::Left => (self.rect.left_top(), self.rect.left_bottom()),
                    HitZone::Right => (self.rect.right_top(), self.rect.right_bottom()),
                    _ => unreachable!(),
                };
                draw_edge(painter, a, b);
            }
        }
    }

    /// 由命中区域映射到 egui 光标图标。
    fn cursor_for(zone: Option<HitZone>) -> Option<CursorIcon> {
        match zone {
            Some(HitZone::TopLeft) | Some(HitZone::BottomRight) => Some(CursorIcon::ResizeNwSe),
            Some(HitZone::TopRight) | Some(HitZone::BottomLeft) => Some(CursorIcon::ResizeNeSw),
            Some(HitZone::Left) | Some(HitZone::Right) => Some(CursorIcon::ResizeHorizontal),
            Some(HitZone::Top) | Some(HitZone::Bottom) => Some(CursorIcon::ResizeVertical),
            Some(HitZone::Move) => Some(CursorIcon::Move),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 100.0))
    }

    #[test]
    fn hit_top_left_corner() {
        let i = RectInteraction::new(Id::new("t"), base());
        assert_eq!(i.hit_test(Pos2::new(2.0, 2.0)), HitZone::TopLeft);
    }

    #[test]
    fn hit_bottom_right_corner() {
        let i = RectInteraction::new(Id::new("t"), base());
        assert_eq!(i.hit_test(Pos2::new(198.0, 98.0)), HitZone::BottomRight);
    }

    #[test]
    fn hit_top_right_corner() {
        let i = RectInteraction::new(Id::new("t"), base());
        assert_eq!(i.hit_test(Pos2::new(198.0, 2.0)), HitZone::TopRight);
    }

    #[test]
    fn hit_bottom_left_corner() {
        let i = RectInteraction::new(Id::new("t"), base());
        assert_eq!(i.hit_test(Pos2::new(2.0, 98.0)), HitZone::BottomLeft);
    }

    #[test]
    fn hit_top_edge() {
        let i = RectInteraction::new(Id::new("t"), base());
        assert_eq!(i.hit_test(Pos2::new(100.0, 2.0)), HitZone::Top);
    }

    #[test]
    fn hit_left_edge() {
        let i = RectInteraction::new(Id::new("t"), base());
        assert_eq!(i.hit_test(Pos2::new(2.0, 50.0)), HitZone::Left);
    }

    #[test]
    fn hit_right_edge() {
        let i = RectInteraction::new(Id::new("t"), base());
        assert_eq!(i.hit_test(Pos2::new(198.0, 50.0)), HitZone::Right);
    }

    #[test]
    fn hit_bottom_edge() {
        let i = RectInteraction::new(Id::new("t"), base());
        assert_eq!(i.hit_test(Pos2::new(100.0, 98.0)), HitZone::Bottom);
    }

    #[test]
    fn hit_inner_move() {
        let i = RectInteraction::new(Id::new("t"), base());
        assert_eq!(i.hit_test(Pos2::new(100.0, 50.0)), HitZone::Move);
    }

    #[test]
    fn hit_outside() {
        let i = RectInteraction::new(Id::new("t"), base());
        assert_eq!(i.hit_test(Pos2::new(250.0, 150.0)), HitZone::Outside);
    }

    #[test]
    fn apply_drag_right_changes_only_width() {
        let r = RectInteraction::apply_drag(base(), HitZone::Right, Vec2::new(10.0, 7.0));
        assert_eq!(r.min.x, 0.0);
        assert_eq!(r.max.x, 210.0);
        assert_eq!(r.min.y, 0.0);
        assert_eq!(r.max.y, 100.0);
    }

    #[test]
    fn apply_drag_bottom_right_changes_both() {
        let r = RectInteraction::apply_drag(base(), HitZone::BottomRight, Vec2::new(10.0, 20.0));
        assert_eq!(r.max.x, 210.0);
        assert_eq!(r.max.y, 120.0);
    }

    #[test]
    fn apply_drag_move_translates() {
        let r = RectInteraction::apply_drag(base(), HitZone::Move, Vec2::new(5.0, 7.0));
        assert_eq!(r.min, Pos2::new(5.0, 7.0));
        assert_eq!(r.max, Pos2::new(205.0, 107.0));
    }

    #[test]
    fn apply_drag_top_left_moves_top_left_corner() {
        let r = RectInteraction::apply_drag(base(), HitZone::TopLeft, Vec2::new(-10.0, -5.0));
        assert_eq!(r.min, Pos2::new(-10.0, -5.0));
        assert_eq!(r.max, Pos2::new(200.0, 100.0));
    }

    #[test]
    fn revert_when_too_small() {
        let orig = base();
        let small = Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 10.0));
        assert_eq!(RectInteraction::revert_if_too_small(orig, small), orig);
    }

    #[test]
    fn keep_when_within_min() {
        let orig = base();
        let ok = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0));
        assert_eq!(RectInteraction::revert_if_too_small(orig, ok), ok);
    }

    #[test]
    fn min_width_boundary_kept() {
        let orig = base();
        let at_min_w = Rect::from_min_size(Pos2::ZERO, Vec2::new(RectInteraction::MIN_W, 100.0));
        assert_eq!(
            RectInteraction::revert_if_too_small(orig, at_min_w),
            at_min_w
        );
    }
}
