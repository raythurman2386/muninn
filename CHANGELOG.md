# Changelog

All notable changes to Muninn are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Default note template: Settings now has a wrapped multi-line **Default note
  template** field. Non-empty text is inserted verbatim at the top of every new
  note (a missing trailing newline is added), which makes YAML frontmatter easy
  to start with. An empty template keeps the original `# Title` body. The value
  is stored as `note_template` in `settings.json`, clamped to 4096 characters.

### Changed

- The sidebar vault chip row is gone; the sidebar is now just the top bar, search,
  New note and the note list. Vault management lives entirely in the Settings modal
  (`Cmd/Ctrl+,` or the gear): the **Vaults** section gained the typed-path field with
  **Open vault**, **New vault** and **Cancel** buttons, which close Settings on a
  successful switch, and the recent-vault list. The separate chip menu (RECENT section,
  recent rows and Cancel) and its `vault_menu` state were removed.

## [0.5.0] - 2026-11-08

### Added

- Settings modal (v0.5): `Cmd/Ctrl+,` or the new sidebar gear opens an in-window
  dialog with a segmented theme control (Omarchy / System / Dark / Light), an
  autosave-delay stepper clamped to 0.05–10 s, and the recent-vault list. Changes
  write through `settings.json` immediately; `Esc` closes the modal and returns
  typing focus to the editor or search field that opened it. While Omarchy is
  followed the dialog says so instead of offering a fixed theme, matching the
  hidden sidebar theme toggle. The shortcut is listed in the README and the Help
  overlay.
- Animated graph focus (v0.5): selecting a node glides the camera to it on a
  short spring instead of snapping, and neighbour dimming eases in on the same
  frame. **Enter** on the already-open focused note glides it back to the
  centre. Switching Global/Local eases the camera into the new fit and fades
  the nodes that enter or leave. Arrow focus and **Find node** keep the same
  glide when they bring an off-screen note into view.
  `MUNINN_REDUCED_MOTION=1` keeps the camera instant and starts no glide, so it
  never wakes the window for one.
- Command palette (`⌘⇧P` / `Ctrl+Shift+P`): one input searches notes
  (title-first, using the same ranking as the sidebar search) and actions
  together. Actions: Open local graph, Switch vault, Open settings, New note,
  Toggle Write / Read and Toggle Graph. With an empty input it lists the
  actions and recently opened notes only. Up/Down wrap the highlight, Enter
  opens or runs the highlighted row, and Esc closes the palette and restores
  the focus that had it.
- The first-run welcome banner previews the link graph with a
  `[[a note]] → graph` pill: clicking it opens the graph for the new vault
  without leaving the banner.

### Changed

- The save-status indicator now makes a clean save and a failure visually
  distinct: a green dot with muted "Saved" stays calm, while a failed save
  or a pending conflict shows a danger/warning dot and matching words, and a
  failed save surfaces Retry beside the message.

## [0.4.1] - 2026-11-08

### Fixed

- The vault-open prompt text no longer overflows the sidebar card: the
  "Paste a folder path…" hint and vault errors (including a long path in the
  OS reason) now wrap inside the card, and the Open / New vault / Cancel
  buttons wrap instead of running past the card edge in a narrow window.

## [0.4.0] - 2026-11-08

### Added

- Omarchy theming: the window follows the shared Omarchy system theme
  (`$XDG_STATE_HOME/omarchy/current/theme/colors.toml`, i.e.
  `~/.local/state/omarchy/current/theme/colors.toml`) by default, deriving
  the full `rune-kit` palette from its background/foreground/accent, layer,
  selection and status colours while fonts and radii stay with the OpenCode
  theme. A theme switch is picked up live (no restart). Off Omarchy the
  palette file is absent and Muninn falls back to the system appearance, so
  non-Omarchy systems see no change. While following Omarchy the sidebar
  theme toggle is hidden: Omarchy is the system theme, not a switcher stop.
  For diverged sessions it cycles System → Dark → Light, and
  `--theme omarchy|system|dark|light` overrides the choice for one session.
  `MUNINN_OMARCHY_COLORS` points at an alternate palette file (or `""` to
  disable).
- Linux `.deb` package (`muninn_0.4.0-1_amd64.deb`, installable with
  `sudo apt install ./muninn_*.deb`) and signed releases: every tarball,
  zip and `.deb` now carries a SLSA build-provenance attestation
  (`gh attestation verify`), and `checksums.txt` is signed with cosign
  (Sigstore keyless, `checksums.txt.sigstore.json`).

