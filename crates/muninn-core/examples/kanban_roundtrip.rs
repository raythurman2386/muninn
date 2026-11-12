//! Check a kanban note, and optionally apply a line-based edit script.
//!
//! ```sh
//! cargo run -p muninn-core --example kanban_roundtrip -- FILE.md
//! cargo run -p muninn-core --example kanban_roundtrip -- FILE.md --edit SCRIPT
//! ```
//!
//! With no script the example parses `FILE` and checks that the unedited
//! model writes the original bytes. It also saves through the atomic writer
//! to a hidden scratch file next to `FILE` (removed afterwards) and checks
//! the reloaded bytes. The original file is only read.
//!
//! `--edit SCRIPT` does that check first, then applies the script and prints
//! a unified diff of the result against the original. The script is one
//! command per line (`#` starts a comment). Indexes are 0-based.
//!
//! ```text
//! add-card LANE INDEX TEXT
//! edit-card LANE INDEX TEXT
//! delete-card LANE INDEX
//! move-card FROM_LANE FROM_INDEX TO_LANE TO_INDEX
//! add-lane INDEX LIMIT DONE TITLE
//! rename-lane INDEX TITLE
//! delete-lane INDEX
//! limit LANE N|clear
//! toggle LANE INDEX
//! set KEY JSON
//! unset KEY
//! ```
//!
//! `TEXT` is the rest of the line; `\n` is a newline and `\\` is a backslash.
//! `move-card`'s `TO_INDEX` is the card's index in the destination after the
//! move (`cards.len()` on another lane appends; on the same lane the last
//! index is `cards.len() - 1`). `add-lane`'s `LIMIT` is `none` or a number
//! (`0` writes no suffix) and `DONE` is `done` or `open`. `set` parses `JSON`
//! with the footer splice, so a new key is appended and an existing value is
//! replaced in place.
//!
//! Exits 1 when the round-trip, the script, or the scratch reload fails.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use muninn_core::Kanban;
use serde_json::Value;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut script: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--edit" {
            let Some(path) = args.get(index + 1) else {
                eprintln!("usage: kanban_roundtrip FILE [--edit SCRIPT]");
                return ExitCode::from(2);
            };
            script = Some(PathBuf::from(path));
            index += 2;
            continue;
        }
        files.push(PathBuf::from(&args[index]));
        index += 1;
    }
    if files.is_empty() {
        eprintln!("usage: kanban_roundtrip FILE [--edit SCRIPT]");
        return ExitCode::from(2);
    }
    if script.is_some() && files.len() != 1 {
        eprintln!("--edit takes exactly one file");
        return ExitCode::from(2);
    }
    for path in &files {
        if let Err(message) = check(path, script.as_deref()) {
            eprintln!("FAIL {}: {message}", path.display());
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

fn check(path: &Path, script: Option<&Path>) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let text = String::from_utf8(bytes).map_err(|error| error.to_string())?;
    let kanban = Kanban::parse(&text).map_err(|error| error.to_string())?;
    if kanban.source() != text {
        return Err("unedited output is not byte-identical".into());
    }
    scratch_reload(path, &kanban)?;
    println!(
        "ok {}: {} lanes, {} cards, archive {}, footer {}, {} bytes",
        path.display(),
        kanban.lanes().len(),
        kanban
            .lanes()
            .iter()
            .map(|lane| lane.cards().len())
            .sum::<usize>(),
        kanban
            .archive()
            .map(|archive| archive.word())
            .unwrap_or("-"),
        if kanban.footer_json().is_some() {
            "yes"
        } else {
            "no"
        },
        kanban.source().len(),
    );
    let Some(script) = script else {
        return Ok(());
    };
    let commands = std::fs::read_to_string(script).map_err(|error| error.to_string())?;
    let edited = apply(&kanban, &commands)?;
    if edited.source() != edited_round_trip(&edited)? {
        return Err("edited output does not round-trip".into());
    }
    scratch_reload(path, &edited)?;
    print!("{}", unified("kanban", &text, edited.source()));
    Ok(())
}

fn edited_round_trip(kanban: &Kanban) -> Result<String, String> {
    let again = Kanban::parse(kanban.source()).map_err(|error| error.to_string())?;
    Ok(again.source().to_owned())
}

fn scratch_reload(path: &Path, kanban: &Kanban) -> Result<(), String> {
    let name = path
        .file_name()
        .map_or_else(Default::default, |name| name.to_string_lossy().into_owned());
    let scratch = path.with_file_name(format!(".roundtrip-{}-{name}", std::process::id()));
    let saved = kanban.save(&scratch);
    let reloaded = saved
        .and_then(|()| std::fs::read(&scratch))
        .map_err(|error| format!("atomic save: {error}"));
    let _ = std::fs::remove_file(&scratch);
    let bytes = reloaded?;
    if bytes != kanban.source().as_bytes() {
        return Err("scratch reload differs from the model".into());
    }
    Ok(())
}

fn apply(kanban: &Kanban, script: &str) -> Result<Kanban, String> {
    let mut kanban = kanban.clone();
    for (number, raw) in script.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        apply_line(&mut kanban, line).map_err(|error| format!("line {}: {error}", number + 1))?;
    }
    Ok(kanban)
}

