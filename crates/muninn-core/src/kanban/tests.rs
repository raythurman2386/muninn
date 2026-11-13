//! Behaviour tests: bytes in, bytes out, and the model a caller can see.
//!
//! Fixtures under `tests/fixtures/kanban/` are hand-written from
//! `docs/kanban-format.md`.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde_json::json;

use super::{
    Archive, ArchiveDatePosition, ArchiveSettings, CardOrder, CivilDate, DateSettings, Kanban,
    KanbanDefaults, Lane, is_kanban, scan_card_text,
};
use crate::Vault;
use crate::testing::TempVault;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kanban")
}

fn read_fixture(name: &str) -> String {
    let path = fixtures().join(name);
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    String::from_utf8(bytes).unwrap_or_else(|error| panic!("{name} is not utf-8: {error}"))
}

fn parse(source: &str) -> Kanban {
    Kanban::parse(source).unwrap_or_else(|error| panic!("parse: {error}"))
}

#[test]
fn every_valid_fixture_round_trips() {
    let mut seen = 0;
    for entry in fs::read_dir(fixtures()).expect("fixture dir") {
        let path = entry.expect("entry").path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "README.md" || !name.ends_with(".md") || name.starts_with("bad-") {
            continue;
        }
        let bytes = fs::read(&path).expect("read");
        let text = String::from_utf8(bytes.clone()).expect("utf-8");
        let kanban = Kanban::parse(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(kanban.source().as_bytes(), bytes.as_slice(), "{name}");
        assert!(is_kanban(&text), "{name}");
        seen += 1;
    }
    assert!(seen >= 16, "expected the hand-written corpus, saw {seen}");
}

#[test]
fn malformed_fixtures_are_invalid_data_and_still_detect() {
    for name in ["bad-unterminated.md", "bad-json.md"] {
        let text = read_fixture(name);
        assert!(is_kanban(&text), "{name}");
        let error = Kanban::parse(&text).expect_err(name);
        assert_eq!(error.kind(), ErrorKind::InvalidData, "{name}: {error}");
        assert!(error.to_string().contains("footer"), "{name}: {error}");
    }
}

#[test]
fn fixture_models_match_the_spec() {
    let fresh = parse(&read_fixture("new-kanban.md"));
    assert_eq!(
        fresh
            .lanes()
            .iter()
            .map(|lane| lane.title())
            .collect::<Vec<_>>(),
        ["To do", "Doing", "Done"]
    );
    assert!(fresh.lanes().iter().all(|lane| lane.cards().is_empty()));
    assert!(fresh.archive().is_none());
    assert!(fresh.footer_json().is_none());

    let example = parse(&read_fixture("spec-example.md"));
    assert_eq!(example.lanes().len(), 3);
    assert_eq!(example.lanes()[0].cards().len(), 2);
    assert_eq!(
        example.lanes()[0].cards()[1].text(),
        "Review [[Roadmap]] with the team\nsecond line of the same card"
    );
    assert_eq!(example.lanes()[1].title(), "Doing");
    assert_eq!(example.lanes()[1].limit(), Some(3));
    assert_eq!(example.lanes()[1].cards()[0].block_id(), Some("card-7f2"));
    assert_eq!(example.lanes()[1].cards()[0].text(), "Fix the sync bug");
    assert_eq!(example.lanes()[2].done_word(), Some("Complete"));
    assert!(example.lanes()[2].cards()[0].is_done());
    assert_eq!(example.lanes()[2].cards()[0].mark(), 'x');
    let archive = example.archive().expect("archive");
    assert_eq!(archive.word(), "Archive");
    assert_eq!(archive.cards().len(), 1);
    assert_eq!(example.footer_json(), Some("{\"kanban-plugin\":\"board\"}"));

    let limits = parse(&read_fixture("lanes-and-limits.md"));
    assert_eq!(limits.lanes()[2].title(), "Review");
    assert_eq!(limits.lanes()[2].written_limit(), Some(0));
    assert_eq!(limits.lanes()[2].limit(), None);

    let nested = parse(&read_fixture("multiline-cards.md"));
    assert_eq!(nested.lanes()[0].cards().len(), 2);
    assert!(nested.lanes()[0].cards()[0].text().contains("- [ ] parser"));
    assert!(nested.lanes()[0].cards()[0].text().contains("- deeper"));

    let plain = parse(&read_fixture("plain-list.md"));
    assert_eq!(plain.lanes()[0].cards().len(), 2);
    assert_eq!(plain.lanes()[0].cards()[0].text(), "Real card");
    assert_eq!(plain.lanes()[0].cards()[1].text(), "Another card");

    let passed = parse(&read_fixture("passthrough.md"));
    assert_eq!(passed.lanes().len(), 2);
    assert_eq!(passed.lanes()[0].cards().len(), 1);

    let quotes = parse(&read_fixture("frontmatter-extra.md"));
    assert_eq!(
        quotes.lanes()[0].cards()[0].text(),
        "Quoted value still detects"
    );

    let german = parse(&read_fixture("archive-de.md"));
    assert_eq!(german.lanes()[0].done_word(), Some("Fertiggestellt"));
    assert_eq!(german.archive().expect("archiv").word(), "Archiv");

    let japanese = parse(&read_fixture("archive-ja.md"));
    assert_eq!(japanese.lanes()[0].done_word(), Some("完了"));
    assert_eq!(japanese.archive().expect("archive").word(), "アーカイブ");

    let footer = parse(&read_fixture("settings-footer.md"));
    assert_eq!(
        footer.footer_json(),
        Some("{\"kanban-plugin\":\"board\",\"alpha\":1,\"beta\":\"two\"}")
    );

    let bom = read_fixture("bom.md");
    assert!(bom.starts_with('\u{feff}'));
    assert!(is_kanban(&bom));
    let crlf = read_fixture("crlf.md");
    assert!(crlf.contains("\r\n"));
    assert!(!crlf.replace("\r\n", "").contains('\n'));
    let open = read_fixture("no-final-newline.md");
    assert!(!open.ends_with('\n'));
    assert_eq!(parse(&open).source(), open);
}

#[test]
fn detection_edges() {
    assert!(is_kanban("---\nkanban-plugin: board\n---\n"));
    assert!(is_kanban("---\nkanban-plugin: 'board'\n---\n"));
    assert!(is_kanban("---\nkanban-plugin: \"board\"\n---\n"));
    assert!(is_kanban("---\r\nkanban-plugin: list\r\n---\r\n"));
    assert!(is_kanban(
        "\u{feff}---\r\nkanban-plugin: 'board'\r\n---\r\n"
    ));
    assert!(is_kanban("---\n\nkanban-plugin:\n  - board\n\n---\n"));
    assert!(!is_kanban("# kanban-plugin: board\n"));
    assert!(!is_kanban("---\n# kanban-plugin: board\n---\n"));
    assert!(!is_kanban("---\nKanban-Plugin: board\n---\n"));
    assert!(!is_kanban("intro\n---\nkanban-plugin: board\n---\n"));
    assert!(!is_kanban("---\nkanban-plugin: board\n"));
    assert!(!is_kanban("---\nparent:\n  kanban-plugin: board\n---\n"));
    assert!(!is_kanban(""));
    let error = Kanban::parse("just a note").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

#[test]
fn every_listed_marker_language_round_trips() {
    let cases = [
        ("**Complete**", "Complete", "Archive"),
        ("**Fertiggestellt**", "Fertiggestellt", "Archiv"),
        ("**Completato**", "Completato", "Archivio"),
        ("**完了**", "完了", "アーカイブ"),
        ("**완료됨**", "완료됨", "보관됨"),
        ("**Concluído**", "Concluído", "Arquivado"),
        ("**Выполнено**", "Выполнено", "Архивировать"),
        ("**完成**", "完成", "归档"),
    ];
    for (marker, word, archive) in cases {
        let source = format!(
            "---\nkanban-plugin: board\n---\n\n## Done\n\n{marker}\n- [x] Item\n\n***\n\n## {archive}\n\n- [x] Old\n"
        );
        let kanban = parse(&source);
        assert_eq!(kanban.source(), source, "{word}");
        assert_eq!(kanban.lanes()[0].done_word(), Some(word));
        assert!(kanban.lanes()[0].cards()[0].is_done());
        assert_eq!(kanban.archive().expect(archive).word(), archive);
    }
}

#[test]
fn open_points_keep_the_text() {
    // A-1: a heading named Archive with no thematic break is a lane.
    let lane = parse("---\nkanban-plugin: board\n---\n\n## Archive\n\n- [ ] Kept\n");
    assert!(lane.archive().is_none());
    assert_eq!(lane.lanes()[0].title(), "Archive");

    // M-1: an unlisted marker stays in the file and is not a done lane.
    let unknown = "---\nkanban-plugin: board\n---\n\n## Done\n\n**Not a marker**\n- [x] Item\n";
    let kanban = parse(unknown);
    assert_eq!(kanban.lanes()[0].done_word(), None);
    assert_eq!(kanban.source(), unknown);

    // C-1: a done mark other than x is kept until toggled.
    let odd = parse("---\nkanban-plugin: board\n---\n\n## To do\n\n- [~] Odd\n");
    assert_eq!(odd.lanes()[0].cards()[0].mark(), '~');
    assert!(odd.lanes()[0].cards()[0].is_done());

    // C-2: a list item with no checkbox is not a card.
    let bullet = parse("---\nkanban-plugin: board\n---\n\n## To do\n\n- plain\n");
    assert!(bullet.lanes()[0].cards().is_empty());
    assert_eq!(
        bullet.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- plain\n"
    );

    // L-1: a written 0 is no limit.
    let zero = parse("---\nkanban-plugin: board\n---\n\n## Doing (0)\n\n- [ ] A\n");
    assert_eq!(zero.lanes()[0].written_limit(), Some(0));
    assert_eq!(zero.lanes()[0].limit(), None);

    // Dates, tags and inline fields stay card text; slice 9 reads dates and
    // times without changing the bytes. T-1 stays open.
    let text = parse(
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Due @{2026-11-11} @@{14:30} #release [owner:: muninn]\n",
    );
    assert_eq!(
        text.lanes()[0].cards()[0].text(),
        "Due @{2026-11-11} @@{14:30} #release [owner:: muninn]"
    );
    let found = scan_card_text(text.lanes()[0].cards()[0].text(), &DateSettings::default());
    assert_eq!(
        found.date.as_ref().map(|token| token.value.as_str()),
        Some("2026-11-11")
    );
    assert_eq!(
        found.time.as_ref().map(|token| token.value.as_str()),
        Some("14:30")
    );
    assert_eq!(found.tags[0].tag, "release");
}

#[test]
fn edits_change_only_their_lines() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] Alpha
- [ ] Beta
  nested

## Doing (3)

- [X] Gamma

## Done

**Complete**
- [ ] Delta
";
    let base = parse(source);

    let mut added = base.clone();
    added.add_card(0, 1, "Between").unwrap();
    assert_eq!(
        added.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] Alpha
- [ ] Between
- [ ] Beta
  nested

## Doing (3)

- [X] Gamma

## Done

**Complete**
- [ ] Delta
"
    );

    let mut edited = base.clone();
    edited.edit_card(0, 1, "Beta\nnested still").unwrap();
    assert_eq!(
        edited.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] Alpha
- [ ] Beta
  nested still

## Doing (3)

- [X] Gamma

## Done

