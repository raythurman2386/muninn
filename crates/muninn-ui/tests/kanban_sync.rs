//! Kanban slice 7: autosave, the watcher, the conflict bar, the recovery
//! copy, the quit save and the Write hand-off, headless.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{
    BoardStatus, DocConflict, Mode, RecoverySlot, Settings, State, on_key, poll, view,
};
use rune::{Env, Runtime};

const BOARD: &str = include_str!("../../muninn-core/tests/fixtures/kanban/edit-board.md");
/// `BOARD` after the checkbox of "Fix the sync bug" (lane 1, card 0).
const TOGGLED: &str = "- [x] Fix the sync bug";
/// Another app's version: one more card in To do.
const THEIRS: &str = "- [ ] Added by another app\n";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-sync-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn vault() -> TempVault {
    TempVault::new(&[("Agenda.md", "# Agenda\n"), ("Board.md", BOARD)])
}

struct Session {
    rt: Runtime<State>,
    config: PathBuf,
    state_home: PathBuf,
}

fn launch_with(root: &Path, config: &Path, state_home: &Path) -> Runtime<State> {
    let state = State::with_settings(
        root.to_path_buf(),
        state_home.to_path_buf(),
        config.to_path_buf(),
        Settings::load_from(config),
    );
    let mut rt = Runtime::new(state, view, (1400.0, 900.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

/// The app on `Board.md`'s Kanban page; `autosave` is the kanban setting.
fn on_board(tmp: &TempVault, autosave: bool) -> Session {
    let (config, state_home) = (unique_dir("config"), unique_dir("state"));
    fs::create_dir_all(config.join("muninn")).unwrap();
    fs::write(
        config.join("muninn/settings.json"),
        format!("{{\"kanban_autosave\": {autosave}}}"),
    )
    .unwrap();
    let mut rt = launch_with(tmp.path(), &config, &state_home);
    open(&mut rt);
    Session {
        rt,
        config,
        state_home,
    }
}

fn open(rt: &mut Runtime<State>) {
    let id = rt.state.vault.resolve("Board").expect("Board");
    rt.update(|s, _| s.open_kanban(id));
    frames(rt, 2);
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

/// Long enough for the watcher (1.5 s) and autosave (1 s) to act.
fn wait(rt: &mut Runtime<State>) {
    frames(rt, 200);
}

fn act(rt: &mut Runtime<State>, f: impl FnOnce(&mut State)) {
    rt.update(|s, _| f(s));
    frames(rt, 2);
}

fn click(rt: &mut Runtime<State>, key: &str) {
    frames(rt, 10);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 2);
}

fn shown(rt: &Runtime<State>) -> String {
    rt.state
        .kanban
        .as_ref()
        .and_then(|k| k.board.as_ref())
        .expect("a board")
        .source()
        .to_owned()
}

fn status(rt: &Runtime<State>) -> BoardStatus {
    rt.state.kanban.as_ref().expect("shown").status.clone()
}

fn conflict(rt: &Runtime<State>) -> Option<DocConflict> {
    rt.state.kanban.as_ref().and_then(|k| k.conflict)
}

fn file(tmp: &TempVault) -> PathBuf {
    tmp.path().join("Board.md")
}

fn read(tmp: &TempVault) -> String {
    fs::read_to_string(file(tmp)).expect("Board.md")
}

fn mine() -> String {
    BOARD.replace("- [ ] Fix the sync bug", TOGGLED)
}

fn theirs() -> String {
    BOARD.replace("\nA paragraph", &format!("{THEIRS}\nA paragraph"))
}

/// Another app's save: a temp file in the same folder renamed over the file.
fn atomic_write(path: &Path, text: &str) {
    let temp = path.with_file_name(".Board.md.other-app.tmp");
    fs::write(&temp, text).unwrap();
    fs::rename(&temp, path).unwrap();
}

#[test]
fn autosave_writes_rested_edits_and_off_keeps_the_explicit_save() {
    let tmp = vault();
    let mut s = on_board(&tmp, true);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    assert_eq!(read(&tmp), BOARD, "not before the edits rest");
    wait(&mut s.rt);
    assert_eq!(read(&tmp), mine(), "autosaved");
    assert_eq!(status(&s.rt), BoardStatus::Saved);

    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    wait(&mut s.rt);
    wait(&mut s.rt);
    assert_eq!(read(&tmp), BOARD, "off: nothing until Cmd+S");
    assert!(matches!(status(&s.rt), BoardStatus::Unsaved));
    assert!(s.rt.state.save_kanban());
    assert_eq!(read(&tmp), mine());
}

#[test]
fn autosave_waits_while_a_card_editor_is_open() {
    let tmp = vault();
    let mut s = on_board(&tmp, true);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    act(&mut s.rt, |s| s.begin_add_card(0));
    wait(&mut s.rt);
    assert_eq!(read(&tmp), BOARD, "nothing saved half-way");
    act(&mut s.rt, |s| s.cancel_card_edit());
    wait(&mut s.rt);
    assert_eq!(read(&tmp), mine());
}

#[test]
fn a_clean_kanban_reloads_when_another_app_writes_it_in_place_or_atomically() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.select_kanban_card(1, 0));
    fs::write(file(&tmp), theirs()).unwrap();
    wait(&mut s.rt);
    assert_eq!(shown(&s.rt), theirs(), "reloaded in place");
    assert_eq!(s.rt.state.mode, Mode::Kanban);
    assert_eq!(
        s.rt.state.kanban_selected,
        Some((1, 0)),
        "the selection stays"
    );
    assert_eq!(conflict(&s.rt), None);

    let again = theirs().replace("Added by another app", "Atomic");
    atomic_write(&file(&tmp), &again);
    wait(&mut s.rt);
    assert_eq!(shown(&s.rt), again, "an atomic save is followed");
    assert_eq!(read(&tmp), again);
}

#[test]
fn unsaved_edits_are_flagged_never_reloaded_and_each_choice_gives_its_bytes() {
    for choice in ["kanban-keep-mine", "kanban-load-theirs", "kanban-save-copy"] {
        let tmp = vault();
        let mut s = on_board(&tmp, false);
        act(&mut s.rt, |s| s.toggle_card_done(1, 0));
        atomic_write(&file(&tmp), &theirs());
        wait(&mut s.rt);
        assert_eq!(conflict(&s.rt), Some(DocConflict::Changed), "{choice}");
        assert_eq!(shown(&s.rt), mine(), "{choice}: my edits are kept");
        assert!(!s.rt.state.save_kanban(), "{choice}: Cmd+S is refused");
        assert_eq!(read(&tmp), theirs(), "{choice}: their file untouched");
        click(&mut s.rt, choice);
        assert_eq!(conflict(&s.rt), None, "{choice}");
        match choice {
            "kanban-keep-mine" => {
                assert_eq!(read(&tmp), mine());
                assert_eq!(status(&s.rt), BoardStatus::Saved);
            }
            "kanban-load-theirs" => {
                assert_eq!(read(&tmp), theirs());
                assert_eq!(shown(&s.rt), theirs());
                assert!(!s.rt.state.kanban_dirty());
            }
            _ => {
                assert_eq!(read(&tmp), theirs(), "the original keeps theirs");
                let rel = s.rt.state.kanban.as_ref().unwrap().rel.clone();
                assert!(
                    rel.starts_with("Board (conflict ") && rel.ends_with(").md"),
                    "{rel}"
                );
                assert_eq!(fs::read_to_string(tmp.path().join(&rel)).unwrap(), mine());
                assert!(
                    s.rt.state
                        .vault
                        .resolve(rel.trim_end_matches(".md"))
                        .is_some()
                );
            }
        }
    }
}

#[test]
fn a_text_typed_in_an_open_card_editor_counts_as_unsaved() {
    let tmp = vault();
    let mut s = on_board(&tmp, true);
    act(&mut s.rt, |s| s.begin_add_card(0));
    s.rt.ime(rune::Ime::Commit("Typed".to_owned()));
    frames(&mut s.rt, 2);
    fs::write(file(&tmp), theirs()).unwrap();
    wait(&mut s.rt);
    assert_eq!(conflict(&s.rt), Some(DocConflict::Changed), "flagged");
    assert_eq!(
        s.rt.state.kanban_card_field.text, "Typed",
        "the editor keeps it"
    );
    assert_eq!(read(&tmp), theirs());
}

#[test]
fn a_dirty_kanban_whose_file_is_deleted_offers_save_copy_and_quits_with_a_copy() {
    let tmp = vault();
    let mut s = on_board(&tmp, true);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    fs::remove_file(file(&tmp)).unwrap();
    wait(&mut s.rt);
    assert_eq!(conflict(&s.rt), Some(DocConflict::Missing));
    assert_eq!(shown(&s.rt), mine(), "kept on the page");
    assert!(!file(&tmp).exists(), "autosave never recreates it unasked");
    // Quitting is not blocked: the recovery copy holds the edits.
    assert!(s.rt.state.flush_before_quit());
    assert!(s.rt.state.can_quit());
    let copy = s.rt.state.kanban_recovery_path("Board.md");
    assert_eq!(fs::read_to_string(&copy).unwrap(), mine());
    click(&mut s.rt, "kanban-save-copy");
    let rel = s.rt.state.kanban.as_ref().unwrap().rel.clone();
    assert_eq!(fs::read_to_string(tmp.path().join(&rel)).unwrap(), mine());
    assert!(!file(&tmp).exists());
    frames(&mut s.rt, 2);
    assert!(
        !copy.exists(),
        "the copy is saved, so the recovery copy goes"
    );
}

#[test]
fn discard_drops_the_edits_of_a_deleted_kanban() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    fs::remove_file(file(&tmp)).unwrap();
    wait(&mut s.rt);
    click(&mut s.rt, "kanban-load-theirs");
    assert!(s.rt.state.kanban.is_none(), "nothing left to show");
    assert!(!file(&tmp).exists());
    assert!(!s.rt.state.kanban_recovery_path("Board.md").exists());
}

