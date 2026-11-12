//! Kanban slice 7: autosave, the watcher, the conflict bar, the recovery
//! copy, the quit save and the Write hand-off, headless.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{BoardStatus, DocConflict, Mode, Settings, State, on_key, poll, view};
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
    let copy = s.rt.state.recovery_path("Board.md");
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
    assert!(!s.rt.state.recovery_path("Board.md").exists());
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
    let copy = s.rt.state.recovery_path("Board.md");
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
    let copy = s.rt.state.recovery_path("Board.md");
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
        !s.rt.state.recovery_path("Board.md").exists(),
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
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    frames(&mut s.rt, 60);
    assert!(
        s.rt.state.recovery_path("Board.md").exists(),
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
    let copy = s.rt.state.recovery_path("Board.md");
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

/// P2-5b: following a rename clears the old path's recovery copy; the save
/// leaves none under the new name either.
#[test]
fn following_a_rename_clears_the_old_paths_recovery_copy() {
    let tmp = vault();
    let mut s = on_board(&tmp, false);
    act(&mut s.rt, |s| s.toggle_card_done(1, 0));
    frames(&mut s.rt, 60);
    let old = s.rt.state.recovery_path("Board.md");
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
        !s.rt.state.recovery_path("Renamed.md").exists(),
        "a save leaves no copy"
    );
    assert!(s.rt.state.recovered.is_empty());
}