**Complete**
- [ ] Delta
"
    );

    let mut deleted = base.clone();
    deleted.delete_card(0, 0).unwrap();
    assert_eq!(
        deleted.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] Beta
  nested

## Doing (3)

- [X] Gamma

## Done

**Complete**
- [ ] Delta
"
    );

    let mut moved = base.clone();
    moved.move_card(0, 0, 0, 1).unwrap();
    assert_eq!(
        moved.lanes()[0]
            .cards()
            .iter()
            .map(|card| card.text())
            .collect::<Vec<_>>(),
        ["Beta\nnested", "Alpha"]
    );
    assert_eq!(
        moved.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] Beta
  nested
- [ ] Alpha

## Doing (3)

- [X] Gamma

## Done

**Complete**
- [ ] Delta
"
    );

    let mut across = base.clone();
    across.move_card(0, 0, 1, 0).unwrap();
    assert_eq!(
        across.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] Beta
  nested

## Doing (3)

- [ ] Alpha
- [X] Gamma

## Done

**Complete**
- [ ] Delta
"
    );

    let mut renamed = base.clone();
    renamed.rename_lane(1, "Busy").unwrap();
    assert_eq!(renamed.lanes()[1].title(), "Busy");
    assert_eq!(renamed.lanes()[1].limit(), Some(3));
    assert_eq!(
        renamed.source(),
        source.replacen("## Doing (3)", "## Busy (3)", 1)
    );

    let mut limited = base.clone();
    limited.set_lane_limit(0, Some(4)).unwrap();
    assert_eq!(
        limited.source(),
        source.replacen("## To do\n", "## To do (4)\n", 1)
    );
    limited.set_lane_limit(1, Some(9)).unwrap();
    assert_eq!(
        limited.source(),
        source.replacen("## To do\n", "## To do (4)\n", 1).replacen(
            "## Doing (3)",
            "## Doing (9)",
            1
        )
    );
    limited.set_lane_limit(1, None).unwrap();
    assert_eq!(
        limited.source(),
        source.replacen("## To do\n", "## To do (4)\n", 1).replacen(
            "## Doing (3)\n",
            "## Doing\n",
            1
        )
    );
    assert_eq!(limited.lanes()[1].limit(), None);
    limited.set_lane_limit(1, Some(0)).unwrap();
    assert_eq!(
        limited.source(),
        source.replacen("## To do\n", "## To do (4)\n", 1).replacen(
            "## Doing (3)\n",
            "## Doing\n",
            1
        )
    );

    let mut toggled = base.clone();
    toggled.toggle_done(1, 0).unwrap();
    assert_eq!(
        toggled.source(),
        source.replacen("- [X] Gamma", "- [ ] Gamma", 1)
    );
    toggled.toggle_done(1, 0).unwrap();
    assert_eq!(
        toggled.source(),
        source.replacen("- [X] Gamma", "- [x] Gamma", 1)
    );

    let mut created = base.clone();
    created.add_lane(1, "Review", Some(2), false).unwrap();
    assert_eq!(created.lanes()[1].title(), "Review");
    assert_eq!(created.lanes()[1].limit(), Some(2));
    assert_eq!(
        created.source(),
        source.replacen("## Doing (3)\n", "## Review (2)\n\n\n\n## Doing (3)\n", 1)
    );
    let mut later = base.clone();
    later.add_lane(0, "Later", None, true).unwrap();
    assert_eq!(later.lanes()[0].done_word(), Some("Complete"));
    assert_eq!(
        later.source(),
        source.replacen("## To do\n", "## Later\n\n**Complete**\n\n\n## To do\n", 1)
    );

    let mut removed = base.clone();
    removed.delete_lane(2).unwrap();
    assert_eq!(
        removed.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] Alpha
- [ ] Beta
  nested

## Doing (3)

- [X] Gamma

"
    );
}

#[test]
fn footer_edits_touch_one_key() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] Alpha

%% kanban:settings
```
{\"kanban-plugin\":\"board\",\"alpha\":1,\"beta\":\"two\"}
```
%%
";
    let mut set = parse(source);
    set.set_setting("alpha", &json!(2)).unwrap();
    assert_eq!(
        set.footer_json(),
        Some("{\"kanban-plugin\":\"board\",\"alpha\":2,\"beta\":\"two\"}")
    );
    set.set_setting("delta", &json!(true)).unwrap();
    assert_eq!(
        set.footer_json(),
        Some("{\"kanban-plugin\":\"board\",\"alpha\":2,\"beta\":\"two\",\"delta\":true}")
    );
    set.remove_setting("beta").unwrap();
    assert_eq!(
        set.footer_json(),
        Some("{\"kanban-plugin\":\"board\",\"alpha\":2,\"delta\":true}")
    );

    let spaced = "\
---
kanban-plugin: board
---

## To do

- [ ] Alpha

%% kanban:settings
```
{ \"kanban-plugin\" : \"board\" , \"alpha\" : 1 }
```
%%
";
    let mut spaced_board = parse(spaced);
    spaced_board.set_setting("alpha", &json!(2)).unwrap();
    assert_eq!(
        spaced_board.footer_json(),
        Some("{ \"kanban-plugin\" : \"board\" , \"alpha\" : 2 }")
    );
    spaced_board.remove_setting("kanban-plugin").unwrap();
    assert_eq!(spaced_board.footer_json(), Some("{  \"alpha\" : 2 }"));

    let mut fresh = parse("---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Alpha\n");
    fresh.set_setting("alpha", &json!(1)).unwrap();
    assert!(
        fresh
            .source()
            .ends_with("\n\n%% kanban:settings\n```\n{\"alpha\":1}\n```\n%%\n")
    );
    assert!(fresh.footer_json().unwrap().contains("\"alpha\":1"));

    let missing = parse(source);
    let error = missing.clone().remove_setting("nope").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert_eq!(missing.source(), source);
}

#[test]
fn a_broken_footer_is_never_an_empty_board() {
    let array = "\
---
kanban-plugin: board
---
%% kanban:settings
```
[]
```
%%
";
    let error = Kanban::parse(array).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

#[test]
fn save_uses_the_atomic_writer() {
    let kanban = parse("---\nkanban-plugin: board\n---\n\n## To do\n");
    let dir = TempVault::new(&[]);
    let path = dir.path().join("Board.md");
    kanban.save(&path).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), kanban.source());
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("Board.md")]);
}

#[test]
fn card_wikilinks_stay_in_the_note_index() {
    let body = "\
---
kanban-plugin: board
---

## To do

- [ ] See [[Roadmap]] and ![[Team]]
";
    let dir = TempVault::new(&[
        ("Board.md", body),
        ("Roadmap.md", "# Roadmap\n"),
        ("Team.md", "# Team\n"),
        ("Plain.md", "# Plain\n\nNo key here.\n"),
    ]);
    let vault = Vault::load(dir.path()).unwrap();
    let board = vault
        .notes
        .iter()
        .find(|note| note.title == "Board")
        .unwrap();
    let roadmap = vault.resolve("Roadmap").unwrap();
    let team = vault.resolve("Team").unwrap();
    assert!(board.links.contains(&roadmap), "{:?}", board.links);
    assert!(board.links.contains(&team), "{:?}", board.links);
    assert!(vault.notes[roadmap].backlinks.contains(&board.id));
    assert!(vault.notes[team].backlinks.contains(&board.id));
    let kanbans = vault.kanbans();
    assert_eq!(kanbans.len(), 1);
    assert_eq!(kanbans[0].title, "Board");
    assert_eq!(kanbans[0].rel, "Board.md");
    assert!(is_kanban(&board.source));
    assert!(!is_kanban(
        &vault
            .notes
            .iter()
            .find(|note| note.title == "Plain")
            .unwrap()
            .source
    ));
}

#[test]
fn toggle_of_a_multibyte_mark_changes_that_character() {
    let mut kanban = parse("---\nkanban-plugin: board\n---\n\n## To do\n\n- [✓] Done\n");
    assert!(kanban.lanes()[0].cards()[0].is_done());
    kanban.toggle_done(0, 0).unwrap();
    assert_eq!(kanban.lanes()[0].cards()[0].mark(), ' ');
    assert_eq!(
        kanban.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Done\n"
    );
}

#[test]
fn heading_after_the_archive_is_not_dropped() {
    let source = "\
---
kanban-plugin: board
---

***

## Archive

- [x] Old

## To do

- [ ] After
";
    let kanban = parse(source);
    assert_eq!(kanban.source(), source);
    assert!(kanban.lanes().is_empty(), "a later heading is not a lane");
    let archive = kanban.archive().expect("archive");
    assert_eq!(archive.cards().len(), 2);
    assert_eq!(archive.cards()[1].text(), "After");
}

#[test]
fn a_column_zero_fence_is_not_board_structure() {
    let source = read_fixture("fence-opaque.md");
    let kanban = parse(&source);
    assert_eq!(kanban.source(), source);
    assert_eq!(kanban.lanes().len(), 1);
    assert_eq!(kanban.lanes()[0].cards().len(), 1);
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "Real card");
    assert!(kanban.footer_json().is_none());
    let fence = source
        .split_once("~~~\n")
        .expect("tilde fence")
        .1
        .split_once("\n~~~\n")
        .expect("close")
        .0;
    let mut toggled = kanban.clone();
    toggled.toggle_done(0, 0).unwrap();
    assert_eq!(
        toggled.source(),
        source.replacen("- [ ] Real card", "- [x] Real card", 1)
    );
    assert!(toggled.source().contains(fence));
    let mut deleted = kanban.clone();
    deleted.delete_card(0, 0).unwrap();
    assert_eq!(
        deleted.source(),
        source.replacen("- [ ] Real card\n", "", 1)
    );
    assert!(deleted.source().contains("## Not a lane"));
    assert!(deleted.source().contains("%% kanban:settings"));
}

#[test]
fn a_blank_line_then_an_indented_paragraph_stays_with_the_card() {
    let source = read_fixture("card-paragraph.md");
    let kanban = parse(&source);
    assert_eq!(kanban.source(), source);
    assert_eq!(kanban.lanes()[0].cards().len(), 2);
    assert_eq!(
        kanban.lanes()[0].cards()[0].text(),
        "Alpha\n\nsecond paragraph of Alpha"
    );
    let mut moved = kanban.clone();
    moved.move_card(0, 0, 0, 1).unwrap();
    assert_eq!(
        moved.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] Beta
- [ ] Alpha

  second paragraph of Alpha
"
    );
    let mut edited = kanban;
    edited.edit_card(0, 0, "Alpha\n\nafter blank").unwrap();
    assert_eq!(edited.lanes()[0].cards()[0].text(), "Alpha\n\nafter blank");
    assert_eq!(
        edited.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] Alpha

  after blank
- [ ] Beta
"
    );
}

#[test]
fn editing_a_card_does_not_keep_a_stray_carriage_return() {
    let mut kanban = parse("---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] A ^a1\n");
    kanban.edit_card(0, 0, "a\r\nb").unwrap();
    assert_eq!(
        kanban.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a ^a1\n  b\n"
    );
    assert!(!kanban.source().contains('\r'));
}

#[test]
fn add_lane_on_a_file_with_no_final_newline_keeps_that_and_a_gap() {
    let mut kanban = parse("---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] A");
    kanban.add_lane(1, "B", None, false).unwrap();
    assert_eq!(
        kanban.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] A\n\n\n## B"
    );
    assert!(!kanban.source().ends_with('\n'));
}