### Changed

- Keyboard shortcuts help lays its sections out in a grid: two columns on a normal
  desktop, four when the window is wide enough for a card per section, and one
  column when a second card would be too narrow. Keys render as keycaps (with
  `⌘` and Ctrl chords kept readable), and autosave, diagnostics and trash each
  sit in their own block. Diagnostics lines wrap, and the log scrolls once it
  outgrows its block.

## [0.3.0] - 2026-11-07

### Added

- Session restore and settings (LIF-01): theme, collapsed folders, the last vault and note,
  Write/Read/Graph mode, Global/Local scope and the recent-vault MRU persist in
  `$XDG_CONFIG_HOME/muninn/settings.json` (fallback `~/.config/muninn/`). Writes are immediate
  and atomic; a corrupt or missing file starts from defaults instead of failing.
- Vault switching (LIF-01): the sidebar vault chip opens an in-window menu with **Open vault…**
  (typed path, `~` expansion), **New vault…** (creates a missing folder) and the five most
  recent vaults, newest first. A successful open resets the graph, history and recovery store.
- Help overlay (DES-01): `Cmd/Ctrl+?`, the sidebar `?` button, or plain `?` outside a text field
  opens a grouped shortcut map with autosave timing, the version and a GitHub issue link.
  `Esc` closes it; there is no native menu bar on Linux.
- Session log and diagnostics (X-01): `$XDG_STATE_HOME/muninn/session.log` records startup
  build/vault/counts, vault opens with scan time, save failures, conflicts, trash/restore and
  watcher reloads as timestamped key-only lines (never note bodies). `--trace` logs every
  autosave tick, and Help shows the last five lines with a **Copy diagnostics** button.
- Headless vault checks (X-03): `muninn --check <vault>` prints broken links, duplicate titles,
  skipped files and a depth warning with exit codes 0/1/2; `--list-links` prints sorted
  `referrer -> target` lines. Both run before Rune starts a window.
- Attachments and Unicode (X-02/DES-02): `![[image.png]]` renders as a filename placeholder
  instead of a faint missing note (Rune has no image pipeline and refuses `file:` URLs), and
  CJK/accented/RTL titles have regression tests for search, resolve and duplicate labels.
- Attachment open and reveal (DES-02): the reader shows each embed as filename + size in an
  attachment strip with an **Open** action, and a note's context menu gained **Open
  externally** and **Reveal in file manager**. Both spawn the platform handler
  (`xdg-open` / `open` / `start`) on a canonical path, since `Tx::open_url` still refuses
  `file:`; a failure lands in the status bar rather than panicking.
- Drag-drop import (DES-02): dropping `.md` or png/jpg/jpeg/gif/webp/pdf files anywhere in the
  window copies them into the current folder through the atomic temp+rename path and reloads;
  Markdown opens in the editor, non-Markdown files stay attachments. An existing filename is
  refused with its name and never overwritten. Works on macOS, Windows and X11; Wayland
  exposes no file drags in winit 0.30.
- Reduced motion and labels (DES-03): `MUNINN_REDUCED_MOTION=1` (`true`, `yes`, `on`) freezes
  the force layout at its deterministic seed positions without waking the window, and the
  icon-only theme, help and folder `+` buttons now carry accessible labels and tooltips.
- First-run welcome and scan timing (LIF-01): a vault folder created at launch shows
  "New vault created at … — make your first note" instead of an empty editor, and the status
  bar reports the startup scan time.
- Chunked vault scan with progress and cancel (M3): a vault above 100 `.md` files reads 50
  note bodies per poll tick and shows "Scanning N/M notes — Esc to cancel" with a progress
  bar, so startup stays responsive at 2,000 notes; `Esc` (or **Cancel**) abandons the scan and
  leaves the Retry page without ever installing a partial index. Vaults at or below 100 files
  still load synchronously in one frame, and every completed chunked scan produces exactly the
  same notes, folders, links and backlinks as `Vault::load`.
- Large-vault settle and dormancy (GRP-01): dense layouts now seed inside the world instead of
  wedging against its walls, damp and cap velocity more tightly, and derive velocity from the
  separation correction so the wall/spring/repulsion tail loses energy. Once heat is zero and
  no node moves faster than 0.4 pt/step the simulation goes dormant and `poll` sleeps; the
  2,000-note fixture is quiet by frame ~2,600 with a final-frame residual of 0.39 pt. Vaults up
  to 200 notes keep the exhaustive path bit-for-bit, including the sample layout.
