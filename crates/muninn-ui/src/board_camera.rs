//! The board camera (Board slice 3): pan and zoom over a board's scene.
//!
//! It follows the graph camera's split. [`BoardView`] is pure math in scene
//! units, so tests and the settings file can hold it. [`BoardCam`] wraps it
//! with the Rune `Live` values the board's world element binds, so a drag or
//! wheel redraws without rebuilding the element tree.
//!
//! The graph camera works over a fixed world (`muninn_graph::WORLD`). A board's
//! world is its content's bounding box, which changes when the file does, so
//! this camera stores the scene point at the canvas's top-left corner (`eye`)
//! rather than an offset tied to one world size. A reload that grows the
//! board keeps the view where it was.

use rune::{Live, Spring};

/// Smallest and largest zoom: Excalidraw's own floor, and far enough in to
/// read a freehand stroke.
pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 8.0;
/// Space kept around the content by a fit, in screen points. It clears the
/// floating controls along the canvas's bottom edge.
pub const FIT_MARGIN: f32 = 56.0;
/// A fit never magnifies past this, so a tiny sketch is not blown up.
pub const FIT_MAX_ZOOM: f32 = 2.0;
/// One wheel notch, as on the graph canvas.
pub const WHEEL_IN: f32 = 1.08;
pub const WHEEL_OUT: f32 = 0.93;
/// `+` / `-` and the zoom buttons.
pub const KEY_ZOOM: f32 = 1.25;
/// Arrow-key pan, in screen points.
pub const KEY_PAN: f32 = 60.0;

/// `screen = (scene - eye) * zoom`, with `screen` relative to the canvas's
/// top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoardView {
    pub eye: [f32; 2],
    pub zoom: f32,
}

impl Default for BoardView {
    fn default() -> Self {
        BoardView {
            eye: [0.0; 2],
            zoom: 1.0,
        }
    }
}

impl BoardView {
    /// A view from stored values, or `None` when they are not usable (for
    /// example a hand-edited settings file).
    #[must_use]
    pub fn new(eye: [f32; 2], zoom: f32) -> Option<BoardView> {
        (eye.iter().all(|v| v.is_finite()) && zoom.is_finite() && zoom > 0.0).then(|| BoardView {
            eye,
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
        })
    }

    #[must_use]
    pub fn screen(&self, scene: [f32; 2]) -> [f32; 2] {
        [
            (scene[0] - self.eye[0]) * self.zoom,
            (scene[1] - self.eye[1]) * self.zoom,
        ]
    }

    #[must_use]
    pub fn scene(&self, screen: [f32; 2]) -> [f32; 2] {
        [
            self.eye[0] + screen[0] / self.zoom,
            self.eye[1] + screen[1] / self.zoom,
        ]
    }

    /// Move the content by `dx`, `dy` screen points (a drag to the right moves
    /// the drawing right).
    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.eye = [self.eye[0] - dx / self.zoom, self.eye[1] - dy / self.zoom];
    }

    /// Zoom by `factor`, keeping the scene point under `screen` in place.
    pub fn zoom_at(&mut self, screen: [f32; 2], factor: f32) {
        let fixed = self.scene(screen);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.eye = [
            fixed[0] - screen[0] / self.zoom,
            fixed[1] - screen[1] / self.zoom,
        ];
    }

    /// The view that centres `bounds` (min x, min y, max x, max y, scene
    /// units) in a canvas of `view` points, or centres the scene origin at
    /// zoom 1 when there is nothing to fit.
    #[must_use]
    pub fn fitted(bounds: Option<[f32; 4]>, view: [f32; 2]) -> BoardView {
        let Some(b) = bounds else {
            return BoardView {
                eye: [-view[0] / 2.0, -view[1] / 2.0],
                zoom: 1.0,
            };
        };
        let span = [(b[2] - b[0]).max(1.0), (b[3] - b[1]).max(1.0)];
        let room = [
            (view[0] - FIT_MARGIN * 2.0).max(1.0),
            (view[1] - FIT_MARGIN * 2.0).max(1.0),
        ];
        let zoom = (room[0] / span[0])
            .min(room[1] / span[1])
            .clamp(MIN_ZOOM, FIT_MAX_ZOOM);
        let centre = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        BoardView {
            eye: [
                centre[0] - view[0] / 2.0 / zoom,
                centre[1] - view[1] / 2.0 / zoom,
            ],
            zoom,
        }
    }

    /// The `live_offset` for a world element whose top-left is scene point
    /// `origin` and whose size is `size`. Rune scales that element about its
    /// centre `c`, so `screen = c + (local - c) * zoom + offset`, with
    /// `local = scene - origin`.
    #[must_use]
    pub fn world_offset(&self, origin: [f32; 2], size: [f32; 2]) -> [f32; 2] {
        let c = [size[0] / 2.0, size[1] / 2.0];
        [
            (origin[0] + c[0] - self.eye[0]) * self.zoom - c[0],
            (origin[1] + c[1] - self.eye[1]) * self.zoom - c[1],
        ]
    }
}

