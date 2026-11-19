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
  still fails: the envelope authenticates the note id it was locked for (see
  “What v2 binds” below). The one exception is a sync conflict copy, which is
  renamed and therefore keeps the original id; the UI must explain that case.

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

## Envelope format v2

The whole envelope is ASCII by construction. Any Unicode in the plaintext or
in the note id is encrypted or base64-encoded, so no non-ASCII byte reaches
the file.

```text
MUNINN-NOTE-LOCK v2
kdf=argon2id;m=19456;t=2;p=1;salt=<base64url, 16 bytes>
aead=aes-256-gcm;nonce=<base64url, 12 bytes>
note=<base64url of UTF-8 note id>
rev=<decimal, >= 1>

<base64url of ciphertext || 16-byte tag, wrapped at 76 columns>
```

The note id is the vault-relative path without `.md` (`Note::lock_id()`),
for example `Projects/Trip`. A concrete shape (values shortened for
readability):

```text
MUNINN-NOTE-LOCK v2
kdf=argon2id;m=19456;t=2;p=1;salt=3q2-7wXk9mQ1sT4uV8yB0A
aead=aes-256-gcm;nonce=Rk3n8Yq2Wm5Zp7Ld
note=UHJvamVjdHMvVHJpcA
rev=1

1v6Qm8sK2xT9pR4cW7yB3nH5jL0fD6gA1eZ8uI4oP2aS9dF3hJ6kM0nQ5rT7vX1yB4c
N8mZ2pR6tW9yA3dF7hJ1kL5qS0uX4cV8bN2mQ6rT9wZ3pD7gJ1kL5nS0uY4aC8e
```

### Fields

| Line | Field | v2 value | Rule |
| --- | --- | --- | --- |
| 1 | magic | `MUNINN-NOTE-LOCK` | First bytes of the file. Case-sensitive. |
| 1 | version | `v2` | Decimal, no leading sign. Only `2` is read. Unknown versions are detected as locked but not decrypted. |
| 2 | `kdf` | `argon2id` | Fixed in v2. |
| 2 | `m` | `19456` | Argon2 memory in KiB. Fixed in v2. |
| 2 | `t` | `2` | Argon2 iterations. Fixed in v2. |
| 2 | `p` | `1` | Argon2 lanes. Fixed in v2. |
| 2 | `salt` | 22 base64url chars | Exactly 16 random bytes, fresh for every `lock`. A `SessionKey` keeps its salt and draws only a new nonce per save. |
| 3 | `aead` | `aes-256-gcm` | Fixed in v2. |
| 3 | `nonce` | 16 base64url chars | Exactly 12 random bytes, fresh for every lock and every save. |
| 4 | `note` | base64url | Canonical unpadded base64url of the UTF-8 note id; non-empty. |
| 5 | `rev` | decimal | Monotonic revision, at least `1`. Canonical decimal: no `+`, no leading zero. |
| 6 | separator | empty line | One `\n` after the fifth header line. |
| 7+ | payload | base64url | AES-GCM ciphertext followed by the 16-byte tag. Wrapped at 76 columns; whitespace between characters is ignored when reading. |

### Associated data

All four field lines — `kdf=`, `aead=`, `note=`, `rev=` — in the exact
canonical form above, each including its trailing `\n` (including the `rev`
line that precedes the blank separator), are passed to AES-GCM as associated
data (AAD). The version is part of that header. Changing any parameter, the
salt, the nonce, the note id, the revision, or the version therefore fails
authentication exactly like changing the payload. The blank line and payload
wrapping are presentation only and are not authenticated.

### Parsing rules (v2)

A reader accepts an envelope only when all of these hold:

1. `LOCK_MAGIC`, a space, `v`, and a decimal version start the file.
2. The header has exactly four field lines, in the order above. No extra,
   missing or reordered fields. Numeric values must equal the v2 constants;
   a different `m`/`t`/`p` is `Malformed`, never a silent downgrade.
