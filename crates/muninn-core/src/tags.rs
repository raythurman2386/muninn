//! Tags in a note: frontmatter `tags`/`tag`, and `#tag` in the body.
//!
//! `docs/kanban-format.md` section 7 fixes the tag rules: `#word` at the
//! start of a line or after whitespace, outside fenced blocks and inline
//! code, with letters, digits, `_`, `-` and `/`; a tag is not only digits, a
//! heading marker is not a tag, and a `#` inside `[[Note#heading]]` is not a
//! tag. Case is kept for display and compared case-insensitively.
//!
//! The body scanner is the card scanner's ([`crate::scan_tags`]), so a card
//! and an ordinary paragraph read tags the same way.

use std::collections::HashSet;

use crate::frontmatter::{Frontmatter, FrontmatterValue};
use crate::kanban::{is_tag_char, is_tag_text, scan_tags};

/// Every tag in `source`, unique by case-insensitive key, in file order
/// (frontmatter first, then the body). The first spelling seen is the one
/// kept for display.
#[must_use]
pub fn note_tags(source: &str) -> Vec<String> {
    let front = Frontmatter::parse(source);
    let mut tags = Vec::new();
    let mut seen = HashSet::new();
    for key in ["tags", "tag"] {
        match front.get(key) {
            Some(FrontmatterValue::Text(text)) => {
                for item in split_items(text) {
                    push_tag(&mut tags, &mut seen, item);
                }
            }
            Some(FrontmatterValue::List(items)) => {
                for item in items {
                    for part in split_items(item) {
                        push_tag(&mut tags, &mut seen, part);
                    }
                }
            }
            None => {}
        }
    }
    let body = source.get(front.body_start()..).unwrap_or_default();
    for token in scan_tags(body) {
        push_tag(&mut tags, &mut seen, &token.tag);
    }
    tags
}

/// The lower-case key a tag is grouped and matched by.
#[must_use]
pub fn tag_key(tag: &str) -> String {
    tag.to_lowercase()
}

/// Add `tag` when it is a valid tag and its key is new. A leading `#` is
/// accepted, so both `tags: work` and `tags: #work` read as `work`.
fn push_tag(tags: &mut Vec<String>, seen: &mut HashSet<String>, tag: &str) {
    let tag = tag.strip_prefix('#').unwrap_or(tag);
    if !is_tag_text(tag) {
        return;
    }
    if seen.insert(tag_key(tag)) {
        tags.push(tag.to_owned());
    }
}

/// Split a scalar frontmatter value into tags: commas or whitespace separate
/// them, because a tag itself cannot contain either.
fn split_items(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|item| !item.is_empty())
}

/// The `#…` word being typed at `caret`, when the card editor should offer
/// vault tags. `open` is the byte of the `#`; the query is the text after it.
///
/// A `#` only opens completion at the start of the text or after whitespace,
/// so an email (`a#b`), a URL fragment (`/#frag`) and a link target
/// (`[[Note#heading]]`) do not. A heading marker (`# Heading`, `##`) is not a
/// tag either.
#[must_use]
pub fn unclosed_tag(text: &str, caret: usize) -> Option<(usize, String)> {
    let before = text.get(..caret)?;
    let open = before.rfind('#')?;
    let boundary = before[..open]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    if !boundary {
        return None;
    }
    let query = &before[open + 1..];
    if !query.chars().all(is_tag_char) {
        return None;
    }
    // `# Heading` and a second `#` are heading markers, not a tag.
    if text[caret..].starts_with([' ', '\t', '#']) {
        return None;
    }
    if crate::unclosed_link(text, caret).is_some() {
        return None;
    }
    Some((open, query.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_tags_are_scalar_list_quoted_or_absent() {
        let scalar = "---\ntags: work\n---\n#work\n";
        assert_eq!(note_tags(scalar), ["work"]);
        let list = "---\ntags:\n  - work\n  - home/garden\n---\n";
        assert_eq!(note_tags(list), ["work", "home/garden"]);
        let quoted = "---\ntags: \"alpha, beta\"\n---\n";
        assert_eq!(note_tags(quoted), ["alpha", "beta"]);
        let singular = "---\ntag: '#work'\n---\n";
        assert_eq!(note_tags(singular), ["work"]);
        let absent = "---\ntitle: x\n---\nno tags\n";
        assert!(note_tags(absent).is_empty());
    }

    #[test]
    fn body_tags_keep_the_first_case_and_skip_the_frontmatter() {
        let source = "---\ntags: Ops\n---\n\n#Ops #ops #ui/menu #123 \\#not\n";
        assert_eq!(note_tags(source), ["Ops", "ui/menu"]);
    }

    #[test]
    fn a_kanban_card_is_part_of_the_note_body() {
        let board =
            "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Ship #release\n  more #release\n";
        assert_eq!(note_tags(board), ["release"]);
    }

    #[test]
    fn unclosed_tag_needs_a_word_boundary_and_is_not_a_link_url_or_heading() {
        assert_eq!(unclosed_tag("see #wo", 7), Some((4, "wo".to_owned())));
        assert_eq!(unclosed_tag("#", 1), Some((0, String::new())));
        assert_eq!(unclosed_tag("see #", 5), Some((4, String::new())));
        assert_eq!(unclosed_tag("a#b", 3), None, "not after whitespace");
        assert_eq!(unclosed_tag("https://x/#frag", 16), None, "a URL fragment");
        assert_eq!(unclosed_tag("mail a#b@c", 10), None, "an email");
        assert_eq!(unclosed_tag("[[Note#hea", 10), None, "a link target");
        assert_eq!(unclosed_tag("# Heading", 1), None, "a heading marker");
        assert_eq!(unclosed_tag("##", 2), None, "a second # is a heading");
        assert_eq!(unclosed_tag("a #b c", 6), None, "past a space");
    }

    #[test]
    fn the_vault_index_is_sorted_groups_case_and_lists_cards() {
        use crate::testing::TempVault;

        let tmp = TempVault::new(&[
            ("A.md", "#Alpha and #shared\n"),
            ("B.md", "---\ntags: [alpha, Beta]\n---\nbody\n"),
            (
                "Board.md",
                "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Ship #Gamma\n",
            ),
        ]);
        let vault = tmp.load();
        let keys: Vec<&str> = vault.tag_index().keys().map(String::as_str).collect();
        assert_eq!(keys, ["alpha", "beta", "gamma", "shared"]);

        let alpha = &vault.tag_index()["alpha"];
        assert_eq!(alpha.tag, "Alpha", "the first spelling is kept");
        let a = vault.resolve("A").expect("A");
        let b = vault.resolve("B").expect("B");
        assert_eq!(alpha.notes, [a, b], "ids are in note order");

        let board = vault.resolve("Board").expect("Board");
        assert!(vault.notes[board].is_kanban());
        assert_eq!(vault.notes[board].tags(), ["Gamma"]);
        assert_eq!(vault.notes_with_tag("#GAMMA"), [board]);
        assert!(vault.notes_with_tag("missing").is_empty());
        assert_eq!(vault.suggest_tags("al", 8), ["Alpha"]);
        assert_eq!(vault.suggest_tags("", 2), ["Alpha", "Beta"]);

        // A save rebuilds the same index, not a growing one.
        let mut vault = vault;
        vault.reindex();
        assert_eq!(vault.tag_index()["alpha"].notes, [a, b]);
    }
}
