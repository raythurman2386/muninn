# Sync architecture

Muninn sync is manual, one provider per vault, and one active writer per vault.
The note files remain the source of truth. The sync journal, checkpoints, and
last converged snapshot live under the XDG state directory, outside the vault.
They are bookkeeping, not a second copy of the user's notes as the canonical store.

## Providers

| Provider | What is stored remotely | Concurrency |
| --- | --- | --- |
| GitHub | Ordinary file bytes inside an explicit repository, branch, and prefix. Other paths in the repository are preserved with `base_tree`. | One recursive tree read per round; blobs already known by sha are not downloaded again. The new commit's parent is exactly the commit the plan read and its tree builds on that commit's tree; `git/refs` is updated with `force: false`. A 422 non-fast-forward is never re-parented: the engine re-reads and replans (up to four rounds). |
| Tailscale | Immutable blobs and commit manifests (the Drive commit format) in a store directory on the host device, served on `127.0.0.1` and published to the tailnet by `tailscale serve`. | The host accepts a commit only when its parents equal the current heads, and publish names exactly the heads the plan read. A stale peer gets 409 and the engine replans. Auth is a shared key from the credential store plus an optional `Tailscale-User-Login` allow-list. |
| Google Drive | Immutable blobs and immutable commit manifests in the app's hidden `appDataFolder` (`drive.appdata` scope), under `muninn-sync-<store name>`. | No mutable head file and no compare-and-swap. Publish names exactly the heads the plan merged. After uploading blobs and right before the manifest it re-lists the commits and refuses if any appeared since the read, so the engine replans. A writer that slips in after that last check becomes a second head; the next read merges it. Heads that edit the same path are merged "keep both": the first head by id keeps the path, the other's version is written beside it as `<stem> (conflict <last 8 of its commit id>)<ext>`, and the merge commit names every head, so the store converges to one head on the next sync (including stores an older build left with conflicting heads). A criss-cross history (two merges of the same heads) merges without a base, keeping both versions of anything that differs. Two roots created at once are read as one store. |

Drive is not a mirror of an editable Markdown folder. Drive v3 uploads are not
a compare-and-swap. This crate does not call `files.update` to overwrite note
bytes.

## Inclusion contract

Included:

- regular files, including `.md` notes and other ordinary attachments
- portable relative paths, at most 12 segments and 240 bytes
- at most 20 MiB per file, 200 MiB total, and 5,000 files

Excluded and reported, not silently treated as success:

- hidden files and directories (`.git`, `.muninn-trash`, and other dot paths)
- temporary names (`*~`, `*.tmp`, `*.temp`)
- `Thumbs.db` and `desktop.ini`

Hard errors, and the run does not report success:

- symlinks, reparse points, and special files
- hard links (on Unix, where the count is cheap to read from the metadata;
  on Windows the count API is unavailable, so a hard link is only rejected
  when the entry is not a regular file — see `fsutil`)
- paths that escape the vault, including `..`, absolute paths, and Windows device names
- a file or vault over the size limits
- a remote listing that is truncated, incomplete, or missing a parent commit

Empty directories are not represented. Executable bits are not part of the
logical content. Wikilinks are not rewritten on sync.

## What success means

A successful run means the included files match one published snapshot and the
local journal for that run is complete. It does not mean a single filesystem
transaction, real-time collaborative editing, or that a program outside Muninn
was frozen for the whole time.

The first bind has no deletion authority: files that exist on only one side are
copied, and a same-path disagreement is a conflict. Later deletes are explicit
tombstones. A missing page, a 403, or a 404 is not a delete.

A conflict between this device and the remote (the same note edited on both
since the last sync, an edit against a delete, a rename against an edit, or the
same new path on both) does not stop sync. The app resolves every conflict as
"keep both" on every provider: this device's version keeps the path, and the
remote version, if there is one, is written beside it as `<stem> (remote)<ext>`
(`(remote 2)` and so on if that name is taken). The status line names each copy.
Changes outside the conflicting paths in the same run (one-sided edits,
deletes, renames) apply as usual. A path that still exists with new bytes is
never read as the source of a rename, so a copy holding the old bytes is an add,
not a move. Per-file Keep local / Use remote choices are not in the app yet.

## Credentials

Tokens live in the OS credential store (`keyring` v1: macOS Keychain, Windows
Credential Manager, Linux Secret Service). If that store is missing, locked, or
refuses the request, sync stops. There is no plaintext file fallback. Settings
store the public client id, the binding id, and the remote scope only.

Access tokens carry an absolute expiry. A run refreshes a token within five
minutes of expiry, and refreshes once after a 401; the new pair is stored
before it is used. Sign-in runs on a worker thread: GitHub device flow, or
Google OAuth with PKCE and a `127.0.0.1` loopback redirect, both in the system
browser (fixed program, URL as one argument, `https` only).

Disconnect deletes that binding's credential and the settings entry. It does
not delete local notes, remote notes, or checkpoints.
