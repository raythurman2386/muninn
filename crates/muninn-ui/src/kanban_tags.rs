//! Kanban slice 10: tags on cards.
//!
//! - A card's tags are the `#tag` tokens slice 9's [`scan_card_text`] already
//!   finds, outside fences and inline code.
//! - Board footer keys `hide-tags-in-title` and `show-tags-at-bottom` (and
//!   app-wide `kanban_defaults`) hide the tokens from the card's main text
//!   and/or draw chips at the card's foot. `tag-colors` is kept as written
//!   for a later colour editor; it is not interpreted here.
//! - A click on a chip opens the existing Find bar with `#tag` in it. Slice
//!   13 replaces this with the board's own filter model; this slice adds no
//!   second filter state.
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

    /// A tag chip's click: open the existing Find bar with `#tag` in it.
    /// Nothing is filtered or written yet; slice 13 adds the board filter.
    pub fn filter_by_tag(&mut self, tag: &str) {
        let tag = tag.strip_prefix('#').unwrap_or(tag);
        self.open_find();
        if let Some(find) = self.find.as_mut() {
            find.field.replace_text(format!("#{tag}"));
            find.field.focused = true;
            let end = find.field.text.len();
            find.field.set_caret(end);
        }
        self.find_tick();
    }
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
    let mut seen = HashSet::new();
    let mut line = row()
        .key(format!("kanban-card-{i}-{j}-tags"))
        .w_full()
        .gap(4.0)
        .center_y();
    for (index, token) in found.tags.iter().enumerate() {
        if !seen.insert(token.tag.to_lowercase()) {
            continue;
        }
        let tag = token.tag.clone();
        let filter = tag.clone();
        line = line.child(
            row()
                .key(format!("kanban-card-{i}-{j}-tag-{index}"))
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
    use std::collections::HashSet;

    use muninn_core::scan_card_text;

    #[test]
    fn a_repeated_tag_is_one_chip() {
        let found = scan_card_text("Ship #release and #release #docs", &Default::default());
        let mut seen = HashSet::new();
        let tags: Vec<&str> = found
            .tags
            .iter()
            .filter(|token| seen.insert(token.tag.to_lowercase()))
            .map(|token| token.tag.as_str())
            .collect();
        assert_eq!(tags, ["release", "docs"]);
    }
}
