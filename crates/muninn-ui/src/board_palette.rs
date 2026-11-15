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

use muninn_core::Ink;
use muninn_i18n::{Key as TextKey, t};
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
            Kind::Stroke => t(TextKey::BoardPaletteStroke),
            Kind::Fill => t(TextKey::BoardPaletteFill),
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

/// One palette swatch: its name (for the button's key), the catalog key for
/// its display name (for the button's label) and the Excalidraw colour string
/// it writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Swatch {
    name: &'static str,
    label: TextKey,
    hex: &'static str,
}

const fn swatch(name: &'static str, label: TextKey, hex: &'static str) -> Swatch {
    Swatch { name, label, hex }
}

/// Excalidraw's primary inks.
const STROKE_SWATCHES: [Swatch; 8] = [
    swatch("black", TextKey::BoardPaletteSwatchBlack, "#1e1e1e"),
    swatch("grey", TextKey::BoardPaletteSwatchGrey, "#868e96"),
    swatch("red", TextKey::BoardPaletteSwatchRed, "#e03131"),
    swatch("orange", TextKey::BoardPaletteSwatchOrange, "#f08c00"),
    swatch("yellow", TextKey::BoardPaletteSwatchYellow, "#fab005"),
    swatch("green", TextKey::BoardPaletteSwatchGreen, "#2f9e44"),
    swatch("blue", TextKey::BoardPaletteSwatchBlue, "#1971c2"),
    swatch("purple", TextKey::BoardPaletteSwatchPurple, "#9c36b5"),
];

/// Excalidraw's pastel backgrounds, and no fill at all.
const FILL_SWATCHES: [Swatch; 8] = [
    swatch(
        "transparent",
        TextKey::BoardPaletteSwatchTransparent,
        "transparent",
    ),
    swatch("grey", TextKey::BoardPaletteSwatchGrey, "#e9ecef"),
    swatch("red", TextKey::BoardPaletteSwatchRed, "#ffc9c9"),
    swatch("orange", TextKey::BoardPaletteSwatchOrange, "#ffd8a8"),
    swatch("yellow", TextKey::BoardPaletteSwatchYellow, "#ffec99"),
    swatch("green", TextKey::BoardPaletteSwatchGreen, "#b2f2bb"),
    swatch("blue", TextKey::BoardPaletteSwatchBlue, "#a5d8ff"),
    swatch("purple", TextKey::BoardPaletteSwatchPurple, "#d0bfff"),
];

/// A swatch's target: WCAG 2.2's minimum for a pointer control.
const SWATCH: f32 = 24.0;

/// The ring around every swatch: a neutral that clears 3:1 on both palette
/// panels (`layer_02` in either theme) and on Excalidraw's white and dark
/// papers, so a swatch stays findable when its own colour matches the surface.
/// Fixed rather than a kit token because Excalidraw's swatches are fixed too
/// and the panel sits on a board whose paper can be any colour.
const SWATCH_RING: &str = "#7a7a7a";

/// The "no fill" mark: a diagonal on the transparent swatch.
static NO_FILL: Icon = Icon {
    name: "board-no-fill",
    view: 16.0,
    body: r#"<path d="M3 13L13 3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
};

impl State {
    /// The paper of the open board: its `viewBackgroundColor`, falling back to
    /// Excalidraw's white when the board names none.
    pub(crate) fn board_paper(&self) -> Color {
        self.board_open
            .as_ref()
            .and_then(|open| open.scene.as_ref().ok())
            .map_or(Color::WHITE, |scene| scene.paper)
    }

