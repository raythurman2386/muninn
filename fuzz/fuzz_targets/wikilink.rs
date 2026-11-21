//! Fuzz the Markdown/wikilink scanners on arbitrary bytes.
//!
//! The product path reads UTF-8 note text, so the lossy conversion is the
//! realistic path; it also keeps arbitrary fuzz bytes from failing on the
//! harness rather than in the parser.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
    let _ = muninn_core::parse_links(&source);
    let _ = muninn_core::headings(&source);
    let _ = muninn_core::block_anchors(&source);
    // Caret positions come from the buffer length, so the fuzzer can explore
    // mid-line, mid-link and split-UTF-8 points without an extra input word.
    let _ = muninn_core::unclosed_link(&source, data.len());
    let _ = muninn_core::unclosed_link(&source, data.len() / 2);
});
