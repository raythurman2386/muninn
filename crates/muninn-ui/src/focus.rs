//! #112 Focus mode: the note page with nothing but the note.
//!
//! Focus mode is a session-only layer over the ordinary chrome, not a mode of
//! its own. While it is on (and the page is a note page, Write or Read) the
//! notes sidebar, the note title bar with the mode switcher and the links rail
//! are hidden; Write keeps dimming every paragraph but the caret's and asks
//! Rune to hold the caret's line in the middle of the editor.
//!
//! It never touches the sidebar's own shown/hidden choice:
//! [`State::sidebar_visible`] reports `false` while Focus is on, and the choice
//! underneath is exactly what leaving Focus shows again. `⌘\` keeps toggling
//! that choice only.
//!
//! Rune's `text_input` has no "inactive line" style of its own, so the dimming
//! is built from what it does expose: [`El::spans`](rune::El::spans) paints the
//! byte ranges of every other block in a fainter ink, and a transparent
//! [`TextHighlight`] over the caret's character gives [`Tx::scroll_center`] a
//! keyed target to centre. Both are per frame; nothing is written to the file.

use std::ops::Range;

use rune::{Color, Span, TextHighlight, Tx};

use crate::{Mode, State};

/// Key of the transparent highlight over the caret's line, used to centre it.
pub(crate) const CARET_MARK: &str = "focus-caret-line";

/// Key of the editor's scroll viewport (see `view::editor`).
pub(crate) const EDITOR_SCROLL: &str = "editor-scroll";

/// The Focus mode chord, as the palette and Help label it.
pub(crate) const FOCUS_CHORD: &str = if cfg!(target_os = "macos") {
    "⌘⇧F"
} else {
    "Ctrl+Shift+F"
};

/// The blocks of `text` in byte ranges, split on blank lines.
///
/// A block is a run of consecutive lines that are all blank or all non-blank,
/// so prose paragraphs and code blocks stay whole while the blank line between
/// them is a block of its own (a caret resting there dims every paragraph, as
/// it is not in any of them). Ranges include each line's line break.
pub(crate) fn blocks(text: &str) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut blank: Option<bool> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let is_blank = line.trim().is_empty();
        if blank != Some(is_blank) {
            out.push(offset..offset);
            blank = Some(is_blank);
        }
        if let Some(block) = out.last_mut() {
            block.end = offset + line.len();
        }
        offset += line.len();
    }
    out
}

/// The block containing `caret`, clamped to the text; the last block for a
/// caret at the very end, and `0..0` for empty text.
pub(crate) fn active_block(text: &str, caret: usize) -> Range<usize> {
    let blocks = blocks(text);
    let Some(last) = blocks.last() else {
        return 0..0;
    };
    let caret = caret.min(text.len());
    blocks
        .iter()
        .find(|block| caret < block.end)
        .cloned()
        .unwrap_or_else(|| last.clone())
}

/// A fainter ink for every block of `text` except the one holding `caret`.
///
/// These are the spans a focus-mode `text_input` paints, so the paragraph the
/// user is writing keeps the field's own colour and the rest recedes.
pub(crate) fn dim_spans(text: &str, caret: usize, dim: Color) -> Vec<Span> {
    let active = active_block(text, caret);
    blocks(text)
        .into_iter()
        .filter(|block| *block != active && block.start < block.end)
        .map(|block| Span::new(block.start, block.end).color(dim))
        .collect()
}

