//! Scan a folder of Markdown files and index the links between them.
//!
//! Every `.md` file under the root is a note. Its **title** is the file stem
//! (e.g. `note`), its **folder** is the vault-relative directory, and its
//! **stem** is the vault-relative path without `.md`
//! (e.g. `projects/alpha/note`). The folder of `.md`
//! files remains the source of truth — there is no database.
//!
//! The walk skips dot-directories (`.obsidian`, `.git`, …) and goes at most
//! 12 levels deep. Nested folders are preserved on each [`Note`](crate::Note)
//! as `rel`/`folder`/`stem`; the UI groups the sidebar by folder.
//!
//! ## Wikilink resolution
//!
//! A link resolves case-insensitively by this rule, in order:
//!
//! 1. **Path-qualified (`[[folder/note]]`).** If the target contains `/`, it
//!    matches the vault-relative stem exactly, or as a unique `/`-suffixed
//!    tail (so `[[alpha/note]]` finds `projects/alpha/note`). A path-qualified target
//!    with no path match resolves to nothing — it never falls back to a bare
//!    title in another folder.
//! 2. **Unambiguous title (`[[Title]]`).** If the target has no `/` and exactly
//!    one note has that title, it resolves there.
//! 3. **Duplicate titles.** Otherwise the deterministic choice is the candidate
//!    with the lexicographically smallest vault-relative path
//!    (case-insensitive, then case-sensitive). It is *not* silently “first on
//!    disk”: the ordering is total and documented, and the preview always
//!    writes the full stem for nested notes so a rendered link never depends
//!    on this fallback.
//!
//! Empty targets (`[[#Heading]]`) are self-links and resolve to the note that
//! contains them.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::note::Note;
use crate::preview::excerpt;
use crate::wikilink::parse_links;

/// A folder of notes plus the index its links resolve through.
pub struct Vault {
    pub root: PathBuf,
    pub notes: Vec<Note>,
    /// Vault-relative stem (lower-cased) → note id. A path stem is unique on
    /// disk, so this map holds at most one entry per key up to case variants,
    /// where the smallest `rel` wins deterministically.
    by_path: HashMap<String, usize>,
    /// Title (lower-cased) → note ids, sorted by `rel` (deterministic).
    by_title: HashMap<String, Vec<usize>>,
}

impl Vault {
    /// A vault with no notes, for a folder that couldn't be read.
    #[must_use]
    pub fn empty(root: impl AsRef<Path>) -> Vault {
        Vault {
            root: root.as_ref().to_path_buf(),
            notes: Vec::new(),
            by_path: HashMap::new(),
            by_title: HashMap::new(),
        }
    }

    /// Scan `root` for `.md` files, creating the folder when it doesn't exist.
    pub fn load(root: impl AsRef<Path>) -> io::Result<Vault> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        let mut paths = Vec::new();
        walk(&root, &mut paths, 0);
        paths.sort_by(|a, b| {
            a.to_string_lossy()
                .to_lowercase()
                .cmp(&b.to_string_lossy().to_lowercase())
                .then_with(|| a.cmp(b))
        });

