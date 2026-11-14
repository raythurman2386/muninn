//! Break opportunities for card markdown, so one unbroken token cannot paint
//! past the card.
//!
//! Rune's `Markdown` wraps at spaces and at the breaks its text shaper finds,
//! but it has no `wrap_anywhere` fallback (issue #66). A card whose visible
//! text is one long run (an env var, a URL) therefore keeps the run on one
//! line and draws it over the next lane. This module rewrites only the source
//! it is handed to the reader: it inserts zero-width spaces (U+200B, a break
//! opportunity that draws nothing) inside long runs.
//!
//! - A run longer than [`LONG_RUN`] can break after every character. Shorter
//!   prose is left alone, so ordinary words still wrap at spaces.
//! - Runs joined by `.`, `/`, `_`, `=` and friends break after those marks
//!   even when the whole run is short enough to stay on one line.
//! - A bare `https://…` URL is first written as an explicit Markdown link
//!   whose label is broken and whose destination is the original URL, so the
//!   link still opens the exact address. Explicit links keep their
//!   destination byte-for-byte; only the label is broken.
//! - Fenced code, inline code and link destinations are copied verbatim, so
//!   the source the reader renders keeps every link target intact.
//!
//! Nothing here touches the board: the stored card text and the editor's
//! buffer are the original bytes. This is a drawing fix only.

/// A run of characters this long, with no break opportunity, may break after
/// any of its characters. Long enough that ordinary words keep their shape,
/// short enough that no ordinary card line can overflow by a word.
const LONG_RUN: usize = 24;

/// Zero-width space: a break opportunity with no ink and no advance.
pub(crate) const BREAK: char = '\u{200B}';

/// `source` with break opportunities added to long unbroken runs.
pub(crate) fn card_markdown(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + source.len() / 8);
    let mut fence: Option<&str> = None;
    for line in source.split_inclusive('\n') {
        let (body, newline) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (line, ""),
        };
        let marker = fence_marker(body);
        match (fence, marker) {
            (None, Some(marker)) => {
                fence = Some(marker);
                out.push_str(body);
            }
            (Some(open), Some(marker)) if open == marker => {
                fence = None;
                out.push_str(body);
            }
            // Inside a fence every line is code: it is copied as written.
            (Some(_), _) => out.push_str(body),
            (None, None) => out.push_str(&inline(body)),
        }
        out.push_str(newline);
    }
    out
}

/// The fence marker a line opens or closes, if any. A fence may be indented
/// up to three spaces, as Markdown allows.
fn fence_marker(line: &str) -> Option<&'static str> {
    let indent = line.len() - line.trim_start().len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    if rest.starts_with("```") {
        Some("```")
    } else if rest.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

/// One line that is not fenced code, with links recognised first.
fn inline(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + 8);
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];
        let Some(c) = rest.chars().next() else {
            break;
        };
        match c {
            '\\' => {
                out.push('\\');
                i += 1;
                if let Some(next) = line[i..].chars().next() {
                    out.push(next);
                    i += next.len_utf8();
                }
            }
            '`' => {
                let Some(end) = inline_code_end(line, i) else {
                    let run = run_len(rest, '`');
                    out.push_str(&rest[..run]);
                    i += run;
                    continue;
                };
                out.push_str(&line[i..end]);
                i = end;
            }
            '[' => match link_at(line, i) {
                Some((label_end, end)) => {
                    out.push('[');
                    let label = &line[i + 1..label_end];
                    if label.trim().is_empty() {
                        // #72: a link with no text has an empty accessible
                        // name. Name it from its target: the note, the file,
                        // or a readable form of the URL.
                        let name = link_name(destination(line, label_end, end));
                        push_visible(&escape_label(&name), &mut out);
                    } else {
                        push_visible(label, &mut out);
                    }
                    out.push_str(&line[label_end..end]);
                    i = end;
                }
                None => {
                    out.push('[');
                    i += 1;
                }
            },
            '<' => match angle_url_at(line, i) {
                Some((inner, end)) => {
                    out.push('[');
                    push_url_label(inner, &mut out);
                    out.push_str("](<");
                    out.push_str(inner);
                    out.push_str(">)");
                    i = end;
                }
                None => {
                    out.push('<');
                    i += 1;
                }
            },
            _ => match bare_url_at(line, i) {
                Some(url) => {
                    let is_www = url
                        .get(..4)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("www."));
                    out.push('[');
                    push_url_label(url, &mut out);
                    out.push_str("](<");
                    if is_www {
                        out.push_str("http://");
                    }
                    out.push_str(url);
                    out.push_str(">)");
                    i += url.len();
                }
                None => {
                    let start = i;
                    i += c.len_utf8();
                    while i < line.len() {
                        let next = line[i..].chars().next().unwrap_or(' ');
                        if matches!(next, '\\' | '`' | '[' | '<') || bare_url_at(line, i).is_some()
                        {
                            break;
                        }
                        i += next.len_utf8();
                    }
                    push_visible(&line[start..i], &mut out);
                }
            },
        }
    }
    out
}

