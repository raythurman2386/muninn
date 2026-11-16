//! `xtask i18n-lint`: fail when user-visible copy is a string literal.
//!
//! Every sentence a user can read belongs in `muninn-i18n` behind a `Key`
//! or a typed helper. What remains literal is deliberately not copy: element
//! keys, JSON fields, paths, extensions, keycaps and chords, file-format
//! examples, SVG markup and developer fixtures. The scan checks the argument
//! of the draw calls that take copy, skips test modules and comments, and
//! fails on anything the allow rules below do not classify.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::Result;

/// Call shapes that take user-visible copy directly.
///
/// `text(` is Rune's free function (`use rune::... text`); a method call
/// `.text(` is a frontmatter/JSON lookup, not a draw call, so the pattern
/// requires a word boundary and rejects a preceding dot. `TrayItem::action`
/// takes the id first and the label second, so the scanner reads its second
/// argument. `error` and `notice` may wrap the value in `Some(`; a `format!`,
/// `t(...)` or helper call there is not a plain literal and passes.
const COPY_SHAPES: &[&str] = &[
    "text(",
    ".label(",
    ".help(",
    ".placeholder(",
    "Button::new(",
    "TrayItem::action(",
    "status =",
    "error =",
    "notice =",
];

/// Characters that make up keyboard glyphs and chords (`⌘K`, `‹`, `T / 8`).
const KEYCAP_GLYPHS: &str = "⌘⌥⌃⇧‹›✕↳◐?:—•/−+ ";

/// Words that name a key on their own or inside a chord (`Page Up`, `Esc`).
const KEYCAP_WORDS: &[&str] = &[
    "Esc",
    "Tab",
    "Enter",
    "Return",
    "Shift",
    "Ctrl",
    "Alt",
    "Cmd",
    "Super",
    "Space",
    "Backspace",
    "Delete",
    "Home",
    "End",
    "Page",
    "Up",
    "Down",
    "Left",
    "Right",
    "Ins",
    "PgUp",
    "PgDn",
];

/// Literals that pass every general rule but are still not UI copy.
///
/// Keep this table short and give each entry a reason. A new entry needs a
/// reason that is about the string itself, not the file it lives in: if a
/// sentence a user can read reaches this table, the fix is a `Key` in
/// `muninn-i18n`, not a row here.
const ALLOWED_LITERALS: &[(&str, &str)] = &[
    // `--snap` scene `settings-sync`: fixed status for the Settings › Sync
    // review screenshot. Developer fixture; never drawn outside `--snap`.
    (
        "Scope saved for this vault. No token is stored in settings.",
        "snapshot scene status fixture",
    ),
    // `--snap` scene `settings-sync-drive`: the Drive tab screenshot.
    (
        "Google Drive keeps an app-managed commit history in Muninn's private app data, not editable Markdown mirrors.",
        "snapshot scene status fixture",
    ),
    // `--snap` scene `settings-sync-tailscale`: the Join-host screenshot.
    (
        "Join a host. Enter the host's full MagicDNS name and paste the key from Copy key on the host.",
        "snapshot scene status fixture",
    ),
    // `--snap` scene `settings-sync-conflict`: the two-device screenshot.
    (
        "Last successful sync 1a2b3c4d. A note changed on two devices; both versions were kept: Daily (remote).md.",
        "snapshot scene status fixture",
    ),
];

/// One literal that could be user copy, with the shape that produced it.
#[derive(Debug, PartialEq, Eq)]
struct CopyLiteral {
    /// 1-based line of the literal's opening quote.
    line: usize,
    /// The unescaped text of the literal.
    literal: String,
    /// The call shape that takes copy here, for the failure message.
    shape: &'static str,
}

