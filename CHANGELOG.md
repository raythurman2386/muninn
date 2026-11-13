# Changelog

All notable changes to Muninn are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Board colours.** The Board toolbar's colour button opens a small palette:
  stroke colours in Excalidraw's primaries (black, grey, red, orange, yellow,
  green, blue, purple) and fills in Excalidraw's pastel shades, plus
  transparent. The picked colours ink every new shape, line, arrow, freehand
  stroke and text, and picking one with a selection recolours the selected
  elements as one undo step. Existing boards keep the colours already in the
  file.

### Changed

- **Board accessibility (#72).** The Board toolbar is a named `toolbar` whose
  hand, select and drawing-tool buttons are named, focusable toggle buttons
  with help naming their keys, a focus ring and a 30 pt target; Tab reaches
  them, Space picks a tool and the press still returns the keyboard to the
  canvas. The canvas is a named `canvas` reporting its element count, tool and
  selection, and Help gains a Board group matching the README's keys.

### Fixed

- **Long unbroken card text on the Kanban board.** A card whose visible text
  is one run of letters, digits, `=`, `.`, `/` or `_` (an env var or a URL)
  now stays inside the card in Board, List and Table views instead of
  painting over the next lane. Board and List wrap it, URLs stay one
  clickable link to the exact address, a single long `#tag` chip shrinks and
  wraps, and the stored card text and the editor keep the original bytes.
- The sidebar's broken-link chip no longer counts an existing board or
  attachment embed (`![[Plan.excalidraw]]`, `![[pic.png]]`) as a broken link.
  A target that resolves to a file on disk is now skipped by the link checker;
  a genuinely missing note or file is still reported.
- **Recovery banner.** When a crash leaves both Kanban and Write recovery
  copies of one note, each row keeps its `— Kanban edits` / `— Write edits`
  suffix visible even when the note's path is long: the path gives up the
  width and truncates first, while its full text stays available as the row's
  hover help.

## [0.8.0] - 2026-11-13

### Added

- **Kanban mode.** A fifth mode (`⌘5` / `Ctrl+5`, a Kanban mode
  button, **Toggle Kanban** in the command palette) shows a kanban note as a
  board: lanes in file order with their card counts, `n/N` and an over-limit
  flag against a lane limit, Complete on a done lane, and each card's done
  mark and markdown. It shows the selected note when that is a kanban note,
  else the last kanban shown (restored after a restart), else an empty state.
  A note whose settings footer can't be read offers **Open as markdown**.
  Viewing never writes the file.
- **Finding, opening and making kanbans.** A **Kanbans** section in the
  sidebar lists every kanban note and opens it in Kanban mode, and kanban
  notes carry a checklist glyph in the notes list. The command palette has
  each kanban by name, **New Kanban** (writes `Untitled Kanban.md`, then
  `Untitled Kanban 2.md`…, in the selected note's folder, never over an
  existing file) and, on an empty note, **Convert empty note to Kanban**. A
  kanban note picked from the sidebar search (`⌘K` / `Ctrl+K`) opens in
  Kanban mode from any mode. A kanban note opened any other way (the notes
  list, a link, a backlink) opens as a note with an **Open as kanban**
  button, and the Kanban page's **Open as markdown** returns to Write on that
  note. On the Kanban page, picking another kanban note keeps the page.
- **Editing cards.** On the Kanban page, **+ Add card** at a lane's foot, and
  each card's menu (its `⋯`, or a right-click): Edit, Insert card before,
  Insert card after, Duplicate, Split card and Delete. A double click edits
  a card in place (`Enter` commits, `Shift+Enter` breaks the line, `Esc`
  cancels), and its checkbox toggles the done mark. In a done lane new,
  duplicated and split cards are done, and the lane menu's **Mark all cards
  done** marks the rest. Undo and redo (`⌘Z`, `⇧⌘Z` / `Ctrl+Y`) restore the
  exact text. Nothing is written until `⌘S`, and each action changes only
  its own lines of the file. Unsaved edits stay when you leave the page;
  picking another kanban, switching vaults or quitting saves them first, and
  a file that changed on disk is not written over. Text the file format
  can't hold stays in the open editor, and the page says why.
- **Card dates and times.** A card reads `@{…}` and the link form `@[[…]]` as
  a date, and `@@{…}` as a time, outside fences and inline code. A card's menu
  has **Add/Change date…** (a month grid with Today, Clear, Cancel and Add) and
  **Add/Change time…** (hour and minute fields); accepting writes the card's
  one token of that kind, replacing an existing one in place, as one undo step
  that changes only the card's line. Typing `@` or `@@` at a word
  boundary in the card editor opens the same picker at the caret, and Escape
  leaves the typed trigger. The card's foot shows the formatted date and time,
  optionally relative (`today`, `in 3 days`, counted from the local calendar
  day) and linked to the daily note. Board footer keys `date-trigger`,
  `time-trigger`, `date-format`, `time-format`, `date-display-format`,
  `show-relative-date`, `hide-date-in-title` and `link-date-to-daily-note`,
  with app-wide `kanban_defaults` in `settings.json`; the footer wins, then the
  app default, then the built-in. Sorting by date uses the parsed day and time,
  and cards with no date sort last. Display settings never rewrite the board.
- **Card tags.** A card's `#tag` tokens are read outside fences and inline
  code, with frontmatter `tags:`/`tag:` (scalar or list) and ordinary note
  bodies in the same vault-wide, case-insensitive tag index. The footer's
  `hide-tags-in-title` keeps tags out of the card's main text and
  `show-tags-at-bottom` draws chips at the card's foot; a chip click sets the
  board's tag filter. In the card editor, `#` at a word boundary offers the
  vault's tags and inserts one at the caret, while Escape leaves the typed
  `#`. `tag-colors` is kept as written for a later editor. Display settings
  never rewrite the board. App-wide defaults live under `kanban_defaults` in
  `settings.json`.
- **Card metadata from linked notes.** A card's first wikilink selects a note,
  and the board's `linked-page-metadata` keys draw that note's frontmatter and
  inline fields (`[key:: value]` or `key:: value`) at the card's foot, after
  dates and tags. `containsMarkdown` renders a value through the reader: its
  links work, a note embed becomes a link and an attachment embed keeps the
  reader's filename placeholder; `show-missing-metadata` draws `key: —` for an absent
  key or note. Keys match case-insensitively and the first occurrence wins.
  App-wide defaults live under `kanban_defaults`; the footer wins, then the
  app default, then the built-in. A missing note or key draws nothing by
  default, and display never writes the board or the linked note.
- **Archiving cards.** A card's menu has **Archive**, and `Cmd`/`Ctrl`-clicking
  its checkbox archives it (a plain click still toggles the done mark). The
  card's lines move whole into the note's archive, after a `***` line and
  `## Archive`, which the first archive creates with the English word; a file
  that already has an archive keeps its own word (German `Archiv`, Japanese
  `アーカイブ` and the other marker words). The header's **Archive** page lists
  the archived cards from the same note, with the count, **Restore** (to the
  first lane, or **Restore to…** for a lane picked by name) and **Delete**.
  `archive-with-date`, `archive-date-format`, `archive-date-position`,
  `archive-date-separator` and `max-archive-cards` (absent or `0` is no limit;
  the oldest archive cards go, never a lane card) read from the board footer,
  then `kanban_defaults` in `settings.json`, then the built-in value. The
  archive date is ordinary card text written with the date tokens, not a
  `@{…}`. Every action is one undo step saved by `⌘S` (or autosave), and
  viewing the archive writes nothing.
- **Finding and filtering the board.** On the Kanban page `⌘F` / `Ctrl+F`
  opens the board's own Find bar: it searches the parsed cards
  case-insensitively, outlines the current match's card, steps with `Enter` /
  `↓` and `Shift+Enter` / `↑`, and `Esc` closes it and restores the previous
  selection. The header's **Filter** panel hides cards by text, tag, date
  (`today`, `overdue`, or a `YYYY-MM-DD` range) and open/done state; conditions
  compose with AND, the header shows what is active with a Clear control, and
  a tag chip's click sets the tag filter. Finding and filtering never change
  the board, its file or its undo history, and filters are not saved.
- **Card checklists.** Nested task-list items inside a card are drawn as an
  interactive checklist at the card's foot with a `done/total` count. Clicking
  one toggles exactly that character through the card edit path: the card's
  own checkbox and every other line stay as they are, and one undo puts it
  back. Fenced lines and the card's first line are not checklist items.
- **Board settings and app-wide defaults.** **Settings › Kanban** edits the
  app-wide `kanban_defaults` in `settings.json` (and the top-level
  `kanban_autosave`), and the Kanban page's **Board settings** panel edits the
  shown board's footer. Both are generated from one schema, so every key has
  one label, help text, type and built-in value. Each control says whether it
  is a **Board override**, an **App default** or the built-in **Default** — a
  value that is present but the wrong JSON type is named **Board value
  ignored** / **App value ignored** instead — and can reset one key: **Reset
  board** removes the footer key, **Reset app default** removes it from
  `settings.json`. A board change writes only that key through the normal
  edit/undo/save path; a typed text value is one undo step, and an app text
  default is written once typing rests, not once per character. Opening,
  closing or switching a board writes nothing and unknown footer keys stay byte
  for byte; the panel closes when another board is shown, the archive opens or
  the Settings modal opens, so one board's text can never be typed into
  another. A `linked-page-metadata`
  edit changes only the row's own entry, keeping extra fields, a wrong-typed
  flag and malformed entries. Alongside slices 9–13's keys, a board can set
  `lane-width` (272, drawn between 120 and 720), `new-line-trigger` (`enter` or
  `shift+enter` in the card editor), `new-card-insertion-method` (`prepend` or
  `append` for **+ Add card**), `show-checkboxes`, the header's
  `show-filter-button`, `show-archive-button` and `show-open-markdown-button`,
  and `new-note-folder` / `new-note-template` for **New note from card**.
  Lane width, the new-line trigger and hidden header buttons take effect at
  once, and a card's menu still offers **Mark done** / **Mark open** when its
  checkbox is hidden.

- **Lanes, moving and sorting.** **+ Add lane** after the last lane, and each
  once.
- **List and table views.** A board whose frontmatter says
  `kanban-plugin: list` opens as one scrollable list of lane sections, and
  `kanban-plugin: table` opens as a table with Lane, Card, Date, Tags and
  checklist-progress columns, grouped by lane in file order. `basic` and
  unknown values open as the board, and every value stays byte for byte in the
  file. The header's **Board / List / Table** switcher shows any of the three;
  its choice is ephemeral: it is not saved and never rewrites the frontmatter.
  List and table rows keep the board's card actions (checkbox toggles done,
  `Cmd`/`Ctrl`-click archives, card menu, links, tag chips; the table's Tags
  column draws the same chips), a collapsed lane hides its cards, and an empty
  lane or board says so. Finding marks every matching card in all three views.
  The table flattens a multi-line card to one safe line, renders dates in
  their column, and scrolls sideways when the window is narrow. A card or lane
  menu item that opens an editor or picker (Edit, Insert card before / after,
  Add a date or time, Add card, Insert/Rename lane, Card limit) switches to the
  board first, where that field is drawn. Switching or reading a view, and
  filtering in it, never writes the board.
