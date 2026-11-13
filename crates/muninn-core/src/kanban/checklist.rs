//! Checklist items inside a card (slice 13).
//!
//! A card's continuation lines may hold nested task-list items, which
//! `docs/kanban-format.md` section 5 calls the card's checklist. The card's
//! own first line is the card's checkbox, never a checklist item. Fenced
//! blocks and inline code are not task items, as they are not tags or dates.
//!
//! A checklist item is read by [`card_checklist`] and toggled through
//! [`Kanban::toggle_card_task`](crate::kanban::Kanban::toggle_card_task),
//! which changes exactly the one character of that item through the card
//! edit path. The file is still the source of truth: nothing here writes it.

use super::parse::{fence_closes, indent_columns, opens_fence};
use super::{Kanban, invalid_input};

/// One nested task item in a card's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChecklistItem {
    /// Zero-based line in the card's text. The card's own line is `0`, so a
    /// checklist item is always at least `1`.
    line: usize,
    /// Leading indentation, in columns (a tab advances to the next multiple
    /// of four), of the item in the card's text.
    indent: usize,
    /// The character between the brackets. A space is open.
    mark: char,
    /// Byte offset of `mark` in the card's text.
    mark_at: usize,
    /// The item's text after its checkbox and one following space or tab.
    text: String,
}

impl ChecklistItem {
    /// Zero-based line in the card's text; never the card's own line.
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }

    /// Leading indentation in columns.
    #[must_use]
    pub fn indent(&self) -> usize {
        self.indent
    }

    /// The character between the brackets. A space is open.
    #[must_use]
    pub fn mark(&self) -> char {
        self.mark
    }

    /// True when the item is checked.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.mark != ' '
    }

    /// Byte offset of the done mark in the card's text.
    #[must_use]
    pub fn mark_at(&self) -> usize {
        self.mark_at
    }

    /// The item's text after its checkbox.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Every nested task item in a card's `text`, in file order.
///
/// The first line is the card itself and is never an item. Items inside a
/// fenced block are skipped, and a line that only looks like an item inside
/// an inline code span does not start with the bullet at the start of the
/// line, so it is skipped too. A malformed item (no space after the bullet,
/// an empty or unclosed bracket, a longer bracket) is ordinary continuation
/// text, not a panic.
#[must_use]
pub fn card_checklist(text: &str) -> Vec<ChecklistItem> {
    let mut items = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut line_start = 0;
    for (line, raw) in text.split_inclusive('\n').enumerate() {
        let body = raw.strip_suffix('\n').unwrap_or(raw);
        let body = body.strip_suffix('\r').unwrap_or(body);
        if let Some((marker, run)) = fence {
            if fence_closes(body, marker, run) {
                fence = None;
            }
            line_start += raw.len();
            continue;
        }
        if let Some(opened) = opens_fence(body) {
            fence = Some(opened);
            line_start += raw.len();
            continue;
        }
        if line > 0
            && let Some(found) = item_in_line(body, line, line_start)
        {
            items.push(found);
        }
        line_start += raw.len();
    }
    items
}

/// Read one line as a task item. `line_start` is the line's byte offset in the
/// card text.
fn item_in_line(body: &str, line: usize, line_start: usize) -> Option<ChecklistItem> {
    let lead_len = body
        .chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .map(char::len_utf8)
        .sum();
    let indent = indent_columns(&body[..lead_len]);
    let rest = &body[lead_len..];
    let bullet = bullet_len(rest)?;
    let after_bullet = &rest[bullet..];
    let after_lead = after_bullet
        .chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .map(char::len_utf8)
        .sum::<usize>();
    if after_lead == 0 {
        return None;
    }
    let boxed = &after_bullet[after_lead..];
    let boxed_at = lead_len + bullet + after_lead;
    let bytes = boxed.as_bytes();
    if bytes.first() != Some(&b'[') {
        return None;
    }
    let mark = boxed[1..].chars().next()?;
    let after_mark = 1 + mark.len_utf8();
    if bytes.get(after_mark) != Some(&b']') {
        return None;
    }
    let after_box = after_mark + 1;
    match bytes.get(after_box) {
        None => {}
        Some(b' ') | Some(b'\t') => {}
        Some(_) => return None,
    }
    let text_at = if matches!(bytes.get(after_box), Some(b' ' | b'\t')) {
        after_box + 1
    } else {
        after_box
    };
    let mark_at = line_start + boxed_at + 1;
    Some(ChecklistItem {
        line,
        indent,
        mark,
        mark_at,
        text: boxed[text_at..].trim_end().to_owned(),
    })
}

