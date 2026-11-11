//! Rewrite wikilinks into ordinary Markdown links for the reader view.

use std::path::{Path, PathBuf};

use crate::vault::Vault;
use crate::wikilink::{WikiLink, parse_links};

/// One attachment embed (`![[pic.png]]`) found in a note's source.
///
/// The reader shows a filename and size instead of the image bytes: Rune's
/// Markdown view has no image pipeline in this revision, and its image cache
/// is only created from an `AppSpec::setup` hook Muninn does not own here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    /// The target exactly as written (`assets/pic.png`).
    pub target: String,
    /// The last path segment (`pic.png`), used as the visible label.
    pub name: String,
    /// An optional display alias (`![[pic.png|the shot]]`).
    pub alias: Option<String>,
    /// The absolute file path when the attachment exists in the vault.
    pub path: Option<PathBuf>,
    /// File size in bytes when the file exists and its metadata is readable.
    pub size: Option<u64>,
}

impl Attachment {
    /// `12.4 KB`-style size for the placeholder, or `None` for a missing file.
    #[must_use]
    pub fn size_label(&self) -> Option<String> {
        self.size.map(human_size)
    }

    /// True for an embedded Excalidraw board (`![[Plan.excalidraw]]`).
    #[must_use]
    pub fn is_board(&self) -> bool {
        is_board_name(&self.name)
    }

    /// True when [`Attachment::path`] exists on disk.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.path.is_some()
    }
}

impl Vault {
    /// Rewrite `[[wikilinks]]` into ordinary Markdown links: `note:<stem>`
    /// when a note exists, `missing:<target>` when it doesn't. The destination
    /// is the vault-relative stem (not the bare title) so a link to a nested
    /// or duplicate-titled note opens the right file without relying on the
    /// duplicate-title fallback. Code spans and fenced blocks keep their
    /// brackets.
    #[must_use]
    pub fn preview(&self, source: &str, current: usize) -> String {
        let links = parse_links(source);
        if links.is_empty() {
            return source.to_owned();
        }
        let mut out = String::with_capacity(source.len() + links.len() * 24);
        let mut last = 0;
        for link in links {
            out.push_str(&source[last..link.start]);
            let label = link.label();
            let resolved = if link.target.is_empty() {
                Some(current)
            } else {
                self.resolve(&link.target)
            };
            let dest = match resolved {
                // EDT-04: keep the heading in the destination so `open_link`
                // can place the caret (Write) or scroll (Read) after resolving
                // the note.
                Some(id) => match &link.heading {
                    Some(heading) => format!("note:{}#{heading}", self.notes[id].stem()),
                    None => format!("note:{}", self.notes[id].stem()),
                },
                None => format!("missing:{}", link.target),
            };
            out.push('[');
            escape_label(&label, &mut out);
            out.push_str("](<");
            for c in dest.chars() {
                match c {
                    '<' => out.push_str("%3C"),
                    '>' => out.push_str("%3E"),
                    '\n' | '\r' => out.push(' '),
                    c => out.push(c),
                }
            }
            out.push_str(">)");
            last = link.end;
        }
        out.push_str(&source[last..]);
        out
    }

    /// DES-02: the reader source. Attachment embeds (`![[pic.png]]`) become a
    /// filename-and-size placeholder before the ordinary wikilink rewrite runs,
    /// because Rune's Markdown view has no image pipeline and would otherwise
    /// paint them as faint missing note links.
    #[must_use]
    pub fn preview_with_attachments(&self, source: &str, current: usize) -> String {
        let source = self.rewrite_attachments(source, current);
        self.preview(&source, current)
    }

    /// Every distinct attachment embed in `source`, in reading order, with its
    /// resolved vault path and size when the file exists.
    ///
    /// This is the metadata behind the reader's attachment strip: the Markdown
    /// body cannot draw an image, so the Open button opens the real file with
    /// the platform handler instead.
    #[must_use]
    pub fn attachments(&self, source: &str, current: usize) -> Vec<Attachment> {
        let mut out: Vec<Attachment> = Vec::new();
        for link in attachment_links(source) {
            let Some(name) = attachment_name(&link.target).map(str::to_owned) else {
                continue;
            };
            if out
                .iter()
                .any(|seen| seen.target.eq_ignore_ascii_case(&link.target))
            {
                continue;
            }
            let path = self
                .attachment_path(&link.target, current)
                .and_then(|path| fs_canonical(&path));
            let size = path.as_deref().and_then(file_size);
            out.push(Attachment {
                target: link.target,
                name,
                alias: link.alias,
                path,
                size,
            });
        }
        out
    }

