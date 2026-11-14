# Per-note encryption

**Status:** design + core prototype. This document specifies the on-disk
format and the integration points for issue #113. The encryption envelope is
implemented and unit-tested in `crates/muninn-core/src/note_lock.rs`; the
editor, search, sync and UI are not wired to it yet.

## Threat model

### In scope

An attacker who can read:

- the vault folder on disk (another user account, a stolen disk image, a
  misconfigured backup),
- a copy or backup of that folder, or
- the connected sync store (the Drive app-data store, the GitHub repository,
  or a Tailscale peer),

but who does **not** know the note's password. The note's body, tags,
frontmatter, and the links derived from its content must be ciphertext in all
of those places. The filename and therefore the title stay visible.

### Out of scope

- an attacker who can use a running Muninn session in which the note is
  already unlocked;
- malware inside the Muninn process, a debugger attached to it, or a
  keylogger;
- replacing the operating system's disk encryption;
- account recovery: Muninn is not a key service and has no account that can
  reset a forgotten note password.

### Residual leaks the envelope cannot remove

These are not failures of the format; the lock UI must state them so a user
does not get a false sense of security.

- **Metadata.** The filename (title), folder, file size (which reveals the
  plaintext length to within base64 expansion), modification time, existence
  and the number of locked notes stay visible. Sync sees the same file set as
  before.
- **Prior copies.** Locking encrypts the current file. Earlier plaintext can
  still exist in a vault backup, a version-control history, a sync
  checkpoint, or an immutable provider blob (Drive/Tailscale blobs and GitHub
  commits are create-only). Muninn does not rewrite history.
- **Offline guessing.** Someone with the envelope can try passwords offline.
  Argon2id makes each guess expensive; password strength still matters.
- **Substitution.** Moving a locked envelope over another note's filename
  makes it decrypt under the original password and appear under the other
  title; see “What is not authenticated” below.

## What a locked note looks like

- The file keeps its `.md` name and therefore its title and wikilink target.
- The file's bytes become a versioned ASCII envelope (below), so it is valid
  UTF-8 and `Vault`'s walk still lists it as a note. A binary blob would be
  skipped by the invalid-UTF-8 rule
  (`invalid_utf8_notes_are_skipped_and_counted` in
  `crates/muninn-core/src/vault.rs`).
- The body, frontmatter, tags and content-derived links are only inside the
  ciphertext. A locked note therefore has no outgoing links or backlinks from
  content until it is unlocked in a session; other notes can still link *to*
  it by title or path, because resolution uses the filename.
- There is no password reset, recovery key or Muninn-side escrow in v1. A
  lost password loses the note.

### Not the Board `locked` flag

`crates/muninn-core/src/board/edit.rs` has `Shape::is_locked()`, which reads the
`locked: true` key Excalidraw writes on a drawing element. That flag only
stops a shape being picked, moved or deleted on the canvas. It has nothing to
do with confidentiality and nothing in this document changes it.

Likewise, this is not the OS credential store used for sync tokens
(`crates/muninn-sync/src/credentials.rs`, `secret.rs`). The note password is
never put in that store by the v1 flow.

## Envelope format v1

The whole envelope is ASCII by construction. Any Unicode in the plaintext is
encrypted and then base64-encoded, so no non-ASCII byte reaches the file.

```text
MUNINN-NOTE-LOCK v1
kdf=argon2id;m=19456;t=2;p=1;salt=<base64url, 16 bytes>
aead=aes-256-gcm;nonce=<base64url, 12 bytes>

<base64url of ciphertext || 16-byte tag, wrapped at 76 columns>
```

A concrete shape (values shortened for readability):

```text
MUNINN-NOTE-LOCK v1
kdf=argon2id;m=19456;t=2;p=1;salt=3q2-7wXk9mQ1sT4uV8yB0A
aead=aes-256-gcm;nonce=Rk3n8Yq2Wm5Zp7Ld

1v6Qm8sK2xT9pR4cW7yB3nH5jL0fD6gA1eZ8uI4oP2aS9dF3hJ6kM0nQ5rT7vX1yB4c
N8mZ2pR6tW9yA3dF7hJ1kL5qS0uX4cV8bN2mQ6rT9wZ3pD7gJ1kL5nS0uY4aC8e
```

