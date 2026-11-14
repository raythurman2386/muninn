# Accessibility (AT) checklist

How to run a real screen-reader pass over Muninn and record what breaks. The
first target is **Orca** on Linux (GNOME); VoiceOver (macOS) and Narrator
(Windows) are follow-ups. Run the app on a real session (not the headless
harness) so the AccessKit tree is live.

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
- [ ] The card-menu time picker is a modal returning focus (in progress, #72).
- [ ] Checklist rows announce as list items/tasks (in progress, #72).
- [ ] Card markdown links are named (in progress, #72).

## Known gaps to confirm on the pass

- **Contrast:** `text_faint` / `text_muted` / `icon_muted` on `layer_01` /
  `layer_02`, the "Over limit" warning, done-lane "Complete", and Board default
  ink `#1e1e1e` on a dark board background. (Audit in progress, #72.)
- **Reduced motion:** Kanban drag, Find outline, and Board camera zoom/pan
  should jump instantly under `Settings.reduce_motion`. (In progress, #72.)
- **Board canvas per-shape objects:** named per-shape objects, as far as Rune allows.

## Recording findings

File a child of #72 (or a new issue) with: the mode, the element, what Orca
said versus what it should say, and the file:line if known. Never paste private
note contents.
