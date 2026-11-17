//! Every committed fixture converts, and the limits fail without reading.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_import::{ImportError, MAX_BODY_BYTES, MAX_INPUT_BYTES, convert};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Unique scratch path; the test removes it when done.
struct Scratch(PathBuf);

impl Scratch {
    fn new(extension: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!(
            "muninn-import-{}-{}.{extension}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        Self(std::env::temp_dir().join(name))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn docx_converts_its_heading_and_paragraph() {
    let converted = convert(&fixture("sample.docx")).expect("docx converts");
    assert!(
        converted.markdown.contains("# Quarterly Report"),
        "missing heading in:\n{}",
        converted.markdown
    );
    assert!(converted.markdown.contains("Revenue grew in every region."));
    assert_eq!(converted.source_name, "sample.docx");
    assert!(converted.assets.is_empty());
    assert_eq!(converted.skipped_assets, 0);
}

#[test]
fn xlsx_converts_its_table_cells() {
    let converted = convert(&fixture("sample.xlsx")).expect("xlsx converts");
    assert!(
        converted.markdown.contains("Name"),
        "{}",
        converted.markdown
    );
    assert!(
        converted.markdown.contains("Value"),
        "{}",
        converted.markdown
    );
    assert!(
        converted.markdown.contains("North"),
        "{}",
        converted.markdown
    );
    assert!(converted.markdown.contains("42"), "{}", converted.markdown);
}

#[test]
fn odt_converts_its_heading_and_paragraph() {
    let converted = convert(&fixture("sample.odt")).expect("odt converts");
    assert!(
        converted.markdown.contains("Quarterly Report"),
        "{}",
        converted.markdown
    );
    assert!(
        converted.markdown.contains("Revenue grew in every region."),
        "{}",
        converted.markdown
    );
}

#[test]
fn pptx_converts_its_title() {
    let converted = convert(&fixture("sample.pptx")).expect("pptx converts");
    assert!(
        converted.markdown.contains("# Slide title"),
        "{}",
        converted.markdown
    );
}

#[test]
fn csv_converts_with_its_explicit_extension() {
    let converted = convert(&fixture("sample.csv")).expect("csv converts by extension");
    assert!(
        converted.markdown.contains("Name"),
        "{}",
        converted.markdown
    );
    assert!(
        converted.markdown.contains("North"),
        "{}",
        converted.markdown
    );
    assert!(converted.markdown.contains("42"), "{}", converted.markdown);
}

#[test]
fn rtf_converts_its_text() {
    let converted = convert(&fixture("sample.rtf")).expect("rtf converts");
    assert!(
        converted.markdown.contains("Quarterly report"),
        "{}",
        converted.markdown
    );
}

#[test]
fn epub_converts_its_title_and_chapter() {
    let converted = convert(&fixture("sample.epub")).expect("epub converts");
    assert!(
        converted.markdown.contains("Fixture Book"),
        "{}",
        converted.markdown
    );
    assert!(
        converted.markdown.contains("Chapter One"),
        "{}",
        converted.markdown
    );
}

#[test]
fn text_pdf_converts_its_text() {
    let converted = convert(&fixture("sample.pdf")).expect("text pdf converts");
    assert!(
        converted.markdown.contains("Muninn import fixture"),
        "{}",
        converted.markdown
    );
    // PDFs bypass anydoc's document model, so no assets can be extracted.
    assert!(converted.assets.is_empty());
}

#[test]
fn docx_embedded_png_becomes_one_named_asset() {
    let converted = convert(&fixture("embedded.docx")).expect("docx with image converts");
    assert_eq!(converted.assets.len(), 1);
    let asset = &converted.assets[0];
    assert_eq!(asset.file_name, "embedded-image-1.png");
    assert_eq!(
        asset.bytes,
        fs::read(fixture("pixel.png")).expect("pixel fixture")
    );
    assert_eq!(converted.skipped_assets, 0);
}

#[test]
fn random_bytes_are_unsupported() {
    let scratch = Scratch::new("bin");
    let bytes: Vec<u8> = (0u16..=255).map(|byte| byte as u8).collect();
    fs::write(scratch.path(), bytes).expect("write scratch");
    assert_eq!(convert(scratch.path()), Err(ImportError::Unsupported));
}

#[test]
fn a_51_mib_file_is_too_large_before_any_read() {
    let scratch = Scratch::new("docx");
    let file = File::create(scratch.path()).expect("create sparse file");
    let size = 51 * 1024 * 1024;
    file.set_len(size).expect("size sparse file");
    drop(file);

    assert_eq!(
        convert(scratch.path()),
        Err(ImportError::TooLarge {
            size,
            limit: MAX_INPUT_BYTES,
        })
    );
}

#[test]
fn an_encrypted_docx_is_reported_as_encrypted() {
    assert_eq!(
        convert(&fixture("encrypted.docx")),
        Err(ImportError::Encrypted)
    );
}

#[test]
fn an_over_long_body_is_rejected_after_conversion() {
    let scratch = Scratch::new("csv");
    // One unterminated cell larger than the body cap; the source itself stays
    // below the pre-read cap.
    let mut csv = String::with_capacity(MAX_BODY_BYTES + 2);
    csv.push_str("value\n");
    csv.push_str(&"a".repeat(MAX_BODY_BYTES + 1024));
    fs::write(scratch.path(), csv).expect("write large csv");

    match convert(scratch.path()) {
        Err(ImportError::BodyTooLarge { size, limit }) => {
            assert!(size > MAX_BODY_BYTES);
            assert_eq!(limit, MAX_BODY_BYTES);
        }
        other => panic!("expected BodyTooLarge, got {other:?}"),
    }
}

#[test]
fn a_missing_file_is_io() {
    let scratch = Scratch::new("docx");
    assert_eq!(convert(scratch.path()), Err(ImportError::Io));
}
