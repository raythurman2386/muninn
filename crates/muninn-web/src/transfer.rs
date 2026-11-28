//! Browser vault export and import (#240 slice 6e).
//!
//! Export walks the store's mirror and hands every file's bytes to the stored
//! zip codec in [`crate::zip`]; import reads the same archive back and writes
//! each member through [`VaultStore::write_atomic`]. Both directions are pure
//! `VaultStore` work, so the browser and the native tests exercise one code
//! path: exporting a `MemoryStore`-seeded vault and importing it into a fresh
//! one is the same walk the OPFS mirror goes through.
//!
//! Nothing here decides *when* an import may land: the shared UI runs the
//! ordinary save guard first (a clean vault only) and re-indexes afterwards,
//! through [`crate::transfer`]'s caller in the wasm entry. This module owns only
//! the bytes.

use std::io;
use std::path::Path;

use muninn_core::{EntryKind, VaultStore};

use crate::zip::{self, ZipEntry};

/// One member of an import: a file's bytes, or a directory (`None`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// `/`-separated path below the vault root.
    pub rel: String,
    /// The file's bytes; `None` for a directory.
    pub bytes: Option<Vec<u8>>,
}

/// Why an export or import could not be completed.
///
/// The text is developer-facing (logged to the console); the status bar shows
/// a catalogued key.
#[derive(Debug)]
pub enum TransferError {
    /// A store read or write failed, with the vault-relative path.
    Io { rel: String, error: io::Error },
    /// The archive itself is broken or unsupported.
    Zip(zip::ZipError),
    /// An archive member names a path outside the vault.
    UnsafePath(String),
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferError::Io { rel, error } => write!(formatter, "{rel}: {error}"),
            TransferError::Zip(error) => write!(formatter, "{error}"),
            TransferError::UnsafePath(rel) => write!(formatter, "unsafe archive path {rel:?}"),
        }
    }
}

impl std::error::Error for TransferError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TransferError::Io { error, .. } => Some(error),
            TransferError::Zip(error) => Some(error),
            TransferError::UnsafePath(_) => None,
        }
    }
}

/// Serialize every file and folder the store holds into a stored zip archive.
///
/// The walk is `read_dir` all the way down: notes, boards, attachments, dot
/// files and empty folders all leave exactly as they are spelled. Entries are
/// sorted by name at every level so two exports of the same vault are
/// byte-identical.
pub fn export_zip<S>(store: &S) -> Result<Vec<u8>, TransferError>
where
    S: VaultStore + ?Sized,
{
    let entries = export_entries(store)?;
    zip::write(&entries).map_err(TransferError::Zip)
}

/// The archive members an export would write, before serializing them.
pub fn export_entries<S>(store: &S) -> Result<Vec<ZipEntry>, TransferError>
where
    S: VaultStore + ?Sized,
{
    let mut out = Vec::new();
    walk(store, store.root(), "", &mut out)?;
    Ok(out)
}

fn walk<S>(store: &S, dir: &Path, rel: &str, out: &mut Vec<ZipEntry>) -> Result<(), TransferError>
where
    S: VaultStore + ?Sized,
{
    let mut children = store.read_dir(dir).map_err(|error| TransferError::Io {
        rel: if rel.is_empty() {
            ".".to_owned()
        } else {
            rel.to_owned()
        },
        error,
    })?;
    children.sort_by(|a, b| a.name.cmp(&b.name));
    for entry in children {
        let name = entry.name.to_string_lossy();
        let child_rel = if rel.is_empty() {
            name.into_owned()
        } else {
            format!("{rel}/{name}")
        };
        match entry.kind {
            EntryKind::Dir => {
                out.push(ZipEntry::dir(child_rel.clone()));
                walk(store, &entry.path, &child_rel, out)?;
            }
            EntryKind::File => {
                let bytes = store
                    .read_bytes(&entry.path)
                    .map_err(|error| TransferError::Io {
                        rel: child_rel.clone(),
                        error,
                    })?;
                out.push(ZipEntry::file(child_rel, bytes));
            }
        }
    }
    Ok(())
}

