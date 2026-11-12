# Muninn Kanban slice 2 proof

Proof for PR #46 (`cursor/muninn-kanban-s2-view`), read-only Kanban mode.

- PR head SHA the smoke ran from: `27f5eee1f23356163aca2b97c94dfa2936eba3fd`
- Base: `cursor/muninn-kanban-s1-model` at `3bda7d648d8666a743c55d38f4f875116d8fa095`
- Harness: `scripts/ui-smoke.sh` (all steps), run on minis under `dbus-run-session`, with Wayland/X/D-Bus env unset, a private `XDG_RUNTIME_DIR` inside the clone, a private Xvfb, and `--new-instance` for every windowed launch. Set `UI_SMOKE_FETCH_LAVAPIPE=1`.
- Result: every step PASS (see `summary.txt`).
- wgpu backend: Vulkan on lavapipe, `llvmpipe (LLVM 23.1.3, 256 bits)`, Mesa 26.2.5. This comes from `vulkan-swrast-1:26.2.5-1` (sha256 `e14837dfe51c29d8bdb29422b40daaafc2291908c4a6349a0e63addd09fa546b`), fetched into the run's `tools/`. `headless_modes` ran before the fetch, on the host GPU (AMD Radeon 760M, RADV PHOENIX).
- `x11_kanban` against the headless `kanban-readonly.png`: mean abs diff 0.00.

## Kanban screenshots

| File | What it shows |
| --- | --- |
| `screens/kanban-empty.png` | No kanban open: the empty state with a hint |
| `screens/kanban-readonly.png` | `Release.md` (the lanes-and-limits fixture) as lanes and cards, read only |
| `screens/kanban-wip-over.png` | `Overloaded.md`: Doing at 3/2 with Over limit, Shipped marked Complete |
| `screens/kanban-unreadable.png` | `Torn.md` (bad JSON footer): can't-be-shown state with Open as markdown |
| `screens/kanban-write.png` | After Open as markdown: Write mode on `Torn.md` |
| `screens/kanban-restart.png` | After a restart: the saved kanban is restored in Kanban mode |
| `screens/kanban-x11.png` | The same page under X11 (Xvfb) |

The other screenshots, all logs, and the accessibility trees (`trees/*kanban*.json`) come from the same run. Every fixture file's sha256 was identical before and after each kanban step (`logs/headless_kanban.log`, `logs/x11_kanban.log`).
