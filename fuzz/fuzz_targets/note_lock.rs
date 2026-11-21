//! Fuzz the note-lock envelope frame parser.
//!
//! `parse_for_fuzzing` is the private structure check behind `unlock`, exposed
//! under the off-by-default `fuzzing` feature. Key derivation (Argon2id) is
//! deliberately not part of it, so inputs stay cheap.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = muninn_core::parse_for_fuzzing(&text);
});