/// The byte length of a list bullet and its following whitespace: `- `,
/// `* `, `+ `, or `1. ` / `1) ` with one or more digits.
fn bullet_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let marker_len = match bytes.first().copied()? {
        b'-' | b'*' | b'+' => 1,
        b'0'..=b'9' => {
            let digits = bytes
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            let delimiter = *bytes.get(digits)?;
            if delimiter != b'.' && delimiter != b')' {
                return None;
            }
            digits + 1
        }
        _ => return None,
    };
    let after = *bytes.get(marker_len)?;
    (after == b' ' || after == b'\t').then_some(marker_len)
}

impl Kanban {
    /// Toggle the nested checklist item at zero-based `line` in the card's
    /// text. Only that one character changes: the card's own checkbox, the
    /// other items and every other line stay as they are. A line that is not
    /// a checklist item is refused and the board is unchanged.
    ///
    /// One call is one undo step in the UI, because it goes through the same
    /// card edit path as every other card text change.
    pub fn toggle_card_task(
        &mut self,
        lane: usize,
        index: usize,
        line: usize,
    ) -> std::io::Result<()> {
        let card = self.card(lane, index)?;
        let text = card.text.clone();
        let item = card_checklist(&text)
            .into_iter()
            .find(|item| item.line == line)
            .ok_or_else(|| invalid_input("that line is not a checklist item"))?;
        let replacement = if item.is_done() { " " } else { "x" };
        let mut next = String::with_capacity(text.len());
        next.push_str(&text[..item.mark_at()]);
        next.push_str(replacement);
        next.push_str(&text[item.mark_at() + item.mark().len_utf8()..]);
        self.edit_card(lane, index, &next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "---\nkanban-plugin: board\n---\n\n## Lane\n\n";

    fn card_text(board: &Kanban) -> String {
        board.lanes()[0].cards()[0].text().to_owned()
    }

    #[test]
    fn nested_items_are_read_with_their_marks_indents_and_offsets() {
        let text = "Outer\n- [ ] one\n  - [x] two\n    continued\n* [ ] three\n1. [x] four";
        let items = card_checklist(text);
        assert_eq!(
            items.iter().map(|item| item.line).collect::<Vec<_>>(),
            [1, 2, 4, 5]
        );
        assert_eq!(
            items
                .iter()
                .map(|item| (item.mark, item.is_done()))
                .collect::<Vec<_>>(),
            [(' ', false), ('x', true), (' ', false), ('x', true)]
        );
        assert_eq!(
            items.iter().map(|item| item.indent).collect::<Vec<_>>(),
            [0, 2, 0, 0]
        );
        assert_eq!(
            items.iter().map(|item| item.text()).collect::<Vec<_>>(),
            ["one", "two", "three", "four"]
        );
        for item in &items {
            assert_eq!(
                &text[item.mark_at..item.mark_at + item.mark.len_utf8()],
                item.mark.to_string()
            );
        }
    }

    #[test]
    fn the_cards_own_line_is_never_a_checklist_item() {
        let text = "- [ ] looks nested\n- [ ] real";
        let items = card_checklist(text);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].line, 1);
        assert_eq!(items[0].text(), "real");
        assert!(card_checklist("- [ ] only the card").is_empty());
    }

