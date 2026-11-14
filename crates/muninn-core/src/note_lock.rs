//! Lock a single note behind its own password.
//!
//! [`lock`] turns a note's plaintext into a self-describing, versioned ASCII
//! envelope; [`unlock`] turns it back, failing closed on a wrong password or
//! any change to the envelope. The envelope stays a valid UTF-8 file with a
//! `.md` name, so [`Vault`](crate::Vault)'s walk still lists the note and its
//! filename (the title) still resolves wikilinks and backlinks. The body,
//! tags, and content-derived links are only inside the ciphertext.
//!
//! This module is deliberately pure: it reads no files, keeps no session
//! state, and never writes a password, key, salt or nonce anywhere. The
//! caller decides when the envelope becomes the note's bytes. See
//! `docs/per-note-encryption.md` for the threat model, the field-by-field
//! format, and where the rest of the app has to gate on [`is_locked`].
//!
//! # Envelope
//!
//! ```text
//! MUNINN-NOTE-LOCK v1
//! kdf=argon2id;m=19456;t=2;p=1;salt=<base64url, 16 bytes>
//! aead=aes-256-gcm;nonce=<base64url, 12 bytes>
//!
//! <base64url ciphertext and 16-byte tag, wrapped at 76 columns>
//! ```
//!
//! The three header lines are the associated data authenticated by AES-GCM,
//! so changing a parameter, the salt or the nonce fails [`unlock`] exactly as
//! a changed ciphertext does.
//!
//! # What it does not do
//!
//! A forgotten password cannot be recovered: there is no key escrow, recovery
//! key or Muninn account. The KDF is intentionally memory-hard, so a lock or
//! unlock attempt costs tens of milliseconds and tens of MiB of memory.

use aes_gcm::aead::{Aead as _, KeyInit as _, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use zeroize::Zeroizing;

/// The first token of every locked-note envelope.
///
/// A plaintext note that deliberately starts with this word followed by
/// ` v<number>` is indistinguishable from an envelope's first line and is
/// treated as locked. That is the price of a recognizable magic; every other
/// note is unaffected.
pub const LOCK_MAGIC: &str = "MUNINN-NOTE-LOCK";

/// The envelope version this build writes and the only one it reads.
pub const LOCK_VERSION: u8 = 1;

/// Bytes of random salt in a v1 envelope.
const SALT_LEN: usize = 16;

/// Bytes of AES-GCM nonce in a v1 envelope.
const NONCE_LEN: usize = 12;

/// Derived key length for AES-256.
const KEY_LEN: usize = 32;

/// AES-GCM authentication tag length (kept separate for readability).
const TAG_LEN: usize = 16;

/// Column at which the base64 payload is wrapped. The payload is ASCII, so
/// this is also its byte width.
const PAYLOAD_WRAP: usize = 76;

/// Argon2id memory cost, in KiB (19 MiB).
const ARGON2_M_COST: u32 = 19 * 1024;

/// Argon2id iteration count.
const ARGON2_T_COST: u32 = 2;

/// Argon2id parallelism.
const ARGON2_P_COST: u32 = 1;

/// KDF name written in the `kdf=` line.
const KDF_NAME: &str = "argon2id";

/// AEAD name written in the `aead=` line.
const AEAD_NAME: &str = "aes-256-gcm";

/// Why a lock or unlock attempt did not produce a usable note.
///
/// The variants distinguish a caller mistake or an unsupported format from a
/// failed authentication. A wrong password and a tampered envelope are
/// deliberately the same [`LockError::AuthenticationFailed`]: the caller (and
/// so the UI) must not tell them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LockError {
    /// An empty password is rejected instead of producing a lock that anyone
    /// can open.
    EmptyPassword,
    /// The text is ordinary note content, not a locked-note envelope.
    NotLocked,
    /// The envelope names a version this build does not understand.
    UnsupportedVersion,
    /// The envelope's shape is wrong (missing header line, undecodable
    /// base64, wrong salt/nonce length, or a non-canonical encoding).
    Malformed,
    /// The password is wrong, or the envelope was altered. The cause is not
    /// distinguished.
    AuthenticationFailed,
    /// The cryptographic primitive could not run: the OS random source
    /// failed, or the Argon2 parameters were rejected. Nothing was written.
    Crypto,
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::EmptyPassword => f.write_str("the note password is empty"),
            LockError::NotLocked => f.write_str("the note is not locked"),
            LockError::UnsupportedVersion => f.write_str("unsupported note lock version"),
            LockError::Malformed => f.write_str("the note lock envelope is malformed"),
            LockError::AuthenticationFailed => {
                f.write_str("wrong password, or the locked note was altered")
            }
            LockError::Crypto => f.write_str("the note lock operation could not run"),
        }
    }
}

