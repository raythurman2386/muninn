//! Rune `Live` mirrors around the pure graph camera.

use muninn_graph::Camera as GraphCamera;
use rune::Live;

/// Camera plus the live values the graph view binds to.
///
/// A pan, zoom or fit updates both the pure camera and its live mirrors, so a drag redraws
/// without rebuilding the element tree.
pub struct Camera {
    inner: GraphCamera,
    pub live_pan: Live<[f32; 2]>,
    pub live_zoom: Live<f32>,
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
            inner: GraphCamera::new(),
            live_pan: Live::new([0.0; 2]),
            live_zoom: Live::new(1.0),
        }
    }

    #[must_use]
    pub fn pan(&self) -> [f32; 2] {
        self.inner.pan
    }

    #[must_use]
    pub fn zoom(&self) -> f32 {
        self.inner.zoom
    }

    pub fn set(&mut self, pan: [f32; 2], zoom: f32) {
        self.inner.set(pan, zoom);
        self.sync();
    }

    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.inner.pan_by(dx, dy);
        self.sync();
    }

    pub fn zoom_at(&mut self, screen: [f32; 2], factor: f32) {
        self.inner.zoom_at(screen, factor);
        self.sync();
    }

    pub fn fit(&mut self, bounds: [f32; 4], view: [f32; 2]) {
        self.inner.fit(bounds, view);
        self.sync();
    }

    #[must_use]
    pub fn world(&self, screen: [f32; 2]) -> [f32; 2] {
        self.inner.world(screen)
    }

    #[must_use]
    pub fn screen(&self, world: [f32; 2]) -> [f32; 2] {
        self.inner.screen(world)
    }

    fn sync(&mut self) {
        self.live_pan.set(self.inner.pan);
        self.live_zoom.set(self.inner.zoom);
    }
}
