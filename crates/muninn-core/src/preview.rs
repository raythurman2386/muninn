//! Rewrite wikilinks into ordinary Markdown links for the reader view.

use crate::vault::Vault;
use crate::wikilink::parse_links;

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
                Some(id) => format!("note:{}", self.notes[id].stem()),
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
        assert!(out.contains("[the part](<note:Target>)"), "{out}");
        assert!(out.contains("[Local](<note:Home>)"), "{out}");
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
