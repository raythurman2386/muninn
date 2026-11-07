//! Camera math for the graph canvas.
//!
//! The camera is pure: the UI mirrors its pan and zoom into Rune's `Live` values when it wants
//! a drag to redraw without rebuilding the view.

use crate::layout::{NODE_H, NODE_W, WORLD};

const MIN_ZOOM: f32 = 0.18;
const MAX_ZOOM: f32 = 2.4;

/// A pan and zoom over [`WORLD`]: `screen = c + (world - c) * zoom + pan`, where `c` is the
/// world's centre.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub pan: [f32; 2],
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

impl Camera {
    #[must_use]
    pub fn new() -> Camera {
        Camera {
            pan: [0.0; 2],
            zoom: 1.0,
        }
    }

    pub fn set(&mut self, pan: [f32; 2], zoom: f32) {
        self.pan = pan;
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.set([self.pan[0] + dx, self.pan[1] + dy], self.zoom);
    }

    /// `screen` is a point in the canvas, whose top-left is the world element's top-left.
    #[must_use]
    pub fn world(&self, screen: [f32; 2]) -> [f32; 2] {
        let c = world_centre();
        [
            c[0] + (screen[0] - c[0] - self.pan[0]) / self.zoom,
            c[1] + (screen[1] - c[1] - self.pan[1]) / self.zoom,
        ]
    }

    #[must_use]
    pub fn screen(&self, world: [f32; 2]) -> [f32; 2] {
        let c = world_centre();
        [
            c[0] + (world[0] - c[0]) * self.zoom + self.pan[0],
            c[1] + (world[1] - c[1]) * self.zoom + self.pan[1],
        ]
    }

    /// Zoom by `factor`, keeping the world point under `screen` in place.
    pub fn zoom_at(&mut self, screen: [f32; 2], factor: f32) {
        let fixed = self.world(screen);
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let c = world_centre();
        self.set(
            [
                screen[0] - c[0] - (fixed[0] - c[0]) * zoom,
                screen[1] - c[1] - (fixed[1] - c[1]) * zoom,
            ],
            zoom,
        );
    }

    /// Centre `bounds` (min x, min y, max x, max y) in a canvas of `view` points.
    pub fn fit(&mut self, bounds: [f32; 4], view: [f32; 2]) {
        if view[0] < 40.0 || view[1] < 40.0 {
            return;
        }
        let margin = 64.0;
        let span_x = (bounds[2] - bounds[0]).max(NODE_W);
        let span_y = (bounds[3] - bounds[1]).max(NODE_H);
        let zoom = ((view[0] - margin * 2.0) / span_x)
            .min((view[1] - margin * 2.0) / span_y)
            .clamp(0.24, 1.8);
        let world = [(bounds[0] + bounds[2]) / 2.0, (bounds[1] + bounds[3]) / 2.0];
        let c = world_centre();
        self.set(
            [
                view[0] / 2.0 - c[0] - (world[0] - c[0]) * zoom,
                view[1] / 2.0 - c[1] - (world[1] - c[1]) * zoom,
            ],
            zoom,
        );
    }
}

#[must_use]
pub fn world_centre() -> [f32; 2] {
    [WORLD[0] / 2.0, WORLD[1] / 2.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_zoom_keeps_the_pointer_point() {
        let mut cam = Camera::new();
        cam.set([0.0, 0.0], 1.0);
        let before = cam.world([300.0, 240.0]);
        cam.zoom_at([300.0, 240.0], 1.6);
        let after = cam.world([300.0, 240.0]);
        assert!((before[0] - after[0]).abs() < 1e-3);
        assert!((before[1] - after[1]).abs() < 1e-3);
    }

    #[test]
    fn camera_round_trips_between_world_and_screen() {
        let mut cam = Camera::new();
        cam.set([24.0, -12.0], 1.7);
        let screen = [412.0, 288.0];
        let world = cam.world(screen);
        let back = cam.screen(world);
        assert!((back[0] - screen[0]).abs() < 1e-3);
        assert!((back[1] - screen[1]).abs() < 1e-3);
    }
}
