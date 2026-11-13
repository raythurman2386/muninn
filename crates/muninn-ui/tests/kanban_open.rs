//! Kanban slice 3: the ways in and out of the Kanban page. The sidebar's
//! Kanbans section, the notes-list glyph, the palette's kanban entries, New
//! Kanban, Convert empty note to Kanban, Open as kanban and Open as markdown,
//! and which page a pick lands on.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, PaletteAction, PaletteItem, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const SPRINT: &str = include_str!("../../muninn-core/tests/fixtures/kanban/lanes-and-limits.md");
const BUSY: &str = include_str!("../../muninn-core/tests/fixtures/kanban/over-limit.md");
const NEW_LAYOUT: &str = include_str!("../../muninn-core/tests/fixtures/kanban/new-kanban.md");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-open-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

/// `Agenda` sorts first, so a fresh launch selects a note that is not a kanban.
fn kanban_vault() -> TempVault {
    TempVault::new(&[
        ("Agenda.md", "# Agenda\n\nSee [[Sprint]].\n"),
        ("Busy.md", BUSY),
        ("Plans/Sprint.md", SPRINT),
    ])
}

fn fresh(tmp: &TempVault) -> Runtime<State> {
    let config = unique_dir("config");
    let state_home = unique_dir("state");
    let state = State::with_settings(
        tmp.path().to_path_buf(),
        state_home,
        config.clone(),
        Settings::load_from(&config),
    );
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn frame(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
}

fn command(c: char) -> KeyEvent {
    KeyEvent {
        key: Key::Command(c),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    }
}

fn key(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
    }
}

fn open_palette(rt: &mut Runtime<State>) {
    rt.key(KeyEvent {
        key: Key::Command('p'),
        mods: Mods {
            cmd: true,
            shift: true,
            ..Mods::default()
        },
    });
    frame(rt);
}

fn type_text(rt: &mut Runtime<State>, typed: &str) {
    for c in typed.chars() {
        rt.key(key(Key::Text(c.to_string())));
        frame(rt);
    }
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frame(rt);
}

fn id(rt: &Runtime<State>, title: &str) -> usize {
    rt.state.vault.resolve(title).expect(title)
}

fn shown(rt: &Runtime<State>) -> Option<&str> {
    rt.state.kanban.as_ref().map(|k| k.rel.as_str())
}

fn text_of(rt: &Runtime<State>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
}

/// Titles in the sidebar's Kanbans section, in order.
fn section(rt: &Runtime<State>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter(|e| {
            e.key
                .as_deref()
                .is_some_and(|k| k.starts_with("sidebar-kanban-title-"))
        })
        .filter_map(|e| e.text)
        .collect()
}

