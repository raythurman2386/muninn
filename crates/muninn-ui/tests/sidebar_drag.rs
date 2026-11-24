//! #293: dragging a note row onto a sidebar folder header, with real pointer
//! events. The release goes through the Move menu's `move_note_to`, so the
//! locked-note refusal, the SAF-06 confirm and the clash refusal are the very
//! same ones; the Move picker stays the keyboard path.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::lock;
use muninn_core::testing::TempVault;
use muninn_i18n::{Key, t};
use muninn_ui::{Settings, State, on_key, poll, view};
use rune::{Env, Runtime};

const PW: &str = "correct horse battery staple";
const BODY: &str = "# Secret\n\nTOP SECRET BODY\n";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-sidebar-drag-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn runtime(tmp: &TempVault) -> Runtime<State> {
    let config = unique_dir("config");
    let settings = Settings::load_from(&config);
    let state = State::with_settings(
        tmp.path().to_path_buf(),
        unique_dir("state"),
        config,
        settings,
    );
    let mut rt = Runtime::new(state, view, (1600.0, 900.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn frame(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
}

fn settle(rt: &mut Runtime<State>) {
    for _ in 0..30 {
        frame(rt);
    }
}

fn centre(r: [f32; 4]) -> (f32, f32) {
    (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0)
}

/// The centre of an element as last drawn, after the list has settled.
fn at(rt: &mut Runtime<State>, key: &str) -> (f32, f32) {
    settle(rt);
    let r = rt
        .presented(key)
        .unwrap_or_else(|| panic!("missing {key}"))
        .0;
    centre(r)
}

/// Press at `from`, move to `to` in steps and hold there.
fn hold(rt: &mut Runtime<State>, from: (f32, f32), to: (f32, f32)) {
    rt.pointer_move(from.0, from.1);
    rt.pointer_down(from.0, from.1);
    frame(rt);
    for step in 1..=8 {
        let f = step as f32 / 8.0;
        rt.pointer_move(from.0 + (to.0 - from.0) * f, from.1 + (to.1 - from.1) * f);
        frame(rt);
    }
    settle(rt);
    // Moving the last bit again reads what the frames above laid out.
    rt.pointer_move(to.0, to.1 + 0.5);
    rt.pointer_move(to.0, to.1);
    frame(rt);
}

fn drop_at(rt: &mut Runtime<State>, to: (f32, f32)) {
    rt.pointer_up(to.0, to.1);
    settle(rt);
}

fn drag(rt: &mut Runtime<State>, from: (f32, f32), to: (f32, f32)) {
    hold(rt, from, to);
    drop_at(rt, to);
}

fn envelope(note: &str) -> String {
    lock(BODY, PW, note).expect("lock")
}

#[test]
fn a_row_drag_moves_the_note_into_the_hovered_folder() {
    let tmp = TempVault::new(&[
        ("Home.md", "# Home\n"),
        ("Alpha.md", "# Alpha\nbody\n"),
        ("dest/keep.md", "# Keep\n"),
    ]);
    let mut rt = runtime(&tmp);
    let id = rt.state.vault.resolve("Alpha").expect("Alpha");
    let from = at(&mut rt, &format!("note-row-{id}"));
    let to = at(&mut rt, "folder-header-dest");
    drag(&mut rt, from, to);

    assert!(tmp.path().join("dest/Alpha.md").is_file());
    assert!(!tmp.path().join("Alpha.md").exists());
    assert_eq!(
        rt.state.vault.resolve("dest/Alpha"),
        Some(id),
        "the real note moved, not a row index"
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join("dest/Alpha.md")).unwrap(),
        "# Alpha\nbody\n"
    );
}

#[test]
fn dropping_on_the_vault_root_header_moves_the_note_to_the_root() {
    let tmp = TempVault::new(&[
        ("Home.md", "# Home\n"),
        ("dest/Alpha.md", "# Alpha\n"),
        ("dest/keep.md", "# Keep\n"),
    ]);
    let mut rt = runtime(&tmp);
    let id = rt.state.vault.resolve("dest/Alpha").expect("dest/Alpha");
    let from = at(&mut rt, &format!("note-row-{id}"));
    let to = at(&mut rt, "folder-header-root");
    drag(&mut rt, from, to);

    assert!(tmp.path().join("Alpha.md").is_file());
    assert!(!tmp.path().join("dest/Alpha.md").exists());
    assert_eq!(
        rt.state.vault.notes[id].folder(),
        "",
        "the empty-folder header moves the note to the vault root"
    );
}

#[test]
fn a_drag_with_inbound_links_asks_and_rewrites_like_the_move_menu() {
    let tmp = TempVault::new(&[
        ("Home.md", "See [[Alpha]]"),
        ("Alpha.md", "body"),
        ("dest/keep.md", "keep"),
    ]);
    let mut rt = runtime(&tmp);
    let id = rt.state.vault.resolve("Alpha").expect("Alpha");
    let from = at(&mut rt, &format!("note-row-{id}"));
    let to = at(&mut rt, "folder-header-dest");
    drag(&mut rt, from, to);

    let confirm = rt.state.move_confirm.as_ref().expect("move dialog");
    assert_eq!(confirm.referrers, 1);
    assert_eq!(confirm.folder, "dest");
    assert!(
        tmp.path().join("Alpha.md").exists(),
        "nothing moves before the choice"
    );
    rt.state.confirm_move_note();
    frame(&mut rt);

    assert!(tmp.path().join("dest/Alpha.md").is_file());
    assert!(!tmp.path().join("Alpha.md").exists());
    assert_eq!(
        fs::read_to_string(tmp.path().join("Home.md")).unwrap(),
        "See [[dest/Alpha]]"
    );
    assert!(rt.state.vault.broken_links().is_empty());
}

#[test]
fn a_clash_refuses_the_drop_with_the_move_error() {
    let tmp = TempVault::new(&[
        ("Alpha.md", "# Alpha\n"),
        ("dest/Alpha.md", "# Other Alpha\n"),
    ]);
    let mut rt = runtime(&tmp);
    let id = rt.state.vault.resolve("Alpha").expect("Alpha");
    let from = at(&mut rt, &format!("note-row-{id}"));
    let to = at(&mut rt, "folder-header-dest");
    drag(&mut rt, from, to);

    let wanted = muninn_i18n::could_not_move_note(muninn_i18n::already_exists(
        tmp.path().join("dest").join("Alpha.md").display(),
    ));
    assert_eq!(rt.state.error.as_deref(), Some(wanted.as_str()));
    assert!(
        rt.state.move_confirm.is_none(),
        "no confirm for a refused move"
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join("Alpha.md")).unwrap(),
        "# Alpha\n"
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join("dest/Alpha.md")).unwrap(),
        "# Other Alpha\n",
        "the existing note is untouched"
    );
}

