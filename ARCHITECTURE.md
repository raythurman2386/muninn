# Architecture

Muninn is a small desktop app with a deliberately boring data model: **the file is the source of
truth**. There is no database, migration step or background index writer. The vault is scanned
when it is opened or reloaded, links are re-indexed after a save, and everything the UI shows is
derived from that scan.

## Crate graph

```text
muninn-app ──► muninn-ui ──► muninn-graph ──► muninn-core
     │              │
     │              └──► muninn-sync
     └──────────────┴──► rune, rune-kit, rune-theme-opencode
```

| Crate | May depend on | Why |
| --- | --- | --- |
| `muninn-core` | nothing in the workspace | Pure domain model; fast unit tests. Its only external dependency is `serde_json`, for board files. |
| `muninn-graph` | `muninn-core` | Needs note ids and links, nothing else. |
| `muninn-sync` | nothing in the workspace | Plans and transfers vault bytes. It does not read the UI or the link index. |
| `muninn-ui` | `muninn-core`, `muninn-graph`, `muninn-sync`, Rune | Owns state, rendering, input, and the manual sync session. |
| `muninn-app` | `muninn-ui`, Rune | Entry point only. It may also depend on `muninn-sync` for credential wiring. |
| `xtask` | tooling crates | Never linked into the app. |

`cargo run -p xtask -- layers` reads `cargo metadata` and rejects any new `muninn-*` edge that
is not in this table. That check is part of `xtask check` and CI.

The `muninn` binary's headless `--check` and `--list-links` paths read `muninn-core` directly
(and `--check` never creates a missing vault) before Rune starts a window or GPU context.

## Data flow

1. **Scan.** `muninn_core::Vault::load` walks the root (up to 12 levels, skipping dot-dirs),
   reads every `.md` file and records its title, vault-relative path, folder and source. The
   walk is a metadata pass; reading bodies is the expensive part. `Vault::scan_start` returns a
   `ScanJob` with the same sorted path/folder lists, `read_chunk(n)` reads a bounded number of
   files, and `finish` builds the vault. `Vault::load` is exactly `scan_start` + one unbounded
   chunk + `finish`, so both paths produce identical indexes. On startup the UI loads vaults of
   up to 100 files synchronously; above that `poll` advances 50 files per frame behind a
   progress bar, and Escape cancels without ever installing a partial vault.
2. **Index.** `Vault::reindex` resolves every `[[wikilink]]` through lower-cased
   path/title indexes and writes each note's `links` and `backlinks` lists. Both are
   unique and sorted. It also caches unresolved targets with their referrers
   (`Vault::broken_links`) so the dashboard never scans notes per frame. The sidebar
   groups notes by folder; duplicate titles are identified by their vault-relative path.
3. **Layout.** `muninn_graph::Graph::sync` mirrors the vault as nodes and undirected,
   deduplicated edges, keyed by `rel` so duplicates keep distinct nodes. `Graph::step`
   runs one deterministic force step and reports true only when a node actually moved;
   above 200 notes repulsion and separation use a uniform spatial grid with a
   density-tapered cutoff, while smaller vaults keep the exhaustive pass unchanged.
   Dense vaults seed their spiral scaled to fit the world, damp velocity a little harder,
   absorb separation corrections into velocity (position-based damping), and go dormant
   once heat is zero and no node moves faster than 0.4 pt/step. Dormancy freezes positions
   with a bounded sub-pixel residual, so `poll` can sleep; `reheat` (drag, scope/mode
   change, resync) resumes the simulation. A 2,000-note fixture goes quiet by frame ~2,600;
   the exhaustive ≤200-note path and the sample layout stay bit-identical.
4. **Render.** `muninn_ui::view` builds the Rune element tree from `State`. The graph camera
   publishes pan/zoom through `Live` values so a drag redraws without rebuilding the tree.
   Selecting a node or changing scope starts a spring camera glide held in `State` (never in
   `muninn-graph`, which stays Rune-free); `poll` steps it and asks for the next frame only
   while it is in flight, so reduced motion snaps and sleeps. `poll` reports true only when
   visible state changed (a save completed, a node moved, a glide moved the camera, or the
   camera fit moved the view), never just because the editor is dirty or heat remains.
   The dismissal order for overlaid surfaces is one enum: `Layer` /
   `State::topmost_layer` / `State::dismiss_layer` (see
   [state-machines.md](docs/state-machines.md) for the full inventory,
   including `Mode`, `Scope`, and `ThemeMode`). Overlay types and mode
   transitions live in `overlay.rs` / `mode.rs` so the dismissal stack and
   the page machine stay near their owning modules instead of piling up
   inside `state.rs`; `State` itself keeps the fields both machines read.
5. **Save.** `muninn_ui::poll` notices an edited buffer, waits 0.55 s of idle, and writes the
   file only when its on-disk `(mtime, length)` still matches what the editor loaded. The write
   is atomic (a hidden, uniquely named temp file in the same folder is flushed and fsynced,
   then renamed over the old one) and re-indexes. An external change or deletion pauses
   autosave and raises a conflict (Keep mine / Load theirs / Save copy) instead of
   overwriting; a failed write becomes a status-bar error with Retry, never a silent change
   or a truncated file.
