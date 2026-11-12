# Kanban slice 6, restack + fix round 1

Proof for `cursor/muninn-kanban-s6-drag` at PR head **bf694c05869ea73ed1c10587775788adbdbc6930**:

- `8600dbd` the slice 6 commit, rebased onto s5's fix head `e492fdd93808dcc82f729e6f851b24a057b4f969` (conflicts resolved: both sides' new core tests kept; `move_card_now` keeps s5's new doc and is `pub(crate)`);
- `bf694c0` "Kanban s6 fix: keys can't retarget a held drag (N1)", which also covers GK's N2, N4 and N7.

The earlier proof (head `34a5ae4`) is one level up, in `kanban-s6/`.

## N1: the option chosen

**Cancel the drag whenever the board changes.** A press on a card or lane is held in `kanban_drag` from its first moment (not only past the click distance). Every board change clears it: an edit (`edit_kanban`), undo or redo, a mode switch, and opening or reloading a kanban. The rest of the gesture then does nothing, so it can't move whatever now sits at the pressed place. This covers every key family, and any other path that changes the board, without listing keys. Esc still cancels through Rune. The drag's own editor commit at the threshold doesn't end it, because the drag is taken out while it settles.

## Screenshots (`n1/`, 3 of them)

A headless run of the app (`n1/n1.json`, the same isolation and lavapipe) selects Short card, presses on the tall card and drags it over Done, then presses `Alt+↓` while holding, then releases over Done:

- `n1-held.png`: the bar is drawn in Done and the tall card is dimmed;
- `n1-after-key.png`: `Alt+↓` swapped Short card with the tall card, and the drag ended (no bar);
- `n1-released.png`: the release moved nothing. Before the fix it moved Short card, which was then at the pressed (0, 1), into Done.

The `n1-*.json` trees confirm it: lane 0 is Tall, Short, Another, Last after the key and after the release, Done holds only Shipped, and `Board.md` is untouched.

## Everything else (logs)

- `summary.txt`, `logs/`: the whole `scripts/ui-smoke.sh` harness on `bf694c0`, **40/40 PASS** on the first run. Same isolation as before: `dbus-run-session`; `WAYLAND_*`, `DISPLAY` and `DBUS_SESSION_BUS_ADDRESS` unset; a private 0700 `XDG_RUNTIME_DIR` in the clone; a private Xvfb and `--new-instance`. wgpu: Vulkan on lavapipe (`llvmpipe (LLVM 23.1.3, 256 bits)`, Mesa 26.2.5) for every step. Following the new proof rule, no screens or trees from this run are included.
- `ci/local-ci.log`: the `ci.yml` commands on `bf694c0` (fmt, clippy `-D warnings`, rustdoc `-D warnings`, layers, 687 tests). All exit 0.
- `ci/kanban-drag-tests.log`: `crates/muninn-ui/tests/kanban_drag.rs`, 13 tests. There are 7 new ones:
  - N1: Alt+Shift arrows, undo and redo, and Alt arrows (including a press that hasn't moved yet);
  - N2: a mode switch;
  - N4: a rightward lane drag that settles an open lane field (M11/M26/M24), a drag that closes a menu (M07), and opening another kanban mid-drag (M21).
  - A fresh press after an ended drag drags again (M22).
  - The N1/N2 tests fail without the fix. The M07, M24 and rightward-slot mutants each fail one test.
