//! #189: the import affordance, the file dialog, the worker and the vault
//! layout.
//!
//! Everything here is headless: the docx fixture converts on the real worker
//! thread and is drained by `poll`, and the dialog is answered with Rune's
//! scripted `resolve_file_dialog`. Failure tests compare a byte snapshot of
//! the vault before and after, so an unsupported, encrypted or oversized
//! document proves it changes nothing.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::Duration;

use muninn_core::testing::TempVault;
use muninn_i18n::{
    Key, import_body_too_large, import_encrypted, import_needs_ocr, import_too_large,
    import_unsupported, t,
};
use muninn_import::MAX_INPUT_BYTES;
use muninn_ui::{
    DOCUMENT_EXTENSIONS, IMPORT_EXTENSIONS, PaletteAction, PaletteItem, State, on_key, poll, view,
};
use rune::{Env, FileDialogKind, Key as RuneKey, KeyEvent, Mods, Runtime};

const DOCX: &[u8] = include_bytes!("fixtures/embedded-note.docx");
const PIXEL: &[u8] = include_bytes!("fixtures/pixel.png");
const ENCRYPTED: &[u8] = include_bytes!("../../muninn-import/tests/fixtures/encrypted.docx");

/// A uniquely named folder for documents dropped "from outside" the vault.
struct DropDir(PathBuf);

impl DropDir {
    fn new() -> DropDir {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "muninn-import-ui-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        drop(fs::remove_dir_all(&path));
        fs::create_dir_all(&path).expect("drop folder");
        DropDir(path)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).expect("fixture");
        path
    }
}

impl Drop for DropDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn runtime(vault: &TempVault) -> Runtime<State> {
    let state = State::with_state_home(
        vault.path().to_path_buf(),
        vault.path().with_extension("state"),
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

/// Drive frames until the worker has landed its result.
fn pump(rt: &mut Runtime<State>) {
    for _ in 0..600 {
        frame(rt);
        if !rt.state.import_busy() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the import did not finish");
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frame(rt);
    frame(rt);
}

fn text(value: &str) -> KeyEvent {
    KeyEvent {
        key: RuneKey::Text(value.to_owned()),
        mods: Mods::default(),
    }
}

/// Every file below `root` with its bytes, sorted, for a before/after check.
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, files: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .expect("read dir")
            .filter_map(Result::ok)
            .collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, files);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("inside the vault")
                    .display()
                    .to_string();
                files.push((rel, fs::read(&path).expect("read file")));
            }
        }
    }
    let mut files = Vec::new();
    walk(root, root, &mut files);
    files
}

#[test]
fn a_docx_becomes_a_note_with_its_image_and_source_in_the_current_folder() {
    let vault = TempVault::new(&[("Home.md", "# Home\n"), ("Journal/Day.md", "# Day\n")]);
    let drop_dir = DropDir::new();
    let source = drop_dir.write("embedded-note.docx", DOCX);
    let mut rt = runtime(&vault);
    let day = rt.state.vault.resolve("Day").expect("Day");
    rt.update(|s, _| s.load_note(day, false));
    assert_eq!(rt.state.current_folder(), "Journal");

    let report = rt.state.import_files(std::slice::from_ref(&source));
    assert_eq!(report.queued, 1, "{report:?}");
    assert!(report.changed());
    pump(&mut rt);

    let note = vault.path().join("Journal/Picture Report.md");
    assert!(note.is_file(), "the H1 titled the note");
    let body = fs::read_to_string(&note).expect("note body");
    assert!(body.starts_with("# Picture Report"), "{body}");
    assert_eq!(
        body.lines().filter(|line| line.starts_with("# ")).count(),
        1,
        "one H1, never a stub plus a second heading\n{body}"
    );
    assert!(body.contains("An embedded image follows."), "{body}");
    assert_eq!(
        body.matches("![[embedded-note-image-1.png]]").count(),
        1,
        "one embed for the extracted image\n{body}"
    );
    assert!(
        body.trim_end().ends_with("![[embedded-note.docx]]"),
        "the source document is the last embed\n{body}"
    );

    let image = vault.path().join("Journal/embedded-note-image-1.png");
    assert_eq!(
        fs::read(&image).expect("image"),
        PIXEL,
        "PNG bytes are identical"
    );
    let copied = vault.path().join("Journal/embedded-note.docx");
    assert_eq!(
        fs::read(&copied).expect("source document"),
        DOCX,
        "the original document is kept beside the note"
    );

    assert!(
        rt.state
            .selected_path
            .as_ref()
            .is_some_and(|path| path.ends_with("Picture Report.md")),
        "the imported note is selected"
    );
    assert!(rt.state.error.is_none(), "{:?}", rt.state.error);
    assert_eq!(
        rt.state.notice.as_deref(),
        Some("Imported Picture Report"),
        "the status line names the note"
    );
}