/// Scan every `.rs` under `crates/*/src/` for copy passed as a literal.
pub(crate) fn run() -> Result<()> {
    let root = crate::workspace_root()?;
    let files = source_files(&root)?;
    let test_modules = test_module_files(&files);
    let mut findings = Vec::new();
    let mut scanned = 0;
    for file in &files {
        if test_modules.contains(file) {
            continue;
        }
        let source = std::fs::read_to_string(file)?;
        scanned += 1;
        for hit in scan_source(&source) {
            if allow_reason(&hit.literal).is_some() {
                continue;
            }
            let path = file.strip_prefix(&root).unwrap_or(file);
            findings.push((path.display().to_string(), hit));
        }
    }
    if findings.is_empty() {
        println!("i18n ok ({scanned} source files, no copy literals outside the catalog)");
        Ok(())
    } else {
        for (path, hit) in &findings {
            println!(
                "{}:{}: {:?} ({} argument) — pass a muninn_i18n Key to t(...) or a typed helper",
                path, hit.line, hit.literal, hit.shape
            );
        }
        Err(format!(
            "{} user-copy literal(s) outside muninn-i18n",
            findings.len()
        )
        .into())
    }
}

/// Every `.rs` below `crates/<crate>/src`, in a stable order.
fn source_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root.join("crates"))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let src = entry.path().join("src");
        if src.is_dir() {
            collect_rs(&src, &mut files)?;
        }
    }
    files.sort();
    Ok(files)
}

fn collect_rs(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_rs(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

/// `#[cfg(test)] mod name;` files, plus modules those files declare.
///
/// Inline `#[cfg(test)] mod tests { ... }` blocks are handled by
/// [`scan_source`]; a declaration has no braces, so the file is skipped here.
fn test_module_files(files: &[PathBuf]) -> BTreeSet<PathBuf> {
    let mut skipped = BTreeSet::new();
    for file in files {
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        mark_modules(file, &declared_test_modules(&source), &mut skipped);
    }
    // A test-only file's own `mod` declarations are test-only too.
    loop {
        let mut changed = false;
        for file in skipped.iter().cloned().collect::<Vec<_>>() {
            let Ok(source) = std::fs::read_to_string(&file) else {
                continue;
            };
            let before = skipped.len();
            mark_modules(&file, &declared_modules(&source), &mut skipped);
            changed |= skipped.len() != before;
        }
        if !changed {
            break;
        }
    }
    skipped
}

fn mark_modules(file: &Path, names: &[String], skipped: &mut BTreeSet<PathBuf>) {
    for name in names {
        for candidate in module_candidates(file, name) {
            if candidate.is_file() {
                skipped.insert(candidate);
            }
        }
    }
}

fn module_candidates(file: &Path, name: &str) -> [PathBuf; 2] {
    let Some(parent) = file.parent() else {
        return [PathBuf::new(), PathBuf::new()];
    };
    let base = match file.file_stem().and_then(|stem| stem.to_str()) {
        Some("lib" | "main" | "mod") => parent.to_path_buf(),
        Some(stem) => parent.join(stem),
        None => parent.to_path_buf(),
    };
    [
        base.join(format!("{name}.rs")),
        base.join(name).join("mod.rs"),
    ]
}

fn declared_test_modules(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut after_attribute = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("#[cfg(test)]") {
            let rest = rest.trim();
            if let Some(name) = module_declaration(rest) {
                names.push(name);
                after_attribute = false;
            } else {
                after_attribute = rest.is_empty();
            }
            continue;
        }
        if after_attribute {
            after_attribute = false;
            if trimmed.starts_with("#[") {
                after_attribute = true;
            } else if let Some(name) = module_declaration(trimmed) {
                names.push(name);
            }
        }
    }
    names
}

fn declared_modules(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| module_declaration(line.trim()))
        .collect()
}

fn module_declaration(text: &str) -> Option<String> {
    let name = text.strip_prefix("mod ")?.strip_suffix(';')?.trim();
    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Some(name.to_owned())
    } else {
        None
    }
}

