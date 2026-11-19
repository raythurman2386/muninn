# Security

Muninn reads and writes Markdown files in a folder you choose. Sync, when you
explicitly run it, sends the included vault bytes to the provider you connected.
It does not run note content as a program. Tokens stay in the OS credential store.

## Reporting a vulnerability

Please report suspected vulnerabilities privately through GitHub's
[security advisories](https://github.com/raythurman2386/muninn/security/advisories/new) rather
than opening a public issue. Include the affected version or commit, the platform, and a minimal
reproduction. Do not include the contents of private notes.

If the advisory form is unavailable, open a minimal issue asking for a private contact channel
and omit exploit details.

## Per-note encryption

Any note can be locked with its own password. The file stays `<Title>.md` but its bytes become a
versioned envelope (`MUNINN-NOTE-LOCK v2`): a random 16-byte salt, the Argon2id parameters, the
note id, and a monotonic revision are authenticated as AES-256-GCM associated data, and the body is
the ciphertext. The key is derived from the password with Argon2id (19 MiB, 2 iterations) and never
leaves memory.

- **What is visible.** The filename and therefore the title, folder, file size (to within base64
  expansion), modification time and the fact that a note is locked. The body, frontmatter, tags and
  content-derived links are only inside the ciphertext.
- **No recovery.** There is no password reset, recovery key, escrow or Muninn account. A lost
  password loses the note, locally and in sync. The KDF is memory-hard, so an offline guess costs
  memory and time, but password strength still matters.
- **Crash recovery is off.** Muninn does not write a plaintext recovery copy for a locked note;
  unsaved edits rely on autosave, which writes a fresh envelope.
- **Sync uploads ciphertext.** The inventory hashes the envelope bytes and providers receive them
  unchanged. A sync conflict copy keeps the original note id and therefore reports `WrongNote` under
  the renamed path until it is renamed back; nothing is decrypted during a merge.
- **Local rollback record.** Each envelope carries a revision, and a device-local high-water mark
  (`$XDG_STATE_HOME/muninn/lock-revs/`) refuses to open an older revision than the device has seen.
  It is not synced, only ever raises, and a missing file starts empty rather than blocking the vault.
- **Rename and move.** The envelope binds the note id, so renaming or moving a locked note
  re-encrypts it and requires it to be unlocked in the session. Folder renames are refused while the
  folder holds a locked note.
- **Limitations.** Links inside an unlocked locked note are not rewritten when another note is
  renamed (a link rewrite writes a whole plaintext body), and a locked note cannot be a Kanban
  board. Both are documented in `docs/per-note-encryption.md`.

## Scope

Relevant areas include path handling for `--vault` and `MUNINN_VAULT`, note creation names,
Markdown link rewriting, and anything that could write outside the chosen vault root.

Dependency vulnerabilities should be reported upstream to
[Rune](https://github.com/anomalyco/rune) when the issue is in the framework.