    /// Resolve an attachment target to a file inside this vault.
    ///
    /// Resolution is deliberately conservative: try the note's own folder,
    /// then the vault root, then every scanned folder by full relative target
    /// and by bare filename (so `![[pic.png]]` finds `assets/pic.png`). `..`
    /// and absolute targets are refused; the result must be an existing file.
    #[must_use]
    pub fn attachment_path(&self, target: &str, current: usize) -> Option<PathBuf> {
        let rel = target.trim().replace('\\', "/");
        if rel.starts_with('/') {
            return None;
        }
        let rel = rel.trim_start_matches("./");
        if rel.is_empty()
            || rel
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return None;
        }
        let name = rel.rsplit('/').next().unwrap_or(rel);
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(note) = self.notes.get(current) {
            let folder = note.folder();
            if !folder.is_empty() {
                candidates.push(self.root.join(folder).join(rel));
            }
        }
        candidates.push(self.root.join(rel));
        for dir in &self.dirs {
            let base = if dir.is_empty() {
                self.root.clone()
            } else {
                self.root.join(dir)
            };
            candidates.push(base.join(rel));
            if name != rel {
                candidates.push(base.join(name));
            }
        }
        candidates.into_iter().find(|path| path.is_file())
    }

    /// Replace every `![[attachment]]` span with a visible placeholder, leaving
    /// note embeds and plain links for [`Vault::preview`].
    fn rewrite_attachments(&self, source: &str, current: usize) -> String {
        let embeds = attachment_links(source);
        if embeds.is_empty() {
            return source.to_owned();
        }
        let mut out = String::with_capacity(source.len() + embeds.len() * 32);
        let mut last = 0;
        for link in embeds {
            out.push_str(&source[last..link.start - 1]);
            let size = self
                .attachment_path(&link.target, current)
                .as_deref()
                .and_then(file_size);
            attachment_placeholder(&link, size, &mut out);
            last = link.end;
        }
        out.push_str(&source[last..]);
        out
    }
}

/// Attachment embeds in reading order. A plain `[[pic.png]]` (no `!`) and a
/// `![[Note]]` note embed are deliberately left to [`Vault::preview`].
fn attachment_links(source: &str) -> Vec<WikiLink> {
    parse_links(source)
        .into_iter()
        .filter(|link| is_attachment_embed(source, link.start, &link.target))
        .collect()
}

fn fs_canonical(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|meta| meta.len())
}

/// `999 B`, `12.4 KB`, `3.1 MB`; binary units keep the number short.
fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let bytes_f = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes_f < KB * KB {
        format!("{:.1} KB", bytes_f / KB)
    } else if bytes_f < KB * KB * KB {
        format!("{:.1} MB", bytes_f / (KB * KB))
    } else {
        format!("{:.1} GB", bytes_f / (KB * KB * KB))
    }
}

/// True when `![[target]]` (the `!` is the byte before `start`) names an
/// attachment: a non-Markdown extension, optionally path-qualified. A bare
/// `![[Note]]` keeps the ordinary link rewrite.
fn is_attachment_embed(source: &str, start: usize, target: &str) -> bool {
    let bang = start > 0
        && source.as_bytes()[start - 1] == b'!'
        && (start < 2 || source.as_bytes()[start - 2] != b'\\');
    if !bang || target.is_empty() {
        return false;
    }
    attachment_name(target).is_some()
}

/// The file name of an attachment target (`assets/pic.png` → `pic.png`), or
/// `None` when the extension is Markdown or missing.
fn attachment_name(target: &str) -> Option<&str> {
    let name = target.rsplit(['/', '\\']).next().unwrap_or(target);
    let (stem, extension) = name.rsplit_once('.')?;
    (!stem.is_empty() && !extension.is_empty() && !extension.eq_ignore_ascii_case("md"))
        .then_some(name)
}

/// `true` for an Excalidraw board's file name (`Plan.excalidraw`).
pub(crate) fn is_board_name(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(crate::BOARD_EXTENSION))
}

/// A reader-visible placeholder for an attachment embed. Rune has no image
/// pipeline and `Tx::open_url` refuses `file:` links, so the filename and size
/// are shown in an inline code span and marked as an attachment; the file
/// itself stays opaque (the vault scan ignores non-`.md` files) and the
/// reader's attachment strip offers an Open button.
fn attachment_placeholder(link: &WikiLink, size: Option<u64>, out: &mut String) {
    let name = attachment_name(&link.target).unwrap_or(&link.target);
    out.push('`');
    for c in name.chars() {
        // A backtick would close the code span and turn the rest into text.
        out.push(if c == '`' { '\'' } else { c });
    }
    // Board slice 7: an embedded Excalidraw board says so; the reader's
    // strip opens it on Board.
    out.push_str(if is_board_name(name) {
        "` (board"
    } else {
        "` (attachment"
    });
    if let Some(alias) = &link.alias {
        out.push_str(" — ");
        escape_label(alias, out);
    }
    out.push(')');
    match size {
        Some(bytes) => {
            out.push_str(" · ");
            out.push_str(&human_size(bytes));
        }
        None => out.push_str(" · missing"),
    }
}

