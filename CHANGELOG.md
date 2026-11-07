# Changelog

All notable changes to Muninn are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Workspace layout: `muninn-core`, `muninn-graph`, `muninn-ui`, `muninn-app` and `xtask`.
- `cargo run -p xtask -- check`, `layers`, `bundle` and `snapshots` tasks.
- GitHub CI, release workflow, issue templates and Dependabot configuration.
- `scripts/use-local-rune.sh` for development against a local Rune checkout.

### Changed

- Renamed the proof-of-concept app from `notes-graph` / “Rune Notes” to Muninn.
- Pinned Rune to an exact git revision instead of a sibling path dependency.
- Default vault is now `$MUNINN_VAULT`, `--vault`, or `~/Muninn`; the sample vault is opt-in.
- Split the vault parser, layout engine and UI into layered crates.

## [0.1.0] - 2026-11-07

### Added

- Write, Read and Graph modes; autosave; searchable note list; backlinks rail.
- Global and local graph views with deterministic force layout, pan, zoom and node dragging.
- Wikilinks with aliases and headings, ignoring code spans and fenced blocks.
- Click-to-create for unresolved links.