#[test]
fn a_kanban_renamed_outside_muninn_is_followed_with_its_edits() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    fs::rename(file(&tmp), tmp.path().join("Renamed.md")).unwrap();
    wait(&mut s.rt);
    let shown_rel = s.rt.state.kanban.as_ref().unwrap().rel.clone();
    assert_eq!(shown_rel, "Renamed.md");
    assert_eq!(conflict(&s.rt), None);
    assert_eq!(shown(&s.rt), mine());
    assert!(s.rt.state.save_kanban());
    assert_eq!(
        fs::read_to_string(tmp.path().join("Renamed.md")).unwrap(),
        mine()
    );
    assert!(!file(&tmp).exists());
}

#[test]
fn a_recovery_copy_comes_back_on_the_next_launch_flagged_and_keep_mine_writes_it() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    frames(&mut s.rt, 60);
    let copy = s.rt.state.kanban_recovery_path("Board.md");
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        mine(),
        "copied once rested"
    );
    drop(s.rt); // a crash: no quit flush

    let mut rt = launch_with(tmp.path(), &s.config, &s.state_home);
    frames(&mut rt, 2);
    let i = rt
        .state
        .recovered
        .iter()
        .position(|r| r.rel == "Board.md")
        .expect("listed");
    click(&mut rt, &format!("recovery-restore-{i}"));
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(shown(&rt), mine(), "the edits are back");
    assert_eq!(conflict(&rt), Some(DocConflict::Changed));
    wait(&mut rt);
    assert_eq!(read(&tmp), BOARD, "never written over unasked");
    assert!(!rt.state.save_kanban(), "a plain save is refused");
    click(&mut rt, "kanban-keep-mine");
    assert_eq!(read(&tmp), mine());
    frames(&mut rt, 2);
    assert!(!copy.exists(), "saved, so the copy goes");
}