        let mut notes = Vec::new();
        for path in paths {
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };
            let Some(title) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
                continue;
            };
            if title.is_empty() {
                continue;
            }
            let rel = path
                .strip_prefix(&root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            notes.push(Note {
                id: notes.len(),
                path,
                rel,
                title,
                source,
                links: Vec::new(),
                backlinks: Vec::new(),
            });
        }

        let mut vault = Vault {
            root,
            notes,
            by_path: HashMap::new(),
            by_title: HashMap::new(),
        };
        vault.reindex();
        Ok(vault)
    }

    /// Rebuild the title index and every note's link lists.
    pub fn reindex(&mut self) {
        self.by_path.clear();
        self.by_title.clear();
        for note in &self.notes {
            // Path map: smallest rel wins on a case-variant collision.
            let key = note.stem().to_lowercase();
            match self.by_path.get(&key) {
                Some(&existing) if existing != note.id => {
                    let existing_rel = &self.notes[existing].rel;
                    if (existing_rel.to_lowercase(), existing_rel.as_str())
                        <= (note.rel.to_lowercase(), note.rel.as_str())
                    {
                        // Keep the existing (smaller) entry.
                    } else {
                        self.by_path.insert(key, note.id);
                    }
                }
                _ => {
                    self.by_path.insert(key, note.id);
                }
            }
            self.by_title
                .entry(note.title.to_lowercase())
                .or_default()
                .push(note.id);
        }
        for ids in self.by_title.values_mut() {
            ids.sort_by(|&a, &b| {
                let (ra, rb) = (&self.notes[a].rel, &self.notes[b].rel);
                ra.to_lowercase()
                    .cmp(&rb.to_lowercase())
                    .then_with(|| ra.cmp(rb))
            });
            ids.dedup();
        }
        for note in &mut self.notes {
            note.links.clear();
            note.backlinks.clear();
        }
        for i in 0..self.notes.len() {
            let mut seen = HashSet::new();
            let links = {
                let note = &self.notes[i];
                let mut links = Vec::new();
                for link in parse_links(&note.source) {
                    // Empty target is a self-link (`[[#Heading]]`); it must
                    // resolve to this note even when the title is duplicated.
                    let resolved = if link.target.is_empty() {
                        Some(i)
                    } else {
                        self.resolve(&link.target)
                    };
                    if let Some(id) = resolved
                        && id != i
                        && seen.insert(id)
                    {
                        links.push(id);
                    }
                }
                links
            };
            self.notes[i].links = links;
        }
        for i in 0..self.notes.len() {
            let links = self.notes[i].links.clone();
            for target in links {
                self.notes[target].backlinks.push(i);
            }
        }
        for note in &mut self.notes {
            note.links.sort_unstable();
            note.links.dedup();
            note.backlinks.sort_unstable();
            note.backlinks.dedup();
        }
    }

    /// Resolve a wikilink target by the documented rule: path-qualified
    /// targets first (exact stem, then unique suffix), then an unambiguous
    /// title, then the deterministic smallest-`rel` candidate for duplicates.
    #[must_use]
    pub fn resolve(&self, target: &str) -> Option<usize> {
        let target = normalise_target(target);
        if target.is_empty() {
            return None;
        }
        let lower = target.to_lowercase();
        if lower.contains('/') {
            // Path-qualified: exact stem, then unique suffix. No title
            // fallback — a wrong folder must stay missing.
            if let Some(&id) = self.by_path.get(&lower) {
                return Some(id);
            }
            let tail = format!("/{lower}");
            let mut candidates: Vec<usize> = self
                .notes
                .iter()
                .filter(|n| n.stem().to_lowercase().ends_with(&tail))
                .map(|n| n.id)
                .collect();
            if candidates.is_empty() {
                return None;
            }
            candidates.sort_by(|&a, &b| {
                let (ra, rb) = (&self.notes[a].rel, &self.notes[b].rel);
                ra.to_lowercase()
                    .cmp(&rb.to_lowercase())
                    .then_with(|| ra.cmp(rb))
            });
            return candidates.first().copied();
        }
        // Bare title.
        self.by_title
            .get(&lower)
            .and_then(|ids| ids.first().copied())
    }

    /// One note by id. The id is its index in [`Vault::notes`].
    #[must_use]
    pub fn note(&self, id: usize) -> &Note {
        &self.notes[id]
    }

    /// Distinct folders in the vault, vault root first, then sorted
    /// case-insensitively. The empty string is the vault root.
    #[must_use]
    pub fn folders(&self) -> Vec<String> {
        let mut folders: Vec<String> = self.notes.iter().map(|n| n.folder().to_owned()).collect();
        folders.sort_by(|a, b| {
            (a.is_empty(), a.to_lowercase(), a).cmp(&(b.is_empty(), b.to_lowercase(), b))
        });
        folders.dedup();
        // Root first even though `""` sorts last by the tuple above: move it
        // to the front explicitly for callers that expect root-first.
        folders.sort_by(|a, b| {
            let rank = |s: &String| if s.is_empty() { 0 } else { 1 };
            rank(a)
                .cmp(&rank(b))
                .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
                .then_with(|| a.cmp(b))
        });
        folders
    }

    /// Group note ids by folder: groups sorted root-first then by folder, ids
    /// within a group sorted by title (then `rel`) so headers and rows are
    /// stable. Empty groups are omitted.
    #[must_use]
    pub fn group_by_folder(&self, ids: &[usize]) -> Vec<(String, Vec<usize>)> {
        let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
        for &id in ids {
            let folder = self.notes[id].folder().to_owned();
            if let Some((_, group)) = groups.iter_mut().find(|(f, _)| *f == folder) {
                group.push(id);
            } else {
                groups.push((folder, vec![id]));
            }
        }
        for (_, group) in &mut groups {
            group.sort_by(|&a, &b| {
                let (na, nb) = (&self.notes[a], &self.notes[b]);
                na.title
                    .to_lowercase()
                    .cmp(&nb.title.to_lowercase())
                    .then_with(|| na.rel.to_lowercase().cmp(&nb.rel.to_lowercase()))
                    .then_with(|| na.rel.cmp(&nb.rel))
            });
        }
        groups.sort_by(|(a, _), (b, _)| {
            let rank = |s: &String| if s.is_empty() { 0 } else { 1 };
            rank(a)
                .cmp(&rank(b))
                .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
                .then_with(|| a.cmp(b))
        });
        groups
    }

    /// Note ids in `folder` (exact match, `""` for the root), sorted by title.
    #[must_use]
    pub fn notes_in_folder(&self, folder: &str) -> Vec<usize> {
        let mut ids: Vec<usize> = self
            .notes
            .iter()
            .filter(|n| n.folder() == folder)
            .map(|n| n.id)
            .collect();
        ids.sort_by(|&a, &b| {
            let (na, nb) = (&self.notes[a], &self.notes[b]);
            na.title
                .to_lowercase()
                .cmp(&nb.title.to_lowercase())
                .then_with(|| na.rel.to_lowercase().cmp(&nb.rel.to_lowercase()))
                .then_with(|| na.rel.cmp(&nb.rel))
        });
        ids
    }

    /// True when another note shares this note's title (case-insensitive).
    #[must_use]
    pub fn is_duplicate(&self, id: usize) -> bool {
        self.by_title
            .get(&self.notes[id].title.to_lowercase())
            .is_some_and(|ids| ids.len() > 1)
    }

    /// Label that identifies a note even when titles collide. Unique titles
    /// show as the plain title; duplicates show the vault-relative stem
    /// (`folder/title`), or `title (root)` at the vault root. Use this where
    /// a single line must be unambiguous (rail, graph nodes). The sidebar
    /// uses the title plus a folder subtitle for readability instead.
    #[must_use]
    pub fn display_label(&self, id: usize) -> String {
        let note = &self.notes[id];
        if !self.is_duplicate(id) {
            return note.title.clone();
        }
        let folder = note.folder();
        if folder.is_empty() {
            format!("{} (root)", note.title)
        } else {
            format!("{}/{}", folder, note.title)
        }
    }

    /// Replace one note's source on disk and in the index.
    pub fn write_note(&mut self, id: usize, source: &str) -> io::Result<()> {
        fs::write(&self.notes[id].path, source)?;
        self.notes[id].source = source.to_owned();
        self.reindex();
        Ok(())
    }

    /// Create `title.md` (or `title 2.md`, …) at the vault root, returning
    /// its path. Kept for callers that mean “at the root”; new notes created
    /// from a selected note or folder should use [`Vault::create_note_in`].
    pub fn create_note(&self, title: &str) -> io::Result<PathBuf> {
        self.create_note_in(title, "")
    }

    /// Create `title.md` (or `title 2.md`, …) inside `folder` (a
    /// vault-relative directory, `""` for the root), returning its path.
    /// The folder is created when missing. The de-duplication counter is per
    /// folder, so `folder/Untitled.md` and `folder/Untitled 2.md` are
    /// independent of the root.
    pub fn create_note_in(&self, title: &str, folder: &str) -> io::Result<PathBuf> {
        let title = sanitise_title(title);
        let dir = folder_dir(&self.root, folder);
        fs::create_dir_all(&dir)?;
        let mut n = 1;
        loop {
            let name = if n == 1 {
                format!("{title}.md")
            } else {
                format!("{title} {n}.md")
            };
            let path = dir.join(name);
            if !path.exists() {
                fs::write(&path, format!("# {title}\n\n"))?;
                return Ok(path);
            }
            n += 1;
        }
    }

    /// A one-line excerpt of `from` around its link to `target`, for the backlinks rail.
    #[must_use]
    pub fn snippet(&self, from: usize, target: usize) -> String {
        let note = &self.notes[from];
        for link in parse_links(&note.source) {
            // Self-links resolve to the containing note even for duplicates.
            let resolved = if link.target.is_empty() {
                Some(from)
            } else {
                self.resolve(&link.target)
            };
            if resolved != Some(target) {
                continue;
            }
            let start = note.source[..link.start].rfind('\n').map_or(0, |i| i + 1);
            let end = note.source[link.end..]
                .find('\n')
                .map_or(note.source.len(), |i| link.end + i);
            return excerpt(&note.source[start..end]);
        }
        excerpt(&note.source)
    }
}

