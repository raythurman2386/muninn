//! DES-02 tail: attachment import, external open/reveal command construction
//! and the reader's attachment strip.
//!
//! Everything here is headless. The import tests call `State::import_files`
//! directly (no Rune drag simulation); the open tests check the command shape
//! and path validation without ever launching a program.

#[cfg(all(unix, not(target_os = "macos")))]
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use muninn_core::testing::TempVault;
#[cfg(all(unix, not(target_os = "macos")))]
use muninn_ui::{FileAction, file_action_command};
use muninn_ui::{Mode, State, poll, view};
use rune::{Env, Runtime};

/// A uniquely named sibling folder for files dropped "from outside" the vault.
struct DropDir(PathBuf);

impl DropDir {
    fn new() -> DropDir {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "muninn-drop-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        drop(fs::remove_dir_all(&path));
        fs::create_dir_all(&path).expect("drop folder");
        DropDir(path)
    }

    fn write(&self, name: &str, body: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, body).expect("fixture");
        path
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

fn state(vault: &TempVault, state_home: &Path) -> State {
    State::with_state_home(vault.path().to_path_buf(), state_home.to_path_buf())
}

#[test]
fn attachment_metadata_resolves_size_and_refuses_escapes() {
    let vault = TempVault::new(&[
        (
            "Home.md",
            "![[pic.png]] and ![[assets/shot.jpg]] and ![[pic.png]]\n",
        ),
        ("pic.png", "12345"),
        ("assets/shot.jpg", "xy"),
    ]);
    let v = vault.load();
    let home = v.resolve("Home").expect("Home");
    let found = v.attachments(&v.note(home).source, home);
    assert_eq!(found.len(), 2, "the repeated embed is listed once");
    assert_eq!(found[0].name, "pic.png");
    assert_eq!(found[0].size, Some(5));
    assert_eq!(found[0].size_label().as_deref(), Some("5 B"));
    assert!(found[0].path.as_ref().is_some_and(|p| p.is_absolute()));
    // A bare filename finds a file in a subfolder.
    assert_eq!(found[1].name, "shot.jpg");
    assert!(
        found[1]
            .path
            .as_ref()
            .is_some_and(|p| p.ends_with("assets/shot.jpg"))
    );
    let out = v.preview_with_attachments(&v.note(home).source, home);
    assert!(out.contains("`pic.png` (attachment) · 5 B"), "{out}");
    // `..` and absolute targets never escape the vault.
    assert!(v.attachment_path("../outside.png", home).is_none());
    assert!(v.attachment_path("/etc/passwd", home).is_none());
}

#[test]
fn dropping_markdown_copies_it_into_the_current_folder_and_reloads() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let drop_dir = DropDir::new();
    let mut state = state(&vault, &drop_dir.path().join("state"));
    let source = drop_dir.write("Dropped note.md", "# Dropped\n\nFresh from a drop.\n");

    let report = state.import_files(&[source]);
    assert_eq!(report.notes, 1, "{report:?}");
    assert_eq!(report.imported.len(), 1, "{report:?}");
    assert!(report.refused.is_empty(), "{report:?}");
    assert_eq!(
        report.imported[0],
        vault.path().join("Dropped note.md"),
        "the copy lands in the current (root) folder"
    );
    assert_eq!(
        fs::read_to_string(vault.path().join("Dropped note.md")).expect("copied note"),
        "# Dropped\n\nFresh from a drop.\n"
    );
    // The reload made the imported note part of the index and selected it.
    assert!(state.vault.resolve("Dropped note").is_some());
    assert!(
        state
            .selected_path
            .as_ref()
            .is_some_and(|path| path.ends_with("Dropped note.md"))
    );
    assert!(state.notice.is_some(), "the import reports itself");
    assert!(state.error.is_none(), "{:?}", state.error);
}