    #[test]
    fn fences_hide_items_and_unmatched_or_malformed_lines_are_text() {
        let fenced = "Outer\n```\n- [ ] hidden\n```\n- [ ] shown\n~~~\n- [ ] hidden too\n~~~";
        assert_eq!(
            card_checklist(fenced)
                .iter()
                .map(|i| i.text())
                .collect::<Vec<_>>(),
            ["shown"]
        );
        for malformed in [
            "-[ ] no space",
            "- [] empty",
            "- [ ]x no space after",
            "- [x",
            "- [x]] extra bracket",
            "`- [ ] code`",
            "outdent",
            "- [ ]",
        ] {
            let mut text = String::from("Outer\n");
            text.push_str(malformed);
            let items = card_checklist(&text);
            if malformed == "- [ ]" {
                assert_eq!(items.len(), 1, "an empty item is still an item");
                assert_eq!(items[0].text(), "");
            } else {
                assert!(items.is_empty(), "{malformed:?} must not be an item");
            }
        }
    }

    #[test]
    fn crlf_tabs_and_multibyte_text_keep_byte_offsets() {
        let text = "外\r\n\t- [ ] 漢字\r\n  - [x] café";
        let items = card_checklist(text);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].line, 1);
        assert_eq!(items[0].indent, 4);
        assert_eq!(items[0].text(), "漢字");
        assert_eq!(items[1].indent, 2);
        assert_eq!(items[1].text(), "café");
        for item in &items {
            assert_eq!(
                &text[item.mark_at..item.mark_at + item.mark.len_utf8()],
                item.mark.to_string()
            );
        }
    }

    #[test]
    fn toggling_one_item_changes_only_that_character() {
        let source = format!("{HEAD}- [ ] Outer\n  - [ ] one\n  - [x] two\n  - [ ] 漢字\n");
        let mut board = Kanban::parse(&source).expect("board");
        board.toggle_card_task(0, 0, 3).expect("toggle");
        assert_eq!(
            board.source(),
            source.replacen("  - [ ] 漢字", "  - [x] 漢字", 1)
        );
        assert!(
            board.source().contains("- [ ] Outer"),
            "the outer mark stays"
        );
        assert!(board.source().contains("  - [x] two"), "other items stay");

        board.toggle_card_task(0, 0, 3).expect("toggle back");
        assert_eq!(board.source(), source);
    }

    #[test]
    fn toggling_a_non_item_or_fenced_line_is_refused_without_a_change() {
        let source = format!("{HEAD}- [ ] Outer\n```\n- [ ] hidden\n```\n");
        let mut board = Kanban::parse(&source).expect("board");
        assert!(
            board.toggle_card_task(0, 0, 0).is_err(),
            "the card is not an item"
        );
        assert!(
            board.toggle_card_task(0, 0, 2).is_err(),
            "fenced lines are not items"
        );
        assert!(board.toggle_card_task(0, 0, 99).is_err());
        assert!(board.toggle_card_task(0, 9, 1).is_err());
        assert_eq!(board.source(), source);
    }

    #[test]
    fn toggling_keeps_a_block_id_and_a_one_space_indent() {
        let source = format!("{HEAD}- [ ] Outer ^card-1\n - [ ] one\n");
        let mut board = Kanban::parse(&source).expect("board");
        assert_eq!(board.lanes()[0].cards()[0].block_id(), Some("card-1"));
        board.toggle_card_task(0, 0, 1).expect("toggle");
        assert_eq!(
            board.source(),
            source.replacen(" - [ ] one", " - [x] one", 1)
        );
        assert!(board.source().contains("- [ ] Outer ^card-1"));
    }

    #[test]
    fn crlf_boards_toggle_without_touching_their_line_endings() {
        let source = "---\r\nkanban-plugin: board\r\n---\r\n\r\n## Lane\r\n\r\n- [ ] Outer\r\n  - [ ] one\r\n";
        let mut board = Kanban::parse(source).expect("board");
        let text = card_text(&board);
        let items = card_checklist(&text);
        assert_eq!(items.len(), 1);
        board.toggle_card_task(0, 0, items[0].line).expect("toggle");
        assert_eq!(
            board.source(),
            source.replacen("  - [ ] one", "  - [x] one", 1)
        );
        assert_eq!(
            board.source().matches("\r\n").count(),
            source.matches("\r\n").count()
        );
    }
}
