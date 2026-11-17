//! Document-to-Markdown conversion for Muninn.
//!
//! [`convert`] turns one local document into a Markdown body plus the embedded
//! images the body references. It is pure library code: it never writes to the
//! vault, never starts a worker, and never touches the network (the
//! `anydoc` parser is local-only). The UI slice decides where the note and its
//! assets land and maps [`ImportError`] onto user-facing copy.
//!
//! Format detection follows `anydoc`: the bytes decide, and the file extension
//! is the fallback for signature-less formats such as CSV.
//!
//! # Limits
//!
//! A source larger than [`MAX_INPUT_BYTES`] is rejected from its metadata
//! before any read, and converted Markdown larger than [`MAX_BODY_BYTES`] is
//! rejected so a pathological sheet cannot produce a note the editor cannot
//! open. Both limits are hard errors with their own [`ImportError`] variants.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use anydoc::{ConvertError, Format};

/// Largest source document accepted, checked from metadata before reading.
pub const MAX_INPUT_BYTES: u64 = 50 * 1024 * 1024;

/// Largest converted Markdown body accepted.
pub const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

/// One converted document, ready to be written beside its note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Converted {
    /// The document body as GitHub-Flavored Markdown.
    pub markdown: String,
    /// Embedded images, named for the vault and in document order.
    pub assets: Vec<AssetFile>,
    /// The source file's name, for provenance and the source-note link.
    pub source_name: String,
    /// Assets that were not images with a supported extension and were
    /// therefore left out of [`Converted::assets`].
    pub skipped_assets: usize,
}

/// One embedded image extracted from the source document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetFile {
    /// Vault-relative file name, `<stem>-image-<n>.<ext>`.
    pub file_name: String,
    /// The image bytes exactly as stored in the source.
    pub bytes: Vec<u8>,
}

/// Why a document could not be converted.
///
/// The variants are stable and carry no user-facing copy: the UI maps each one
/// to a `muninn-i18n` message. Counts that affect the message (`NeedsOcr`)
/// travel with the variant.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImportError {
    /// The content and extension name no format `anydoc` can convert.
    Unsupported,
    /// Some PDF pages are scanned or image-only and need OCR. `pages` holds
    /// the 1-indexed page numbers; `page_count` is the document's page count.
    NeedsOcr {
        /// 1-indexed pages that need OCR.
        pages: Vec<u32>,
        /// Pages in the document.
        page_count: u32,
    },
    /// The document is encrypted or password-protected.
    Encrypted,
    /// The document is structurally unusable. `part` names the package part
    /// or stream when the parser could identify it.
    Malformed {
        /// Package part or stream the failure was found in, when known.
        part: Option<String>,
    },
    /// A fixed parser safety limit (archive size, nesting, node count, or
    /// retained asset bytes) was exceeded.
    ResourceLimit,
    /// A part the document needs for meaningful output is missing.
    MissingPart {
        /// The part or stream that was absent.
        part: String,
    },
    /// The source could not be read.
    Io,
    /// The source exceeds [`MAX_INPUT_BYTES`].
    TooLarge {
        /// Size reported by the filesystem, or bytes read when it grew.
        size: u64,
        /// The cap that was exceeded.
        limit: u64,
    },
    /// The converted body exceeds [`MAX_BODY_BYTES`].
    BodyTooLarge {
        /// Size of the converted Markdown.
        size: usize,
        /// The cap that was exceeded.
        limit: usize,
    },
    /// A conversion failure this crate does not model yet. The string is a
    /// diagnostic from the parser, not user-facing copy.
    Other(String),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Stable machine names, not user copy: the UI owns the sentences.
        let code = match self {
            Self::Unsupported => "unsupported",
            Self::NeedsOcr { .. } => "needs-ocr",
            Self::Encrypted => "encrypted",
            Self::Malformed { .. } => "malformed",
            Self::ResourceLimit => "resource-limit",
            Self::MissingPart { .. } => "missing-part",
            Self::Io => "io",
            Self::TooLarge { .. } => "too-large",
            Self::BodyTooLarge { .. } => "body-too-large",
            Self::Other(_) => "other",
        };
        f.write_str(code)
    }
}

impl std::error::Error for ImportError {}

impl From<ConvertError> for ImportError {
    fn from(error: ConvertError) -> Self {
        match error {
            ConvertError::Unsupported(_) => Self::Unsupported,
            ConvertError::NeedsOcr { pages, page_count } => Self::NeedsOcr { pages, page_count },
            ConvertError::Encrypted => Self::Encrypted,
            ConvertError::Malformed { part, .. } => Self::Malformed { part },
            ConvertError::ResourceLimit { .. } => Self::ResourceLimit,
            ConvertError::MissingPart { part } => Self::MissingPart { part },
            ConvertError::Io(_) => Self::Io,
            other => Self::Other(other.to_string()),
        }
    }
}

