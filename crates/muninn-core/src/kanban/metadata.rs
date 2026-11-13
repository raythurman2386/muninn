//! Card metadata from a card's linked note (slice 11).
//!
//! A card's first wikilink selects a note through the vault's own resolver; a
//! heading, block id or alias in the link does not change the note. The
//! board's `linked-page-metadata` setting names the keys to show at the card's
//! foot. A key is read from the linked note's flat frontmatter
//! ([`crate::frontmatter`]) or from an inline field, `[key:: value]` or a line
//! `key:: value`, outside fenced code and inline code. The first occurrence of
//! a key wins and matching is case-insensitive; `docs/kanban-format.md`
//! section 7 resolves open point T-1 with these rules.
//!
//! This is a read-only view. Nothing here writes a note or a board, and every
//! value is display text; the bytes stay exactly as they were.

use std::collections::HashSet;
use std::ops::Range;

use serde_json::{Map, Value};

use crate::frontmatter::{Frontmatter, FrontmatterValue};
use crate::vault::Vault;
use crate::wikilink::parse_links;

use super::dates::KanbanDefaults;
use super::parse::{fence_closes, opens_fence};
use super::text::{code_ranges, in_code};

/// The footer key: the metadata keys to show from a card's linked note.
pub const LINKED_PAGE_METADATA: &str = "linked-page-metadata";
/// The footer key: draw `key: —` when a configured key is absent.
pub const SHOW_MISSING_METADATA: &str = "show-missing-metadata";

/// One configured `linked-page-metadata` entry.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MetadataField {
    /// The note key, as the board wrote it.
    pub key: String,
    /// Render the value as markdown (links and embeds) rather than text.
    pub contains_markdown: bool,
}

/// The metadata settings a board shows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct MetadataSettings {
    /// In written order. An empty list draws nothing.
    pub linked_page_metadata: Vec<MetadataField>,
    /// Draw `key: —` for a key, or a note, that is missing.
    pub show_missing_metadata: bool,
}

impl MetadataSettings {
    /// Footer, then app defaults, then the built-in value.
    #[must_use]
    pub fn resolve(footer_json: Option<&str>, defaults: &KanbanDefaults) -> Self {
        let mut settings = Self {
            linked_page_metadata: defaults.linked_page_metadata.clone().unwrap_or_default(),
            show_missing_metadata: defaults.show_missing_metadata.unwrap_or(false),
        };
        if let Some(value) = footer_json.and_then(|json| serde_json::from_str::<Value>(json).ok()) {
            settings.apply_value(&value);
        }
        settings
    }

    /// Read the keys this type owns from one JSON object. Other keys, and a
    /// value of the wrong type, are ignored; the current value stays.
    pub fn apply_value(&mut self, value: &Value) {
        if let Some(fields) = metadata_fields(value.get(LINKED_PAGE_METADATA)) {
            self.linked_page_metadata = fields;
        }
        if let Some(flag) = value.get(SHOW_MISSING_METADATA).and_then(Value::as_bool) {
            self.show_missing_metadata = flag;
        }
    }
}