- Large-vault graph performance (GRP-01): repulsion and card separation now use a
  uniform spatial grid above 200 notes, with a density-tapered cutoff and force
  normalisation. A 2000-note star+chain fixture steps at 4.8 ms p95 in release
  (19.9 ms in an unoptimised test build); the 186 visible notes in the reference
  vault settle in 61 ms of layout work. Small vaults keep the exhaustive pass, so
  their settled positions and snapshots are unchanged.
- Keyboard-complete graph (GRP-02): arrows move a visible focus ring distinct from
  the selection, `Enter` opens the focused note, `+`/`-` zoom, `0`/`F` fit, and
  `Esc` still leaves the graph. The arrow move is nearest-in-direction with a
  deterministic sorted-order wrap.
- Graph discovery controls (GRP-03): degree-scaled node cards and dots, a
  search-to-node field that selects and centres a title/path match, an Isolate
  toggle that dims non-neighbours, and Local label thinning to dots at far zoom.
- Per-note undo history (EDT-02): switching away from a note and back restores its
  text, caret and Rune undo/redo stack, so `Cmd/Ctrl+Z` still reverts edits made on the
  first visit. Snapshot memory follows Rune's per-note cap (128 steps or 1 MiB) and the
  cache is bounded to 24 notes; an external reload or conflict resolution starts a
  fresh stack instead of resurrecting history for text that is gone.
- Markdown comfort in the editor (EDT-05): `Enter` continues `- `, `* `, `+ ` and
  `1. ` items with the same indentation (numbered items advance), removes the marker on
  an empty item to end the list, and closes an unclosed `[[` with the caret inside when
  the wikilink completion is not showing. `Tab` indents four spaces instead of moving
  focus out of the note.
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
- Keyboard access to links (A11Y-01): the links rail's outgoing and backlink rows, the
  sidebar note list and the broken-links chip and dashboard rows are focusable. `Esc`
  releases the editor, `Tab` reaches the rail (its rows are the first stops, since the
  editor keeps `Tab` for indentation), `↑`/`↓` move through a list, and `Enter` or
  `Space` opens the focused note, link, target or referrer.
- Rename/move link impact (SAF-06): renaming or moving a note with inbound links asks
  first, showing the referrer count and a **Rewrite links to new name** toggle (on by
  default). Rewriting replaces each inbound target while preserving `|alias` and
  `#heading`, uses the existing atomic note write per file, and only rewrites a bare
  duplicate title when it resolved to this note before the change. Per-file failures are
  reported without rolling back the rename or move.
- Folder rename link impact (SAF-06): renaming a folder that has links into it offers the
  same **Rewrite links** / **Leave links dangling** choice. Rewriting replays pre-rename
  resolution, so path-qualified, `#heading` and `|alias` links are pointed at the new stems;
  a bare title that resolved under the old folder becomes path-qualified, while an unrelated
  duplicate title stays put. Per-file failures are reported without rolling back the
  directory rename.
- Atomic note writes: saves and note creation write a hidden, uniquely named temp file in
  the note's folder, flush and fsync it, then rename it into place, followed by a
  best-effort directory fsync on Unix. A failed or interrupted save leaves the previous
  note intact instead of truncated. `Vault::disk_state` exposes a note's on-disk mtime and
  length as the prerequisite for conflict detection (SAF-02).
- Vault-local trash (SAF-04): deleting a note moves its file to
  `.muninn-trash/<unix timestamp>/<original vault-relative path>` with one atomic rename
  (dot-directories are never scanned). The status bar offers **Undo** to rename the file
  back without overwriting an existing note; the toast expires on the next note action.
  Folder deletes stay permanent and empty-only. The trash is capped at 500 files or 100 MB:
  each successful delete prunes the oldest timestamp folder first (best-effort, never
  failing the delete), and the Help overlay shows the current size with an **Empty trash**
  button that permanently clears it without touching live notes.
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

- Window geometry persistence is documented as **blocked** rather than deferred: Rune
  exposes size and position setters (`AppSpec::size`, `Tx::resize_window`,
  `AppHandle::set_position`) but no live-window getter, and its only persistence helper
  (`rune_mac::remember_frame`) is macOS-only, so `settings.json` gains no unreadable window
  field (README and ARCHITECTURE carry the `file:line` evidence). The README screenshots
  were regenerated from the current binary.
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
