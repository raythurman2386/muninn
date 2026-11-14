//! WCAG 2.1 contrast (#72): relative luminance, contrast ratio, and the
//! floors the palette and the Board canvas keep.
//!
//! The audit behind this module measured the OpenCode and Omarchy palettes on
//! the surfaces text and icons actually sit on. These failed and are repaired
//! here rather than upstream (the OpenCode kit is a dependency):
//!
//! - `text_faint` was below 4.5:1 on `layer_01` and `layer_02` (`#808080`:
//!   3.93 and 3.44 on the dark theme, 3.78 and 3.53 on light).
//! - the light theme's status inks were below 4.5:1 on `layer_01`, where the
//!   Kanban lane header paints them: warning `#cb9f34` was 2.35:1 (the
//!   "Over limit" label) and success `#198b43` was 4.18:1 (the done lane's
//!   "Complete" label).
//!
//! [`enforce_kit`] repairs them by moving the ink along its OKLab lightness
//! until it clears the floor, so hue survives a theme switch and both light
//! and dark stay correct. The floors are WCAG 2.1 AA: [`TEXT`] 4.5:1 for
//! normal text, [`UI`] 3:1 for large text, icons and other components. Icons
//! keep the lower floor; `icon_muted` clears it on both layers already.
//!
//! Board strokes are a separate case: Excalidraw stores the exact colour in
//! the file and that is what is written back. A committed ink that would
//! vanish into the paper is painted with a legible substitute of the same hue
//! while the file keeps its bytes (#88, `board_scene::paints`). Only the
//! *default* ink for a new element is adapted, and only on a dark paper (see
//! [`board_stroke`]); the canvas's own empty-board placeholder is inked for
//! its paper too.

use rune::Color;
use rune_kit::Kit;

/// WCAG 2.1 AA, normal-size text.
pub const TEXT: f32 = 4.5;
/// WCAG 2.1 AA, large text and non-text UI (icons, strokes, boundaries).
pub const UI: f32 = 3.0;

/// Excalidraw's default ink on a dark paper: what its own dark theme draws
/// with, so a board opened on a dark `viewBackgroundColor` stays legible.
pub const LIGHT_INK: &str = "#e6e6e6";

/// The contrast ratio between two colours, 1.0 (identical) to 21.0.
#[must_use]
pub fn ratio(a: Color, b: Color) -> f32 {
    let (a, b) = (a.luminance(), b.luminance());
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Whether `fg` on `bg` clears `floor`. The epsilon absorbs f32 rounding so a
/// colour repaired to exactly the floor still passes.
#[must_use]
pub fn meets(fg: Color, bg: Color, floor: f32) -> bool {
    ratio(fg, bg) >= floor - 1e-4
}

/// `t` of the way from `a` to `b` in OKLab: the lightness moves and the hue
/// stays put, which is what a repair should do.
#[must_use]
pub fn mix_oklab(a: Color, b: Color, t: f32) -> Color {
    let (a, b) = (a.to_oklab(), b.to_oklab());
    Color::from_oklab(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t))
}