/// Read a `linked-page-metadata` value: an array of objects with a string
/// `key` and an optional boolean `containsMarkdown`. A malformed entry is
/// ignored; the rest stay. `None` means the value is absent or not an array,
/// so an app default is kept in `other` and a footer value is ignored.
#[must_use]
pub fn metadata_fields(value: Option<&Value>) -> Option<Vec<MetadataField>> {
    let items = value?.as_array()?;
    let mut fields = Vec::with_capacity(items.len());
    for item in items {
        let Some(key) = item.get("key").and_then(Value::as_str) else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        fields.push(MetadataField {
            key: key.to_owned(),
            contains_markdown: item
                .get("containsMarkdown")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }
    Some(fields)
}

/// The `linked-page-metadata` value as written: an array of `{key,
/// containsMarkdown}` objects.
#[must_use]
pub fn metadata_fields_json(fields: &[MetadataField]) -> Value {
    Value::Array(
        fields
            .iter()
            .map(|field| {
                let mut object = Map::new();
                object.insert("key".to_owned(), Value::String(field.key.clone()));
                object.insert(
                    "containsMarkdown".to_owned(),
                    Value::Bool(field.contains_markdown),
                );
                Value::Object(object)
            })
            .collect(),
    )
}

/// One metadata key and its display value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataEntry {
    /// The first spelling of the key in the note, without surrounding space.
    pub key: String,
    /// The value as display text. A frontmatter list is joined with `, `.
    pub value: String,
}

/// Every metadata key in `source`: first the flat frontmatter, then inline
/// fields in the body. The first occurrence of a key, matched
/// case-insensitively, wins; a later duplicate is ignored.
#[must_use]
pub fn note_metadata(source: &str) -> Vec<MetadataEntry> {
    let front = Frontmatter::parse(source);
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for (key, value) in front.iter() {
        let value = match value {
            FrontmatterValue::Text(text) => text.clone(),
            FrontmatterValue::List(items) => items.join(", "),
        };
        push_entry(&mut entries, &mut seen, key, value);
    }
    for entry in inline_fields(source) {
        push_entry(&mut entries, &mut seen, &entry.key, entry.value);
    }
    entries
}

/// The inline fields in `source`, outside fenced code and inline code, in
/// file order. Frontmatter is skipped. The first occurrence of a key wins,
/// matched case-insensitively.
#[must_use]
pub fn inline_fields(source: &str) -> Vec<MetadataEntry> {
    let body_start = Frontmatter::parse(source).body_start();
    let body = source.get(body_start..).unwrap_or_default();
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut fence: Option<(u8, usize)> = None;
    for raw in body.split_inclusive('\n') {
        let line = raw
            .strip_suffix('\n')
            .unwrap_or(raw)
            .strip_suffix('\r')
            .unwrap_or(raw);
        if let Some((marker, run)) = fence {
            if fence_closes(line, marker, run) {
                fence = None;
            }
            continue;
        }
        if let Some(opened) = opens_fence(line) {
            fence = Some(opened);
            continue;
        }
        let code = code_ranges(line);
        scan_line(line, &code, &mut entries, &mut seen);
    }
    entries
}

fn push_entry(
    entries: &mut Vec<MetadataEntry>,
    seen: &mut HashSet<String>,
    key: &str,
    value: String,
) {
    let key = key.trim();
    if key.is_empty() || !valid_key(key) {
        return;
    }
    if seen.insert(key.to_lowercase()) {
        entries.push(MetadataEntry {
            key: key.to_owned(),
            value,
        });
    }
}

/// A key is text before `::` on one line, without brackets or code marks.
fn valid_key(key: &str) -> bool {
    !key.is_empty() && !key.contains(['[', ']', '`', '\n', '\r']) && !key.trim().is_empty()
}

/// Parse the inline fields on one line: bracket fields anywhere, then the
/// line form at its start. `base` is the line's byte offset in the note.
fn scan_line(
    line: &str,
    code: &[Range<usize>],
    entries: &mut Vec<MetadataEntry>,
    seen: &mut HashSet<String>,
) {
    let bytes = line.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'[' && bytes.get(at + 1) == Some(&b'[') && !in_code(code, at) {
            // A wikilink (or embed) is not an inline field; skip the whole
            // target so its brackets are not read as delimiters.
            match line[at + 2..].find("]]") {
                Some(end) => at += 2 + end + 2,
                None => break,
            }
            continue;
        }
        if bytes[at] == b'['
            && !in_code(code, at)
            && let Some((end, key, value)) = bracket_field(line, at, code)
        {
            push_entry(entries, seen, &key, value);
            at = end;
            continue;
        }
        at += line[at..].chars().next().map_or(1, char::len_utf8);
    }

    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if trimmed.is_empty() || trimmed.starts_with(['[', '(', '!']) || in_code(code, indent) {
        return;
    }
    if let Some(delim) = find_outside_code(line, "::", code, indent) {
        let key = line[indent..delim].trim();
        if valid_key(key) {
            push_entry(entries, seen, key, line[delim + 2..].trim().to_owned());
        }
    }
}

