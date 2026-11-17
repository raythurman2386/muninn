//! #189: documents become Markdown notes, converted off the UI thread.
//!
//! One import at a time runs [`muninn_import::convert`] on a `std::thread` and
//! reports back through an `mpsc` channel, drained in [`crate::poll`]. The
//! convert step is pure, so a parser panic costs that import, never the
//! window. Writing happens on the UI thread after a successful conversion:
//! the note, its images and the source document land in the current folder,
//! each through the vault's atomic writer, and a failure removes what this
//! import already wrote so the vault is left as it was.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use muninn_core::{atomic_copy, atomic_write};
use muninn_i18n::{
    Key as TextKey, could_not_create_note, could_not_import_into, import_body_too_large,
    import_document_notice, import_encrypted, import_failed, import_io, import_malformed,
    import_missing_part, import_needs_ocr, import_progress, import_resource_limit,
    import_too_large, import_unsupported, t,
};
use muninn_import::{Converted, ImportError, convert};
use rune::{FileDialogOptions, Tx};

use crate::Mode;
use crate::State;

/// Extensions `anydoc` converts into a note. PDFs convert here too: a dropped
/// PDF is a document, not an opaque attachment.
pub const DOCUMENT_EXTENSIONS: &[&str] = &[
    "doc", "docm", "docx", "odt", "ods", "odp", "pdf", "ppt", "pptm", "pptx", "rtf", "epub", "xls",
    "xlsb", "xlsm", "xlsx", "csv",
];

/// The import chord as this platform labels it (`⌘I` / `Ctrl+I`).
pub(crate) const IMPORT_CHORD: &str = if cfg!(target_os = "macos") {
    "⌘I"
} else {
    "Ctrl+I"
};

/// How often `poll` checks a conversion in flight. The worker wakes the window
/// by finishing; this only keeps a resting window looking while it runs.
pub(crate) const IMPORT_WAKE: Duration = Duration::from_millis(120);

/// One document waiting to convert, with the folder chosen when it was
/// queued. The folder does not follow a later note selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportQueueItem {
    pub path: PathBuf,
    pub folder: String,
}

/// What the worker sends back to the UI thread.
pub(crate) enum ImportMessage {
    Converted {
        path: PathBuf,
        folder: String,
        name: String,
        converted: Converted,
    },
    Failed {
        name: String,
        error: ImportError,
    },
    /// The parser panicked. The unwind was caught on the worker thread.
    Panicked {
        name: String,
    },
}

impl State {
    /// Ask the desktop for one document through Rune's Open dialog.
    ///
    /// A desktop without an XDG FileChooser portal (or a build without the
    /// `native-file-dialog` feature) answers with an error; the status bar
    /// turns that into the drag-and-drop fallback instead of showing the
    /// platform's message. Cancelling changes nothing.
    pub fn open_import_document(&mut self, tx: &mut Tx) {
        let options = FileDialogOptions {
            title: t(TextKey::ImportDialogTitle).to_owned(),
            ..Default::default()
        }
        .filter(t(TextKey::ImportDialogFilterDocuments), DOCUMENT_EXTENSIONS);
        tx.open_file(options, |s: &mut State, result, _| match result {
            Ok(Some(path)) => {
                let _ = s.import_files(&[path]);
            }
            Ok(None) => {}
            Err(_) => {
                s.notice = None;
                s.error = Some(t(TextKey::ErrorImportDialogUnavailable).to_owned());
            }
        });
    }

    /// Whether a conversion is running or queued. The window keeps polling
    /// while this is true so a result does not wait for the next input.
    #[must_use]
    pub fn import_busy(&self) -> bool {
        self.import_inbox.is_some() || !self.import_queue.is_empty()
    }

    /// Queue `path` for conversion into `folder`, starting the worker when
    /// nothing else is in flight.
    pub(crate) fn queue_import(&mut self, path: PathBuf, folder: String) {
        self.import_queue.push(ImportQueueItem { path, folder });
        self.start_next_import();
    }