/// The end of the inline-code span that starts at `start` (a backtick), or
/// `None` when it has no closing run of the same length.
fn inline_code_end(line: &str, start: usize) -> Option<usize> {
    let run = run_len(&line[start..], '`');
    let rest = &line[start + run..];
    let mut i = 0;
    while i < rest.len() {
        if rest[i..].starts_with('`') {
            let close = run_len(&rest[i..], '`');
            if close == run {
                return Some(start + run + i + close);
            }
            i += close;
        } else {
            i += rest[i..].chars().next()?.len_utf8();
        }
    }
    None
}

fn run_len(text: &str, c: char) -> usize {
    text.chars().take_while(|&x| x == c).count() * c.len_utf8()
}

/// `(label_end, end)` when an inline link starts at `start` (`[`): `label_end`
/// is the `]` and `end` is just past the `)`.
fn link_at(line: &str, start: usize) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut i = start;
    let label_end = loop {
        let c = line[i..].chars().next()?;
        match c {
            '\\' => {
                i += 1 + line[i + 1..].chars().next().map_or(0, char::len_utf8);
                continue;
            }
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break i;
                }
            }
            _ => {}
        }
        i += c.len_utf8();
    };
    if !line[label_end + 1..].starts_with('(') {
        return None;
    }
    let end = destination_end(line, label_end + 1)?;
    Some((label_end, end))
}

/// The index just past the destination that opens at `open` (`(`).
fn destination_end(line: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    let mut angle = false;
    while i < line.len() {
        let c = line[i..].chars().next()?;
        match c {
            '\\' => {
                i += 1 + line[i + 1..].chars().next().map_or(0, char::len_utf8);
                continue;
            }
            '<' if depth == 1 => angle = true,
            '>' if angle => angle = false,
            '(' if !angle => depth += 1,
            ')' if !angle => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += c.len_utf8();
    }
    None
}

/// The `https://…` inside `<…>` when it is a web autolink, and the index just
/// past `>`.
fn angle_url_at(line: &str, start: usize) -> Option<(&str, usize)> {
    let close = line[start + 1..].find('>')?;
    let inner = &line[start + 1..start + 1 + close];
    is_web_url(inner).then_some((inner, start + close + 2))
}

/// A bare GFM autolink starting at `start`, bounded and trimmed as the reader
/// would find it.
fn bare_url_at(line: &str, start: usize) -> Option<&str> {
    if start > 0 {
        let before = line[..start].chars().next_back()?;
        if !(before.is_whitespace()
            || matches!(before, '(' | '[' | '{' | '\'' | '"' | '*' | '_' | '~'))
        {
            return None;
        }
    }
    let rest = &line[start..];
    let is_www = rest
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("www."));
    if !is_web_url(rest) && !is_www {
        return None;
    }
    let end = rest
        .find(|c: char| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"'))
        .unwrap_or(rest.len());
    let mut end = start + end;
    // GFM trims trailing punctuation and unmatched closers from the link.
    loop {
        let slice = &line[start..end];
        let Some(last) = slice.chars().next_back() else {
            break;
        };
        let trim = match last {
            '.' | ',' | ':' | ';' | '!' | '?' | '\'' => true,
            ')' => count(line, start, end, '(') < count(line, start, end, ')'),
            ']' => count(line, start, end, '[') < count(line, start, end, ']'),
            _ => false,
        };
        if !trim {
            break;
        }
        end -= last.len_utf8();
    }
    let url = &line[start..end];
    let host = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .or_else(|| url.strip_prefix("www."))
        .unwrap_or(url);
    let host = host
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(host)
        .trim_end_matches('.');
    (!host.is_empty()).then_some(url)
}

fn count(line: &str, start: usize, end: usize, c: char) -> usize {
    line[start..end].matches(c).count()
}

fn is_web_url(text: &str) -> bool {
    text.get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
        || text
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
}

/// Visible text: break long runs, and after the marks that already read as
/// separators. Whitespace is copied as written.
fn push_visible(text: &str, out: &mut String) {
    let mut token = String::new();
    for c in text.chars() {
        if c.is_whitespace() {
            push_token(&token, out);
            token.clear();
            out.push(c);
        } else {
            token.push(c);
        }
    }
    push_token(&token, out);
}

/// A label for a URL we are writing as an explicit link: its Markdown-significant
/// characters are escaped so the URL shows as itself, and it breaks like any
/// other long run.
fn push_url_label(url: &str, out: &mut String) {
    let chars: Vec<char> = url.chars().collect();
    for (index, &c) in chars.iter().enumerate() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>') {
            out.push('\\');
        }
        out.push(c);
        if index + 1 < chars.len() && (chars.len() > LONG_RUN || breaks_after(c)) {
            out.push(BREAK);
        }
    }
}

