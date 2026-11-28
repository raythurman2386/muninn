//! A small, dependency-free ZIP codec for the browser vault's export (#240
//! slice 6e).
//!
//! The archive is deliberately **stored** (method 0, no compression): the vault
//! is mostly Markdown and Excalidraw JSON, a browser has no synchronous
//! compressor, and a stored archive is the part of the format a reader can
//! check byte for byte. What ships is the minimum a standard desktop tool
//! needs to open the file and the minimum this app needs to read its own
//! export back:
//!
//! - one local file header and its data per entry,
//! - one central directory header per entry,
//! - the end-of-central-directory record,
//! - CRC-32 (IEEE) over every file's bytes, verified on read.
//!
//! Names are UTF-8 and every entry sets the zip spec's bit 11 (the UTF-8 name
//! flag). Directories are entries whose name ends in `/`, so empty folders
//! survive the round trip. Zip64 and compression methods other than stored are
//! refused with a typed error instead of being written or half-read: a vault
//! is small enough for the 32-bit fields (the writer checks), and the export
//! this app produces is stored.

/// The local file header signature (`PK\x03\x04`).
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
/// The central directory header signature (`PK\x01\x02`).
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
/// The end-of-central-directory signature (`PK\x05\x06`).
const END_SIGNATURE: u32 = 0x0605_4b50;

/// Version 2.0: the oldest version that covers a stored entry.
const VERSION: u16 = 20;
/// The zip spec's "file names are UTF-8" general-purpose flag.
const FLAG_UTF8: u16 = 0x0800;

/// The DOS epoch, 1980-01-01 00:00:00. Every entry carries it so the archive
/// is deterministic; a vault's own timestamps live in its files' content, not
/// in the zip metadata.
const DOS_DATE: u16 = (1 << 5) | 1;

/// The most entries a non-zip64 archive can name (its count field is a `u16`).
pub const MAX_ENTRIES: usize = 65_535;

/// One archive member: a file's bytes, or an empty directory marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZipEntry {
    /// `/`-separated path relative to the archive root. A directory's name
    /// ends in `/` after [`ZipEntry::dir`].
    pub name: String,
    /// The file's bytes. Empty for a directory.
    pub bytes: Vec<u8>,
    pub is_dir: bool,
}

impl ZipEntry {
    /// A file entry.
    #[must_use]
    pub fn file(name: impl Into<String>, bytes: impl Into<Vec<u8>>) -> ZipEntry {
        ZipEntry {
            name: name.into(),
            bytes: bytes.into(),
            is_dir: false,
        }
    }

    /// A directory entry. The trailing `/` is part of the member's name in a
    /// zip archive, so it is stored here too and a round trip compares equal.
    #[must_use]
    pub fn dir(name: impl Into<String>) -> ZipEntry {
        let mut name = name.into();
        if !name.ends_with('/') {
            name.push('/');
        }
        ZipEntry {
            name,
            bytes: Vec::new(),
            is_dir: true,
        }
    }
}

/// Why an archive could not be written or read.
///
/// These messages are developer-facing (and logged); user-visible copy is a
/// `muninn-i18n` key at the call site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZipError {
    /// More members than a non-zip64 archive can name.
    TooManyEntries,
    /// A member's data or name does not fit its 32/16-bit header field.
    EntryTooLarge(String),
    /// An entry name that is empty or escapes the archive root.
    UnsafeName(String),
    /// A valid archive this codec does not read (zip64, deflate, encryption).
    Unsupported(&'static str),
    /// The bytes are not a well-formed archive this codec can follow.
    Malformed(&'static str),
    /// An entry's bytes do not match the CRC in its header.
    CrcMismatch(String),
}

impl std::fmt::Display for ZipError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZipError::TooManyEntries => write!(formatter, "more than {MAX_ENTRIES} entries"),
            ZipError::EntryTooLarge(name) => write!(formatter, "entry {name:?} is too large"),
            ZipError::UnsafeName(name) => write!(formatter, "entry name {name:?} is unsafe"),
            ZipError::Unsupported(why) => write!(formatter, "unsupported archive: {why}"),
            ZipError::Malformed(why) => write!(formatter, "malformed archive: {why}"),
            ZipError::CrcMismatch(name) => write!(formatter, "CRC mismatch for {name:?}"),
        }
    }
}