fn apply_line(kanban: &mut Kanban, line: &str) -> Result<(), String> {
    let (command, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let rest = rest.trim_start();
    match command {
        "add-card" => {
            let (lane, index, text) = two_indexes_then_text(rest)?;
            kanban
                .add_card(lane, index, &unescape(&text))
                .map_err(|error| error.to_string())
        }
        "edit-card" => {
            let (lane, index, text) = two_indexes_then_text(rest)?;
            kanban
                .edit_card(lane, index, &unescape(&text))
                .map_err(|error| error.to_string())
        }
        "delete-card" => {
            let (lane, index) = two_indexes(rest)?;
            kanban
                .delete_card(lane, index)
                .map_err(|error| error.to_string())
        }
        "move-card" => {
            let mut parts = rest.split_whitespace();
            let from_lane = number(parts.next())?;
            let from_index = number(parts.next())?;
            let to_lane = number(parts.next())?;
            let to_index = number(parts.next())?;
            if parts.next().is_some() {
                return Err("move-card takes four indexes".into());
            }
            kanban
                .move_card(from_lane, from_index, to_lane, to_index)
                .map_err(|error| error.to_string())
        }
        "add-lane" => {
            let (index, rest) = split_word(rest)?;
            let (limit_word, rest) = split_word(rest)?;
            let (done_word, title) = split_word(rest)?;
            if title.is_empty() {
                return Err("add-lane needs a title".into());
            }
            let limit = match limit_word {
                "none" => None,
                other => Some(
                    other
                        .parse::<u32>()
                        .map_err(|_| "limit must be none or a number")?,
                ),
            };
            let done = match done_word {
                "done" => true,
                "open" => false,
                _ => return Err("done flag must be done or open".into()),
            };
            kanban
                .add_lane(number(Some(index))?, title, limit, done)
                .map_err(|error| error.to_string())
        }
        "rename-lane" => {
            let (index, title) = one_index_then_text(rest)?;
            kanban
                .rename_lane(index, &title)
                .map_err(|error| error.to_string())
        }
        "delete-lane" => {
            let index = number(Some(rest.trim()))?;
            kanban.delete_lane(index).map_err(|error| error.to_string())
        }
        "limit" => {
            let (index, value) = one_index_then_text(rest)?;
            let limit = if value == "clear" {
                None
            } else {
                Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| "limit must be a number or clear")?,
                )
            };
            kanban
                .set_lane_limit(index, limit)
                .map_err(|error| error.to_string())
        }
        "toggle" => {
            let (lane, index) = two_indexes(rest)?;
            kanban
                .toggle_done(lane, index)
                .map_err(|error| error.to_string())
        }
        "set" => {
            let (key, json) = split_word(rest)?;
            let value: Value = serde_json::from_str(json).map_err(|error| error.to_string())?;
            kanban
                .set_setting(key, &value)
                .map_err(|error| error.to_string())
        }
        "unset" => kanban
            .remove_setting(rest.trim())
            .map_err(|error| error.to_string()),
        other => Err(format!("unknown command {other}")),
    }
}