    /// Start the next queued document unless one is already converting.
    fn start_next_import(&mut self) {
        if self.import_inbox.is_some() {
            return;
        }
        let Some(item) = self.import_queue.first().cloned() else {
            return;
        };
        self.import_queue.remove(0);
        let name = file_name(&item.path);
        self.notice = Some(import_progress(&name));

        let (sender, receiver) = mpsc::channel();
        self.import_inbox = Some(receiver);
        thread::spawn(move || {
            // A parser panic is one failed import: catch the unwind, then let
            // the default hook print the panic like any other thread. The
            // window and the vault are untouched.
            let converted =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| convert(&item.path)));
            let message = match converted {
                Ok(Ok(converted)) => ImportMessage::Converted {
                    path: item.path,
                    folder: item.folder,
                    name,
                    converted,
                },
                Ok(Err(error)) => ImportMessage::Failed { name, error },
                Err(_) => ImportMessage::Panicked { name },
            };
            let _ = sender.send(message);
        });
    }

    /// Drain finished conversions. Called once per frame from [`crate::poll`].
    pub(crate) fn poll_import(&mut self) -> bool {
        let mut messages = Vec::new();
        let mut disconnected = false;
        if let Some(inbox) = self.import_inbox.as_ref() {
            loop {
                match inbox.try_recv() {
                    Ok(message) => messages.push(message),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if messages.is_empty() {
            if !disconnected {
                return false;
            }
            // The worker vanished without a message: free the slot so the
            // queue keeps moving instead of looking busy forever.
            self.import_inbox = None;
            self.start_next_import();
            return true;
        }
        for message in messages {
            self.import_inbox = None;
            match message {
                ImportMessage::Converted {
                    path,
                    folder,
                    name,
                    converted,
                } => self.finish_import(&path, &folder, &name, converted),
                ImportMessage::Failed { name, error } => {
                    self.notice = None;
                    self.error = Some(import_error_text(&name, &error));
                }
                ImportMessage::Panicked { name } => {
                    self.notice = None;
                    self.error = Some(import_failed(&name));
                }
            }
            self.start_next_import();
        }
        true
    }

    /// Write one converted document beside the current note: the note itself,
    /// one file per embedded image and the original source, then select it.
    fn finish_import(&mut self, source: &Path, folder: &str, name: &str, converted: Converted) {
        let dir = if folder.is_empty() {
            self.vault.root.clone()
        } else {
            self.vault.root.join(folder)
        };
        if let Err(error) = fs::create_dir_all(&dir) {
            self.notice = None;
            self.error = Some(could_not_import_into(dir.display(), error));
            return;
        }

        // Reserve a name per written file before touching disk, so a second
        // file in the same import cannot collide with the first, and an
        // existing file is never overwritten.
        let mut reserved = HashSet::new();
        let mut assets = Vec::with_capacity(converted.assets.len());
        for asset in converted.assets {
            let written = unique_name(&dir, &asset.file_name, &mut reserved);
            assets.push((written, asset.bytes));
        }
        let source_name = unique_name(&dir, &converted.source_name, &mut reserved);

        let title = document_title(&converted.markdown, name);
        let mut body = converted.markdown.trim_end().to_owned();
        body.push_str("\n\n");
        for (written, _) in &assets {
            body.push_str(&format!("![[{written}]]\n"));
        }
        body.push_str(&format!("![[{source_name}]]\n"));

        self.flush();
        let note_path = match self.vault.create_note_with_body_in(&title, folder, &body) {
            Ok(path) => path,
            Err(error) => {
                self.notice = None;
                self.error = Some(could_not_create_note(error));
                return;
            }
        };

        let mut written_files: Vec<PathBuf> = Vec::new();
        let mut write_failed = None;
        for (written, bytes) in &assets {
            let path = dir.join(written);
            match atomic_write(&path, bytes) {
                Ok(()) => written_files.push(path),
                Err(error) => {
                    write_failed = Some(error);
                    break;
                }
            }
        }
        if write_failed.is_none() {
            match atomic_copy(source, &dir.join(&source_name)) {
                Ok(()) => {}
                Err(error) => write_failed = Some(error),
            }
        }
        if let Some(error) = write_failed {
            // Undo this import's writes: the note and every asset that already
            // landed. A failure before the source copy means the vault is as
            // it was.
            let _ = fs::remove_file(&note_path);
            for path in &written_files {
                let _ = fs::remove_file(path);
            }
            self.notice = None;
            self.error = Some(could_not_import_into(dir.display(), error));
            return;
        }

        self.reload(Some(note_path));
        self.mode = Mode::Editor;
        self.focus_editor();
        let end = self.editor.text.len();
        self.editor.anchor = end;
        self.editor.focus = end;
        self.notice = Some(import_document_notice(&title));
        self.trace_log(
            "import-document",
            &format!("source={name} assets={} folder={folder}", assets.len()),
        );
    }
}

/// The first `# ` heading, else the source file stem.
fn document_title(markdown: &str, source_name: &str) -> String {
    markdown
        .lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            Path::new(source_name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
                .filter(|stem| !stem.is_empty())
                .unwrap_or_else(|| "Untitled".to_owned())
        })
}

/// The display name for a path, lossy for non-UTF-8 names.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// `<stem>.<ext>` or `<stem> 2.<ext>`, skipping names already on disk or
/// reserved by this import. The comparison is case-insensitive so a vault on
/// a case-insensitive filesystem never gets two names that collide there.
fn unique_name(dir: &Path, name: &str, reserved: &mut HashSet<String>) -> String {
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, format!(".{extension}")),
        _ => (name, String::new()),
    };
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            name.to_owned()
        } else {
            format!("{stem} {n}{extension}")
        };
        if !dir.join(&candidate).exists() && reserved.insert(candidate.to_lowercase()) {
            return candidate;
        }
        n += 1;
    }
}