#[test]
fn dropping_outside_a_folder_header_cancels() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n"), ("dest/keep.md", "# Keep\n")]);
    let mut rt = runtime(&tmp);
    let id = rt.state.vault.resolve("Alpha").expect("Alpha");
    let from = at(&mut rt, &format!("note-row-{id}"));
    // The main column, well outside the sidebar's folder headers.
    let to = (900.0, 300.0);
    drag(&mut rt, from, to);

    assert!(tmp.path().join("Alpha.md").is_file());
    assert!(!tmp.path().join("dest/Alpha.md").exists());
    assert_eq!(rt.state.error, None);
    assert!(rt.state.move_confirm.is_none());
}

#[test]
fn a_locked_note_cannot_be_dragged_into_a_move() {
    let secret = envelope("Secret");
    let tmp = TempVault::new(&[("Secret.md", secret.as_str()), ("dest/keep.md", "# Keep\n")]);
    let mut rt = runtime(&tmp);
    let id = rt.state.vault.resolve("Secret").expect("Secret");
    let from = at(&mut rt, &format!("note-row-{id}"));
    let to = at(&mut rt, "folder-header-dest");
    drag(&mut rt, from, to);

    assert_eq!(
        rt.state.error.as_deref(),
        Some(t(Key::LockRefusedRename)),
        "the same #134 refusal as the Move menu"
    );
    assert_eq!(rt.state.move_picker, None);
    assert!(rt.state.move_confirm.is_none());
    assert_eq!(
        fs::read_to_string(tmp.path().join("Secret.md")).unwrap(),
        secret,
        "the file is byte-for-byte unchanged"
    );
    assert!(!tmp.path().join("dest/Secret.md").exists());
}

#[test]
fn the_row_still_clicks_opens_and_keeps_its_keyboard_focus() {
    let tmp = TempVault::new(&[
        ("Alpha.md", "# Alpha\nbody\n"),
        ("dest/keep.md", "# Keep\n"),
    ]);
    let mut rt = runtime(&tmp);
    let id = rt.state.vault.resolve("Alpha").expect("Alpha");
    let key = format!("note-row-{id}");
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.key.as_deref() == Some(&key) && e.focusable),
        "adding on_drag did not take the row out of the focus group"
    );

    let (x, y) = at(&mut rt, &key);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frame(&mut rt);
    assert_eq!(
        rt.state.selected,
        Some(id),
        "a short press is still a click"
    );
    assert_eq!(
        rt.state.selected_path,
        Some(tmp.path().join("Alpha.md")),
        "and it opens that note"
    );
    assert!(tmp.path().join("Alpha.md").exists(), "no drag was started");
}

#[test]
fn a_row_in_a_search_filtered_list_drags_the_real_note_id() {
    let tmp = TempVault::new(&[
        ("Alpha.md", "# Alpha\n"),
        ("dest/Alpha two.md", "# Alpha two\n"),
        ("other/keep.md", "# Keep\n"),
    ]);
    let mut rt = runtime(&tmp);
    let id = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.update(|s, _| s.search.replace_text("Alpha"));
    settle(&mut rt);
    // Both matching folders are drawn, so the target header belongs to the
    // filtered list rather than to a rect left over from the full one.
    let from = at(&mut rt, &format!("note-row-{id}"));
    let to = at(&mut rt, "folder-header-dest");
    drag(&mut rt, from, to);

    assert!(tmp.path().join("dest/Alpha.md").is_file());
    assert!(!tmp.path().join("Alpha.md").exists());
    assert_eq!(
        fs::read_to_string(tmp.path().join("dest/Alpha.md")).unwrap(),
        "# Alpha\n"
    );
}