#[test]
fn footer_edits_keep_nested_objects() {
    let source = read_fixture("footer-nested.md");
    let mut kanban = parse(&source);
    assert_eq!(kanban.source(), source);
    let original = kanban.footer_json().expect("footer").to_owned();
    assert!(original.contains("\"tag-colors\":[{\"tagKey\":\"#x\""));
    assert!(original.contains("\"o\":{\"k\":1}"));
    kanban.set_setting("lane-width", &json!(300)).unwrap();
    assert_eq!(
        kanban.footer_json(),
        Some(
            "{\"kanban-plugin\":\"board\",\"lane-width\":300,\"tag-colors\":[{\"tagKey\":\"#x\",\"color\":\"red\"}],\"o\":{\"k\":1}}"
        )
    );
    kanban.remove_setting("lane-width").unwrap();
    assert_eq!(
        kanban.footer_json(),
        Some(
            "{\"kanban-plugin\":\"board\",\"tag-colors\":[{\"tagKey\":\"#x\",\"color\":\"red\"}],\"o\":{\"k\":1}}"
        )
    );
}

#[test]
fn a_longer_closing_fence_ends_the_fence() {
    let source = read_fixture("fence-longer-close.md");
    let kanban = parse(&source);
    assert_eq!(kanban.source(), source);
    assert_eq!(kanban.lanes().len(), 2);
    assert_eq!(kanban.lanes()[1].title(), "B");
    assert_eq!(kanban.lanes()[1].cards()[0].text(), "b");
    // Deleting lane A keeps its fenced note text (spec section 10.3).
    let mut deleted = kanban;
    assert!(deleted.lane_delete_drops(0).unwrap().is_empty());
    deleted.delete_lane(0).unwrap();
    assert_eq!(
        deleted.source(),
        "\
---
kanban-plugin: board
---

```
hidden
`````

## B

- [ ] b
"
    );
}

#[test]
fn an_unclosed_indented_fence_does_not_swallow_the_next_lane() {
    let source = read_fixture("card-open-fence.md");
    let kanban = parse(&source);
    assert_eq!(kanban.source(), source);
    assert_eq!(kanban.lanes().len(), 2);
    assert_eq!(kanban.lanes()[1].title(), "B");
    assert_eq!(kanban.lanes()[1].cards()[0].text(), "b");
    let mut deleted = kanban;
    deleted.delete_card(0, 0).unwrap();
    assert_eq!(
        deleted.source(),
        "\
---
kanban-plugin: board
---

## A


## B

- [ ] b
"
    );
}

#[test]
fn moving_an_open_fence_card_to_the_end_of_the_lane_keeps_its_text() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] a
  ```
  code
- [ ] b

## Done

- [ ] d
";
    let mut kanban = parse(source);
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "a\n```\ncode");
    kanban.move_card(0, 0, 0, 1).unwrap();
    assert_eq!(kanban.lanes()[0].cards()[1].text(), "a\n```\ncode");
    assert_eq!(
        kanban.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] b
- [ ] a
  ```
  code

## Done

- [ ] d
"
    );
}

#[test]
fn a_column_zero_fence_line_is_not_part_of_the_card() {
    let source = read_fixture("fence-column0-card.md");
    let kanban = parse(&source);
    assert_eq!(kanban.source(), source);
    assert_eq!(kanban.lanes().len(), 1);
    assert_eq!(kanban.lanes()[0].cards().len(), 1);
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "a\n```\ncode");
    assert!(kanban.source().contains("## Still inside\n"));
    assert!(!kanban.lanes()[0].cards()[0].text().contains("Still"));
    let mut deleted = kanban;
    deleted.delete_card(0, 0).unwrap();
    assert_eq!(
        deleted.source(),
        "\
---
kanban-plugin: board
---

## A

```
## Still inside

- [ ] not a card
"
    );
}

#[test]
fn a_column_zero_task_while_an_indented_fence_is_open_starts_a_new_card() {
    let source = read_fixture("task-open-fence.md");
    let kanban = parse(&source);
    assert_eq!(kanban.source(), source);
    assert_eq!(kanban.lanes().len(), 2);
    assert_eq!(kanban.lanes()[0].cards().len(), 2);
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "a\n```\ncode");
    assert_eq!(kanban.lanes()[0].cards()[1].text(), "b");
    assert_eq!(kanban.lanes()[1].title(), "B");
    assert_eq!(kanban.lanes()[1].cards().len(), 1);
    assert_eq!(kanban.lanes()[1].cards()[0].text(), "c");
}

#[test]
fn a_column_zero_card_after_a_closed_fence_is_its_own_card() {
    let source = read_fixture("card-closed-fence.md");
    let kanban = parse(&source);
    assert_eq!(kanban.source(), source);
    assert_eq!(kanban.lanes()[0].cards().len(), 2);
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "a\n```\ncode\n```");
    assert_eq!(kanban.lanes()[0].cards()[1].text(), "b");
    let mut deleted = kanban;
    deleted.delete_card(0, 0).unwrap();
    assert_eq!(deleted.lanes()[0].cards().len(), 1);
    assert_eq!(deleted.lanes()[0].cards()[0].text(), "b");
    assert!(deleted.source().contains("- [ ] b\n"));
    assert!(!deleted.source().contains("```"));
}

#[test]
fn set_setting_does_not_grow_a_file_that_ends_in_a_fence() {
    let source = read_fixture("fence-unclosed.md");
    let mut kanban = parse(&source);
    assert_eq!(kanban.source(), source);
    assert!(kanban.footer_json().is_none());
    let error = kanban.set_setting("alpha", &json!(1)).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(kanban.source(), source);
    let again = kanban.set_setting("beta", &json!(2)).unwrap_err();
    assert_eq!(again.kind(), ErrorKind::InvalidData);
    assert_eq!(kanban.source(), source);
}

#[test]
fn an_unchanged_edit_keeps_mixed_continuation_indents() {
    let source = read_fixture("indent-mixed.md");
    let mut kanban = parse(&source);
    assert_eq!(kanban.source(), source);
    let text = kanban.lanes()[0].cards()[0].text().to_owned();
    assert_eq!(text, "a\n\t```\n\t## X\n\t```\ntwo-space line");
    // An unchanged edit returns before rendering, so render the card the way
    // an edit does and check the exact lines the writer makes.
    assert_eq!(
        kanban.render_card_for_test(0, 0).expect("render"),
        "- [ ] a\n\t```\n\t## X\n\t```\n  two-space line\n"
    );
    kanban.edit_card(0, 0, &text).unwrap();
    assert_eq!(kanban.source(), source);
}

#[test]
fn an_added_line_on_a_mixed_indent_card_moves_with_the_card() {
    let source = read_fixture("indent-mixed.md");
    let mut kanban = parse(&source);
    let text = kanban.lanes()[0].cards()[0].text().to_owned();
    kanban.edit_card(0, 0, &(text + "\nadded")).unwrap();
    assert_eq!(
        kanban.lanes()[0].cards()[0].text(),
        "a\n\t```\n\t## X\n\t```\ntwo-space line\nadded"
    );
    kanban.add_lane(1, "Done", None, true).unwrap();
    kanban.move_card(0, 0, 1, 0).unwrap();
    assert_eq!(
        kanban.source(),
        "\
---
kanban-plugin: board
---

## To do

## Done

**Complete**
- [ ] a
	```
	## X
	```
  two-space line
  added

"
    );
}

/// The text `edit_card` stores: carriage returns dropped, a line of spaces or
/// tabs made blank, one trailing newline trimmed.
fn normalised_edit(text: &str) -> String {
    let text = text.replace('\r', "");
    let mut text = text
        .split('\n')
        .map(|line| {
            if line.chars().all(|ch| ch == ' ' || ch == '\t') {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.ends_with('\n') {
        text.pop();
    }
    text
}

#[test]
fn an_edit_round_trips_every_kind_of_new_line() {
    let two_space = "\
---
kanban-plugin: board
---

## To do

- [ ] a
  l1
- [ ] b
";
    let mixed = read_fixture("indent-mixed.md");
    // The third column is the line as it must appear in the file.
    let rows = [
        ("tab", "\n\tx", "\tx"),
        ("one-space", "\n x", "   x"),
        ("ideographic", "\n\u{3000}全角", "  \u{3000}全角"),
        ("nbsp", "\n\u{a0}x", "  \u{a0}x"),
        ("four-spaces", "\n    deep", "      deep"),
        ("fence", "\n```", "  ```"),
        ("task", "\n- [ ] sub", "  - [ ] sub"),
        ("heading", "\n## Next", "  ## Next"),
        ("crlf", "\r\ncrlf line\r\n", "  crlf line"),
    ];
    for (name, addition, file_line) in rows {
        for (label, source, after) in [
            ("two-space", two_space, "  l1\n"),
            ("mixed", mixed.as_str(), "  two-space line\n"),
        ] {
            let mut kanban = parse(source);
            let current = kanban.lanes()[0].cards()[0].text().to_owned();
            let requested = format!("{current}{addition}");
            kanban
                .edit_card(0, 0, &requested)
                .unwrap_or_else(|error| panic!("{label}/{name}: {error}"));
            let stored = normalised_edit(&requested);
            assert_eq!(
                kanban.lanes()[0].cards()[0].text(),
                stored,
                "{label}/{name} text"
            );
            let again = parse(kanban.source());
            assert_eq!(
                again.lanes()[0].cards()[0].text(),
                stored,
                "{label}/{name} re-parse"
            );
            let at = source.find(after).expect(label) + after.len();
            let mut expected = String::new();
            expected.push_str(&source[..at]);
            expected.push_str(file_line);
            expected.push('\n');
            expected.push_str(&source[at..]);
            assert_eq!(kanban.source(), expected, "{label}/{name} file");
        }
    }
}

#[test]
fn an_edit_that_would_change_the_card_text_is_rejected() {
    // A one-space indent cannot store a line with a wider all-space lead.
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n l1\n";
    let mut kanban = parse(source);
    let error = kanban.edit_card(0, 0, "a\n  l1").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(kanban.source(), source);
}

#[test]
fn an_ideographic_space_stays_inside_the_card() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] a
  l1
- [ ] b
";
    let mut kanban = parse(source);
    kanban.edit_card(0, 0, "a\nl1\n\n\u{3000}全角").unwrap();
    assert_eq!(
        kanban.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] a
  l1

  \u{3000}全角
- [ ] b
"
    );
    assert_eq!(kanban.lanes()[0].cards().len(), 2);
    assert_eq!(kanban.lanes()[0].cards()[1].text(), "b");
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "a\nl1\n\n\u{3000}全角");
}

#[test]
fn an_ideographic_space_inside_a_fence_does_not_swallow_the_next_card() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] a
  ```
  code
  ```
- [ ] b
";
    let mut kanban = parse(source);
    kanban
        .edit_card(0, 0, "a\n```\ncode\n\u{3000}x\n```")
        .unwrap();
    assert_eq!(
        kanban.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] a
  ```
  code
  \u{3000}x
  ```
- [ ] b
"
    );
    assert_eq!(kanban.lanes()[0].cards().len(), 2);
    assert_eq!(
        kanban.lanes()[0].cards()[0].text(),
        "a\n```\ncode\n\u{3000}x\n```"
    );
    assert_eq!(kanban.lanes()[0].cards()[1].text(), "b");
}