### Fields

| Line | Field | v1 value | Rule |
| --- | --- | --- | --- |
| 1 | magic | `MUNINN-NOTE-LOCK` | First bytes of the file. Case-sensitive. |
| 1 | version | `v1` | Decimal, no leading sign. Only `1` is read. Unknown versions are detected as locked but not decrypted. |
| 2 | `kdf` | `argon2id` | Fixed in v1. |
| 2 | `m` | `19456` | Argon2 memory in KiB. Fixed in v1. |
| 2 | `t` | `2` | Argon2 iterations. Fixed in v1. |
| 2 | `p` | `1` | Argon2 lanes. Fixed in v1. |
| 2 | `salt` | 22 base64url chars | Exactly 16 random bytes, fresh for every lock/save. |
| 3 | `aead` | `aes-256-gcm` | Fixed in v1. |
| 3 | `nonce` | 16 base64url chars | Exactly 12 random bytes, fresh for every lock/save. |
| 4 | separator | empty line | One `\n` after the third header line. |
| 5+ | payload | base64url | AES-GCM ciphertext followed by the 16-byte tag. Wrapped at 76 columns; whitespace between characters is ignored when reading. |

### Associated data

The first three header lines, in the exact canonical form above and including
their trailing `\n` before the blank separator, are passed to AES-GCM as
associated data (AAD). The version is part of that header. Changing any
parameter, the salt, the nonce, or the version therefore fails authentication
exactly like changing the payload. The blank line and payload wrapping are
presentation only and are not authenticated.

### Parsing rules (v1)

A reader accepts an envelope only when all of these hold:

1. `LOCK_MAGIC`, a space, `v`, and a decimal version start the file.
2. The header has exactly two field lines, in the order above. No extra,
   missing or reordered fields. Numeric values must equal the v1 constants;
   a different `m`/`t`/`p` is `Malformed`, never a silent downgrade.
3. `salt` and `nonce` decode as unpadded base64url to exactly 16 and 12 bytes,
   and re-encode to the same text (canonical encoding only). A padded variant
   is rejected.
4. The payload contains only base64url characters and ASCII whitespace,
   decodes successfully, and is at least the 16-byte tag.
5. The password is not empty.
6. The AEAD tag verifies against the canonical header as AAD.

A file that fails 1 is ordinary text (`NotLocked`). A recognized magic with an
unknown version is `UnsupportedVersion` and stays visibly locked. All
authentication failures — wrong password or altered bytes — are the single
`AuthenticationFailed` error, so the UI cannot become a password oracle.

The reader normalizes CRLF to LF before parsing. This keeps a locked file
readable if a tool rewrote line endings; the AEAD still verifies the decoded
bytes, so only presentation is tolerant.

### Why this shape stays a note

- It is ASCII, so `fs::read_to_string` succeeds and the file is not counted as
  skipped.
- The first line is plain text, not a heading, so no title is derived from it.
  The title remains the file stem (`Note.title`).
- The base64url alphabet has no `#`, `[`, `]`, or `|`, so the tag scanner and
  wikilink parser cannot find content-derived tags or links in the ciphertext.
- `is_kanban` sees no `kanban-plugin` frontmatter, so an envelope is never
  parsed as a kanban board.

The integration still guards all of those readers explicitly (see below)
rather than relying on the alphabet. A future envelope version must keep the
same guarantee or the walk needs a change.

### What is not authenticated

The envelope does not name the note. An attacker who can write the vault can
move `Private.md`'s envelope over `Public.md`; the bytes still authenticate
under the original password and would be displayed under the new title.
Binding the vault-relative stem into the AAD would stop that, but it would
also invalidate the ciphertext on every rename, or force a re-encrypt on
rename while unlocked. It is left as an open question below. Moving files is
outside the read-only threat model, but it is not a property of the format.

The envelope also has no anti-rollback counter. An attacker who can write can
restore an older valid envelope. That is likewise outside the read-only
threat model and would need per-note monotonic state that sync does not carry.

## Crypto choices

### Key derivation: Argon2id (recommended)