/// Parse a picked file into import entries.
///
/// A zip archive is the contract. A plain Markdown file is accepted too when
/// the picker named one (import one note at the vault root); anything else is
/// the archive's own error.
pub fn read_archive(
    bytes: &[u8],
    picked_name: Option<&str>,
) -> Result<Vec<ArchiveEntry>, TransferError> {
    match zip::read(bytes) {
        Ok(entries) => Ok(entries
            .into_iter()
            .map(|entry| ArchiveEntry {
                rel: entry.name,
                bytes: if entry.is_dir {
                    None
                } else {
                    Some(entry.bytes)
                },
            })
            .collect()),
        Err(error) => match picked_name.and_then(markdown_name) {
            Some(rel) if std::str::from_utf8(bytes).is_ok() => Ok(vec![ArchiveEntry {
                rel,
                bytes: Some(bytes.to_vec()),
            }]),
            _ => Err(TransferError::Zip(error)),
        },
    }
}

/// Write entries into `store`. Returns how many files were written.
///
/// Folders are created before their contents; every file goes through
/// [`VaultStore::write_atomic`], so the store's dirty journal (OPFS in the
/// browser) persists it on the next drain. An unsafe member path refuses the
/// whole import before anything is written: the paths are validated in a first
/// pass, not member by member.
pub fn write_archive(
    store: &dyn VaultStore,
    entries: &[ArchiveEntry],
) -> Result<usize, TransferError> {
    let mut safe: Vec<(&ArchiveEntry, String)> = Vec::with_capacity(entries.len());
    for entry in entries {
        let rel =
            safe_rel(&entry.rel).ok_or_else(|| TransferError::UnsafePath(entry.rel.clone()))?;
        safe.push((entry, rel));
    }
    let root = store.root().to_path_buf();
    let mut files = 0;
    for (entry, rel) in safe {
        let path = root.join(&rel);
        match &entry.bytes {
            None => store
                .create_dir_all(&path)
                .map_err(|error| TransferError::Io {
                    rel: rel.clone(),
                    error,
                })?,
            Some(bytes) => {
                if let Some(parent) = path.parent() {
                    store
                        .create_dir_all(parent)
                        .map_err(|error| TransferError::Io {
                            rel: rel.clone(),
                            error,
                        })?;
                }
                store
                    .write_atomic(&path, bytes)
                    .map_err(|error| TransferError::Io {
                        rel: rel.clone(),
                        error,
                    })?;
                files += 1;
            }
        }
    }
    Ok(files)
}