/// A camera move over time: the board's counterpart to the graph's
/// [`CameraGlide`](crate::CameraGlide). A short critically damped spring from
/// the pose at the key or button press to its target, stepped once per `poll`
/// frame by [`BoardCam::advance`].
///
/// Reduced motion never starts one (see `State`), and the wheel, a drag and a
/// magnify gesture set the camera directly, so they cancel a glide in flight
/// rather than fight it.
#[derive(Clone, Copy, Debug)]
pub struct BoardGlide {
    from: BoardView,
    to: BoardView,
    /// The pose this glide last published, so a direct camera change can
    /// cancel it instead of being overwritten.
    at: BoardView,
    started: f64,
    spring: Spring,
    duration: f64,
}

impl BoardGlide {
    /// The graph camera's own `.smooth` feel.
    const SPRING: Spring = Spring::new(0.34, 0.0);
    /// A little past the spring's settling tail, then the target lands exactly.
    const DURATION: f64 = 0.45;

    #[must_use]
    pub fn new(from: BoardView, to: BoardView, now: f64) -> BoardGlide {
        BoardGlide {
            from,
            to,
            at: from,
            started: now,
            spring: Self::SPRING,
            duration: Self::DURATION,
        }
    }

    #[must_use]
    pub fn target(&self) -> BoardView {
        self.to
    }

    /// The pose this glide last published.
    #[must_use]
    pub fn pose(&self) -> BoardView {
        self.at
    }

    /// Aim an in-flight glide at a newer target. The start and clock stay put,
    /// so a held arrow key or a second zoom press is followed without
    /// restarting the spring.
    pub fn retarget(&mut self, to: BoardView) {
        self.to = to;
    }

    /// Sample the glide at `now` and remember the pose for the
    /// external-change check. The flag is true once the target has landed.
    #[must_use]
    pub fn advance(&mut self, now: f64) -> (BoardView, bool) {
        let (next, done) = self.sample(now);
        self.at = next;
        (next, done)
    }

    /// Sample the glide at `now`. The flag is true once the target has landed.
    #[must_use]
    pub fn sample(&self, now: f64) -> (BoardView, bool) {
        let elapsed = (now - self.started).max(0.0);
        if elapsed >= self.duration - 1e-9 {
            return (self.to, true);
        }
        let t = elapsed as f32;
        let (x, _) = self.spring.sample(self.from.eye[0], 0.0, self.to.eye[0], t);
        let (y, _) = self.spring.sample(self.from.eye[1], 0.0, self.to.eye[1], t);
        let (zoom, _) = self.spring.sample(self.from.zoom, 0.0, self.to.zoom, t);
        (
            BoardView {
                eye: [x, y],
                zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            },
            false,
        )
    }
}

/// [`BoardView`] plus the `Live` values the world element binds.
pub struct BoardCam {
    view: BoardView,
    origin: [f32; 2],
    size: [f32; 2],
    /// A key or button camera move still travelling (see [`BoardGlide`]).
    glide: Option<BoardGlide>,
    pub live_offset: Live<[f32; 2]>,
    pub live_zoom: Live<f32>,
}

impl BoardCam {
    /// A camera for a world element at scene `origin` of `size` points.
    #[must_use]
    pub fn new(view: BoardView, origin: [f32; 2], size: [f32; 2]) -> BoardCam {
        BoardCam {
            view,
            origin,
            size,
            glide: None,
            live_offset: Live::new(view.world_offset(origin, size)),
            live_zoom: Live::new(view.zoom),
        }
    }