#[test]
fn a_four_space_line_on_a_tab_card_stays_in_the_card() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] a
	l0
- [ ] b
";
    let mut kanban = parse(source);
    kanban.edit_card(0, 0, "a\nl0\n    code").unwrap();
    assert_eq!(
        kanban.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] a
	l0
    code
- [ ] b
"
    );
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "a\nl0\n    code");
    assert_eq!(kanban.lanes()[0].cards()[1].text(), "b");
}

#[test]
fn a_tab_line_on_a_three_space_card_stays_in_the_card() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] a
   l0
- [ ] b
";
    let mut kanban = parse(source);
    kanban.edit_card(0, 0, "a\n l0\n \tx").unwrap();
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "a\n l0\n \tx");
    assert_eq!(kanban.lanes()[0].cards().len(), 2);
    assert_eq!(kanban.lanes()[0].cards()[1].text(), "b");
    assert_eq!(
        kanban.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] a
   l0
 \tx
- [ ] b
"
    );
}

#[test]
fn adding_a_card_beside_an_ideographic_line_keeps_the_new_line_indented() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] a
  l1
\u{3000}全角
- [ ] b
";
    let mut kanban = parse(source);
    kanban.add_card(0, 2, "new\nsecond").unwrap();
    assert_eq!(
        kanban.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] a
  l1
\u{3000}全角
- [ ] b
- [ ] new
  second
"
    );
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "a\nl1");
    assert_eq!(kanban.lanes()[0].cards()[2].text(), "new\nsecond");
    kanban.edit_card(0, 1, "b\nmore").unwrap();
    assert_eq!(kanban.lanes()[0].cards()[1].text(), "b\nmore");
    assert!(kanban.source().contains("- [ ] b\n  more\n"));
}

#[test]
fn a_one_space_card_indents_a_new_line_by_one_space() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] a
 l0
";
    let mut kanban = parse(source);
    kanban.edit_card(0, 0, "a\nl0\n\nx").unwrap();
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "a\nl0\n\nx");
    assert_eq!(
        kanban.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] a
 l0

 x
"
    );
}

#[test]
fn a_four_space_line_above_a_tab_line_is_rejected() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] a
	l0
";
    let mut kanban = parse(source);
    let error = kanban.edit_card(0, 0, "a\n    code\nl0").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(kanban.source(), source);
}

/// An empty lane can hold indented passthrough text. A card put above that
/// text would take it as a continuation, so the card goes after it.
#[test]
fn a_card_moved_into_an_empty_lane_goes_after_its_indented_note() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] a
  l1

## Done

  indented note
";
    let mut kanban = parse(source);
    kanban.move_card(0, 0, 1, 0).expect("move");
    assert_eq!(
        kanban.source(),
        "\
---
kanban-plugin: board
---

## To do


## Done

  indented note
- [ ] a
  l1
"
    );
    assert_eq!(kanban.lanes()[1].cards()[0].text(), "a\nl1");
}

#[test]
fn a_card_added_to_an_empty_lane_goes_after_its_leading_indented_text() {
    let source = "---\nkanban-plugin: board\n---\n\n## A\n\n  an indented note\n\n## B\n";
    let mut board = parse(source);
    board.add_card(0, 0, "new").expect("add");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## A\n\n  an indented note\n- [ ] new\n\n## B\n"
    );
    assert_eq!(board.lanes()[0].cards()[0].text(), "new");

    // A blank line inside the note does not split it from the card.
    let source = "---\nkanban-plugin: board\n---\n\n## A\n\n  one\n\n  two\n\n## B\n";
    let mut board = parse(source);
    board.add_card(0, 0, "new").expect("add");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## A\n\n  one\n\n  two\n- [ ] new\n\n## B\n"
    );
    assert_eq!(board.lanes()[0].cards()[0].text(), "new");

    // A done-lane marker keeps its place above the note.
    let source = "---\nkanban-plugin: board\n---\n\n## Done\n**Complete**\n  note\n";
    let mut board = parse(source);
    board.add_card(0, 0, "new").expect("add");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## Done\n**Complete**\n  note\n- [ ] new\n"
    );
    assert_eq!(board.lanes()[0].cards()[0].text(), "new");
}

#[test]
fn a_unicode_whitespace_line_survives_an_edit() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] a
  l1
\u{3000}
\u{a0}
  l2
";
    let mut kanban = parse(source);
    let text = kanban.lanes()[0].cards()[0].text().to_owned();
    kanban.edit_card(0, 0, &(text + "\nx")).unwrap();
    assert_eq!(
        kanban.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] a
  l1
  x
\u{3000}
\u{a0}
  l2
"
    );
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "a\nl1\nx");
}

#[test]
fn adding_a_card_on_a_one_space_board_uses_two_spaces() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] a
 l0
";
    let mut kanban = parse(source);
    kanban.add_card(0, 1, "new\n\nx").unwrap();
    assert_eq!(
        kanban.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] a
 l0
- [ ] new

  x
"
    );
    assert_eq!(kanban.lanes()[0].cards()[1].text(), "new\n\nx");
}

#[test]
fn one_indented_continuation_keeps_its_indent() {
    let source = read_fixture("indent-deep-line.md");
    let mut kanban = parse(&source);
    assert_eq!(kanban.source(), source);
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "x\n\n\n  y");
    // An unchanged edit returns before rendering, so render the card the way
    // an edit does and check the exact lines the writer makes.
    assert_eq!(
        kanban.render_card_for_test(0, 0).expect("render"),
        "- [ ] x\n\n\n    y\n"
    );
    let text = kanban.lanes()[0].cards()[0].text().to_owned();
    kanban.edit_card(0, 0, &text).unwrap();
    assert_eq!(kanban.source(), source);
    let mut fresh = parse("---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] keep\n");
    fresh.edit_card(0, 0, "x\n\n\n  y").unwrap();
    assert_eq!(fresh.lanes()[0].cards()[0].text(), "x\n\n\n  y");
    assert_eq!(
        fresh.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] x\n\n\n    y\n"
    );
}

#[test]
fn a_deeper_first_continuation_does_not_change_later_lines() {
    let mut kanban = parse("---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] keep\n");
    kanban.edit_card(0, 0, "x\n  deeper\n\n- [ ] sub").unwrap();
    assert_eq!(
        kanban.lanes()[0].cards()[0].text(),
        "x\n  deeper\n\n- [ ] sub"
    );
    let again = parse(kanban.source());
    assert_eq!(
        again.lanes()[0].cards()[0].text(),
        "x\n  deeper\n\n- [ ] sub"
    );
}

#[test]
fn a_final_carriage_return_is_the_same_line_ending_for_detection_and_parse() {
    let source = "---\r\nkanban-plugin: board\r\n---\r";
    assert!(is_kanban(source));
    assert_eq!(parse(source).source(), source);
}

/// Slice 3: New Kanban names, bytes, never-overwrite and folder rules.
#[test]
fn new_kanban_writes_the_spec_layout_under_unique_names() {
    let dir = TempVault::new(&[("Untitled Kanban 2.md", "keep\n")]);
    let vault = Vault::load(dir.path()).unwrap();
    let first = vault.create_kanban_in("").unwrap();
    let next = vault.create_kanban_in("").unwrap();
    assert_eq!(first, dir.path().join("Untitled Kanban.md"));
    assert_eq!(next, dir.path().join("Untitled Kanban 3.md"), "2 is taken");
    assert_eq!(
        fs::read_to_string(dir.path().join("Untitled Kanban 2.md")).unwrap(),
        "keep\n",
        "an existing file is never overwritten"
    );
    let layout = read_fixture("new-kanban.md");
    for path in [&first, &next] {
        assert_eq!(fs::read_to_string(path).unwrap(), layout, "{path:?}");
    }
    assert!(is_kanban(&layout) && parse(&layout).footer_json().is_none());
    let hidden: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
        .collect();
    assert!(hidden.is_empty(), "no temp file left: {hidden:?}");
}

#[test]
fn new_kanban_folders_drop_dot_parts_and_stay_in_the_vault() {
    let dir = TempVault::new(&[]);
    let vault = Vault::load(dir.path()).unwrap();
    let path = vault.create_kanban_in("../../Plans/./.hidden/Q4").unwrap();
    assert_eq!(path, dir.path().join("Plans/Q4/Untitled Kanban.md"));
}

#[cfg(unix)]
#[test]
fn new_kanban_is_refused_in_a_folder_that_links_out_of_the_vault() {
    let dir = TempVault::new(&[]);
    let outside = TempVault::new(&[]);
    std::os::unix::fs::symlink(outside.path(), dir.path().join("Link")).unwrap();
    let vault = Vault::load(dir.path()).unwrap();
    for folder in ["Link", "Link/Deeper"] {
        let error = vault.create_kanban_in(folder).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::PermissionDenied, "{folder}");
    }
    assert!(
        fs::read_dir(outside.path()).unwrap().next().is_none(),
        "nothing written or made outside the vault"
    );
}

#[test]
fn the_index_follows_a_new_kanban_and_a_convert() {
    let dir = TempVault::new(&[("Blank.md", " \n\t\n"), ("Plain.md", "# Plain\n")]);
    let vault = Vault::load(dir.path()).unwrap();
    assert!(vault.kanbans().is_empty());
    vault.create_kanban_in("").unwrap();
    let mut vault = Vault::load(dir.path()).unwrap();
    let titles = |v: &Vault| v.kanbans().into_iter().map(|k| k.title).collect::<Vec<_>>();
    assert_eq!(titles(&vault), ["Untitled Kanban"]);
    let blank = vault.resolve("Blank").unwrap();
    assert!(!vault.notes[blank].is_kanban());
    vault.convert_to_kanban(blank).unwrap();
    assert!(vault.notes[blank].is_kanban(), "the index caches the write");
    assert_eq!(titles(&vault), ["Blank", "Untitled Kanban"]);
    assert_eq!(
        fs::read_to_string(dir.path().join("Blank.md")).unwrap(),
        read_fixture("new-kanban.md")
    );
    // Writing the frontmatter away makes it a plain note again.
    vault.write_note(blank, "# Blank\n").unwrap();
    assert_eq!(titles(&vault), ["Untitled Kanban"]);
}

#[test]
fn convert_refuses_a_note_with_text_and_writes_nothing() {
    let dir = TempVault::new(&[("Plain.md", "# Plain\n"), ("Late.md", "")]);
    let mut vault = Vault::load(dir.path()).unwrap();
    let plain = vault.resolve("Plain").unwrap();
    let error = vault.convert_to_kanban(plain).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        fs::read_to_string(dir.path().join("Plain.md")).unwrap(),
        "# Plain\n"
    );
    assert!(!vault.notes[plain].is_kanban());
    // Empty in the index, but another app wrote to it since: refused too.
    let late = vault.resolve("Late").unwrap();
    fs::write(dir.path().join("Late.md"), "theirs\n").unwrap();
    let error = vault.convert_to_kanban(late).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        fs::read_to_string(dir.path().join("Late.md")).unwrap(),
        "theirs\n"
    );
}

const LANES: &str = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] b\n- [ ] A\n\n## Doing\n- [ ] c\n\n## Fertig\n\n**Fertiggestellt**\n- [x] d\n";