/// Normalise a resolve/search target the same way `parse_links` does before
/// the vault sees it: trim, unify separators, drop a leading `./` or `/` and
/// a `.md` suffix, and stop at a `#heading`.
fn normalise_target(target: &str) -> String {
    let mut target = target.trim().replace('\\', "/");
    // Headings are not part of the note identity (defensive: callers that
    // already split them are unaffected).
    if let Some(at) = target.find('#') {
        target.truncate(at);
    }
    while let Some(rest) = target.strip_prefix("./") {
        target = rest.to_owned();
    }
    while let Some(rest) = target.strip_prefix('/') {
        target = rest.to_owned();
    }
    while target.ends_with('/') && target.len() > 1 {
        target.pop();
    }
    if target.to_ascii_lowercase().ends_with(".md") {
        target.truncate(target.len() - 3);
    }
    target.trim().to_owned()
}

/// Absolute directory for a vault-relative `folder` (`""` = root). Dot
/// segments are dropped so a note can never be created where the walk would
/// immediately hide it, and `..` can never escape the root.
fn folder_dir(root: &Path, folder: &str) -> PathBuf {
    let mut dir = root.to_path_buf();
    for part in folder.replace('\\', "/").split('/') {
        let part = part.trim();
        if part.is_empty() || part == "." || part == ".." || part.starts_with('.') {
            continue;
        }
        dir.push(part);
    }
    dir
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 12 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk(&path, out, depth + 1);
        } else if kind.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            out.push(path);
        }
    }
}

