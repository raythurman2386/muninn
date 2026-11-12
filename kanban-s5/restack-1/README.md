# Kanban slice 5, restack 1: on top of the s4 fix round

This is the proof for `cursor/muninn-kanban-s5-lanes` at PR head **c48e8008d5544a5a0b865396ca9053294f100319**.

- **Base:** the branch was rebased onto `cursor/muninn-kanban-s4-edit` at `5d153039723605c2513f22a2b26ee585b7732f69`, the s4 fix round for B1 and B2.
- **Earlier proof:** the files one level up (`kanban-s5/`) are for the earlier head `bf1f356`, which sat on `cb786b0`.

## Changes in this restack

The lane, move, sort and collapse actions now follow slice 4's rules:

- **Commit first.** An open card editor or lane field is committed first.
- **Re-resolve the target.** After the commit, the action finds its target again:
  - a card added at or above the target card moves it down one;
  - a lane added at or before the target lane moves it right one.
- **Refused commits.** If the commit is refused, the editor keeps its text, the reason shows on the page, and the action waits.

## How it was run

The harness ran on minis under `dbus-run-session`:

- `WAYLAND_DISPLAY`, `WAYLAND_SOCKET`, `DISPLAY` and `DBUS_SESSION_BUS_ADDRESS` were unset.
- `XDG_RUNTIME_DIR` (private, 0700), config, state and `TMPDIR` were all inside the clone.
- Windowed launches used the harness's private Xvfb and `--new-instance`.

wgpu backend for every step: Vulkan on lavapipe (`llvmpipe (LLVM 23.1.3, 256 bits)`, Mesa 26.2.5). It was set through `UI_SMOKE_VK_ICD`, `VK_DRIVER_FILES` and `VK_ICD_FILENAMES` for the whole run.

## Contents

- `summary.txt`, `logs/`, `screens/`, `trees/`: the whole `scripts/ui-smoke.sh` harness on head `c48e800`. All 38 steps pass.
- `ci/local-ci.log`: the CI commands from `.github/workflows/ci.yml` on that head. All exit 0.
- `earlier-run/`: the first whole-harness run on this head, kept for the record. 37 steps passed and `x11_board` failed one Board-mode pixel check (a rectangle position). Board mode is untouched by slice 5. The failure happened while minis was under heavy load from another smoke run that was not ours. The rerun above passes every step, with no step changed.
