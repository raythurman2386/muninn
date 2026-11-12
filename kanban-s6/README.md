# Kanban slice 6: dragging cards and lanes

Proof for `cursor/muninn-kanban-s6-drag` at PR head **34a5ae4c8effc5eeffccc7bfbe00047d9c6b73d2**, stacked on `cursor/muninn-kanban-s5-lanes` at `c48e8008d5544a5a0b865396ca9053294f100319` (#49).

## How it was run

The whole `scripts/ui-smoke.sh` harness ran on minis under `dbus-run-session`, on the working tree that was committed unchanged as `34a5ae4`:

- `WAYLAND_DISPLAY`, `WAYLAND_SOCKET`, `DISPLAY` and `DBUS_SESSION_BUS_ADDRESS` were unset.
- `XDG_RUNTIME_DIR` (private, 0700), config, state and `TMPDIR` were all inside the clone.
- Windowed launches used the harness's private Xvfb and `--new-instance`; throwaway vaults only.

wgpu backend for every step: Vulkan on lavapipe (`llvmpipe (LLVM 23.1.3, 256 bits)`, Mesa 26.2.5), set through `UI_SMOKE_VK_ICD`, `VK_DRIVER_FILES` and `VK_ICD_FILENAMES` for the whole run.

The x11 drags are real pointer drags made with XTEST (press, 12 motion steps, release), the mechanism xdotool uses; xdotool itself is not installed on minis, and no system package was installed for this slice.

## Contents

- `summary.txt`, `logs/`, `screens/`, `trees/`: the whole harness. All 40 steps pass on the first run, including the new `headless_kanban_drag` and `x11_kanban_drag`. No earlier failed run exists, so there is no `earlier-run/`.
- `logs/kanban-drag-saved.md`, `logs/x11-kanban-drag-saved.md`: the bytes each new step saved (equal to `scripts/ui-smoke/kanban-drag-saved.md` and `kanban-drag-x11.md`).
- `ci/local-ci.log`: the CI commands from `.github/workflows/ci.yml` on `34a5ae4` (fmt, clippy `-D warnings`, rustdoc `-D warnings`, xtask layers, 665 tests). All exit 0.

Screens for this slice: `kanban-drag-mid.png` (the bar between the tall card and the next), `kanban-drag-dropped.png`, `kanban-drag-cancel.png`, `kanban-lane-dragged.png`, and their `-x11` twins plus `kanban-drag-cancel-mid-x11.png` and `kanban-drag-saved-x11.png`.
