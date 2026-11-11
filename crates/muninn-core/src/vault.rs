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
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Distinguishes concurrent temp files from the same process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

use crate::note::Note;
use crate::preview::excerpt;
use crate::wikilink::parse_links;

/// A missing wikilink target and the notes that refer to it.
///
/// Built by [`Vault::reindex`] so the broken-links dashboard reads a cache
/// instead of re-parsing every note each frame. Targets are grouped
/// case-insensitively; `target` keeps the first spelling seen in note order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrokenLink {
    pub target: String,
    /// Referrer note ids, unique and sorted.
    pub referrers: Vec<usize>,
}

/// A folder of notes plus the index its links resolve through.
pub struct Vault {
    pub root: PathBuf,
    pub notes: Vec<Note>,
    /// Every non-dot directory under the root (vault-relative, `""` for the
    /// root), including empty folders. Folders themselves are part of the
    /// model so the sidebar can show and target empty ones.
    pub dirs: Vec<String>,
    /// Vault-relative stem (lower-cased) → note id. A path stem is unique on
    /// disk, so this map holds at most one entry per key up to case variants,
    /// where the smallest `rel` wins deterministically.
    by_path: HashMap<String, usize>,
    /// Title (lower-cased) → note ids, sorted by `rel` (deterministic).
    by_title: HashMap<String, Vec<usize>>,
    /// `.md` files the last scan found but could not read (invalid UTF-8,
    /// permissions, …). They are deliberately absent from the index instead of
    /// being silently treated as empty notes; the UI surfaces the count.
    skipped: usize,
    /// NAV-02: unresolved link targets, grouped, with referrers. Rebuilt by
    /// [`Vault::reindex`].
    broken: Vec<BrokenLink>,
}

/// An in-progress chunked scan started by [`Vault::scan_start`].
///
/// The folder walk and path sort happen immediately (metadata only); note
/// bodies are read in bounded chunks with [`ScanJob::read_chunk`], and the link
/// index is rebuilt once in [`ScanJob::finish`] because links need the global
/// title/path map. The result is identical to [`Vault::load`].
pub struct ScanJob {
    root: PathBuf,
    paths: Vec<PathBuf>,
    dirs: Vec<String>,
    /// Index of the next path to read; also the number of files processed.
    next: usize,
    notes: Vec<Note>,
    skipped: usize,
}

impl ScanJob {
    /// Every `.md` file the walk found, including ones a read will skip.
    #[must_use]
    pub fn total(&self) -> usize {
        self.paths.len()
    }

    /// Files processed so far (read or skipped). Progress is `done()/total()`.
    #[must_use]
    pub fn done(&self) -> usize {
        self.next
    }

    /// True once every path has been read (or skipped).
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.next >= self.paths.len()
    }

    /// The vault root this job scans.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Read up to `limit` note files, returning how many were processed. A
    /// skipped file (invalid UTF-8, permissions, unusable name) counts toward
    /// progress and [`Vault::skipped_count`], exactly as in a full load.
    pub fn read_chunk(&mut self, limit: usize) -> usize {
        let end = self.next.saturating_add(limit).min(self.paths.len());
        let mut read = 0;
        while self.next < end {
            let path = self.paths[self.next].clone();
            match note_from_path(&self.root, &path, self.notes.len()) {
                Some(note) => self.notes.push(note),
                None => self.skipped += 1,
            }
            self.next += 1;
            read += 1;
        }
        read
    }

    /// Build the vault and resolve every link. Any unread paths are read first,
    /// so `finish` is safe even mid-scan (it is the synchronous path).
    #[must_use]
    pub fn finish(mut self) -> Vault {
        if !self.is_done() {
            self.read_chunk(usize::MAX);
        }
        let mut vault = Vault {
            root: self.root,
            notes: self.notes,
            dirs: self.dirs,
            by_path: HashMap::new(),
            by_title: HashMap::new(),
            skipped: self.skipped,
            broken: Vec::new(),
        };
        vault.reindex();
        vault
    }
}

impl Vault {
    /// Hidden folder, directly under the vault root, where deleted notes are
    /// moved. Starting with `.` means [`Vault::load`]'s walk ignores it, so a
    /// trashed note never reappears as a live note.
    pub const TRASH_DIR: &str = ".muninn-trash";

    /// How many trashed files are kept before the oldest timestamp folder is
    /// pruned. Best-effort and applied after each successful trash move.
    pub const TRASH_MAX_ENTRIES: usize = 500;

    /// Aggregate trash size kept before the oldest timestamp folder is pruned
    /// (100 MiB). Best-effort, like [`Vault::TRASH_MAX_ENTRIES`].
    pub const TRASH_MAX_BYTES: u64 = 100 * 1024 * 1024;

    /// A vault with no notes, for a folder that couldn't be read.
    #[must_use]
    pub fn empty(root: impl AsRef<Path>) -> Vault {
        Vault {
            root: root.as_ref().to_path_buf(),
            notes: Vec::new(),
            dirs: vec![String::new()],
            by_path: HashMap::new(),
            by_title: HashMap::new(),
            skipped: 0,
            broken: Vec::new(),
        }
    }

    /// Scan `root` for `.md` files, creating the folder when it doesn't exist.
    /// Dot-directories are skipped for both notes and the folder list.
    ///
    /// A missing root is created empty (the app starts with an empty vault),
    /// but a root that exists and is not a readable folder is an error. That
    /// distinction matters: silently returning an empty vault for a permission
    /// problem would make the UI claim "No notes yet" instead of surfacing the
    /// OS error.
    pub fn load(root: impl AsRef<Path>) -> io::Result<Vault> {
        let mut job = Self::scan_start(root)?;
        job.read_chunk(usize::MAX);
        Ok(job.finish())
    }