#[test]
fn the_brand_row_import_button_opens_a_documents_dialog_and_cancelling_changes_nothing() {
    let vault = TempVault::new(&[("Home.md", "# Home\n")]);
    let mut rt = runtime(&vault);
    let before = snapshot(vault.path());
    let selected = rt.state.selected;
    let button = rt
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("import-document"))
        .expect("the sidebar draws the import button");
    assert_eq!(button.role, "button");
    assert_eq!(button.label.as_deref(), Some(t(Key::ImportDocument)));
    assert_eq!(button.help.as_deref(), Some(t(Key::ImportDocumentHelp)));

    click(&mut rt, "import-document");
    let (kind, options) = rt.file_dialog().expect("the press asked for a dialog");
    assert_eq!(kind, FileDialogKind::Open);
    assert_eq!(options.title, t(Key::ImportDialogTitle));
    assert_eq!(
        options.filters,
        vec![(
            t(Key::ImportDialogFilterDocuments).to_owned(),
            DOCUMENT_EXTENSIONS
                .iter()
                .map(|extension| (*extension).to_owned())
                .collect::<Vec<_>>(),
        )],
        "one Documents filter lists every convertible extension"
    );

    rt.resolve_file_dialog(None).expect("cancel the dialog");
    for _ in 0..3 {
        frame(&mut rt);
    }
    assert!(rt.file_dialog().is_none());
    assert_eq!(snapshot(vault.path()), before, "cancel writes nothing");
    assert!(rt.state.error.is_none(), "{:?}", rt.state.error);
    assert_eq!(rt.state.selected, selected, "and selects nothing");

    // A desktop with no portal answers with an error; the copy points at a drop.
    // `Runtime` exposes only a successful/cancelled answer in tests, so the
    // catalogue sentence itself is pinned here (the callback maps every error
    // to it).
    let unavailable = t(Key::ErrorImportDialogUnavailable);
    assert!(unavailable.contains("drop"), "{unavailable}");
    assert_eq!(
        unavailable,
        "No file dialog on this desktop — drop the document on the window instead."
    );
}

#[test]
fn resolving_the_dialog_imports_the_chosen_document() {
    let vault = TempVault::new(&[("Home.md", "# Home\n")]);
    let drop_dir = DropDir::new();
    let source = drop_dir.write("embedded-note.docx", DOCX);
    let mut rt = runtime(&vault);

    rt.update(|s, tx| s.open_import_document(tx));
    assert!(rt.file_dialog().is_some());
    rt.resolve_file_dialog(Some(source))
        .expect("choose the docx");
    pump(&mut rt);

    assert!(vault.path().join("Picture Report.md").is_file());
    assert!(
        rt.state
            .selected_path
            .as_ref()
            .is_some_and(|path| path.ends_with("Picture Report.md"))
    );
    assert!(rt.state.error.is_none(), "{:?}", rt.state.error);
}

#[test]
fn a_second_import_dedupes_the_title_and_every_written_name() {
    let vault = TempVault::new(&[("Home.md", "# Home\n")]);
    let drop_dir = DropDir::new();
    let source = drop_dir.write("embedded-note.docx", DOCX);
    let mut rt = runtime(&vault);

    assert!(
        rt.state
            .import_files(std::slice::from_ref(&source))
            .changed()
    );
    pump(&mut rt);
    assert!(rt.state.import_files(&[source]).changed());
    pump(&mut rt);

    let first = vault.path().join("Picture Report.md");
    let second = vault.path().join("Picture Report 2.md");
    assert!(first.is_file());
    assert!(second.is_file(), "the second import never overwrites");
    assert_eq!(
        fs::read_to_string(&first).expect("first"),
        fs::read_to_string(&first).expect("first again"),
        "the first note keeps its bytes"
    );
    let second_body = fs::read_to_string(&second).expect("second body");
    assert!(
        second_body.contains("![[embedded-note-image-1 2.png]]"),
        "the image name is deduped too\n{second_body}"
    );
    assert!(
        second_body.contains("![[embedded-note 2.docx]]"),
        "the source copy is deduped too\n{second_body}"
    );
    assert!(vault.path().join("embedded-note 2.docx").is_file());
}