fn push_token(token: &str, out: &mut String) {
    if token.is_empty() {
        return;
    }
    let chars: Vec<char> = token.chars().collect();
    let long = chars.len() > LONG_RUN;
    for (index, &c) in chars.iter().enumerate() {
        out.push(c);
        if index + 1 < chars.len() && (long || breaks_after(c)) {
            out.push(BREAK);
        }
    }
}

fn breaks_after(c: char) -> bool {
    // `@`, `#` and `-` stay out: a short date, tag or mention reads as one
    // token in the page's text, and a long one breaks at every character
    // anyway. These marks only help the runs that need it.
    matches!(
        c,
        '.' | ',' | ';' | '!' | '?' | '/' | '_' | '=' | '&' | '%' | '+'
    )
}

/// The destination of the link whose `]` is `label_end` and whose `)` is `end`:
/// `<url>`, a bare `url`, or a title after whitespace.
fn destination(line: &str, label_end: usize, end: usize) -> &str {
    let inside = &line[label_end + 2..end - 1].trim_start();
    let dest = if let Some(rest) = inside.strip_prefix('<') {
        rest.split_once('>').map_or(rest, |(dest, _)| dest)
    } else {
        inside.split_whitespace().next().unwrap_or(inside)
    };
    dest.trim()
}

/// A readable name for a link that has no text of its own (#72), from its
/// target: a note or file name, or a host and path tail for a web address.
pub(crate) fn link_name(target: &str) -> String {
    for scheme in ["note:", "missing:", "attachment:"] {
        if let Some(rest) = target.strip_prefix(scheme) {
            let name = rest.split(['#', '|']).next().unwrap_or(rest);
            let name = name.rsplit('/').next().unwrap_or(name);
            if !name.is_empty() {
                return name.to_owned();
            }
        }
    }
    if let Some(rest) = target
        .strip_prefix("https://")
        .or_else(|| target.strip_prefix("http://"))
    {
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = host.trim_end_matches('.');
        let tail = path
            .split(['?', '#'])
            .next()
            .unwrap_or(path)
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("");
        return if tail.is_empty() || tail == host {
            host.to_owned()
        } else {
            format!("{host} — {tail}")
        };
    }
    if target.is_empty() {
        "Link".to_owned()
    } else {
        target.to_owned()
    }
}

/// What a link's tooltip says: where it goes, in words, for a pointer user and
/// as a description for AT where the platform exposes link descriptions.
pub(crate) fn link_description(target: &str) -> String {
    format!("Open {}", link_name(target))
}