#[test]
fn quitting_is_refused_when_the_kanban_can_neither_be_saved_nor_copied() {
    let tmp = vault();
    let config = unique_dir("config");
    let state_home = unique_dir("state").join("not-a-dir");
    fs::write(&state_home, "x").unwrap();
    fs::create_dir_all(config.join("muninn")).unwrap();
    fs::write(
        config.join("muninn/settings.json"),
        r#"{"kanban_autosave": false}"#,
    )
    .unwrap();
    let mut rt = launch_with(tmp.path(), &config, &state_home);
    open(&mut rt);
    act(&mut rt, |s| s.toggle_card_done(1, 0));
    fs::write(file(&tmp), theirs()).unwrap();
    let mut quit = true;
    act(&mut rt, |s| quit = s.flush_before_quit());
    let error = rt.state.error.clone().unwrap_or_default();
    assert!(
        error.contains("Save copy"),
        "the refusal offers Save copy: {error:?}"
    );
    assert!(!quit && !rt.state.can_quit());
    frames(&mut rt, 2);
    click(&mut rt, "kanban-save-copy");
    assert!(
        rt.state.flush_before_quit(),
        "after Save copy the quit goes ahead"
    );
}

#[test]
fn open_as_markdown_saves_first_and_stays_when_that_is_refused() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    click(&mut s.rt, "kanban-open-markdown");
    assert_eq!(s.rt.state.mode, Mode::Editor);
    assert_eq!(read(&tmp), mine(), "saved before Write shows it");
    assert_eq!(s.rt.state.editor.text, mine());

    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    fs::write(file(&tmp), theirs()).unwrap();
    click(&mut s.rt, "kanban-open-markdown");
    assert_eq!(s.rt.state.mode, Mode::Kanban, "refused: the page stays");
    assert_eq!(conflict(&s.rt), Some(DocConflict::Changed));
    assert_eq!(read(&tmp), theirs());
}

#[test]
fn text_typed_in_write_is_handed_to_the_kanban() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    click(&mut s.rt, "kanban-open-markdown");
    // Typed in Write and saved: the page reads it.
    let typed = BOARD.replace("Write the release notes", "Write the notes");
    act(&mut s.rt, |s| s.editor.replace_text(typed.clone()));
    act(&mut s.rt, |s| s.set_mode(Mode::Kanban));
    assert_eq!(shown(&s.rt), typed);
    assert_eq!(read(&tmp), typed);

    // Typed in Write while another app changed the file, so Write can't
    // save it: the page takes the text as unsaved edits and flags the file.
    act(&mut s.rt, |s| s.open_kanban_as_markdown());
    let typed2 = typed.replace("Write the notes", "Write the docs");
    act(&mut s.rt, |s| s.editor.replace_text(typed2.clone()));
    fs::write(file(&tmp), theirs()).unwrap();
    act(&mut s.rt, |s| s.set_mode(Mode::Kanban));
    assert_eq!(shown(&s.rt), typed2, "nothing typed is dropped");
    assert!(s.rt.state.kanban_dirty());
    assert!(!s.rt.state.save_kanban(), "their file is not written over");
    assert_eq!(conflict(&s.rt), Some(DocConflict::Changed));
    assert_ne!(s.rt.state.editor.text, typed2, "Write gave the text up");
    assert_eq!(read(&tmp), theirs());
}

/// P1-1: the watcher compares `(mtime, len)`, but a save compares the bytes:
/// another app's same-length edit with the old modified time is not written
/// over, and the page flags it.
#[test]
fn a_same_length_change_that_keeps_the_mtime_is_not_written_over() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    let path = file(&tmp);
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    let theirs = BOARD.replace("Fix the sync bug", "Fix the sync BUG");
    assert_eq!(theirs.len(), BOARD.len(), "same length");
    assert_ne!(theirs, BOARD);
    fs::write(&path, &theirs).unwrap();
    let handle = fs::File::options().write(true).open(&path).unwrap();
    handle
        .set_times(fs::FileTimes::new().set_modified(before))
        .unwrap();
    drop(handle);
    // Their edit is invisible to the watcher: the page still holds what it read.
    wait(&mut s.rt);
    assert_eq!(shown(&s.rt), BOARD);
    assert_eq!(conflict(&s.rt), None, "the watcher cannot see it");

    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    let mut saved = true;
    act(&mut s.rt, |s| saved = s.save_kanban());
    assert!(!saved, "the save is refused");
    assert_eq!(read(&tmp), theirs, "their bytes survive");
    assert_eq!(
        conflict(&s.rt),
        Some(DocConflict::Changed),
        "the page flags the change"
    );
    assert_eq!(shown(&s.rt), mine(), "my edits stay on the page");
}