3. `salt` and `nonce` decode as unpadded base64url to exactly 16 and 12 bytes,
   and re-encode to the same text (canonical encoding only). A padded variant
   is rejected.
4. `note` decodes as unpadded base64url to non-empty valid UTF-8, and
   re-encoding those bytes reproduces the field exactly. An empty, padded or
   invalid-UTF-8 note id is `Malformed`.
5. `rev` is digits only, at least 1, and `rev.to_string()` equals the field,
   so `01` is `Malformed` rather than a second spelling of 1.
6. The payload contains only base64url characters and ASCII whitespace,
   decodes successfully, and is at least the 16-byte tag.
7. The password is not empty.
8. The decoded note id equals the note id the caller read the file from
   (`Note::lock_id()`). A mismatch is `WrongNote`, returned *before* key
   derivation.
9. The AEAD tag verifies against the canonical header as AAD.

A file that fails 1 is ordinary text (`NotLocked`). A recognized magic with an
unknown version is `UnsupportedVersion` and stays visibly locked. All
authentication failures — wrong password or altered bytes — are the single
`AuthenticationFailed` error, so the UI cannot become a password oracle.
`WrongNote` is not an oracle either: it depends only on the envelope's public
note id and the filename the caller already knows, and it is checked without
deriving a key.

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

### What v2 binds

