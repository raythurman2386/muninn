//! SAF-02 conflict tests: an external edit must block autosave and every
//! resolution must leave disk and editor in a predictable state.

use std::fs;
use std::path::PathBuf;

use muninn_core::testing::TempVault;
use muninn_ui::{ConflictReason, State, poll, view};
use rune::{Env, Runtime};

fn temp_vault() -> TempVault {
    TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]].\n"),
        ("Beta.md", "# Beta\n\nBack to [[Alpha]].\n"),
    ])
}

fn runtime(tmp: &TempVault) -> Runtime<State> {
    let mut rt = Runtime::new(
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state")),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.frame(0.0);
    rt
}

fn open_alpha(rt: &mut Runtime<State>) -> PathBuf {
    let id = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.load_note(id, false);
    rt.state.vault.notes[id].path.clone()
}

const MINE: &str = "# Alpha\n\nMy in-progress edit.\n";
const THEIRS: &str = "# Alpha\n\nChanged by another writer, with more text.\n";

fn make_conflict(rt: &mut Runtime<State>, path: &PathBuf) {
    fs::write(path, THEIRS).expect("external write");
    rt.state.focus_editor();
    rt.state.editor.replace_text(MINE);
    assert!(
        rt.state.save_now(),
        "the refused save surfaces the conflict"
    );
    assert!(rt.state.conflict.is_some());
}

#[test]
fn external_edit_blocks_autosave_and_pauses_polling() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let path = open_alpha(&mut rt);

    fs::write(&path, THEIRS).expect("external write");
    rt.state.focus_editor();
    rt.state.editor.replace_text(MINE);
    rt.frame(0.1);
    rt.frame(0.8);

    assert_eq!(fs::read_to_string(&path).expect("disk"), THEIRS);
    let conflict = rt.state.conflict.as_ref().expect("typed conflict");
    assert_eq!(conflict.reason, ConflictReason::Modified);
    assert_eq!(conflict.message(), "External change detected");
    assert!(rt.state.dirty(), "the buffer must survive the conflict");
    assert_eq!(rt.state.editor.text, MINE);
    assert!(rt.find("conflict-keep-mine").is_some());
    assert!(rt.find("conflict-load-theirs").is_some());
    assert!(rt.find("conflict-save-copy").is_some());

    // Once pending, autosave stays quiet: no repeated write, no journal spam.
    for step in 0..4 {
        let quiet = rt.frame(2.0 + f64::from(step));
        assert!(
            !quiet,
            "poll must not report a change while conflict pending"
        );
    }
    assert_eq!(fs::read_to_string(&path).expect("disk"), THEIRS);
}

#[test]
fn keep_mine_overwrites_disk_and_clears_conflict() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let path = open_alpha(&mut rt);
    make_conflict(&mut rt, &path);

    assert!(rt.state.resolve_conflict_keep_mine());
    assert_eq!(fs::read_to_string(&path).expect("disk"), MINE);
    assert!(rt.state.conflict.is_none());
    assert!(!rt.state.dirty());
    assert!(!rt.state.save_now(), "a second save is already clean");
}

#[test]
fn load_theirs_discards_the_buffer() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let path = open_alpha(&mut rt);
    make_conflict(&mut rt, &path);

    assert!(rt.state.resolve_conflict_load_theirs());
    assert_eq!(fs::read_to_string(&path).expect("disk"), THEIRS);
    assert_eq!(rt.state.editor.text, THEIRS);
    assert_eq!(rt.state.saved, THEIRS);
    assert!(!rt.state.dirty());
    assert!(rt.state.conflict.is_none());
}

#[test]
fn save_copy_writes_a_sibling_and_loads_theirs() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let path = open_alpha(&mut rt);
    make_conflict(&mut rt, &path);

    assert!(rt.state.resolve_conflict_save_copy());
    assert_eq!(fs::read_to_string(&path).expect("original"), THEIRS);
    assert_eq!(rt.state.editor.text, THEIRS);
    assert!(rt.state.conflict.is_none());

    let copies: Vec<PathBuf> = fs::read_dir(tmp.path())
        .expect("vault dir")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("Alpha (conflict "))
        })
        .collect();
    assert_eq!(copies.len(), 1, "one conflict copy in the same folder");
    assert_eq!(fs::read_to_string(&copies[0]).expect("copy"), MINE);
}

#[test]
fn deleted_note_becomes_conflict_and_load_theirs_shows_empty() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let path = open_alpha(&mut rt);

    fs::remove_file(&path).expect("external delete");
    rt.state.focus_editor();
    rt.state.editor.replace_text(MINE);
    assert!(rt.state.save_now());

    let conflict = rt.state.conflict.as_ref().expect("typed conflict");
    assert_eq!(conflict.reason, ConflictReason::Deleted);
    assert_eq!(conflict.message(), "file deleted externally");
    assert!(rt.state.dirty(), "the buffer must survive the deletion");

    assert!(rt.state.resolve_conflict_load_theirs());
    assert!(rt.state.conflict.is_none());
    assert!(rt.state.selected.is_none());
    assert!(rt.state.editor.text.is_empty());
    assert!(!path.exists());
}

/// TRU-01: an IO failure stays visible with Retry, and Retry clears the
/// autosave backoff so the next attempt happens immediately.
#[cfg(unix)]
#[test]
fn io_error_is_visible_and_retry_resets_the_backoff() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let path = open_alpha(&mut rt);
    let root = tmp.path().to_path_buf();

    fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).expect("lock vault");
    rt.state.focus_editor();
    rt.state.editor.replace_text(MINE);
    assert!(!rt.state.save_now(), "the write must fail, not panic");
    assert!(rt.state.error.is_some());
    assert!(rt.state.dirty());
    rt.frame(0.0);
    assert!(rt.find("retry-save").is_some(), "failure is actionable");

    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("unlock vault");
    rt.state.dirty_at = Some(9.0);
    assert!(rt.state.retry_save());
    assert!(rt.state.dirty_at.is_none());
    assert!(rt.state.error.is_none());
    assert!(!rt.state.dirty());
    assert_eq!(fs::read_to_string(&path).expect("disk"), MINE);
}
