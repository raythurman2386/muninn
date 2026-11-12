//! Behaviour tests: bytes in, bytes out, and the model a caller can see.
//!
//! Fixtures under `tests/fixtures/kanban/` are hand-written from
//! `docs/kanban-format.md`.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde_json::json;

use super::{Kanban, is_kanban};
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

    // Dates, tags and inline fields stay card text (D-1, D-2, T-1).
    let text = parse(
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Due @{2026-11-11} @@{14:30} #release [owner:: muninn]\n",
    );
    assert_eq!(
        text.lanes()[0].cards()[0].text(),
        "Due @{2026-11-11} @@{14:30} #release [owner:: muninn]"
    );
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
    let mut deleted = kanban;
    deleted.delete_lane(0).unwrap();
    assert_eq!(
        deleted.source(),
        "\
---
kanban-plugin: board
---

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

/// The text `edit_card` stores: carriage returns dropped, one trailing newline trimmed.
fn normalised_edit(text: &str) -> String {
    let mut text = text.replace('\r', "");
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
    let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] a\n  l1\n";
    let mut kanban = parse(source);
    let error = kanban.edit_card(0, 0, "a ^abc\nl1").unwrap_err();
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
fn one_indented_continuation_keeps_its_indent() {
    let source = read_fixture("indent-deep-line.md");
    let mut kanban = parse(&source);
    assert_eq!(kanban.source(), source);
    assert_eq!(kanban.lanes()[0].cards()[0].text(), "x\n\n\n  y");
    let text = kanban.lanes()[0].cards()[0].text().to_owned();
    kanban.edit_card(0, 0, &text).unwrap();
    assert_eq!(kanban.source(), source);
    let mut fresh = parse("---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] keep\n");
    fresh.edit_card(0, 0, "x\n\n\n  y").unwrap();
    assert_eq!(fresh.lanes()[0].cards()[0].text(), "x\n\n\n  y");
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