/// Convert one local document into Markdown and its embedded images.
///
/// The file is read once, after a metadata check against [`MAX_INPUT_BYTES`],
/// so a directory, an unreadable file or an oversized document fails without
/// consuming memory. The body is rendered by `anydoc` (`to_markdown` logic on
/// the bytes already read: content detection first, extension for signature-
/// less CSV). For every format except PDF and CSV, `anydoc` parses the
/// document a second time to expose its embedded assets; PDFs have no document
/// model and CSV has no payloads, so neither yields assets. That second parse
/// receives the same resolved format.
///
/// Asset names use the source stem and a one-based counter:
/// `report-image-1.png`, `report-image-2.jpg`. Assets that are not images with
/// a supported extension are counted in [`Converted::skipped_assets`] rather
/// than returned.
///
/// # Errors
///
/// Returns [`ImportError::TooLarge`] when the file is larger than
/// [`MAX_INPUT_BYTES`], [`ImportError::BodyTooLarge`] when the Markdown is
/// larger than [`MAX_BODY_BYTES`], and the mapped parser error otherwise.
pub fn convert(path: &Path) -> Result<Converted, ImportError> {
    let metadata = fs::metadata(path).map_err(|_| ImportError::Io)?;
    if !metadata.is_file() {
        return Err(ImportError::Io);
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(ImportError::TooLarge {
            size: metadata.len(),
            limit: MAX_INPUT_BYTES,
        });
    }

    let bytes = read_capped(path)?;
    let format = Format::from_bytes(&bytes)
        .or_else(|| Format::from_path(path))
        .ok_or(ImportError::Unsupported)?;

    let markdown = anydoc::to_markdown_bytes(&bytes, format)?;
    if markdown.len() > MAX_BODY_BYTES {
        return Err(ImportError::BodyTooLarge {
            size: markdown.len(),
            limit: MAX_BODY_BYTES,
        });
    }

    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("document");
    let (assets, skipped_assets) = collect_assets(&bytes, format, stem)?;

    Ok(Converted {
        markdown,
        assets,
        source_name: source_name(path),
        skipped_assets,
    })
}

/// Read at most [`MAX_INPUT_BYTES`] plus one byte, so a file that grew after
/// the metadata check still fails the cap instead of filling memory.
fn read_capped(path: &Path) -> Result<Vec<u8>, ImportError> {
    let file = File::open(path).map_err(|_| ImportError::Io)?;
    let mut reader = file.take(MAX_INPUT_BYTES + 1);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|_| ImportError::Io)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(ImportError::TooLarge {
            size: bytes.len() as u64,
            limit: MAX_INPUT_BYTES,
        });
    }
    Ok(bytes)
}

/// Extract embedded images. PDFs bypass the document model in `anydoc`, and
/// CSV has no embedded payloads, so neither needs a second parse.
fn collect_assets(
    bytes: &[u8],
    format: Format,
    stem: &str,
) -> Result<(Vec<AssetFile>, usize), ImportError> {
    if matches!(format, Format::Pdf | Format::Csv) {
        return Ok((Vec::new(), 0));
    }

    let document = anydoc::to_document(bytes, format)?;
    let mut assets = Vec::new();
    let mut skipped = 0;
    for asset in document.assets {
        let Some(extension) = image_extension(&asset.media_type) else {
            skipped += 1;
            continue;
        };
        let file_name = format!("{stem}-image-{}.{extension}", assets.len() + 1);
        assets.push(AssetFile {
            file_name,
            bytes: asset.bytes,
        });
    }
    Ok((assets, skipped))
}

/// Map an image media type onto the vault extension, or `None` for payloads
/// the note renderer cannot display (objects, fonts, EMF/WMF).
fn image_extension(media_type: &str) -> Option<&'static str> {
    let media_type = media_type.trim().to_ascii_lowercase();
    Some(match media_type.as_str() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => return None,
    })
}

/// The source file's name for provenance; lossy for non-UTF-8 names.
fn source_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_types_map_to_extensions() {
        assert_eq!(image_extension("image/png"), Some("png"));
        assert_eq!(image_extension("IMAGE/JPEG"), Some("jpg"));
        assert_eq!(image_extension("image/svg+xml"), Some("svg"));
        assert_eq!(image_extension("application/octet-stream"), None);
        assert_eq!(image_extension("image/emf"), None);
        assert_eq!(image_extension("image/wmf"), None);
    }

    #[test]
    fn every_convert_error_maps_to_a_stable_variant() {
        let cases = [
            (
                ConvertError::Unsupported("x".into()),
                ImportError::Unsupported,
            ),
            (ConvertError::Encrypted, ImportError::Encrypted),
            (
                ConvertError::Malformed {
                    part: Some("word/document.xml".into()),
                    detail: "bad".into(),
                },
                ImportError::Malformed {
                    part: Some("word/document.xml".into()),
                },
            ),
            (
                ConvertError::ResourceLimit {
                    limit: "max_nodes",
                    detail: "too many".into(),
                },
                ImportError::ResourceLimit,
            ),
            (
                ConvertError::MissingPart {
                    part: "word/document.xml".into(),
                },
                ImportError::MissingPart {
                    part: "word/document.xml".into(),
                },
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(ImportError::from(source), expected);
        }

        let ocr = ImportError::from(ConvertError::NeedsOcr {
            pages: vec![2, 3],
            page_count: 9,
        });
        assert_eq!(
            ocr,
            ImportError::NeedsOcr {
                pages: vec![2, 3],
                page_count: 9,
            }
        );
        assert_eq!(
            ImportError::from(ConvertError::Io(std::io::Error::other("x"))),
            ImportError::Io
        );
    }

    #[test]
    fn error_display_is_a_machine_name() {
        assert_eq!(ImportError::Unsupported.to_string(), "unsupported");
        assert_eq!(
            ImportError::Malformed { part: None }.to_string(),
            "malformed"
        );
        assert_eq!(
            ImportError::TooLarge { size: 10, limit: 5 }.to_string(),
            "too-large"
        );
    }
}