fn two_indexes(rest: &str) -> Result<(usize, usize), String> {
    let mut parts = rest.split_whitespace();
    let lane = number(parts.next())?;
    let index = number(parts.next())?;
    if parts.next().is_some() {
        return Err("expected two indexes".into());
    }
    Ok((lane, index))
}

fn two_indexes_then_text(rest: &str) -> Result<(usize, usize, String), String> {
    let mut parts = rest.splitn(3, char::is_whitespace);
    let lane = number(parts.next())?;
    let index = number(parts.next())?;
    let text = parts.next().unwrap_or("").to_owned();
    Ok((lane, index, text))
}

fn one_index_then_text(rest: &str) -> Result<(usize, String), String> {
    let (index, text) = split_word(rest)?;
    Ok((number(Some(index))?, text.to_owned()))
}

fn split_word(rest: &str) -> Result<(&str, &str), String> {
    let rest = rest.trim_start();
    if rest.is_empty() {
        return Err("missing argument".into());
    }
    Ok(rest
        .split_once(char::is_whitespace)
        .map(|(word, tail)| (word, tail.trim_start()))
        .unwrap_or((rest, "")))
}

fn number(word: Option<&str>) -> Result<usize, String> {
    word.unwrap_or("")
        .parse()
        .map_err(|_| "expected an index".into())
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// A small unified diff. Lines keep their `\n`; a missing final newline is a
/// line of its own so it still shows up.
fn unified(label: &str, before: &str, after: &str) -> String {
    let left = split_keep(before);
    let right = split_keep(after);
    let mut out = format!("--- {label}\n+++ {label}\n");
    for hunk in hunks(&left, &right) {
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.left_start,
            hunk.left.len(),
            hunk.right_start,
            hunk.right.len()
        ));
        for line in &hunk.left {
            out.push('-');
            out.push_str(line);
        }
        for line in &hunk.right {
            out.push('+');
            out.push_str(line);
        }
    }
    out
}

struct Hunk<'a> {
    left_start: usize,
    right_start: usize,
    left: Vec<&'a str>,
    right: Vec<&'a str>,
}

fn hunks<'a>(left: &'a [&'a str], right: &'a [&'a str]) -> Vec<Hunk<'a>> {
    let matches = lcs(left, right);
    let mut hunks = Vec::new();
    let mut li = 0;
    let mut ri = 0;
    for (lm, rm) in matches {
        if li < lm || ri < rm {
            hunks.push(Hunk {
                left_start: li + 1,
                right_start: ri + 1,
                left: left[li..lm].to_vec(),
                right: right[ri..rm].to_vec(),
            });
        }
        li = lm + 1;
        ri = rm + 1;
    }
    if li < left.len() || ri < right.len() {
        hunks.push(Hunk {
            left_start: li + 1,
            right_start: ri + 1,
            left: left[li..].to_vec(),
            right: right[ri..].to_vec(),
        });
    }
    hunks
}

fn lcs(left: &[&str], right: &[&str]) -> Vec<(usize, usize)> {
    let rows = left.len() + 1;
    let cols = right.len() + 1;
    let mut score = vec![0u32; rows * cols];
    let at = |row: usize, col: usize| row * cols + col;
    for row in (0..left.len()).rev() {
        for col in (0..right.len()).rev() {
            score[at(row, col)] = if left[row] == right[col] {
                score[at(row + 1, col + 1)] + 1
            } else {
                score[at(row + 1, col)].max(score[at(row, col + 1)])
            };
        }
    }
    let mut out = Vec::new();
    let mut row = 0;
    let mut col = 0;
    while row < left.len() && col < right.len() {
        if left[row] == right[col] {
            out.push((row, col));
            row += 1;
            col += 1;
        } else if score[at(row + 1, col)] >= score[at(row, col + 1)] {
            row += 1;
        } else {
            col += 1;
        }
    }
    out
}

fn split_keep(source: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let bytes = source.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            lines.push(&source[start..=index]);
            start = index + 1;
        }
    }
    if start < source.len() {
        lines.push(&source[start..]);
    }
    lines
}