    #[must_use]
    pub fn view(&self) -> BoardView {
        self.view
    }

    /// Put the camera at `view` at once. A fit on open and a resize use this;
    /// user gestures go through [`BoardCam::glide_to`] so reduced motion can
    /// decide between a glide and a jump.
    pub fn set(&mut self, view: BoardView) {
        self.glide = None;
        self.view = view;
        self.sync();
    }

    /// The world element changed (the file was re-read): keep the view.
    pub fn rebase(&mut self, origin: [f32; 2], size: [f32; 2]) {
        self.glide = None;
        self.origin = origin;
        self.size = size;
        self.sync();
    }

    /// The view a glide in flight is heading for, or the current view.
    #[must_use]
    pub fn goal(&self) -> BoardView {
        self.glide.map_or(self.view, |glide| glide.target())
    }

    /// Start or retarget a glide to `view`, or land it at once when it is
    /// already there. `State` calls this only while motion is allowed.
    pub fn glide_to(&mut self, view: BoardView, now: f64) {
        if view == self.view {
            self.glide = None;
            return;
        }
        match &mut self.glide {
            Some(glide) => glide.retarget(view),
            None => self.glide = Some(BoardGlide::new(self.view, view, now)),
        }
    }

    /// Step a glide at `now`. Returns true when the visible view moved.
    pub fn advance(&mut self, now: f64) -> bool {
        let Some(mut glide) = self.glide else {
            return false;
        };
        // A wheel, a drag or a direct set wins over a glide in flight.
        if glide.pose() != self.view {
            self.glide = None;
            return false;
        }
        let (next, done) = glide.advance(now);
        let moved = next != self.view;
        self.view = next;
        self.sync();
        self.glide = if done { None } else { Some(glide) };
        moved
    }

    /// Whether a glide still has frames left to draw.
    #[must_use]
    pub fn gliding(&self) -> bool {
        self.glide.is_some()
    }

