//! Issue #69: the Board's primary colour palette.
//!
//! Excalidraw's default swatches, not a custom picker: one row of stroke
//! colours and one of fills, `transparent` included, in a compact panel under
//! the toolbar (`board_tools::toolbar` holds the button that opens it).
//!
//! - The picked stroke and fill ink the next element drawn: a box, line,
//!   arrow, freehand stroke or text (text uses the stroke as its ink and no
//!   fill, as Excalidraw does). The preview while drawing shows them too.
//! - With a selection, a swatch writes the colour onto the selected elements
//!   as one edit ([`muninn_core::Board::recolor`]): one undo step, versions
//!   bumped, the board marked unsaved, as a move does.
//! - The panel closes on a choice, so it never sits between the pointer and
//!   the canvas; `Esc` and picking a drawing tool close it too. It is not a
//!   saved setting: a fresh session starts from Excalidraw's defaults.
//!
//! Boards that already carry colours keep them: the palette only writes when
//! a swatch is picked, and only onto what the pick applies to.

use rune::{Color, Cursor, El, Icon, Key, KeyEvent, Mods, Tx, col, icon, row, text};
use rune_kit::Kit;

use crate::State;

/// A colour control's row: the stroke swatches or the fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Stroke,
    Fill,
}

impl Kind {
    fn key(self) -> &'static str {
        match self {
            Kind::Stroke => "stroke",
            Kind::Fill => "fill",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Stroke => "Stroke",
            Kind::Fill => "Fill",
        }
    }

    fn swatches(self) -> &'static [Swatch] {
        match self {
            Kind::Stroke => &STROKE_SWATCHES,
            Kind::Fill => &FILL_SWATCHES,
        }
    }

    fn chosen(self, s: &State) -> &'static str {
        match self {
            Kind::Stroke => s.board_stroke,
            Kind::Fill => s.board_fill,
        }
    }

    fn choose(self, s: &mut State, colour: &'static str) {
        match self {
            Kind::Stroke => s.set_board_stroke(colour),
            Kind::Fill => s.set_board_fill(colour),
        }
    }
}

/// One palette swatch: its name (for the button's key and label) and the
/// Excalidraw colour string it writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Swatch {
    name: &'static str,
    hex: &'static str,
}

const fn swatch(name: &'static str, hex: &'static str) -> Swatch {
    Swatch { name, hex }
}

/// Excalidraw's primary inks.
const STROKE_SWATCHES: [Swatch; 8] = [
    swatch("black", "#1e1e1e"),
    swatch("grey", "#868e96"),
    swatch("red", "#e03131"),
    swatch("orange", "#f08c00"),
    swatch("yellow", "#fab005"),
    swatch("green", "#2f9e44"),
    swatch("blue", "#1971c2"),
    swatch("purple", "#9c36b5"),
];

/// Excalidraw's pastel backgrounds, and no fill at all.
const FILL_SWATCHES: [Swatch; 8] = [
    swatch("transparent", "transparent"),
    swatch("grey", "#e9ecef"),
    swatch("red", "#ffc9c9"),
    swatch("orange", "#ffd8a8"),
    swatch("yellow", "#ffec99"),
    swatch("green", "#b2f2bb"),
    swatch("blue", "#a5d8ff"),
    swatch("purple", "#d0bfff"),
];

/// A swatch's target: WCAG 2.2's minimum for a pointer control.
const SWATCH: f32 = 24.0;

/// The "no fill" mark: a diagonal on the transparent swatch.
static NO_FILL: Icon = Icon {
    name: "board-no-fill",
    view: 16.0,
    body: r#"<path d="M3 13L13 3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
};

impl State {
    /// Pick the palette's stroke colour: it inks the next element drawn and,
    /// with a selection, recolours it as one undoable edit.
    pub fn set_board_stroke(&mut self, colour: &'static str) {
        self.board_stroke = colour;
        self.board_palette = false;
        self.recolour_board(Some(colour), None);
    }

    /// Pick the palette's fill colour (`transparent` for none), applied like
    /// the stroke.
    pub fn set_board_fill(&mut self, colour: &'static str) {
        self.board_fill = colour;
        self.board_palette = false;
        self.recolour_board(None, Some(colour));
    }

    /// Show or hide the palette panel.
    pub fn toggle_board_palette(&mut self) {
        self.board_palette = !self.board_palette;
    }

    /// Write a picked colour onto the selection, if there is one: one edit,
    /// so one undo takes it back. `false` when nothing was selected.
    fn recolour_board(&mut self, stroke: Option<&'static str>, fill: Option<&'static str>) -> bool {
        let Some(open) = &mut self.board_open else {
            return false;
        };
        let ids = open.edit.selection.clone();
        if ids.is_empty() {
            return false;
        }
        let Some(model) = &mut open.model else {
            return false;
        };
        let edit = model.recolor(
            &ids,
            stroke,
            fill,
            &mut self.board_ids,
            crate::board_tools::now_ms(),
        );
        open.record(edit)
    }
}

