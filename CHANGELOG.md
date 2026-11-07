# Changelog

All notable changes to Muninn are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Note and folder CRUD in the sidebar: name-first note creation (no silent `Untitled`),
  inline rename, delete with confirmation, move-to-folder, new/rename/delete folders, and
  a right-click context menu on notes and folders. `F2` renames the selected note and
  `Delete` asks to delete it. Renaming does not rewrite inbound `[[wikilinks]]`; stale
  bare targets keep resolving by the documented rule or become missing.

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
