# Kanban slice 4: card editing and history

Proof for `cursor/muninn-kanban-s4-edit` at PR head **cb786b0c5a9d0bb553080039f284535a3085757b**,
stacked on `cursor/muninn-kanban-s3-open` at `ae05869786ed07321f017986171e7a4e44c90432` (#47 after fix round 1).

Run on minis under `dbus-run-session`, with `WAYLAND_DISPLAY`, `WAYLAND_SOCKET`, `DISPLAY` and
`DBUS_SESSION_BUS_ADDRESS` unset, private 0700 `XDG_RUNTIME_DIR`, config, state and `TMPDIR` inside the
clone, the harness's private Xvfb and `--new-instance` for every windowed launch. wgpu backend: Vulkan on
lavapipe (`llvmpipe (LLVM 23.1.3, 256 bits)`, Mesa 26.2.5) for every step, through `UI_SMOKE_VK_ICD`,
`VK_DRIVER_FILES` and `VK_ICD_FILENAMES` for the whole run.

- `summary.txt`, `logs/`, `screens/`, `trees/`: the whole `scripts/ui-smoke.sh` harness, every step PASS,
  including the new `headless_kanban_edit` and `x11_kanban_edit`.
- `logs/kanban-edit-saved.md` and `logs/x11-kanban-edit-saved.md`: the files as saved, equal byte for byte
  to `scripts/ui-smoke/kanban-edit-saved.md` and `scripts/ui-smoke/kanban-edit-x11.md`.
- `ci/`: the CI commands from `.github/workflows/ci.yml` (fmt, clippy `-D warnings`, test, xtask layers,
  rustdoc `-D warnings`) on that head, all exit 0.