    /// Begin a chunked scan: resolve the root, walk it once, and collect the
    /// note paths and folders. Reading the note bodies — the expensive part —
    /// is deferred to [`ScanJob::read_chunk`], so a UI can show progress without
    /// a background thread.
    ///
    /// The result is the same as [`Vault::load`] once every chunk is read and
    /// [`ScanJob::finish`] reindexes. A missing root is created and an
    /// unreadable root is still an error (see [`Vault::load`]).
    pub fn scan_start(root: impl AsRef<Path>) -> io::Result<ScanJob> {
        let root = root.as_ref().to_path_buf();
        prepare_root(&root)?;
        let mut paths = Vec::new();
        let mut dirs = Vec::new();
        walk(&root, &mut paths, &mut dirs, &root, 0, "md");
        dirs.sort_by(|a, b| {
            a.to_lowercase()
                .cmp(&b.to_lowercase())
                .then_with(|| a.cmp(b))
        });
        dirs.dedup();
        sort_paths(&mut paths);
        Ok(ScanJob {
            root,
            paths,
            dirs,
            next: 0,
            notes: Vec::new(),
            skipped: 0,
        })
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
        let mut broken = HashMap::new();
        for i in 0..self.notes.len() {
            self.notes[i].links = self.note_outgoing(i, &mut broken);
        }
        self.broken = group_broken(broken);
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

    /// Resolve one note's outgoing links, recording each unresolved target in
    /// `broken` (keyed case-insensitively, first spelling kept).
    fn note_outgoing(
        &self,
        i: usize,
        broken: &mut HashMap<String, (String, Vec<usize>)>,
    ) -> Vec<usize> {
        let note = &self.notes[i];
        let mut seen = HashSet::new();
        let mut links = Vec::new();
        for link in parse_links(&note.source) {
            // Empty target is a self-link (`[[#Heading]]`); it must resolve to
            // this note even when the title is duplicated.
            let resolved = if link.target.is_empty() {
                Some(i)
            } else {
                self.resolve(&link.target)
            };
            match resolved {
                Some(id) => {
                    if id != i && seen.insert(id) {
                        links.push(id);
                    }
                }
                None => {
                    let entry = broken
                        .entry(link.target.to_lowercase())
                        .or_insert_with(|| (link.target.clone(), Vec::new()));
                    entry.1.push(i);
                }
            }
        }
        links
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

    /// How many `.md` files the last scan found but could not index (invalid
    /// UTF-8, unreadable file, unusable file name). Zero when the scan was
    /// complete. Use it to warn without failing the whole vault.
    #[must_use]
    pub fn skipped_count(&self) -> usize {
        self.skipped
    }

    /// Distinct folders in the vault, vault root first, then sorted
    /// case-insensitively. The empty string is the vault root. Includes
    /// empty folders (from [`Vault::dirs`]), not just note folders.
    #[must_use]
    pub fn folders(&self) -> Vec<String> {
        let mut folders = self.dirs.clone();
        for folder in self.notes.iter().map(|n| n.folder()) {
            if !folders.iter().any(|f| f == folder) {
                folders.push(folder.to_owned());
            }
        }
        folders.sort_by(|a, b| {
            let rank = |s: &String| if s.is_empty() { 0 } else { 1 };
            rank(a)
                .cmp(&rank(b))
                .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
                .then_with(|| a.cmp(b))
        });
        folders.dedup();
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

    /// NAV-02: unresolved link targets grouped with their referrers, rebuilt by
    /// [`Vault::reindex`]. The dashboard reads this cache; it never scans notes
    /// per frame.
    #[must_use]
    pub fn broken_links(&self) -> &[BrokenLink] {
        &self.broken
    }

    /// EDT-03: note ids whose title or vault-relative stem matches `query`
    /// case-insensitively, best first, capped at `cap`.
    ///
    /// Ranking is title-prefix, stem-prefix, title-contains, stem-contains, then
    /// title and path for stability. An empty query lists every note in title
    /// order, which is what the explicit insert-link picker shows.
    #[must_use]
    pub fn suggest_links(&self, query: &str, cap: usize) -> Vec<usize> {
        let query = query.trim().to_lowercase();
        let mut ranked: Vec<(u8, usize)> = Vec::new();
        for note in &self.notes {
            let title = note.title.to_lowercase();
            let stem = note.stem().to_lowercase();
            let score = if query.is_empty() || title.starts_with(&query) {
                0
            } else if stem.starts_with(&query) {
                1
            } else if title.contains(&query) {
                2
            } else if stem.contains(&query) {
                3
            } else {
                continue;
            };
            ranked.push((score, note.id));
        }
        ranked.sort_by(|&(sa, a), &(sb, b)| {
            let (na, nb) = (&self.notes[a], &self.notes[b]);
            sa.cmp(&sb)
                .then_with(|| na.title.to_lowercase().cmp(&nb.title.to_lowercase()))
                .then_with(|| na.rel.to_lowercase().cmp(&nb.rel.to_lowercase()))
                .then_with(|| na.rel.cmp(&nb.rel))
        });
        ranked.truncate(cap);
        ranked.into_iter().map(|(_, id)| id).collect()
    }

    /// SAF-06: notes that link to `id`, with the distinct target spellings that
    /// resolve there. Computed before a rename or move so the rewrite knows
    /// exactly which links pointed at this note (a bare duplicate title is only
    /// included when it resolved here under the documented rule).
    #[must_use]
    pub fn link_referrers(&self, id: usize) -> Vec<(usize, Vec<String>)> {
        let mut out = Vec::new();
        for note in &self.notes {
            if note.id == id {
                continue;
            }
            let mut targets: Vec<String> = Vec::new();
            for link in parse_links(&note.source) {
                if link.target.is_empty() {
                    continue;
                }
                if self.resolve(&link.target) == Some(id)
                    && !targets.iter().any(|t| t == &link.target)
                {
                    targets.push(link.target.clone());
                }
            }
            if !targets.is_empty() {
                out.push((note.id, targets));
            }
        }
        out
    }

    /// SAF-06: notes with at least one link whose target currently resolves to
    /// a note under `folder` (its stem starts with `folder/`). Used before a
    /// folder rename to decide whether the link-impact prompt is needed.
    #[must_use]
    pub fn link_referrers_for_folder(&self, folder: &str) -> Vec<usize> {
        let folder = folder.trim_matches('/');
        if folder.is_empty() {
            return Vec::new();
        }
        let prefix = format!("{}/", folder.to_lowercase());
        let mut out = Vec::new();
        for note in &self.notes {
            let refers = parse_links(&note.source).into_iter().any(|link| {
                !link.target.is_empty()
                    && self
                        .resolve(&link.target)
                        .is_some_and(|id| self.notes[id].stem().to_lowercase().starts_with(&prefix))
            });
            if refers {
                out.push(note.id);
            }
        }
        out
    }

    /// SAF-06: rewrite inbound wikilinks to `new_stem`, preserving each link's
    /// heading and alias. `inbound` is the pre-mutation result of
    /// [`Vault::link_referrers`].
    ///
    /// Each changed file is written through [`Vault::write_note`], so every
    /// file replaces its old bytes atomically and the index is rebuilt. The
    /// rewrite is *not* a cross-file transaction: if one file fails, the others
    /// stay rewritten and the failure is returned as `(referrer id, error)`.
    /// Callers surface those errors and must not roll back the rename/move.
    pub fn rewrite_inbound_links(
        &mut self,
        inbound: &[(usize, Vec<String>)],
        new_stem: &str,
    ) -> Vec<(usize, io::Error)> {
        let mut failures = Vec::new();
        for (from, targets) in inbound {
            let Some(note) = self.notes.get(*from) else {
                continue;
            };
            let source = note.source.clone();
            let mut out = String::with_capacity(source.len());
            let mut last = 0;
            let mut changed = false;
            for link in parse_links(&source) {
                if link.target.is_empty() || !targets.iter().any(|t| t.as_str() == link.target) {
                    continue;
                }
                out.push_str(&source[last..link.start]);
                out.push_str("[[");
                out.push_str(new_stem);
                if let Some(heading) = &link.heading {
                    out.push('#');
                    out.push_str(heading);
                }
                if let Some(alias) = &link.alias {
                    out.push('|');
                    out.push_str(alias);
                }
                out.push_str("]]");
                last = link.end;
                changed = true;
            }
            if !changed {
                continue;
            }
            out.push_str(&source[last..]);
            if let Err(error) = self.write_note(*from, &out) {
                failures.push((*from, error));
            }
        }
        failures
    }

    /// SAF-06: rewrite wikilinks that pointed at notes inside a folder that was
    /// just renamed from `old_folder` to `new_folder`.
    ///
    /// Call this **after** [`Vault::rename_folder`]. The notes are already at
    /// their new paths, so resolution is replayed against the pre-rename layout:
    /// every link whose target resolved (by path, unique suffix, or bare title)
    /// to a note that used to live under `old_folder` is pointed at that note's
    /// new full stem. This also upgrades a bare title that resolved under the
    /// old folder to the canonical path-qualified form. Headings and aliases are
    /// preserved.
    ///
    /// Files are written one at a time through [`Vault::write_note`] (atomic per
    /// file). A failure leaves the folder rename in place and is returned as
    /// `(referrer id, error)` for the caller to surface; there is no rollback.
    pub fn rewrite_inbound_links_for_folder(
        &mut self,
        old_folder: &str,
        new_folder: &str,
    ) -> Vec<(usize, io::Error)> {
        let old_folder = old_folder.trim_matches('/');
        let new_folder = new_folder.trim_matches('/');
        if old_folder.is_empty() || old_folder == new_folder {
            return Vec::new();
        }
        let old_prefix = format!("{old_folder}/");
        let new_prefix = format!("{new_folder}/");
        let old_prefix_lower = old_prefix.to_lowercase();
        // Reconstruct the pre-rename rel for every note. Only notes now under
        // the new folder moved; everything else kept its path.
        let old_rels: Vec<String> = self
            .notes
            .iter()
            .map(|note| match note.rel.strip_prefix(&new_prefix) {
                Some(rest) => format!("{old_prefix}{rest}"),
                None => note.rel.clone(),
            })
            .collect();
        let old_stems: Vec<String> = old_rels
            .iter()
            .map(|rel| rel.strip_suffix(".md").unwrap_or(rel).to_owned())
            .collect();

        // Rewrite in memory first: `write_note` reindexes, which would change
        // the live resolution this pass is deliberately replaying.
        let mut edits = Vec::new();
        for note in &self.notes {
            let mut out = String::with_capacity(note.source.len());
            let mut last = 0;
            let mut changed = false;
            for link in parse_links(&note.source) {
                if link.target.is_empty() {
                    continue;
                }
                let Some(winner) = self.resolve_in_old_layout(&link.target, &old_rels, &old_stems)
                else {
                    continue;
                };
                if !old_stems[winner]
                    .to_lowercase()
                    .starts_with(&old_prefix_lower)
                {
                    continue;
                }
                let new_stem = self.notes[winner].stem();
                if link.target == new_stem {
                    continue;
                }
                out.push_str(&note.source[last..link.start]);
                out.push_str("[[");
                out.push_str(new_stem);
                if let Some(heading) = &link.heading {
                    out.push('#');
                    out.push_str(heading);
                }
                if let Some(alias) = &link.alias {
                    out.push('|');
                    out.push_str(alias);
                }
                out.push_str("]]");
                last = link.end;
                changed = true;
            }
            if changed {
                out.push_str(&note.source[last..]);
                edits.push((note.id, out));
            }
        }

        let mut failures = Vec::new();
        for (id, source) in edits {
            if let Err(error) = self.write_note(id, &source) {
                failures.push((id, error));
            }
        }
        failures
    }

    /// Resolve `target` against a layout captured before a folder rename.
    /// Mirrors [`Vault::resolve`] (exact path stem, then unique suffix, then
    /// bare title, deterministic smallest `rel`) but uses `old_rels`/`old_stems`
    /// for the moved notes. Titles do not change when a folder is renamed.
    fn resolve_in_old_layout(
        &self,
        target: &str,
        old_rels: &[String],
        old_stems: &[String],
    ) -> Option<usize> {
        let target = normalise_target(target);
        if target.is_empty() {
            return None;
        }
        let lower = target.to_lowercase();
        let mut best: Option<usize> = None;
        let consider = |id: usize, best: &mut Option<usize>| {
            let better = best.is_none_or(|current| {
                let (a, b) = (&old_rels[id], &old_rels[current]);
                (a.to_lowercase(), a.as_str()) < (b.to_lowercase(), b.as_str())
            });
            if better {
                *best = Some(id);
            }
        };
        if lower.contains('/') {
            for (id, stem) in old_stems.iter().enumerate() {
                if stem.to_lowercase() == lower {
                    consider(id, &mut best);
                }
            }
            if best.is_some() {
                return best;
            }
            let tail = format!("/{lower}");
            for (id, stem) in old_stems.iter().enumerate() {
                if stem.to_lowercase().ends_with(&tail) {
                    consider(id, &mut best);
                }
            }
        } else {
            for note in &self.notes {
                if note.title.to_lowercase() == lower {
                    consider(note.id, &mut best);
                }
            }
        }
        best
    }

    /// Replace one note's source atomically on disk and in the index.
    ///
    /// The in-memory note and the index are updated only after the rename
    /// succeeds, so a failed write leaves both the file and `note.source`
    /// unchanged.
    pub fn write_note(&mut self, id: usize, source: &str) -> io::Result<()> {
        atomic_write(&self.notes[id].path, source.as_bytes())?;
        self.notes[id].source = source.to_owned();
        self.reindex();
        Ok(())
    }

    /// SAF-02 prerequisite: a note file's on-disk identity `(mtime, length)`.
    ///
    /// The conflict flow is not built yet. TODO(SAF-02): a later lane should
    /// capture this before an edit and call
    /// `write_note_if_unmodified(id, source, expected)` before overwriting;
    /// when the current state differs, offer keep-mine/reload instead of
    /// silently replacing the other writer's change.
    pub fn disk_state(&self, id: usize) -> io::Result<(Option<SystemTime>, u64)> {
        let meta = fs::metadata(&self.notes[id].path)?;
        Ok((meta.modified().ok(), meta.len()))
    }

    /// SAF-02 prerequisite: write only when the disk still matches `expected`.
    ///
    /// Returns `Ok(false)` when another writer changed the file first; the
    /// file, the note and the index are left untouched in that case. The
    /// check is best-effort (a writer can still race between the stat and the
    /// rename), which is enough to avoid clobbering a git checkout or an
    /// external editor in normal use.
    pub fn write_note_if_unmodified(
        &mut self,
        id: usize,
        source: &str,
        expected: (Option<SystemTime>, u64),
    ) -> io::Result<bool> {
        if self.disk_state(id)? != expected {
            return Ok(false);
        }
        self.write_note(id, source)?;
        Ok(true)
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
                let body = format!("# {title}\n\n");
                atomic_write(&path, body.as_bytes())?;
                return Ok(path);
            }
            n += 1;
        }
    }

    /// Board slice 8: create an empty board named `title` (sanitised like a
    /// note title) in `folder`, as `Title.excalidraw`, or `Title 2…` when
    /// that name is taken; never overwrites. The folder is resolved like a
    /// note's (`..` and hidden parts dropped), and a folder that resolves
    /// outside the vault (a symlink) is refused.
    pub fn create_board_in(&self, title: &str, folder: &str) -> io::Result<PathBuf> {
        let title = sanitise_title(title);
        let dir = folder_dir(&self.root, folder);
        let root = fs::canonicalize(&self.root)?;
        let outside = || {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} is outside the vault", dir.display()),
            )
        };
        // Check containment before making any folder: the deepest part of
        // `dir` that exists must resolve inside the vault, or a link out of
        // it would get new folders created on the far side.
        let existing = dir.ancestors().find(|a| a.exists()).unwrap_or(&self.root);
        if !fs::canonicalize(existing)?.starts_with(&root) {
            return Err(outside());
        }
        fs::create_dir_all(&dir)?;
        // And again once it exists, in case a part changed in between.
        if !fs::canonicalize(&dir)?.starts_with(&root) {
            return Err(outside());
        }
        let body = crate::Board::new().to_json();
        let mut n = 1;
        loop {
            let name = if n == 1 {
                format!("{title}.excalidraw")
            } else {
                format!("{title} {n}.excalidraw")
            };
            let path = dir.join(name);
            if !path.exists() {
                atomic_write(&path, body.as_bytes())?;
                return Ok(path);
            }
            n += 1;
        }
    }

    /// Board rename: rename the board at `path` to `new_name` in its own
    /// folder, returning the new path. The name is checked by [`board_name`]
    /// (refused, never rewritten) and `.excalidraw` stays implicit. The board
    /// must be a plain file inside the vault (no symlink, no folder that
    /// resolves outside it), an existing file is never overwritten, and the
    /// rename itself is one `fs::rename`, so the file is never half-moved.
    /// Renaming to the same name returns the path unchanged.
    pub fn rename_board(&self, path: &Path, new_name: &str) -> io::Result<PathBuf> {
        let name = board_name(new_name)?;
        let outside = || {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} is not a board inside the vault", path.display()),
            )
        };
        let root = fs::canonicalize(&self.root)?;
        let rel = path.strip_prefix(&self.root).map_err(|_| outside())?;
        if !rel
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err(outside());
        }
        let dir = path.parent().ok_or_else(outside)?;
        if !fs::canonicalize(dir)?.starts_with(&root) {
            return Err(outside());
        }
        let meta = fs::symlink_metadata(path)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(outside());
        }
        let target = dir.join(format!("{name}.{}", crate::BOARD_EXTENSION));
        if target == path {
            return Ok(target);
        }
        // A case-only rename on a case-insensitive disk "exists" as the same
        // file; anything else there is someone else's and is kept.
        if fs::symlink_metadata(&target).is_ok()
            && fs::canonicalize(&target).ok() != fs::canonicalize(path).ok()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{name}.{} already exists", crate::BOARD_EXTENSION),
            ));
        }
        fs::rename(path, &target)?;
        Ok(target)
    }

    /// Board rename: notes whose `![[Board.excalidraw]]` embeds (or plain
    /// `[[Board.excalidraw]]` links) resolve to the board at `board`, with the
    /// distinct target spellings that do. The board-file counterpart of
    /// [`Vault::link_referrers`]: computed before the rename, while the old
    /// path still resolves, and handed to [`Vault::rewrite_board_embeds`].
    #[must_use]
    pub fn board_embed_referrers(&self, board: &Path) -> Vec<(usize, Vec<String>)> {
        let Ok(want) = fs::canonicalize(board) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for note in &self.notes {
            let mut targets: Vec<String> = Vec::new();
            for link in parse_links(&note.source) {
                let name = link.target.rsplit(['/', '\\']).next().unwrap_or("");
                if !crate::preview::is_board_name(name) || targets.contains(&link.target) {
                    continue;
                }
                let here = self
                    .attachment_path(&link.target, note.id)
                    .and_then(|p| fs::canonicalize(p).ok());
                if here.as_ref() == Some(&want) {
                    targets.push(link.target.clone());
                }
            }
            if !targets.is_empty() {
                out.push((note.id, targets));
            }
        }
        out
    }

    /// Board rename: point the links found by [`Vault::board_embed_referrers`]
    /// at the board's new file name (`new_path`), keeping each one's folder
    /// part, heading and alias. Written like [`Vault::rewrite_inbound_links`]:
    /// one atomic [`Vault::write_note`] per file, failures returned, no
    /// rollback of the rename.
    pub fn rewrite_board_embeds(
        &mut self,
        inbound: &[(usize, Vec<String>)],
        new_path: &Path,
    ) -> Vec<(usize, io::Error)> {
        let Some(file) = new_path.file_name().and_then(|n| n.to_str()) else {
            return Vec::new();
        };
        let mut failures = Vec::new();
        for (from, targets) in inbound {
            let Some(note) = self.notes.get(*from) else {
                continue;
            };
            let source = note.source.clone();
            let mut out = String::with_capacity(source.len());
            let mut last = 0;
            let mut changed = false;
            for link in parse_links(&source) {
                if !targets.iter().any(|t| t.as_str() == link.target) {
                    continue;
                }
                let folder = match link.target.rfind(['/', '\\']) {
                    Some(at) => &link.target[..=at],
                    None => "",
                };
                out.push_str(&source[last..link.start]);
                out.push_str("[[");
                out.push_str(folder);
                out.push_str(file);
                if let Some(heading) = &link.heading {
                    out.push('#');
                    out.push_str(heading);
                }
                if let Some(alias) = &link.alias {
                    out.push('|');
                    out.push_str(alias);
                }
                out.push_str("]]");
                last = link.end;
                changed = true;
            }
            if !changed {
                continue;
            }
            out.push_str(&source[last..]);
            if let Err(error) = self.write_note(*from, &out) {
                failures.push((*from, error));
            }
        }
        failures
    }

    /// Rename a note's stem (file name) in place, keeping its folder. The
    /// new title is sanitised like a create name; an existing target file is
    /// an error (never overwrite). Updates the note and the index, so the
    /// graph node title follows.
    pub fn rename_note(&mut self, id: usize, new_title: &str) -> io::Result<PathBuf> {
        let old = self.notes[id].path.clone();
        let title = sanitise_title(new_title);
        let dir = old
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.root.clone());
        let target = dir.join(format!("{title}.md"));
        if target == old {
            return Ok(target);
        }
        if target.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", target.display()),
            ));
        }
        fs::rename(&old, &target)?;
        self.apply_path(id, &target);
        Ok(target)
    }

    /// Delete a note's file and its index entry.
    ///
    /// This is the permanent variant; the UI deletes through
    /// [`Vault::delete_note_to_trash`] so the action can be undone.
    pub fn delete_note(&mut self, id: usize) -> io::Result<()> {
        fs::remove_file(&self.notes[id].path)?;
        self.notes.remove(id);
        for (i, note) in self.notes.iter_mut().enumerate() {
            note.id = i;
        }
        self.reindex();
        Ok(())
    }

    /// Move a note's file into the vault-local trash and drop it from the
    /// index, returning the trash-relative path for [`Vault::restore_note`].
    ///
    /// Layout: `.muninn-trash/<unix timestamp>[-n]/<original vault-relative
    /// path>`. The nested path is preserved so restoring is a plain rename
    /// back to where the note came from; the timestamp folder is unique even
    /// for repeated deletes in the same second (`-2`, `-3`, …). The move is a
    /// same-volume `fs::rename`, so it is atomic with no copy and no new
    /// dependency. On failure the note and its index entry stay untouched.
    ///
    /// Only note deletes are trashed; [`Vault::delete_folder`] stays a
    /// permanent empty-folder `remove_dir`.
    pub fn delete_note_to_trash(&mut self, id: usize) -> io::Result<String> {
        let source = self.notes[id].path.clone();
        let rel = self.notes[id].rel.clone();
        let trash = self.root.join(Self::TRASH_DIR);
        fs::create_dir_all(&trash)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let mut slot = trash.join(stamp.to_string());
        let mut suffix = 2;
        while slot.exists() {
            slot = trash.join(format!("{stamp}-{suffix}"));
            suffix += 1;
        }
        let target = slot.join(&rel);
        if target.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", target.display()),
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&source, &target)?;
        self.notes.remove(id);
        for (i, note) in self.notes.iter_mut().enumerate() {
            note.id = i;
        }
        self.reindex();
        // Pruning is a budget, not part of the delete: a read-only or busy
        // trash folder must never turn a successful move into an error.
        self.prune_trash();
        Ok(target
            .strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default())
    }

    /// Files and bytes currently held under [`Vault::TRASH_DIR`]. A missing or
    /// unreadable trash folder reads as `(0, 0)`; this is a readout and prune
    /// input, never an error path.
    #[must_use]
    pub fn trash_size(&self) -> (usize, u64) {
        trash_usage(&self.root.join(Self::TRASH_DIR))
    }

    /// Delete everything under [`Vault::TRASH_DIR`], leaving live notes alone.
    ///
    /// The folder is renamed to a unique hidden sibling first, so the vault's
    /// visible tree is unchanged even if removing the renamed copy fails
    /// part-way. A missing trash folder is success.
    pub fn empty_trash(&self) -> io::Result<()> {
        let trash = self.root.join(Self::TRASH_DIR);
        if !trash.exists() {
            return Ok(());
        }
        let temp = self.root.join(format!(
            "{}.emptying-{}-{}",
            Self::TRASH_DIR,
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::rename(&trash, &temp)?;
        fs::remove_dir_all(&temp)
    }

    /// Enforce [`Vault::TRASH_MAX_ENTRIES`] and [`Vault::TRASH_MAX_BYTES`] by
    /// removing whole timestamp folders, oldest first. Every error is ignored:
    /// the user asked to delete a note, and a full or unwritable trash folder
    /// must not make that fail.
    fn prune_trash(&self) {
        let trash = self.root.join(Self::TRASH_DIR);
        let Ok(entries) = fs::read_dir(&trash) else {
            return;
        };
        let mut slots: Vec<(u64, u64, PathBuf)> = entries
            .flatten()
            .filter_map(|entry| {
                let kind = entry.file_type().ok()?;
                if !kind.is_dir() {
                    return None;
                }
                let name = entry.file_name();
                let (stamp, suffix) = parse_trash_slot(&name.to_string_lossy())?;
                Some((stamp, suffix, entry.path()))
            })
            .collect();
        slots.sort_by_key(|(stamp, suffix, _)| (*stamp, *suffix));
        let (mut files, mut bytes) = self.trash_size();
        for (_, _, path) in slots {
            if files <= Self::TRASH_MAX_ENTRIES && bytes <= Self::TRASH_MAX_BYTES {
                break;
            }
            let (slot_files, slot_bytes) = trash_usage(&path);
            // A failed removal stays for the next prune; this loop still moves
            // on instead of retrying the same folder.
            if fs::remove_dir_all(&path).is_ok() {
                files = files.saturating_sub(slot_files);
                bytes = bytes.saturating_sub(slot_bytes);
            }
        }
    }

    /// Restore a note previously moved by [`Vault::delete_note_to_trash`].
    ///
    /// `trash_rel` is the vault-relative path returned by the delete. The file
    /// is renamed back to its original location; an existing target is an
    /// error and is never overwritten. A successful restore rescans the vault,
    /// so ids, links and folders match the file exactly.
    pub fn restore_note(&mut self, trash_rel: &str) -> io::Result<PathBuf> {
        let trash_rel = trash_rel.replace('\\', "/");
        let invalid = || {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a trash path", trash_rel),
            )
        };
        let rest = trash_rel
            .strip_prefix(Self::TRASH_DIR)
            .and_then(|rest| rest.strip_prefix('/'))
            .ok_or_else(invalid)?;
        let (slot, original) = rest.split_once('/').ok_or_else(invalid)?;
        // Reject a slot or a path that could escape the vault. Only the
        // timestamp folder and the original nested path are accepted.
        if slot.is_empty()
            || slot == "."
            || slot == ".."
            || original.is_empty()
            || original
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(invalid());
        }
        let source = self.root.join(&trash_rel);
        if !source.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} is no longer in the trash", trash_rel),
            ));
        }
        let target = self.root.join(original);
        if target.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", target.display()),
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&source, &target)?;
        *self = Vault::load(&self.root)?;
        Ok(target)
    }

    /// Move a note into an existing `folder` (`""` = root), keeping its file
    /// name. An existing target file is an error.
    pub fn move_note(&mut self, id: usize, folder: &str) -> io::Result<PathBuf> {
        let old = self.notes[id].path.clone();
        let dir = folder_dir(&self.root, folder);
        if !dir.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} is not a folder", dir.display()),
            ));
        }
        let name = old
            .file_name()
            .map(|n| n.to_owned())
            .unwrap_or_else(|| "Untitled.md".into());
        let target = dir.join(name);
        if target == old {
            return Ok(target);
        }
        if target.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", target.display()),
            ));
        }
        fs::rename(&old, &target)?;
        self.apply_path(id, &target);
        Ok(target)
    }

    fn apply_path(&mut self, id: usize, path: &Path) {
        let rel = path
            .strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_owned();
        let note = &mut self.notes[id];
        note.path = path.to_path_buf();
        note.rel = rel;
        note.title = title;
        self.reindex();
    }

    /// Create `name` as a folder under `parent` (`""` = vault root),
    /// returning its absolute path. An existing folder is an error.
    pub fn create_folder(&mut self, parent: &str, name: &str) -> io::Result<PathBuf> {
        let name = sanitise_folder_name(name);
        if name.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty folder name",
            ));
        }
        let parent_dir = folder_dir(&self.root, parent);
        fs::create_dir_all(&parent_dir)?;
        let dir = parent_dir.join(name.clone());
        if dir.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", dir.display()),
            ));
        }
        fs::create_dir(&dir)?;
        let rel = if parent.is_empty() {
            name
        } else {
            format!("{}/{}", parent.trim_matches('/'), name)
        };
        if !self.dirs.iter().any(|d| d == &rel) {
            self.dirs.push(rel);
        }
        Ok(dir)
    }

    /// Rename one folder's last segment. `folder` is the vault-relative path
    /// of the folder to rename; `new_name` is its new last segment.
    /// Notes under the renamed folder keep resolving.
    pub fn rename_folder(&mut self, folder: &str, new_name: &str) -> io::Result<PathBuf> {
        let folder = folder.trim_matches('/');
        if folder.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot rename the root",
            ));
        }
        let name = sanitise_folder_name(new_name);
        if name.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty folder name",
            ));
        }
        let (parent, _old) = folder.rsplit_once('/').unwrap_or(("", folder));
        let source = folder_dir(&self.root, folder);
        let parent_dir = folder_dir(&self.root, parent);
        let target = parent_dir.join(name);
        if target == source {
            return Ok(target);
        }
        if target.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", target.display()),
            ));
        }
        fs::rename(&source, &target)?;
        let target_rel = target
            .strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let prefix = format!("{folder}/");
        for note in &mut self.notes {
            if let Some(rest) = note.rel.strip_prefix(&prefix) {
                note.rel = format!("{target_rel}/{rest}");
                note.path = self.root.join(&note.rel);
            }
        }
        for dir in &mut self.dirs {
            if dir == folder {
                *dir = target_rel.clone();
            } else if let Some(rest) = dir.strip_prefix(&prefix) {
                *dir = format!("{target_rel}/{rest}");
            }
        }
        self.reindex();
        Ok(target)
    }

    /// Delete an empty folder. A non-empty folder is an error; recursive
    /// deletes need explicit confirmation in the UI and are out of scope
    /// here by design.
    pub fn delete_folder(&mut self, folder: &str) -> io::Result<()> {
        let folder = folder.trim_matches('/');
        if folder.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot delete the root",
            ));
        }
        fs::remove_dir(folder_dir(&self.root, folder))?;
        self.dirs.retain(|d| d != folder);
        Ok(())
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