- **Keyboard and accessibility on the Kanban page.** Every header, lane,
  card, menu, picker and conflict-bar control is a Tab stop with a role,
  name, help text and focus ring; a focused button activates on Space or
  Enter. A lane's cards are a list whose Up/Down keys move the selection and
  focus together, and Space/Enter toggles the focused/selected card; a card
  row is a nested group, so Tab reaches its checkbox and arrows rove its
  checkbox, menu and checklist rows. Menus (and Delete lane's confirmation
  and Move to list…) take the keyboard when they open, close on Escape, an
  action or a press outside them, and give focus back to the control that
  opened them in the Board, List and Table views. The date picker is a real
  modal overlay whose first control is a button: Enter alone never writes a
  date, a day click or Add date does; its day grid is reachable by Tab and by
  arrows. The conflict bar is an alert dialog: Escape puts it away and gives
  the keyboard to Resolve conflict. Icon and picker targets are at least
  24 × 24 points.
- **Lanes, moving and sorting.** **+ Add lane** after the last lane, and each
  lane's menu: Insert lane before / after, Rename lane, Card limit… (written
  as `## Title (N)`; blank clears it), Mark as done lane / not done lane,
  Sort by text, date or tags, and Delete lane (a lane with cards asks first;
  note text in the lane that the board doesn't show stays in the file, or the
  confirmation says how much goes).
  A card's menu moves it up, down, to the top or bottom of its lane, to the
  previous or next lane, or to a lane picked by name; a click selects a card
  and `Alt+↑/↓`, `Alt+Shift+↑/↓` and `Alt+←/→` move it. A card moved into a
  done lane becomes done. A lane's `‹` collapses it to a narrow strip with
  its title and count, kept in the note's settings. All of it is undoable and
  saved by `⌘S`.