/// A one-character range on the caret's line, for the transparent highlight
/// [`State::focus_centre_tick`] centres.
///
/// The character at the caret when there is one; otherwise the last one before
/// it (the end of a line, or of the note), which is on the same visual line.
/// A caret on an empty line falls back to the line above. `None` for a note
/// with no characters at all, where there is nothing to centre.
pub(crate) fn caret_mark_range(text: &str, caret: usize) -> Option<Range<usize>> {
    let mut caret = caret.min(text.len());
    while caret > 0 && !text.is_char_boundary(caret) {
        caret -= 1;
    }
    if let Some(ch) = text[caret..].chars().next()
        && ch != '\n'
        && ch != '\r'
    {
        return Some(caret..caret + ch.len_utf8());
    }
    // Walk back over line breaks (a caret at the end of the note, or on a
    // line of its own) to the last character that draws a line box.
    let mut end = caret;
    while let Some((start, ch)) = text[..end].char_indices().next_back() {
        if ch != '\n' && ch != '\r' {
            return Some(start..start + ch.len_utf8());
        }
        end = start;
    }
    None
}

impl State {
    /// Whether Focus mode is on and applies to the page on screen.
    ///
    /// Focus is a note-page feature: Graph, Board and Kanban keep their chrome
    /// (and [`State::set_mode`] leaves Focus when one of them opens).
    #[must_use]
    pub fn focus_active(&self) -> bool {
        self.focus_mode && self.mode.is_note_mode()
    }

    /// Enter Focus mode, or leave it. Only note pages can enter it, and the
    /// chord is not a page overlay's: Settings and Help keep their own layout.
    pub fn toggle_focus_mode(&mut self) {
        if self.settings_open || self.help {
            return;
        }
        if self.focus_active() {
            self.leave_focus_mode();
        } else if self.mode.is_note_mode() {
            self.focus_mode = true;
            self.focus_caret_seen = None;
            // The sidebar goes out of sight with the rest of the chrome, so a
            // field in it must not keep the keyboard (as hiding it does).
            if self.sidebar_shown() {
                self.release_sidebar_fields();
            }
            if self.mode == Mode::Editor {
                // A press on the button takes the keyboard; writing is the
                // point of the mode, so the note keeps it.
                self.focus_editor();
            }
        }
    }

    /// Leave Focus mode and bring the ordinary chrome back. The sidebar shows
    /// again with the state it had before Focus, because Focus never changed
    /// it.
    pub fn leave_focus_mode(&mut self) {
        self.focus_mode = false;
        self.focus_caret_seen = None;
    }

    /// Whether the notes sidebar is on screen: Focus mode hides it without
    /// changing the user's own shown/hidden choice.
    #[must_use]
    pub fn sidebar_visible(&self) -> bool {
        self.sidebar_shown() && !self.focus_active()
    }

    /// The faint spans that dim every paragraph but the caret's, for the Write
    /// editor. Empty when Focus is off.
    #[must_use]
    pub fn focus_dim_spans(&self, dim: Color) -> Vec<Span> {
        if !self.focus_active() || self.mode != Mode::Editor {
            return Vec::new();
        }
        dim_spans(&self.editor.text, self.editor.focus, dim)
    }

    /// The transparent highlight over the caret's line, when Focus is on and
    /// the note has text to mark.
    #[must_use]
    pub fn focus_caret_mark(&self) -> Option<TextHighlight> {
        if !self.focus_active() || self.mode != Mode::Editor {
            return None;
        }
        let range = caret_mark_range(&self.editor.text, self.editor.focus)?;
        Some(TextHighlight::new(range, Color::TRANSPARENT).key(CARET_MARK))
    }