/// A `[key:: value]` field opening at `open`, through its closing `]`.
/// `[[…]]` inside the value is skipped, so a wikilink does not close it.
fn bracket_field(
    line: &str,
    open: usize,
    code: &[Range<usize>],
) -> Option<(usize, String, String)> {
    let delim = find_outside_code(line, "::", code, open + 1)?;
    let key = line[open + 1..delim].trim();
    if !valid_key(key) {
        return None;
    }
    let value_start = delim + 2;
    let bytes = line.as_bytes();
    let mut at = value_start;
    while at < bytes.len() {
        if bytes[at] == b'[' && bytes.get(at + 1) == Some(&b'[') {
            let end = line[at + 2..].find("]]")?;
            at += 2 + end + 2;
            continue;
        }
        if bytes[at] == b']' && !in_code(code, at) && (at == 0 || bytes[at - 1] != b'\\') {
            return Some((
                at + 1,
                key.to_owned(),
                line[value_start..at].trim().to_owned(),
            ));
        }
        at += line[at..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// Find `needle` at or after `from`, skipping inline code spans.
fn find_outside_code(
    line: &str,
    needle: &str,
    code: &[Range<usize>],
    from: usize,
) -> Option<usize> {
    let mut at = from;
    while at + needle.len() <= line.len() {
        if line[at..].starts_with(needle) && !in_code(code, at) {
            return Some(at);
        }
        at += line[at..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// One row the card foot draws: a configured key, its value when the linked
/// note has it, and whether the value is markdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataRow {
    pub key: String,
    pub value: Option<String>,
    pub contains_markdown: bool,
}

/// The note a card's first wikilink selects, through the vault's resolver.
/// A heading, block id or alias in the link does not change the note. A
/// self-link (`[[#Heading]]`) selects the note the card is on, when known.
/// A missing target yields `None`; a later link does not stand in for it.
#[must_use]
pub fn first_linked_note(vault: &Vault, current: Option<usize>, text: &str) -> Option<usize> {
    let link = parse_links(text).into_iter().next()?;
    if link.target.is_empty() {
        return current;
    }
    vault.resolve(&link.target)
}

/// The rows a card foot draws for `settings`: one per configured key the
/// linked note has. A missing note or key is skipped unless
/// `show_missing_metadata` asks for `key: —` (a `None` value).
#[must_use]
pub fn card_metadata(
    vault: &Vault,
    current: Option<usize>,
    card_text: &str,
    settings: &MetadataSettings,
) -> Vec<MetadataRow> {
    if settings.linked_page_metadata.is_empty() {
        return Vec::new();
    }
    let linked = first_linked_note(vault, current, card_text)
        .and_then(|id| vault.notes.get(id))
        .map(|note| note_metadata(&note.source));
    let mut rows = Vec::with_capacity(settings.linked_page_metadata.len());
    for field in &settings.linked_page_metadata {
        let value = linked.as_ref().and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.key.to_lowercase() == field.key.to_lowercase())
                .map(|entry| entry.value.clone())
        });
        if value.is_none() && !settings.show_missing_metadata {
            continue;
        }
        rows.push(MetadataRow {
            key: field.key.clone(),
            value,
            contains_markdown: field.contains_markdown,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempVault;

    fn entry<'a>(entries: &'a [MetadataEntry], key: &str) -> Option<&'a str> {
        entries
            .iter()
            .find(|entry| entry.key.to_lowercase() == key.to_lowercase())
            .map(|entry| entry.value.as_str())
    }

    #[test]
    fn flat_frontmatter_reads_scalars_lists_quotes_comments_and_bom() {
        let source = "\u{feff}---\r\n# a comment\r\n\r\nstatus: \"In review\"\r\ntags:\r\n  - work\r\n  - home\r\nflow: [one, \"two, three\"]\r\nnested:\r\n  inner: ignored\r\n---\r\n";
        let entries = note_metadata(source);
        assert_eq!(entry(&entries, "status"), Some("In review"));
        assert_eq!(entry(&entries, "tags"), Some("work, home"));
        assert_eq!(entry(&entries, "flow"), Some("one, two, three"));
        assert_eq!(entry(&entries, "nested"), None);
        assert_eq!(entry(&entries, "inner"), None);
    }

    #[test]
    fn inline_fields_read_both_forms_whitespace_and_markdown() {
        let source = "---\ntitle: Note\n---\n[status:: In review]\n[ owner :: Ada Lovelace ]\nline:: plain value\n  indented:: spaced\n[docs:: **bold** and [[Guide|the guide]]]\n";
        let entries = inline_fields(source);
        assert_eq!(entry(&entries, "status"), Some("In review"));
        assert_eq!(entry(&entries, "owner"), Some("Ada Lovelace"));
        assert_eq!(entry(&entries, "line"), Some("plain value"));
        assert_eq!(entry(&entries, "indented"), Some("spaced"));
        assert_eq!(
            entry(&entries, "docs"),
            Some("**bold** and [[Guide|the guide]]"),
            "the value keeps its markdown"
        );
        assert_eq!(entry(&entries, "title"), None, "frontmatter is skipped");
    }

    #[test]
    fn inline_fields_stay_out_of_code_and_first_duplicate_wins() {
        let source = "---\r\na: front\r\n---\r\n\r\n`[a:: code]` and `line:: code`\r\n```\r\n[fenced:: no]\r\nline:: no\r\n```\r\n[a:: first]\r\n[a:: second]\r\n";
        let entries = inline_fields(source);
        assert_eq!(entry(&entries, "a"), Some("first"));
        assert_eq!(entry(&entries, "line"), None);
        assert_eq!(entry(&entries, "fenced"), None);
        let note = note_metadata(source);
        assert_eq!(entry(&note, "a"), Some("front"), "frontmatter comes first");
    }

    #[test]
    fn a_configured_key_matches_case_insensitively_and_a_list_is_display_text() {
        let source = "---\nStatus: Done\nTags:\n  - one\n  - two\n---\n";
        let entries = note_metadata(source);
        assert_eq!(entry(&entries, "status"), Some("Done"));
        assert_eq!(entry(&entries, "Tags"), Some("one, two"));
    }

    #[test]
    fn the_first_wikilink_selects_the_note_whatever_its_fragment() {
        let tmp = TempVault::new(&[
            ("Agenda.md", "# Agenda\n"),
            ("Other.md", "# Other\n"),
            (
                "Board.md",
                "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] card\n",
            ),
        ]);
        let vault = tmp.load();
        let agenda = vault.resolve("Agenda").expect("Agenda");
        let board = vault.resolve("Board").expect("Board");

        assert_eq!(
            first_linked_note(&vault, Some(board), "see ![[Agenda]] then [[Other]]"),
            Some(agenda),
            "an embed is a wikilink"
        );
        assert_eq!(
            first_linked_note(&vault, Some(board), "[[Agenda#Part|alias]]"),
            Some(agenda)
        );
        assert_eq!(
            first_linked_note(&vault, Some(board), "[[Agenda#^card-1]]"),
            Some(agenda)
        );
        assert_eq!(
            first_linked_note(&vault, Some(board), "`[[Agenda]]` then [[Other]]"),
            vault.resolve("Other"),
            "inline code does not link"
        );
        assert_eq!(
            first_linked_note(&vault, Some(board), "[[Missing]] then [[Agenda]]"),
            None,
            "a later link does not stand in for a missing first one"
        );
        assert_eq!(
            first_linked_note(&vault, Some(board), "[[#Part]]"),
            Some(board),
            "a self-link is the card's own note"
        );
    }

    fn settings(fields: &[(&str, bool)], show_missing: bool) -> MetadataSettings {
        MetadataSettings {
            linked_page_metadata: fields
                .iter()
                .map(|(key, markdown)| MetadataField {
                    key: (*key).to_owned(),
                    contains_markdown: *markdown,
                })
                .collect(),
            show_missing_metadata: show_missing,
        }
    }

    #[test]
    fn rows_follow_the_configured_keys_and_skip_missing_ones_by_default() {
        let tmp = TempVault::new(&[
            ("Agenda.md", "---\nstatus: Active\n---\n[owner:: Ada]\n"),
            (
                "Board.md",
                "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] see [[Agenda]]\n",
            ),
        ]);
        let vault = tmp.load();
        let board = vault.resolve("Board").expect("Board");
        let rows = card_metadata(
            &vault,
            Some(board),
            "see [[Agenda]]",
            &settings(&[("status", false), ("owner", true), ("due", false)], false),
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].key, "status");
        assert_eq!(rows[0].value.as_deref(), Some("Active"));
        assert!(!rows[0].contains_markdown);
        assert_eq!(rows[1].value.as_deref(), Some("Ada"));
        assert!(rows[1].contains_markdown);

        let rows = card_metadata(
            &vault,
            Some(board),
            "see [[Agenda]]",
            &settings(&[("status", false), ("due", false)], true),
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].value, None, "the board asks for a missing row");

        let rows = card_metadata(
            &vault,
            Some(board),
            "see [[Missing]]",
            &settings(&[("status", false)], false),
        );
        assert!(rows.is_empty(), "a missing note draws nothing by default");

        let rows = card_metadata(
            &vault,
            Some(board),
            "see [[Missing]]",
            &settings(&[("status", false), ("owner", true)], true),
        );
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.value.is_none()));
    }

    #[test]
    fn settings_resolve_footer_then_app_defaults_then_builtin() {
        let builtin = MetadataSettings::resolve(None, &KanbanDefaults::default());
        assert!(builtin.linked_page_metadata.is_empty());
        assert!(!builtin.show_missing_metadata);

        let defaults = KanbanDefaults::from_json(Some(&serde_json::json!({
            "linked-page-metadata": [{"key": "status"}],
            "show-missing-metadata": true,
        })));
        let resolved = MetadataSettings::resolve(None, &defaults);
        assert_eq!(resolved.linked_page_metadata[0].key, "status");
        assert!(resolved.show_missing_metadata);

        let footer = r#"{"linked-page-metadata":[{"key":"owner","containsMarkdown":true}],"show-missing-metadata":false}"#;
        let resolved = MetadataSettings::resolve(Some(footer), &defaults);
        assert_eq!(resolved.linked_page_metadata.len(), 1);
        assert_eq!(resolved.linked_page_metadata[0].key, "owner");
        assert!(resolved.linked_page_metadata[0].contains_markdown);
        assert!(!resolved.show_missing_metadata);

        let empty = MetadataSettings::resolve(Some(r#"{"linked-page-metadata":[]}"#), &defaults);
        assert!(empty.linked_page_metadata.is_empty(), "an empty array wins");

        let wrong = MetadataSettings::resolve(Some(r#"{"linked-page-metadata":"no"}"#), &defaults);
        assert_eq!(
            wrong.linked_page_metadata,
            defaults.linked_page_metadata.clone().unwrap(),
            "a wrong footer type is ignored"
        );

        // A wrong app default is unset for display but kept for the next write.
        let app = KanbanDefaults::from_json(Some(&serde_json::json!({
            "linked-page-metadata": "no",
            "show-missing-metadata": 1,
        })));
        assert_eq!(app.linked_page_metadata, None);
        assert_eq!(app.show_missing_metadata, None);
        let written = app.to_json();
        assert_eq!(written["linked-page-metadata"], "no");
        assert_eq!(written["show-missing-metadata"], 1);
    }

    #[test]
    fn metadata_fields_skip_malformed_entries_and_round_trip() {
        let value: Value = serde_json::from_str(
            r#"[{"key":"a"},{"key":2},{"key":"b","containsMarkdown":true},{"key":"  "},{"key":"c","containsMarkdown":"yes"}]"#,
        )
        .expect("json");
        let fields = metadata_fields(Some(&value)).expect("fields");
        assert_eq!(
            fields,
            [
                MetadataField {
                    key: "a".to_owned(),
                    contains_markdown: false
                },
                MetadataField {
                    key: "b".to_owned(),
                    contains_markdown: true
                },
                MetadataField {
                    key: "c".to_owned(),
                    contains_markdown: false
                }
            ]
        );
        assert_eq!(
            metadata_fields_json(&fields),
            serde_json::json!([
                {"key": "a", "containsMarkdown": false},
                {"key": "b", "containsMarkdown": true},
                {"key": "c", "containsMarkdown": false}
            ])
        );
        assert_eq!(metadata_fields(None), None);
        assert_eq!(
            metadata_fields(Some(&serde_json::json!({"key": "a"}))),
            None
        );
    }
}
