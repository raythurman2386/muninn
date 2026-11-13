# State machines in Muninn

Implicit state is how this app currently works: a pile of `Option<T>`
and boolean fields on `State` whose *combination* decides what the user
sees and which keys do what. This document makes those machines explicit so
a change to one row can't silently change another. The refactor that
introduced it added one enum + two functions and changed no
behaviour: `pub enum Layer`, `State::topmost_layer`, `State::dismiss_layer`,
pinned by `crates/muninn-ui/tests/state_layers.rs`.

## The two axes

Overlay types and behaviour now live in `overlay.rs` (palette, Settings,
Help, layer stack) and mode transitions in `mode.rs` (`Mode`/`Scope`); the
`State` fields behind them stay in `state.rs` so the struct's invariants are
still visible in one place.

- **Page** (`Mode`): Editor, Preview, Graph. What the main column draws.
- **Overlay stack** (`Layer`): what Escape dismisses, one per press, top
  first. This section is the dismissal order.

Escape always peels the topmost layer; with no layer open, Escape in Graph
returns to the note mode. One press never closes two layers.

## Escape stack, top first

| # | `Layer` | Backing field | Opens via |
|---|---|---|---|
| 1 | `Palette` | `palette: Option<PaletteState>` | `⌘⇧P` |
| 2 | `Settings` | `settings_open: bool` | `⌘,` or gear button |
| 3 | `Help` | `help: bool` | `⌘?` / `?` / `?` button |
| 4 | `Complete` | `complete: Option<LinkComplete>` | typing `[[` or `⌘P` |
| 5 | `Find` | `find: Option<FindState>` | `⌘F` |
| 6 | `KanbanFind` | `kanban_find: Option<KanbanFindState>` | `⌘F` on the Kanban page |
| 7 | `KanbanFilter` | `kanban_filter.open: bool` | the Kanban header's **Filter** |
| 8 | `BrokenPanel` | `broken_panel: bool` | broken-links chip |
| 9 | `RenameConfirm` | `rename_confirm: Option<RenameConfirm>` | F2 on a note with inbound links |
| 10 | `MoveConfirm` | `move_confirm: Option<MoveConfirm>` | note move with inbound links |
| 11 | `FolderRenameConfirm` | `folder_rename_confirm: Option<FolderRenameConfirm>` | folder rename with links into it |
| 12 | `Menu` | `menu: Option<ContextMenu>` | right-click |
| 13 | `KanbanPicker` | `kanban_picker: Option<KanbanPicker>` | a card menu's date/time item, or typing `@` / `@@` in the card editor |
| 14 | `NewNote` | `new_note_prompt: Option<NamePrompt>` | `⌘N`, New note button |
| 15 | `RenameNote` | `rename_prompt: Option<RenamePrompt>` | F2 on a note |
| 16 | `NewFolder` | `new_folder_prompt: Option<FolderPrompt>` | folder `+` |
| 17 | `RenameFolder` | `rename_folder_prompt: Option<FolderRename>` | folder menu |
| 18 | `DeleteConfirm` | `delete_confirm: Option<DeleteConfirm>` | Delete key |
| 19 | `MovePicker` | `move_picker: Option<usize>` | note menu "Move to…" |

Consistency rules the stack encodes:

- **Modals mask pages.** Palette and Settings are full overlays; opening one
  dismisses nothing beneath it except where a flow replaces it (Settings
  opens over the help page by clearing `help`).
- **One press, one layer.** `dismiss_layer` consumes exactly the topmost
  layer per Escape; the underlying field is cleared by the same cancel/close
  fn the button presses.
- **Inline prompts live in their row.** The four `*_prompt` layers render
  inside the sidebar row that asked; they never float.
- **Graph is not a layer.** `Mode::Graph` exits via Escape after layers are
  peeled, back to `last_note_mode`.

## Other machines (not in the Escape stack)

| Machine | States | Notes |
|---|---|---|
| `Mode` | Editor · Preview · Graph · Board · Kanban | drives the main column; `Graph` shows the camera |
| `Scope` | Global · Local | graph canvas only; local keeps the open note + neighbours |
| Theme (`ThemeMode`) | Omarchy · System · Dark · Light | persisted; Settings picker or sidebar cycle |
| Graph camera | `needs_fit` / `auto_fit` / `CameraGlide` | snap vs glide; reduced motion snaps |
| Graph focus | `graph_focus: Option<usize>` | separate from selection; arrows move it, Enter opens |
| Search highlight | `search_selected` + reflected `Ambiguous` | title-first ranking; `None` means first result |
| Palette highlight | `PaletteState { selected, reveal }` | query snapshot resets highlight to row 0 |
| Kanban filter | inactive · active | display-only; one shown-card predicate drives lanes and arrows; Clear puts back the pre-filter selection |
| Board Find (kanban) | closed · open · blurred | `kanban_find`; scans the shown cards, and its field owns the keyboard only while focused |
| Wikilink completion | `index`, `dismissed` | eight max; `Escape` dismisses the list, not the picker reasoning |
| Editor | dirty / conflict `expected_disk` | conflict pauses autosave until resolved |
| Vault load | sync ≤100 files · chunked `ScanJob` | progress vs blocking; cancel keeps the Retry page |
| Welcome | first-run banner | armed by vault creation at launch |
| Omarchy follow | file hash drives kit updates | `unknown`/absent → system appearance fallback |

## DX and AX consequences

- `state_layers.rs` pins the layer order and the peel order; a reorder
  breaks a test before it breaks a user's Escape key.
- Every field's `pub` is intentional: Escape, `keys.rs`, and the overlay
  toasts read them by name; `Layer` names them as a group.
- Hybrids to avoid in future work: new overlays go through `Layer`
  (renders + dismisses through the same enum row), new prompts render in
  their row, and new transient busy states go through `poll` (which returns
  the changed flag) rather than the view tree.