#[test]
fn a_done_lane_marker_is_one_line_in_or_out() {
    let mut board = parse(LANES);
    board.set_lane_done(0, true).expect("mark To do");
    assert!(
        board
            .source()
            .contains("## To do\n\n**Complete**\n- [ ] b\n")
    );
    assert_eq!(board.lanes()[0].done_word(), Some("Complete"));
    // No blank line under the title: the marker goes right under it.
    board.set_lane_done(1, true).expect("mark Doing");
    assert!(board.source().contains("## Doing\n**Complete**\n- [ ] c\n"));
    // Removing keeps every other byte, whatever the word.
    board.set_lane_done(2, false).expect("unmark Fertig");
    assert!(board.source().ends_with("## Fertig\n\n- [x] d\n"));
    assert_eq!(
        board.lanes()[2].cards()[0].mark(),
        'x',
        "cards keep their marks"
    );
    board.set_lane_done(0, false).expect("unmark");
    board.set_lane_done(1, false).expect("unmark");
    board.set_lane_done(2, true).expect("mark again");
    assert_eq!(
        board.source(),
        LANES.replace("**Fertiggestellt**", "**Complete**"),
        "only the marker lines changed"
    );
    // A no-op changes nothing; a title on the unterminated last line gets a marker too.
    board.set_lane_done(2, true).expect("again");
    let mut open = parse("---\nkanban-plugin: board\n---\n\n## Last");
    open.set_lane_done(0, true).expect("mark last");
    assert_eq!(
        open.source(),
        "---\nkanban-plugin: board\n---\n\n## Last\n**Complete**"
    );
    assert!(open.lanes()[0].done_word().is_some());
}

#[test]
fn sorting_moves_whole_cards_and_keeps_passthrough_in_place() {
    let source = "---\nkanban-plugin: board\n---\n\n## L\n\n- [ ] beta #ops @{2026-03-01}\n  more beta\n- [x] Alpha\npassthrough between\n- [ ] gamma #api @{2026-01-05} @@{09:00}\n- [ ] delta @{2026-01-05} @@{08:00}\n- [ ] epsilon";
    let mut board = parse(source);
    board.sort_cards(0, CardOrder::Text).expect("text");
    let texts = |b: &Kanban| -> Vec<String> {
        b.lanes()[0]
            .cards()
            .iter()
            .map(|c| c.text().lines().next().unwrap_or_default().to_owned())
            .collect()
    };
    assert_eq!(
        texts(&board),
        [
            "Alpha",
            "beta #ops @{2026-03-01}",
            "delta @{2026-01-05} @@{08:00}",
            "epsilon",
            "gamma #api @{2026-01-05} @@{09:00}"
        ]
    );
    assert!(
        board
            .source()
            .contains("- [ ] beta #ops @{2026-03-01}\n  more beta\npassthrough between\n")
    );
    // The card that moved off the unterminated last line gained a newline; the one moved there has none.
    assert!(
        board
            .source()
            .ends_with("- [ ] gamma #api @{2026-01-05} @@{09:00}")
    );
    // Date: by day, then time; a card with no date last, in its original order.
    board.sort_cards(0, CardOrder::Date).expect("date");
    assert_eq!(
        texts(&board),
        [
            "delta @{2026-01-05} @@{08:00}",
            "gamma #api @{2026-01-05} @@{09:00}",
            "beta #ops @{2026-03-01}",
            "Alpha",
            "epsilon"
        ]
    );
    board.sort_cards(0, CardOrder::Tags).expect("tags");
    assert_eq!(
        texts(&board),
        [
            "gamma #api @{2026-01-05} @@{09:00}",
            "beta #ops @{2026-03-01}",
            "delta @{2026-01-05} @@{08:00}",
            "Alpha",
            "epsilon"
        ]
    );
    // Every line is still there once: the sort only reordered card blocks.
    let mut before: Vec<&str> = source.lines().collect();
    let mut after: Vec<&str> = board.source().lines().collect();
    before.sort_unstable();
    after.sort_unstable();
    assert_eq!(before, after);
    // An ordered lane is not rewritten.
    let sorted = board.source().to_owned();
    board.sort_cards(0, CardOrder::Tags).expect("again");
    assert_eq!(board.source(), sorted);
}

#[test]
fn date_sort_reads_the_board_format_and_puts_undated_cards_last() {
    let source = "---\nkanban-plugin: board\n---\n\n## L\n\n\
                  - [ ] Later @{01.12.2026}\n\
                  - [ ] Undated\n\
                  - [ ] Earlier @{12.11.2026}\n\
                  - [ ] Same @{12.11.2026} @@{09:00}\n\
                  - [ ] Same late @{12.11.2026} @@{10:00}\n\n\
                  %% kanban:settings\n```\n{\"date-format\":\"DD.MM.YYYY\"}\n```\n%%\n";
    let mut board = parse(source);
    board.sort_cards(0, CardOrder::Date).expect("date");
    let texts: Vec<&str> = board.lanes()[0]
        .cards()
        .iter()
        .map(|card| card.text().lines().next().unwrap_or_default())
        .collect();
    assert_eq!(
        texts,
        [
            "Earlier @{12.11.2026}",
            "Same @{12.11.2026} @@{09:00}",
            "Same late @{12.11.2026} @@{10:00}",
            "Later @{01.12.2026}",
            "Undated",
        ]
    );
    // The written text would sort `01.12` before `12.11`; the parsed day does not.
    assert!(board.source().contains("- [ ] Undated\n"));
    // The app defaults can supply the format when the caller resolves them.
    let mut defaults = parse(source);
    let dates = DateSettings::resolve(
        None,
        &super::KanbanDefaults {
            date_format: Some("DD.MM.YYYY".to_owned()),
            ..super::KanbanDefaults::default()
        },
    );
    defaults
        .sort_cards_with(0, CardOrder::Date, &dates)
        .expect("date");
    assert_eq!(
        defaults.lanes()[0].cards()[0].text().lines().next(),
        Some("Earlier @{12.11.2026}")
    );
}

#[test]
fn collapse_writes_only_its_key() {
    let footer =
        "%% kanban:settings\n```\n{\"kanban-plugin\":\"board\", \"lane-width\": 300}\n```\n%%\n";
    let mut board = parse(&format!("{LANES}\n{footer}"));
    assert!(board.lanes().iter().all(|lane| !lane.collapsed()));
    board.set_lane_collapsed(1, true).expect("collapse");
    assert_eq!(
        board.footer_json(),
        Some(
            "{\"kanban-plugin\":\"board\", \"lane-width\": 300,\"list-collapse\":[false,true,false]}"
        )
    );
    assert!(board.lanes()[1].collapsed());
    board.set_lane_collapsed(0, true).expect("collapse");
    assert_eq!(
        board.footer_json(),
        Some(
            "{\"kanban-plugin\":\"board\", \"lane-width\": 300,\"list-collapse\":[true,true,false]}"
        )
    );
    assert!(
        board.source().starts_with(LANES),
        "the lanes' bytes are unchanged"
    );
    // A board with no footer gains one holding only the key.
    let mut bare = parse(LANES);
    bare.set_lane_collapsed(2, true).expect("collapse");
    assert_eq!(
        bare.footer_json(),
        Some("{\"list-collapse\":[false,false,true]}")
    );
    // A short array, or an entry that is not a boolean, is not collapsed.
    let short = parse(&format!(
        "{LANES}\n%% kanban:settings\n```\n{{\"list-collapse\":[1,true]}}\n```\n%%\n"
    ));
    let flags: Vec<bool> = short.lanes().iter().map(Lane::collapsed).collect();
    assert_eq!(flags, [false, true, false]);
}

#[test]
fn adding_or_deleting_a_lane_keeps_collapse_aligned() {
    let footer =
        "%% kanban:settings\n```\n{\"list-collapse\":[false,true,true],\"x\":1}\n```\n%%\n";
    let mut board = parse(&format!("{LANES}\n{footer}"));
    board.add_lane(1, "New", None, false).expect("add");
    assert_eq!(
        board.footer_json(),
        Some("{\"list-collapse\":[false,false,true,true],\"x\":1}")
    );
    let flags: Vec<bool> = board.lanes().iter().map(Lane::collapsed).collect();
    assert_eq!(flags, [false, false, true, true]);
    board.delete_lane(2).expect("delete Doing");
    assert_eq!(
        board.footer_json(),
        Some("{\"list-collapse\":[false,false,true],\"x\":1}")
    );
    assert_eq!(board.lanes()[2].title(), "Fertig");
    assert!(board.lanes()[2].collapsed());
    // Past the end of the array there is nothing to move.
    board.add_lane(3, "Later", None, false).expect("append");
    assert_eq!(
        board.footer_json(),
        Some("{\"list-collapse\":[false,false,true],\"x\":1}")
    );
}

const NOTED: &str = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n\n\n## Notes\n\nA paragraph the page doesn't show.\n### A sub-heading\n%% a comment %%\n\n\n## Doing\n\n- [ ] b\n- [ ] c\n\nParagraph after Doing's cards.\n\n\n## Done\n\n- [x] d\n";

#[test]
fn deleting_a_lane_with_only_note_text_keeps_the_text() {
    let mut board = parse(NOTED);
    assert!(board.lane_delete_drops(1).unwrap().is_empty());
    board.delete_lane(1).expect("delete Notes");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n\n\nA paragraph the page doesn't show.\n### A sub-heading\n%% a comment %%\n\n\n## Doing\n\n- [ ] b\n- [ ] c\n\nParagraph after Doing's cards.\n\n\n## Done\n\n- [x] d\n"
    );
    let titles: Vec<&str> = board.lanes().iter().map(Lane::title).collect();
    assert_eq!(titles, ["To do", "Doing", "Done"]);
    assert_eq!(board.lanes()[0].cards().len(), 1);
}

#[test]
fn deleting_a_lane_with_cards_and_note_text_keeps_the_text() {
    let mut board = parse(NOTED);
    assert!(board.lane_delete_drops(2).unwrap().is_empty());
    board.delete_lane(2).expect("delete Doing");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n\n\n## Notes\n\nA paragraph the page doesn't show.\n### A sub-heading\n%% a comment %%\n\n\nParagraph after Doing's cards.\n\n\n## Done\n\n- [x] d\n"
    );
    // A lane with nothing but cards and blank lines goes whole.
    let mut plain = parse(NOTED);
    plain.delete_lane(3).expect("delete Done");
    assert!(
        plain
            .source()
            .ends_with("Paragraph after Doing's cards.\n\n\n")
    );
}

#[test]
fn note_text_that_would_join_another_lane_goes_with_the_lane_and_is_counted() {
    // Kept, the indented line would read as a continuation of card `a`.
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n\n## B\n\n  indented note\nsecond line\n- [ ] b\n";
    let mut board = parse(source);
    assert_eq!(
        board.lane_delete_drops(1).unwrap(),
        ["indented note", "second line"]
    );
    board.delete_lane(1).expect("delete B");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n\n"
    );
    assert_eq!(board.lanes()[0].cards()[0].text(), "a");
}

#[test]
fn lane_add_and_delete_are_refused_inside_an_open_fence() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n\n## B\n\n- [ ] b\n\n```\nnever closed\n";
    let mut board = parse(source);
    for result in [
        board.clone().add_lane(2, "Later", None, false),
        board.clone().add_lane(0, "First", None, false),
        board.delete_lane(0),
        board.delete_lane(1),
        board.lane_delete_drops(0).map(|_| ()),
    ] {
        let error = result.expect_err("refused");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(
            error.to_string(),
            "the note ends inside an unclosed code fence"
        );
    }
    assert_eq!(board.source(), source);
}