The envelope names the note it belongs to in `note=`, and that note id is part
of the AAD. `unlock` compares it with the id of the file it was read from
(`Note::lock_id()`) before deriving any key. Moving `Private.md`'s envelope
over `Public.md` therefore fails with `WrongNote` instead of decrypting under
the wrong title. Renaming a locked note must re-encrypt it while it is
unlocked (a later layer's job); that is the accepted cost of binding the
identity.

**Conflict-copy caveat.** Sync resolves a same-path disagreement by keeping
both files and renaming one to `<stem> (remote).md` (or `<stem> (conflict
<id>).md`). A copy renamed that way still authenticates the original note id,
so it will not open under its new name: the reader compares the envelope's
`note=Projects/Trip` with the copy's `Projects/Trip (remote)` and reports
`WrongNote`. This is not corruption, and nothing is lost — the bytes are the
same valid ciphertext. The UI must explain it and offer the two recoveries:
rename the copy back to the original note id (after moving the local file
aside) to unlock it, or unlock the original note, which still holds the last
local save. It must not offer a "force open under this name" path, because
that would discard the binding. A future layer may re-encrypt a conflict copy
with the session key while the original is unlocked.

The envelope also carries an authenticated monotonic revision, `rev`. Each
`lock` starts at 1 and each `SessionKey::seal` increments it, so a rollback to
an earlier envelope of the same note is detectable by comparing its `rev`
against a higher revision the caller has already seen. This layer provides the
counter only; the persistent high-water record per note is the next layer and
is *not* implemented here. Without that record a rollback is visible only
within one unlocked session (the `SessionKey` remembers its revision), not
across restarts. A rollback still cannot make an old envelope decrypt to new
content: the bytes are exactly as old as they are.

Moving files is outside the read-only threat model, but the identity binding
now makes the substitution attack fail even when the attacker can write.

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
| AAD | canonical four-line header (parameters, note id, revision) |

AES-GCM is authenticated, streams over the whole body without padding, has
hardware acceleration on the platforms Muninn targets, and is implemented in
pure Rust by RustCrypto. A random 96-bit nonce is safe here: a fresh `lock`
draws a new salt and therefore a new key, so a nonce collision across notes is
harmless; and a `SessionKey` keeps its key but draws a fresh nonce for every
`seal`, so the (key, nonce) pair is never reused. The 128-bit tag
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
  `unlock_session` additionally returns a `SessionKey`, which is the only
  state this module defines; the caller owns it and decides when it drops.
- `SessionKey` holds the note id, the salt, the derived key and the last
  revision. It never holds the password, it is not `Clone`, and it has no
  `Debug` that would print the key. Each `seal` draws a fresh nonce, reuses
  the salt, increments the revision, and only advances its own counter after
  the encryption succeeds. `rebind(note)` re-labels an unlocked session after
  a rename or move: the key and revision continue, and the next `seal` writes
  the new note id.
- The password is used as UTF-8 bytes exactly as typed. It is not normalized
  (a future decision, noted below), trimmed or stored by this module.
- The derived key lives in a `Zeroizing<[u8; 32]>`; the decrypted buffer is a
  `Zeroizing<Vec<u8>>` before it becomes the returned `String`. Clearing is
  best effort because the returned `String` and the caller's editor buffer are
  outside this module.
- An empty password is refused (`EmptyPassword`) instead of producing a lock
  anyone can open, and an empty note id is refused (`EmptyNote`) because it
  would authenticate no identity.
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

- `note_lock::lock(source, password, note.lock_id())` produces the envelope;
  `Vault::write_note` / `atomic_write` write it as one atomic replace, exactly
  where the note used to be. No plaintext sidecar, no `.bak`, no second
  extension.
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
commit id>).md` when Drive merges two heads). Sync never merges plaintext and
never decrypts. The copy is still a valid envelope, but because v2 binds the
note id it was locked for, it does **not** open under the renamed path
(`WrongNote`). The UI explains this: rename the copy back to the original
note id to unlock it, or unlock the original (which holds this device's last
save). Re-encrypting a conflict copy under the session key is a possible later
refinement; a forced open that ignores the binding is not.

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
- `LockError::WrongNote` is separate from both: the envelope is valid but was
  locked for a different note id (a moved file, or a renamed sync conflict
  copy). The UI explains how to recover rather than reporting a bad password.
- A malformed envelope and an unsupported future version are distinct errors
  so the UI can say “this note was made by a newer Muninn” instead of
  “wrong password”, but neither exposes plaintext.

### 8. Tests and security documentation

The envelope tests live in `note_lock.rs` (round-trip including a Unicode note
id, wrong password, wrong note and a substituted envelope, tampered
payload/header/revision, fresh salt/nonce, sealed revisions reusing the salt,
UTF-8 magic, no plaintext in ciphertext, malformed/non-canonical
note/revision input, a v1 envelope, empty password and empty note, `note_id`
stripping). The follow-up
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
- **Session state** lives in `State`, never in `Settings`. `unlock_session`
  already returns a `SessionKey` that keeps only the derived key, so the v1 UI
  stores one per unlocked note instead of the password. Clear it when the note
  is re-locked, another vault is opened or the app quits; it is never
  serialized and never written to the OS keychain. Settings gets no new tab in
  v1.
- **No new Settings tab.** An eventual “remember for this device” would be an
  off-by-default checkbox backed by the `keyring` dependency
  `crates/muninn-sync` already uses, not a `settings.json` field.

### Autosave, `⌘S`, recovery and conflicts

- `crates/muninn-ui/src/state.rs::save_now` writes `self.editor.text` through
  `Vault::write_note`. For an unlocked note the buffer is plaintext; every
  save seals it with the note's `SessionKey` (`seal(&text)` — same salt, fresh
  nonce, next revision) and writes the envelope, never the buffer. The
  in-memory `saved` copy can stay plaintext so dirty detection keeps working,
  but `Note.source` and the disk file are the envelope.
- **Recovery copies are suppressed for locked notes (v1).**
  `crates/muninn-ui/src/recovery.rs` writes dirty editor buffers as `.txt`
  under `$XDG_STATE_HOME/muninn/recovery/`, which for a locked note would leak
  the body outside the vault. A note whose indexed source is an envelope
  (`Note::is_locked`) never gets a copy in either slot (editor or kanban), and
  a copy left from before the note was locked is deleted when the vault is
  loaded (including when a chunked scan finishes), on an explicit vault switch,
  and when the selected note is or becomes locked. Unsaved edits of a locked
  note therefore rely on autosave; the UI says so in the lock layer's copy.
- A sync conflict on a locked note follows the “keep both” rule above. The
  editor may show both files; the original asks for its password, and the
  renamed `(remote)` copy reports `WrongNote` until the user renames it back
  to the original note id.
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

1. **Session cache shape and lifetime (API settled).** `SessionKey` now keeps
   only the derived key and seals with a fresh nonce, so the password is not
   retained. Lock-on-blur, an idle timeout, and clearing on vault switch are
   still UX decisions for the follow-up.
2. **Recovery copies (settled for v1).** Suppress them for locked notes in
   both slots, and clear a stale copy on load, on a vault switch and as soon as
   a note is seen locked; unsaved edits rely on autosave. Encrypting the copy
   with the session key remains a possible later refinement.
3. **Note identity in the AAD (settled).** Envelope v2 binds
   `Note::lock_id()` in the authenticated header and `unlock` rejects a
   mismatch as `WrongNote` before key derivation. A rename of a locked note
   therefore requires a re-encrypt while unlocked. The tradeoff is the sync
   conflict copy: renaming it to `<stem> (remote).md` leaves the original id
   inside, so it will not open under the new name until the user renames it
   back (or unlocks the original). The UI must explain that; see “What v2
   binds”.
4. **Rollback (format settled, storage deferred).** The envelope now carries
   an authenticated monotonic `rev`, incremented by every `SessionKey::seal`,
   so an older envelope is detectable by comparison with a higher revision the
   caller has seen. Recording that high-water mark per note across restarts is
   the next layer; this slice only provides and tests the counter. Sync’s
   snapshots are still not ordered by revision, so the record has to be kept
   locally (and merged like any other state).
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

- `note_id(rel: &str) -> &str` — one trailing `.md` stripped; the envelope's
  note identity. `Note::lock_id()` returns this for `Note.rel`.
- `lock(source: &str, password: &str, note: &str) -> Result<String, LockError>`
  — writes a fresh salt and nonce and `rev=1`; fails on an empty password, an
  empty note, or an unavailable OS random source/KDF.
- `unlock(envelope: &str, password: &str, note: &str) -> Result<String, LockError>`
  — parses, checks the note id (before deriving), derives, authenticates, and
  only then returns the text.
- `unlock_session(envelope: &str, password: &str, note: &str) ->
  Result<(SessionKey, String), LockError>` — the same checks plus a session
  key for later saves.
- `SessionKey` — `seal(&mut self, source: &str) -> Result<String, LockError>`,
  `rebind(&mut self, note: &str) -> Result<(), LockError>`, `rev()`, `note()`.
  Keeps the note id, salt, derived key and revision; never the password; not
  `Clone`; the key is zeroized on drop.
- `is_locked(source: &str) -> bool` — cheap magic/version check for the walk,
  sidebar, search and editor.
- `LockError` — `EmptyPassword`, `EmptyNote`, `NotLocked`,
  `UnsupportedVersion`, `Malformed`, `WrongNote`, `AuthenticationFailed`,
  `Crypto`.
- `LOCK_MAGIC`, `LOCK_VERSION`.

The tests cover the round trip (including a Unicode note id), wrong password,
wrong note and a substituted envelope, a flipped payload byte, a flipped salt
byte, a rewritten `rev`, fresh salt/nonce/ciphertext across two locks,
`SessionKey` sealing (fresh nonce, same salt, revisions 1 → 2 → 3, the newest
envelope reopens), ASCII/UTF-8 and magic, no plaintext in the envelope or the
raw ciphertext, malformed headers and non-canonical `note`/`rev`, a v1
envelope as `UnsupportedVersion`, a CRLF-rewritten envelope, `note_id` suffix
stripping, the vault walk still listing the file as a note, and empty
passwords and notes.