| Parameter | Value |
| --- | --- |
| Algorithm | Argon2id, version 0x13 (19) |
| Memory | 19,456 KiB (19 MiB) |
| Iterations | 2 |
| Parallelism | 1 |
| Salt | 16 random bytes per lock |
| Output | 32 bytes (AES-256 key) |

Argon2id is memory-hard, resists GPU/ASIC amortization better than
PBKDF2, and is the default in current password-hashing guidance. The hybrid
mode uses the data-independent first pass and data-dependent later passes,
which limits side-channel exposure while keeping the memory cost. The
parameters are the OWASP baseline for interactive logins and cost tens of
milliseconds on a laptop, which is acceptable for an explicit lock/unlock
action. The salt is random per lock, so two notes with the same password
derive different keys.

**Conservative alternative: PBKDF2-HMAC-SHA-256.** If a future compliance
requirement names a FIPS-approved KDF, PBKDF2 with a high iteration count
(e.g. 600,000) is the conventional fallback. It is simpler and widely
available but far weaker against parallel attackers for the same user-facing
latency. Using it would be a new envelope version, not a silent change.

### AEAD: AES-256-GCM (recommended)

| Parameter | Value |
| --- | --- |
| Cipher | AES-256-GCM |
| Nonce | 12 random bytes per encryption |
| Tag | 16 bytes |
| AAD | canonical three-line header |

AES-GCM is authenticated, streams over the whole body without padding, has
hardware acceleration on the platforms Muninn targets, and is implemented in
pure Rust by RustCrypto. A random 96-bit nonce is safe here for two reasons:
each lock has a fresh 32-byte key because each lock has a fresh salt, so a
nonce collision across notes is harmless; and a re-lock or save always
draws a new salt and nonce rather than reusing the pair. The 128-bit tag
authenticates every byte of ciphertext; truncation and bit flips fail closed.

**Alternative: XChaCha20-Poly1305.** A 192-bit nonce and pure-software speed
make it attractive on targets without AES acceleration. It is not needed for
desktop notes, and adding both ciphers would double the envelope’s test
surface. It is the natural choice for a future version if AES-GCM is ever
unavailable on a supported target.

### Dependencies

Added to `crates/muninn-core/Cargo.toml`:

| Crate | Version | Why |
| --- | --- | --- |
| `argon2` | 0.6 | RustCrypto Argon2id, `hash_password_into` path, no PHC string needed. `alloc` + `zeroize`; the default `password-hash`/`getrandom` features are off because the module derives a raw key. |
| `aes-gcm` | 0.11 | RustCrypto AES-256-GCM with `aes`, `alloc` and `zeroize` features. |
| `base64` | 0.23 | Base64url without padding. The workspace already uses this version. |
| `getrandom` | 0.4 | OS random bytes for the salt and nonce. The workspace already uses this version. |
| `zeroize` | 1 | Clears derived keys and decrypted buffers on drop, best effort. The workspace already uses this version. |

All are pure Rust. They share the RustCrypto stack the workspace already uses
for `sha1`/`sha2` (see `crates/muninn-sync/Cargo.toml`), so there is no C
toolchain or vendored native code in the build. The crates contain `unsafe`
internally, which is allowed: the workspace lint `unsafe_code = "forbid"`
applies to Muninn's own code, and this module contains none. Dependabot's
single `cargo` group in `.github/dependabot.yml` covers these crates.

### Key and password handling

- `lock` and `unlock` are pure: no file I/O, no globals, no session state.
- The password is used as UTF-8 bytes exactly as typed. It is not normalized
  (a future decision, noted below), trimmed or stored by this module.
- The derived key lives in a `Zeroizing<[u8; 32]>`; the decrypted buffer is a
  `Zeroizing<Vec<u8>>` before it becomes the returned `String`. Clearing is
  best effort because the returned `String` and the caller's editor buffer are
  outside this module.
- An empty password is refused (`EmptyPassword`) instead of producing a lock
  anyone can open.
- No salt, nonce, key, password or plaintext is ever written to
  `settings.json`, the OS keychain, a log or a sync journal. The only output
  is the envelope string.

## Integration map

