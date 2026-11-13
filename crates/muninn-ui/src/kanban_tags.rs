//! Kanban slice 10: tags on cards.
//!
//! - A card's tags are the `#tag` tokens slice 9's [`scan_card_text`] already
//!   finds, outside fences and inline code.
//! - Board footer keys `hide-tags-in-title` and `show-tags-at-bottom` (and
//!   app-wide `kanban_defaults`) hide the tokens from the card's main text
//!   and/or draw chips at the card's foot. `tag-colors` is kept as written
//!   for a later colour editor; it is not interpreted here.
//! - A click on a chip sets the board's tag filter (slice 13,
//!   `kanban_search.rs`), so the card list and the header's filter summary
//!   agree. It does not write the board.
//! - Display and filtering never write the board: the file is the source of
//!   truth and only an edit changes a byte.

use std::collections::HashSet;

use muninn_core::{CardText, Kanban, TagSettings};
use rune::{El, row, text};
use rune_kit::Kit;

use crate::State;

impl State {
    /// The tag settings the shown board displays: its footer first, then
    /// `kanban_defaults` from `settings.json`, then the built-in value.
    #[must_use]
    pub fn kanban_tag_settings(&self) -> TagSettings {
        TagSettings::resolve(
            self.kanban_board().and_then(Kanban::footer_json),
            &self.settings.kanban_defaults,
        )
    }

    /// A tag chip's click: set the board's tag filter to `tag`. The header
    /// then shows what is filtered and its Clear control, and the cards
    /// without the tag are hidden. Nothing is written.
    pub fn filter_by_tag(&mut self, tag: &str) {
        self.set_kanban_tag_filter(tag);
    }
}

/// The card's tags with case-insensitive duplicates removed, first spelling
/// first. One chip per tag, and the same list decides a chip's stable index.
pub(crate) fn unique_tags(found: &CardText) -> Vec<&str> {
    let mut seen = HashSet::new();
    found
        .tags
        .iter()
        .filter(|token| seen.insert(token.tag.to_lowercase()))
        .map(|token| token.tag.as_str())
        .collect()
}

/// The card's tag chips, when `show-tags-at-bottom` asks for them. A tag
/// repeated in one card is shown once, with the first spelling.
pub(crate) fn card_tags(
    k: &Kit,
    settings: &TagSettings,
    found: &CardText,
    i: usize,
    j: usize,
) -> El {
    if !settings.show_tags_at_bottom || found.tags.is_empty() {
        return rune::el();
    }
    tag_chips(k, found, i, j, "kanban-card", None)
}

/// A card's tag chips as a row. The list and the table reuse it: the table
/// gives it its Tags column width and draws it whichever
/// `show-tags-at-bottom` says, because tags have a column of their own there
/// (spec section 5, **Views**).
pub(crate) fn tag_chips(
    k: &Kit,
    found: &CardText,
    i: usize,
    j: usize,
    prefix: &str,
    width: Option<f32>,
) -> El {
    let mut line = row()
        .key(format!("{prefix}-{i}-{j}-tags"))
        .w_full()
        .gap(4.0)
        .center_y();
    if let Some(width) = width {
        line = line.w(width);
    }
    for (index, tag) in unique_tags(found).into_iter().enumerate() {
        let filter = tag.to_owned();
        line = line.child(
            row()
                .key(format!("{prefix}-{i}-{j}-tag-{index}"))
                .h(18.0)
                .px(6.0)
                .radius(4.0)
                .center_y()
                .fill(k.alpha(k.c.text_accent, 0.10))
                .hover(k.c.hover)
                .press(k.c.pressed)
                .cursor(rune::Cursor::Pointer)
                .role("button")
                .label(format!("Filter by #{tag}"))
                .on_press(move |s: &mut State| s.filter_by_tag(&filter))
                .child(
                    text(format!("#{tag}"))
                        .font(k.ty.caption())
                        .color(k.c.text_accent),
                ),
        );
    }
    line
}

#[cfg(test)]
mod tests {
    use muninn_core::scan_card_text;

    use super::unique_tags;

    #[test]
    fn a_repeated_tag_is_one_chip() {
        let found = scan_card_text("Ship #release and #release #docs", &Default::default());
        assert_eq!(unique_tags(&found), ["release", "docs"]);
        let case = scan_card_text("#Ops #ops #OPS", &Default::default());
        assert_eq!(unique_tags(&case), ["Ops"], "the first spelling stays");
    }
}