impl std::error::Error for LockError {}

/// True when `source` looks like a locked-note envelope.
///
/// This is a cheap prefix check for the vault walk, the sidebar, search and
/// the editor: it never derives a key and never reveals whether a password
/// is right. It returns true for a recognized [`LOCK_MAGIC`] followed by any
/// numeric version, including ones this build cannot open, so an unknown
/// future version stays visibly locked instead of being shown as plaintext.
#[must_use]
pub fn is_locked(source: &str) -> bool {
    let Some(rest) = source.strip_prefix(LOCK_MAGIC) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(" v") else {
        return false;
    };
    let version = rest.split(['\n', '\r']).next().unwrap_or_default();
    !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit())
}

/// Encrypt `source` into a locked-note envelope.
///
/// Every call draws a fresh 16-byte salt and 12-byte nonce from the OS random
/// source, so locking the same text twice produces two different envelopes.
/// The password is used exactly as UTF-8 bytes; it is not normalized, stored
/// or returned. An empty password is refused.
///
/// # Errors
///
/// [`LockError::EmptyPassword`] when `password` is empty, or
/// [`LockError::Crypto`] when the OS random source or Argon2id cannot run. In
/// both cases no envelope is produced.
pub fn lock(source: &str, password: &str) -> Result<String, LockError> {
    if password.is_empty() {
        return Err(LockError::EmptyPassword);
    }
    let mut salt = [0u8; SALT_LEN];
    getrandom::fill(&mut salt).map_err(|_| LockError::Crypto)?;
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|_| LockError::Crypto)?;

    let key = derive_key(password, &salt)?;
    let header = canonical_header(&salt, &nonce);
    let cipher = Aes256Gcm::new_from_slice(&*key).map_err(|_| LockError::Crypto)?;
    let nonce = Nonce::try_from(&nonce[..]).map_err(|_| LockError::Crypto)?;
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: source.as_bytes(),
                aad: header.as_bytes(),
            },
        )
        .map_err(|_| LockError::Crypto)?;

    let encoded = URL_SAFE_NO_PAD.encode(&ciphertext);
    let mut envelope = header;
    // The blank line separates the authenticated header from the payload.
    envelope.push('\n');
    envelope.push_str(&wrap(&encoded));
    envelope.push('\n');
    Ok(envelope)
}

/// Decrypt a locked-note envelope back to the original text.
///
/// The result is owned so the caller can hand it to the editor and drop it
/// when the note is re-locked. Internal buffers holding plaintext and the
/// derived key are zeroized on drop (best effort; the returned `String` is
/// the caller's to manage).
///
/// # Errors
///
/// [`LockError::EmptyPassword`] for an empty password,
/// [`LockError::NotLocked`] when `envelope` is ordinary note text,
/// [`LockError::UnsupportedVersion`] for a future version,
/// [`LockError::Malformed`] for a broken envelope, and
/// [`LockError::AuthenticationFailed`] when the password is wrong or any byte
/// of the header or payload was altered. A failed call never returns partial
/// plaintext.
pub fn unlock(envelope: &str, password: &str) -> Result<String, LockError> {
    if password.is_empty() {
        return Err(LockError::EmptyPassword);
    }
    let parsed = parse(envelope)?;
    let key = derive_key(password, &parsed.salt)?;
    let cipher = Aes256Gcm::new_from_slice(&*key).map_err(|_| LockError::Crypto)?;
    let nonce = Nonce::try_from(&parsed.nonce[..]).map_err(|_| LockError::Crypto)?;
    let plaintext = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &parsed.ciphertext,
                aad: parsed.header.as_bytes(),
            },
        )
        .map_err(|_| LockError::AuthenticationFailed)?;
    let plaintext = Zeroizing::new(plaintext);
    let text = std::str::from_utf8(&plaintext).map_err(|_| LockError::Malformed)?;
    Ok(text.to_owned())
}