/// Group collected missing targets into sorted [`BrokenLink`] rows.
fn group_broken(broken: HashMap<String, (String, Vec<usize>)>) -> Vec<BrokenLink> {
    let mut rows: Vec<BrokenLink> = broken
        .into_values()
        .map(|(target, mut referrers)| {
            referrers.sort_unstable();
            referrers.dedup();
            BrokenLink { target, referrers }
        })
        .collect();
    rows.sort_by(|a, b| {
        a.target
            .to_lowercase()
            .cmp(&b.target.to_lowercase())
            .then_with(|| a.target.cmp(&b.target))
    });
    rows
}

/// Write `data` to `path` atomically: a hidden, uniquely named temp file in
/// the same directory is flushed and fsynced, then renamed over `path`.
///
/// A crash at any point leaves either the old bytes or the new bytes at
/// `path`, never a truncated file. A failed write removes the temp file. The
/// temp name starts with `.` so the vault walk ignores it even if a killed
/// process leaves one behind.
pub(crate) fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    atomic_replace(path, true, |file| file.write_all(data))
}

/// Copy `source` into `dest` through the same hidden temp + fsync + rename path
/// as note saves. An existing `dest` is never replaced.
///
/// Drag-drop imports use this so a half-copied attachment can never appear in
/// the vault and a same-named file keeps its bytes.
pub fn atomic_copy(source: &Path, dest: &Path) -> io::Result<()> {
    atomic_replace(dest, false, |file| {
        let mut reader = fs::File::open(source)?;
        io::copy(&mut reader, file).map(|_| ())
    })
}