- **Dragging cards and lanes.** On the Kanban page a card drags within its
  lane or into another, with a bar showing exactly where it will land; a
  collapsed lane takes it at its end and a done lane marks it done. A lane
  drags by its header (a collapsed lane by its strip) and its collapse state
  moves with it. `Esc` cancels a drag and a drop in place changes nothing;
  a key that changes the board mid-drag (an undo, a keyboard move) ends it.
  Each drop is one undo step, saved by `⌘S`. Without a mouse, a lane's menu
  has **Move lane left / right**, and `Alt+Shift+←/→` moves the selected
  card's lane.
- **Kanban autosave, sync and recovery.** Kanban edits now save on their own
  once they rest, with Board's delay (`"kanban_autosave": false` in
  `settings.json` keeps saving to `⌘S`). The page follows its file: a change
  made by another app (an atomic save included) is reloaded when there are
  no unsaved edits, keeping the selected card; with unsaved edits the header
  shows **Keep mine**, **Save copy** (a new `Name (conflict …).md` beside it)
  and **Load theirs**, or **Discard** when the file was removed; a rename is
  followed. Unsaved edits are kept in a recovery copy and offered on the next
  launch after a crash. Quitting saves them, else keeps that copy, and only
  refuses (with Save copy offered) when neither works. **Open as markdown**
  saves first, and text typed in Write that could not be saved comes back to
  the Kanban page as unsaved edits. Board and Kanban share one disk rule.
- **Links and rename.** Clicking a `[[link]]` in a card opens that note.
  Typing `[[` in the card editor offers notes. **Copy link to card** writes
  a block id (`^card-N`) once, when the card has none, and copies
  `[[Note#^id]]`; a later copy keeps that id. **New note from card** creates
  a note from the card text, in the folder New note uses and with the note
  template, then replaces the card with a link to it. Opening `[[Note#^id]]`
  jumps to that block, in a kanban card or in an ordinary note. Renaming a
  linked note, or a folder a card links into, updates a clean kanban and
  reloads it; a kanban with unsaved edits is not rewritten, and the page
  names it. Renaming the kanban itself
  updates the path remembered in settings.

### Fixed

- A card moved into an empty lane whose heading is followed by plain text now
  goes after that text instead of above it.