/// A vault-relative spelling for an archive member, or `None` when it could
/// escape the root.
///
/// `\` becomes `/` (some Windows tools spell folders that way), empty and `.`
/// segments are dropped, and an absolute path, drive prefix, NUL or `..` is
/// refused rather than rewritten.
#[must_use]
pub fn safe_rel(rel: &str) -> Option<String> {
    let rel = rel.replace('\\', "/");
    if rel.starts_with('/') || rel.contains('\0') {
        return None;
    }
    if matches!(rel.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic()) {
        return None;
    }
    let mut parts = Vec::new();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn markdown_name(name: &str) -> Option<String> {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if name.to_lowercase().ends_with(".md") {
        safe_rel(name)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use muninn_core::{MemoryStore, Vault};

    const ROOT: &str = "/Muninn sample";
    const BOARD: &str =
        r#"{"type":"excalidraw","version":2,"elements":[],"appState":{},"files":{}}"#;
    const PIC: &[u8] = b"\x89PNG-not-really-a-picture";

    fn seeded() -> MemoryStore {
        let store = MemoryStore::new(ROOT);
        store.put_file("Welcome.md", b"# Welcome\n\nThe tour.\n");
        store.put_file(
            "Projects/Plan.md",
            b"# Plan\n\n![[Plan.excalidraw]] and ![[assets/pic.png]]\n",
        );
        store.put_file("Plan.excalidraw", BOARD.as_bytes());
        store.put_file("assets/pic.png", PIC);
        store
            .create_dir_all(&Path::new(ROOT).join("empty"))
            .expect("an empty folder");
        store
    }

    #[test]
    fn every_file_and_folder_is_in_the_archive() {
        let entries = export_entries(&seeded()).expect("export");
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Plan.excalidraw",
                "Projects/",
                "Projects/Plan.md",
                "Welcome.md",
                "assets/",
                "assets/pic.png",
                "empty/",
            ]
        );
        let pic = entries
            .iter()
            .find(|entry| entry.name == "assets/pic.png")
            .expect("the attachment");
        assert_eq!(pic.bytes, PIC, "attachment bytes are exported exactly");
    }

    #[test]
    fn export_and_import_round_trip_the_vault_index() {
        let source = seeded();
        let archive = export_zip(&source).expect("export");
        let entries = read_archive(&archive, Some("muninn-vault.zip")).expect("read");
        let target = MemoryStore::new(ROOT);
        let files = write_archive(&target, &entries).expect("import");
        assert_eq!(files, 4, "the four files, not the folders");
        let reexported = export_zip(&target).expect("re-export");
        assert_eq!(
            reexported, archive,
            "export -> import -> export is byte-identical"
        );

        let before = Vault::open(Arc::new(source)).expect("source vault");
        let after = Vault::open(Arc::new(target)).expect("imported vault");
        assert_eq!(after.notes.len(), before.notes.len());
        let titles = |vault: &Vault| {
            let mut titles: Vec<(String, String)> = vault
                .notes
                .iter()
                .map(|note| (note.rel.clone(), note.title.clone()))
                .collect();
            titles.sort();
            titles
        };
        assert_eq!(titles(&after), titles(&before));
        assert_eq!(after.boards(), before.boards());
        assert_eq!(
            after.dirs, before.dirs,
            "folders, including the empty one, survive"
        );

        let plan = after.resolve("Plan").expect("Plan");
        let attachments = after.attachments(&after.note(plan).source, plan);
        let pic = attachments
            .iter()
            .find(|attachment| attachment.name == "pic.png")
            .expect("the attachment embed resolves after import");
        assert_eq!(pic.size, Some(PIC.len() as u64));
    }

    #[test]
    fn an_unsafe_member_refuses_the_whole_archive() {
        let store = MemoryStore::new(ROOT);
        let entries = [
            ArchiveEntry {
                rel: "Notes/Good.md".to_owned(),
                bytes: Some(b"# Good\n".to_vec()),
            },
            ArchiveEntry {
                rel: "../escape.md".to_owned(),
                bytes: Some(b"# Escape\n".to_vec()),
            },
        ];
        assert!(matches!(
            write_archive(&store, &entries),
            Err(TransferError::UnsafePath(_))
        ));
        assert!(
            !store.exists(&Path::new(ROOT).join("Notes/Good.md")),
            "a refused archive writes nothing"
        );
        assert!(!store.exists(Path::new("/escape.md")));
    }

    #[test]
    fn a_plain_markdown_file_imports_as_one_note() {
        let entries = read_archive(b"# Loose note\n", Some("Loose note.md")).expect("read");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].rel, "Loose note.md");
        assert_eq!(
            entries[0].bytes.as_deref(),
            Some(b"# Loose note\n".as_slice())
        );

        // Not Markdown and not a zip: the archive error stands.
        assert!(read_archive(b"\x00\x01", Some("photo.png")).is_err());
    }

    #[test]
    fn a_dot_file_and_a_folder_name_round_trip() {
        let store = MemoryStore::new(ROOT);
        store.put_file(".hidden/state.json", b"{}");
        let archive = export_zip(&store).expect("export");
        let target = MemoryStore::new(ROOT);
        let entries = read_archive(&archive, None).expect("read");
        write_archive(&target, &entries).expect("import");
        assert!(target.is_file(&Path::new(ROOT).join(".hidden/state.json")));
    }
}
