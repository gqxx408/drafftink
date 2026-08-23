//! Camera module — handles world-to-screen coordinate transforms,
//! zoom, pan, and grid-line calculations.

use egui::Pos2;

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

/// A 2D camera that maps an infinite world coordinate system onto a
/// finite screen viewport.
#[derive(Debug, Clone)]
pub struct Camera {
    /// Zoom factor, clamped to `[MIN_ZOOM, MAX_ZOOM]`.
    pub zoom: f32,
    /// World-space coordinates of the camera's focal point (maps to
    /// viewport centre).
    pub offset: [f32; 2],
    /// Current viewport dimensions `[width, height]` in screen pixels.
    pub viewport: [f32; 2],
}

pub const MIN_ZOOM: f32 = 0.05;
pub const MAX_ZOOM: f32 = 8.0;

impl Default for Camera {
    fn default() -> Self {
        Self {
            zoom: 0.8,
            offset: [0.0, 0.0],
            viewport: [800.0, 600.0],
        }
    }
}

impl Camera {
    pub fn new(viewport: [f32; 2]) -> Self {
        Self {
            zoom: 0.8,
            offset: [0.0, 0.0],
            viewport,
        }
    }

    // ------------------------------------------------------------------
    // Coordinate transforms
    // ------------------------------------------------------------------

    /// Convert a world-space point to screen-space pixel coordinates.
    pub fn world_to_screen(&self, world: [f32; 2]) -> Pos2 {
        let sx = (world[0] - self.offset[0]) * self.zoom + self.viewport[0] * 0.5;
        let sy = (world[1] - self.offset[1]) * self.zoom + self.viewport[1] * 0.5;
        Pos2::new(sx, sy)
    }

    /// Convert a world-space size to screen-space size.
    pub fn world_size_to_screen(&self, size: [f32; 2]) -> [f32; 2] {
        [size[0] * self.zoom, size[1] * self.zoom]
    }

    /// Convert a screen-space pixel coordinate to world-space.
    pub fn screen_to_world(&self, screen: Pos2) -> [f32; 2] {
        let wx = (screen.x - self.viewport[0] * 0.5) / self.zoom + self.offset[0];
        let wy = (screen.y - self.viewport[1] * 0.5) / self.zoom + self.offset[1];
        [wx, wy]
    }

    // ------------------------------------------------------------------
    // Navigation
    // ------------------------------------------------------------------

    /// Pan the camera by a screen-space delta.
    pub fn pan_screen(&mut self, delta: [f32; 2]) {
        self.offset[0] -= delta[0] / self.zoom;
        self.offset[1] -= delta[1] / self.zoom;
    }

    /// Pan the camera by a world-space delta.
    pub fn pan_world(&mut self, delta: [f32; 2]) {
        self.offset[0] -= delta[0];
        self.offset[1] -= delta[1];
    }

    /// Zoom at a specific screen-space anchor point.
    ///
    /// After the zoom, the world point under `anchor` stays at the same
    /// screen position.
    pub fn zoom_at(&mut self, factor: f32, anchor: Pos2) {
        let world_before = self.screen_to_world(anchor);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let world_after = self.screen_to_world(anchor);
        self.offset[0] += world_before[0] - world_after[0];
        self.offset[1] += world_before[1] - world_after[1];
    }

    /// Simple zoom without anchoring.
    pub fn zoom_center(&mut self, factor: f32) {
        let center = Pos2::new(self.viewport[0] * 0.5, self.viewport[1] * 0.5);
        self.zoom_at(factor, center);
    }

    // ------------------------------------------------------------------
    // Visible area queries
    // ------------------------------------------------------------------

    /// World-space bounding box of the visible viewport.
    pub fn visible_world_bounds(&self) -> [f32; 4] {
        let top_left = self.screen_to_world(Pos2::new(0.0, 0.0));
        let bottom_right = self.screen_to_world(Pos2::new(self.viewport[0], self.viewport[1]));
        [top_left[0], top_left[1], bottom_right[0], bottom_right[1]]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_identity() {
        let cam = Camera {
            zoom: 1.0,
            offset: [0.0, 0.0],
            viewport: [1920.0, 1080.0],
        };
        let world = [500.0, 300.0];
        let screen = cam.world_to_screen(world);
        let back = cam.screen_to_world(screen);
        assert!((back[0] - world[0]).abs() < 0.01);
        assert!((back[1] - world[1]).abs() < 0.01);
    }

    #[test]
    fn zoom_at_anchor() {
        let mut cam = Camera {
            zoom: 1.0,
            offset: [0.0, 0.0],
            viewport: [800.0, 600.0],
        };
        let anchor = Pos2::new(400.0, 300.0);
        let world_before = cam.screen_to_world(anchor);
        cam.zoom_at(2.0, anchor);
        let world_after = cam.screen_to_world(anchor);
        assert!((world_before[0] - world_after[0]).abs() < 0.01);
        assert!((world_before[1] - world_after[1]).abs() < 0.01);
    }
}
