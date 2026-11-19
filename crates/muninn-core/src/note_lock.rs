//! Lock a single note behind its own password.
//!
//! [`lock`] turns a note's plaintext into a self-describing, versioned ASCII
//! envelope; [`unlock`] turns it back, failing closed on a wrong password or
//! any change to the envelope. The envelope stays a valid UTF-8 file with a
//! `.md` name, so [`Vault`](crate::Vault)'s walk still lists the note and its
//! filename (the title) still resolves wikilinks and backlinks. The body,
//! tags, and content-derived links are only inside the ciphertext.
//!
//! This module is deliberately pure: it reads no files and never writes a
//! password, key, salt or nonce anywhere. The caller decides when the envelope
//! becomes the note's bytes. A caller that saves repeatedly can keep a
//! [`SessionKey`], which retains only the derived key (never the password) and
//! re-seals under the same salt with a fresh nonce and the next revision. See
//! `docs/per-note-encryption.md` for the threat model, the field-by-field
//! format, and where the rest of the app has to gate on [`is_locked`].
//!
//! # Envelope
//!
//! ```text
//! MUNINN-NOTE-LOCK v2
//! kdf=argon2id;m=19456;t=2;p=1;salt=<base64url, 16 bytes>
//! aead=aes-256-gcm;nonce=<base64url, 12 bytes>
//! note=<base64url of UTF-8 note id>
//! rev=<decimal, >= 1>
//!
//! <base64url ciphertext and 16-byte tag, wrapped at 76 columns>
//! ```
//!
//! The four header lines are the associated data authenticated by AES-GCM, so
//! changing a parameter, the salt, the nonce, the note id or the revision
//! fails [`unlock`] exactly as a changed ciphertext does.
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
pub const LOCK_VERSION: u8 = 2;

/// Bytes of random salt in a v2 envelope.
const SALT_LEN: usize = 16;

/// Bytes of AES-GCM nonce in a v2 envelope.
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
/// so the UI) must not tell them apart. A wrong note id is
/// [`LockError::WrongNote`] and is detected before any key is derived, so it
/// cannot be used as a password oracle either.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LockError {
    /// An empty password is rejected instead of producing a lock that anyone
    /// can open.
    EmptyPassword,
    /// An empty note id is rejected: the envelope would authenticate no
    /// identity at all.
    EmptyNote,
    /// The text is ordinary note content, not a locked-note envelope.
    NotLocked,
    /// The envelope names a version this build does not understand.
    UnsupportedVersion,
    /// The envelope's shape is wrong (missing header line, undecodable
    /// base64, wrong salt/nonce length, or a non-canonical encoding).
    Malformed,
    /// The envelope is a valid lock for a different note. Detected before key
    /// derivation; no plaintext and no key material is produced.
    WrongNote,
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
            LockError::EmptyNote => f.write_str("the note id is empty"),
            LockError::NotLocked => f.write_str("the note is not locked"),
            LockError::UnsupportedVersion => f.write_str("unsupported note lock version"),
            LockError::Malformed => f.write_str("the note lock envelope is malformed"),
            LockError::WrongNote => f.write_str("the locked note belongs to another note"),
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

/// The note id an envelope binds: `rel` without one trailing `.md`.
///
/// Callers pass [`Note::rel`](crate::Note::rel), which is already normalized
/// to `/` separators and includes the `.md` extension. Only one suffix is
/// stripped, so `a/b.md.md` names `a/b.md` and `a/b` is returned unchanged.
/// The result is the [`Note::lock_id`](crate::Note::lock_id) value that
/// [`lock`], [`unlock`] and [`unlock_session`] expect.
#[must_use]
pub fn note_id(rel: &str) -> &str {
    rel.strip_suffix(".md").unwrap_or(rel)
}