/// The literals in `source` that the copy-call shapes pass as their argument.
fn scan_source(source: &str) -> Vec<CopyLiteral> {
    let mask = code_mask(source);
    let starts = line_starts(source);
    let mut state = ScanState::default();
    let mut hits = Vec::new();
    for (index, &start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(source.len());
        if !state.skipping {
            collect_hits(source, &mask, &starts, start, end, &mut hits);
        }
        state.advance(source, &mask, start, end);
    }
    hits
}

/// Brace depth plus the `#[cfg(test)]` block the scanner is inside.
#[derive(Default)]
struct ScanState {
    depth: usize,
    skip_at: Option<usize>,
    skipping: bool,
    pending_test: bool,
}

impl ScanState {
    fn advance(&mut self, source: &str, mask: &[bool], start: usize, end: usize) {
        if contains_code(source, mask, start, end, "#[cfg(test)]") {
            self.pending_test = true;
        }
        let bytes = source.as_bytes();
        for i in start..end {
            if !mask[i] {
                continue;
            }
            match bytes[i] {
                b'{' => {
                    self.depth += 1;
                    if self.pending_test {
                        self.skip_at = Some(self.depth);
                        self.pending_test = false;
                    }
                }
                b'}' => {
                    if self.skip_at == Some(self.depth) {
                        self.skip_at = None;
                    }
                    self.depth = self.depth.saturating_sub(1);
                }
                b';' => self.pending_test = false,
                _ => {}
            }
        }
        self.skipping = self.skip_at.is_some();
    }
}

fn collect_hits(
    source: &str,
    mask: &[bool],
    starts: &[usize],
    start: usize,
    end: usize,
    hits: &mut Vec<CopyLiteral>,
) {
    for &shape in COPY_SHAPES {
        let mut from = start;
        while let Some(relative) = source[from..end].find(shape) {
            let at = from + relative;
            from = at + shape.len();
            if !mask[at] || !boundary_before(source, at, shape) {
                continue;
            }
            let argument = match shape {
                "TrayItem::action(" => second_argument(source, mask, at + shape.len()),
                "error =" | "notice =" => Some(some_argument(source, at + shape.len())),
                _ => Some(skip_trivia(source, at + shape.len())),
            };
            let Some(argument) = argument else {
                continue;
            };
            let Some((literal, _)) = parse_literal(source, argument) else {
                continue;
            };
            let line = starts.partition_point(|&offset| offset <= argument);
            hits.push(CopyLiteral {
                line,
                literal,
                shape,
            });
        }
    }
}

/// `text(` is the Rune free function; `.text(` is a data lookup.
fn boundary_before(source: &str, at: usize, shape: &str) -> bool {
    let Some(previous) = source[..at].chars().next_back() else {
        return true;
    };
    if previous.is_alphanumeric() || previous == '_' {
        return false;
    }
    !(shape == "text(" && previous == '.')
}

fn contains_code(source: &str, mask: &[bool], start: usize, end: usize, needle: &str) -> bool {
    let mut from = start;
    while let Some(relative) = source[from..end].find(needle) {
        let at = from + relative;
        if mask[at] {
            return true;
        }
        from = at + needle.len();
        if from >= end {
            break;
        }
    }
    false
}

/// Skip whitespace and comments between a shape and its argument.
fn skip_trivia(source: &str, mut at: usize) -> usize {
    let bytes = source.as_bytes();
    loop {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if source[at..].starts_with("//") {
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        if source[at..].starts_with("/*") {
            match source[at + 2..].find("*/") {
                Some(rel) => {
                    at += 2 + rel + 2;
                    continue;
                }
                None => return bytes.len(),
            }
        }
        return at;
    }
}

/// The start of the second argument of an open call, when it is at top level.
fn second_argument(source: &str, mask: &[bool], open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut at = open;
    while at < bytes.len() {
        if mask[at] {
            match bytes[at] {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    if depth == 0 {
                        return None;
                    }
                    depth -= 1;
                }
                b',' if depth == 0 => return Some(skip_trivia(source, at + 1)),
                _ => {}
            }
        }
        at += 1;
    }
    None
}