#[test]
fn a_title_ending_in_a_number_in_brackets_is_refused() {
    let mut board = parse(LANES);
    for title in ["Sprint (2)", "Later (0)", "X (2) "] {
        assert_eq!(
            board.rename_lane(0, title).expect_err(title).kind(),
            ErrorKind::InvalidInput
        );
        assert_eq!(
            board
                .add_lane(0, title, None, false)
                .expect_err(title)
                .kind(),
            ErrorKind::InvalidInput
        );
    }
    assert_eq!(board.source(), LANES);
    // Brackets that are not a limit are fine.
    board.rename_lane(0, "Sprint (two)").expect("rename");
    assert_eq!(board.lanes()[0].title(), "Sprint (two)");
    assert_eq!(board.lanes()[0].limit(), None);
}

#[test]
fn a_limit_replaces_trailing_spaces_after_the_title() {
    let mut board = parse("---\nkanban-plugin: board\n---\n\n## Trail  \n\n- [ ] a\n");
    board.set_lane_limit(0, Some(99)).expect("limit");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## Trail (99)\n\n- [ ] a\n"
    );
    assert_eq!(board.lanes()[0].title(), "Trail");
}

#[test]
fn deleting_a_middle_lane_removes_its_own_collapse_entry() {
    let footer =
        "%% kanban:settings\n```\n{\"list-collapse\":[false,true,false],\"x\":1}\n```\n%%\n";
    let mut board = parse(&format!("{LANES}\n{footer}"));
    board.delete_lane(1).expect("delete Doing");
    assert_eq!(
        board.footer_json(),
        Some("{\"list-collapse\":[false,false],\"x\":1}")
    );
    let flags: Vec<bool> = board.lanes().iter().map(Lane::collapsed).collect();
    assert_eq!(flags, [false, false]);
}

#[test]
fn removing_a_marker_that_would_expose_another_is_refused() {
    // A second marker line under the first would make the lane done again.
    let source =
        "---\nkanban-plugin: board\n---\n\n## Done\n\n**Complete**\n**Complete**\n- [x] d\n";
    let mut board = parse(source);
    let error = board.set_lane_done(0, false).expect_err("refused");
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(board.source(), source);
    assert_eq!(board.lanes()[0].done_word(), Some("Complete"));
}

#[test]
fn an_appended_lane_ends_like_the_new_kanban_template() {
    let mut board = parse("---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n\n\n");
    board.add_lane(1, "Later", None, false).expect("append");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n\n\n## Later\n\n\n"
    );
}

#[test]
fn moving_a_lane_moves_its_lines_unchanged() {
    let mut board = parse(LANES);
    board.move_lane(2, 0).expect("Fertig first");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## Fertig\n\n**Fertiggestellt**\n- [x] d\n\
         ## To do\n\n- [ ] b\n- [ ] A\n\n## Doing\n- [ ] c\n\n"
    );
    assert_eq!(board.lanes()[0].done_word(), Some("Fertiggestellt"));
    assert!(board.lanes()[0].cards()[0].is_done());
    board.move_lane(0, 2).expect("Fertig last again");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] b\n- [ ] A\n\n## Doing\n- [ ] c\n\n\
         ## Fertig\n\n**Fertiggestellt**\n- [x] d\n"
    );
    // Text between lanes travels with the lane before it.
    let mut passthrough =
        parse("---\nkanban-plugin: board\n---\n\n## A\n\n- [ ] a\n\nA note.\n\n## B\n\n- [ ] b\n");
    passthrough.move_lane(0, 1).expect("A after B");
    assert_eq!(
        passthrough.source(),
        "---\nkanban-plugin: board\n---\n\n## B\n\n- [ ] b\n## A\n\n- [ ] a\n\nA note.\n\n"
    );
}

#[test]
fn a_lane_that_stays_put_changes_nothing_and_a_bad_index_is_refused() {
    let mut board = parse(LANES);
    board.move_lane(1, 1).expect("no move");
    assert_eq!(board.source(), LANES);
    for (from, to) in [(3, 0), (0, 3)] {
        let error = board.move_lane(from, to).expect_err("out of range");
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
        assert_eq!(board.source(), LANES);
    }
}

#[test]
fn moving_a_lane_keeps_collapse_aligned_and_the_footer_order() {
    let footer =
        "%% kanban:settings\n```\n{\"list-collapse\":[false,true,true],\"x\":1}\n```\n%%\n";
    let mut board = parse(&format!("{LANES}\n{footer}"));
    board.move_lane(0, 2).expect("To do last");
    assert_eq!(
        board.footer_json(),
        Some("{\"list-collapse\":[true,true,false],\"x\":1}")
    );
    let titles: Vec<&str> = board.lanes().iter().map(Lane::title).collect();
    assert_eq!(titles, ["Doing", "Fertig", "To do"]);
    let flags: Vec<bool> = board.lanes().iter().map(Lane::collapsed).collect();
    assert_eq!(flags, [true, true, false]);
    // A short array is padded only as far as the move needs.
    let short = |json: &str| {
        parse(&format!(
            "{LANES}\n%% kanban:settings\n```\n{json}\n```\n%%\n"
        ))
    };
    let mut padded = short("{\"list-collapse\":[true]}");
    padded.move_lane(0, 2).expect("move");
    assert_eq!(
        padded.footer_json(),
        Some("{\"list-collapse\":[false,false,true]}")
    );
    // Lanes past the end of the array are not collapsed either way.
    let mut past = short("{\"list-collapse\":[true]}");
    past.move_lane(2, 1).expect("move");
    assert_eq!(past.footer_json(), Some("{\"list-collapse\":[true]}"));
}

/// Kanban slice 7: a card moved or added into an empty lane goes after the
/// lane's own text (a line at column 0 right under the title, or a later
/// paragraph), so that text stays where it was relative to the title.
#[test]
fn a_card_entering_an_empty_lane_goes_after_its_text() {
    let source = "---\nkanban-plugin: board\n---\n\n## Doing\n\n- [ ] Ship it\n\n\n\
                  ## Notes\nPlain text under the title\n\n\n## Later\n\n\n";
    let mut board = parse(source);
    board.move_card(0, 0, 1, 0).expect("move");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## Doing\n\n\n\n\
         ## Notes\nPlain text under the title\n- [ ] Ship it\n\n\n## Later\n\n\n"
    );
    assert_eq!(board.lanes()[1].cards()[0].text(), "Ship it");
    // Blank lines first, then a paragraph: still after the text.
    let mut board = parse("---\nkanban-plugin: board\n---\n\n## Notes\n\nA paragraph\n\n## B\n");
    board.add_card(0, 0, "New").expect("add");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## Notes\n\nA paragraph\n- [ ] New\n\n## B\n"
    );
    // A lane with nothing under its title keeps the blank line after it.
    let mut board = parse(source);
    board.add_card(2, 0, "Soon").expect("add");
    assert!(
        board.source().ends_with("## Later\n\n- [ ] Soon\n\n"),
        "{:?}",
        board.source()
    );
}

#[test]
fn a_block_id_is_written_once_and_then_left_alone() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Write\n- [ ] Review [[Roadmap]] ^card-7f2\n  with the team\n";
    let mut board = parse(source);
    assert_eq!(board.lanes()[0].cards()[0].block_id(), None);
    let id = board.assign_block_id(0, 0).expect("assign");
    assert_eq!(id, "card-1");
    let written = board.source().to_owned();
    assert_eq!(
        written,
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Write ^card-1\n- [ ] Review [[Roadmap]] ^card-7f2\n  with the team\n"
    );
    // The existing id is not a candidate, and a second request does not rewrite.
    assert_eq!(board.assign_block_id(0, 0).expect("again"), "card-1");
    assert_eq!(board.source(), written);
    assert_eq!(board.assign_block_id(0, 1).expect("kept"), "card-7f2");
    assert_eq!(board.source(), written);
    // The next new id skips both.
    board.add_card(0, 2, "Another").expect("add");
    assert_eq!(board.assign_block_id(0, 2).expect("third"), "card-2");
    assert!(
        board.source().contains("- [ ] Another ^card-2\n"),
        "{}",
        board.source()
    );
}

/// New text is read the way a file is: a trailing ` ^id` on its first line is
/// the card's block id, not text.
#[test]
fn a_trailing_block_id_in_new_text_is_the_card_id() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] keep
- [ ] old ^card-1
";
    let mut board = parse(source);
    board.add_card(0, 0, "Fix login ^abc").expect("add");
    assert_eq!(board.lanes()[0].cards()[0].text(), "Fix login");
    assert_eq!(board.lanes()[0].cards()[0].block_id(), Some("abc"));
    assert_eq!(
        board.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] Fix login ^abc
- [ ] keep
- [ ] old ^card-1
"
    );

    // An edit sets an id that was absent and replaces one that was there.
    board.edit_card(0, 1, "seed ^seed-1").expect("seed");
    assert_eq!(board.lanes()[0].cards()[1].text(), "seed");
    assert_eq!(board.lanes()[0].cards()[1].block_id(), Some("seed-1"));
    board.edit_card(0, 2, "replaced ^new-2").expect("replace");
    assert_eq!(board.lanes()[0].cards()[2].text(), "replaced");
    assert_eq!(board.lanes()[0].cards()[2].block_id(), Some("new-2"));
    assert_eq!(
        board.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] Fix login ^abc
- [ ] seed ^seed-1
- [ ] replaced ^new-2
"
    );

    // A caret that is not a trailing id stays text, and so does the id that
    // was there.
    board
        .edit_card(0, 0, "Fix ^login now")
        .expect("caret in text");
    assert_eq!(board.lanes()[0].cards()[0].text(), "Fix ^login now");
    assert_eq!(board.lanes()[0].cards()[0].block_id(), Some("abc"));
    board.edit_card(0, 0, "Fix login ^abc").expect("same id");
    assert_eq!(board.lanes()[0].cards()[0].text(), "Fix login");
    assert_eq!(board.lanes()[0].cards()[0].block_id(), Some("abc"));

    // Only the first line can carry the id.
    board.add_card(0, 3, "first\nsecond ^abc").expect("add");
    assert_eq!(board.lanes()[0].cards()[3].text(), "first\nsecond ^abc");
    assert_eq!(board.lanes()[0].cards()[3].block_id(), None);
}

#[test]
fn moving_a_lane_to_the_end_of_a_file_without_a_final_newline_keeps_that() {
    let source = "---\nkanban-plugin: board\n---\n\n## A\n\n- [ ] a\n\n## B\n\n- [ ] b";
    let mut board = parse(source);
    assert_eq!(board.source(), source);
    assert!(!board.source().ends_with('\n'));
    board.move_lane(0, 1).expect("A last");
    // The moved block's trailing blank lines go, so the file still ends
    // without a newline (spec section 10 rule 5).
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## B\n\n- [ ] b\n## A\n\n- [ ] a"
    );
    assert!(!board.source().ends_with('\n'));
    assert_eq!(board.lanes()[1].cards()[0].text(), "a");

    let crlf =
        "---\r\nkanban-plugin: board\r\n---\r\n\r\n## A\r\n\r\n- [ ] a\r\n\r\n## B\r\n\r\n- [ ] b";
    let mut board = parse(crlf);
    assert!(!board.source().ends_with('\n'));
    board.move_lane(0, 1).expect("A last");
    assert_eq!(
        board.source(),
        "---\r\nkanban-plugin: board\r\n---\r\n\r\n## B\r\n\r\n- [ ] b\r\n## A\r\n\r\n- [ ] a"
    );
    assert!(!board.source().ends_with('\n'));
}