/// P1-2: text the model refuses in a lane field must not stop the committed
/// edits' recovery copy, nor leave the persistence error on the page.
#[test]
fn a_refused_lane_field_does_not_stop_the_committed_edits_recovery_copy() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    act(&mut s.rt, |s| s.begin_lane_limit(1));
    act(&mut s.rt, |s| s.kanban_lane_field.replace_text("abc"));
    frames(&mut s.rt, 90);
    let copy = s.rt.state.kanban_recovery_path("Board.md");
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        mine(),
        "the copy holds the committed edit"
    );
    assert_eq!(
        s.rt.state.kanban_lane_field.text, "abc",
        "the field keeps it"
    );
    assert!(
        s.rt.state
            .error
            .as_deref()
            .is_none_or(|e| !e.contains("persist kanban recovery copy")),
        "no persistence error: {:?}",
        s.rt.state.error
    );
    // The refused field still blocks the quit, as slice 4 says.
    assert!(!s.rt.state.flush_before_quit());

    // A clean board with only the refused field has nothing to recover.
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.begin_lane_limit(1));
    act(&mut s.rt, |s| s.kanban_lane_field.replace_text("abc"));
    frames(&mut s.rt, 90);
    assert!(
        !s.rt.state.kanban_recovery_path("Board.md").exists(),
        "no copy for an unchanged board"
    );
    assert!(s.rt.state.error.is_none(), "{:?}", s.rt.state.error);
}

/// P1-3: a restore over a removed file has no saved text, so an unrelated
/// empty note must not be taken for its rename (and autosave must not write
/// the recovered board into it).
#[test]
fn a_restored_kanban_is_not_renamed_onto_an_empty_note() {
    let tmp = TempVault::new(&[
        ("Agenda.md", "# Agenda\n"),
        ("Board.md", BOARD),
        ("Empty.md", ""),
    ]);
    let mut s = on_board(&tmp, true);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    // Rested enough for the recovery copy (0.5 s), not yet autosaved (1 s):
    // the crash leaves both a dirty board and its copy.
    frames(&mut s.rt, 45);
    assert!(
        s.rt.state.kanban_recovery_path("Board.md").exists(),
        "copy written"
    );
    drop(s.rt);

    fs::remove_file(tmp.path().join("Board.md")).unwrap();
    let mut rt = launch_with(tmp.path(), &s.config, &s.state_home);
    frames(&mut rt, 2);
    assert!(
        rt.state.recovered.iter().any(|r| r.rel == "Board.md"),
        "listed"
    );
    rt.update(|s, _| assert!(s.restore_recovery("Board.md")));
    frames(&mut rt, 200);
    assert_eq!(
        rt.state.kanban.as_ref().unwrap().rel,
        "Board.md",
        "the page stays on the missing note"
    );
    assert_eq!(conflict(&rt), Some(DocConflict::Missing));
    assert_eq!(
        fs::read_to_string(tmp.path().join("Empty.md")).unwrap(),
        "",
        "the empty note is untouched"
    );
}

/// P2-4: Save copy makes the copy the selected note, so a later vault reload
/// keeps the page on it instead of reading the original back.
#[test]
fn save_copy_keeps_the_copy_selected_through_a_vault_reload() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    atomic_write(&file(&tmp), &theirs());
    wait(&mut s.rt);
    assert_eq!(conflict(&s.rt), Some(DocConflict::Changed));
    let mut copied = false;
    act(&mut s.rt, |s| copied = s.kanban_save_copy());
    assert!(copied);
    let copy = s.rt.state.kanban.as_ref().unwrap().rel.clone();
    let id =
        s.rt.state
            .vault
            .resolve(copy.trim_end_matches(".md"))
            .expect("the copy is indexed");
    assert_eq!(s.rt.state.selected, Some(id), "the copy is selected");
    assert_eq!(
        s.rt.state.selected_path,
        Some(tmp.path().join(&copy)),
        "and its path"
    );

    // Another file changes: the vault watcher reloads and selects the copy.
    fs::write(tmp.path().join("Agenda.md"), "# Agenda v2\n").unwrap();
    wait(&mut s.rt);
    assert_eq!(s.rt.state.kanban.as_ref().unwrap().rel, copy);
    assert_eq!(s.rt.state.selected, Some(id));
    assert_eq!(shown(&s.rt), mine());
    assert_eq!(fs::read_to_string(tmp.path().join(&copy)).unwrap(), mine());
}

/// P2-5a: saving a shown kanban that is not the selected note still clears
/// its recovery copy, so a later vault switch does not strand it.
#[test]
fn saving_a_shown_kanban_that_is_not_selected_clears_its_recovery_copy() {
    let tmp = vault();
    let other = TempVault::new(&[("B.md", "# B\n")]);
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    frames(&mut s.rt, 60);
    let copy = s.rt.state.kanban_recovery_path("Board.md");
    assert_eq!(fs::read_to_string(&copy).unwrap(), mine());
    // Select a plain note: the page keeps showing the kanban.
    let agenda = s.rt.state.vault.resolve("Agenda").unwrap();
    act(&mut s.rt, |s| s.load_note(agenda, false));
    let board_id =
        s.rt.state
            .vault
            .notes
            .iter()
            .find(|n| n.rel == "Board.md")
            .map(|n| n.id);
    assert_ne!(s.rt.state.selected, board_id, "the kanban is not selected");
    assert_eq!(s.rt.state.kanban.as_ref().unwrap().rel, "Board.md");
    let mut saved = false;
    act(&mut s.rt, |s| saved = s.save_kanban());
    assert!(saved);
    assert_eq!(read(&tmp), mine());
    assert!(!copy.exists(), "the saved kanban's recovery copy goes");
    assert!(s.rt.state.recovered.iter().all(|r| r.rel != "Board.md"));

    let mut switched = false;
    act(&mut s.rt, |s| {
        switched = s.switch_vault(other.path().to_path_buf())
    });
    assert!(switched);
    assert!(
        !copy.exists(),
        "nothing is left to offer on the next launch"
    );
}