/// `fg`, or the closest colour of the same hue that clears `floor` on `bg`.
///
/// Moves toward white or black, whichever reaches the floor with the smaller
/// shift; when neither can (`bg` is a mid grey and the floor is high) returns
/// the far end with the better contrast.
#[must_use]
pub fn at_least(fg: Color, bg: Color, floor: f32) -> Color {
    if meets(fg, bg, floor) {
        return fg;
    }
    let toward = |target: Color| {
        if !meets(target, bg, floor) {
            return None;
        }
        // Smallest shift that clears the floor: mix is monotone in luminance
        // for a fixed target, so a bisection finds it.
        let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
        for _ in 0..20 {
            let mid = (lo + hi) / 2.0;
            if meets(mix_oklab(fg, target, mid), bg, floor) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        Some(mix_oklab(fg, target, hi))
    };
    let (white, black) = (toward(Color::WHITE), toward(Color::BLACK));
    match (white, black) {
        (Some(w), Some(b)) => {
            if ratio(w, fg) <= ratio(b, fg) {
                w
            } else {
                b
            }
        }
        (Some(w), None) => w,
        (None, Some(b)) => b,
        (None, None) => {
            if ratio(Color::WHITE, bg) >= ratio(Color::BLACK, bg) {
                Color::WHITE
            } else {
                Color::BLACK
            }
        }
    }
}

/// The surfaces secondary text and icons paint on: the page, and the two
/// raised layers (cards, lanes, panels).
fn surfaces(kit: &Kit) -> [Color; 3] {
    [kit.c.bg_base, kit.c.layer_01, kit.c.layer_02]
}

/// The colour `fg` must reach to clear `floor` on every surface: repair
/// against whichever surface is worst, then verify, because moving an ink
/// can change which surface is worst.
fn on_surfaces(fg: Color, surfaces: &[Color], floor: f32) -> Color {
    let mut fg = fg;
    for _ in 0..8 {
        let worst = surfaces
            .iter()
            .copied()
            .min_by(|a, b| ratio(fg, *a).total_cmp(&ratio(fg, *b)))
            .unwrap_or(fg);
        if meets(fg, worst, floor) {
            return fg;
        }
        fg = at_least(fg, worst, floor);
    }
    fg
}

/// Raise the palette's secondary inks and status inks to their WCAG floors.
///
/// `text_muted` / `text_faint` are text (4.5:1); `icon_muted` / `icon_faint`
/// are icons (3:1). The four status inks are text — a lane header, a menu row,
/// a banner — so they keep the text floor on every surface.
pub fn enforce_kit(kit: &mut Kit) {
    let behind = surfaces(kit);
    let c = &mut kit.c;
    c.text_muted = on_surfaces(c.text_muted, &behind, TEXT);
    c.text_faint = on_surfaces(c.text_faint, &behind, TEXT);
    c.icon_muted = on_surfaces(c.icon_muted, &behind, UI);
    c.icon_faint = on_surfaces(c.icon_faint, &behind, UI);
    for state in [&mut c.success, &mut c.warning, &mut c.danger, &mut c.info] {
        state.fg = on_surfaces(state.fg, &behind, TEXT);
    }
}

/// The stroke a new element is written with.
///
/// Excalidraw's stored colours round-trip unchanged: an existing shape keeps
/// the ink the file names (painted legibly if it would vanish, see #88). A
/// *new* element with the default ink ([`muninn_core::DEFAULT_STROKE`]) on a
/// dark `viewBackgroundColor` would be invisible (`#1e1e1e` on `#121212` is
/// 1.12:1), so it is written with [`LIGHT_INK`] instead — the value
/// Excalidraw's own dark theme uses. A colour the user picked from the palette
/// is never changed.
#[must_use]
pub fn board_stroke(stroke: &'static str, paper: Color) -> &'static str {
    if stroke == muninn_core::DEFAULT_STROKE && !meets(stroke_ink(stroke), paper, UI) {
        return LIGHT_INK;
    }
    stroke
}

/// The [`Color`] for a stored Excalidraw string, falling back to black.
fn stroke_ink(value: &str) -> Color {
    rune::color::hex(value).unwrap_or(Color::BLACK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::color::hex;

    fn h(s: &str) -> Color {
        hex(s).expect("hex")
    }

    /// The two kits the app ships, both repaired.
    fn kits() -> [(&'static str, Kit); 2] {
        let mut dark = rune_theme_opencode::kit(true).clone();
        let mut light = rune_theme_opencode::kit(false).clone();
        enforce_kit(&mut dark);
        enforce_kit(&mut light);
        [("dark", dark), ("light", light)]
    }

    #[test]
    fn contrast_ratio_matches_wcag_worked_examples() {
        // Black on white is 21:1, white on white 1:1; WCAG's own example:
        // #777777 on #ffffff is 4.48:1.
        assert!((ratio(h("#000000"), h("#ffffff")) - 21.0).abs() < 0.01);
        assert!((ratio(h("#ffffff"), h("#ffffff")) - 1.0).abs() < 0.001);
        assert!((ratio(h("#777777"), h("#ffffff")) - 4.48).abs() < 0.02);
    }

    #[test]
    fn at_least_raises_an_ink_and_leaves_a_passing_one_alone() {
        let ink = h("#808080");
        let bg = h("#2e2e2e");
        assert!(!meets(ink, bg, TEXT));
        let raised = at_least(ink, bg, TEXT);
        assert!(meets(raised, bg, TEXT), "{} on {}", raised.hex(), bg.hex());
        // Same hue family: still a neutral grey, and only as light as needed.
        assert!(meets(h("#aeaeae"), bg, TEXT));
        assert_eq!(at_least(h("#aeaeae"), bg, TEXT), h("#aeaeae"));
    }

    #[test]
    fn secondary_text_and_icons_clear_their_floors_on_every_layer() {
        for (name, kit) in kits() {
            for surface in surfaces(&kit) {
                for (token, ink, floor) in [
                    ("text_muted", kit.c.text_muted, TEXT),
                    ("text_faint", kit.c.text_faint, TEXT),
                    ("icon_muted", kit.c.icon_muted, UI),
                    ("icon_faint", kit.c.icon_faint, UI),
                ] {
                    assert!(
                        meets(ink, surface, floor),
                        "{name} {token} {} on {} is {:.2}, floor {floor}",
                        ink.hex(),
                        surface.hex(),
                        ratio(ink, surface),
                    );
                }
            }
        }
    }

    #[test]
    fn status_text_clears_the_text_floor_on_a_lane() {
        for (name, kit) in kits() {
            for surface in surfaces(&kit) {
                for (token, ink) in [
                    ("warning", kit.c.warning.fg),
                    ("success", kit.c.success.fg),
                    ("danger", kit.c.danger.fg),
                    ("info", kit.c.info.fg),
                ] {
                    assert!(
                        meets(ink, surface, TEXT),
                        "{name} {token} {} on {} is {:.2}",
                        ink.hex(),
                        surface.hex(),
                        ratio(ink, surface),
                    );
                }
            }
        }
    }

    #[test]
    fn board_default_ink_is_adapted_only_on_a_dark_paper() {
        assert_eq!(
            board_stroke(muninn_core::DEFAULT_STROKE, h("#ffffff")),
            muninn_core::DEFAULT_STROKE,
            "Excalidraw's ink on its paper is untouched"
        );
        assert_eq!(
            board_stroke(muninn_core::DEFAULT_STROKE, h("#121212")),
            LIGHT_INK,
            "a dark board gets the dark-theme ink"
        );
        assert_eq!(
            board_stroke("#e03131", h("#121212")),
            "#e03131",
            "a picked colour is never changed"
        );
        assert!(meets(
            stroke_ink(board_stroke(muninn_core::DEFAULT_STROKE, h("#121212"))),
            h("#121212"),
            UI
        ));
    }
}