/// The one temp-file + rename implementation behind note saves and attachment
/// imports. `replace` is false for imports: the destination is re-checked just
/// before the rename so a racing copy cannot overwrite a file that appeared.
fn atomic_replace(
    path: &Path,
    replace: bool,
    write: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map_or_else(|| "note".into(), |n| n.to_string_lossy().into_owned());
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    // pid + counter keeps concurrent writers (and repeated saves) apart.
    let temp = dir.join(format!(".{name}.{}.{counter}.tmp", std::process::id()));

    let write = (|| -> io::Result<()> {
        {
            let mut file = fs::File::create(&temp)?;
            write(&mut file)?;
            file.flush()?;
            // fsync before rename: otherwise the rename can reach disk first
            // and a crash could expose an empty or partial file.
            file.sync_all()?;
            // Close before rename; Windows cannot rename an open file.
        }
        if !replace && path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "destination exists",
            ));
        }
        fs::rename(&temp, path)
    })();

    if write.is_err() {
        // Best-effort cleanup: report the original error, not the removal's.
        let _ = fs::remove_file(&temp);
        return write;
    }
    sync_parent(dir);
    Ok(())
}

/// fsync the directory a rename landed in, best-effort.
///
/// Some filesystems and platforms reject fsync on a directory. The rename is
/// already atomic; skipping (or failing) this sync only weakens durability
/// across a power loss, so the error is deliberately ignored — there is no
/// logger yet. Windows has no portable directory fsync, so it is a no-op.
fn sync_parent(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = fs::File::open(dir) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
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

/// Validate (or create) the vault root before scanning.
///
/// A missing root is created empty. An existing non-directory is
/// `NotADirectory`, and any other metadata/read_dir failure is the root's own
/// error: the scan must not turn a permission problem into "no notes".
fn prepare_root(root: &Path) -> io::Result<()> {
    match fs::metadata(root) {
        Ok(meta) if !meta.is_dir() => {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("{} is not a folder", root.display()),
            ));
        }
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // A missing vault is created empty; anything else (permission,
            // loop, …) is the root's own failure and must be reported.
            fs::create_dir_all(root)?;
        }
        Err(e) => return Err(e),
    }
    // The root is the vault itself: unlike a nested folder, a read_dir
    // failure here cannot be treated as "no notes".
    fs::read_dir(root)?;
    Ok(())
}

/// Read one scanned path into a note, returning `None` when it cannot be
/// indexed (invalid UTF-8, permissions, unusable file name). The caller counts
/// those as skipped instead of failing the whole vault.
fn note_from_path(root: &Path, path: &Path, id: usize) -> Option<Note> {
    let source = fs::read_to_string(path).ok()?;
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_owned)?;
    if title.is_empty() {
        return None;
    }
    let rel = path
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    Some(Note {
        id,
        path: path.to_path_buf(),
        rel,
        title,
        source,
        links: Vec::new(),
        backlinks: Vec::new(),
    })
}

/// Case-insensitive path order, then exact, so scans are deterministic.
pub(crate) fn sort_paths(paths: &mut [PathBuf]) {
    paths.sort_by(|a, b| {
        a.to_string_lossy()
            .to_lowercase()
            .cmp(&b.to_string_lossy().to_lowercase())
            .then_with(|| a.cmp(b))
    });
}