    /// Per-frame hook: after the caret moved in Focus mode, ask Rune to centre
    /// its line in the editor's viewport.
    ///
    /// One request per caret position, so a user who wheels the note away is
    /// left where they put it until the caret moves again. The request is
    /// resolved from the commit's own layout, and it jumps rather than glides:
    /// typewriter scrolling moves a line at a time, and a glide across a long
    /// jump would lag the typing it belongs to.
    pub(crate) fn focus_centre_tick(&mut self, tx: &mut Tx) -> bool {
        if !self.focus_active() || self.mode != Mode::Editor {
            self.focus_caret_seen = None;
            return false;
        }
        let mut caret = self.editor.focus.min(self.editor.text.len());
        while caret > 0 && !self.editor.text.is_char_boundary(caret) {
            caret -= 1;
        }
        if self.focus_caret_seen == Some(caret) {
            return false;
        }
        self.focus_caret_seen = Some(caret);
        if caret_mark_range(&self.editor.text, caret).is_none() {
            return false;
        }
        tx.scroll_center_in_instant(EDITOR_SCROLL, CARET_MARK);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "First paragraph.\n\nSecond paragraph,\nover two lines.\n\nThird.\n";

    #[test]
    fn blocks_split_on_blank_lines_and_keep_line_breaks() {
        assert_eq!(
            blocks(NOTE),
            vec![
                0..17,  // "First paragraph.\n"
                17..18, // the blank line
                18..52, // "Second paragraph,\nover two lines.\n"
                52..53, // the blank line
                53..60, // "Third.\n"
            ]
        );
        assert_eq!(blocks(""), Vec::<Range<usize>>::new());
        assert_eq!(blocks("only one line"), vec![0..13]);
    }

    #[test]
    fn the_active_block_is_the_one_holding_the_caret() {
        assert_eq!(active_block(NOTE, 0), 0..17);
        assert_eq!(active_block(NOTE, 16), 0..17, "the line break ends it");
        assert_eq!(active_block(NOTE, 17), 17..18, "a blank line is its own");
        assert_eq!(active_block(NOTE, 25), 18..52);
        assert_eq!(active_block(NOTE, 40), 18..52, "a wrapped line stays");
        assert_eq!(active_block(NOTE, 53), 53..60);
        assert_eq!(active_block(NOTE, NOTE.len()), 53..60, "the end");
        assert_eq!(active_block(NOTE, 99_999), 53..60, "clamped");
        assert_eq!(active_block("", 0), 0..0);
    }

    #[test]
    fn dimming_leaves_the_caret_paragraph_alone() {
        let dim = Color::WHITE.alpha(0.4);
        let spans = dim_spans(NOTE, 25, dim);
        let ranges: Vec<(usize, usize)> = spans
            .iter()
            .map(|s| (s.start as usize, s.end as usize))
            .collect();
        assert_eq!(ranges, vec![(0, 17), (17, 18), (52, 53), (53, 60)]);
        assert!(spans.iter().all(|s| s.color == Some(dim)));
        let active = active_block(NOTE, 25);
        assert!(
            !ranges
                .iter()
                .any(|&(a, b)| a < active.end && b > active.start),
            "no span overlaps the block holding the caret"
        );
    }

    #[test]
    fn a_caret_on_a_blank_line_dims_every_paragraph() {
        let spans = dim_spans(NOTE, 17, Color::WHITE);
        let ranges: Vec<(usize, usize)> = spans
            .iter()
            .map(|s| (s.start as usize, s.end as usize))
            .collect();
        assert_eq!(ranges, vec![(0, 17), (18, 52), (52, 53), (53, 60)]);
    }

    #[test]
    fn single_paragraph_notes_need_no_dimming() {
        assert!(dim_spans("Just one paragraph.\n", 4, Color::WHITE).is_empty());
        assert!(dim_spans("", 0, Color::WHITE).is_empty());
    }

    #[test]
    fn the_caret_mark_is_one_character_on_its_line() {
        assert_eq!(caret_mark_range("hello\nworld\n", 0), Some(0..1));
        assert_eq!(caret_mark_range("hello\nworld\n", 7), Some(7..8));
        assert_eq!(
            caret_mark_range("hello\nworld\n", 5),
            Some(4..5),
            "before the break"
        );
        assert_eq!(
            caret_mark_range("hello\nworld\n", 12),
            Some(10..11),
            "at the end, past the last break"
        );
        assert_eq!(caret_mark_range("hi", 2), Some(1..2), "at the end");
        assert_eq!(caret_mark_range("é ok", 0), Some(0..2), "a whole character");
        assert_eq!(caret_mark_range("\n\n", 1), None, "nothing to mark");
        assert_eq!(caret_mark_range("", 0), None);
    }
}