/// MF1: Write's save of the same note must not clear the board's unfinalized
/// copy, or a crash (or a quit the refused save lets through) loses the card
/// edit.
#[test]
fn a_write_save_does_not_strand_a_dirty_kanbans_recovery_copy() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    frames(&mut s.rt, 60);
    let copy = s.rt.state.kanban_recovery_path("Board.md");
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        mine(),
        "the card edit is copied first"
    );

    // Cmd+1 to Write on the same note, type a line, and save it.
    let typed = format!("{BOARD}\nWritten in Write.\n");
    act(&mut s.rt, |s| s.set_mode(Mode::Editor));
    s.rt.state.editor.replace_text(typed.clone());
    let mut saved = false;
    act(&mut s.rt, |s| saved = s.save_now());
    assert!(saved);
    assert_eq!(read(&tmp), typed, "Write's save reached the file");

    // The crash check: the card edit is still in the shared copy at once,
    // even though Write cleared it (the board is still unsaved).
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        mine(),
        "Write's save rewrites the board's unfinalized copy"
    );

    // Quit: the kanban save is refused (Write changed the file); the quit
    // must keep the copy rather than trust the stale recovery record.
    let mut quit = false;
    act(&mut s.rt, |s| quit = s.flush_before_quit());
    assert!(quit, "the copy holds the edit, so the quit goes ahead");
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        mine(),
        "the quit leaves the card edit in the recovery copy"
    );
    assert_eq!(read(&tmp), typed, "the file holds Write's save, not a mix");
    assert!(
        !s.rt.state.recovered.iter().any(|r| r.rel == "Board.md"),
        "a live copy is not also offered as a crash leftover"
    );
}

/// MF1b: a crash before Write saves still loses the card edit when both
/// buffers share one recovery file. Write's recovery autosave overwrites
/// that file, and the Kanban page does not rewrite it. The same window stays
/// open for as long as a conflict refuses Write's save. Both texts have to
/// come back.
#[test]
fn a_crash_before_write_saves_keeps_the_card_edit_and_writes_text() {
    // Inside the autosave delay: recovery has copied Write's text, the file
    // has not been saved.
    crash_with_unsaved_write(false);
    // A conflict refuses the save, so the delay never ends.
    crash_with_unsaved_write(true);
}

fn crash_with_unsaved_write(conflict: bool) {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    frames(&mut s.rt, 60);
    assert_eq!(
        fs::read_to_string(s.rt.state.kanban_recovery_path("Board.md")).unwrap(),
        mine(),
        "the card edit is copied first"
    );

    let typed = "Written in Write, and not a kanban.\n";
    act(&mut s.rt, |s| s.set_mode(Mode::Editor));
    s.rt.state.editor.replace_text(typed);
    if conflict {
        atomic_write(&file(&tmp), &theirs());
        let mut raised = false;
        act(&mut s.rt, |s| raised = s.save_now());
        assert!(raised, "the refused save surfaces the conflict");
        assert!(s.rt.state.conflict.is_some(), "autosave stays paused");
        frames(&mut s.rt, 90);
        assert_eq!(read(&tmp), theirs(), "the refused save wrote nothing");
    } else {
        // Recovery debounce is 0.2s; note autosave is 0.55s.
        frames(&mut s.rt, 18);
        assert_eq!(read(&tmp), BOARD, "Write has not saved yet");
    }
    assert_eq!(
        fs::read_to_string(s.rt.state.recovery_path("Board.md")).unwrap(),
        typed,
        "Write's text has its own copy"
    );
    assert_eq!(
        fs::read_to_string(s.rt.state.kanban_recovery_path("Board.md")).unwrap(),
        mine(),
        "Write's copy did not replace the card edit"
    );

    let (config, state_home) = (s.config.clone(), s.state_home.clone());
    drop(s.rt);

    let mut rt = launch_with(tmp.path(), &config, &state_home);
    frames(&mut rt, 2);
    let kanban_i = rt
        .state
        .recovered
        .iter()
        .position(|r| r.rel == "Board.md" && r.slot == RecoverySlot::Kanban)
        .expect("the card edit is offered");
    assert!(
        rt.state
            .recovered
            .iter()
            .any(|r| { r.rel == "Board.md" && r.slot == RecoverySlot::Editor }),
        "Write's text is offered too"
    );
    click(&mut rt, &format!("recovery-restore-{kanban_i}"));
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(shown(&rt), mine(), "relaunch restores the card edit");
    assert_eq!(
        fs::read_to_string(rt.state.recovery_path("Board.md")).unwrap(),
        typed,
        "restoring the card edit leaves Write's text"
    );
    assert!(
        rt.state
            .recovered
            .iter()
            .any(|r| r.rel == "Board.md" && r.slot == RecoverySlot::Editor),
        "Write's row stays on the banner"
    );
    rt.update(|s, _| {
        assert!(s.restore_recovery_slot("Board.md", RecoverySlot::Editor));
    });
    assert_eq!(rt.state.editor.text, typed, "Write's text restores as well");
}

