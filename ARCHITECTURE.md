# Architecture

Muninn is a small desktop app with a deliberately boring data model: **the file is the source of
truth**. There is no database, migration step or background index writer. The vault is scanned
when it is opened or reloaded, links are re-indexed after a save, and everything the UI shows is
derived from that scan.

## Crate graph

```text
muninn-app ──► muninn-ui ──► muninn-graph ──► muninn-core
     │              │
     └──────────────┴──► rune, rune-kit, rune-theme-opencode
```

| Crate | May depend on | Why |
| --- | --- | --- |
| `muninn-core` | nothing in the workspace | Pure domain model; fast unit tests. |
| `muninn-graph` | `muninn-core` | Needs note ids and links, nothing else. |
| `muninn-ui` | `muninn-core`, `muninn-graph`, Rune | Owns state, rendering and input. |
| `muninn-app` | `muninn-ui`, Rune | Entry point only. |
| `xtask` | tooling crates | Never linked into the app. |

`cargo run -p xtask -- layers` reads `cargo metadata` and rejects any new `muninn-*` edge that
is not in this table. That check is part of `xtask check` and CI.

## Data flow

1. **Scan.** `muninn_core::Vault::load` walks the root (up to 12 levels, skipping dot-dirs),
   reads every `.md` file and records its title, vault-relative path, folder and source.
2. **Index.** `Vault::reindex` resolves every `[[wikilink]]` through lower-cased
   path/title indexes and writes each note's `links` and `backlinks` lists. Both are
   unique and sorted. The sidebar groups notes by folder; duplicate titles are
   identified by their vault-relative path.
3. **Layout.** `muninn_graph::Graph::sync` mirrors the vault as nodes and undirected,
   deduplicated edges, keyed by `rel` so duplicates keep distinct nodes. `Graph::step`
   runs one deterministic force step and reports true only when a node actually moved.
4. **Render.** `muninn_ui::view` builds the Rune element tree from `State`. The graph camera
   publishes pan/zoom through `Live` values so a drag redraws without rebuilding the tree.
   `poll` reports true only when visible state changed (a save completed, a node moved,
   or the camera fit moved the view), never just because the editor is dirty or heat remains.
5. **Save.** `muninn_ui::poll` notices an edited buffer, waits 0.55 s of idle, and writes the
   file only when its on-disk `(mtime, length)` still matches what the editor loaded. The write
   is atomic (a hidden, uniquely named temp file in the same folder is flushed and fsynced,
   then renamed over the old one) and re-indexes. An external change or deletion pauses
   autosave and raises a conflict (Keep mine / Load theirs / Save copy) instead of
   overwriting; a failed write becomes a status-bar error with Retry, never a silent change
   or a truncated file.
6. **Crash recovery (SAF-05).** While a buffer is dirty it is copied, after a 0.2 s idle
   (and at most one second apart during continuous typing), to the XDG state directory:
   `$XDG_STATE_HOME/muninn/recovery/<sanitized-rel>.txt`, with a tiny
   `<sanitized-rel>.json` sidecar holding the exact vault-relative path, a stable hash of
   the canonical vault root and a Unix timestamp. The hash keeps two vaults that share a
   note name from listing each other's edits. The copy uses the same temp-file + fsync +
   rename pattern, so a `kill -9` leaves the newest complete text. Clean buffers are never
   written; saving, discarding or resolving a conflict to the incoming file removes the
   copy. On launch `State::new` scans that folder and shows a banner with the note paths
   plus Restore/Restore all/Dismiss. The vault itself is never used for recovery state.
7. **External changes (SAF-03).** Every 1.5 s `poll` takes a metadata-only snapshot of the
   vault (`fs::metadata` for each note and known folder; no file contents) and compares it
   with the previous one. Folder mtimes make new, renamed and deleted notes visible without
   a rescan. A change to a clean vault reloads it while preserving selection, mode, scope
   and collapsed folders. A change to the dirty open note raises the same typed conflict as
   a guarded save instead of touching the buffer; reloads are deferred until that buffer is
   clean. A snapshot failure is represented as a missing entry and stays quiet rather than
   reporting every tick.
8. **Navigation (M2).** Back/Forward is a path-keyed history deque, not a tab model: every
   `load_note` that changes the selection appends the note's path (consecutive repeats
   collapse, a new open truncates the abandoned forward branch, the list is capped at 100).
   Storing paths instead of note ids keeps history valid across a reload, where ids are
   rebuilt from the scan; each entry also keeps the caret to restore. In-note find matches
   are recomputed once a frame in `poll` from the query and editor text, because the view
   only borrows `State` and typing commits after the key handler runs.

## Invariants

- **Note ids are indexes.** `Note::id` is always the position in `Vault::notes`. The graph uses
  the same ids for nodes and edges.
- **Links are resolved, not stored.** Changing a title re-resolves on the next `reindex`; no
  cross-file state needs repairing.
- **Determinism.** The initial graph layout depends only on note order and path hash. Tests
  compare settled positions exactly.
- **No side database.** Only `.md` files are written, plus the hidden trash folder: a note
  delete moves it to `.muninn-trash/<timestamp>/<original path>` (skipped by the scan) and
  Undo renames it back, never overwriting. A root that exists but is not a readable folder is
  an error, not an empty vault. The sample vault stays ordinary content used by tests and
  snapshots.
- **Writes are atomic.** Every note save or creation writes a hidden, unique temp file in the
  note's folder, fsyncs it, then renames it into place; renames and moves use the filesystem's
  atomic rename. A crash or failed write leaves the previous bytes intact instead of a
  truncated file. Crash-recovery copies use the same pattern under the XDG state directory
  and are deleted once their buffer is saved, so the vault never accumulates recovery files.
- **Code is not a link.** Wikilinks inside inline code and fenced blocks are ignored by the
  parser and preserved by the preview rewriter.

## Parsing rules

`[[target]]`, `[[target|alias]]` and `[[target#heading]]` are supported. A target resolves
case-insensitively by:

1. **Path-qualified** (`[[folder/note]]`): exact vault-relative stem, then a unique
   `/`-suffixed tail. No match means missing — never a bare-title fallback.
2. **Unambiguous title** (`[[Title]]`): exactly one note has that title.
3. **Duplicate titles**: the deterministic smallest vault-relative path (case-insensitive,
   then case-sensitive). The preview always renders a note's full stem, so rendered links
   do not depend on this fallback.

Links to the current note are allowed in the source but do not produce self-edges in the graph.
Unresolved targets stay in the source, render faintly, and create a note when clicked —
path-qualified ones inside their folder, bare ones in the current note's folder.
New notes otherwise target the selected note's folder; each folder header's `+` chooses a
different one. Dot-directories stay ignored by the scan.

## Scaling notes

- New non-UI behaviour belongs in `muninn-core` or `muninn-graph` with unit tests.
- New screens belong in `muninn-ui`; keep `muninn-app` to wiring.
- If a second front end appears (CLI, web, mobile), it should consume `muninn-core` and
  `muninn-graph` directly rather than growing `muninn-ui`.
- Add a crate only when it has a clear owner and dependency direction; update `allowed_edges`
  in `xtask/src/main.rs` at the same time.
