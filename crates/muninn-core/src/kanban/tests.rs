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
    assert_eq!(edited.lanes()[0].cards()[1].text(), "Beta\nnested still");
    assert!(edited.source().contains("- [ ] Beta\n  nested still\n"));
    assert!(edited.source().contains("- [ ] Alpha\n"));
    assert!(!edited.source().contains("  nested\n"));

    let mut deleted = base.clone();
    deleted.delete_card(0, 0).unwrap();
    assert_eq!(deleted.lanes()[0].cards().len(), 1);
    assert_eq!(deleted.lanes()[0].cards()[0].text(), "Beta\nnested");
    assert!(!deleted.source().contains("Alpha"));
    assert!(deleted.source().contains("  nested\n"));

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
    assert_eq!(across.lanes()[0].cards().len(), 1);
    assert_eq!(across.lanes()[1].cards()[0].text(), "Alpha");
    assert_eq!(across.lanes()[1].cards()[1].text(), "Gamma");
    assert!(
        across
            .source()
            .contains("## Doing (3)\n\n- [ ] Alpha\n- [X] Gamma\n")
    );

    let mut renamed = base.clone();
    renamed.rename_lane(1, "Busy").unwrap();
    assert_eq!(renamed.lanes()[1].title(), "Busy");
    assert_eq!(renamed.lanes()[1].limit(), Some(3));
    assert!(renamed.source().contains("## Busy (3)\n"));
    assert!(!renamed.source().contains("## Doing"));

    let mut limited = base.clone();
    limited.set_lane_limit(0, Some(4)).unwrap();
    assert!(limited.source().contains("## To do (4)\n"));
    limited.set_lane_limit(1, Some(9)).unwrap();
    assert!(limited.source().contains("## Doing (9)\n"));
    assert!(!limited.source().contains("## Doing (3)\n"));
    limited.set_lane_limit(1, None).unwrap();
    assert!(limited.source().contains("## Doing\n"));
    assert_eq!(limited.lanes()[1].limit(), None);
    limited.set_lane_limit(1, Some(0)).unwrap();
    assert!(!limited.source().contains("(0)"));

    let mut toggled = base.clone();
    toggled.toggle_done(1, 0).unwrap();
    assert_eq!(toggled.lanes()[1].cards()[0].mark(), ' ');
    assert!(toggled.source().contains("- [ ] Gamma\n"));
    assert!(!toggled.source().contains("- [X] Gamma\n"));
    toggled.toggle_done(1, 0).unwrap();
    assert_eq!(toggled.lanes()[1].cards()[0].mark(), 'x');
    assert!(toggled.source().contains("- [x] Gamma\n"));

    let mut created = base.clone();
    created.add_lane(1, "Review", Some(2), false).unwrap();
    assert_eq!(created.lanes()[1].title(), "Review");
    assert_eq!(created.lanes()[1].limit(), Some(2));
    assert!(
        created
            .source()
            .contains("## Review (2)\n\n\n\n## Doing (3)\n")
    );
    created.add_lane(0, "Later", None, true).unwrap();
    assert_eq!(created.lanes()[0].done_word(), Some("Complete"));
    assert!(created.source().contains("## Later\n\n**Complete**\n"));

    let mut removed = base.clone();
    removed.delete_lane(2).unwrap();
    assert_eq!(removed.lanes().len(), 2);
    assert!(!removed.source().contains("## Done"));
    assert!(!removed.source().contains("Delta"));
    assert!(removed.source().contains("## Doing (3)\n"));
    assert!(removed.source().contains("- [X] Gamma\n"));
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
    let dir = std::env::temp_dir().join(format!("muninn-kanban-save-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Board.md");
    kanban.save(&path).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), kanban.source());
    let names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("Board.md")]);
    fs::remove_dir_all(&dir).unwrap();
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
    assert!(kanban.source().contains("- [ ] Done\n"));
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