/// Encrypt `source` into a locked-note envelope for `note`.
///
/// `note` is the note id ([`note_id`] of `Note.rel`), and is authenticated in
/// the header, so an envelope cannot be moved onto another note's filename.
/// Every call draws a fresh 16-byte salt and 12-byte nonce from the OS random
/// source, so locking the same text twice produces two different envelopes.
/// The revision starts at 1. The password is used exactly as UTF-8 bytes; it
/// is not normalized, stored or returned.
///
/// # Errors
///
/// [`LockError::EmptyPassword`] when `password` is empty,
/// [`LockError::EmptyNote`] when `note` is empty, or [`LockError::Crypto`]
/// when the OS random source or Argon2id cannot run. In every case no
/// envelope is produced.
pub fn lock(source: &str, password: &str, note: &str) -> Result<String, LockError> {
    if password.is_empty() {
        return Err(LockError::EmptyPassword);
    }
    if note.is_empty() {
        return Err(LockError::EmptyNote);
    }
    let mut salt = [0u8; SALT_LEN];
    getrandom::fill(&mut salt).map_err(|_| LockError::Crypto)?;
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|_| LockError::Crypto)?;

    let key = derive_key(password, &salt)?;
    encrypt(source, note, &salt, &nonce, 1, &key)
}

/// Decrypt a locked-note envelope back to the original text.
///
/// `note` is the note id of the file the envelope was read from; the envelope
/// must name the same note. The result is owned so the caller can hand it to
/// the editor and drop it when the note is re-locked. Internal buffers holding
/// plaintext and the derived key are zeroized on drop (best effort; the
/// returned `String` is the caller's to manage).
///
/// # Errors
///
/// [`LockError::EmptyPassword`] for an empty password,
/// [`LockError::NotLocked`] when `envelope` is ordinary note text,
/// [`LockError::UnsupportedVersion`] for a future version,
/// [`LockError::Malformed`] for a broken envelope,
/// [`LockError::WrongNote`] when the envelope names a different note (checked
/// before the key is derived), and [`LockError::AuthenticationFailed`] when
/// the password is wrong or any byte of the header or payload was altered. A
/// failed call never returns partial plaintext.
pub fn unlock(envelope: &str, password: &str, note: &str) -> Result<String, LockError> {
    Ok(open(envelope, password, note)?.text)
}

/// Decrypt an envelope and keep its key as a [`SessionKey`].
///
/// Performs exactly the checks of [`unlock`], but also returns a session key
/// so later saves can be sealed without keeping the password. The key is
/// derived once; each [`SessionKey::seal`] uses the same salt and a fresh
/// nonce.
///
/// # Errors
///
/// The same errors as [`unlock`].
pub fn unlock_session(
    envelope: &str,
    password: &str,
    note: &str,
) -> Result<(SessionKey, String), LockError> {
    let opened = open(envelope, password, note)?;
    let session = SessionKey {
        note: opened.note,
        salt: opened.salt,
        rev: opened.rev,
        key: opened.key,
    };
    Ok((session, opened.text))
}

/// A derived key for one note, for saving without retaining the password.
///
/// Created by [`unlock_session`]. It holds the note id, the salt the key was
/// derived from, the key itself, and the last revision it wrote or read. It
/// never holds the password, and it has no `Clone` so one unlocked session
/// cannot be duplicated by accident. Dropping it zeroizes the key.
pub struct SessionKey {
    /// The note id this key is bound to.
    note: String,
    /// Salt from the envelope the key was derived from; reused by `seal`.
    salt: [u8; SALT_LEN],
    /// The derived AES-256 key, cleared on drop.
    key: Zeroizing<[u8; KEY_LEN]>,
    /// The highest revision this session has read or written.
    rev: u64,
}

impl SessionKey {
    /// Encrypt `source` as the next revision of this note.
    ///
    /// Every call draws a fresh 12-byte nonce and keeps the session's salt, so
    /// the derived key stays valid while no nonce is reused. The revision is
    /// incremented and written into the authenticated header; it is only
    /// advanced after the encryption succeeds.
    ///
    /// # Errors
    ///
    /// [`LockError::Crypto`] when the OS random source, the counter, or the
    /// AEAD cannot produce an envelope. On an error the session's revision is
    /// unchanged.
    pub fn seal(&mut self, source: &str) -> Result<String, LockError> {
        let rev = self.rev.checked_add(1).ok_or(LockError::Crypto)?;
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(|_| LockError::Crypto)?;
        let envelope = encrypt(source, &self.note, &self.salt, &nonce, rev, &self.key)?;
        self.rev = rev;
        Ok(envelope)
    }