    /// Finish a glide in flight at its target (leaving the page, quitting).
    pub fn land(&mut self) {
        if let Some(glide) = self.glide.take() {
            self.view = glide.target();
            self.sync();
        }
    }

    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.glide = None;
        self.view.pan_by(dx, dy);
        self.sync();
    }

    pub fn zoom_at(&mut self, screen: [f32; 2], factor: f32) {
        self.glide = None;
        self.view.zoom_at(screen, factor);
        self.sync();
    }

    fn sync(&mut self) {
        self.live_offset
            .set(self.view.world_offset(self.origin, self.size));
        self.live_zoom.set(self.view.zoom);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn screen_and_scene_round_trip() {
        let view = BoardView::new([-120.5, 40.0], 1.7).unwrap();
        let p = [412.0, 288.0];
        assert!(close(view.screen(view.scene(p)), p));
    }

    #[test]
    fn zoom_keeps_the_point_under_the_pointer() {
        let mut view = BoardView::new([10.0, -30.0], 0.8).unwrap();
        let p = [300.0, 240.0];
        let before = view.scene(p);
        view.zoom_at(p, WHEEL_IN);
        assert!(close(view.scene(p), before));
        assert!((view.zoom - 0.8 * WHEEL_IN).abs() < 1e-6);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut view = BoardView::default();
        for _ in 0..100 {
            view.zoom_at([0.0, 0.0], 2.0);
        }
        assert_eq!(view.zoom, MAX_ZOOM);
        for _ in 0..100 {
            view.zoom_at([0.0, 0.0], 0.5);
        }
        assert_eq!(view.zoom, MIN_ZOOM);
    }

    #[test]
    fn a_drag_moves_the_content_by_the_drag() {
        let mut view = BoardView::new([0.0, 0.0], 1.5).unwrap();
        let before = view.screen([100.0, 100.0]);
        view.pan_by(120.0, -60.0);
        let after = view.screen([100.0, 100.0]);
        assert!(close(after, [before[0] + 120.0, before[1] - 60.0]));
    }

    #[test]
    fn a_fit_centres_the_bounds_with_the_margin() {
        let bounds = [-200.0, 50.0, 400.0, 350.0];
        let view = BoardView::fitted(Some(bounds), [900.0, 620.0]);
        let zoom = ((900.0 - 2.0 * FIT_MARGIN) / 600.0).min((620.0 - 2.0 * FIT_MARGIN) / 300.0);
        assert!((view.zoom - zoom).abs() < 1e-6);
        assert!(close(view.screen([100.0, 200.0]), [450.0, 310.0]));
        // A dot is never magnified past the fit limit.
        let tiny = BoardView::fitted(Some([0.0, 0.0, 2.0, 2.0]), [900.0, 620.0]);
        assert_eq!(tiny.zoom, FIT_MAX_ZOOM);
        // Nothing drawn: the scene origin sits in the middle at 100 %.
        let empty = BoardView::fitted(None, [900.0, 620.0]);
        assert!(close(empty.screen([0.0, 0.0]), [450.0, 310.0]));
        assert_eq!(empty.zoom, 1.0);
    }

    #[test]
    fn the_world_offset_matches_rune_scaling_about_the_centre() {
        let view = BoardView::new([-37.0, 12.0], 1.6).unwrap();
        let (origin, size) = ([-80.0, -20.0], [700.0, 420.0]);
        let offset = view.world_offset(origin, size);
        let c = [size[0] / 2.0, size[1] / 2.0];
        for scene in [[-80.0, -20.0], [0.0, 0.0], [520.5, 300.25]] {
            let local = [scene[0] - origin[0], scene[1] - origin[1]];
            let rune = [
                c[0] + (local[0] - c[0]) * view.zoom + offset[0],
                c[1] + (local[1] - c[1]) * view.zoom + offset[1],
            ];
            assert!(close(rune, view.screen(scene)), "{scene:?}");
        }
    }

    #[test]
    fn stored_views_must_be_finite_and_positive() {
        assert!(BoardView::new([f32::NAN, 0.0], 1.0).is_none());
        assert!(BoardView::new([0.0, 0.0], 0.0).is_none());
        assert_eq!(BoardView::new([0.0, 0.0], 99.0).unwrap().zoom, MAX_ZOOM);
    }

    #[test]
    fn a_glide_moves_on_the_first_frame_and_lands_on_time() {
        let from = BoardView::new([0.0, 0.0], 1.0).unwrap();
        let to = BoardView::new([400.0, 220.0], 1.6).unwrap();
        let glide = BoardGlide::new(from, to, 10.0);

        let (first, done) = glide.sample(10.0 + 1.0 / 60.0);
        assert!(!done);
        assert!(
            first.eye[0] > 0.0 && first.eye[0] < 400.0,
            "{:?}",
            first.eye
        );
        assert!(first.zoom > 1.0 && first.zoom < 1.6, "{}", first.zoom);

        let (middle, done) = glide.sample(10.0 + 0.2);
        assert!(!done);
        assert!(middle.eye[0] > first.eye[0], "{:?}", middle.eye);

        let (landed, done) = glide.sample(10.0 + BoardGlide::DURATION);
        assert!(done);
        assert_eq!(landed, to);
    }

    #[test]
    fn a_glide_is_deterministic_and_a_direct_change_cancels_it() {
        let from = BoardView::new([0.0, 0.0], 1.0).unwrap();
        let to = BoardView::new([-300.0, 500.0], 0.8).unwrap();
        let one = BoardGlide::new(from, to, 0.0);
        let two = BoardGlide::new(from, to, 0.0);
        for frame in 1..=30 {
            let t = f64::from(frame) / 60.0;
            assert_eq!(one.sample(t), two.sample(t));
        }

        let mut cam = BoardCam::new(from, [0.0, 0.0], [100.0, 100.0]);
        cam.glide_to(to, 0.0);
        assert!(cam.gliding());
        assert!(cam.advance(1.0 / 60.0));
        // A wheel or a drag sets the view directly and wins.
        let before = cam.view();
        cam.pan_by(10.0, 0.0);
        assert!(!cam.gliding());
        assert!(!cam.advance(2.0 / 60.0));
        assert_eq!(
            cam.view().eye,
            [before.eye[0] - 10.0 / before.zoom, before.eye[1]]
        );
    }
}