- Kanban saves now compare the note's text, not only its modified time and
  length: another app's same-size change, even with the old timestamp kept, is
  flagged instead of written over. **Save copy** keeps the copy selected and on
  the page through a later vault reload, a successful save clears the recovery
  copy even when the kanban is not the selected note, and a restored copy over
  a removed file is no longer mistaken for a rename onto an unrelated empty
  note. A lane limit the model refuses no longer stops the recovery copy of
  edits already made, and a vault refresh no longer replaces the board under a
  held drag.
- **Kanban menus name what they act on.** A card or lane menu, its **Move to
  list…** picker, and **Delete lane**'s confirmation close when a keyboard
  move, an undo or redo, or another kanban's re-read replaces or shifts the
  board. A still-open menu can no longer delete the neighbouring card or lane
  at that place. The selected card is followed by its content instead (or
  cleared when it is gone), and a menu is not carried from one kanban to the
  next.
- A crash while Write still has unsaved text for a kanban note no longer
  loses the Kanban page's card edit. The page keeps its own recovery file, so
  Write's recovery autosave (including while a conflict refuses the save)
  cannot replace it, and the next launch offers both the card edit and
  Write's text. Restoring Write's copy, even when that text is still a
  kanban, does not replace the card-edit file; **Restore all** brings the
  card edit back first, and the two rows are labelled Kanban edits and
  Write edits.
- **Esc on the Kanban page cancels a field that lost its focus.** After
  clicking away from an open card editor or lane field, `Esc` now closes the
  field and stays on the page (as the docs say) instead of leaving the page
  and committing the text by doing so; `⌘Z` / `Ctrl+Z` there undoes the
  field's typing rather than committing it first.
- **List and table menus act where their field is drawn.** Edit, Insert card
  before / after, Add a date or time, Add card, and a lane's Insert, Rename
  and Card limit from the List or Table view now switch to the Board before
  opening the editor or picker, so the field is visible and the keyboard is
  not left in a hidden editor. The switch commits an open editor and writes
  nothing. The table's Tags column draws real tag chips (a click sets the tag
  filter), Find outlines every matching card in both views, two empty lanes
  no longer share an element key, and each lane's rows are rebuilt only when
  what they draw changes instead of every frame.

### Known limitations

- The date picker's day grid is a `grid`: Tab reaches it, arrows move among
  days without writing, and a day click or Add date is the write. A
  menu-opened time picker keeps its hour/minute fields, so it stays an inline
  group that Tab can leave; Escape returns to the card.
- A Kanban menu floats under its lane header (clamped to stay inside a short
  window) and is a modal overlay: it closes on Escape, an action or a press
  outside it.
- The table has a fixed 1080 pt width and scrolls sideways in narrow windows.
- Metadata note embeds render as links; attachment embeds keep the reader's
  filename placeholder.
- `settings.json` is re-serialized (object order may change); kanban footer
  bytes are untouched.
- The list, table, archive and settings panels have headless coverage but no
  X11 screenshot step yet; slice 16's release smoke is the full harness.
- Checklist rows render as plain text, and a non-task sub-bullet under an item
  can render at the card's top level.
- A Cmd/Ctrl-click whose modifier is released in the same frame can toggle
  instead of archive (one-frame window; the menu path has no window).

## [0.7.1] - 2026-11-11

### Fixed