    /// Encrypt `source` as the next revision under a different note id.
    ///
    /// A rename or a move changes the id the envelope authenticates, so the
    /// next save cannot reuse [`SessionKey::seal`] while the file still has its
    /// old name. This is that save: it keeps the session's salt and key, draws
    /// a fresh 12-byte nonce, writes `note` into the header instead of the
    /// session's own id, and advances the revision exactly like `seal` — only
    /// after the encryption succeeds. The session's own note id is unchanged;
    /// the caller follows the file with [`SessionKey::rebind`] once the rename
    /// has happened.
    ///
    /// # Errors
    ///
    /// [`LockError::EmptyNote`] when `note` is empty, or [`LockError::Crypto`]
    /// when the OS random source, the counter, or the AEAD cannot produce an
    /// envelope. On an error the session's revision is unchanged.
    pub fn seal_as(&mut self, note: &str, source: &str) -> Result<String, LockError> {
        if note.is_empty() {
            return Err(LockError::EmptyNote);
        }
        let rev = self.rev.checked_add(1).ok_or(LockError::Crypto)?;
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(|_| LockError::Crypto)?;
        let envelope = encrypt(source, note, &self.salt, &nonce, rev, &self.key)?;
        self.rev = rev;
        Ok(envelope)
    }

    /// The revision of the last envelope this session read or wrote.
    #[must_use]
    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// The note id this key is bound to.
    #[must_use]
    pub fn note(&self) -> &str {
        &self.note
    }

    /// Point this session at a new note id after a rename or move.
    ///
    /// The key is derived from the password and salt, not the note id, so it
    /// stays valid; only the authenticated label changes. The next
    /// [`SessionKey::seal`] writes the new id and continues the revision.
    /// Call this only after the file has moved: sealing under the new id while
    /// the old file still exists would make the old copy unopenable.
    ///
    /// # Errors
    ///
    /// [`LockError::EmptyNote`] when `note` is empty; the session is unchanged.
    pub fn rebind(&mut self, note: &str) -> Result<(), LockError> {
        if note.is_empty() {
            return Err(LockError::EmptyNote);
        }
        self.note = note.to_owned();
        Ok(())
    }
}

/// The parts of a successfully authenticated envelope.
struct Opened {
    note: String,
    salt: [u8; SALT_LEN],
    rev: u64,
    key: Zeroizing<[u8; KEY_LEN]>,
    text: String,
}

/// Parse, check the note id, derive, decrypt.
///
/// The note check happens before [`derive_key`], so opening an envelope under
/// the wrong note reports [`LockError::WrongNote`] without doing expensive KDF
/// work and without becoming a password oracle.
fn open(envelope: &str, password: &str, note: &str) -> Result<Opened, LockError> {
    if password.is_empty() {
        return Err(LockError::EmptyPassword);
    }
    let parsed = parse(envelope)?;
    if parsed.note != note {
        return Err(LockError::WrongNote);
    }
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
    let text = std::str::from_utf8(&plaintext)
        .map_err(|_| LockError::Malformed)?
        .to_owned();
    Ok(Opened {
        note: parsed.note,
        salt: parsed.salt,
        rev: parsed.rev,
        key,
        text,
    })
}