#[test]
fn a_card_added_at_the_end_of_a_file_without_a_final_newline_keeps_that() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a";
    let mut board = parse(source);
    board.add_card(0, 1, "b").expect("add");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n- [ ] b"
    );
}

#[test]
fn duplicate_footer_keys_edit_and_remove_every_copy() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] Alpha

%% kanban:settings
```
{\"b\":0,\"a\":1,\"a\":2,\"a\":3}
```
%%
";
    let mut board = parse(source);
    assert_eq!(
        board.footer_json(),
        Some("{\"b\":0,\"a\":1,\"a\":2,\"a\":3}")
    );
    // JSON readers use the last member, so setting `a` changes that one.
    board.set_setting("a", &json!(9)).expect("set");
    assert_eq!(
        board.footer_json(),
        Some("{\"b\":0,\"a\":1,\"a\":2,\"a\":9}")
    );
    // Removing `a` takes every copy, so no earlier copy becomes effective.
    board.remove_setting("a").expect("remove");
    assert_eq!(board.footer_json(), Some("{\"b\":0}"));
    let error = board.remove_setting("a").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

/// Two cards with one block id would make `[[Note#^id]]` point at whichever
/// comes first, so a new or changed id another card uses is refused whole.
#[test]
fn a_block_id_already_on_another_card_is_refused() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] first
- [ ] second ^card-1

## Doing

- [ ] third

***

## Archive

- [x] old ^card-9
";
    let mut board = parse(source);
    assert_eq!(board.archive().expect("archive").cards().len(), 1);

    for (lane, index, text) in [(0, 0, "new ^card-1"), (0, 0, "new ^card-9")] {
        let error = board.add_card(lane, index, text).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{text}");
        assert!(error.to_string().contains("already used"), "{error}");
        assert_eq!(board.source(), source, "{text}");
    }
    for (lane, index, text) in [
        (0, 0, "seen ^card-1"),
        (1, 0, "seen ^card-1"),
        (0, 0, "seen ^card-9"),
    ] {
        let error = board.edit_card(lane, index, text).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{text}");
        assert_eq!(board.source(), source, "{text}");
    }
}

#[test]
fn a_card_keeps_its_own_block_id() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] keep ^card-1\n- [ ] other\n";
    let mut board = parse(source);
    // The card's own id is not a collision.
    board.edit_card(0, 0, "kept ^card-1").expect("own id");
    assert_eq!(board.lanes()[0].cards()[0].text(), "kept");
    assert_eq!(board.lanes()[0].cards()[0].block_id(), Some("card-1"));
    // Repeating the same text and id is still a no-op.
    let written = board.source().to_owned();
    board.edit_card(0, 0, "kept ^card-1").expect("again");
    assert_eq!(board.source(), written);
}

/// Literal card text (Split card, Duplicate) never reads an id out of a
/// trailing ` ^id`. The file cannot store that ending as text, so it is
/// refused rather than losing it or assigning an id.
#[test]
fn literal_card_text_with_a_trailing_block_id_is_refused() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] keep\n";
    let mut board = parse(source);
    let error = board.add_card_text(0, 0, "see ^card-1").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert_eq!(board.source(), source);

    // A continuation line is not the card's id, so it stays literal text.
    board
        .add_card_text(0, 0, "head\nsee ^card-1")
        .expect("literal");
    assert_eq!(board.lanes()[0].cards()[0].text(), "head\nsee ^card-1");
    assert_eq!(board.lanes()[0].cards()[0].block_id(), None);
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] head\n  see ^card-1\n- [ ] keep\n"
    );
}

/// A text edit keeps the card's id, so `[[Note#^id]]` still points here.
/// Only an explicit new id in the edited text changes the target.
#[test]
fn a_text_edit_keeps_the_block_link_target_and_only_a_new_id_changes_it() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] Write ^card-1
- [ ] See [[Board#^card-1]]

## Done

**Complete**
- [x] Shipped ^card-2
";
    let mut board = parse(source);
    board
        .edit_card(0, 0, "Write the release notes")
        .expect("text");
    assert_eq!(board.lanes()[0].cards()[0].block_id(), Some("card-1"));
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Write the release notes ^card-1\n\
         - [ ] See [[Board#^card-1]]\n\n## Done\n\n**Complete**\n- [x] Shipped ^card-2\n"
    );
    // Only a new id in the text changes the target; the old one is gone.
    board
        .edit_card(0, 0, "Write the release notes ^card-3")
        .expect("id");
    assert_eq!(board.lanes()[0].cards()[0].block_id(), Some("card-3"));
    assert!(!board.source().contains("^card-1\n"));
}

#[test]
fn adding_a_card_with_only_a_block_id_is_refused() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n";
    let mut board = parse(source);
    for text in [" ^abc", "", "  ", " \n\t"] {
        let error = board.add_card(0, 1, text).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{text:?}");
        assert_eq!(board.source(), source, "{text:?}");
    }
}

/// The spec stores a blank continuation as blank: spaces or tabs on a line by
/// themselves are not text.
#[test]
fn a_blank_continuation_is_stored_blank() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n";
    let mut added = parse(source);
    added.add_card(0, 1, "new\n \n  tail").expect("add");
    assert_eq!(added.lanes()[0].cards()[1].text(), "new\n\n  tail");
    assert_eq!(
        added.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n- [ ] new\n\n    tail\n"
    );

    let mut edited = parse(source);
    edited.edit_card(0, 0, "a\n\t\n b").expect("edit");
    assert_eq!(edited.lanes()[0].cards()[0].text(), "a\n\n b");
    assert_eq!(
        edited.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n\n   b\n"
    );
}

#[test]
fn a_block_id_on_a_crlf_card_keeps_the_line_ending() {
    let source = "---\r\nkanban-plugin: board\r\n---\r\n\r\n## To do\r\n\r\n- [ ] Write\r\n";
    let mut board = parse(source);
    assert_eq!(board.assign_block_id(0, 0).expect("assign"), "card-1");
    assert_eq!(
        board.source(),
        "---\r\nkanban-plugin: board\r\n---\r\n\r\n## To do\r\n\r\n- [ ] Write ^card-1\r\n"
    );
}

// --- Slice 12: the archive ------------------------------------------------

#[test]
fn archiving_the_first_card_creates_the_rule_and_heading_before_the_footer() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] one
- [x] two

## Done

**Complete**
- [x] shipped

%% kanban:settings
```
{\"kanban-plugin\":\"board\"}
```
%%
";
    let mut board = parse(source);
    board.archive_card(1, 0).expect("archive");
    assert_eq!(
        board.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] one
- [x] two

## Done

**Complete**

***

## Archive

- [x] shipped

%% kanban:settings
```
{\"kanban-plugin\":\"board\"}
```
%%
"
    );
    let archive = board.archive().expect("archive");
    assert_eq!(archive.word(), "Archive");
    assert_eq!(archive.cards().len(), 1);
    assert_eq!(archive.cards()[0].text(), "shipped");
    assert!(archive.cards()[0].is_done());
    // The card's own bytes moved, not a re-rendering of them.
    assert!(
        board
            .source()
            .contains("- [x] shipped\n\n%% kanban:settings")
    );
}

#[test]
fn archiving_without_a_footer_appends_the_section_at_the_end() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n- [ ] two\n";
    let mut board = parse(source);
    board.archive_card(0, 0).expect("archive");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] two\n\n\
         ***\n\n## Archive\n\n- [ ] one\n"
    );
}

#[test]
fn an_existing_archive_heading_keeps_the_files_own_word() {
    for word in [
        "Archive",
        "Archiv",
        "Archivio",
        "アーカイブ",
        "보관됨",
        "Arquivado",
        "Архивировать",
        "归档",
    ] {
        let source = format!(
            "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n\n***\n\n## {word}\n\n- [x] old\n"
        );
        let mut board = parse(&source);
        board.archive_card(0, 0).expect("archive");
        assert_eq!(
            board.archive().map(Archive::word),
            Some(word),
            "{word}: the file's word is kept"
        );
        assert_eq!(board.archive().expect("archive").cards().len(), 2);
        assert_eq!(
            board.source(),
            format!(
                "---\nkanban-plugin: board\n---\n\n## To do\n\n\n***\n\n## {word}\n\n- [x] old\n- [ ] one\n"
            ),
            "{word}"
        );
    }
}

#[test]
fn an_empty_existing_archive_takes_the_card_under_its_heading() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n\n***\n\n## Archiv\n\n%% kanban:settings\n```\n{}\n```\n%%\n";
    let mut board = parse(source);
    board.archive_card(0, 0).expect("archive");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n\n***\n\n## Archiv\n\n- [ ] one\n\n%% kanban:settings\n```\n{}\n```\n%%\n"
    );
}

#[test]
fn the_archive_date_is_plain_text_at_the_start_or_the_end() {
    let day = CivilDate::new(2026, 11, 12).expect("date");
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n";

    let mut end = parse(source);
    let end_settings = ArchiveSettings {
        with_date: true,
        date_format: "DD.MM.YYYY".to_owned(),
        date_position: ArchiveDatePosition::End,
        ..ArchiveSettings::default()
    };
    end.archive_card_with(0, 0, &end_settings, Some(day))
        .expect("archive");
    assert!(end.source().contains("- [ ] one 12.11.2026\n"));
    assert_eq!(
        end.archive().expect("archive").cards()[0].text(),
        "one 12.11.2026"
    );

    let mut start = parse(source);
    let start_settings = ArchiveSettings {
        with_date: true,
        date_format: "YYYY-MM-DD".to_owned(),
        date_position: ArchiveDatePosition::Start,
        date_separator: " · ".to_owned(),
        ..ArchiveSettings::default()
    };
    start
        .archive_card_with(0, 0, &start_settings, Some(day))
        .expect("archive");
    assert!(
        start.source().contains("- [ ] 2026-11-12 · one\n"),
        "{}",
        start.source()
    );

    let mut tight = parse(source);
    let tight_settings = ArchiveSettings {
        with_date: true,
        date_position: ArchiveDatePosition::End,
        date_separator: String::new(),
        ..ArchiveSettings::default()
    };
    tight
        .archive_card_with(0, 0, &tight_settings, Some(day))
        .expect("archive");
    assert!(tight.source().contains("- [ ] one2026-11-12\n"));

    // No date given writes none, whatever the setting says; the off setting
    // writes none either.
    let mut none = parse(source);
    none.archive_card_with(0, 0, &end_settings, None)
        .expect("archive");
    assert!(none.source().contains("- [ ] one\n"));
    let mut off = parse(source);
    off.archive_card_with(0, 0, &ArchiveSettings::default(), Some(day))
        .expect("archive");
    assert!(off.source().contains("- [ ] one\n"));
}