- **Duplicate tray icons / a second Muninn running (#42).** Opening Muninn
  while it is already running (for example from the app launcher while it
  sits in the tray with Close to tray on) now raises the existing window
  instead of starting a second app with a second "M" tray icon. A launch with
  `--vault PATH` hands that vault to the running app, which switches to it.
  The first windowed launch takes a lock
  (`$XDG_RUNTIME_DIR/muninn/instance.lock`, released when the app exits or is
  killed); a later launch passes on its activation token and `--vault` and
  exits 0, or says why and exits 1 when the running app does not answer.
  The channel is a Unix socket on Linux and macOS, and a loopback port with a
  random secret on Windows. Headless runs (`--snap`, `--tree`, `--check`,
  `--list-links`) never take the lock. `--new-instance` starts a separate
  app anyway.
- **Note when updating:** quit a running 0.7.0 once after updating. It
  predates the hand-off and cannot accept it, so until it quits a new launch
  runs beside it as before.
- UI smoke steps each get their own `XDG_RUNTIME_DIR` and no session bus, so a
  test launch can never reach a Muninn or tray on the desktop. A new
  `x11_single_instance` step checks for exactly one tray item (a
  StatusNotifierItem on a private bus) across a second launch, the Show tray
  icon switch, a tray-host restart, a vault switch and a kill -9 relaunch.

## [0.7.0] - 2026-11-11

### Added

- **Board tab.** A fourth mode (`⌘4` / `Ctrl+4`) for drawing, on a canvas
  drawn natively by Muninn. Boards are ordinary `.excalidraw` files in the
  vault, read and written in Excalidraw's own format, so Excalidraw can open
  them, and elements Muninn does not draw are kept as they are. In short:
  drawing tools (rectangle, diamond, ellipse, arrow, line, freehand, text)
  with Excalidraw's keys; select, move, delete, undo and redo; text and shape
  labels; autosave with conflict and changed-on-disk protection; crash
  recovery; a Boards section in the sidebar and `![[Name.excalidraw]]` embeds
  in notes; a collapsible sidebar; and New board and rename. The entries below
  give the details.
- Board mode: a fourth mode button, `⌘4` / `Ctrl+4`, and **Toggle
  Board** in the command palette open a Board page that lists the vault's
  `.excalidraw` boards with their element counts.
- Board canvas: picking a board draws its rectangles,
  ellipses, diamonds, lines, arrows, freehand strokes and text on the board's
  own paper colour, fitted to the canvas. Drag or Shift+wheel pans, the wheel
  zooms at the pointer (as in Graph), `+`/`-` zoom, arrows pan, `0`/`F` or a
  double-click refits, and corner buttons zoom and fit. Each board keeps its
  view while Muninn runs, and the last board and its view come back on launch.
  This holds in large vaults too (the chunked startup scan keeps the restored
  board, also when Muninn relaunches in another mode and the board is not
  drawn until Board opens), and a newly picked board is only remembered once a
  frame has drawn it, so a board that cannot be drawn is not reopened on every
  launch. Text sizes are clamped
  (1–2048, and at most 1024 pt on screen at any zoom), so a huge `fontSize`
  cannot crash the app.
  Viewing alone never writes the board file.
  Esc leaves Board for the last note mode, and the last mode (now including
  Board) is restored on launch; older `settings.json` files load unchanged.
- Board drawing tools: a toolbar and Excalidraw's keys (`R`,
  `D`, `O`, `A`, `L`, `P`/`X`, `V`/`Esc` for no tool) draw rectangles,
  diamonds, ellipses, arrows, lines and freehand strokes as new Excalidraw
  elements (fresh ids, seeds, versions and fractional indexes). The heading
  shows Unsaved; `⌘S` / `Ctrl+S` saves the board atomically, and Muninn also
  saves it before opening another board, switching vault, rescanning the
  vault or quitting; if that save fails, the board switch, vault switch or
  rescan is refused and the drawing stays. Every element already in the file
  is kept, and a board that changed on disk, was removed, or belongs to
  another vault is never written without asking.
- Board editing: with no tool picked, click selects an element
  (its outermost group; Shift+click adds or removes), a drag on empty canvas
  draws a marquee, a drag on the selection moves it, arrows nudge it (Shift
  for 5), `Delete`/`Backspace` deletes it, `⌘A` selects all, and `⌘Z` /
  `⌘⇧Z` (`Ctrl+Y`) undo and redo drawing, moves and deletes (last 100 per
  board). `Esc` clears the selection, then drops the tool, then leaves Board.
  Panning moves to the hand tool (`H`), Shift+wheel, a trackpad or the
  arrows with nothing selected. Each edit bumps the touched elements'
  `version`, `versionNonce` and `updated` as Excalidraw does; a deleted
  element stays in the file as an `isDeleted` tombstone. Locked elements and
  the elements Muninn keeps as-is (frames, images) are never picked; resize
  handles are shown but resizing is not supported yet (a press on a
  handle does nothing). A text label bound to a shape moves, deletes, undoes
  and redoes with it, and clicking the label selects the shape, so no label
  is left behind or orphaned in the file. Saving is still
  the explicit `⌘S` with all its guards.
- Board autosave and watcher: an open board's drawing is saved
  once it rests (the autosave delay, at least 1 s, or every 10 s while it keeps
  changing), through the same guarded atomic save as `⌘S`, so a file another
  app changed is never written over (the conflict bar decides). It never saves
  mid-drag or with a text editor open; `"board_autosave": false` in
  `settings.json` leaves saving to `⌘S`. While Board shows, boards changed by
  another app are noticed within about 1.5 s: a clean open board is re-read in
  place, a board with unsaved drawing is flagged and kept, and the list's
  counts follow. The sidebar has a Boards section, a note's
  `![[Name.excalidraw]]` embed shows a board placeholder with an Open board
  button, and Board with nothing picked reads "No board selected".
- Board memory and writes: undo history is bounded by size (32 MiB) as well as
  by count, so editing a 10 MB board no longer grows memory towards gigabytes;
  the board recovery copy and `settings.json` are written once a stream of
  changes (a held arrow key, a pan, typing) rests, not once per change; an
  old-layout recovery record shadowed by a newer copy is removed instead of
  being offered again on every launch.
- Board text: the text tool (`T`/`8` or the toolbar) places a
  text element where you click and opens an editor there; `Enter` breaks
  the line, and `Esc`, a click outside, leaving Board or saving commits.
  Double-clicking a text element edits it in place. New text is written
  with Excalidraw's fields (`text`, `originalText`, `fontSize` 20,
  `fontFamily`, `textAlign`, `verticalAlign`, `lineHeight`, measured
  `width`/`height`, `containerId` null, `autoResize`); committing empty text
  adds nothing, and emptying existing text deletes it as a tombstone. Each
  commit is one undoable edit that bumps `version`, `versionNonce` and
  `updated`. While typing, Board and Graph keys stay in the editor and `⌘Z`
  undoes typing. Double-clicking a shape with a label inside it (or the
  label) edits that label in place; it stays bound (`containerId`,
  `boundElements`) and is re-centred in the shape on commit. A label inside
  a locked shape is locked with it. Adding a label to a shape without one is
  not supported yet. Text typed but not yet committed counts as
  unsaved: the recovery copy is written on a timer, once typing rests for
  about 0.5 s and at least every 2 s while it goes on, so a crash keeps
  everything up to the last copy but can lose the last moments of typing
  (up to about 2 s). Quitting or saving mid-edit commits it.