/// Encrypt into a complete envelope. The header is both written and the AAD.
fn encrypt(
    source: &str,
    note: &str,
    salt: &[u8; SALT_LEN],
    nonce: &[u8; NONCE_LEN],
    rev: u64,
    key: &Zeroizing<[u8; KEY_LEN]>,
) -> Result<String, LockError> {
    let header = canonical_header(salt, nonce, note, rev);
    let cipher = Aes256Gcm::new_from_slice(&**key).map_err(|_| LockError::Crypto)?;
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
///
/// All four lines end in `\n`, including `rev`; the blank line that follows on
/// disk is not part of the AAD.
fn canonical_header(salt: &[u8], nonce: &[u8], note: &str, rev: u64) -> String {
    format!(
        "{LOCK_MAGIC} v{LOCK_VERSION}\n\
         kdf={KDF_NAME};m={ARGON2_M_COST};t={ARGON2_T_COST};p={ARGON2_P_COST};salt={}\n\
         aead={AEAD_NAME};nonce={}\n\
         note={}\n\
         rev={rev}\n",
        URL_SAFE_NO_PAD.encode(salt),
        URL_SAFE_NO_PAD.encode(nonce),
        URL_SAFE_NO_PAD.encode(note),
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

/// A syntactically valid v2 envelope, split into the parts the crypto needs.
struct Parsed {
    /// Canonical header, rebuilt from the parsed values (AAD).
    header: String,
    salt: [u8; SALT_LEN],
    nonce: [u8; NONCE_LEN],
    note: String,
    rev: u64,
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
    let note_line = lines.next().ok_or(LockError::Malformed)?;
    let rev_line = lines.next().ok_or(LockError::Malformed)?;
    if lines.next().is_some() {
        return Err(LockError::Malformed);
    }

    let salt_b64 = parse_kdf_line(kdf_line)?;
    let nonce_b64 = parse_aead_line(aead_line)?;
    let note = parse_note_line(note_line)?;
    let rev = parse_rev_line(rev_line)?;

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

    let header = canonical_header(&salt, &nonce, &note, rev);
    Ok(Parsed {
        header,
        salt,
        nonce,
        note,
        rev,
        ciphertext,
    })
}

/// Parse the v2 `kdf=` line, returning the base64 salt.
///
/// The fields are fixed and in a fixed order for version 2: a different cost,
/// order or spelling is a malformed v2 envelope. A future version that wants
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

/// Parse the v2 `aead=` line, returning the base64 nonce.
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

/// Parse the v2 `note=` line into the note id it names.
///
/// The value must be canonical unpadded base64url of non-empty UTF-8. A padded
/// variant, an undecodable byte or an empty id is [`LockError::Malformed`].
fn parse_note_line(line: &str) -> Result<String, LockError> {
    let value = line.strip_prefix("note=").ok_or(LockError::Malformed)?;
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| LockError::Malformed)?;
    if bytes.is_empty() {
        return Err(LockError::Malformed);
    }
    if URL_SAFE_NO_PAD.encode(&bytes) != value {
        return Err(LockError::Malformed);
    }
    String::from_utf8(bytes).map_err(|_| LockError::Malformed)
}

/// Parse the v2 `rev=` line: canonical decimal, at least 1.
///
/// Leading zeros are rejected, so `01` is [`LockError::Malformed`]; the field
/// text must equal [`u64::to_string`] of the parsed value.
fn parse_rev_line(line: &str) -> Result<u64, LockError> {
    let value = line.strip_prefix("rev=").ok_or(LockError::Malformed)?;
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(LockError::Malformed);
    }
    let rev: u64 = value.parse().map_err(|_| LockError::Malformed)?;
    if rev < 1 || rev.to_string() != value {
        return Err(LockError::Malformed);
    }
    Ok(rev)
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
    const NOTE_ID: &str = "notes/trip";
    const PASSWORD: &str = "correct horse battery staple";

    fn locked() -> String {
        lock(NOTE, PASSWORD, NOTE_ID).expect("lock")
    }

    #[test]
    fn round_trip_returns_the_exact_text() {
        let envelope = locked();
        assert_eq!(unlock(&envelope, PASSWORD, NOTE_ID).expect("unlock"), NOTE);
    }

    #[test]
    fn a_unicode_note_id_round_trips() {
        let note = "Café/秘密";
        let envelope = lock(NOTE, PASSWORD, note).expect("lock");
        assert_eq!(unlock(&envelope, PASSWORD, note).expect("unlock"), NOTE);
        assert_eq!(parse(&envelope).expect("parse").note, note);
    }

    #[test]
    fn wrong_password_fails_closed() {
        let envelope = locked();
        assert_eq!(
            unlock(&envelope, "not the password", NOTE_ID),
            Err(LockError::AuthenticationFailed)
        );
        // A near miss of the right password is still a failure.
        assert_eq!(
            unlock(&envelope, "correct horse battery stapl", NOTE_ID),
            Err(LockError::AuthenticationFailed)
        );
    }

    #[test]
    fn a_different_note_is_wrong_note_not_a_password_failure() {
        let envelope = locked();
        assert_eq!(
            unlock(&envelope, PASSWORD, "notes/other"),
            Err(LockError::WrongNote)
        );
        // The right password does not help: the check runs before derivation.
        assert_eq!(
            unlock_session(&envelope, PASSWORD, "notes/other")
                .map(|(key, _)| key.note().to_owned()),
            Err(LockError::WrongNote)
        );
    }

    #[test]
    fn a_substituted_envelope_names_its_original_note() {
        let private = lock("private body", PASSWORD, "Private").expect("lock");
        assert_eq!(
            unlock(&private, PASSWORD, "Public"),
            Err(LockError::WrongNote),
            "moving the bytes onto Public.md does not make them Public"
        );
        assert_eq!(
            unlock(&private, PASSWORD, "Private").expect("unlock"),
            "private body"
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
            unlock(&tampered, PASSWORD, NOTE_ID),
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
            unlock(&tampered, PASSWORD, NOTE_ID),
            Err(LockError::AuthenticationFailed)
        );
    }

    #[test]
    fn tampering_with_rev_fails_authentication() {
        let envelope = locked();
        let tampered = envelope.replacen("rev=1\n", "rev=2\n", 1);
        assert_ne!(tampered, envelope, "the revision was rewritten");
        assert_eq!(
            unlock(&tampered, PASSWORD, NOTE_ID),
            Err(LockError::AuthenticationFailed)
        );
    }

    #[test]
    fn an_unrelated_note_is_not_an_envelope() {
        assert_eq!(
            unlock("# just a note\n", PASSWORD, NOTE_ID),
            Err(LockError::NotLocked)
        );
        assert_eq!(
            unlock("MUNINN-NOTE-LOCK, but prose", PASSWORD, NOTE_ID),
            Err(LockError::NotLocked)
        );
        assert!(!is_locked("# just a note\n"));
    }

    #[test]
    fn a_v1_envelope_is_reported_not_decoded() {
        let envelope = locked();
        let old = envelope.replacen("v2", "v1", 1);
        assert!(is_locked(&old));
        assert_eq!(
            unlock(&old, PASSWORD, NOTE_ID),
            Err(LockError::UnsupportedVersion)
        );
    }

    #[test]
    fn empty_passwords_and_notes_are_refused() {
        assert_eq!(lock(NOTE, "", NOTE_ID), Err(LockError::EmptyPassword));
        assert_eq!(
            unlock(&locked(), "", NOTE_ID),
            Err(LockError::EmptyPassword)
        );
        assert_eq!(lock(NOTE, PASSWORD, ""), Err(LockError::EmptyNote));
    }

    #[test]
    fn salt_and_nonce_are_fresh_for_every_lock() {
        let one = parse(&locked()).expect("first");
        let two = parse(&locked()).expect("second");
        assert_ne!(one.salt, two.salt, "a new lock draws a fresh salt");
        assert_ne!(one.nonce, two.nonce);
        assert_ne!(one.ciphertext, two.ciphertext);
    }

    #[test]
    fn seal_keeps_the_salt_and_advances_the_revision() {
        let first = locked();
        let (mut session, text) = unlock_session(&first, PASSWORD, NOTE_ID).expect("session");
        assert_eq!(text, NOTE);
        assert_eq!(session.note(), NOTE_ID);
        assert_eq!(session.rev(), 1);

        let second = session.seal("second body").expect("seal");
        assert_eq!(session.rev(), 2);
        let third = session.seal("third body").expect("seal");
        assert_eq!(session.rev(), 3);

        let one = parse(&first).expect("first");
        let two = parse(&second).expect("second");
        let three = parse(&third).expect("third");
        assert_eq!(one.rev, 1);
        assert_eq!(two.rev, 2);
        assert_eq!(three.rev, 3);
        assert_eq!(one.salt, two.salt, "a session keeps its salt");
        assert_eq!(one.salt, three.salt);
        assert_ne!(one.nonce, two.nonce, "every seal draws a fresh nonce");
        assert_ne!(two.nonce, three.nonce);
        assert_ne!(one.nonce, three.nonce);
        assert_eq!(two.note, NOTE_ID);
        assert_eq!(three.note, NOTE_ID);

        // The newest envelope reopens with the original password.
        assert_eq!(
            unlock(&third, PASSWORD, NOTE_ID).expect("unlock sealed"),
            "third body"
        );
    }

    #[test]
    fn a_rebound_session_seals_under_the_new_note_id() {
        let first = locked();
        let (mut session, _) = unlock_session(&first, PASSWORD, NOTE_ID).expect("session");
        session.rebind("renamed/trip").expect("rebind");
        assert_eq!(session.note(), "renamed/trip");

        let sealed = session.seal(NOTE).expect("seal");
        assert_eq!(parse(&sealed).expect("parse").note, "renamed/trip");
        assert_eq!(
            unlock(&sealed, PASSWORD, "renamed/trip").expect("new note id"),
            NOTE
        );
        assert_eq!(
            unlock(&sealed, PASSWORD, NOTE_ID),
            Err(LockError::WrongNote),
            "the old name no longer opens the sealed file"
        );
        assert_eq!(session.rebind(""), Err(LockError::EmptyNote));
        assert_eq!(session.note(), "renamed/trip", "a failed rebind keeps it");
    }

    #[test]
    fn seal_as_writes_the_new_note_id_and_advances_the_revision() {
        let first = locked();
        let (mut session, _) = unlock_session(&first, PASSWORD, NOTE_ID).expect("session");
        assert_eq!(session.rev(), 1);

        let renamed = session
            .seal_as("renamed/trip", "renamed body")
            .expect("seal_as");
        assert_eq!(
            session.note(),
            NOTE_ID,
            "seal_as does not rebind the session"
        );
        assert_eq!(session.rev(), 2, "the revision advanced on success");
        let parsed = parse(&renamed).expect("parse");
        assert_eq!(parsed.note, "renamed/trip");
        assert_eq!(parsed.rev, 2);
        assert_eq!(
            unlock(&renamed, PASSWORD, "renamed/trip").expect("new note id"),
            "renamed body"
        );
        assert_eq!(
            unlock(&renamed, PASSWORD, NOTE_ID),
            Err(LockError::WrongNote),
            "the old name no longer opens the envelope"
        );

        // The next seal continues the revision that seal_as consumed.
        let next = session.seal("next body").expect("seal");
        assert_eq!(session.rev(), 3);
        assert_eq!(parse(&next).expect("parse").rev, 3);
        assert_eq!(parse(&next).expect("parse").note, NOTE_ID);
        assert_eq!(
            unlock(&next, PASSWORD, NOTE_ID).expect("unlock"),
            "next body"
        );
    }

    #[test]
    fn seal_as_keeps_the_salt_and_draws_a_fresh_nonce() {
        let first = locked();
        let (mut session, _) = unlock_session(&first, PASSWORD, NOTE_ID).expect("session");
        let renamed = session.seal_as("Renamed", NOTE).expect("seal_as");
        let one = parse(&first).expect("first");
        let two = parse(&renamed).expect("renamed");
        assert_eq!(one.salt, two.salt, "a session keeps its salt");
        assert_ne!(one.nonce, two.nonce, "every seal_as draws a fresh nonce");
    }

    #[test]
    fn a_failed_seal_as_does_not_advance_the_revision() {
        let first = locked();
        let (mut session, _) = unlock_session(&first, PASSWORD, NOTE_ID).expect("session");
        assert_eq!(session.seal_as("", NOTE), Err(LockError::EmptyNote));
        assert_eq!(session.rev(), 1, "an empty id leaves the revision alone");
        assert_eq!(session.note(), NOTE_ID);
    }

    #[test]
    fn note_id_strips_one_extension_only() {
        assert_eq!(note_id("a/b.md"), "a/b");
        assert_eq!(note_id("a/b.md.md"), "a/b.md");
        assert_eq!(note_id("a/b"), "a/b");
        assert_eq!(note_id("Welcome.md"), "Welcome");
    }

    #[test]
    fn the_envelope_is_a_valid_utf8_markdown_file_with_a_magic() {
        let envelope = locked();
        assert!(envelope.is_ascii());
        assert!(envelope.starts_with(&format!("{LOCK_MAGIC} v{LOCK_VERSION}\n")));
        assert!(envelope.starts_with("MUNINN-NOTE-LOCK v2\n"));
        assert!(envelope.ends_with('\n'));
        assert!(is_locked(&envelope));
        assert!(std::str::from_utf8(envelope.as_bytes()).is_ok());

        // Four authenticated header lines, then the blank separator.
        let mut lines = envelope.lines();
        assert_eq!(lines.next(), Some("MUNINN-NOTE-LOCK v2"));
        assert!(lines.next().is_some_and(|l| l.starts_with("kdf=argon2id;")));
        assert!(
            lines
                .next()
                .is_some_and(|l| l.starts_with("aead=aes-256-gcm;"))
        );
        assert!(lines.next().is_some_and(|l| l.starts_with("note=")));
        assert_eq!(lines.next(), Some("rev=1"));
        assert_eq!(lines.next(), Some(""));
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
        assert_eq!(unlock(&crlf, PASSWORD, NOTE_ID).expect("unlock"), NOTE);
    }

    #[test]
    fn a_malformed_envelope_is_rejected() {
        let envelope = locked();
        // Drop the blank line that separates header from payload.
        let broken = envelope.replacen("\n\n", "\n", 1);
        assert_eq!(
            unlock(&broken, PASSWORD, NOTE_ID),
            Err(LockError::Malformed)
        );
        // Extra header line.
        let broken = envelope.replacen("\n\n", "\nextra=1\n\n", 1);
        assert_eq!(
            unlock(&broken, PASSWORD, NOTE_ID),
            Err(LockError::Malformed)
        );
        // Non-canonical padded salt.
        let parsed = parse(&envelope).expect("parse");
        let padded = format!("{}==", URL_SAFE_NO_PAD.encode(parsed.salt));
        let broken = envelope.replacen(&URL_SAFE_NO_PAD.encode(parsed.salt), &padded, 1);
        assert_eq!(
            unlock(&broken, PASSWORD, NOTE_ID),
            Err(LockError::Malformed)
        );
    }

    #[test]
    fn non_canonical_note_and_rev_are_malformed() {
        let envelope = locked();

        // `01` is not canonical decimal.
        let broken = envelope.replacen("rev=1\n", "rev=01\n", 1);
        assert_eq!(
            unlock(&broken, PASSWORD, NOTE_ID),
            Err(LockError::Malformed)
        );

        // A padded note id is not canonical unpadded base64url.
        let encoded = URL_SAFE_NO_PAD.encode(NOTE_ID);
        let broken = envelope.replacen(&format!("note={encoded}"), &format!("note={encoded}=="), 1);
        assert_eq!(
            unlock(&broken, PASSWORD, NOTE_ID),
            Err(LockError::Malformed)
        );

        // An empty note id decodes but names nothing.
        let broken = envelope.replacen(&format!("note={encoded}"), "note=", 1);
        assert_eq!(
            unlock(&broken, PASSWORD, NOTE_ID),
            Err(LockError::Malformed)
        );
    }
}