pub(crate) fn escape_label(label: &str, out: &mut String) {
    for c in label.chars() {
        match c {
            '[' | ']' => {
                out.push('\\');
                out.push(c);
            }
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
}

pub(crate) fn excerpt(line: &str) -> String {
    // Show the sentence, not its Markdown: wikilinks become their labels and list markers go.
    let mut plain = String::with_capacity(line.len());
    let mut last = 0;
    for link in parse_links(line) {
        plain.push_str(&line[last..link.start]);
        plain.push_str(&link.label());
        last = link.end;
    }
    plain.push_str(&line[last..]);
    let mut line = plain.trim().trim_start_matches(['#', '>', ' ']).trim();
    if let Some(rest) = line
        .strip_prefix("- [ ] ")
        .or_else(|| line.strip_prefix("- [x] "))
    {
        line = rest;
    }
    line = line.trim_start_matches(['-', '*', ' ']).trim();
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0
        && let Some(rest) = line[digits..]
            .strip_prefix(". ")
            .or_else(|| line[digits..].strip_prefix(") "))
    {
        line = rest;
    }
    let cleaned: String = line.chars().filter(|c| !matches!(c, '*' | '`')).collect();
    let mut out: String = cleaned.chars().take(92).collect();
    if cleaned.chars().count() > 92 {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::testing::TempVault;

    #[test]
    fn preview_rewrites_only_real_links() {
        let src = "a [[Real]] link\n\n`[[Not]]` and ``[[AlsoNot]]``\n\n```\n[[Fenced]]\n```\n\nend [[Last]]\n";
        let tmp = TempVault::new(&[("Real.md", "# Real\n")]);
        let v = tmp.load();
        let out = v.preview(src, 0);
        assert!(out.contains("[Real](<note:Real>)"), "{out}");
        assert!(out.contains("`[[Not]]`"));
        assert!(out.contains("[[Fenced]]"));
        assert!(out.contains("[Last](<missing:Last>)"));
        assert!(!out.contains("[[Real]]"));
    }

    #[test]
    fn aliases_headings_and_self_links_resolve() {
        let tmp = TempVault::new(&[
            (
                "Home.md",
                "[[Target#Part|the part]] and [[#Local]] and [[Target]]",
            ),
            ("Target.md", "# Part\n"),
        ]);
        let v = tmp.load();
        let home = v.resolve("Home").unwrap();
        let target = v.resolve("Target").unwrap();
        assert_eq!(v.note(home).links, vec![target]);
        let out = v.preview(&v.note(home).source, home);
        assert!(out.contains("[the part](<note:Target#Part>)"), "{out}");
        assert!(out.contains("[Local](<note:Home#Local>)"), "{out}");
    }

    #[test]
    fn attachment_embeds_render_a_placeholder_not_a_missing_link() {
        let tmp = TempVault::new(&[("Home.md", "![[pic.png]] and ![[assets/shot.jpg|A shot]]\n")]);
        let v = tmp.load();
        let home = v.resolve("Home").unwrap();
        let out = v.preview_with_attachments(&v.note(home).source, home);
        assert!(out.contains("`pic.png` (attachment)"), "{out}");
        assert!(out.contains("`shot.jpg` (attachment — A shot)"), "{out}");
        assert!(!out.contains("missing:pic.png"), "{out}");
        assert!(!out.contains("![[pic.png]]"), "{out}");
        // A plain (non-embed) attachment link and a ![[Note]] embed are not
        // attachment placeholders: note embeds still rewrite as note links.
        let out = v.preview("[[pic.png]] and ![[Other]]", home);
        assert!(out.contains("missing:pic.png"), "{out}");
        assert!(out.contains("[Other](<missing:Other>)"), "{out}");
    }

    #[test]
    fn a_board_embed_renders_a_board_placeholder() {
        let tmp = TempVault::new(&[
            (
                "Home.md",
                "![[Plan.excalidraw]] and ![[Gone.excalidraw|old plan]]\n",
            ),
            ("Plan.excalidraw", "{}"),
        ]);
        let v = tmp.load();
        let home = v.resolve("Home").unwrap();
        let out = v.preview_with_attachments(&v.note(home).source, home);
        assert!(out.contains("`Plan.excalidraw` (board) · "), "{out}");
        assert!(
            out.contains("`Gone.excalidraw` (board — old plan) · missing"),
            "{out}"
        );
        assert!(!out.contains("(attachment"), "{out}");
    }

    #[test]
    fn nested_preview_uses_the_full_stem() {
        let tmp = TempVault::new(&[
            ("Home.md", "see [[b/note]] and [[note]]"),
            ("a/note.md", "a"),
            ("b/note.md", "b"),
        ]);
        let v = tmp.load();
        let home = v.resolve("Home").unwrap();
        let out = v.preview(&v.note(home).source, home);
        // The path-qualified link keeps its folder; the bare duplicate goes
        // to the deterministic smallest-rel note but still renders its stem.
        assert!(out.contains("note:b/note"), "{out}");
        assert!(out.contains("note:a/note"), "{out}");
    }
}