/// Write's unsaved text for a kanban note is usually still a kanban. Restoring
/// that row (the first one when both copies share a timestamp), Restore all,
/// or either order must leave the card edit on disk and on the page once it
/// is restored, and must leave Write's text too.
#[test]
fn restoring_a_kanban_shaped_write_copy_keeps_the_card_edit() {
    let written = BOARD.replace("- [ ] Tag the release", "- [x] Tag the release");
    assert_ne!(written, mine());
    assert!(muninn_core::is_kanban(&written));

    // Row 0 is Write. The card-edit file stays, and restoring it afterwards
    // still shows the card edit.
    let (tmp, config, state_home) = crash_with_kanban_shaped_write(&written);
    let mut rt = launch_with(tmp.path(), &config, &state_home);
    frames(&mut rt, 2);
    assert_eq!(rt.state.recovered[0].slot, RecoverySlot::Editor, "row 0");
    assert_eq!(rt.state.recovered[1].slot, RecoverySlot::Kanban);
    assert_eq!(
        text_of(&rt, "recovery-label-0").as_deref(),
        Some("Board.md — Write edits")
    );
    assert_eq!(
        text_of(&rt, "recovery-label-1").as_deref(),
        Some("Board.md — Kanban edits")
    );
    click(&mut rt, "recovery-restore-0");
    assert_eq!(rt.state.editor.text, written, "row 0 restores Write");
    assert_eq!(
        fs::read_to_string(rt.state.kanban_recovery_path("Board.md")).unwrap(),
        mine(),
        "row 0 does not replace the card edit"
    );
    assert_eq!(
        fs::read_to_string(rt.state.recovery_path("Board.md")).unwrap(),
        written
    );
    click(&mut rt, "recovery-restore-0");
    assert_eq!(shown(&rt), mine(), "the card edit still restores");
    assert_eq!(rt.state.editor.text, written);

    // Restore all brings the card edit back first and keeps Write's text.
    let (tmp, config, state_home) = crash_with_kanban_shaped_write(&written);
    let mut rt = launch_with(tmp.path(), &config, &state_home);
    frames(&mut rt, 2);
    click(&mut rt, "recovery-restore-all");
    assert_eq!(shown(&rt), mine(), "Restore all shows the card edit");
    assert_eq!(rt.state.editor.text, written, "Restore all keeps Write");
    assert_eq!(
        fs::read_to_string(rt.state.kanban_recovery_path("Board.md")).unwrap(),
        mine()
    );
    assert_eq!(
        fs::read_to_string(rt.state.recovery_path("Board.md")).unwrap(),
        written
    );

    // Each order. The card edit ends on the page; neither file is replaced.
    for order in [
        [RecoverySlot::Editor, RecoverySlot::Kanban],
        [RecoverySlot::Kanban, RecoverySlot::Editor],
    ] {
        let (tmp, config, state_home) = crash_with_kanban_shaped_write(&written);
        let mut rt = launch_with(tmp.path(), &config, &state_home);
        frames(&mut rt, 2);
        rt.update(|s, _| {
            for slot in order {
                assert!(s.restore_recovery_slot("Board.md", slot));
            }
        });
        assert_eq!(shown(&rt), mine(), "card edit after {order:?}");
        assert_eq!(
            rt.state.editor.text, written,
            "Write's text after {order:?}"
        );
        assert_eq!(
            fs::read_to_string(rt.state.kanban_recovery_path("Board.md")).unwrap(),
            mine(),
            "card-edit file after {order:?}"
        );
        // Opening the Kanban page flushes a restored Write buffer into the
        // note and then drops that recovery file. The text is still here.
        let write_file = fs::read_to_string(rt.state.recovery_path("Board.md")).ok();
        assert!(
            write_file.as_deref() == Some(written.as_str()) || read(&tmp) == written,
            "Write's text was dropped after {order:?}"
        );
    }
}

/// A dirty kanban and a kanban-shaped Write buffer, both copied, then a crash.
/// The two sidecars share a timestamp so the banner lists Write first.
fn crash_with_kanban_shaped_write(written: &str) -> (TempVault, PathBuf, PathBuf) {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    frames(&mut s.rt, 60);
    act(&mut s.rt, |s| s.set_mode(Mode::Editor));
    s.rt.state.editor.replace_text(written);
    frames(&mut s.rt, 18);
    assert_eq!(read(&tmp), BOARD, "Write has not saved yet");
    assert_eq!(
        fs::read_to_string(s.rt.state.kanban_recovery_path("Board.md")).unwrap(),
        mine()
    );
    assert_eq!(
        fs::read_to_string(s.rt.state.recovery_path("Board.md")).unwrap(),
        written
    );
    let dir = s.state_home.join("muninn/recovery");
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let text = text
            .split("\"ts\":")
            .enumerate()
            .map(|(i, part)| {
                if i == 0 {
                    part.to_owned()
                } else {
                    let rest = part.trim_start_matches(|c: char| c.is_ascii_digit());
                    format!("\"ts\":1{rest}")
                }
            })
            .collect::<String>();
        fs::write(&path, text).unwrap();
    }
    let (config, state_home) = (s.config.clone(), s.state_home.clone());
    drop(s.rt);
    (tmp, config, state_home)
}

fn text_of(rt: &Runtime<State>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some(key) && !el.exiting)
        .and_then(|el| el.text)
}