6. **Crash recovery (SAF-05).** While a buffer is dirty it is copied, after a 0.2 s idle
   (and at most one second apart during continuous typing), to the XDG state directory:
   `$XDG_STATE_HOME/muninn/recovery/<vault-id>~<sanitized-rel>.txt`, with a tiny
   `<vault-id>~<sanitized-rel>.json` sidecar holding the exact vault-relative path, a stable hash of
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
9. **Per-note undo (M2 EDT-02).** Rune's `TextEdit` owns its undo stack, and
   `replace_text` clears it when the app swaps notes. `State` therefore parks a clone of
   the whole `TextEdit` in a path-keyed, 24-entry cache whenever a note is left; cloning
   shares Rune's immutable history until the next edit. On revisit the snapshot is
   restored only while its text still equals the file the vault will show, so an
   external reload or conflict resolution discards stale history instead of undoing
   into discarded text. Rename and move re-key the open note's entry.

10. **Settings (M3 LIF-01).** UI preferences — theme, collapsed folders, last vault/note,
    mode/scope, recent-vault MRU and the autosave delay — are written as small JSON to
    `$XDG_CONFIG_HOME/muninn/settings.json` (fallback `~/.config/muninn/`) with the same
    temp-file + rename pattern as note saves; a corrupt file falls back to defaults. The
    startup vault scan is synchronous up to 100 files and chunked (see step 1) above that;
    `load_ms` covers the whole load in either case. Rune's `AppSpec` window size
    remains fixed at 1180×780. Window geometry persistence is **BLOCKED**, not
    merely deferred: Rune (rev `b39c24f`) exposes the setters — `AppSpec::size`
    (`crates/rune/src/tooling.rs:19`, applied as the inner size at
    `crates/rune/src/windows.rs:148`), `Tx::resize_window`
    (`crates/rune/src/el/tx.rs:255`), `WindowHandle::resize`
    (`crates/rune/src/window/handle.rs:84`) and `AppHandle::set_position`
    (`crates/rune/src/app.rs:579`) — but no getter for the live window's size or
    position on `AppHandle`, `WindowHandle` or `Cx`. `El::on_screen_layout`
    (`crates/rune/src/el/layout_watch.rs:63`) reports a content rect in display
    points (hidden on Wayland) and cannot be converted to the physical **outer**
    position those setters take. Rune's only persistence helper,
    `rune_mac::remember_frame` / `saved_size` (`crates/rune-mac/src/lib.rs:15,78`),
    uses AppKit's frame autosave and is a no-op elsewhere. `Settings` therefore
    has no window field: a value written from a guess could not be read back.

## Invariants

- **Note ids are indexes.** `Note::id` is always the position in `Vault::notes`. The graph uses
  the same ids for nodes and edges.
- **Links are resolved, not stored.** Changing a title re-resolves on the next `reindex`;
  the index is always derived from the files. Rename/move can additionally rewrite inbound
  wikilink source (SAF-06, per file and atomic, including folder renames), but that is a
  user-chosen file edit, not index state: the next scan would still work without it.
- **Determinism.** The initial graph layout depends only on note order and path hash, and the
  large-vault grid visits cells in a fixed order (never hash iteration). Tests compare settled
  positions exactly.
- **No side database for notes.** Note bodies stay files. Sync keeps a journal,
  checkpoints, and the last converged manifest under the XDG state directory,
  outside the vault. That journal is recovery bookkeeping for sync, not a
  second source of truth. Settings and tokens are not part of the synced
  content. Only `.md` files are notes. A note
  delete moves it to `.muninn-trash/<timestamp>/<original path>` (skipped by the scan) and
  Undo renames it back, never overwriting. The trash is budgeted, not a database: after each
  successful move it is pruned oldest-timestamp-first to at most 500 files or 100 MiB
  (`Vault::TRASH_MAX_ENTRIES` / `TRASH_MAX_BYTES`), and **Empty trash** renames the whole
  folder aside before deleting it, so a failure can never touch a live note. Pruning is
  best-effort and never fails the delete. A root that exists but is not a readable folder is
  an error, not an empty vault. The sample vault stays ordinary content used by tests and
  snapshots.
- **Attachments stay outside the index.** Only `.md` files are notes. An embed such as
  `![[pic.png]]` becomes a filename+size placeholder, and the reader lists attachment files
  with an Open action. Open/Reveal spawn the platform handler on a canonical path
  (`xdg-open`/`open`/`start`) because Rune's `Tx::open_url` refuses `file:`; drag-drop imports
  copy accepted files into the current folder with the same temp+rename rule and never
  overwrite an existing name.
