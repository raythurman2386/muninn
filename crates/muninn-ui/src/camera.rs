//! Rune `Live` mirrors around the pure graph camera, plus the camera glide.

use muninn_graph::Camera as GraphCamera;
use rune::{Live, Spring};

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

    /// The pure camera, for a glide's start or target.
    #[must_use]
    pub fn inner(&self) -> GraphCamera {
        self.inner
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

/// A camera move over time: a critically damped spring from the pose at the click to a
/// target pose, sampled once per frame by `poll`.
///
/// The state layer owns this rather than a Rune element so a headless test can watch the
/// camera move frame by frame, and so `MUNINN_REDUCED_MOTION` can skip the glide entirely.
#[derive(Clone, Copy, Debug)]
pub struct CameraGlide {
    from: GraphCamera,
    to: GraphCamera,
    /// The pose this glide last published, so an external camera change (a
    /// wheel, drag or test) can cancel the glide instead of fighting it.
    at: GraphCamera,
    started: f64,
    spring: Spring,
    duration: f64,
}

impl CameraGlide {
    /// SwiftUI's `.smooth` feel: a short glide that settles without overshoot.
    const SPRING: Spring = Spring::new(0.34, 0.0);
    /// A little past the spring's settling tail, then the target lands exactly.
    const DURATION: f64 = 0.45;

    #[must_use]
    pub fn new(from: GraphCamera, to: GraphCamera, now: f64) -> CameraGlide {
        CameraGlide {
            from,
            to,
            at: from,
            started: now,
            spring: Self::SPRING,
            duration: Self::DURATION,
        }
    }

    #[must_use]
    pub fn target(&self) -> GraphCamera {
        self.to
    }

    /// The camera pose this glide last published.
    #[must_use]
    pub fn pose(&self) -> GraphCamera {
        self.at
    }

    /// Aim an in-flight glide at a newer target. The start and clock stay put,
    /// so a moving fit (the layout settling under a scope change) is followed
    /// without restarting the spring every frame.
    pub fn retarget(&mut self, to: GraphCamera) {
        self.to = to;
    }

    /// Sample the glide at `now` and remember the pose for the external-change
    /// check. The flag is true once the target has landed.
    #[must_use]
    pub fn advance(&mut self, now: f64) -> (GraphCamera, bool) {
        let (next, done) = self.sample(now);
        self.at = next;
        (next, done)
    }

    /// Sample the glide at `now`. The flag is true once the target has landed.
    #[must_use]
    pub fn sample(&self, now: f64) -> (GraphCamera, bool) {
        let elapsed = (now - self.started).max(0.0);
        if elapsed >= self.duration - 1e-9 {
            return (self.to, true);
        }
        let t = elapsed as f32;
        let (x, _) = self.spring.sample(self.from.pan[0], 0.0, self.to.pan[0], t);
        let (y, _) = self.spring.sample(self.from.pan[1], 0.0, self.to.pan[1], t);
        let (zoom, _) = self.spring.sample(self.from.zoom, 0.0, self.to.zoom, t);
        (
            GraphCamera {
                pan: [x, y],
                zoom: zoom.clamp(0.18, 2.4),
            },
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glide_moves_a_little_on_the_first_frame_and_lands_on_time() {
        let mut from = GraphCamera::new();
        from.set([0.0, 0.0], 1.0);
        let mut to = GraphCamera::new();
        to.set([400.0, 220.0], 1.6);
        let glide = CameraGlide::new(from, to, 10.0);

        let (first, done) = glide.sample(10.0 + 1.0 / 60.0);
        assert!(!done);
        assert!(
            first.pan[0] > 0.0 && first.pan[0] < 400.0,
            "{:?}",
            first.pan
        );
        assert!(first.zoom > 1.0 && first.zoom < 1.6, "{}", first.zoom);

        let (middle, done) = glide.sample(10.0 + 0.2);
        assert!(!done);
        assert!(middle.pan[0] > first.pan[0], "{:?}", middle.pan);

        let (landed, done) = glide.sample(10.0 + CameraGlide::DURATION);
        assert!(done);
        assert_eq!(landed.pan, [400.0, 220.0]);
        assert_eq!(landed.zoom, 1.6);
    }

    #[test]
    fn a_glide_is_deterministic() {
        let from = GraphCamera::new();
        let mut to = GraphCamera::new();
        to.set([-300.0, 500.0], 0.8);
        let one = CameraGlide::new(from, to, 0.0);
        let two = CameraGlide::new(from, to, 0.0);
        for frame in 1..=30 {
            let t = f64::from(frame) / 60.0;
            assert_eq!(one.sample(t).0.pan, two.sample(t).0.pan);
            assert_eq!(one.sample(t).0.zoom, two.sample(t).0.zoom);
        }
    }
}
