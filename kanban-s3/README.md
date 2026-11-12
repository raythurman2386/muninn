# Muninn Kanban slice 3 proof

Proof for PR #47 (`cursor/muninn-kanban-s3-open`): the Kanbans section, the notes-list glyph, the palette entries, New Kanban, Convert empty note to Kanban, Open as kanban and Open as markdown.

- PR head SHA the smoke ran from: `ea747a290c3bb40156f76c858e70a73156ccc1d8`
- Base: `cursor/muninn-kanban-s2-view` at `27f5eee1f23356163aca2b97c94dfa2936eba3fd`
- Harness: `scripts/ui-smoke.sh` (every step), run on minis under `dbus-run-session`, with `WAYLAND_DISPLAY`, `WAYLAND_SOCKET`, `DISPLAY` and `DBUS_SESSION_BUS_ADDRESS` unset, a private 0700 `XDG_RUNTIME_DIR` inside the clone, a private Xvfb, `--new-instance` for every windowed launch, and throwaway vaults under the run's `work/`.
- Result: every step PASS (see `summary.txt`).
- wgpu backend: Vulkan on lavapipe, `llvmpipe (LLVM 23.1.3, 256 bits)`, Mesa 26.2.5, for **every** step, `headless_modes` included. The run was given the lavapipe ICD from slice 2's fetch (`target/ui-smoke/tools/lvp_icd.json`) through `UI_SMOKE_VK_ICD`, and `VK_DRIVER_FILES`/`VK_ICD_FILENAMES` were set to it for the whole run. No log names a host GPU.
- `x11_kanban_open`: each windowed screenshot against its headless reference, mean abs diff 0.00 (note with Open as kanban, Kanban page, back to Write).

## Slice 3 screenshots

| File | What it shows |
| --- | --- |
| `screens/kanban-sidebar.png` | The notes list with the kanban glyph on each kanban note, and the Kanbans section below it |
| `screens/kanban-new.png` | After New Kanban at the vault root: `Untitled Kanban.md` on the Kanban page (To do, Doing, Done), listed in the Kanbans section; `kanban-new-second.png` is the second one, `Untitled Kanban 2.md` |
| `screens/kanban-open-button.png` | `Release.md` opened from the notes list (X11): Write mode with Open as kanban in the header |
| `screens/kanban-open-kanban.png` | After Open as kanban (X11): the Kanban page with Open as markdown |
| `screens/kanban-as-markdown.png` | After Open as markdown (X11): back in Write on `Release.md` |
| `screens/kanban-new-x11.png` | New Kanban from the palette in the windowed app |
| `screens/kanban-new-convert-offered.png` | The palette on the empty `Blank.md`: Convert empty note to Kanban is offered |
| `screens/kanban-new-convert.png` | After Convert: `Blank.md` on the Kanban page |
| `screens/kanban-new-refuse.png` | On `Agenda.md` (has text): Convert is not offered; the note is unchanged |
| `screens/kanban-new-palette.png` | The palette on `Agenda.md`: its actions (New Kanban is further down the list, then the Kanbans section with one entry per kanban by name; see `trees/kanban-new-palette.json`) |
| `screens/kanban-open-from-palette.png` | Overloaded opened from the palette (focus on the Kanban page) |

The other screenshots, all logs, and the accessibility trees (`trees/*kanban*.json`) come from the same run. The pre-existing `Plans/Untitled Kanban.md` and every other seeded file kept their sha256 (`logs/headless_new_kanban.log`, `logs/x11_new_kanban.log`, `logs/x11_kanban_open.log`).