/// Every `.md` file under `root`, with its bytes.
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(dir).expect("dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                let bytes = fs::read(&path).expect("bytes");
                files.push((path, bytes));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn the_kanbans_section_lists_kanbans_and_opens_one_on_the_page() {
    let tmp = kanban_vault();
    let mut rt = fresh(&tmp);
    let before = snapshot(tmp.path());
    assert_eq!(section(&rt), ["Busy", "Sprint"]);
    assert_eq!(rt.state.mode, Mode::Editor);
    let sprint = id(&rt, "Sprint");
    click(&mut rt, &format!("sidebar-kanban-{sprint}"));
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(shown(&rt), Some("Plans/Sprint.md"));
    assert!(!rt.state.is_typing(), "the page has the keyboard");
    assert_eq!(snapshot(tmp.path()), before, "opening writes nothing");
}

#[test]
fn kanban_rows_in_the_notes_list_carry_a_glyph() {
    let tmp = kanban_vault();
    let rt = fresh(&tmp);
    for (title, glyph) in [("Agenda", false), ("Busy", true), ("Sprint", true)] {
        let key = format!("note-row-{}-kanban", id(&rt, title));
        assert_eq!(rt.find(&key).is_some(), glyph, "{title}");
        assert!(rt.find(&format!("note-row-{}", id(&rt, title))).is_some());
    }
}

#[test]
fn the_kanbans_section_follows_file_changes() {
    let tmp = kanban_vault();
    let mut rt = fresh(&tmp);
    assert_eq!(section(&rt), ["Busy", "Sprint"]);
    // Another app adds a kanban and turns Busy into a plain note. The
    // section keeps the index's path order (Plans/Sprint before Roadmap).
    fs::write(tmp.path().join("Roadmap.md"), NEW_LAYOUT).unwrap();
    fs::write(tmp.path().join("Busy.md"), "# Busy\n").unwrap();
    let mut waited = 0;
    while section(&rt) != ["Sprint", "Roadmap"] {
        std::thread::sleep(std::time::Duration::from_millis(100));
        rt.frame(0.6);
        waited += 1;
        assert!(
            waited < 60,
            "the section never followed: {:?}",
            section(&rt)
        );
    }
    assert!(
        rt.find(&format!("note-row-{}-kanban", id(&rt, "Busy")))
            .is_none()
    );
    // Typing in the editor is the other way a note changes.
    let agenda = id(&rt, "Agenda");
    rt.state.open_from_list(agenda);
    rt.state
        .editor
        .replace_text("---\nkanban-plugin: board\n---\n\n## Later\n");
    rt.state.save_now();
    frame(&mut rt);
    assert_eq!(section(&rt), ["Agenda", "Sprint", "Roadmap"], "path order");
}

#[test]
fn a_kanban_opened_as_a_note_offers_open_as_kanban() {
    let tmp = kanban_vault();
    let mut rt = fresh(&tmp);
    let agenda = id(&rt, "Agenda");
    assert_eq!(rt.state.selected, Some(agenda));
    assert!(rt.find("open-as-kanban").is_none(), "not on a plain note");

    // A wikilink: Agenda links to Sprint.
    let sprint = id(&rt, "Sprint");
    rt.state.open_from_link(sprint);
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Editor, "a link opens the note");
    assert!(rt.find("open-as-kanban").is_some());

    // Search (⌘K), Enter: a kanban note opens on the Kanban page (B1).
    rt.state.open_from_list(agenda);
    rt.key(command('k'));
    frame(&mut rt);
    type_text(&mut rt, "Busy");
    rt.key(key(Key::Down));
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert_eq!(rt.state.selected, Some(id(&rt, "Busy")));
    assert_eq!(rt.state.mode, Mode::Kanban, "search opens the kanban");
    rt.state.search.replace_text("");
    frame(&mut rt);

    // The notes list, outside the Kanban page, opens the note.
    rt.key(command('1'));
    frame(&mut rt);
    rt.state.open_from_list(id(&rt, "Busy"));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Editor, "the list opens the note");
    assert!(rt.find("open-as-kanban").is_some());

    // Read keeps the button too; it switches to Kanban on that note.
    rt.key(command('2'));
    frame(&mut rt);
    click(&mut rt, "open-as-kanban");
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(shown(&rt), Some("Busy.md"));
    assert!(
        rt.find("open-as-kanban").is_none(),
        "not on the Kanban page"
    );

    // Open as markdown, in the header of a readable kanban, returns to Write.
    click(&mut rt, "kanban-open-markdown");
    assert_eq!(rt.state.mode, Mode::Editor);
    assert_eq!(rt.state.selected, Some(id(&rt, "Busy")));
    assert!(rt.state.editor.text.contains("kanban-plugin"));
}