impl std::error::Error for ZipError {}

/// Write `entries` as a stored zip archive.
pub fn write(entries: &[ZipEntry]) -> Result<Vec<u8>, ZipError> {
    if entries.len() > MAX_ENTRIES {
        return Err(ZipError::TooManyEntries);
    }
    let mut out = Vec::new();
    // (name bytes, crc, size, local header offset, is_dir)
    let mut central: Vec<(Vec<u8>, u32, u32, u32, bool)> = Vec::with_capacity(entries.len());
    for entry in entries {
        let name = entry_name(&entry.name, entry.is_dir)?;
        let size = u32::try_from(entry.bytes.len())
            .map_err(|_| ZipError::EntryTooLarge(entry.name.clone()))?;
        let offset =
            u32::try_from(out.len()).map_err(|_| ZipError::EntryTooLarge(entry.name.clone()))?;
        let crc = crc32(&entry.bytes);
        out.extend_from_slice(&LOCAL_SIGNATURE.to_le_bytes());
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&FLAG_UTF8.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // stored
        out.extend_from_slice(&0u16.to_le_bytes()); // DOS time
        out.extend_from_slice(&DOS_DATE.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&name_len(&name)?.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // no extra field
        out.extend_from_slice(&name);
        out.extend_from_slice(&entry.bytes);
        central.push((name, crc, size, offset, entry.is_dir));
    }

    let directory_offset = u32::try_from(out.len())
        .map_err(|_| ZipError::EntryTooLarge("central directory".to_owned()))?;
    for (name, crc, size, offset, is_dir) in &central {
        out.extend_from_slice(&CENTRAL_SIGNATURE.to_le_bytes());
        out.extend_from_slice(&VERSION.to_le_bytes()); // version made by
        out.extend_from_slice(&VERSION.to_le_bytes()); // version needed
        out.extend_from_slice(&FLAG_UTF8.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // stored
        out.extend_from_slice(&0u16.to_le_bytes()); // DOS time
        out.extend_from_slice(&DOS_DATE.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&name_len(name)?.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra
        out.extend_from_slice(&0u16.to_le_bytes()); // comment
        out.extend_from_slice(&0u16.to_le_bytes()); // disk
        out.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        let attributes: u32 = if *is_dir { 0x10 } else { 0 };
        out.extend_from_slice(&attributes.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(name);
    }
    let directory_size = u32::try_from(out.len() - directory_offset as usize)
        .map_err(|_| ZipError::EntryTooLarge("central directory".to_owned()))?;
    let count = u16::try_from(central.len()).map_err(|_| ZipError::TooManyEntries)?;

    out.extend_from_slice(&END_SIGNATURE.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // this disk
    out.extend_from_slice(&0u16.to_le_bytes()); // directory's disk
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&directory_size.to_le_bytes());
    out.extend_from_slice(&directory_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // no comment
    Ok(out)
}

/// Read a stored zip archive into its entries, verifying every CRC.
///
/// The central directory is the source of truth (a local header may use a data
/// descriptor); each local header is still checked for its signature before
/// its data is read.
pub fn read(bytes: &[u8]) -> Result<Vec<ZipEntry>, ZipError> {
    let end = find_end(bytes).ok_or(ZipError::Malformed(
        "no end-of-central-directory record; this is not a zip archive",
    ))?;
    let count = u16_at(bytes, end + 10).ok_or(ZipError::Malformed("short end record"))?;
    let directory_size = u32_at(bytes, end + 12).ok_or(ZipError::Malformed("short end record"))?;
    let directory_offset =
        u32_at(bytes, end + 16).ok_or(ZipError::Malformed("short end record"))?;
    if count == u16::MAX || directory_size == u32::MAX || directory_offset == u32::MAX {
        return Err(ZipError::Unsupported("zip64 archives are not read"));
    }
    let directory_offset =
        usize::try_from(directory_offset).map_err(|_| ZipError::Malformed("central directory"))?;
    let directory_end = directory_offset
        .checked_add(
            usize::try_from(directory_size)
                .map_err(|_| ZipError::Malformed("central directory"))?,
        )
        .ok_or(ZipError::Malformed("central directory does not fit"))?;
    if directory_end > bytes.len() || directory_end > end {
        return Err(ZipError::Malformed("central directory does not fit"));
    }

    let mut entries = Vec::with_capacity(usize::from(count));
    let mut at = directory_offset;
    for _ in 0..count {
        let (entry, next) = read_central(bytes, at, directory_end)?;
        entries.push(entry);
        at = next;
    }
    if at > directory_end {
        return Err(ZipError::Malformed("central directory overruns its size"));
    }
    Ok(entries)
}

/// One central header plus the offset the next one starts at.
fn read_central(
    bytes: &[u8],
    at: usize,
    directory_end: usize,
) -> Result<(ZipEntry, usize), ZipError> {
    if at + 46 > directory_end || u32_at(bytes, at) != Some(CENTRAL_SIGNATURE) {
        return Err(ZipError::Malformed("central directory header"));
    }
    let flags = u16_at(bytes, at + 8).ok_or(ZipError::Malformed("central header"))?;
    let method = u16_at(bytes, at + 10).ok_or(ZipError::Malformed("central header"))?;
    let crc = u32_at(bytes, at + 16).ok_or(ZipError::Malformed("central header"))?;
    let compressed = u32_at(bytes, at + 20).ok_or(ZipError::Malformed("central header"))?;
    let size = u32_at(bytes, at + 24).ok_or(ZipError::Malformed("central header"))?;
    let name_len = u16_at(bytes, at + 28).ok_or(ZipError::Malformed("central header"))?;
    let extra_len = u16_at(bytes, at + 30).ok_or(ZipError::Malformed("central header"))?;
    let comment_len = u16_at(bytes, at + 32).ok_or(ZipError::Malformed("central header"))?;
    let local = u32_at(bytes, at + 42).ok_or(ZipError::Malformed("central header"))?;
    if flags & 0x0001 != 0 {
        return Err(ZipError::Unsupported("encrypted entries are not read"));
    }
    if method != 0 {
        return Err(ZipError::Unsupported(
            "compressed entries are not read; export the vault as a stored zip",
        ));
    }
    if compressed == u32::MAX || size == u32::MAX || local == u32::MAX {
        return Err(ZipError::Unsupported("zip64 archives are not read"));
    }
    if compressed != size {
        return Err(ZipError::Malformed(
            "a stored entry's compressed and uncompressed sizes differ",
        ));
    }
    let name_end = at + 46 + usize::from(name_len);
    let next = name_end + usize::from(extra_len) + usize::from(comment_len);
    if name_end > directory_end || next > directory_end {
        return Err(ZipError::Malformed("central header does not fit"));
    }
    let name = std::str::from_utf8(&bytes[at + 46..name_end])
        .map_err(|_| ZipError::Malformed("entry name is not UTF-8"))?
        .to_owned();

    let local_at = usize::try_from(local).map_err(|_| ZipError::Malformed("local header"))?;
    if local_at + 30 > bytes.len() || u32_at(bytes, local_at) != Some(LOCAL_SIGNATURE) {
        return Err(ZipError::Malformed("local header"));
    }
    let local_name_len = u16_at(bytes, local_at + 26).ok_or(ZipError::Malformed("local header"))?;
    let local_extra_len =
        u16_at(bytes, local_at + 28).ok_or(ZipError::Malformed("local header"))?;
    let data_at = local_at + 30 + usize::from(local_name_len) + usize::from(local_extra_len);
    let data_end = data_at
        .checked_add(usize::try_from(compressed).map_err(|_| ZipError::Malformed("entry size"))?)
        .ok_or(ZipError::Malformed("entry size"))?;
    if data_end > bytes.len() {
        return Err(ZipError::Malformed("entry data does not fit"));
    }
    let is_dir = name.ends_with('/');
    let data = &bytes[data_at..data_end];
    if !is_dir && crc32(data) != crc {
        return Err(ZipError::CrcMismatch(name));
    }
    if is_dir && !data.is_empty() {
        return Err(ZipError::Malformed("directory entry carries data"));
    }
    if is_dir && usize::try_from(size).ok() != Some(0) {
        return Err(ZipError::Malformed("directory entry has a size"));
    }
    Ok((
        ZipEntry {
            name,
            bytes: if is_dir { Vec::new() } else { data.to_vec() },
            is_dir,
        },
        next,
    ))
}

/// Find the end-of-central-directory record, scanning backwards over at most
/// one maximum-length archive comment.
fn find_end(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 22 {
        return None;
    }
    let lowest = bytes.len().saturating_sub(22 + 65_535);
    let mut at = bytes.len() - 22;
    loop {
        if u32_at(bytes, at) == Some(END_SIGNATURE)
            && let Some(comment_len) = u16_at(bytes, at + 20)
            && at + 22 + usize::from(comment_len) <= bytes.len()
        {
            return Some(at);
        }
        if at == lowest {
            return None;
        }
        at -= 1;
    }
}

/// The archive spelling of an entry name, with the directory marker.
fn entry_name(name: &str, is_dir: bool) -> Result<Vec<u8>, ZipError> {
    let mut name = name.replace('\\', "/");
    while name.ends_with('/') {
        name.pop();
    }
    let drive_prefix =
        matches!(name.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic());
    if name.is_empty()
        || name.starts_with('/')
        || name.contains('\0')
        || drive_prefix
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ZipError::UnsafeName(name));
    }
    if is_dir {
        name.push('/');
    }
    Ok(name.into_bytes())
}

fn name_len(name: &[u8]) -> Result<u16, ZipError> {
    u16::try_from(name.len())
        .map_err(|_| ZipError::EntryTooLarge(String::from_utf8_lossy(name).into_owned()))
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let slice = bytes.get(at..at + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// CRC-32 (IEEE 802.3), the checksum every zip entry carries.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        let index = usize::try_from((crc ^ u32::from(byte)) & 0xff).unwrap_or(0);
        crc = (crc >> 8) ^ CRC_TABLE[index];
    }
    !crc
}

/// The IEEE polynomial's lookup table, built at compile time.
static CRC_TABLE: [u32; 256] = crc_table();

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0usize;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                (value >> 1) ^ 0xEDB8_8320
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<ZipEntry> {
        vec![
            ZipEntry::file("Welcome.md", b"# Welcome\n".to_vec()),
            ZipEntry::dir("Projects"),
            ZipEntry::file("Projects/Plan.md", b"# Plan\n\n![[pic.png]]\n".to_vec()),
            ZipEntry::file("assets/pic.png", vec![0x89, 0x50, 0x4e, 0x47, 0x00, 0xff]),
            ZipEntry::file("Ünïcode.md", "grüße\n".as_bytes().to_vec()),
        ]
    }

    #[test]
    fn a_round_trip_keeps_every_byte_name_and_empty_folder() {
        let entries = sample();
        let bytes = write(&entries).expect("write");
        let read_back = read(&bytes).expect("read");
        assert_eq!(read_back.len(), entries.len());
        for (expected, actual) in entries.iter().zip(&read_back) {
            assert_eq!(actual.name, expected.name);
            assert_eq!(actual.is_dir, expected.is_dir);
            if expected.is_dir {
                assert_eq!(actual.name, "Projects/");
                assert!(actual.bytes.is_empty());
            } else {
                assert_eq!(actual.bytes, expected.bytes, "{}", expected.name);
            }
        }
    }

    #[test]
    fn the_written_bytes_carry_standard_zip_structures() {
        let bytes = write(&sample()).expect("write");
        assert_eq!(u32_at(&bytes, 0), Some(LOCAL_SIGNATURE));
        // The first local header names the first file and carries flags,
        // method, sizes and CRC a desktop reader uses.
        assert_eq!(u16_at(&bytes, 4), Some(VERSION));
        assert_eq!(u16_at(&bytes, 6), Some(FLAG_UTF8));
        assert_eq!(u16_at(&bytes, 8), Some(0), "stored, not deflated");
        let name_len = usize::from(u16_at(&bytes, 26).expect("name length"));
        assert_eq!(&bytes[30..30 + name_len], b"Welcome.md");
        let size = u32_at(&bytes, 22).expect("size");
        assert_eq!(size, b"# Welcome\n".len() as u32);
        let data = &bytes[30 + name_len..30 + name_len + size as usize];
        assert_eq!(u32_at(&bytes, 14), Some(crc32(data)));

        // The end record names every member and points at the directory.
        let end = find_end(&bytes).expect("end record");
        assert_eq!(u32_at(&bytes, end), Some(END_SIGNATURE));
        let count = u16_at(&bytes, end + 10).expect("count");
        assert_eq!(count, 5);
        let directory = u32_at(&bytes, end + 16).expect("directory offset") as usize;
        assert_eq!(u32_at(&bytes, directory), Some(CENTRAL_SIGNATURE));
    }

    #[test]
    fn a_corrupt_crc_is_refused() {
        let mut bytes = write(&[ZipEntry::file("One.md", b"one".to_vec())]).expect("write");
        // Flip the last byte of the stored data; the local and central CRCs
        // stay what the writer put there.
        let at = bytes.len() - 22 - 46 - 6 - 1;
        bytes[at] ^= 0xff;
        assert_eq!(
            read(&bytes),
            Err(ZipError::CrcMismatch("One.md".to_owned()))
        );
    }

    #[test]
    fn a_deflated_entry_is_refused_with_a_typed_error() {
        let mut bytes = write(&[ZipEntry::file("One.md", b"one".to_vec())]).expect("write");
        // Patch the central header's method from stored (0) to deflate (8).
        let end = find_end(&bytes).expect("end record");
        let directory = u32_at(&bytes, end + 16).expect("directory offset") as usize;
        bytes[directory + 10..directory + 12].copy_from_slice(&8u16.to_le_bytes());
        assert!(matches!(read(&bytes), Err(ZipError::Unsupported(_))));
    }

    #[test]
    fn non_zip_bytes_are_malformed() {
        assert!(matches!(
            read(b"# just a note\n"),
            Err(ZipError::Malformed(_))
        ));
        assert!(matches!(read(b""), Err(ZipError::Malformed(_))));
    }

    #[test]
    fn unsafe_names_are_refused() {
        for name in [
            "",
            "/etc/passwd",
            "../escape.md",
            "a/../../b.md",
            "a//b.md",
            "C:/x.md",
        ] {
            assert!(
                matches!(
                    write(&[ZipEntry::file(name, b"x".to_vec())]),
                    Err(ZipError::UnsafeName(_))
                ),
                "{name:?} was accepted"
            );
        }
        assert!(write(&[ZipEntry::dir("")]).is_err());
        // A plain folder keeps its marker exactly once.
        let bytes = write(&[ZipEntry::dir("notes/")]).expect("write");
        let entries = read(&bytes).expect("read");
        assert_eq!(entries[0].name, "notes/");
        assert!(entries[0].is_dir);
    }

    #[test]
    fn the_crc_matches_the_reference_check_value() {
        // The IEEE CRC-32 of "123456789" is the format's standard check value.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