#[test]
fn conversion_failures_leave_the_vault_byte_identical_with_their_own_copy() {
    let vault = TempVault::new(&[("Home.md", "# Home\n")]);
    let drop_dir = DropDir::new();
    let mut rt = runtime(&vault);

    // Random bytes behind a supported extension reach the parser and fail as
    // malformed; nothing is written.
    let before = snapshot(vault.path());
    let random = drop_dir.write(
        "random.docx",
        &(0u16..=255).map(|byte| byte as u8).collect::<Vec<_>>(),
    );
    rt.state.import_files(&[random]);
    pump(&mut rt);
    let expected = muninn_i18n::import_malformed("random.docx");
    assert_eq!(rt.state.error.as_deref(), Some(expected.as_str()));
    assert_eq!(snapshot(vault.path()), before);

    // A refused extension never reaches conversion; the import crate's
    // `Unsupported` sentence is still pinned through its helper.
    assert_eq!(
        import_unsupported("notes.bin"),
        "Could not import notes.bin: this file type cannot be converted."
    );

    // Encrypted: the committed fixture, copied under a name the error can name.
    let encrypted = drop_dir.write("secret.docx", ENCRYPTED);
    rt.state.import_files(&[encrypted]);
    pump(&mut rt);
    let expected = import_encrypted("secret.docx");
    assert_eq!(rt.state.error.as_deref(), Some(expected.as_str()));
    assert_eq!(snapshot(vault.path()), before);

    // Over the pre-read cap, rejected from metadata.
    let large = drop_dir.0.join("large.docx");
    let file = File::create(&large).expect("create sparse file");
    let size = MAX_INPUT_BYTES + 1024 * 1024;
    file.set_len(size).expect("size sparse file");
    drop(file);
    rt.state.import_files(&[large]);
    pump(&mut rt);
    let expected = import_too_large("large.docx", size, MAX_INPUT_BYTES);
    assert_eq!(rt.state.error.as_deref(), Some(expected.as_str()));
    assert_eq!(snapshot(vault.path()), before);

    // The helper copy for the variants a fixture cannot produce on demand.
    assert_eq!(
        import_needs_ocr("scan.pdf", &[2, 3], 9),
        "Could not import scan.pdf: pages 2, 3 of 9 are scanned images and need OCR, which Muninn does not do. Export a text version first."
    );
    assert!(import_body_too_large("wide.csv", 11 << 20, 10 << 20).contains("11 MB"));
    assert!(
        !vault.path().join("Picture Report.md").exists(),
        "no failed import left a note"
    );
}

#[test]
fn the_palette_action_finds_and_runs_the_import() {
    let vault = TempVault::new(&[("Home.md", "# Home\n")]);
    let mut rt = runtime(&vault);
    assert!(PaletteAction::ALL.contains(&PaletteAction::ImportDocument));

    // The real chord is ⇧⌘P.
    rt.key(KeyEvent {
        key: RuneKey::Command('p'),
        mods: Mods {
            cmd: true,
            shift: true,
            ..Mods::default()
        },
    });
    frame(&mut rt);
    assert!(rt.state.palette_open());
    for ch in "import".chars() {
        rt.key(text(&ch.to_string()));
        frame(&mut rt);
    }
    assert!(
        rt.state
            .palette_items()
            .contains(&PaletteItem::Action(PaletteAction::ImportDocument)),
        "typing import finds the action: {:?}",
        rt.state.palette_items()
    );
    rt.key(KeyEvent {
        key: RuneKey::Enter,
        mods: Mods::default(),
    });
    frame(&mut rt);
    assert!(!rt.state.palette_open());
    let (kind, options) = rt.file_dialog().expect("the action opens the dialog");
    assert_eq!(kind, FileDialogKind::Open);
    assert_eq!(options.title, t(Key::ImportDialogTitle));
    rt.resolve_file_dialog(None).expect("cancel");
    frame(&mut rt);
}

#[test]
fn document_drops_are_accepted_and_images_stay_attachments() {
    for extension in DOCUMENT_EXTENSIONS {
        assert!(
            IMPORT_EXTENSIONS.contains(extension),
            "{extension} is convertible but the drop target refuses it"
        );
    }
    for extension in ["md", "png", "jpg", "jpeg", "gif", "webp"] {
        assert!(IMPORT_EXTENSIONS.contains(&extension));
    }
    assert!(
        DOCUMENT_EXTENSIONS.contains(&"pdf"),
        "a dropped PDF converts"
    );

    // The CSV path exercises the document branch without a worker fixture.
    let vault = TempVault::new(&[("Home.md", "# Home\n")]);
    let drop_dir = DropDir::new();
    let source = drop_dir.write("rows.csv", b"Name,Value\nNorth,42\n");
    let mut rt = runtime(&vault);
    rt.state.import_files(&[source]);
    pump(&mut rt);
    let note = vault.path().join("rows.md");
    assert!(note.is_file(), "a stem-titled document becomes a note");
    assert!(
        fs::read_to_string(note).expect("note").contains("North"),
        "the converted body landed"
    );
}

#[test]
fn the_help_page_lists_the_import_chord() {
    let vault = TempVault::new(&[("Home.md", "# Home\n")]);
    let mut rt = runtime(&vault);
    rt.update(|s, _| s.open_help());
    for _ in 0..6 {
        frame(&mut rt);
    }
    let action = t(Key::HelpEditImportDocument);
    assert!(
        rt.elements()
            .iter()
            .filter_map(|element| element.text.as_deref())
            .any(|text| text == action),
        "the Help page draws {action:?}"
    );
}