fn sanitise_title(title: &str) -> String {
    let mut title: String = title
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect();
    while title.ends_with('.') || title.ends_with(' ') {
        title.pop();
    }
    if title.is_empty() {
        "Untitled".to_owned()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempVault;

    #[test]
    fn links_resolve_by_title_path_and_case() {
        let tmp = TempVault::new(&[
            (
                "Alpha.md",
                "to [[Beta]] and [[folder/Gamma.md]] and [[delta|Dee]]",
            ),
            ("Beta.md", "back to [[alpha]]"),
            ("folder/Gamma.md", "gamma"),
            ("delta.md", "delta"),
        ]);
        let v = tmp.load();
        let alpha = v.resolve("Alpha").expect("Alpha");
        assert_eq!(v.note(alpha).links.len(), 3);
        assert!(v.note(alpha).links.contains(&v.resolve("Beta").unwrap()));
        assert!(v.note(alpha).links.contains(&v.resolve("gamma").unwrap()));
        assert!(v.note(alpha).links.contains(&v.resolve("delta").unwrap()));
        let beta = v.resolve("BETA").unwrap();
        assert_eq!(v.note(beta).backlinks, vec![alpha]);
    }

    #[test]
    fn created_notes_get_unique_names() {
        let tmp = TempVault::new(&[]);
        let v = tmp.load();
        let a = v.create_note("Untitled").unwrap();
        let b = v.create_note("Untitled").unwrap();
        assert_eq!(a.file_name().unwrap(), "Untitled.md");
        assert_eq!(b.file_name().unwrap(), "Untitled 2.md");
        assert!(fs::read_to_string(&a).unwrap().starts_with("# Untitled"));
    }

    #[test]
    fn duplicate_titles_resolve_deterministically_and_path_wins() {
        let tmp = TempVault::new(&[
            ("a/_Index_.md", "a index"),
            ("b/_Index_.md", "b index"),
            (
                "Home.md",
                "go [[_Index_]] and [[b/_Index_]] and [[a/_Index_]]",
            ),
        ]);
        let v = tmp.load();
        let a = v.resolve("a/_Index_").expect("a path");
        let b = v.resolve("b/_Index_").expect("b path");
        assert_ne!(a, b);
        assert_eq!(v.note(a).folder(), "a");
        assert_eq!(v.note(b).folder(), "b");
        // Bare duplicate: smallest rel wins ("a/_Index_.md" < "b/_Index_.md").
        let bare = v.resolve("_Index_").expect("bare");
        assert_eq!(bare, a);
        // Suffix path still finds the right folder.
        assert_eq!(v.resolve("b/_Index_").unwrap(), b);
        // Wrong folder never falls back to a same-titled note.
        assert_eq!(v.resolve("c/_Index_"), None);
        // Home links: bare goes to `a`, both paths stay distinct.
        let home = v.resolve("Home").unwrap();
        let links = &v.note(home).links;
        assert!(links.contains(&a) && links.contains(&b), "{links:?}");
        // Duplicates are labelled with their folder.
        assert!(v.is_duplicate(a) && v.is_duplicate(b));
        assert_eq!(v.display_label(a), "a/_Index_");
        assert_eq!(v.display_label(b), "b/_Index_");
        assert!(!v.is_duplicate(home));
        assert_eq!(v.display_label(home), "Home");
    }

    #[test]
    fn self_links_stay_on_their_own_duplicate() {
        let tmp = TempVault::new(&[("a/_Index_.md", "a [[#Local]]"), ("b/_Index_.md", "b")]);
        let v = tmp.load();
        let a = v.resolve("a/_Index_").unwrap();
        let b = v.resolve("b/_Index_").unwrap();
        // The self-link produces no self-edge, and the other duplicate must
        // have no link from `a` via the bare title.
        assert!(v.note(a).links.is_empty(), "{:?}", v.note(a).links);
        assert!(v.note(b).backlinks.is_empty());
    }

    #[test]
    fn folders_group_notes_and_creation_targets_a_folder() {
        let tmp = TempVault::new(&[
            ("Welcome.md", "root"),
            ("projects/alpha/note.md", "alpha"),
            ("projects/alpha/overview.md", "over"),
            ("projects/beta/note.md", "beta"),
            (".hidden/notes/hidden.md", "hidden"),
            ("nested/readme.md", "nested readme"),
            ("README.md", "root readme"),
        ]);
        let v = tmp.load();
        // Dot-dirs stay ignored.
        assert!(v.resolve("hidden").is_none());
        let folders = v.folders();
        assert_eq!(folders[0], "", "{folders:?}");
        assert!(folders.contains(&"projects/alpha".to_owned()));
        assert!(folders.contains(&"nested".to_owned()));
        let groups = v.group_by_folder(&v.notes.iter().map(|n| n.id).collect::<Vec<_>>());
        let root_group = groups
            .iter()
            .find(|(f, _)| f.is_empty())
            .expect("root group");
        assert!(root_group.1.iter().any(|&id| v.note(id).title == "Welcome"));
        let alpha = groups
            .iter()
            .find(|(f, _)| f == "projects/alpha")
            .expect("alpha group");
        assert_eq!(alpha.1.len(), 2);
        // Case-variant readme duplicates are distinguishable; the bare title
        // goes to the smallest rel ("nested/…" < "README…" case-insensitively).
        let bare = v
            .resolve("README")
            .expect("bare readme resolves deterministically");
        assert_eq!(v.note(bare).rel, "nested/readme.md");
        assert!(v.is_duplicate(bare));
        assert_eq!(v.display_label(bare), "nested/readme");
        let root_readme = v.resolve("nested/README").expect("nested path");
        assert_eq!(v.note(root_readme).rel, "nested/readme.md");
        let root_id = v.notes.iter().find(|n| n.rel == "README.md").unwrap().id;
        assert!(v.is_duplicate(root_id));
        assert_eq!(v.display_label(root_id), "README (root)");
        // Creation in a folder is independent of the root.
        let created = v.create_note_in("Draft", "projects/alpha").unwrap();
        assert!(created.ends_with("projects/alpha/Draft.md"));
        let again = v.create_note_in("Draft", "projects/alpha").unwrap();
        assert!(again.ends_with("projects/alpha/Draft 2.md"));
        let root_created = v.create_note("Draft").unwrap();
        assert!(root_created.ends_with("Draft.md"));
    }
}