Each acceptance criterion from #113 lands in these places. Nothing below is
implemented in this slice except the envelope itself.

### 1. A locked note cannot be read in the editor, Read view, search or graph

- **`crates/muninn-ui/src/state.rs::load_note_at`** currently clones
  `note.source` straight into `editor.replace_text`. It must check
  `note.is_locked()` first: show a locked placeholder page (title, lock
  icon, Unlock button) and leave the editor buffer empty or on a read-only
  placeholder. Autosave must not run on that placeholder.
- **Read view** shares the editor buffer (`Mode::Editor` / `Mode::Read`), so
  the same gate covers it. `preview.rs::Vault::preview` must never be called
  with an envelope; the gate is before it.
- **Search** (`crates/muninn-core/src/search.rs::Vault::search`) must match
  only `Note.title` for a locked note and must not add `note.source` (the
  envelope) to the haystack either, so a query cannot hit base64 noise.
- **Snippets** (`Vault::snippet`) and the reader attachment strip read
  `source`; a locked note has no reader view, so the UI never asks for them,
  but `snippet` should return an empty/placeholder excerpt for a locked note
  as a second guard.
- **Graph text** (`crates/muninn-graph/src/layout.rs::sync`) copies only
  `title`, `rel` and indexed `links`. A locked note still appears as a node
  with its title and no outgoing edges until it is unlocked in the session.

### 2. The file on disk is ciphertext in a documented UTF-8 envelope

- `note_lock::lock` produces the envelope; `Vault::write_note` /
  `atomic_write` write it as one atomic replace, exactly where the note used
  to be. No plaintext sidecar, no `.bak`, no second extension.
- The walk already reads it: add a `Note.locked` flag (cached from
  `is_locked(&source)` in `note_from_path`, like the cached `kanban` flag) and
  a `Note::is_locked()` accessor. Keep `Note.source` as the envelope; that
  way a locked note never keeps plaintext in the vault’s memory model.

### 3. Sync uploads the envelope bytes

`crates/muninn-sync` needs no provider change:

- `crates/muninn-sync/src/inventory.rs` walks every included file, calls
  `dir.read_file`, hashes those exact bytes (`ContentHash::of`) and records
  size and kind. For a locked note that is the envelope.
- `crates/muninn-sync/src/engine.rs` stages those bytes as `local_bytes` and
  `apply_desired` writes the chosen bytes; `drive.rs` uploads the same bytes
  as a content-addressed blob. GitHub and Tailscale already receive the same
  file bytes.
- The follow-up integration test should lock a note, run the inventory on a
  fixture vault, and assert the snapshot hash equals the hash of the envelope
  and that the blob bytes unlock with the note password. The existing
  `provider_tests` fixtures can host that.

**Conflicts.** `docs/sync-architecture.md` already resolves a same-path
disagreement by keeping both copies: this device keeps the path, the other
side becomes `<stem> (remote).md` locally (or `<stem> (conflict <last 8 of
commit id>).md` when Drive merges two heads). That rule needs no special
case: each copy is an independent envelope. If both devices used the same
note password, both copies open with it; if they used different passwords,
each opens only with its own. Sync never merges plaintext and never decrypts.

**Lost password.** The remote copy is no help: there is no server-side key,
no reset, and Muninn never uploaded the password or key. A user who loses the
password loses both the local and remote copies.

### 4. The sidebar shows a locked note without its body

- `Note::is_locked()` drives a small lock glyph and the title in
  `crates/muninn-ui/src/view.rs`’s note row. The row already gets the title
  from the filename, so nothing else is needed.
- The note remains selectable. Selecting it shows the locked placeholder from
  item 1, not an editor.

### 5. `reindex` / `search` do not retain the body of a locked note

- `Vault::reindex` (`crates/muninn-core/src/vault.rs`) currently calls
  `is_kanban(&note.source)`, `note_tags(&note.source)` and
  `note_outgoing` (which runs `parse_links`) for every note. Add an early
  `if note.locked` guard: no kanban detection, no tags, no outgoing links.
  Other notes’ backlinks to it still resolve by title/path, which is the
  desired “target stays visible” behavior. The tag index therefore omits the
  note, and `notes_with_tag` does not list it.