/// The toolbar button that shows the current colours and opens the panel.
pub(crate) fn toggle(s: &State, k: &Kit) -> El {
    let open = s.board_palette;
    let label = if open {
        "Close colour palette"
    } else {
        "Colour palette"
    };
    row()
        .key("board-palette-toggle")
        .size(30.0, 30.0)
        .radius(6.0)
        .center()
        .fill(if open {
            k.c.layer_03
        } else {
            Color::TRANSPARENT
        })
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .role("button")
        .label(label)
        .help("Stroke and fill colours")
        .expanded(open)
        .focusable(true)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        .on_press_tx(|s: &mut State, tx: &mut Tx| {
            s.toggle_board_palette();
            // Keep the keyboard on the canvas for tool letters and arrows.
            tx.blur();
        })
        .child(ink_preview(s))
}

/// The toggle's picture: the fill with a border in the stroke colour.
fn ink_preview(s: &State) -> El {
    let stroke = crate::board_scene::color(s.board_stroke).unwrap_or(Color::BLACK);
    let mut preview = row().size(16.0, 16.0).radius(3.0).border(2.0, stroke);
    if let Some(fill) = crate::board_scene::color(s.board_fill) {
        preview = preview.fill(fill);
    }
    preview
}

/// The palette panel under the toolbar: a stroke row and a fill row. Nothing
/// is built while it is closed.
pub(crate) fn panel(s: &State, k: &Kit) -> El {
    if !s.board_palette {
        return rune::el();
    }
    col()
        .key("board-palette")
        .pin(Some(12.0), Some(54.0), None, None)
        .padding(4.0)
        .gap(3.0)
        .radius(8.0)
        .fill(k.alpha(k.c.layer_02, 0.98))
        .border(1.0, k.c.border_base)
        .role("group")
        .label("Board colours")
        .child(swatch_row(s, k, Kind::Stroke))
        .child(swatch_row(s, k, Kind::Fill))
}

fn swatch_row(s: &State, k: &Kit, kind: Kind) -> El {
    let mut line = row()
        .key(format!("board-{}-row", kind.key()))
        .gap(3.0)
        .center_y()
        .child(
            text(kind.label())
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .w(42.0),
        );
    for swatch in kind.swatches() {
        line = line.child(swatch_button(
            k,
            kind,
            *swatch,
            kind.chosen(s) == swatch.hex,
        ));
    }
    line
}

fn swatch_button(k: &Kit, kind: Kind, swatch: Swatch, chosen: bool) -> El {
    let label = format!("{} {}", kind.label(), swatch.name);
    let hex = swatch.hex;
    let mut button = row()
        .key(format!("board-{}-{}", kind.key(), swatch.name))
        .size(SWATCH, SWATCH)
        .center()
        .radius(6.0)
        .border(
            if chosen { 2.0 } else { 1.0 },
            if chosen {
                k.c.text_accent
            } else {
                k.c.border_base
            },
        )
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .role("button")
        .label(label.clone())
        .help(label)
        .focusable(true)
        .focus_selected(chosen)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        .on_press_tx(move |s: &mut State, tx: &mut Tx| {
            kind.choose(s, hex);
            // Back to the canvas for drawing, as a tool button does.
            tx.blur();
        })
        // A focused button presses on Space; Return needs asking for.
        .on_key(move |s: &mut State, e: &KeyEvent, tx: &mut Tx| {
            if e.key == Key::Enter && e.mods == Mods::default() {
                kind.choose(s, hex);
                tx.blur();
                true
            } else {
                false
            }
        });
    match crate::board_scene::color(swatch.hex) {
        Some(colour) => button = button.fill(colour),
        None => button = button.child(icon(&NO_FILL, 16.0).color(k.c.icon_muted)),
    }
    button
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_has_excalidraws_primaries_and_transparent_fill() {
        for kind in [Kind::Stroke, Kind::Fill] {
            for swatch in kind.swatches() {
                if swatch.name == "transparent" {
                    assert_eq!(kind, Kind::Fill);
                    continue;
                }
                assert!(
                    crate::board_scene::color(swatch.hex).is_some(),
                    "{:?} {} is not a colour",
                    kind,
                    swatch.name
                );
                assert_eq!(swatch.hex, swatch.hex.to_lowercase());
            }
        }
        assert!(FILL_SWATCHES.iter().any(|s| s.hex == "transparent"));
        assert!(!STROKE_SWATCHES.iter().any(|s| s.hex == "transparent"));
        // Both rows name the usual primaries; the stroke row has the black
        // ink, the fill row the transparent option instead.
        let names = |row: &[Swatch]| -> Vec<&str> { row.iter().map(|s| s.name).collect() };
        assert_eq!(
            names(&STROKE_SWATCHES),
            [
                "black", "grey", "red", "orange", "yellow", "green", "blue", "purple"
            ]
        );
        assert_eq!(
            names(&FILL_SWATCHES),
            [
                "transparent",
                "grey",
                "red",
                "orange",
                "yellow",
                "green",
                "blue",
                "purple"
            ]
        );
    }
}