#[test]
fn the_palette_offers_new_kanban_and_each_kanban_by_name() {
    let tmp = kanban_vault();
    let mut rt = fresh(&tmp);
    let (busy, sprint) = (id(&rt, "Busy"), id(&rt, "Sprint"));
    // The search field had the keyboard when the palette opened.
    rt.key(command('k'));
    frame(&mut rt);
    open_palette(&mut rt);
    let items = rt.state.palette_items();
    assert!(items.contains(&PaletteItem::Action(PaletteAction::NewKanban)));
    assert!(
        !items.contains(&PaletteItem::Action(PaletteAction::ConvertToKanban)),
        "Agenda has text"
    );
    let kanbans: Vec<_> = items
        .iter()
        .filter(|i| matches!(i, PaletteItem::Kanban(_)))
        .collect();
    assert_eq!(
        kanbans,
        [&PaletteItem::Kanban(busy), &PaletteItem::Kanban(sprint)]
    );

    type_text(&mut rt, "sprint");
    let items = rt.state.palette_items();
    let at = items
        .iter()
        .position(|i| *i == PaletteItem::Kanban(sprint))
        .expect("Sprint by name");
    assert!(!items.contains(&PaletteItem::Kanban(busy)));
    for _ in 0..at {
        rt.key(key(Key::Down));
    }
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(shown(&rt), Some("Plans/Sprint.md"));
    assert!(!rt.state.search.focused && !rt.state.is_typing());
    // A key the page handles reaches it: Esc leaves Kanban.
    rt.key(key(Key::Escape));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Editor);
}

#[test]
fn new_kanban_writes_the_layout_in_the_selected_folder_and_never_overwrites() {
    let tmp = kanban_vault();
    fs::write(tmp.path().join("Plans/Untitled Kanban.md"), "mine\n").unwrap();
    let mut rt = fresh(&tmp);
    rt.state.open_from_list(id(&rt, "Sprint"));
    for _ in 0..2 {
        open_palette(&mut rt);
        type_text(&mut rt, "new kanban");
        assert_eq!(
            rt.state.palette_items(),
            [PaletteItem::Action(PaletteAction::NewKanban)]
        );
        rt.key(key(Key::Enter));
        frame(&mut rt);
    }
    let plans = tmp.path().join("Plans");
    assert_eq!(
        fs::read_to_string(plans.join("Untitled Kanban.md")).unwrap(),
        "mine\n"
    );
    for name in ["Untitled Kanban 2.md", "Untitled Kanban 3.md"] {
        assert_eq!(
            fs::read_to_string(plans.join(name)).unwrap(),
            NEW_LAYOUT,
            "{name}"
        );
    }
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(shown(&rt), Some("Plans/Untitled Kanban 3.md"));
    assert_eq!(
        text_of(&rt, "kanban-lane-2-title").as_deref(),
        Some("Done"),
        "the new board's lanes"
    );
    assert_eq!(
        section(&rt),
        ["Busy", "Sprint", "Untitled Kanban 2", "Untitled Kanban 3"]
    );
    assert!(!rt.state.is_typing());
}

#[test]
fn convert_is_offered_for_an_empty_note_only_and_refuses_text() {
    let tmp = TempVault::new(&[("Agenda.md", "# Agenda\n"), ("Blank.md", "  \n")]);
    let mut rt = fresh(&tmp);
    let blank = id(&rt, "Blank");

    // Refused on a note with text, even when asked directly.
    open_palette(&mut rt);
    type_text(&mut rt, "convert");
    assert!(rt.state.palette_items().is_empty(), "not offered");
    rt.key(key(Key::Escape));
    rt.state.convert_to_kanban();
    frame(&mut rt);
    assert!(rt.state.notice.is_some(), "the refusal is said");
    assert_eq!(rt.state.mode, Mode::Editor);
    assert_eq!(
        fs::read_to_string(tmp.path().join("Agenda.md")).unwrap(),
        "# Agenda\n"
    );

    rt.state.open_from_list(blank);
    open_palette(&mut rt);
    type_text(&mut rt, "convert");
    assert_eq!(
        rt.state.palette_items(),
        [PaletteItem::Action(PaletteAction::ConvertToKanban)]
    );
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(shown(&rt), Some("Blank.md"));
    assert_eq!(
        rt.state.editor.text, NEW_LAYOUT,
        "the editor holds the file"
    );
    // A save from the old, empty buffer can't follow and undo it.
    rt.state.save_now();
    rt.frame(3.0);
    assert_eq!(
        fs::read_to_string(tmp.path().join("Blank.md")).unwrap(),
        NEW_LAYOUT
    );
    assert_eq!(section(&rt), ["Blank"]);
    assert_eq!(rt.state.selected, Some(blank));
}