/// P1: Save copy on the Kanban page must not drop Write's dirty buffer for
/// another note, nor leave the editor's conflict bar naming that note: a
/// later Keep mine would write the kanban's text into it.
#[test]
fn save_copy_keeps_writes_dirty_buffer_and_its_conflict_on_its_own_note() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    // A dirty board whose file changed under it: the page offers Save copy.
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    atomic_write(&file(&tmp), &theirs());
    wait(&mut s.rt);
    assert_eq!(conflict(&s.rt), Some(DocConflict::Changed));

    // Write on Agenda, dirty with a conflict of its own. The page keeps
    // showing Board (a canvas shows a note that is not the selected one).
    let agenda = s.rt.state.vault.resolve("Agenda").expect("Agenda");
    act(&mut s.rt, |s| s.load_note(agenda, false));
    let agenda_path = tmp.path().join("Agenda.md");
    fs::write(&agenda_path, "# Agenda changed elsewhere\n").unwrap();
    s.rt.state.editor.replace_text("# Agenda, mine\n");
    let mut raised = false;
    act(&mut s.rt, |s| raised = s.save_now());
    assert!(raised, "the refused save surfaces Write's conflict");
    assert_eq!(
        s.rt.state.conflict.as_ref().expect("a conflict").note_id,
        agenda
    );
    frames(&mut s.rt, 60);
    let agenda_copy = s.rt.state.recovery_path("Agenda.md");
    assert_eq!(
        fs::read_to_string(&agenda_copy).unwrap(),
        "# Agenda, mine\n",
        "Write's buffer is copied before Save copy"
    );
    assert_eq!(s.rt.state.kanban.as_ref().unwrap().rel, "Board.md");

    let mut copied = false;
    act(&mut s.rt, |s| copied = s.kanban_save_copy());
    assert!(copied);
    let copy = s.rt.state.kanban.as_ref().unwrap().rel.clone();
    let copy_id =
        s.rt.state
            .vault
            .resolve(copy.trim_end_matches(".md"))
            .unwrap();
    assert_eq!(s.rt.state.kanban.as_ref().unwrap().rel, copy);
    assert_eq!(s.rt.state.selected, Some(copy_id), "the page and the note");
    assert_eq!(
        s.rt.state.selected_path,
        Some(tmp.path().join(&copy)),
        "agree on the copy"
    );
    assert_eq!(
        s.rt.state.editor.text,
        fs::read_to_string(tmp.path().join(&copy)).unwrap(),
        "the editor shows the copy, clean"
    );
    assert!(
        s.rt.state.conflict.is_none(),
        "no conflict is left pointing at another note"
    );
    assert_eq!(
        fs::read_to_string(&agenda_copy).unwrap(),
        "# Agenda, mine\n",
        "Write's dirty buffer is not dropped"
    );
    assert_eq!(
        fs::read_to_string(&agenda_path).unwrap(),
        "# Agenda changed elsewhere\n",
        "the original is untouched"
    );

    // Keep mine on the editor's own note (the copy) writes there, never Agenda.
    let copy_path = tmp.path().join(&copy);
    fs::write(&copy_path, "their copy\n").unwrap();
    s.rt.state.editor.replace_text("# edited copy\n");
    let mut raised = false;
    act(&mut s.rt, |s| raised = s.save_now());
    assert!(raised);
    assert_eq!(
        s.rt.state.conflict.as_ref().expect("a conflict").note_id,
        copy_id
    );
    act(&mut s.rt, |s| assert!(s.resolve_conflict_keep_mine()));
    assert_eq!(fs::read_to_string(&copy_path).unwrap(), "# edited copy\n");
    assert_eq!(
        fs::read_to_string(&agenda_path).unwrap(),
        "# Agenda changed elsewhere\n",
        "Keep mine never writes the kanban's text into Agenda.md"
    );
}

/// P1: a kanban save clears the shared recovery slot. Write's record for the
/// same note must go with it, or its next tick trusts a copy that is gone
/// and the dirty buffer stops being recoverable.
#[test]
fn a_kanban_save_does_not_strand_the_editors_recovery_copy_for_the_same_rel() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    // Write on Board, dirty with a conflict; the typed text is not a board,
    // so the Kanban page re-reads the file instead of taking it over.
    act(&mut s.rt, |s| s.set_mode(Mode::Editor));
    atomic_write(&file(&tmp), &theirs());
    let typed = "Text typed in Write, not a board yet.\n";
    s.rt.state.editor.replace_text(typed);
    let mut raised = false;
    act(&mut s.rt, |s| raised = s.save_now());
    assert!(raised);
    act(&mut s.rt, |s| s.set_mode(Mode::Kanban));
    frames(&mut s.rt, 60);
    let copy = s.rt.state.recovery_path("Board.md");
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        typed,
        "Write's buffer is copied"
    );

    // Save the board (from the file's text, plus one card movement).
    act(&mut s.rt, |s| s.toggle_card_done(0, 0));
    let mut saved = false;
    act(&mut s.rt, |s| saved = s.save_kanban());
    assert!(saved);
    let board_bytes = fs::read_to_string(file(&tmp)).unwrap();
    assert!(board_bytes.contains("- [x] Write the release notes"));

    // The save cleared the shared slot; the editor's next tick rewrites its
    // own copy instead of trusting the record for a file that is gone.
    frames(&mut s.rt, 60);
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        typed,
        "Write's buffer is recoverable again after the kanban save"
    );
    assert_eq!(
        fs::read_to_string(file(&tmp)).unwrap(),
        board_bytes,
        "and the kanban save is not undone"
    );
}

/// A second card edit bumps the change counter, so the recovery copy follows
/// it rather than resting on the first edit's record.
#[test]
fn each_card_edit_renews_the_recovery_copy() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    frames(&mut s.rt, 60);
    let copy = s.rt.state.kanban_recovery_path("Board.md");
    assert_eq!(fs::read_to_string(&copy).unwrap(), mine());
    act(&mut s.rt, |s| s.toggle_card_done(0, 0));
    let want = shown(&s.rt);
    assert_ne!(want, mine(), "a second edit");
    frames(&mut s.rt, 60);
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        want,
        "the copy follows the newest edit"
    );
}

