# Sync architecture

Muninn sync is manual, one provider per vault, and one active writer per vault.
The note files remain the source of truth. The sync journal, checkpoints, and
last converged snapshot live under the XDG state directory, outside the vault.
They are bookkeeping, not a second copy of the user's notes as the canonical store.

## Providers

| Provider | What is stored remotely | Concurrency |
| --- | --- | --- |
| GitHub | Ordinary file bytes inside an explicit repository, branch, and prefix. Other paths in the repository are preserved with `base_tree`. | One commit whose parent is the commit just read, then `git/refs` update with `force: false`. A rejected update is re-read and replanned. |
| Tailscale | Immutable blobs and commit manifests (the Drive commit format) in a store directory on the host device, served on `127.0.0.1` and published to the tailnet by `tailscale serve`. | The host accepts a commit only when its parents equal the current heads. A stale peer gets 409, re-reads, and replans. Auth is a shared key from the credential store plus an optional `Tailscale-User-Login` allow-list. |
| Google Drive | Immutable blobs and immutable commit manifests in an app-created folder inside the folder the user selected. | No mutable head file. Two writers create two commits with the same parent. The next sync merges those heads or stops on a conflict. |

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

## Credentials

Tokens live in the OS credential store (`keyring` v1: macOS Keychain, Windows
Credential Manager, Linux Secret Service). If that store is missing, locked, or
refuses the request, sync stops. There is no plaintext file fallback. Settings
store the public client id, the binding id, and the remote scope only.

Disconnect deletes that binding's credential and the settings entry. It does
not delete local notes, remote notes, or checkpoints.
