# Kanban slice 5, fix round 1

This is the proof for `cursor/muninn-kanban-s5-lanes` at PR head **e492fdd93808dcc82f729e6f851b24a057b4f969**: "Kanban s5 fix: never drop note text on lane delete; guard lane ops in open fences (B1, F1)". It is a fast-forward on `c48e800`, stacked on s4 `5d153039723605c2513f22a2b26ee585b7732f69`.

## What changed in this round

- **B1:** Delete lane keeps the lane's note text: paragraphs, headings and comments.
  - Only the title, the done marker and the cards are removed.
  - When the text can't stay, the lane asks first and counts and quotes what goes.
- **F1:** lane add and lane delete are refused inside an open fence.
- Nits N1–N4, N7, N9–N11, plus tests for the M04 and M22 mutations.

The two smoke expected files each lose one blank line, from N11.

## How it was run

The harness ran on minis under `dbus-run-session`:

- `WAYLAND_DISPLAY`, `WAYLAND_SOCKET`, `DISPLAY` and `DBUS_SESSION_BUS_ADDRESS` were unset.
- `XDG_RUNTIME_DIR` (private, 0700), config, state and `TMPDIR` were all inside the clone.
- Windowed launches used the harness's private Xvfb and `--new-instance`.

wgpu backend for every step: Vulkan on lavapipe (`llvmpipe (LLVM 23.1.3, 256 bits)`, Mesa 26.2.5). It was set through `UI_SMOKE_VK_ICD`, `VK_DRIVER_FILES` and `VK_ICD_FILENAMES` for the whole run.

## Contents

- `summary.txt`, `logs/`, `screens/`, `trees/`: the whole `scripts/ui-smoke.sh` harness on head `e492fdd`. All 38 steps pass.
- `ci/local-ci.log`: the CI commands from `.github/workflows/ci.yml` on that head. All exit 0.
- `earlier-run/`: the first whole-harness run on this head, for the record.
  - 37 steps passed. `x11_edit` failed its first Board-mode pixel check: no rectangle fill had been drawn yet.
  - Board mode is untouched by this commit, and minis had a load average of about 25 to 30 at the time.
  - A second run was cut off by an interruption before it got through the harness.
  - The run above is a rerun, with no step changed.