/// Collect files with extension `ext` (case-insensitive) and every visible
/// folder under `dir`. Notes use `md`; boards use
/// [`BOARD_EXTENSION`](crate::BOARD_EXTENSION). Both share these rules: at most
/// 12 levels, dot-entries skipped, symlinks not followed.
pub(crate) fn walk(
    dir: &Path,
    out: &mut Vec<PathBuf>,
    dirs: &mut Vec<String>,
    root: &Path,
    depth: usize,
    ext: &str,
) {
    if depth > 12 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    // Seed the root itself so folders() always includes "".
    if depth == 0 {
        dirs.push(String::new());
    }
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
            if let Ok(rel) = path.strip_prefix(root) {
                dirs.push(rel.to_string_lossy().replace('\\', "/"));
            }
            walk(&path, out, dirs, root, depth + 1, ext);
        } else if kind.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(ext))
        {
            out.push(path);
        }
    }
}

/// Files and bytes below `dir`, recursively. Unreadable entries are skipped:
/// the same best-effort posture as the trash budget that reads it.
fn trash_usage(dir: &Path) -> (usize, u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return (0, 0);
    };
    let mut files = 0;
    let mut bytes = 0;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            let (nested_files, nested_bytes) = trash_usage(&entry.path());
            files += nested_files;
            bytes += nested_bytes;
        } else if kind.is_file() {
            files += 1;
            bytes += entry.metadata().map_or(0, |meta| meta.len());
        }
    }
    (files, bytes)
}

/// Order a trash timestamp folder: `<unix seconds>` or `<unix seconds>-<n>`.
/// `None` for anything else, which is left alone rather than pruned.
fn parse_trash_slot(name: &str) -> Option<(u64, u64)> {
    match name.split_once('-') {
        Some((stamp, suffix)) => Some((stamp.parse().ok()?, suffix.parse().ok()?)),
        None => Some((name.parse().ok()?, 0)),
    }
}

/// Board rename: the board name `input` asks for, trimmed and without a
/// typed `.excalidraw`, or `InvalidInput` when it cannot be a board's file
/// name on every platform Muninn runs on: empty, a path (separators, `.` or
/// `..`), hidden (a leading `.`), a character Windows forbids or a control
/// character, a trailing `.`, or a reserved device name (`CON`, `NUL`, …).
/// Unlike a new note's title it is refused, not quietly rewritten, so the
/// name on disk is the one the user typed.
pub fn board_name(input: &str) -> io::Result<String> {
    let invalid = |why: &str| {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("a board name {why}"),
        ))
    };
    let mut name = input.trim();
    // GK B-1: never slice at a byte offset that may sit inside a multibyte
    // character. `get` is `None` there, and the suffix is ASCII, so a name
    // that ends in the extension always has a boundary where it starts.
    let suffix = format!(".{}", crate::BOARD_EXTENSION);
    let cut = name.len().saturating_sub(suffix.len());
    if name
        .get(cut..)
        .is_some_and(|end| end.eq_ignore_ascii_case(&suffix))
    {
        name = name[..cut].trim_end();
    }
    if name.is_empty() {
        return invalid("cannot be empty");
    }
    if name.contains(['/', '\\']) || name == "." || name == ".." {
        return invalid("cannot contain / or \\");
    }
    if name.starts_with('.') {
        return invalid("cannot start with a dot");
    }
    if name
        .chars()
        .any(|c| c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        return invalid("cannot contain : * ? \" < > | or control characters");
    }
    if name.ends_with('.') {
        return invalid("cannot end with a dot");
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        return invalid("cannot be a reserved device name");
    }
    Ok(name.to_owned())
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

/// Sanitise a single folder segment: like a title, but an empty result stays
/// empty so callers can reject it instead of silently inventing a name.
fn sanitise_folder_name(name: &str) -> String {
    let mut name: String = name
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
    while name.ends_with('.') || name.ends_with(' ') {
        name.pop();
    }
    if name == "." || name == ".." || name.starts_with('.') {
        return String::new();
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempVault;

    #[test]
    fn chunked_scan_matches_a_full_load() {
        let tmp = TempVault::new(&[
            ("Alpha.md", "to [[Beta]] and [[folder/Gamma]]"),
            ("Beta.md", "back to [[Alpha]]"),
            ("folder/Gamma.md", "third, [[delta|Dee]]"),
            ("delta.md", "fourth"),
            ("folder/Delta.md", "duplicate title, [[Gamma]]"),
        ]);
        // A file the scan must count as skipped in both paths.
        fs::write(tmp.path().join("bad.md"), [0xff, 0xfe, 0xfd]).expect("invalid utf8");

        let full = Vault::load(tmp.path()).expect("full load");
        let mut job = Vault::scan_start(tmp.path()).expect("scan start");
        assert_eq!(job.total(), 6);
        assert_eq!(job.done(), 0);
        let mut chunks = 0;
        while !job.is_done() {
            assert_eq!(job.read_chunk(2), 2);
            chunks += 1;
        }
        assert!(chunks > 1, "the fixture must span several chunks");
        let chunked = job.finish();

        assert_eq!(chunked.notes.len(), full.notes.len());
        assert_eq!(chunked.dirs, full.dirs);
        assert_eq!(chunked.skipped_count(), full.skipped_count());
        assert_eq!(chunked.skipped_count(), 1);
        assert_eq!(chunked.broken_links(), full.broken_links());
        for (a, b) in full.notes.iter().zip(&chunked.notes) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.rel, b.rel);
            assert_eq!(a.title, b.title);
            assert_eq!(a.source, b.source);
            assert_eq!(a.links, b.links);
            assert_eq!(a.backlinks, b.backlinks);
        }
        assert_eq!(chunked.display_label(0), full.display_label(0));
    }

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

    #[test]
    fn unicode_titles_search_resolve_and_duplicate_labels() {
        // X-02: CJK, accented Latin and RTL titles are just path segments.
        // Resolution is exact after Unicode lower-casing (never locale
        // stemming), and search is a substring match over the same strings.
        let tmp = TempVault::new(&[
            ("日本語.md", "日本語のノート [[Café]] [[مرحبا]]"),
            ("Café.md", "accented"),
            ("مرحبا.md", "right to left"),
            ("a/日本語.md", "duplicate in a"),
            ("b/日本語.md", "duplicate in b"),
        ]);
        let v = tmp.load();
        assert!(v.resolve("日本語").is_some(), "CJK title resolves");
        assert!(v.resolve("日本語のノート").is_none());
        assert!(v.resolve("café").is_some(), "accented title is case-folded");
        assert!(v.resolve("CAFÉ").is_some());
        assert!(v.resolve("مرحبا").is_some(), "RTL title resolves");
        assert_eq!(v.search("日本").len(), 3, "CJK substring search");
        assert_eq!(v.search("café").len(), 2);
        assert_eq!(v.search("مرحبا").len(), 2);

        // Duplicate CJK titles disambiguate by folder exactly like ASCII ones.
        for folder in ["a", "b"] {
            let id = v
                .resolve(&format!("{folder}/日本語"))
                .unwrap_or_else(|| panic!("{folder} resolves"));
            assert!(v.is_duplicate(id), "{folder} is a duplicate");
            assert_eq!(v.display_label(id), format!("{folder}/日本語"));
        }
        let bare = v.resolve("日本語").expect("bare duplicate");
        assert_eq!(v.note(bare).rel, "a/日本語.md");
    }

    #[test]
    fn broken_links_are_cached_grouped_and_update_after_create() {
        let tmp = TempVault::new(&[
            ("Home.md", "[[Missing]] and [[gone/Else#Part]] and [[Beta]]"),
            ("Beta.md", "also [[Missing]] and [[Missing]]"),
            ("Else.md", "fine"),
        ]);
        let v = tmp.load();
        let broken = v.broken_links();
        assert_eq!(broken.len(), 2, "{broken:?}");
        assert_eq!(broken[0].target, "gone/Else", "{broken:?}");
        assert_eq!(broken[1].target, "Missing");
        let home = v.resolve("Home").unwrap();
        let beta = v.resolve("Beta").unwrap();
        let mut referrers = broken[1].referrers.clone();
        referrers.sort_unstable();
        assert_eq!(
            referrers,
            {
                let mut ids = vec![home, beta];
                ids.sort_unstable();
                ids
            },
            "both referrers are recorded once"
        );
        // Creating the target on disk and rescanning clears it.
        v.create_note("Missing").unwrap();
        let v = tmp.load();
        assert_eq!(v.broken_links().len(), 1);
        assert_eq!(v.broken_links()[0].target, "gone/Else");
    }

    #[test]
    fn suggestions_rank_titles_and_show_duplicates_by_folder() {
        let tmp = TempVault::new(&[
            ("Home.md", "home"),
            ("a/Index.md", "a"),
            ("b/Index.md", "b"),
            ("India.md", "i"),
        ]);
        let v = tmp.load();
        let hits = v.suggest_links("ind", 8);
        // Both `Index` duplicates and `India` are title-prefix matches; the
        // duplicates carry their folder in `display_label`.
        let labels: Vec<String> = hits.iter().map(|&id| v.display_label(id)).collect();
        assert!(labels.contains(&"a/Index".to_owned()), "{labels:?}");
        assert!(labels.contains(&"b/Index".to_owned()), "{labels:?}");
        // Case-insensitive and capped.
        assert!(
            v.suggest_links("HOME", 8)
                .iter()
                .any(|&id| id == v.resolve("Home").unwrap())
        );
        assert_eq!(v.suggest_links("", 2).len(), 2);
        assert!(v.suggest_links("zzz", 8).is_empty());
    }

    #[test]
    fn rename_rewrite_preserves_alias_heading_and_duplicate_choice() {
        let tmp = TempVault::new(&[
            ("Home.md", "See [[Alpha]] and [[Alpha#Top|the top]]"),
            ("Second.md", "Back to [[alpha|A]]"),
            ("Alpha.md", "# Top\nbody"),
        ]);
        let mut v = tmp.load();
        let alpha = v.resolve("Alpha").expect("Alpha");
        let inbound = v.link_referrers(alpha);
        assert_eq!(inbound.len(), 2, "{inbound:?}");
        v.rename_note(alpha, "Beta").expect("rename");
        let new_stem = v.notes[alpha].stem().to_owned();
        let failures = v.rewrite_inbound_links(&inbound, &new_stem);
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(
            fs::read_to_string(tmp.path().join("Home.md")).unwrap(),
            "See [[Beta]] and [[Beta#Top|the top]]"
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join("Second.md")).unwrap(),
            "Back to [[Beta|A]]"
        );
        assert!(v.broken_links().is_empty(), "{:?}", v.broken_links());
    }

    #[test]
    fn duplicate_bare_title_is_rewritten_only_when_it_resolved_here() {
        let tmp = TempVault::new(&[
            ("a/Index.md", "a"),
            ("b/Index.md", "b"),
            ("Home.md", "bare [[Index]] and path [[b/Index]]"),
        ]);
        let mut v = tmp.load();
        let b = v.resolve("b/Index").expect("b index");
        let inbound = v.link_referrers(b);
        assert_eq!(inbound.len(), 1);
        assert_eq!(inbound[0].1, vec!["b/Index".to_owned()]);
        v.rename_note(b, "Bee").expect("rename");
        let new_stem = v.notes[b].stem().to_owned();
        assert!(v.rewrite_inbound_links(&inbound, &new_stem).is_empty());
        let home = fs::read_to_string(tmp.path().join("Home.md")).unwrap();
        assert_eq!(
            home, "bare [[Index]] and path [[b/Bee]]",
            "a bare link that resolved to the other duplicate stays put"
        );
    }

    #[test]
    fn folder_rename_rewrites_qualified_heading_alias_and_bare_links() {
        let tmp = TempVault::new(&[
            ("old/one.md", "# One\nbody"),
            ("old/two.md", "# Two\nbody"),
            (
                "Home.md",
                "[[old/one|alias]] and [[old/two#Two]] and bare [[two]]",
            ),
        ]);
        let mut v = tmp.load();
        let referrers = v.link_referrers_for_folder("old");
        assert_eq!(referrers.len(), 1, "{referrers:?}");
        v.rename_folder("old", "new").expect("rename folder");
        let failures = v.rewrite_inbound_links_for_folder("old", "new");
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(
            fs::read_to_string(tmp.path().join("Home.md")).unwrap(),
            "[[new/one|alias]] and [[new/two#Two]] and bare [[new/two]]"
        );
        assert!(v.resolve("new/one").is_some());
        assert!(v.resolve("new/two").is_some());
        assert!(v.broken_links().is_empty(), "{:?}", v.broken_links());
    }

    #[test]
    fn leaving_folder_links_alone_keeps_them_dangling() {
        let tmp = TempVault::new(&[
            ("old/one.md", "# One\nbody"),
            ("old/two.md", "# Two\nbody"),
            ("Home.md", "[[old/one|alias]] and [[old/two#Two]]"),
        ]);
        let mut v = tmp.load();
        v.rename_folder("old", "new").expect("rename folder");
        // The Leave option skips rewriting entirely.
        let home = fs::read_to_string(tmp.path().join("Home.md")).unwrap();
        assert_eq!(home, "[[old/one|alias]] and [[old/two#Two]]");
        let broken: Vec<&str> = v.broken_links().iter().map(|b| b.target.as_str()).collect();
        assert!(broken.contains(&"old/one"), "{broken:?}");
        assert!(broken.contains(&"old/two"), "{broken:?}");
    }

    #[test]
    fn folder_rename_handles_nested_paths_and_respects_bare_duplicate_choice() {
        let tmp = TempVault::new(&[
            ("old/child/deep.md", "# Deep\nbody"),
            ("elsewhere/deep.md", "other"),
            ("Home.md", "[[old/child/deep#Deep|d]] and bare [[deep]]"),
        ]);
        let mut v = tmp.load();
        // The bare title resolves to the smaller-rel duplicate outside the
        // folder, so it must stay bare even though a moved note shares it.
        assert_eq!(
            v.resolve("deep")
                .map(|id| v.notes[id].rel.clone())
                .as_deref(),
            Some("elsewhere/deep.md")
        );
        v.rename_folder("old", "new").expect("rename folder");
        assert!(v.rewrite_inbound_links_for_folder("old", "new").is_empty());
        assert_eq!(
            fs::read_to_string(tmp.path().join("Home.md")).unwrap(),
            "[[new/child/deep#Deep|d]] and bare [[deep]]"
        );
        assert!(v.resolve("new/child/deep").is_some());
    }

    #[test]
    fn note_crud_rename_delete_and_move() {
        let tmp = TempVault::new(&[
            ("notes/alpha.md", "alpha body"),
            ("notes/beta.md", "beta body"),
            ("other/gamma.md", "gamma body"),
        ]);
        let mut v = tmp.load();
        let alpha = v.resolve("notes/alpha").unwrap();
        // Rename keeps the folder and the file contents.
        let renamed = v.rename_note(alpha, "Alpha Two").unwrap();
        assert!(renamed.ends_with("notes/Alpha Two.md"));
        assert_eq!(fs::read_to_string(&renamed).unwrap(), "alpha body");
        // Renaming onto an existing name must fail rather than overwrite.
        assert_eq!(
            v.rename_note(alpha, "beta").unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        // Move into another folder; moving onto an existing name fails.
        let moved = v.move_note(alpha, "other").unwrap();
        assert!(moved.ends_with("other/Alpha Two.md"));
        let beta = v.resolve("notes/beta").unwrap();
        assert!(v.move_note(beta, "missing").is_err());
        // Delete removes the file.
        v.delete_note(beta).unwrap();
        assert!(!tmp.path().join("notes/beta.md").exists());
        // Sanitise: illegal characters become dashes.
        let third = v.resolve("other/gamma").unwrap();
        let sanitised = v.rename_note(third, "a/b:c").unwrap();
        assert!(sanitised.ends_with("other/a-b-c.md"));
    }

    #[test]
    fn folder_crud_create_rename_and_delete_empty_only() {
        let tmp = TempVault::new(&[("projects/alpha/note.md", "n")]);
        let mut v = tmp.load();
        // Create under a folder and at the root; duplicates and dot names fail.
        let created = v.create_folder("projects", "Beta").unwrap();
        assert!(created.ends_with("projects/Beta"));
        assert!(v.create_folder("projects", "Beta").is_err());
        assert!(v.create_folder("", ".hidden").is_err());
        // Rename moves the directory.
        let renamed = v.rename_folder("projects/Beta", "Gamma").unwrap();
        assert!(renamed.ends_with("projects/Gamma"));
        // Delete is empty-only: populated folders fail, empty ones succeed.
        assert!(v.delete_folder("projects/Gamma").is_ok());
        assert!(v.delete_folder("projects/alpha").is_err());
        assert!(v.delete_folder("").is_err());
        assert!(v.rename_folder("", "x").is_err());
    }
}