- **Boards are files, not notes.** A whiteboard is an Excalidraw `.excalidraw` JSON file in
  the vault (`muninn_core::Board`). `Vault::boards` lists them with the note scan's own walk
  (same depth limit, dot-entries skipped) but never adds them to the note index, links or
  search. Loading keeps every field and element type Muninn does not model, so a save never
  drops what Excalidraw wrote, and saves use the same atomic temp+rename path as notes.
  Obsidian's `.excalidraw.md` files end in `.md` and stay notes.
  The Board canvas (`muninn-ui` `board_scene`, `board_camera`) draws a board read-only from
  the loaded file; its camera (an eye point and zoom in scene units, so a reload that
  changes the drawing's extent keeps the view) lives in the session and in
  `settings.json`, never in the board file.
  Drawing (`board_tools`) adds elements to the loaded `Board` through
  `Board::add_drawn` and redraws from the model. A board is written by an explicit save
  (`⌘S`), by autosave (`board_sync`: once the drawing has rested for the autosave delay,
  at least 1 s, or every 10 s while it never rests; never mid-gesture, with a text editor
  open, after a failed save or over a conflict; `"board_autosave": false` in
  `settings.json` turns it off), or by a flush where the drawing would otherwise be lost
  (picking another board, switching vault, a vault rescan, quitting). The board watcher
  (`board_sync`, every 1.5 s while Board is showing) compares each listed board's file
  modified time and length with the last look: a changed open board that is clean is
  re-read (keeping the view; Muninn's own writes are recognised and skipped), a dirty one
  is flagged as below and never dropped, and any other changed board's list entry is
  re-read. Each write is `Board::save` (atomic temp+rename) to the board's own path,
  refused unless that path is inside the vault the board was opened from *and* the
  currently open vault, and refused if the file's bytes no longer match what was loaded
  or last saved (changed or removed on disk). A refused flush keeps the drawing in memory
  and blocks the board switch, the vault switch or the vault rescan (Retry, or the
  chunked startup scan finishing), like a note that cannot be saved.
  Editing (`board_edit`, over `muninn_core` `board::edit`) selects, moves and deletes
  elements of the in-memory `Board` and keeps a bounded undo history (`History`, 100
  edits) per open board; every edit, undo and redo bumps the touched elements' Excalidraw
  `version`/`versionNonce`/`updated`, a delete is an `isDeleted` tombstone, and the history
  is dropped when the file changes underneath. The history is also bounded by size (32 MiB
  of changed elements, the oldest edits dropped first; the newest edit is always kept), so
  edits on a big drawing cannot grow memory without bound. Edits mark the board unsaved:
  they are written by the save or autosave above, and the recovery copy below is taken
  once the drawing rests (0.5 s, or every 2 s while it keeps changing; a move drag once it
  lands), not per frame or per held key. View changes (pan, zoom) write `settings.json`
  once they rest, too.
  Text (`board_text`, over `muninn_core` `board::text`) places and edits free-standing
  text elements through an in-canvas Rune text field; a commit (Esc, a click outside,
  focus loss, leaving Board, saving) is one history edit with the size measured by the
  canvas's own text layout. While the field is open `State::is_typing` is true, so Board
  and Graph keys never fire into it. A label bound to a shape (`containerId`) is edited by
  double-clicking the shape or the label and re-centred in the shape on commit; a label in
  a locked shape is locked with it. Text typed but not committed counts as unsaved, and
  the recovery copy is the model with that text applied (on a clone).
  Unsaved drawing is never dropped (`board_safety`): a file removed, renamed or changed
  outside Muninn is noticed by the board watcher while Board is showing, when the boards
  are re-listed (entering Board, a vault reload, a rescan) or when a save re-reads the
  file; then
  the board is kept open and flags it in the heading with the note conflict bar's choices:
  Keep mine (write to the board's own path, recreating a removed file, still only inside
  the open vault), Save copy (a new sibling `Title (conflict YYYYMMDD-HHMMSS).excalidraw`
  that becomes the open board) and Load theirs / Discard. While a board is dirty its drawing
  is also copied after each change to the SAF-05 recovery store notes use (keyed by the
  vault's id and its vault-relative path, outside the vault, same temp+fsync+rename), and the copy is removed
  once the board is saved or discarded; the launch banner restores it onto Board, never as
  a note, flagged the same way when the file is gone or now holds something else, and only
  for a plain relative path that stays inside the vault (`board_safety::inside_vault`, also
  checked by every board write). Closing or quitting saves the board, or else updates that copy; if neither is
  possible the quit is refused and the window comes back with the reason.
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
different one. Dot-directories stay ignored by the scan. Heading links
(`[[Note#Heading]]`) resolve the note and then put the Write caret on that heading line or
scroll the Read view to it, matching case-insensitively outside fenced blocks; a missing
heading opens the note at the top. Rename/move rewrites only targets that resolved to the
note before the change, so a bare duplicate title that resolved elsewhere is untouched.

## Scaling notes

- New non-UI behaviour belongs in `muninn-core` or `muninn-graph` with unit tests.
- New screens belong in `muninn-ui`; keep `muninn-app` to wiring.
- If a second front end appears (CLI, web, mobile), it should consume `muninn-core` and
  `muninn-graph` directly rather than growing `muninn-ui`.
- Add a crate only when it has a clear owner and dependency direction; update `allowed_edges`
  in `xtask/src/main.rs` at the same time.