#[test]
fn the_archive_date_keeps_a_block_id_at_the_end_of_the_line() {
    let day = CivilDate::new(2026, 11, 12).expect("date");
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one ^card-1\n";
    let mut board = parse(source);
    board
        .archive_card_with(
            0,
            0,
            &ArchiveSettings {
                with_date: true,
                ..ArchiveSettings::default()
            },
            Some(day),
        )
        .expect("archive");
    assert!(board.source().contains("- [ ] one 2026-11-12 ^card-1\n"));
    assert_eq!(
        board.archive().expect("archive").cards()[0].block_id(),
        Some("card-1")
    );
    assert_eq!(
        board.archive().expect("archive").cards()[0].text(),
        "one 2026-11-12"
    );
}

#[test]
fn the_archive_limit_drops_the_oldest_cards_and_never_a_lane_card() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] lane one
- [ ] lane two

***

## Archive

- [x] oldest
- [x] older
- [x] old
";
    let settings = ArchiveSettings {
        max_cards: Some(2),
        ..ArchiveSettings::default()
    };
    let mut board = parse(source);
    board
        .archive_card_with(0, 0, &settings, None)
        .expect("archive");
    assert_eq!(
        board.archive().expect("archive").cards().len(),
        2,
        "the limit holds"
    );
    let archive: Vec<&str> = board
        .archive()
        .expect("archive")
        .cards()
        .iter()
        .map(|card| card.text())
        .collect();
    assert_eq!(archive, ["old", "lane one"]);
    assert_eq!(board.lanes()[0].cards().len(), 1, "the lane is untouched");
    assert_eq!(board.lanes()[0].cards()[0].text(), "lane two");
    assert!(!board.source().contains("- [x] oldest\n"));

    // Absent, and a written 0, are unlimited.
    for max in [None, Some(0)] {
        let mut board = parse(source);
        board
            .archive_card_with(
                0,
                0,
                &ArchiveSettings {
                    max_cards: max,
                    ..ArchiveSettings::default()
                },
                None,
            )
            .expect("archive");
        assert_eq!(
            board.archive().expect("archive").cards().len(),
            4,
            "{max:?}: unlimited"
        );
    }
}

#[test]
fn max_archive_cards_resolves_footer_then_defaults() {
    let footer = r#"{"kanban-plugin":"board","max-archive-cards":1,"archive-with-date":true}"#;
    let settings = ArchiveSettings::resolve(Some(footer), &KanbanDefaults::default());
    assert_eq!(settings.max_cards, Some(1));
    assert!(settings.with_date);

    let defaults = KanbanDefaults::from_json(Some(&json!({
        "max-archive-cards": 3,
        "archive-date-format": "DD.MM.YYYY",
        "archive-date-position": "start",
        "archive-date-separator": " — ",
    })));
    let settings = ArchiveSettings::resolve(None, &defaults);
    assert_eq!(settings.max_cards, Some(3));
    assert_eq!(settings.date_format, "DD.MM.YYYY");
    assert_eq!(settings.date_position, ArchiveDatePosition::Start);
    assert_eq!(settings.date_separator, " — ");

    // The footer wins; an unknown value leaves the built-in.
    let settings = ArchiveSettings::resolve(
        Some(r#"{"archive-date-position":"sideways","max-archive-cards":0}"#),
        &defaults,
    );
    assert_eq!(settings.date_position, ArchiveDatePosition::Start);
    assert_eq!(settings.max_cards, None, "0 is unlimited");
    let built_in = ArchiveSettings::resolve(
        Some(r#"{"archive-date-position":"sideways"}"#),
        &KanbanDefaults::default(),
    );
    assert_eq!(built_in.date_position, ArchiveDatePosition::End);
    assert_eq!(built_in.date_format, "YYYY-MM-DD");
    assert_eq!(built_in.date_separator, " ");
    assert!(!built_in.with_date);
}

#[test]
fn unarchiving_puts_the_card_whole_at_the_end_of_the_chosen_lane() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] one

## Done

**Complete**
- [x] old

***

## Archive

- [x] first
- [x] second
  with a second line
  - [ ] nested
- [x] third ^card-9
";
    let mut board = parse(source);
    board.unarchive_card(1, 1).expect("unarchive");
    assert_eq!(
        board.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] one

## Done

**Complete**
- [x] old
- [x] second
  with a second line
  - [ ] nested

***

## Archive

- [x] first
- [x] third ^card-9
"
    );
    assert_eq!(
        board.lanes()[1].cards()[1].text(),
        "second\nwith a second line\n- [ ] nested"
    );

    // Back to the archive: the same bytes as the start, in the file order
    // the moves left.
    board.archive_card(1, 1).expect("archive again");
    assert_eq!(board.archive().expect("archive").cards().len(), 3);
    assert!(
        board
            .source()
            .contains("- [x] second\n  with a second line\n  - [ ] nested\n")
    );
    assert_eq!(
        board.archive().expect("archive").cards()[2].block_id(),
        None
    );

    // A card with an id keeps it when it returns.
    board.unarchive_card(1, 1).expect("unarchive third");
    let last = board.lanes()[1].cards().last().expect("restored");
    assert_eq!(last.block_id(), Some("card-9"));
    assert_eq!(last.text(), "third");
    let words: Vec<&str> = board
        .archive()
        .expect("archive")
        .cards()
        .iter()
        .map(|card| card.text())
        .collect();
    assert_eq!(words, ["first", "second\nwith a second line\n- [ ] nested"]);
}

#[test]
fn deleting_an_archived_card_removes_only_its_lines() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] one

***

## Archive

- [x] keep

- [x] gone
  second line

%% kanban:settings
```
{\"kanban-plugin\":\"board\"}
```
%%
";
    let mut board = parse(source);
    board.delete_archived_card(1).expect("delete");
    assert_eq!(
        board.source(),
        "\
---
kanban-plugin: board
---

## To do

- [ ] one

***

## Archive

- [x] keep


%% kanban:settings
```
{\"kanban-plugin\":\"board\"}
```
%%
"
    );
    assert_eq!(board.archive().expect("archive").cards().len(), 1);
}

#[test]
fn archiving_a_crlf_card_keeps_every_line_ending() {
    let source = "---\r\nkanban-plugin: board\r\n---\r\n\r\n## To do\r\n\r\n- [ ] one\r\n  second\r\n- [ ] two\r\n";
    let mut board = parse(source);
    board.archive_card(0, 0).expect("archive");
    assert_eq!(
        board.source(),
        "---\r\nkanban-plugin: board\r\n---\r\n\r\n## To do\r\n\r\n- [ ] two\r\n\r\n\
         ***\r\n\r\n## Archive\r\n\r\n- [ ] one\r\n  second\r\n"
    );
    assert!(!board.source().replace("\r\n", "").contains('\n'));

    let mut restored = parse(board.source());
    restored.unarchive_card(0, 0).expect("unarchive");
    assert_eq!(
        restored.source(),
        "---\r\nkanban-plugin: board\r\n---\r\n\r\n## To do\r\n\r\n- [ ] two\r\n\
         - [ ] one\r\n  second\r\n\r\n***\r\n\r\n## Archive\r\n\r\n"
    );
    assert!(!restored.source().replace("\r\n", "").contains('\n'));
}

#[test]
fn archiving_keeps_passthrough_text_where_it_was() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] one

A paragraph between lanes.

  an indented note

## Done

**Complete**
- [x] shipped

***

## Archive

A note the board doesn't show.

- [x] old
";
    let mut board = parse(source);
    board.archive_card(0, 0).expect("archive");
    assert_eq!(
        board.source(),
        "\
---
kanban-plugin: board
---

## To do


A paragraph between lanes.

  an indented note

## Done

**Complete**
- [x] shipped

***

## Archive

A note the board doesn't show.

- [x] old
- [ ] one
"
    );

    // A card with an indented continuation arrives whole.
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n  second\n  third\n";
    let mut board = parse(source);
    board.archive_card(0, 0).expect("archive");
    assert!(board.source().contains("- [ ] one\n  second\n  third\n"));
    assert_eq!(
        board.archive().expect("archive").cards()[0].text(),
        "one\nsecond\nthird"
    );
}

#[test]
fn archiving_an_indented_fenced_card_keeps_the_fence_with_it() {
    let source = "\
---
kanban-plugin: board
---

## To do

- [ ] one
  ```md
  - [ ] not a card
  ```
- [ ] two
";
    let mut board = parse(source);
    board.archive_card(0, 0).expect("archive");
    assert!(
        board
            .source()
            .contains("***\n\n## Archive\n\n- [ ] one\n  ```md\n  - [ ] not a card\n  ```\n")
    );
    assert_eq!(board.lanes()[0].cards().len(), 1);
    assert_eq!(board.lanes()[0].cards()[0].text(), "two");
    assert_eq!(board.archive().expect("archive").cards().len(), 1);
}

#[test]
fn a_heading_named_archive_without_a_rule_is_an_ordinary_lane() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n\n## Archive\n\n- [x] a lane card\n";
    let mut board = parse(source);
    assert!(board.archive().is_none());
    board.archive_card(0, 0).expect("archive");
    assert_eq!(board.lanes().len(), 2, "the lane named Archive stays");
    assert_eq!(
        board.archive().expect("archive").word(),
        "Archive",
        "the new section is English"
    );
    assert_eq!(board.archive().expect("archive").cards()[0].text(), "one");
}

#[test]
fn archiving_into_a_note_that_ends_in_an_open_fence_is_refused() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n\n```\nnever closed\n";
    let mut board = parse(source);
    let error = board.archive_card(0, 0).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(board.source(), source);
}

#[test]
fn archiving_an_unknown_card_changes_nothing() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n";
    let mut board = parse(source);
    assert_eq!(
        board.archive_card(0, 1).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        board.archive_card(9, 0).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        board.unarchive_card(0, 0).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        board.delete_archived_card(0).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(board.source(), source);
}

#[test]
fn the_archive_keeps_a_non_default_block_id_shape_and_multibyte() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] 漢字 🎉 ^card-α\n";
    let mut board = parse(source);
    // The parser's id shape is ASCII: the card text keeps the tail.
    assert_eq!(board.lanes()[0].cards()[0].block_id(), None);
    board.archive_card(0, 0).expect("archive");
    assert_eq!(
        board.archive().expect("archive").cards()[0].text(),
        "漢字 🎉 ^card-α"
    );
}

#[test]
fn an_empty_card_gets_the_date_and_stays_a_card() {
    let day = CivilDate::new(2026, 11, 12).expect("date");
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ]\n- [ ] two\n";
    let mut board = parse(source);
    board
        .archive_card_with(
            0,
            0,
            &ArchiveSettings {
                with_date: true,
                ..ArchiveSettings::default()
            },
            Some(day),
        )
        .expect("archive");
    assert!(board.source().contains("- [ ] 2026-11-12\n"));
    assert_eq!(
        board.archive().expect("archive").cards()[0].text(),
        "2026-11-12"
    );
}

#[test]
fn archiving_into_a_file_with_no_final_newline_keeps_that_ending() {
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n- [ ] two";
    let mut board = parse(source);
    board.archive_card(0, 0).expect("archive");
    assert_eq!(
        board.source(),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] two\n\n\
         ***\n\n## Archive\n\n- [ ] one"
    );
    assert!(!board.source().ends_with('\n'));
    board.unarchive_card(0, 0).expect("restore");
    // The archive's last line was the file's last line; taking it out leaves
    // the blank line before it, as deleting any last line does. The card
    // itself arrives whole.
    assert_eq!(
        board.lanes()[0].cards().last().expect("restored").text(),
        "one"
    );
    assert!(board.source().contains("- [ ] two\n- [ ] one\n"));
}