#[cfg(test)]
mod trash_tests {
    use super::*;
    use crate::testing::TempVault;

    #[test]
    fn deleted_notes_go_to_hidden_trash_and_restore_with_links() {
        let tmp = TempVault::new(&[
            ("Alpha.md", "# Alpha\n\nSee [[Beta]].\n"),
            ("notes/Beta.md", "# Beta\n\nBack to [[Alpha]].\n"),
        ]);
        let mut v = tmp.load();
        let alpha = v.resolve("Alpha").expect("Alpha");

        let rel = v.delete_note_to_trash(alpha).expect("trash Alpha");
        assert!(rel.starts_with(".muninn-trash/"), "{rel}");
        assert!(rel.ends_with("/Alpha.md"), "{rel}");
        assert!(!tmp.path().join("Alpha.md").exists(), "move, not delete");
        let trashed = tmp.path().join(&rel);
        assert_eq!(
            fs::read_to_string(&trashed).expect("trashed body"),
            "# Alpha\n\nSee [[Beta]].\n"
        );

        // Dot-directories are skipped, so the trashed note never reappears as
        // a live note in a fresh scan.
        let after = tmp.load();
        assert_eq!(after.notes.len(), 1);
        assert!(after.resolve("Alpha").is_none(), "trash is invisible");
        assert_eq!(after.skipped_count(), 0);

        // Undo is a rename back: content and both link directions survive.
        v.restore_note(&rel).expect("restore Alpha");
        assert!(tmp.path().join("Alpha.md").is_file());
        assert!(!trashed.exists(), "trash copy is moved, not copied");
        let reloaded = tmp.load();
        let alpha = reloaded.resolve("Alpha").expect("restored Alpha");
        let beta = reloaded.resolve("Beta").expect("Beta");
        assert_eq!(reloaded.note(alpha).source, "# Alpha\n\nSee [[Beta]].\n");
        assert!(reloaded.note(alpha).links.contains(&beta));
        assert!(reloaded.note(beta).links.contains(&alpha));
    }

    #[test]
    fn trash_preserves_nested_paths_and_is_unique_per_delete() {
        let tmp = TempVault::new(&[("projects/alpha/note.md", "alpha")]);
        let mut v = tmp.load();
        let id = v.resolve("projects/alpha/note").expect("nested note");
        let first = v.delete_note_to_trash(id).expect("first delete");
        assert!(
            first.ends_with("/projects/alpha/note.md"),
            "nested path kept: {first}"
        );

        let path = tmp.path().join("projects/alpha/note.md");
        fs::write(&path, "alpha again").unwrap();
        let mut v = tmp.load();
        let id = v.resolve("projects/alpha/note").expect("recreated note");
        let second = v.delete_note_to_trash(id).expect("second delete");
        assert_ne!(first, second, "same-second deletes get distinct slots");
        assert!(tmp.path().join(&first).is_file());
        assert!(tmp.path().join(&second).is_file());
    }

    #[test]
    fn restore_refuses_to_overwrite_an_existing_note() {
        let tmp = TempVault::new(&[("Alpha.md", "original")]);
        let mut v = tmp.load();
        let alpha = v.resolve("Alpha").expect("Alpha");
        let rel = v.delete_note_to_trash(alpha).expect("trash");
        fs::write(tmp.path().join("Alpha.md"), "recreated").unwrap();

        let error = v.restore_note(&rel).expect_err("existing target");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(!error.to_string().is_empty());
        assert_eq!(
            fs::read_to_string(tmp.path().join("Alpha.md")).unwrap(),
            "recreated",
            "the live note is never overwritten"
        );
        assert!(
            tmp.path().join(&rel).is_file(),
            "the trashed copy survives a refused restore"
        );
    }

    #[test]
    fn restore_rejects_paths_outside_the_trash() {
        let tmp = TempVault::new(&[("Alpha.md", "alpha")]);
        let mut v = tmp.load();
        for bad in [
            "Alpha.md",
            ".muninn-trash/",
            ".muninn-trash/1/../../Alpha.md",
            ".muninn-trash/../Alpha.md",
        ] {
            assert!(v.restore_note(bad).is_err(), "must reject {bad}");
        }
        assert!(tmp.path().join("Alpha.md").is_file());
    }
}

