//! Scan a folder of Markdown files and index the links between them.
//!
//! Every `.md` file under the root is a note, its title is the file stem, and a link resolves
//! by title or vault-relative path, case-insensitively.

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
    index: HashMap<String, usize>,
}

impl Vault {
    /// A vault with no notes, for a folder that couldn't be read.
    #[must_use]
    pub fn empty(root: impl AsRef<Path>) -> Vault {
        Vault {
            root: root.as_ref().to_path_buf(),
            notes: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Scan `root` for `.md` files, creating the folder when it doesn't exist.
    pub fn load(root: impl AsRef<Path>) -> io::Result<Vault> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        let mut paths = Vec::new();
        walk(&root, &mut paths, 0);
        paths.sort_by_key(|p| p.to_string_lossy().to_lowercase());

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
            index: HashMap::new(),
        };
        vault.reindex();
        Ok(vault)
    }

    /// Rebuild the title index and every note's link lists.
    pub fn reindex(&mut self) {
        self.index.clear();
        for note in &self.notes {
            self.index
                .entry(note.title.to_lowercase())
                .or_insert(note.id);
            let stem = note.rel.strip_suffix(".md").unwrap_or(&note.rel);
            self.index.entry(stem.to_lowercase()).or_insert(note.id);
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
                    let target = if link.target.is_empty() {
                        note.title.clone()
                    } else {
                        link.target.clone()
                    };
                    if let Some(id) = self.resolve(&target)
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

    /// Resolve a wikilink target by title, vault-relative path or bare filename.
    #[must_use]
    pub fn resolve(&self, target: &str) -> Option<usize> {
        let target = target.trim().replace('\\', "/");
        if target.is_empty() {
            return None;
        }
        let lower = target.to_lowercase();
        if let Some(&id) = self.index.get(&lower) {
            return Some(id);
        }
        let stem = lower.strip_suffix(".md").unwrap_or(&lower);
        if let Some(&id) = self.index.get(stem) {
            return Some(id);
        }
        let last = stem.rsplit('/').next().unwrap_or(stem);
        self.index.get(last).copied()
    }

    /// One note by id. The id is its index in [`Vault::notes`].
    #[must_use]
    pub fn note(&self, id: usize) -> &Note {
        &self.notes[id]
    }

    /// Replace one note's source on disk and in the index.
    pub fn write_note(&mut self, id: usize, source: &str) -> io::Result<()> {
        fs::write(&self.notes[id].path, source)?;
        self.notes[id].source = source.to_owned();
        self.reindex();
        Ok(())
    }

    /// Create `title.md` (or `title 2.md`, …) with a heading, returning its path.
    pub fn create_note(&self, title: &str) -> io::Result<PathBuf> {
        let title = sanitise_title(title);
        let mut n = 1;
        loop {
            let name = if n == 1 {
                format!("{title}.md")
            } else {
                format!("{title} {n}.md")
            };
            let path = self.root.join(name);
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
            let target_title = if link.target.is_empty() {
                note.title.clone()
            } else {
                link.target.clone()
            };
            if self.resolve(&target_title) != Some(target) {
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
}
