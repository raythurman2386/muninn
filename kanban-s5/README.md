# Kanban slice 5: lanes, card moves, sorting and collapse

This is the proof for `cursor/muninn-kanban-s5-lanes` at PR head **bf1f356e0bbdacaf2916ee8701ac6c7e3361f0ef**. That branch is stacked on `cursor/muninn-kanban-s4-edit` at `cb786b0c5a9d0bb553080039f284535a3085757b` (#48).

## How it was run

The harness ran on minis under `dbus-run-session`:

- `WAYLAND_DISPLAY`, `WAYLAND_SOCKET`, `DISPLAY` and `DBUS_SESSION_BUS_ADDRESS` were unset.
- `XDG_RUNTIME_DIR` (private, 0700), config, state and `TMPDIR` were all inside the clone.
- Windowed launches used the harness's private Xvfb and `--new-instance`.

wgpu backend for every step: Vulkan on lavapipe (`llvmpipe (LLVM 23.1.3, 256 bits)`, Mesa 26.2.5). It was set through `UI_SMOKE_VK_ICD`, `VK_DRIVER_FILES` and `VK_ICD_FILENAMES` for the whole run.

## Contents

- `summary.txt`, `logs/`, `screens/`, `trees/`: the whole `scripts/ui-smoke.sh` harness. All 38 steps pass, including the new `headless_kanban_lanes` and `x11_kanban_lanes`.
- `logs/kanban-lanes-saved.md` and `logs/x11-kanban-lanes-saved.md`: the files as saved. They match `scripts/ui-smoke/kanban-lanes-saved.md` and `scripts/ui-smoke/kanban-lanes-x11.md` byte for byte.
- `ci/local-ci.log`: the CI commands from `.github/workflows/ci.yml` on that head: fmt, clippy `-D warnings`, test, xtask layers and rustdoc `-D warnings`. All exit 0.