/// The fixed trash budget is separate from the undo/restore contract above:
/// these build timestamp slots directly, because the cap needs hundreds of
/// deletes that a per-note test loop would make needlessly slow.
#[cfg(test)]
mod trash_budget_tests {
    use super::*;
    use crate::testing::TempVault;

    /// Fill `slots` timestamp folders, one file each.
    fn fill(root: &Path, slots: usize, bytes_each: Option<u64>) -> PathBuf {
        let trash = root.join(Vault::TRASH_DIR);
        fs::create_dir_all(&trash).expect("trash");
        for i in 0..slots {
            let slot = trash.join(format!("{:010}", i + 1));
            fs::create_dir_all(&slot).expect("slot");
            let file = fs::File::create(slot.join("Old.md")).expect("trash file");
            if let Some(bytes) = bytes_each {
                file.set_len(bytes).expect("sparse trash file");
            }
        }
        trash
    }

    #[test]
    fn prune_keeps_the_entry_cap_and_drops_the_oldest_slots() {
        let tmp = TempVault::new(&[("keep.md", "keep")]);
        let vault = tmp.load();
        let extra = 3;
        fill(tmp.path(), Vault::TRASH_MAX_ENTRIES + extra, None);
        assert_eq!(vault.trash_size().0, Vault::TRASH_MAX_ENTRIES + extra);

        vault.prune_trash();

        assert_eq!(vault.trash_size().0, Vault::TRASH_MAX_ENTRIES);
        let trash = tmp.path().join(Vault::TRASH_DIR);
        assert!(!trash.join(format!("{:010}", 1)).exists(), "oldest gone");
        assert!(!trash.join(format!("{:010}", extra)).exists());
        assert!(
            trash
                .join(format!("{:010}", Vault::TRASH_MAX_ENTRIES + extra))
                .is_dir(),
            "the newest slot is kept"
        );
    }

    #[test]
    fn prune_keeps_the_byte_cap_and_drops_the_oldest_slot() {
        let tmp = TempVault::new(&[("keep.md", "keep")]);
        let vault = tmp.load();
        let megabyte = 1024 * 1024;
        let slots = (Vault::TRASH_MAX_BYTES / megabyte + 1) as usize;
        fill(tmp.path(), slots, Some(megabyte));
        assert!(vault.trash_size().1 > Vault::TRASH_MAX_BYTES);

        vault.prune_trash();

        assert!(vault.trash_size().1 <= Vault::TRASH_MAX_BYTES);
        assert!(vault.trash_size().0 < slots, "one slot was pruned");
        assert!(
            !tmp.path()
                .join(Vault::TRASH_DIR)
                .join(format!("{:010}", 1))
                .exists(),
            "oldest gone"
        );
    }

    #[test]
    fn empty_trash_clears_the_budget_without_touching_live_files() {
        let tmp = TempVault::new(&[("keep.md", "keep")]);
        let vault = tmp.load();
        fill(tmp.path(), 2, None);
        assert_eq!(vault.trash_size().0, 2);

        vault.empty_trash().expect("empty");

        assert_eq!(vault.trash_size(), (0, 0));
        assert!(!tmp.path().join(Vault::TRASH_DIR).exists());
        assert!(tmp.path().join("keep.md").is_file(), "live file untouched");
    }

    #[test]
    fn an_over_budget_trash_never_fails_a_delete() {
        let tmp = TempVault::new(&[("gone.md", "gone"), ("keep.md", "keep")]);
        let mut vault = tmp.load();
        fill(tmp.path(), Vault::TRASH_MAX_ENTRIES + 5, None);
        let id = vault.resolve("gone").expect("gone");

        let rel = vault
            .delete_note_to_trash(id)
            .expect("a full trash never fails the delete");

        assert!(!tmp.path().join("gone.md").exists());
        assert!(tmp.path().join(&rel).is_file(), "the new delete is kept");
        assert!(vault.trash_size().0 <= Vault::TRASH_MAX_ENTRIES, "pruned");
    }
}

#[cfg(test)]
mod load_error_tests {
    use super::*;
    use crate::testing::TempVault;

    #[test]
    fn a_file_as_the_vault_root_is_an_error() {
        let tmp = TempVault::new(&[("not-a-folder.md", "# file")]);
        let file = tmp.path().join("not-a-folder.md");
        let error = match Vault::load(&file) {
            Ok(_) => panic!("a file must not load as a vault"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), io::ErrorKind::NotADirectory);
    }

    #[test]
    fn an_unreadable_root_is_an_error_not_an_empty_vault() {
        let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o000)).expect("chmod");
            // Root ignores mode bits (containers often run as root). Detect
            // that case and fall back to the non-directory test above for the
            // error path; the assertion still runs as a normal user.
            let readable_as_root = fs::read_dir(tmp.path()).is_ok();
            let loaded = Vault::load(tmp.path());
            fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o700)).expect("chmod");
            if readable_as_root {
                return;
            }
            let error = match loaded {
                Ok(_) => panic!("an unreadable root must not look empty"),
                Err(error) => error,
            };
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        }
        #[cfg(not(unix))]
        let _ = tmp;
    }

    #[test]
    fn a_missing_root_is_created_empty() {
        let tmp = TempVault::new(&[]);
        let root = tmp.path().join("brand/new");
        let v = Vault::load(&root).expect("missing root is created");
        assert!(root.is_dir());
        assert!(v.notes.is_empty());
        assert_eq!(v.skipped_count(), 0);
    }

    #[test]
    fn invalid_utf8_notes_are_skipped_and_counted() {
        let tmp = TempVault::new(&[("Good.md", "# Good\n")]);
        fs::write(tmp.path().join("Broken.md"), [0xff, 0xfe, 0x00, 0x01]).expect("bytes");

        let v = tmp.load();
        assert_eq!(v.notes.len(), 1);
        assert!(v.resolve("Good").is_some());
        assert!(v.resolve("Broken").is_none());
        assert_eq!(v.skipped_count(), 1);
    }
}

/// Atomic-write guarantees live in their own module so the existing tests
/// module keeps its size and baseline.
#[cfg(test)]
mod atomic_write_tests {
    use super::*;
    use crate::testing::TempVault;

    /// Every `.tmp` file left in `dir` (the atomic-write temp files are hidden).
    fn temp_files(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "tmp"))
            .collect()
    }

    #[test]
    fn atomic_write_leaves_old_or_new_and_cleans_temp() {
        let tmp = TempVault::new(&[("note.md", "old")]);
        let path = tmp.path().join("note.md");

        atomic_write(&path, b"new body").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new body");
        assert!(
            temp_files(tmp.path()).is_empty(),
            "{:?}",
            temp_files(tmp.path())
        );

        // Rename onto an existing directory fails after the temp file was
        // fully written and fsynced: the destination stays untouched and no
        // partial temp survives. This is the injection point for the
        // kill-9/rename-failure contract (old bytes or new bytes, never both).
        let blocked = tmp.path().join("blocked.md");
        fs::create_dir(&blocked).unwrap();
        assert!(atomic_write(&blocked, b"never lands").is_err());
        assert!(blocked.is_dir());
        assert!(
            temp_files(tmp.path()).is_empty(),
            "{:?}",
            temp_files(tmp.path())
        );
    }

    #[test]
    fn create_note_in_writes_atomically_and_leaves_no_temp() {
        let tmp = TempVault::new(&[]);
        let v = tmp.load();
        let created = v.create_note_in("Atomic", "notes").unwrap();

        assert_eq!(fs::read_to_string(&created).unwrap(), "# Atomic\n\n");
        let dir = created.parent().unwrap();
        assert!(temp_files(dir).is_empty(), "{:?}", temp_files(dir));

        // A second create still gets a unique name and leaves no temp behind.
        let again = v.create_note_in("Atomic", "notes").unwrap();
        assert_eq!(again.file_name().unwrap(), "Atomic 2.md");
        assert!(temp_files(dir).is_empty(), "{:?}", temp_files(dir));
    }

    #[test]
    fn failed_write_keeps_in_memory_source() {
        let tmp = TempVault::new(&[("note.md", "old")]);
        let mut v = tmp.load();
        let id = v.resolve("note").unwrap();
        let path = v.note(id).path.clone();
        // Block the rename: the destination is now a directory.
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();

        assert!(v.write_note(id, "new text").is_err());

        assert_eq!(v.note(id).source, "old");
        assert!(
            temp_files(tmp.path()).is_empty(),
            "{:?}",
            temp_files(tmp.path())
        );
    }
}

/// Board slice 8: `Vault::create_board_in`.
#[cfg(test)]
mod board_create_tests {
    use std::fs;

    use crate::testing::TempVault;
    use crate::{Board, Vault};

    fn vault(tmp: &TempVault) -> Vault {
        Vault::load(tmp.path()).unwrap()
    }

    #[test]
    fn a_new_board_is_an_empty_scene_and_never_overwrites() {
        let tmp = TempVault::new(&[("Untitled board.excalidraw", "keep")]);
        let v = vault(&tmp);
        let a = v.create_board_in("Untitled board", "").unwrap();
        let b = v.create_board_in("Untitled board", "").unwrap();
        assert_eq!(a, tmp.path().join("Untitled board 2.excalidraw"));
        assert_eq!(b, tmp.path().join("Untitled board 3.excalidraw"));
        assert_eq!(
            fs::read_to_string(tmp.path().join("Untitled board.excalidraw")).unwrap(),
            "keep"
        );
        let board = Board::from_json(&fs::read_to_string(&a).unwrap()).unwrap();
        assert_eq!(board.live_elements().count(), 0);
    }

    #[test]
    fn a_new_board_stays_inside_the_vault() {
        let tmp = TempVault::new(&[]);
        let v = vault(&tmp);
        let p = v.create_board_in("Plan", "../../Out/./.hidden/In").unwrap();
        assert!(p.starts_with(tmp.path()), "{p:?}");
        assert_eq!(p, tmp.path().join("Out/In/Plan.excalidraw"));
    }

    #[cfg(unix)]
    #[test]
    fn a_new_board_is_refused_in_a_folder_that_links_out_of_the_vault() {
        let tmp = TempVault::new(&[]);
        let outside = TempVault::new(&[]);
        std::os::unix::fs::symlink(outside.path(), tmp.path().join("Link")).unwrap();
        let v = vault(&tmp);
        let err = v.create_board_in("B", "Link").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(
            fs::read_dir(outside.path()).unwrap().next().is_none(),
            "nothing written outside"
        );
    }

