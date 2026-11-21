//! Fuzz the two JSON documents sync reads from a provider or another device:
//! the local `journal.json` and the Drive commit manifest.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // The journal is read with `read_to_string`; only valid UTF-8 reaches the
    // parser, so mirror that gate instead of folding bytes.
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = muninn_sync::parse_journal_for_fuzzing(text);
    }
    // Commits are bytes from the provider and go through `serde_json` directly.
    let _ = muninn_sync::parse_commit_for_fuzzing(data);
});