- `Vault::search` matches title only, as in item 1.
- In-session unlock may populate the index until the note is re-locked or
  Muninn exits. Re-locking calls `reindex` again and drops those derived
  entries; the plaintext buffer is cleared from `State`. This is best effort
  inside a live process, as the threat model says.

### 6. Locking warns that a lost password is unrecoverable

- A confirm step before the first lock must say: there is no reset, no
  recovery key and no Muninn account; a lost password loses the note; and
  earlier backups/sync history may still contain the plaintext.
- There is no “forgot password” path anywhere in the UI or sync code.

### 7. Wrong password fails closed

- `note_lock::unlock` returns `LockError::AuthenticationFailed` for both a
  wrong password and a tampered envelope. The UI keeps the note locked, shows
  the error inline, and never displays a partial buffer. The password field is
  cleared after a failed attempt.
- A malformed envelope and an unsupported future version are distinct errors
  so the UI can say “this note was made by a newer Muninn” instead of
  “wrong password”, but neither exposes plaintext.

### 8. Tests and security documentation

The envelope tests live in `note_lock.rs` (round-trip, wrong password,
tampered payload/header, fresh salt/nonce, UTF-8 magic, no plaintext in
ciphertext, malformed/unsupported input, empty password). The follow-up
slice adds vault-level tests (locked note omitted from search, tags and
links), a sync fixture proving only envelope bytes are uploaded, and updates
`SECURITY.md` and `PRIVACY.md` for the new threat model. Those docs are not
changed in this slice because the feature is not user-visible yet.

### 9. Help and CHANGELOG

The Help page and the user-facing CHANGELOG entry ship with the UI wiring.
This slice only records the design + prototype under `[Unreleased]`.

### UI entry points

- **Note context menu** (`crates/muninn-ui/src/view.rs::context_menu`,
  `MenuTarget::Note`): add `("Lock note…", "lock")` when the note is
  unlocked and `("Unlock note…", "unlock")` when it is locked, next to
  Rename/Move/Delete. `crates/muninn-ui/src/state.rs::pick_menu` handles the
  two actions and opens a password prompt.
- **Command palette** (`crates/muninn-ui/src/overlay.rs::PaletteAction`): add
  `LockNote` / `UnlockNote` with labels, hints and keywords. They are
  note-specific, so follow `ConvertToKanban`’s pattern: add them to the
  palette only while a note is selected (the list is built in
  `state.rs::palette_items`), not to `PaletteAction::ALL`.
- **Password prompt** is a new dismissible layer: extend
  `overlay.rs::Layer`, `State::topmost_layer` and `State::dismiss_layer` so
  Escape closes it, and follow the existing inline prompt/modal pattern
  (`rename_prompt`, `delete_confirm`) for focus handling.
- **Session state** lives in `State`, never in `Settings`. The simplest v1 is
  a `Zeroizing<String>` password per unlocked note, cleared when the note is
  re-locked, another vault is opened or the app quits; it is never serialized.
  Keeping a derived key instead would avoid retaining the password but needs
  a reseal API and a new nonce per save (see open questions). Settings gets no
  new tab in v1.
- **No new Settings tab.** An eventual “remember for this device” would be an
  off-by-default checkbox backed by the `keyring` dependency
  `crates/muninn-sync` already uses, not a `settings.json` field.

### Autosave, `⌘S`, recovery and conflicts

- `crates/muninn-ui/src/state.rs::save_now` writes `self.editor.text` through
  `Vault::write_note`. For an unlocked note the buffer is plaintext; every
  save must re-run `note_lock::lock(&text, password)` (fresh salt and nonce)
  and write the envelope, never the buffer. The in-memory `saved` copy can
  stay plaintext so dirty detection keeps working, but `Note.source` and the
  disk file are the envelope.
- **Recovery is a plaintext sidecar today.**
  `crates/muninn-ui/src/recovery.rs` writes dirty editor buffers as `.txt`
  under `$XDG_STATE_HOME/muninn/recovery/`. For a locked note that would leak
  the body outside the vault. The follow-up must either store a `lock`ed
  envelope in that `.txt` (and ask for the password on restore) or suppress
  recovery for locked notes and say so in the UI. Do not ship the lock UI
  before this is decided.