/// Derive the 32-byte AEAD key. The buffer is zeroized when it drops.
fn derive_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; KEY_LEN]>, LockError> {
    let params = Params::new(ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST, Some(KEY_LEN))
        .map_err(|_| LockError::Crypto)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut *key)
        .map_err(|_| LockError::Crypto)?;
    Ok(key)
}

/// The exact header bytes written to disk and authenticated as AAD.
fn canonical_header(salt: &[u8], nonce: &[u8]) -> String {
    format!(
        "{LOCK_MAGIC} v{LOCK_VERSION}\n\
         kdf={KDF_NAME};m={ARGON2_M_COST};t={ARGON2_T_COST};p={ARGON2_P_COST};salt={}\n\
         aead={AEAD_NAME};nonce={}\n",
        URL_SAFE_NO_PAD.encode(salt),
        URL_SAFE_NO_PAD.encode(nonce),
    )
}

/// Wrap base64 at [`PAYLOAD_WRAP`] columns with `\n` separators.
fn wrap(encoded: &str) -> String {
    let mut out = String::with_capacity(encoded.len() + encoded.len() / PAYLOAD_WRAP);
    for (i, chunk) in encoded.as_bytes().chunks(PAYLOAD_WRAP).enumerate() {
        if i > 0 {
            out.push('\n');
        }
        // The input is base64 ASCII, so every chunk is valid UTF-8.
        out.push_str(std::str::from_utf8(chunk).expect("base64 is ASCII"));
    }
    out
}

/// A syntactically valid v1 envelope, split into the parts the crypto needs.
struct Parsed {
    /// Canonical header, rebuilt from the parsed values (AAD).
    header: String,
    salt: [u8; SALT_LEN],
    nonce: [u8; NONCE_LEN],
    ciphertext: Vec<u8>,
}

/// Parse and validate an envelope without deriving a key.
///
/// CRLF is normalized to LF first, so a file that travelled through a tool
/// with Windows line endings still opens. The AEAD still authenticates the
/// decoded bytes; only the envelope's presentation is tolerant.
fn parse(envelope: &str) -> Result<Parsed, LockError> {
    let normalized = envelope.replace("\r\n", "\n");
    let envelope = normalized.as_str();
    let (first, rest) = envelope.split_once('\n').ok_or(LockError::NotLocked)?;
    let Some(version) = first
        .strip_prefix(LOCK_MAGIC)
        .and_then(|r| r.strip_prefix(" v"))
    else {
        return Err(LockError::NotLocked);
    };
    if version != LOCK_VERSION.to_string() {
        return Err(LockError::UnsupportedVersion);
    }
    let (header_block, payload) = rest.split_once("\n\n").ok_or(LockError::Malformed)?;
    let mut lines = header_block.split('\n');
    let kdf_line = lines.next().ok_or(LockError::Malformed)?;
    let aead_line = lines.next().ok_or(LockError::Malformed)?;
    if lines.next().is_some() {
        return Err(LockError::Malformed);
    }

    let salt_b64 = parse_kdf_line(kdf_line)?;
    let nonce_b64 = parse_aead_line(aead_line)?;

    let salt = decode_fixed::<SALT_LEN>(salt_b64)?;
    let nonce = decode_fixed::<NONCE_LEN>(nonce_b64)?;

    // Only ASCII whitespace is allowed between base64 characters; anything
    // else is a malformed envelope rather than a password failure.
    let mut encoded = String::with_capacity(payload.len());
    for c in payload.chars() {
        if c.is_whitespace() {
            continue;
        }
        if !c.is_ascii() {
            return Err(LockError::Malformed);
        }
        encoded.push(c);
    }
    let ciphertext = URL_SAFE_NO_PAD
        .decode(&encoded)
        .map_err(|_| LockError::Malformed)?;
    if ciphertext.len() < TAG_LEN {
        return Err(LockError::Malformed);
    }

    let header = canonical_header(&salt, &nonce);
    Ok(Parsed {
        header,
        salt,
        nonce,
        ciphertext,
    })
}