/// One `ImportError` as the status line's sentence.
pub(crate) fn import_error_text(name: &str, error: &ImportError) -> String {
    match error {
        ImportError::Unsupported => import_unsupported(name),
        ImportError::NeedsOcr { pages, page_count } => import_needs_ocr(name, pages, *page_count),
        ImportError::Encrypted => import_encrypted(name),
        ImportError::Malformed { .. } => import_malformed(name),
        ImportError::ResourceLimit => import_resource_limit(name),
        ImportError::MissingPart { .. } => import_missing_part(name),
        ImportError::Io => import_io(name),
        ImportError::TooLarge { size, limit } => import_too_large(name, *size, *limit),
        ImportError::BodyTooLarge { size, limit } => import_body_too_large(name, *size, *limit),
        // `Other` and any future variant: the parser's own text is a
        // diagnostic, not copy, so the catalog's generic sentence stands in.
        _ => import_failed(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_import_error_maps_to_a_sentence_naming_the_file() {
        let errors = [
            ImportError::Unsupported,
            ImportError::NeedsOcr {
                pages: vec![4],
                page_count: 8,
            },
            ImportError::Encrypted,
            ImportError::Malformed {
                part: Some("word/document.xml".into()),
            },
            ImportError::ResourceLimit,
            ImportError::MissingPart {
                part: "word/document.xml".into(),
            },
            ImportError::Io,
            ImportError::TooLarge {
                size: 60 * 1024 * 1024,
                limit: 50 * 1024 * 1024,
            },
            ImportError::BodyTooLarge {
                size: 12 * 1024 * 1024,
                limit: 10 * 1024 * 1024,
            },
            ImportError::Other("parser detail".into()),
        ];
        for error in errors {
            let text = import_error_text("report.docx", &error);
            assert!(
                text.starts_with("Could not import report.docx:"),
                "{error:?} produced {text:?}"
            );
            assert!(text.ends_with('.'), "{error:?} produced {text:?}");
        }
    }

    #[test]
    fn a_title_prefers_the_first_h1_then_the_stem() {
        assert_eq!(
            document_title("# First\n\n## Second\n", "stem.docx"),
            "First"
        );
        assert_eq!(document_title("## Only two\n", "stem.docx"), "stem");
        assert_eq!(document_title("#   \n", "stem.docx"), "stem");
    }

    #[test]
    fn unique_names_never_reuse_an_existing_or_reserved_name() {
        let dir = std::env::temp_dir();
        let mut reserved = HashSet::new();
        let stem = format!("muninn-unique-{}", std::process::id());
        let first = format!("{stem}.png");
        let second = format!("{stem} 2.png");
        let third = format!("{stem} 3.png");
        assert_eq!(unique_name(&dir, &first, &mut reserved), first);
        reserved.insert(second);
        assert_eq!(unique_name(&dir, &first, &mut reserved), third);
    }
}