/// A refresh that reads the same text back keeps the page as it is, so its
/// undo history survives; replacing it would lose the redo.
#[test]
fn a_same_text_refresh_keeps_the_pages_undo_history() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    act(&mut s.rt, |s| s.undo_kanban());
    assert_eq!(shown(&s.rt), BOARD, "undo puts the exact bytes back");
    act(&mut s.rt, |s| s.refresh_kanban());
    assert_eq!(shown(&s.rt), BOARD);
    act(&mut s.rt, |s| s.redo_kanban());
    assert_eq!(shown(&s.rt), mine(), "redo still has the edit");
}

/// P2: a followed rename moves the recovery copy to the new key before
/// forgetting the old one, so there is no window with the edits under
/// neither name.
#[test]
fn following_a_rename_moves_the_recovery_copy_before_forgetting_it() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    frames(&mut s.rt, 60);
    let old = s.rt.state.kanban_recovery_path("Board.md");
    assert_eq!(fs::read_to_string(&old).unwrap(), mine());
    fs::rename(file(&tmp), tmp.path().join("Renamed.md")).unwrap();
    // Stop in the frame the page follows: the copy must already be there, not
    // written by a later recovery tick.
    let mut followed = false;
    for _ in 0..240 {
        s.rt.frame(1.0 / 60.0);
        if s.rt.state.kanban.as_ref().map(|k| k.rel.as_str()) == Some("Renamed.md") {
            followed = true;
            break;
        }
    }
    assert!(followed, "the rename is followed");
    assert_eq!(
        fs::read_to_string(s.rt.state.kanban_recovery_path("Renamed.md")).unwrap(),
        mine(),
        "the edits are under the new name as soon as the rename is followed"
    );
    assert!(!old.exists(), "and the old key is gone");
}

/// P2-5b: following a rename clears the old path's recovery copy; the save
/// leaves none under the new name either.
#[test]
fn following_a_rename_clears_the_old_paths_recovery_copy() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    frames(&mut s.rt, 60);
    let old = s.rt.state.kanban_recovery_path("Board.md");
    assert!(old.exists(), "old copy written");
    fs::rename(file(&tmp), tmp.path().join("Renamed.md")).unwrap();
    wait(&mut s.rt);
    assert_eq!(
        s.rt.state.kanban.as_ref().unwrap().rel,
        "Renamed.md",
        "the rename is followed"
    );
    assert!(!old.exists(), "the old path's copy goes with the rename");
    assert!(s.rt.state.recovered.iter().all(|r| r.rel != "Board.md"));
    assert!(s.rt.state.save_kanban());
    assert_eq!(
        fs::read_to_string(tmp.path().join("Renamed.md")).unwrap(),
        mine()
    );
    assert!(
        !s.rt.state.kanban_recovery_path("Renamed.md").exists(),
        "a save leaves no copy"
    );
    assert!(s.rt.state.recovered.is_empty());
}

/// P1: restoring a recovery copy replaces the board. A menu opened on the old
/// one names a place there, so it must close with it rather than let Delete
/// act on the restored board.
#[test]
fn restoring_another_kanban_closes_a_menu_opened_on_the_old_one() {
    let b_src = BOARD.replace("## To do\n\n", "## To do\n\n- [ ] B only\n");
    let b_mine = b_src.replace("- [ ] B only", "- [x] B only");
    let tmp = TempVault::new(&[
        ("Agenda.md", "# Agenda\n"),
        ("A.md", BOARD),
        ("B.md", b_src.as_str()),
    ]);
    let (config, state_home) = (unique_dir("config"), unique_dir("state"));
    fs::create_dir_all(config.join("muninn")).unwrap();
    fs::write(
        config.join("muninn/settings.json"),
        r#"{"kanban_autosave": false}"#,
    )
    .unwrap();

    // A crashed session leaves B's card edit in its recovery copy.
    let mut rt = launch_with(tmp.path(), &config, &state_home);
    let b = rt.state.vault.resolve("B").expect("B");
    rt.update(|s, _| s.open_kanban(b));
    frames(&mut rt, 2);
    rt.update(|s, _| s.toggle_card_done(0, 0));
    frames(&mut rt, 60);
    assert_eq!(
        fs::read_to_string(rt.state.kanban_recovery_path("B.md")).unwrap(),
        b_mine
    );
    drop(rt);

    // A new launch: open A, open its first card's menu, then restore B.
    let mut rt = launch_with(tmp.path(), &config, &state_home);
    frames(&mut rt, 2);
    let a = rt.state.vault.resolve("A").expect("A");
    rt.update(|s, _| s.open_kanban(a));
    frames(&mut rt, 2);
    rt.update(|s, _| s.select_kanban_card(0, 0));
    rt.update(|s, _| s.open_card_menu(0, 0));
    assert!(rt.state.menu.is_some(), "a menu is open on A");
    rt.update(|s, _| assert!(s.restore_recovery("B.md")));
    frames(&mut rt, 2);
    assert_eq!(rt.state.kanban.as_ref().unwrap().rel, "B.md");
    assert!(
        rt.state.menu.is_none(),
        "the menu closed with the old board"
    );
    // The menu named A's first card; picking Delete must not touch B's.
    rt.update(|s, _| s.pick_menu("delete"));
    frames(&mut rt, 2);
    assert_eq!(shown(&rt), b_mine, "B's card is not deleted");
    assert_eq!(
        fs::read_to_string(tmp.path().join("B.md")).unwrap(),
        b_src,
        "and nothing was written"
    );
}