/// Parse the v1 `kdf=` line, returning the base64 salt.
///
/// The fields are fixed and in a fixed order for version 1: a different cost,
/// order or spelling is a malformed v1 envelope. A future version that wants
/// different costs must bump [`LOCK_VERSION`].
fn parse_kdf_line(line: &str) -> Result<&str, LockError> {
    let mut parts = line.split(';');
    let expected_kdf = format!("kdf={KDF_NAME}");
    if parts.next() != Some(expected_kdf.as_str()) {
        return Err(LockError::Malformed);
    }
    let expected_m = format!("m={ARGON2_M_COST}");
    if parts.next() != Some(expected_m.as_str()) {
        return Err(LockError::Malformed);
    }
    let expected_t = format!("t={ARGON2_T_COST}");
    if parts.next() != Some(expected_t.as_str()) {
        return Err(LockError::Malformed);
    }
    let expected_p = format!("p={ARGON2_P_COST}");
    if parts.next() != Some(expected_p.as_str()) {
        return Err(LockError::Malformed);
    }
    let salt = parts
        .next()
        .and_then(|part| part.strip_prefix("salt="))
        .ok_or(LockError::Malformed)?;
    if parts.next().is_some() {
        return Err(LockError::Malformed);
    }
    Ok(salt)
}

/// Parse the v1 `aead=` line, returning the base64 nonce.
fn parse_aead_line(line: &str) -> Result<&str, LockError> {
    let mut parts = line.split(';');
    let expected_aead = format!("aead={AEAD_NAME}");
    if parts.next() != Some(expected_aead.as_str()) {
        return Err(LockError::Malformed);
    }
    let nonce = parts
        .next()
        .and_then(|part| part.strip_prefix("nonce="))
        .ok_or(LockError::Malformed)?;
    if parts.next().is_some() {
        return Err(LockError::Malformed);
    }
    Ok(nonce)
}