/// The argument of `error =` / `notice =`, past an optional `Some(` wrapper.
fn some_argument(source: &str, at: usize) -> usize {
    let at = skip_trivia(source, at);
    if !source[at..].starts_with("Some") {
        return at;
    }
    let after = skip_trivia(source, at + "Some".len());
    if source[after..].starts_with('(') {
        skip_trivia(source, after + 1)
    } else {
        at
    }
}

/// Read the string literal at `at`: `"…"` or `r#"…"#`.
///
/// Backslash escapes are decoded enough to test the text; `\` at the end of a
/// line joins it to the next line, as Rust does.
fn parse_literal(source: &str, at: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    if bytes.get(at) == Some(&b'r') {
        let mut cursor = at + 1;
        let mut hashes = 0;
        while bytes.get(cursor) == Some(&b'#') {
            hashes += 1;
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'"') {
            return None;
        }
        let content_start = cursor + 1;
        let terminator = format!("\"{}", "#".repeat(hashes));
        let length = source[content_start..].find(&terminator)?;
        let content_end = content_start + length;
        return Some((
            source[content_start..content_end].to_owned(),
            content_end + terminator.len(),
        ));
    }
    if bytes.get(at) != Some(&b'"') {
        return None;
    }
    let mut literal = String::new();
    let mut cursor = at + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => match bytes.get(cursor + 1) {
                Some(b'\n') => {
                    cursor += 2;
                    while matches!(bytes.get(cursor), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                        cursor += 1;
                    }
                }
                Some(b'n') => {
                    literal.push('\n');
                    cursor += 2;
                }
                Some(b't') => {
                    literal.push('\t');
                    cursor += 2;
                }
                Some(b'r') => {
                    literal.push('\r');
                    cursor += 2;
                }
                Some(b'0') => {
                    literal.push('\0');
                    cursor += 2;
                }
                Some(_) => {
                    let character = source[cursor + 1..].chars().next()?;
                    literal.push(character);
                    cursor += 1 + character.len_utf8();
                }
                None => {
                    literal.push('\\');
                    cursor += 1;
                }
            },
            b'"' => return Some((literal, cursor + 1)),
            _ => {
                let character = source[cursor..].chars().next()?;
                literal.push(character);
                cursor += character.len_utf8();
            }
        }
    }
    None
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

/// Mark the bytes that are code, not string contents, character literals or
/// comments. Brace counting and pattern matching both read this mask.
fn code_mask(source: &str) -> Vec<bool> {
    let bytes = source.as_bytes();
    let mut mask = vec![false; bytes.len()];
    let mut at = 0;
    let mut block_depth = 0usize;
    while at < bytes.len() {
        if block_depth > 0 {
            if bytes[at..].starts_with(b"/*") {
                block_depth += 1;
                at += 2;
            } else if bytes[at..].starts_with(b"*/") {
                block_depth -= 1;
                at += 2;
            } else {
                at += 1;
            }
            continue;
        }
        if bytes[at..].starts_with(b"//") {
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            block_depth = 1;
            at += 2;
            continue;
        }
        if let Some(end) = raw_string_end(source, at) {
            at = end;
            continue;
        }
        if bytes[at] == b'"' || is_byte_string_start(bytes, at) {
            at = quoted_end(source, at + usize::from(bytes[at] != b'"'));
            continue;
        }
        if bytes[at] == b'\''
            && let Some(end) = character_end(bytes, at)
        {
            at = end;
            continue;
        }
        mask[at] = true;
        at += 1;
    }
    mask
}

