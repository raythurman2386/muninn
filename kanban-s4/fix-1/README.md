# Kanban slice 4, fix round 1 (B1, B2, N-a, N-b, N-e)

Proof for `cursor/muninn-kanban-s4-edit` at head **5d153039723605c2513f22a2b26ee585b7732f69**
("Kanban s4 fix: re-resolve card targets after commit; keep text on refused edit (B1, B2)"), on top of `cb786b0`.

Run on minis under `dbus-run-session`, with `WAYLAND_DISPLAY`, `WAYLAND_SOCKET`, `DISPLAY` and
`DBUS_SESSION_BUS_ADDRESS` unset, private 0700 `XDG_RUNTIME_DIR`, config, state and `TMPDIR` inside the
clone, the harness's private Xvfb and `--new-instance` for every windowed launch. wgpu backend: Vulkan on
lavapipe (`llvmpipe (LLVM 23.1.3, 256 bits)`, Mesa 26.2.5) for every step, through `UI_SMOKE_VK_ICD`,
`VK_DRIVER_FILES` and `VK_ICD_FILENAMES` for the whole run.

- `summary.txt`, `logs/`, `screens/`, `trees/`: the whole `scripts/ui-smoke.sh` harness, 36 of 36 steps PASS.
  On Linux the unsaved indicator now reads `Unsaved · Ctrl+S saves` (see `logs/headless_kanban_edit.log`).
- `ci/`: the CI commands from `.github/workflows/ci.yml` (fmt, clippy `-D warnings`, test, xtask layers,
  rustdoc `-D warnings`) on that head, all exit 0.
