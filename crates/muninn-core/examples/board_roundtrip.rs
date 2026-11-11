//! Check that `.excalidraw` files survive Muninn's board load/save unchanged.
//!
//! ```sh
//! cargo run -p muninn-core --example board_roundtrip -- path/to/Board.excalidraw [more…]
//! ```
//!
//! For each file: parse it, compare the re-serialised scene with the original
//! JSON (only the documented normalisations are allowed: sorted keys, integral
//! numbers as integers, absent modelled element fields filled with Excalidraw
//! defaults, absent `source`/`version` written, and absent `appState`/`files`
//! written as `{}`), then save it
//! through the atomic-write path into a scratch copy next to the original
//! (a hidden `.roundtrip-*` file, removed afterwards), reload it, and compare
//! again. The original file is only read. Exits 1 on the first mismatch.
//! Used by `scripts/ui-smoke.sh`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use muninn_core::{Board, Element};
use serde_json::Value;

fn main() -> ExitCode {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if paths.is_empty() {
        eprintln!("usage: board_roundtrip FILE.excalidraw [FILE…]");
        return ExitCode::from(2);
    }
    for path in &paths {
        if let Err(message) = check(path) {
            eprintln!("FAIL {}: {message}", path.display());
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

fn check(path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let original: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let board = Board::from_json(&text).map_err(|e| e.to_string())?;

    let mut expected = original.clone();
    for key in ["appState", "files"] {
        if expected.get(key).is_none_or(Value::is_null) {
            expected[key] = Value::Object(Default::default());
        }
    }
    let written = board.to_value();
    if let Some(diff) = first_difference(&expected, &written, "$") {
        return Err(format!("re-serialised scene differs at {diff}"));
    }

    let name = path
        .file_name()
        .map_or_else(Default::default, |n| n.to_string_lossy().into_owned());
    let scratch = path.with_file_name(format!(".roundtrip-{}-{name}", std::process::id()));
    let saved = board
        .save(&scratch)
        .map_err(|e| format!("atomic save: {e}"));
    let reloaded = saved.and_then(|()| Board::load(&scratch).map_err(|e| format!("reload: {e}")));
    let _ = std::fs::remove_file(&scratch);
    let reloaded = reloaded?;
    if reloaded != board {
        return Err("saved and reloaded board differs from the loaded one".into());
    }

    let shapes = board
        .elements
        .iter()
        .filter(|e| matches!(e, Element::Shape(_)))
        .count();
    println!(
        "ok {}: {} elements ({} modelled, {} kept raw), {} files, {} bytes written",
        path.display(),
        board.elements.len(),
        shapes,
        board.elements.len() - shapes,
        board.files.len(),
        board.to_json().len(),
    );
    Ok(())
}

/// The JSON path of the first difference, or `None` when equal.
fn first_difference(a: &Value, b: &Value, at: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().chain(y.keys()) {
                match (x.get(key), y.get(key)) {
                    (Some(l), Some(r)) => {
                        if let Some(d) = first_difference(l, r, &format!("{at}.{key}")) {
                            return Some(d);
                        }
                    }
                    (Some(_), None) => return Some(format!("{at}.{key} (dropped)")),
                    (None, _) => {
                        // Only element defaults and the scene's own
                        // `source`/`version` may be added.
                        let header = at == "$" && (key == "source" || key == "version");
                        if !header && !at.contains(".elements[") {
                            return Some(format!("{at}.{key} (added)"));
                        }
                    }
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (l, r))| first_difference(l, r, &format!("{at}[{i}]"))),
        _ if a == b => None,
        _ => Some(format!("{at}: {a} != {b}")),
    }
}