    /// GK P2: containment is checked before any folder is made, so a
    /// subfolder of a link that points out of the vault is not created
    /// outside it on the way to being refused.
    #[cfg(unix)]
    #[test]
    fn a_refused_board_folder_is_not_created_outside_the_vault() {
        let tmp = TempVault::new(&[]);
        let outside = TempVault::new(&[]);
        std::os::unix::fs::symlink(outside.path(), tmp.path().join("Link")).unwrap();
        let v = vault(&tmp);
        let err = v.create_board_in("B", "Link/New/Deeper").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(
            !outside.path().join("New").exists(),
            "no folder made outside the vault"
        );
    }
}

/// Board rename: `Vault::rename_board`, `board_name` and the embed rewrite.
#[cfg(test)]
mod board_rename_tests {
    use std::fs;
    use std::io::ErrorKind;

    use crate::testing::TempVault;
    use crate::{Vault, board_name};

    const EMPTY: &str =
        r#"{"type":"excalidraw","version":2,"elements":[],"appState":{},"files":{}}"#;

    fn vault(tmp: &TempVault) -> Vault {
        Vault::load(tmp.path()).unwrap()
    }

    #[test]
    fn a_board_is_renamed_in_place_with_its_bytes_and_the_extension_implicit() {
        let tmp = TempVault::new(&[("Ideas/Untitled board.excalidraw", EMPTY)]);
        let v = vault(&tmp);
        let old = tmp.path().join("Ideas/Untitled board.excalidraw");
        let new = v.rename_board(&old, "Roadmap").unwrap();
        assert_eq!(new, tmp.path().join("Ideas/Roadmap.excalidraw"));
        assert!(!old.exists(), "the old file is gone");
        assert_eq!(fs::read_to_string(&new).unwrap(), EMPTY, "same bytes");
        // Typing the extension does not double it.
        let again = v.rename_board(&new, "Plan.excalidraw").unwrap();
        assert_eq!(again, tmp.path().join("Ideas/Plan.excalidraw"));
        // The same name is a no-op, not a collision with itself.
        assert_eq!(v.rename_board(&again, " Plan ").unwrap(), again);
    }

    #[test]
    fn a_board_rename_never_overwrites_another_file() {
        let tmp = TempVault::new(&[("A.excalidraw", EMPTY), ("B.excalidraw", "keep")]);
        let v = vault(&tmp);
        let err = v
            .rename_board(&tmp.path().join("A.excalidraw"), "B")
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read_to_string(tmp.path().join("B.excalidraw")).unwrap(),
            "keep"
        );
        assert!(tmp.path().join("A.excalidraw").is_file());
    }

    /// GK B-1: `board_name` compared the last 11 bytes with `.excalidraw`
    /// by slicing, which panicked when that cut fell inside a multibyte
    /// character (`é ünïcode` is 12 bytes; the cut at byte 1 is inside `é`).
    /// Every name of 11–14 bytes built from 2-, 3- and 4-byte characters,
    /// with and without a typed extension, is accepted unchanged.
    #[test]
    fn multibyte_board_names_of_11_to_14_bytes_are_accepted_without_panicking() {
        let mut names = vec!["é ünïcode".to_owned(), "ünïcode ñé".to_owned()];
        for total in 11..=14 {
            for wide in ['é', '€', '😀'] {
                for pad in 0..wide.len_utf8() {
                    let mut name = "a".repeat(pad);
                    while name.len() + wide.len_utf8() <= total {
                        name.push(wide);
                    }
                    name.push_str(&"b".repeat(total - name.len()));
                    names.push(name);
                }
            }
        }
        let mid_char = names
            .iter()
            .filter(|n| n.len() >= 11 && !n.is_char_boundary(n.len() - 11))
            .count();
        assert!(
            mid_char >= 8,
            "the cut lands inside a character ({mid_char})"
        );
        for name in &names {
            assert!((11..=14).contains(&name.len()), "{name:?}");
            assert_eq!(board_name(name).unwrap(), *name, "{name:?}");
            for ext in [".excalidraw", ".EXCALIDRAW", " .excalidraw"] {
                let typed = format!("{name}{ext}");
                assert_eq!(board_name(&typed).unwrap(), *name, "{typed:?}");
            }
        }
        // A multibyte name that only looks like the extension is kept whole.
        assert_eq!(board_name("ｅxcalidraw").unwrap(), "ｅxcalidraw");
        assert_eq!(board_name("é.excalidraw").unwrap(), "é");
        // And the rename itself goes through with it.
        let tmp = TempVault::new(&[("Ideas/Untitled board.excalidraw", EMPTY)]);
        let v = vault(&tmp);
        let old = tmp.path().join("Ideas/Untitled board.excalidraw");
        let new = v.rename_board(&old, "é ünïcode").unwrap();
        assert_eq!(new, tmp.path().join("Ideas/é ünïcode.excalidraw"));
        assert_eq!(fs::read_to_string(&new).unwrap(), EMPTY);
    }

    /// The B-1 sweep: every other path that takes a name the user typed
    /// (new note, note rename, new folder, folder rename, New board, link
    /// suggestions) handles multibyte names of the same awkward lengths.
    #[test]
    fn multibyte_names_through_every_name_taking_path() {
        let tmp = TempVault::new(&[("Home.md", "# Home\n")]);
        let mut v = vault(&tmp);
        let note = v.create_note_in("é ünïcode", "").unwrap();
        assert_eq!(note, tmp.path().join("é ünïcode.md"));
        v = vault(&tmp);
        let id = v.resolve("é ünïcode").expect("the new note resolves");
        let renamed = v.rename_note(id, "ünïcode ñé").unwrap();
        assert!(renamed.ends_with("ünïcode ñé.md"), "{renamed:?}");
        v = vault(&tmp);
        assert!(!v.suggest_links("ünï", 5).is_empty(), "suggestions find it");
        assert!(v.suggest_links("é", 5).len() <= 5);
        let folder = v.create_folder("", "😀 ünïcode").unwrap();
        assert!(folder.is_dir());
        v = vault(&tmp);
        let moved = v.rename_folder("😀 ünïcode", "€€€ ünï").unwrap();
        assert!(moved.ends_with("€€€ ünï"), "{moved:?}");
        let board = v.create_board_in("é ünïcode", "€€€ ünï").unwrap();
        assert_eq!(board, tmp.path().join("€€€ ünï/é ünïcode.excalidraw"));
        let again = v.create_board_in("é ünïcode", "€€€ ünï").unwrap();
        assert_eq!(again, tmp.path().join("€€€ ünï/é ünïcode 2.excalidraw"));
    }

    #[test]
    fn invalid_board_names_are_refused_not_rewritten() {
        for bad in [
            "",
            "   ",
            ".excalidraw",
            "a/b",
            "a\\b",
            "..",
            ".hidden",
            "x:y",
            "what?",
            "tab\there",
            "trailing.",
            "CON",
            "nul.excalidraw",
        ] {
            let err = board_name(bad).expect_err(bad);
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad:?}");
        }
        assert_eq!(board_name("  Road map  ").unwrap(), "Road map");
        assert_eq!(board_name("Plan.EXCALIDRAW").unwrap(), "Plan");
        assert_eq!(board_name("v1.2 plan").unwrap(), "v1.2 plan");
        let tmp = TempVault::new(&[("A.excalidraw", EMPTY)]);
        let v = vault(&tmp);
        let a = tmp.path().join("A.excalidraw");
        assert_eq!(
            v.rename_board(&a, "../Out").unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
        assert!(a.is_file(), "a refused rename leaves the board alone");
    }

    #[cfg(unix)]
    #[test]
    fn a_board_outside_the_vault_or_behind_a_link_is_not_renamed() {
        let tmp = TempVault::new(&[]);
        let outside = TempVault::new(&[("Far.excalidraw", EMPTY)]);
        std::os::unix::fs::symlink(outside.path(), tmp.path().join("Link")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("Far.excalidraw"),
            tmp.path().join("Alias.excalidraw"),
        )
        .unwrap();
        let v = vault(&tmp);
        for path in [
            tmp.path().join("Link/Far.excalidraw"),
            tmp.path().join("Alias.excalidraw"),
            outside.path().join("Far.excalidraw"),
        ] {
            let err = v.rename_board(&path, "Moved").unwrap_err();
            assert_eq!(err.kind(), ErrorKind::PermissionDenied, "{path:?}");
        }
        assert!(outside.path().join("Far.excalidraw").is_file());
        assert!(!outside.path().join("Moved.excalidraw").exists());
        assert!(!tmp.path().join("Moved.excalidraw").exists());
    }

    #[test]
    fn embeds_and_links_to_a_renamed_board_follow_it() {
        let tmp = TempVault::new(&[
            ("Ideas/Plan.excalidraw", EMPTY),
            ("Other/Plan.excalidraw", EMPTY),
            (
                "Ideas/Note.md",
                "See ![[Plan.excalidraw]] and ![[Plan.excalidraw|the plan]], [[Plan.excalidraw]].\n",
            ),
            (
                "Home.md",
                "![[Ideas/Plan.excalidraw]] ![[Other/Plan.excalidraw]] [[Plan]]\n",
            ),
        ]);
        let mut v = vault(&tmp);
        let old = tmp.path().join("Ideas/Plan.excalidraw");
        let inbound = v.board_embed_referrers(&old);
        assert_eq!(inbound.len(), 2, "{inbound:?}");
        let new = v.rename_board(&old, "Roadmap").unwrap();
        let failures = v.rewrite_board_embeds(&inbound, &new);
        assert!(failures.is_empty());
        assert_eq!(
            fs::read_to_string(tmp.path().join("Ideas/Note.md")).unwrap(),
            "See ![[Roadmap.excalidraw]] and ![[Roadmap.excalidraw|the plan]], [[Roadmap.excalidraw]].\n"
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join("Home.md")).unwrap(),
            "![[Ideas/Roadmap.excalidraw]] ![[Other/Plan.excalidraw]] [[Plan]]\n",
            "the other Plan board and the note link [[Plan]] are left alone"
        );
        let note = v.notes.iter().find(|n| n.rel == "Home.md").unwrap();
        assert!(
            note.source.contains("Ideas/Roadmap.excalidraw"),
            "the index follows"
        );
    }
}
