# Accessibility (AT) checklist

How to run a real screen-reader pass over Muninn and record what breaks. The
first target is **Orca** on Linux (GNOME); VoiceOver (macOS) and Narrator
(Windows) are follow-ups. Run the app on a real session (not the headless
harness) so the AccessKit tree is live.

**A live AT pass was recorded on 2026-11-15** through the same AT-SPI tree
Orca consumes: a real app window on a real session, walked with a pyatspi
client across Write, Read, Graph, Board, Kanban, Help, the command palette and
every Settings tab. It found five unnamed text fields (fixed in #148:
sidebar search, graph node search, note editor, vault path, and the Sync
fields) and the rest passed at the tree level. It did **not** listen to speech
output or announcements, so the walkthrough boxes below stay unchecked until a
spoken Orca session happens. The automated suites prove roles, names and
contrast in the headless harness; they are not a substitute for listening.

## What is already covered (v0.8.0 + a11y follow-up #72)

- **Kanban.** Header, lanes, cards, menus, pickers and the conflict bar are Tab
  stops with role, name, help and a focus ring; buttons activate on Space/Enter;
  Up/Down move selection and focus together; menus are modal overlays that
  return focus on close; the date picker is a modal grid. (`PR #65`,
  `crates/muninn-ui/tests/kanban_a11y.rs`.)
- **Board toolbar & canvas.** The toolbar is a named `toolbar`; its hand,
  select and drawing-tool buttons are named, focusable toggle buttons with help
  naming their keys and a focus ring; the canvas is a named `canvas` reporting
  element count, tool and selection; Help has a Board shortcut group.
  (`PR #82`, `crates/muninn-ui/tests/board_a11y.rs`.)
- **Board colour palette.** Palette buttons are named, focusable, 24 pt targets.
  (`PR #79`.)
- **Kanban leftovers.** The card-menu time picker is a modal that returns focus;
  card markdown links take their accessible name from the link text (see the
  bare-URL limitation below); checklist rows are list items/tasks. (`PR #85`,
  `crates/muninn-ui/tests/kanban_a11y.rs`, `kanban_links.rs`.)
- **Contrast.** WCAG 2.1 AA floors are enforced by `muninn_ui::contrast` and
  pinned in `crates/muninn-ui/tests/contrast.rs`: `text_faint` / `text_muted`
  on the page and the raised layers (`layer_01`–`layer_03`, including the hover
  overlay over the selected `layer_03`), `icon_muted` / `icon_faint` at 3:1,
  the four status inks at 4.5:1, Board's default ink on a dark paper, and the
  Board palette's swatch rings on either paper. (`PR #86` plus the
  `layer_03`/overlay and swatch-ring follow-ups.)
- **Reduced motion.** Kanban drag, Find's outline and the Board camera's
  zoom/pan jump instantly under `Settings.reduce_motion`. (`PR #87`,
  `crates/muninn-ui/tests/reduce_motion.rs`.)
- **Board committed ink.** A shape stored with `#1e1e1e` on a dark board is
  painted with a legible hue-preserving substitute while the element and the
  `.excalidraw` file keep the saved colour. Display-only, never rewritten.
  (`PR #88`, `crates/muninn-ui/tests/board_ink.rs`.)
- **This checklist.** (`PR #84`.)

## Walkthrough checklist

### Write (editor)
- [ ] Editor caret and selection are announced; note headings/lists are exposed.
- [ ] The command palette is modal and returns focus to the editor/search on close.
- [ ] The broken-links chip and its dashboard are reachable and named.

### Read (preview)
- [ ] Links are named (link text or target), not a bare URL.
- [ ] The backlinks rail is a reachable list.

### Graph
- [ ] The focused note has a focus ring; Enter opens it; arrows move focus; zoom/fit keys are keyboard-only.

### Board
- [ ] Toolbar announces "Board tools" and each tool with its key.
- [ ] Canvas name announces element count, tool and selection; selection changes are announced.
- [ ] Keyboard select/delete/nudge of drawn shapes works (arrows, Delete, click/Shift+click, marquee).

### Kanban
- [ ] Lanes/cards announce as a list; cards are nested groups; arrows rove checkbox/menu/checklist.
- [ ] The card-menu time picker is a modal returning focus.
- [ ] Checklist rows announce as list items/tasks.
- [ ] Card markdown links are named.

## Known gaps to confirm on the pass

- **Bare-URL accessible name.** A link written as a bare URL (`<https://…>`
  or an autolinked URL) is announced as that URL: Rune's Markdown links take
  their name from the link's text span, and there is no per-link label API
  (`rune-kit/src/markdown/links.rs`). This is a Rune limitation, not a Muninn
  bug; the fix belongs upstream, and Muninn does not work around it by
  inventing hidden text.
- **Spoken AT pass.** The 2026-11-15 pass verified the AT-SPI tree (roles,
  names, reachability). Speech output, announcements on selection/typing, and
  focus-trap behaviour still need a listening Orca session; the walkthrough is
  the source of truth for those, and no box is checked until it has been.
- **Board canvas per-shape objects:** named per-shape objects, as far as Rune allows.
- **Contrast beyond the automated matrix.** The tests pin the palette's
  resolved kits and the Board palette; a real pass should still confirm
  third-party Omarchy palettes and images/board papers with unusual colours,
  where the app cannot know the ink a file stores.

## Recording findings

File a child of #72 (or a new issue) with: the mode, the element, what Orca
said versus what it should say, and the file:line if known. Never paste private
note contents.
