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

use rune::Live;

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

/// [`BoardView`] plus the `Live` values the world element binds.
pub struct BoardCam {
    view: BoardView,
    origin: [f32; 2],
    size: [f32; 2],
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
            live_offset: Live::new(view.world_offset(origin, size)),
            live_zoom: Live::new(view.zoom),
        }
    }

    #[must_use]
    pub fn view(&self) -> BoardView {
        self.view
    }

    pub fn set(&mut self, view: BoardView) {
        self.view = view;
        self.sync();
    }

    /// The world element changed (the file was re-read): keep the view.
    pub fn rebase(&mut self, origin: [f32; 2], size: [f32; 2]) {
        self.origin = origin;
        self.size = size;
        self.sync();
    }

    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.view.pan_by(dx, dy);
        self.sync();
    }

    pub fn zoom_at(&mut self, screen: [f32; 2], factor: f32) {
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
}