    /// The ink a *new* element is written with (#72). The picked stroke is
    /// kept, except that Excalidraw's default ink is replaced with its
    /// dark-theme ink on a dark paper, where `#1e1e1e` would be invisible.
    /// Existing elements keep the colour the file names.
    pub(crate) fn new_board_ink(&self) -> Ink<'static> {
        Ink {
            stroke: crate::contrast::board_stroke(self.board_stroke, self.board_paper()),
            background: self.board_fill,
        }
    }

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
        t(TextKey::BoardPaletteClose)
    } else {
        t(TextKey::BoardPaletteOpen)
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
        .help(t(TextKey::BoardPaletteHelp))
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
        // Opaque on purpose (#72): at 98% a white or near-black board paper
        // tints the panel enough to drop the faint labels under 4.5:1, and
        // the swatch rings then vary with the canvas behind the palette.
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        .role("group")
        .label(t(TextKey::BoardPaletteTitle))
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
    let label = format!("{} {}", kind.label(), t(swatch.label));
    let hex = swatch.hex;
    let ring = rune::color::hex(SWATCH_RING).unwrap_or(k.c.icon_muted);
    let mut button = row()
        .key(format!("board-{}-{}", kind.key(), swatch.name))
        .size(SWATCH, SWATCH)
        .center()
        .radius(6.0)
        // The ring makes a swatch findable when its own colour blends into the
        // panel (a pastel fill, or Excalidraw's ink on a dark board). It is
        // always the neutral ring; a chosen swatch thickens it and adds an
        // inset accent ring, so selection never removes the boundary.
        .border(if chosen { 2.0 } else { 1.0 }, ring)
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
    if chosen {
        // Inside the 1 point of neutral ring: the accent shows the pick while
        // the outer edge keeps the ring that reads on either paper.
        button = button.outline(1.0, -2.0, k.c.text_accent);
    }
    match crate::board_scene::color(swatch.hex) {
        Some(colour) => button = button.fill(colour),
        None => button = button.child(icon(&NO_FILL, 16.0).color(k.c.icon_muted)),
    }
    button
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contrast::{TEXT, UI, enforce_kit, meets, ratio};

    /// Excalidraw's papers the palette can sit on: its own white and a dark
    /// `viewBackgroundColor` such as the one in the Board tests.
    fn papers() -> [Color; 2] {
        [
            rune::color::hex("#ffffff").expect("white"),
            rune::color::hex("#121212").expect("dark paper"),
        ]
    }

    /// The four resolved appearances: OpenCode light/dark and an Omarchy
    /// palette for each, all repaired like [`crate::resolve_kit`].
    fn kits() -> [(&'static str, Kit); 4] {
        let mut kits = Vec::new();
        for dark in [true, false] {
            let mut kit = rune_theme_opencode::kit(dark).clone();
            enforce_kit(&mut kit);
            kits.push((if dark { "dark" } else { "light" }, kit));
        }
        for (name, palette) in [
            (
                "omarchy-dark",
                "mode = \"dark\"\nbackground = \"#141414\"\nforeground = \"#e1e1e1\"\naccent = \"#34d399\"\n",
            ),
            (
                "omarchy-light",
                "mode = \"light\"\nbackground = \"#f5f4ed\"\nforeground = \"#3f4a45\"\naccent = \"#064e3b\"\n",
            ),
        ] {
            let mut kit = crate::omarchy::OmarchyColors::parse(palette)
                .and_then(|colors| colors.kit())
                .expect("omarchy kit");
            enforce_kit(&mut kit);
            kits.push((name, kit));
        }
        kits.try_into().expect("four kits")
    }

    /// #72 follow-up: a swatch may legitimately match the panel (a pastel on
    /// a light panel), but its ring must then make it findable. The panel is
    /// opaque, so it is the same surface on either board paper; the ring also
    /// clears 3:1 against both raw papers on its own.
    #[test]
    fn every_swatch_is_findable_on_either_paper() {
        let papers = papers();
        for (name, k) in kits() {
            // The panel is opaque (it fills with `layer_02` directly): board
            // paper never shows through it, so the same rules hold on either.
            let panel = k.c.layer_02;
            assert_eq!(panel.a, 1.0, "{name}: the palette panel is opaque");
            // The row labels and the "no fill" icon sit on the panel too.
            assert!(
                meets(k.c.text_faint, panel, TEXT),
                "{name}: palette labels {} on {} is {:.2}",
                k.c.text_faint.hex(),
                panel.hex(),
                ratio(k.c.text_faint, panel),
            );
            assert!(
                meets(k.c.icon_muted, panel, UI),
                "{name}: no-fill icon {} on {} is {:.2}",
                k.c.icon_muted.hex(),
                panel.hex(),
                ratio(k.c.icon_muted, panel),
            );

            // The neutral ring is the boundary in both states; the chosen
            // swatch only adds an inset accent ring.
            let ring = rune::color::hex(SWATCH_RING).expect("ring");
            assert!(
                meets(ring, panel, UI),
                "{name}: ring {} on panel {} is {:.2}",
                ring.hex(),
                panel.hex(),
                ratio(ring, panel),
            );
            for paper in papers {
                assert!(
                    meets(ring, paper, UI),
                    "{name}: ring {} on paper {} is {:.2}",
                    ring.hex(),
                    paper.hex(),
                    ratio(ring, paper),
                );
            }
            // The selection accent is decoration inside the ring, so it is
            // audited on the panel only.
            assert!(
                meets(k.c.text_accent, panel, UI),
                "{name}: chosen accent {} on panel {} is {:.2}",
                k.c.text_accent.hex(),
                panel.hex(),
                ratio(k.c.text_accent, panel),
            );

            // Excalidraw's swatches are not chosen for the app's palette: some
            // fills match the panel and are findable only by their ring. Pin
            // that the ring carries those, and that at least one needs it.
            let mut matched = 0;
            for kind in [Kind::Stroke, Kind::Fill] {
                for swatch in kind.swatches() {
                    let Some(fill) = crate::board_scene::color(swatch.hex) else {
                        continue;
                    };
                    if meets(fill, panel, UI) {
                        continue;
                    }
                    matched += 1;
                    assert!(
                        meets(ring, panel, UI),
                        "{name}: {:?} {} has no ring on {}",
                        kind,
                        swatch.name,
                        panel.hex(),
                    );
                }
            }
            assert!(
                matched > 0,
                "{name}: no swatch needed its ring; the audit is not exercising it"
            );
        }
    }

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