- A sync conflict on a locked note follows the “keep both” rule above. The
  editor may show both files; each asks for its own password.
- `Vault::write_note_if_unmodified` still compares `disk_state` (mtime and
  length) before writing. A lock or unlock is just a write through the same
  path; the expected state is refreshed after it.

### Kanban and Board

- A locked markdown note is never a kanban: `is_kanban` on the envelope is
  false, and `state.rs::open_kanban` / `refresh_kanban` must refuse to parse
  it. There is no plaintext export, no card cache entry, and `Kanban::parse`
  never sees the envelope.
- A `.excalidraw` board is a separate file type and is not affected by this
  feature; “lock the note next to a board” only locks the note.

## Open questions and deferred work

1. **Session cache shape and lifetime.** Hold the password
   (`Zeroizing<String>`) and re-lock on each save, or add a `SessionKey` API
   that keeps only the derived key and reseals with a fresh nonce. The latter
   avoids retaining the password but adds API surface and must never reuse a
   nonce. Lock-on-blur, an idle timeout, and clearing on vault switch are UX
   decisions for the follow-up.
2. **Recovery copies.** Encrypt them with the same session key or suppress
   them. This must be settled before users can edit a locked note.
3. **Note identity in the AAD.** Binding the vault-relative stem would stop
   envelope substitution but makes rename require a re-encrypt (only possible
   while unlocked). Decide whether that cost is worth it.
4. **Rollback.** A monotonic per-note version authenticated in the envelope
   would detect restoring an older ciphertext; sync’s snapshots are not
   ordered that way today.
5. **Pathname metadata.** Filenames remain visible by design. If a user wants
   the title hidden too, that is a separate feature (rename to an opaque id
   plus an encrypted title field) and would break wikilink targets.
6. **Password normalization.** v1 uses exact UTF-8 bytes. A future version
   might normalize NFC so a password typed on different keyboard layouts
   matches; that must be a versioned change, because it changes the derived
   key.
7. **Argon2 parameter tuning.** 19 MiB / 2 iterations is a desktop baseline.
   Low-memory targets may need a documented lower cost, which means a new
   envelope version or a parameter field that readers bound-check.
8. **“Remember for this device.”** Off by default, backed by the OS
   credential store, and never in `settings.json`. Not v1.
9. **Recovery key.** If added, it must be generated at lock time, shown once,
   never stored by Muninn, and must wrap the same data key (which implies a
   two-level key design, not a second password).
10. **Multiple recipients / sharing.** Out of scope; the envelope has one
    password.
11. **Compression.** Compressing before encryption would reduce the size
    leak for repetitive notes but adds a parser and a side channel of its own.
    Not planned for v1.
12. **Unlock and index timing.** Whether unlocking immediately reindexes the
    whole vault or incrementally adds the note’s tags and links is an
    implementation detail for the follow-up; the security rule is only that
    re-locking removes them again.

## Prototype API and tests

`crates/muninn-core/src/note_lock.rs` exports:

- `lock(source: &str, password: &str) -> Result<String, LockError>` — writes a
  fresh salt and nonce; fails only on an empty password or an unavailable OS
  random source/KDF.
- `unlock(envelope: &str, password: &str) -> Result<String, LockError>` —
  parses, derives, authenticates, and only then returns the text.
- `is_locked(source: &str) -> bool` — cheap magic/version check for the walk,
  sidebar, search and editor.
- `LockError` — `EmptyPassword`, `NotLocked`, `UnsupportedVersion`,
  `Malformed`, `AuthenticationFailed`, `Crypto`.
- `LOCK_MAGIC`, `LOCK_VERSION`.

The tests cover the round trip (including non-ASCII text), wrong password,
a flipped payload byte, a flipped salt byte, fresh salt/nonce/ciphertext
across two locks, ASCII/UTF-8 and magic, no plaintext in the envelope or the
raw ciphertext, malformed headers, an unsupported version, a CRLF-rewritten
envelope, the vault walk still listing the file as a note, and empty
passwords.