#[test]
fn importing_never_overwrites_an_existing_file() {
    let vault = TempVault::new(&[("Dropped.md", "original\n")]);
    let drop_dir = DropDir::new();
    let mut state = state(&vault, &drop_dir.path().join("state"));
    let source = drop_dir.write("Dropped.md", "replacement\n");

    let report = state.import_files(&[source]);
    assert!(report.imported.is_empty(), "{report:?}");
    assert_eq!(report.refused.len(), 1);
    assert!(
        report.refused[0].contains("Dropped.md") && report.refused[0].contains("already exists"),
        "{:?}",
        report.refused
    );
    assert_eq!(
        fs::read_to_string(vault.path().join("Dropped.md")).expect("original"),
        "original\n",
        "the existing note keeps its bytes"
    );
    assert!(state.error.is_some(), "the refusal is visible");
    // No stray temp file survives the failed copy.
    let leftovers: Vec<_> = fs::read_dir(vault.path())
        .expect("vault")
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with('.') && name.ends_with(".tmp")
        })
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn unsupported_drop_types_are_refused_with_a_reason() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let drop_dir = DropDir::new();
    let mut state = state(&vault, &drop_dir.path().join("state"));
    let source = drop_dir.write("notes.txt", "not a note\n");

    let report = state.import_files(&[source]);
    assert!(report.imported.is_empty(), "{report:?}");
    assert!(
        report.refused[0].contains("unsupported file type"),
        "{:?}",
        report.refused
    );
    assert!(!vault.path().join("notes.txt").exists());
    assert_eq!(state.vault.notes.len(), 1);
}

#[test]
fn images_are_imported_as_files_but_do_not_become_notes() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let drop_dir = DropDir::new();
    let mut state = state(&vault, &drop_dir.path().join("state"));
    let source = drop_dir.write("photo.png", "not really a png");

    let report = state.import_files(&[source]);
    assert_eq!(report.imported.len(), 1, "{report:?}");
    assert_eq!(report.notes, 0);
    assert!(vault.path().join("photo.png").is_file());
    assert_eq!(state.vault.notes.len(), 1, "the scan ignores non-.md files");
    let notice = state.notice.expect("attachment import notice");
    assert!(notice.contains("stay attachments"), "{notice}");
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn reveal_and_open_use_xdg_open_on_an_absolute_path() {
    let path = Path::new("/tmp/muninn-vault/Note.md");
    let (program, args) = file_action_command(FileAction::Open, path).expect("open command");
    assert_eq!(program, "xdg-open");
    assert_eq!(args, vec![OsString::from(path)]);

    let (program, args) = file_action_command(FileAction::Reveal, path).expect("reveal command");
    assert_eq!(program, "xdg-open");
    assert_eq!(
        args,
        vec![OsString::from("/tmp/muninn-vault")],
        "Reveal opens the containing folder, not the file"
    );
}

#[test]
fn opening_a_missing_path_reports_an_error_without_spawning() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let drop_dir = DropDir::new();
    let mut state = state(&vault, &drop_dir.path().join("state"));

    assert!(!state.open_externally(Path::new("/definitely/gone/attachment.png")));
    let error = state.error.expect("path validation error");
    assert!(error.contains("no longer exists"), "{error}");
}

#[test]
fn the_reader_lists_attachments_with_an_open_button() {
    let vault = TempVault::new(&[
        ("Home.md", "# Home\n\n![[assets/shot.jpg]]\n"),
        ("assets/shot.jpg", "fake jpeg bytes"),
    ]);
    let drop_dir = DropDir::new();
    let state = state(&vault, &drop_dir.path().join("state"));
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    let home = rt.state.vault.resolve("Home").expect("Home");
    rt.state.load_note(home, false);
    rt.state.set_mode(Mode::Preview);
    rt.frame(0.0);

    assert!(rt.find("attachments").is_some(), "the strip is mounted");
    assert!(rt.find("attachment-row-0").is_some());
    assert!(
        rt.find("attachment-open-0").is_some(),
        "each attachment has an Open action"
    );
}
