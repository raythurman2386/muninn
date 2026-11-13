//! Kanban slice 11: metadata from a card's linked note.
//!
//! - A card's first wikilink selects the note to read, through the vault's
//!   resolver. A heading, block id or alias does not change the note.
//! - The board footer's `linked-page-metadata` (or `kanban_defaults`) names
//!   the keys to draw at the card's foot, after dates and tags. A key comes
//!   from the linked note's flat frontmatter or an inline field; matching is
//!   case-insensitive and the first occurrence wins.
//! - `containsMarkdown` renders the value through the reader: links work, a
//!   note embed becomes a link, and an attachment embed keeps the reader's
//!   filename placeholder. Otherwise it is plain text.
//!   `show-missing-metadata` draws `key: —` for an absent key or note.
//! - Everything here is display-only. Viewing or a dirty board never changes
//!   a byte.

use muninn_core::{Kanban, MetadataRow, MetadataSettings};
use rune::{El, col, row, text};
use rune_kit::{Kit, Markdown, MarkdownStyle};

use crate::State;

/// The longest value drawn on one card. A value is clipped at a character
/// boundary so a very long or multibyte value cannot make the card huge.
const VALUE_CLIP: usize = 600;

impl State {
    /// The metadata settings the shown board displays: its footer first, then
    /// `kanban_defaults` from `settings.json`, then the built-in value.
    #[must_use]
    pub fn kanban_metadata_settings(&self) -> MetadataSettings {
        MetadataSettings::resolve(
            self.kanban_board().and_then(Kanban::footer_json),
            &self.settings.kanban_defaults,
        )
    }
}

/// The card's metadata rows, already resolved from the board's metadata
/// cache. `value_note` is the linked note a markdown value's relative embeds
/// resolve against.
pub(crate) fn card_metadata_rows(
    s: &State,
    k: &Kit,
    rows: &[MetadataRow],
    value_note: Option<usize>,
    i: usize,
    j: usize,
) -> El {
    if rows.is_empty() {
        return rune::el();
    }
    let mut column = col()
        .key(format!("kanban-card-{i}-{j}-metadata"))
        .w_full()
        .gap(2.0);
    for (index, metadata) in rows.iter().enumerate() {
        let key = metadata.key.clone();
        let label = text(format!("{key}:"))
            .font(k.ty.caption())
            .color(k.c.text_faint);
        let value = match &metadata.value {
            None => text("—")
                .key(format!("kanban-card-{i}-{j}-metadata-{index}"))
                .font(k.ty.caption())
                .color(k.c.text_faint),
            Some(value) if metadata.contains_markdown => {
                let value = clip(value);
                let source = crate::kanban_view::card_markdown(&s.vault, value_note, &value);
                Markdown::new(source)
                    .style(MarkdownStyle::from_kit(k).link(k.c.text_accent))
                    .link_style(crate::view::missing_link_style(k))
                    .on_link_tx(|s: &mut State, url: &str, tx: &mut rune::Tx| {
                        s.open_card_link(url, tx)
                    })
                    .tasks_read_only()
                    .build(k)
                    .key(format!("kanban-card-{i}-{j}-metadata-{index}"))
            }
            Some(value) => text(clip(value))
                .key(format!("kanban-card-{i}-{j}-metadata-{index}"))
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .wrap(),
        };
        column = column.child(
            row()
                .key(format!("kanban-card-{i}-{j}-metadata-row-{index}"))
                .w_full()
                .gap(4.0)
                .items_start()
                .child(label)
                .child(value),
        );
    }
    column
}

/// `value` cut to [`VALUE_CLIP`] characters with an ellipsis. Clipping happens
/// on a character boundary, so multibyte text is never split.
fn clip(value: &str) -> String {
    if value.chars().count() <= VALUE_CLIP {
        return value.to_owned();
    }
    let mut clipped: String = value.chars().take(VALUE_CLIP).collect();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_multibyte_value_is_clipped_on_a_character_boundary() {
        let value = "漢".repeat(VALUE_CLIP + 10);
        let clipped = clip(&value);
        assert_eq!(clipped.chars().count(), VALUE_CLIP + 1);
        assert!(clipped.ends_with('…'));
        assert!(clipped.starts_with('漢'));

        let short = "e\u{301}👍🏽漢";
        assert_eq!(clip(short), short);
    }
}
