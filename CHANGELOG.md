# Changelog

All notable changes to Muninn are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Note and folder CRUD in the sidebar: name-first note creation (no silent `Untitled`),
  inline rename, delete with confirmation, move-to-folder, new/rename/delete folders, and
  a right-click context menu on notes and folders. `F2` renames the selected note and
  `Delete` asks to delete it. Renaming and moving now offer to rewrite inbound
  `[[wikilinks]]` (SAF-06, below); choosing to leave them keeps the old documented
  resolution rule.
- Wikilink completion (EDT-03): typing an unclosed `[[` opens a list of up to eight
  matching notes ranked by title/stem prefix, with duplicate titles disambiguated by
  folder. Up/Down move the highlight, Enter/Tab insert the full `[[stem]]`, Escape
  dismisses, and a pointer click accepts. Completion ignores inline code and fenced
  blocks. `Cmd/Ctrl+P` opens the same insert-link picker without typing brackets — it
  inserts links, not feed search; `Cmd/Ctrl+K` is still the search field.
- Heading deep links (EDT-04): `[[Note#Heading]]` and `note:stem#Heading` resolve the
  note and then place the caret on the heading line in Write or scroll the reader to it
  in Read. Matching is case-insensitive and heading decoration is not shown; a missing
  heading opens the note at the top instead of erroring.
- Broken-links dashboard (NAV-02): a sidebar chip shows the number of unresolved targets
  and opens a panel listing each target with its referrers. Clicking a target creates it
  in the right folder; clicking a referrer opens that note. The list is cached and
  rebuilt by `Vault::reindex`, and the empty state says "All links resolve".
- Rename/move link impact (SAF-06): renaming or moving a note with inbound links asks
  first, showing the referrer count and a **Rewrite links to new name** toggle (on by
  default). Rewriting replaces each inbound target while preserving `|alias` and
  `#heading`, uses the existing atomic note write per file, and only rewrites a bare
  duplicate title when it resolved to this note before the change. Per-file failures are
  reported without rolling back the rename or move; folder renames leave inbound links
  alone for now (follow-up).
- Atomic note writes: saves and note creation write a hidden, uniquely named temp file in
  the note's folder, flush and fsync it, then rename it into place, followed by a
  best-effort directory fsync on Unix. A failed or interrupted save leaves the previous
  note intact instead of truncated. `Vault::disk_state` exposes a note's on-disk mtime and
  length as the prerequisite for conflict detection (SAF-02).
- Vault-local trash (SAF-04): deleting a note moves its file to
  `.muninn-trash/<unix timestamp>/<original vault-relative path>` with one atomic rename
  (dot-directories are never scanned). The status bar offers **Undo** to rename the file
  back without overwriting an existing note; the toast expires on the next note action.
  Folder deletes stay permanent and empty-only.
- Startup scan failures are visible (SAF-07): a root that exists but is not a readable
  folder shows its OS error with **Retry** instead of an empty "No notes yet" screen, and
  unreadable or invalid-UTF-8 `.md` files are counted in the status bar as "N files
  skipped" while the rest of the vault loads.
- Crash recovery (SAF-05): only dirty editor buffers are copied to
  `$XDG_STATE_HOME/muninn/recovery/`, atomically and after a short idle, so a `kill -9`
  before autosave still recovers the typed text. The next launch shows a banner with
  per-note **Restore**, **Restore all** and **Dismiss**; a clean save removes the copy.
  Recovery state never lives inside the vault.
- External changes are watched without a new dependency (SAF-03): `poll` compares a
  metadata-only vault snapshot every 1.5 s and reports new, deleted, renamed or edited
  notes within about two seconds. A clean vault reloads in place (selection, mode, scope
  and collapsed folders preserved); an external edit to the dirty open note raises the
  existing conflict bar and never clobbers the buffer.
- Find in the open note (EDT-01): `Cmd/Ctrl+F` opens a bar with a live match count
  (`3/12`). Enter and Shift+Enter step forward and backward with wrapping, the match is
  selected in the editor, and Escape closes the bar and returns typing to the note.
  Matching is case-insensitive on Unicode without changing the note's text.
- Keyboard search results (NAV-01): Up/Down move a highlighted row through the ranked
  results with wrapping, and Enter opens the highlighted note instead of always the
  first. Changing the query clears the highlight; title-first ranking is unchanged.
- Back/Forward note history (NAV-03): the topbar chips and `Cmd/Ctrl+[` / `Cmd/Ctrl+]`
  revisit notes by path, keep the caret per entry, drop the abandoned forward branch on
  a new open, and cap the list at 100 entries. M2 deliberately chose history-only over
  tabs so there is still one editor, one dirty buffer and one focus owner.

### Fixed

- Escape in the editor now clears its focus flag in the same key event (EDT-06), so `F2`
  and `Delete` work immediately afterwards instead of staying disabled until the next
  frame. Switching modes also clears stale field focus.

### Changed

- Saves are conflict-safe (SAF-02): autosave compares the note's on-disk `(mtime, length)`
  with the identity captured at load/save and refuses to overwrite an external change or
  deletion. The editor keeps the dirty buffer and the status bar offers **Keep mine**,
  **Load theirs** and **Save copy** (`Title (conflict YYYYMMDD-HHMMSS).md` in the same
  folder). Autosave pauses while a conflict is pending; IO failures stay visible with a
  **Retry** button instead of looping silently (TRU-01).

## [0.2.0] - 2026-11-07

### Added

- Workspace layout: `muninn-core`, `muninn-graph`, `muninn-ui`, `muninn-app` and `xtask`.
- `cargo run -p xtask -- check`, `layers`, `bundle` and `snapshots` tasks.
- GitHub CI, release workflow, issue templates and Dependabot configuration.
- `scripts/use-local-rune.sh` for development against a local Rune checkout.
- Folder-aware sidebar: notes grouped by folder with collapsing sections; duplicates show their
  vault-relative path, and new notes can target the current or a chosen folder.
- Documented wikilink resolution preferring path-qualified targets, then unambiguous titles,
  then a deterministic smallest-path choice for duplicates.
- App logo (`assets/logo.svg`): README header, sidebar brand mark, desktop entry and release
  assets.

### Changed

- Renamed the proof-of-concept app from `notes-graph` / “Rune Notes” to Muninn.
- Pinned Rune to an exact git revision instead of a sibling path dependency.
- Default vault is now `$MUNINN_VAULT`, `--vault`, or `~/Muninn`; the sample vault is opt-in.
- Split the vault parser, layout engine and UI into layered crates.
- Graph wheel: plain scroll zooms at the pointer and `Shift+scroll` pans, matching the hint.
- `poll` reports a view change only when a save completed, a node moved or the camera fit
  moved the view, ending spurious “returned true but nothing changed” warnings.
- Global graph now shows dot markers below zoom `0.6` (labels kept for the selection and its
  neighbours) and culls off-screen nodes and edges; layout skips repulsion beyond 800 units.
  Local graph behaviour is unchanged.

## [0.1.0] - 2026-11-07

### Added

- Write, Read and Graph modes; autosave; searchable note list; backlinks rail.
- Global and local graph views with deterministic force layout, pan, zoom and node dragging.
- Wikilinks with aliases and headings, ignoring code spans and fenced blocks.
- Click-to-create for unresolved links.