fn is_byte_string_start(bytes: &[u8], at: usize) -> bool {
    matches!(bytes[at], b'b' | b'c')
        && bytes.get(at + 1) == Some(&b'"')
        && (at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_'))
}

fn quoted_end(source: &str, quote: usize) -> usize {
    let bytes = source.as_bytes();
    let mut at = quote + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

/// End of a raw string at `at`, if one starts there.
fn raw_string_end(source: &str, at: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes[at] != b'r'
        || (at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_'))
    {
        return None;
    }
    let mut cursor = at + 1;
    let mut hashes = 0;
    while bytes.get(cursor) == Some(&b'#') {
        hashes += 1;
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    let terminator = format!("\"{}", "#".repeat(hashes));
    source[cursor + 1..]
        .find(&terminator)
        .map(|length| cursor + 1 + length + terminator.len())
        .or(Some(bytes.len()))
}

/// End of a character literal at `at`; a lifetime such as `'a` is code.
fn character_end(bytes: &[u8], at: usize) -> Option<usize> {
    let escaped = bytes.get(at + 1) == Some(&b'\\') && bytes.get(at + 3) == Some(&b'\'');
    let plain = bytes.get(at + 2) == Some(&b'\'');
    if escaped || plain {
        bytes[at + 1..]
            .iter()
            .position(|&byte| byte == b'\'')
            .map(|offset| at + 1 + offset + 1)
    } else {
        None
    }
}

/// Why `literal` is not user copy, or `None` when it must move to the catalog.
///
/// Rules run before the exact table so the table only lists what no rule can
/// classify. Each rule is deliberately about the text, not the file:
///
/// 1. no ASCII letters at all: glyphs, separators and field masks;
/// 2. lowercase identifier/path shape: element keys, JSON fields, paths and
///    extensions (`note-row-{id}`, `~/notes`, `.excalidraw`);
/// 3. keycaps and chords (`⌘K`, `T / 8`, `Page Up`) and single characters;
/// 4. file-format examples (`YYYY-MM-DD`, `@{…}`, `[[Note]]`);
/// 5. SVG/XML markup.
fn allow_reason(literal: &str) -> Option<&'static str> {
    if !literal.bytes().any(|byte| byte.is_ascii_alphabetic()) {
        return Some("no letters: glyph, separator or field mask");
    }
    if is_identifier_shape(literal) {
        return Some("lowercase identifier/path shape: key, JSON field, path or extension");
    }
    if is_keycap(literal) {
        return Some("keycap or chord, not copy");
    }
    if is_format_example(literal) {
        return Some("file-format example, not copy");
    }
    if literal.contains('<') {
        return Some("SVG/XML markup, not copy");
    }
    ALLOWED_LITERALS
        .iter()
        .find(|(allowed, _)| *allowed == literal)
        .map(|(_, reason)| *reason)
}

fn is_identifier_shape(literal: &str) -> bool {
    literal.bytes().all(|byte| {
        byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || matches!(
                byte,
                b'_' | b'.' | b'/' | b'{' | b'}' | b':' | b'-' | b' ' | b'~'
            )
    })
}

fn is_keycap(literal: &str) -> bool {
    let mut characters = literal.chars();
    let Some(first) = characters.next() else {
        return true;
    };
    if characters.next().is_none() {
        return true;
    }
    if literal
        .chars()
        .all(|character| KEYCAP_GLYPHS.contains(character))
    {
        return true;
    }
    if "⌘⌥⌃⇧".contains(first)
        && literal
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "⌘⌥⌃⇧".contains(character))
    {
        return true;
    }
    if literal.chars().any(|character| {
        !(character.is_ascii_alphanumeric() || matches!(character, ' ' | '/' | '+'))
    }) {
        return false;
    }
    literal
        .split([' ', '/', '+'])
        .filter(|token| !token.is_empty())
        .all(|token| {
            token.chars().count() == 1
                || KEYCAP_WORDS
                    .iter()
                    .any(|word| word.eq_ignore_ascii_case(token))
        })
}

fn is_format_example(literal: &str) -> bool {
    literal.contains("[[")
        || literal.starts_with("@{")
        || literal.starts_with("@@{")
        || literal.contains("YYYY")
        || literal.contains("HH:mm")
        || literal.contains("MMMM")
        || literal.ends_with(".md")
        || literal.ends_with(".excalidraw")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Literals that survive every allow rule, i.e. what the lint would fail.
    fn flagged(source: &str) -> Vec<String> {
        scan_source(source)
            .into_iter()
            .filter(|hit| allow_reason(&hit.literal).is_none())
            .map(|hit| hit.literal)
            .collect()
    }

    #[test]
    fn a_sentence_passed_to_text_is_flagged() {
        assert_eq!(
            flagged(r#"fn view() { text("Save the note"); }"#),
            vec!["Save the note"]
        );
    }

    #[test]
    fn glyphs_keys_and_format_examples_pass() {
        for source in [
            r#"text("⌘K");"#,
            r#"text("note-row-{id}");"#,
            r#"text("YYYY-MM-DD");"#,
            r#"text("@{YYYY-MM-DD}");"#,
            r#"text("[[Note]]");"#,
            r#"".placeholder("~/notes");"#,
            r#"Button::new("−");"#,
        ] {
            assert!(flagged(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn cfg_test_modules_are_skipped() {
        let source = concat!(
            "fn view() { text(\"Listed once\"); }\n",
            "#[cfg(test)]\n",
            "mod tests {\n",
            "    #[test]\n",
            "    fn helper() { text(\"Saved the note\"); }\n",
            "}\n",
            "fn after() { text(\"Still listed\"); }\n",
        );
        assert_eq!(flagged(source), vec!["Listed once", "Still listed"]);
    }

    #[test]
    fn cfg_test_fn_attributes_are_skipped() {
        let source = concat!(
            "#[cfg(test)]\n",
            "fn helper() -> String {\n",
            "    text(\"Only in tests\").to_owned()\n",
            "}\n",
            "fn view() { text(\"Shown to the reader\"); }\n",
        );
        assert_eq!(flagged(source), vec!["Shown to the reader"]);
    }

    #[test]
    fn a_literal_after_a_line_break_is_read() {
        let source = "state.status =\n    \"Sync finished.\".to_owned();";
        assert_eq!(flagged(source), vec!["Sync finished."]);
    }

    #[test]
    fn a_tray_label_is_the_second_argument() {
        let source = concat!(
            "TrayItem::action(TRAY_QUIT, \"Quit Muninn\"),\n",
            "TrayItem::action(TRAY_SHOW, t(TextKey::TrayShow)),\n",
        );
        assert_eq!(flagged(source), vec!["Quit Muninn"]);
    }

    #[test]
    fn direct_error_and_notice_literals_are_flagged() {
        assert_eq!(
            flagged(r#"self.error = Some("Save failed");"#),
            vec!["Save failed"]
        );
        assert_eq!(
            flagged(r#"self.notice = "Link copied";"#),
            vec!["Link copied"]
        );
        assert!(flagged(r#"self.error = Some(could_not_save(e));"#).is_empty());
        assert!(flagged(r#"self.error = Some(t(TextKey::SaveFailed).to_owned());"#).is_empty());
        assert!(flagged(r#"self.error = Some(format!("Save failed: {e}"));"#).is_empty());
    }

    #[test]
    fn raw_string_literals_are_read() {
        assert_eq!(
            flagged(r##"text(r#"Save the note"#);"##),
            vec!["Save the note"]
        );
        assert!(flagged(r##"text(include_str!("copy.txt"));"##).is_empty());
    }

    #[test]
    fn comments_and_existing_strings_are_ignored() {
        let source = concat!(
            "// text(\"Save the note\")\n",
            "let sample = \"text(\\\"Save the note\\\")\";\n",
            "/* .label(\"Close\") */\n",
        );
        assert!(flagged(source).is_empty(), "{source}");
    }

    #[test]
    fn a_documented_table_entry_passes() {
        for (literal, reason) in ALLOWED_LITERALS {
            assert!(!reason.is_empty());
            assert!(allow_reason(literal).is_some(), "{literal}");
        }
    }
}