- Unsaved board drawing is never lost: if the board's file is removed,
  renamed or changed by another app, the board stays open and, once Muninn
  notices (when the boards are re-listed — entering Board, a vault reload or
  rescan — when a save reads the file first, or within about 1.5 s
  while Board shows), is flagged in the heading and offers **Keep mine** (write it back to its own path), **Save
  copy** (a new `Title (conflict …).excalidraw` beside it) and **Load
  theirs** / **Discard**. A dirty board also keeps a crash-recovery copy, like
  a note, that the launch banner can restore onto Board (flagged, and
  never written over a changed file without Keep mine); quitting with a board
  that cannot be saved keeps that copy, or refuses to quit if even the copy
  cannot be written. Recovery copies are kept per vault, so two vaults with
  a same-named board or note never overwrite or clear each other's copy
  (older copies are picked up by their own vault), and a recovery record is
  never restored or written outside its vault (no `..`, absolute paths or
  symlinks out).
- New board: a **New board** button in the Boards panel header, and in the
  empty state when a vault has no boards yet ("No boards in this vault yet"),
  creates an empty `Untitled board.excalidraw` (numbered if taken), opens it
  and lists it.
- Collapsible sidebar: the title-bar toggle or `⌘\` / `Ctrl+\` hides and
  shows the notes sidebar (not while a text field has the keyboard). Entering
  Board collapses it to give the canvas room and leaving Board restores it;
  reopening it on Board lasts for the session. A hidden sidebar never keeps
  the keyboard: hiding it unfocuses search and cancels its inline prompts,
  and `⌘K` or a new-note prompt shows it first.
- Board rename: `F2` on Board (with no text field focused), **Rename** in a
  board row's right-click menu (Boards panel or the sidebar's Boards
  section), or a double-click on the board's title opens an inline name
  field (`F2` uses the title when the board has no row in the Boards panel,
  for example one in a hidden folder). Names in any script work, accents
  and emoji included; `Enter` renames and `Esc` cancels, and `.excalidraw` is added for
  you. A taken or invalid name is refused with the reason and the field
  stays open. The open board is saved first and stays open under its new
  name with its view and recovery copy, the file watcher
  does not report the rename, and notes that embed or link the board
  (`![[Old.excalidraw]]`) are updated to the new name, keeping folder,
  heading and alias.
- `muninn --version` (or `-V`) prints `muninn 0.7.0` and exits without
  opening a window.
- Typing in the sidebar search on Board or Graph no longer triggers the
  canvas's keys (tool letters, zoom, fit, arrows).

### Known limitations

- Board parsing and saving run on the UI thread, so opening or saving a very
  large board can pause the window briefly.
- Long text labels overflow their shape; they are not wrapped to its width.
- An IME composition in progress (for example CJK input) is dropped when the
  text is committed, and CJK line breaking is approximate. The measured text
  width uses Muninn's font, which can differ slightly from Excalidraw's.
- Not yet supported on Board: resize and rotate, arrows that follow the
  shapes they are bound to, copy/paste and duplicate, text styling, adding a
  label to a shape that has none, and deleting or moving a board to another
  folder from inside Muninn (use the file manager).
- After restoring a board from crash recovery, it can be flagged as changed
  on disk by another app even when it was not; choose **Keep mine** to save.
  Until then the board cannot be renamed either, and the rename field says
  only "Save the board first: it could not be saved".
- A crash-recovery copy is written as two files (the content, then a small
  record of where it belongs and when), each atomically but not as a pair.
  A crash between the two writes can leave a first copy without its record,
  which is then not offered on the next launch, or a newer copy with the
  previous copy's time.
- New board checks that its name is free and then writes the file, so a
  file another app creates under that exact name in the same instant is
  replaced. A broken symlink with the next free name (`Untitled board
  2.excalidraw` pointing at nothing) is replaced by the new board; nothing
  outside the vault is written.
- Board rename has the same check-then-rename window: a file another app
  creates under the new name in that instant is replaced.
- Rename updates the notes that embed or link the board from the copy
  Muninn holds, without checking whether another app changed those notes a
  moment before (the same as note rename), and it does not roll back: if
  one note cannot be written, the board is still renamed, the other notes
  are updated, and the error names the note that was not.
- Embeds written with a `../` path (`![[../Ideas/Plan.excalidraw]]`) are
  not followed by Muninn and are not updated by a rename.
- A rename that only changes letter case (`plan` to `Plan`) is tested on
  Linux (ext4); on the case-insensitive disks of macOS and Windows it is
  expected to work but has not been tried by hand.
- Opening Settings › Vaults › Open vault while on Board also opens the notes
  sidebar on Board for the rest of the session, as do `⌘K` and `⌘N` there.
  Opening a note from the list or a new-note prompt switches to Write
  without going through the usual mode switch; nothing is lost, as no
  sidebar field is open at that point.
- Saving a board resets its file permissions to the default (0644), and the
  first save of an Excalidraw file reorders its keys and fills in defaults.
- The sidebar's hidden or shown state on Board is not remembered across
  restarts.

## [0.6.0] - 2026-11-09

### Added

- Marketing landing page in `site/` (static HTML, CSS and JS using
  Muninn's Dark and Light themes), and `Dockerfile.marketing`, a static
  nginx image that serves it on port 80. The site is live at
  https://muninn.ravenwoodsoftware.dev, deployed with GoForge. Its
  download buttons link to the latest GitHub Release (Linux x86_64
  `.deb` and tarball, macOS Apple Silicon only, Windows x86_64 zip).

- Privacy policy in `PRIVACY.md` (optional sync, credentials, and website analytics).

- Tray slice two, on Settings › Appearance, each off by default and saved in
  `settings.json` (older files load as off):
  - **Close to tray** (`close_to_tray`): closing the window hides it while a
    tray icon is up; Show Muninn or a click on the icon brings it back, and
    Quit Muninn saves and quits. With the tray off, failed or unsupported,
    closing quits and saves as before. Muninn cannot tell whether a tray host
    shows the icon (stock GNOME has none), so this stays opt-in.
  - **Start Muninn at login** (`auto_start`): an XDG autostart entry on Linux,
    a `HKCU\…\Run` value on Windows (through `reg.exe`). Re-pointed at the
    running executable on startup while on. Disabled on macOS.
  - **Show Muninn with Ctrl+Alt+M** (`show_shortcut`, ⌃⌥M on macOS): a
    system-wide shortcut through Rune's global shortcuts (X11, the Wayland
    portal, Windows, macOS) that shows and focuses the window. A refused
    chord shows why under the switch and is not retried until it changes.
  - The tray menu gains **Sync now**, the Sync tab's Sync Now. It is greyed
    while no provider is connected for this vault, sync is disabled, or a
    sync or sign-in is running. The menu stays flat: Rune's tray has no
    submenus or checkmarks yet.
- Optional tray icon: Settings › Appearance › **Show tray icon**, off by
  default and saved as `tray_icon` in `settings.json` (older files load as
  off). On Linux (StatusNotifierItem) and Windows the icon offers Show Muninn,
  New note, the current sync status as a disabled row, and Quit Muninn, which
  saves the open note first. Clicking the icon shows and focuses the window.
  If the icon cannot be added, the reason shows under the switch and it is not
  retried until the switch changes. Not available on macOS yet:
  the switch is shown disabled. Headless runs (`--snap`) never add an icon.
- Manual vault sync for GitHub and Google Drive from Settings. Sync Now,
  Cancel, status, last success, and Disconnect are explicit. There is no
  background sync. Drive stores an app-managed commit history in Muninn's
  private Drive app data; it is not an editable Markdown mirror. GitHub publishes Git objects
  and updates the selected branch with `force` false. Tokens stay in the OS
  credential store. Disconnect does not delete notes.
- Tailscale as a third sync provider. One device hosts an immutable commit
  store on `127.0.0.1`, published to the tailnet with `tailscale serve`; other
  devices join by MagicDNS name. A shared key in the OS credential store is
  required, and an optional `Tailscale-User-Login` allow-list can narrow it. The
  host only accepts fast-forward commits, so conflicts go through the same
  planner as the other providers.
- Connect signs in for real: GitHub device flow, and Google OAuth with PKCE
  and a `127.0.0.1` loopback redirect, both in the system browser. Tokens go
  straight to the OS credential store and are refreshed before they expire
  and once after a 401.
- Tailscale host: the shared key is masked in Settings, with Copy key (secret
  clipboard, cleared after 60 seconds), Show key and Rotate key. The join
  field is masked.
- Bundled public client ids: the official Muninn Sync GitHub App (device
  flow, Contents read/write) and the official Google Desktop OAuth client
  (PKCE, `drive.appdata` only). Builds can replace them with
  `MUNINN_GITHUB_CLIENT_ID` / `MUNINN_GOOGLE_CLIENT_ID`; the Settings client id
  fields are optional overrides, and an empty id fails closed. No private
  key ships; the Google Desktop client secret comes from the build (see
  Fixed). The official Google app is published, with
  the privacy policy in `PRIVACY.md`.
- **Reduce motion** setting on the Settings › Graph tab: System (default; follows
  the OS reduce-motion preference), On or Off, stored as `reduce_motion` in
  `settings.json`. Older files without the key load as System. `MUNINN_REDUCED_MOTION`
  still forces reduced motion on over any choice. When reduced motion is on, the graph
  layout freezes, camera glides snap, graph highlights land at once and Rune's hover,
  selection and indicator transitions don't travel. The OS preference now also freezes
  the graph layout under System; before, it only affected Rune's transitions. One
  resolved value, `State::reduced_motion()`, feeds all of these.
  When the resolved value turns off mid-session (Settings toggle, or the OS
  preference clearing under System), the graph layout reheats once, so vaults over
  200 notes resume settling instead of staying on their seed positions.

### Changed

- `PRIVACY.md` now says the marketing site uses first-party Umami
  (hosted at https://umami-app.ravenwoodsoftware.dev) to count visits. That
  is separate from the desktop app, which still does not send notes or
  usage data to us.
- Google Drive sync keeps its store in the app's hidden `appDataFolder` with
  the `drive.appdata` scope, named by a store name, instead of a pasted folder
  id that `drive.file` could not reach.
- Each vault binding has its own sync state directory
  (`sync/bindings/<binding id>`, 0700 on Unix).
- GitHub reads one recursive tree per sync and caches blobs by sha.
- Settings is split into tabs — Vaults, Editor, Appearance, Graph and Sync — so the
  modal shows one group at a time. The selected tab is session-only (not written to
  `settings.json`) and resets to Vaults whenever Settings opens; switching away from
  Vaults drops focus from the path field. The Sync tab holds the sync
  provider, fields, status and buttons. The tab bar slides its indicator like the theme picker. The dialog is 540 pt wide and
  the theme picker sizes its segments to their labels, so "Omarchy" and "System" no
  longer truncate. `scripts/settings-tab-snapshots.sh` renders one screenshot per tab
  against a throwaway vault and isolated home (it builds with `--locked` before
  isolating HOME and removes its temp dir on exit).

### Fixed

- Google Drive Connect finishes. Google refuses a Desktop client's token
  request without its client secret, even with PKCE, so sign-in stopped with
  "Authentication needed: [redacted]" and stored nothing. Official builds now
  compile in the Desktop client secret from `MUNINN_GOOGLE_CLIENT_SECRET`
  (never committed) and send it with the code exchange and refresh. A build
  without it, or a custom Google client id in Settings, fails closed before
  the browser opens and sends nothing. The browser tab says "signed in" only
  after the token is in the OS credential store, and shows a failure page
  otherwise. Provider reasons such as "client_secret is missing." are no
  longer hidden, while token and secret values stay redacted. Sync Now keeps
  a failed sign-in's reason in the status instead of a generic hint.

- GitHub and Drive could lose another device's change when both synced at
  once: a rejected GitHub ref update was re-parented onto the new tip with the
  old tree, and Drive re-listed heads at publish time without planning against
  them. Publish now names exactly the state the plan read and the run replans.
- Drive heads that edit the same path no longer yield an empty snapshot.
- Token expiry was stored as a duration; it is now an absolute time.
- A note edited on two devices between syncs stopped sync for that vault on
  every provider, with no way to resolve it in the app. Every such conflict now
  resolves "keep both": this device's text keeps the path, the remote's is
  written as `<stem> (remote)<ext>`, the status names the copies, and other
  changes in the same run still apply. The planner also no longer reads an
  edited path as a rename source, which could drop an edit when a copy held
  the old bytes.
- Renaming a note on one device to a name another device had just used for a
  new note overwrote one of the two notes on the next sync, reported as a
  success. The rename is now declined in that case and both notes are kept,
  the other device's as `<stem> (remote)<ext>`.
- The same note renamed to two different names on two devices, where one of
  those names already held a different note, made every Sync Now fail with
  "differs but was not in the conflict list" until that note was moved. Both
  renames are now declined in that case and every note is kept.
- Drive heads that edited the same path wedged the store with a conflict no
  screen could resolve. They now merge "keep both" with a
  `(conflict <id>)` copy and one merge commit, which also unsticks stores left
  that way. The "no new commits" check moved from before the blob uploads to
  just before the manifest.
- A request to the sign-in loopback with the wrong `state` ended the Google
  sign-in; it now gets 400 and the listener keeps waiting.
- Connect replaced and saved the vault's binding before sign-in, so a failed
  or cancelled sign-in lost a working connection. The swap now happens after
  the new credential is stored, and the old credential is deleted (Tailscale
  too).

## [0.5.2] - 2026-11-09

### Changed

- Removed the write-only `State::vault_prompt` flag (`begin_vault_prompt` /
  `cancel_vault_prompt` now just mark focus on the Vaults path field).

- Overlaid UI state machine is one module now: `overlay.rs` (command palette, Settings modal, Help, and the `Layer` Escape stack) and `mode.rs` (Mode/Scope enums and their transitions) moved out of `state.rs`; lib re-exports keep old import paths stable. `keys.rs`'s thirteen-field `typing` short-circuit is now one call, `State::is_typing()`. (Escape order pinned by `state_layers.rs`; a typing-ownership test added.)
- Dismissal layers are now an explicit enum: `Layer`, `State::topmost_layer`
  and `State::dismiss_layer` name the Escape stack (command palette, Settings
  modal, Help, completion, find, broken-links dashboard, three
  rename/move confirmations, the context menu, the four inline sidebar
  prompts, delete-confirm and the move picker) so one Escape press peels
  exactly one layer. Behaviour is unchanged; the order is pinned by
  `crates/muninn-ui/tests/state_layers.rs`. The full machine inventory now
  lives in `docs/state-machines.md`.

## [0.5.1] - 2026-11-08

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