/// `label` with the characters that would end or style a Markdown link
/// escaped, so a generated name always reads as itself.
fn escape_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len() + 4);
    for c in label.chars() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_env_var_token_can_break_inside_the_card() {
        let out = card_markdown("NEXT_PUBLIC_CONSTRUCTION_MODE=true");
        assert!(out.contains(BREAK), "{out:?}");
        assert_eq!(out.replace(BREAK, ""), "NEXT_PUBLIC_CONSTRUCTION_MODE=true");
        assert!(!out.contains(' '), "no space is written: {out:?}");
        assert_eq!(
            break_count(&out),
            "NEXT_PUBLIC_CONSTRUCTION_MODE=true".chars().count() - 1,
            "every character but the last"
        );
    }

    #[test]
    fn a_long_url_stays_one_link_to_the_exact_address() {
        let url = "https://www.sistersbythemoon.com/api/webhooks/stripe/events";
        let out = card_markdown(url);
        assert_eq!(
            out.replace(BREAK, ""),
            format!("[{url}](<{url}>)"),
            "the label shows the URL and the destination is the URL"
        );
        assert!(out.contains("](<https://www.sistersbythemoon.com"), "{out}");
        assert!(out.contains(BREAK), "the label can wrap");
    }

    #[test]
    fn ordinary_prose_is_left_alone() {
        let prose = "Ship the release notes to the team on Friday.";
        assert_eq!(card_markdown(prose), prose);
    }

    #[test]
    fn an_explicit_link_keeps_its_destination_and_breaks_its_label() {
        let link = "[NEXT_PUBLIC_CONSTRUCTION_MODE=true](<note:Config>)";
        let out = card_markdown(link);
        assert!(out.contains("](<note:Config>)"), "{out}");
        assert!(
            out.contains(&format!("[N{BREAK}E{BREAK}X{BREAK}T{BREAK}_")),
            "{out:?}"
        );
        assert_eq!(
            out.replace(BREAK, ""),
            link,
            "breaks are the only change: no space is written"
        );
    }

    #[test]
    fn a_short_link_label_is_left_alone() {
        let link = "[Sisters by the Moon](<note:Sisters by the Moon>)";
        assert_eq!(
            card_markdown(link),
            link,
            "short labels keep their shape, and the destination is untouched"
        );
    }

    #[test]
    fn inline_code_and_fences_are_copied_verbatim() {
        let source = "before `NEXT_PUBLIC_CONSTRUCTION_MODE=true` after";
        assert_eq!(card_markdown(source), source);
        let fenced = "```\nhttps://example.test/a/very/long/path/that/does/not/fit\n```\n";
        assert_eq!(card_markdown(fenced), fenced);
    }

    #[test]
    fn multibyte_text_never_splits_a_character() {
        let text = "漢字 😀 long line ".repeat(40);
        let out = card_markdown(&text);
        assert_eq!(out.replace(BREAK, ""), text);
    }

    #[test]
    fn a_long_bare_url_inside_prose_becomes_a_link() {
        let out = card_markdown("see https://example.test/one/two/three/four/five/six now");
        assert!(out.starts_with("see ["), "{out}");
        assert!(out.ends_with(" now"), "{out}");
        assert!(
            out.contains("](<https://example.test/one/two/three/four/five/six>)"),
            "{out}"
        );
    }

    #[test]
    fn an_empty_link_label_is_named_from_its_target() {
        assert_eq!(
            card_markdown("[the docs](https://example.test/docs)"),
            "[the docs](https://example.test/docs)",
            "a link with text keeps its own name"
        );
        assert_eq!(
            card_markdown("[Agenda](<note:Agenda>)"),
            "[Agenda](<note:Agenda>)"
        );
        assert_eq!(
            card_markdown("[](<note:Agenda>)"),
            "[Agenda](<note:Agenda>)",
            "an empty wikilink label takes the note's name"
        );
        assert_eq!(
            card_markdown("[](https://example.test/guide)").replace(BREAK, ""),
            "[example.test — guide](https://example.test/guide)",
            "an empty web link takes a readable target description"
        );
    }

    #[test]
    fn link_names_and_descriptions_describe_the_target() {
        assert_eq!(link_name("note:Notes/Agenda#Today"), "Agenda");
        assert_eq!(link_name("missing:Notes/Plan"), "Plan");
        assert_eq!(link_name("attachment:files/Plan.pdf"), "Plan.pdf");
        assert_eq!(link_name("https://example.test"), "example.test");
        assert_eq!(
            link_name("https://www.example.test/one/two/guide?q=1#top"),
            "www.example.test — guide"
        );
        assert_eq!(link_name(""), "Link");
        assert_eq!(
            link_description("https://example.test/docs"),
            "Open example.test — docs"
        );
    }

    fn break_count(text: &str) -> usize {
        text.chars().filter(|&c| c == BREAK).count()
    }
}