/// Decode a canonical (unpadded) base64url value of exactly `N` bytes.
fn decode_fixed<const N: usize>(value: &str) -> Result<[u8; N], LockError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| LockError::Malformed)?;
    let array: [u8; N] = bytes.try_into().map_err(|_| LockError::Malformed)?;
    if URL_SAFE_NO_PAD.encode(array) != value {
        return Err(LockError::Malformed);
    }
    Ok(array)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "# Trip notes\n\n- café at 9\n- gate code: 4417\n\nSee [[Packing]].\n";
    const PASSWORD: &str = "correct horse battery staple";

    fn locked() -> String {
        lock(NOTE, PASSWORD).expect("lock")
    }

    #[test]
    fn round_trip_returns_the_exact_text() {
        let envelope = locked();
        assert_eq!(unlock(&envelope, PASSWORD).expect("unlock"), NOTE);
    }

    #[test]
    fn wrong_password_fails_closed() {
        let envelope = locked();
        assert_eq!(
            unlock(&envelope, "not the password"),
            Err(LockError::AuthenticationFailed)
        );
        // A near miss of the right password is still a failure.
        assert_eq!(
            unlock(&envelope, "correct horse battery stapl"),
            Err(LockError::AuthenticationFailed)
        );
    }

    #[test]
    fn tampered_payload_fails() {
        let envelope = locked();
        let at = envelope.find("\n\n").expect("payload separator") + 2;
        let mut bytes = envelope.into_bytes();
        // Flip one base64 character in the payload. Keep it inside the
        // alphabet, so this is a valid envelope whose bytes changed.
        bytes[at] = if bytes[at] == b'A' { b'B' } else { b'A' };
        let tampered = String::from_utf8(bytes).expect("still UTF-8");
        assert_eq!(
            unlock(&tampered, PASSWORD),
            Err(LockError::AuthenticationFailed)
        );
    }

    #[test]
    fn tampered_header_fails() {
        let envelope = locked();
        let marker = "salt=";
        let start = envelope.find(marker).expect("salt") + marker.len();
        let mut bytes = envelope.into_bytes();
        bytes[start] = if bytes[start] == b'A' { b'B' } else { b'A' };
        let tampered = String::from_utf8(bytes).expect("still UTF-8");
        // A different salt derives a different key; the AEAD tag (and the AAD
        // binding) reject it.
        assert_eq!(
            unlock(&tampered, PASSWORD),
            Err(LockError::AuthenticationFailed)
        );
    }

    #[test]
    fn an_unrelated_note_is_not_an_envelope() {
        assert_eq!(
            unlock("# just a note\n", PASSWORD),
            Err(LockError::NotLocked)
        );
        assert_eq!(
            unlock("MUNINN-NOTE-LOCK, but prose", PASSWORD),
            Err(LockError::NotLocked)
        );
        assert!(!is_locked("# just a note\n"));
    }

    #[test]
    fn a_future_version_is_reported_not_decoded() {
        let envelope = locked();
        let bumped = envelope.replacen("v1", "v2", 1);
        assert!(is_locked(&bumped));
        assert_eq!(
            unlock(&bumped, PASSWORD),
            Err(LockError::UnsupportedVersion)
        );
    }

    #[test]
    fn empty_passwords_are_refused() {
        assert_eq!(lock(NOTE, ""), Err(LockError::EmptyPassword));
        assert_eq!(unlock(&locked(), ""), Err(LockError::EmptyPassword));
    }

    #[test]
    fn salt_and_nonce_are_fresh_for_every_lock() {
        let one = parse(&locked()).expect("first");
        let two = parse(&locked()).expect("second");
        assert_ne!(one.salt, two.salt);
        assert_ne!(one.nonce, two.nonce);
        assert_ne!(one.ciphertext, two.ciphertext);
    }

    #[test]
    fn the_envelope_is_a_valid_utf8_markdown_file_with_a_magic() {
        let envelope = locked();
        assert!(envelope.is_ascii());
        assert!(envelope.starts_with(&format!("{LOCK_MAGIC} v{LOCK_VERSION}\n")));
        assert!(envelope.ends_with('\n'));
        assert!(is_locked(&envelope));
        assert!(std::str::from_utf8(envelope.as_bytes()).is_ok());
    }

    #[test]
    fn the_vault_walk_still_lists_a_locked_note() {
        use crate::testing::TempVault;

        let envelope = locked();
        let tmp = TempVault::new(&[("Diary.md", envelope.as_str())]);
        let vault = tmp.load();
        assert_eq!(vault.notes.len(), 1);
        assert_eq!(vault.skipped_count(), 0);
        assert_eq!(vault.notes[0].title, "Diary");
        assert!(is_locked(&vault.notes[0].source));
        // The base64url alphabet cannot form a tag or a wikilink.
        assert!(vault.notes[0].tags.is_empty());
        assert!(vault.notes[0].links.is_empty());
    }

    #[test]
    fn the_ciphertext_does_not_contain_the_plaintext() {
        let marker = "gate code: 4417";
        let envelope = locked();
        assert!(!envelope.contains(marker));
        assert!(!envelope.contains("Trip notes"));
        let parsed = parse(&envelope).expect("parse");
        let needle = marker.as_bytes();
        assert!(
            !parsed
                .ciphertext
                .windows(needle.len())
                .any(|window| window == needle),
            "the raw ciphertext must not contain the body"
        );
    }

    #[test]
    fn crlf_rewritten_envelopes_still_open() {
        let envelope = locked();
        let crlf = envelope.replace('\n', "\r\n");
        assert!(is_locked(&crlf));
        assert_eq!(unlock(&crlf, PASSWORD).expect("unlock"), NOTE);
    }

    #[test]
    fn a_malformed_envelope_is_rejected() {
        let envelope = locked();
        // Drop the blank line that separates header from payload.
        let broken = envelope.replacen("\n\n", "\n", 1);
        assert_eq!(unlock(&broken, PASSWORD), Err(LockError::Malformed));
        // Extra header line.
        let broken = envelope.replacen("\n\n", "\nextra=1\n\n", 1);
        assert_eq!(unlock(&broken, PASSWORD), Err(LockError::Malformed));
        // Non-canonical padded salt.
        let parsed = parse(&envelope).expect("parse");
        let padded = format!("{}==", URL_SAFE_NO_PAD.encode(parsed.salt));
        let broken = envelope.replacen(&URL_SAFE_NO_PAD.encode(parsed.salt), &padded, 1);
        assert_eq!(unlock(&broken, PASSWORD), Err(LockError::Malformed));
    }
}