#[test]
fn on_the_kanban_page_a_kanban_pick_stays_and_a_note_pick_leaves() {
    let tmp = kanban_vault();
    let mut rt = fresh(&tmp);
    let (agenda, busy, sprint) = (id(&rt, "Agenda"), id(&rt, "Busy"), id(&rt, "Sprint"));
    rt.state.open_from_list(sprint);
    rt.key(command('5'));
    frame(&mut rt);
    assert_eq!(shown(&rt), Some("Plans/Sprint.md"));

    // The notes list: a kanban stays on the page.
    rt.state.open_from_list(busy);
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(shown(&rt), Some("Busy.md"));
    // The Kanbans section: the same.
    click(&mut rt, &format!("sidebar-kanban-{sprint}"));
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(shown(&rt), Some("Plans/Sprint.md"));
    // A plain note drops to its note mode, as before.
    rt.state.open_from_list(agenda);
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Editor);
    assert_eq!(rt.state.selected, Some(agenda));
}

#[test]
fn the_title_bar_names_the_kanban_shown_not_the_selected_note() {
    let tmp = kanban_vault();
    let mut rt = fresh(&tmp);
    rt.state.open_from_list(id(&rt, "Sprint"));
    rt.key(command('5'));
    frame(&mut rt);
    assert_eq!(text_of(&rt, "topbar-title").as_deref(), Some("Sprint"));
    // Back to Agenda: the page keeps Sprint, and so does the title.
    rt.key(command('['));
    frame(&mut rt);
    assert_eq!(rt.state.selected, Some(id(&rt, "Agenda")));
    assert_eq!(shown(&rt), Some("Plans/Sprint.md"));
    assert_eq!(text_of(&rt, "topbar-title").as_deref(), Some("Sprint"));
    rt.key(key(Key::Escape));
    frame(&mut rt);
    assert_eq!(text_of(&rt, "topbar-title").as_deref(), Some("Agenda"));
}

/// B1 (slice 3 review): a kanban note picked from the sidebar search
/// opens on the Kanban page from any mode, with the keyboard on the page. A
/// plain note picked there opens as before.
#[test]
fn a_kanban_picked_from_search_opens_on_the_kanban_page_from_any_mode() {
    let tmp = kanban_vault();
    let mut rt = fresh(&tmp);
    let before = snapshot(tmp.path());
    let (agenda, busy) = (id(&rt, "Agenda"), id(&rt, "Busy"));
    for start in ['1', '2', '3', '4'] {
        rt.state.open_from_list(agenda);
        rt.key(command(start));
        frame(&mut rt);
        let mode = rt.state.mode;
        rt.key(command('k'));
        frame(&mut rt);
        type_text(&mut rt, "Busy");
        rt.key(key(Key::Enter));
        frame(&mut rt);
        assert_eq!(rt.state.mode, Mode::Kanban, "from {mode:?}");
        assert_eq!(shown(&rt), Some("Busy.md"));
        assert_eq!(rt.state.selected, Some(busy));
        assert!(!rt.state.is_typing(), "the page has the keyboard");
        // The page's own key reaches it: Esc leaves Kanban.
        rt.key(key(Key::Escape));
        frame(&mut rt);
        assert_ne!(rt.state.mode, Mode::Kanban, "Esc from {mode:?}");
        rt.state.search.replace_text("");
        frame(&mut rt);
    }

    // A click on a search result is a pick from search too.
    rt.state.open_from_list(agenda);
    frame(&mut rt);
    rt.key(command('k'));
    frame(&mut rt);
    type_text(&mut rt, "Busy");
    click(&mut rt, &format!("note-row-{busy}"));
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(shown(&rt), Some("Busy.md"));
    rt.key(key(Key::Escape));
    frame(&mut rt);

    // A plain note from search opens in Write, as before.
    rt.state.search.replace_text("");
    rt.key(command('1'));
    frame(&mut rt);
    rt.key(command('k'));
    frame(&mut rt);
    type_text(&mut rt, "Agenda");
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Editor);
    assert_eq!(rt.state.selected, Some(agenda));
    assert_eq!(snapshot(tmp.path()), before, "picking writes nothing");
}
