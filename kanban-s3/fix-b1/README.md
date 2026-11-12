# Kanban slice 3, fix round 1 (B1, N-b)

Proof for `cursor/muninn-kanban-s3-open` at head **ae05869786ed07321f017986171e7a4e44c90432**
("Kanban s3 fix: Cmd+K search opens kanbans in Kanban mode (B1)"), on top of `ea747a2`.

Run on minis under `dbus-run-session`, with `WAYLAND_DISPLAY`, `WAYLAND_SOCKET`, `DISPLAY` and
`DBUS_SESSION_BUS_ADDRESS` unset, private 0700 `XDG_RUNTIME_DIR`, config, state and `TMPDIR` inside
the clone, a private Xvfb and `--new-instance` for every windowed launch. wgpu backend: Vulkan on
lavapipe (`llvmpipe (LLVM 23.1.3, 256 bits)`, Mesa 26.2.5) for every step, through
`UI_SMOKE_VK_ICD`, `VK_DRIVER_FILES` and `VK_ICD_FILENAMES` for the whole run.

- `summary.txt`, `logs/`, `screens/`, `trees/`: the whole `scripts/ui-smoke.sh` harness, every step PASS.
- `screens/kanban-open-search.png`: B1 in the windowed app, Ctrl+K, "release", Enter: Kanban mode on Release.
- `screens/kanban-links.png`: N-b, the card `[[Nowhere]]` (no such note) dimmed, `[[Agenda]]` in the accent
  (`logs/headless_kanban.log`: ink (118, 134, 176) vs (102, 102, 102)).
- `ci/negative-control-headless_kanban.log`: the same check with `.link_style(missing_link_style)` removed
  from the kanban card fails ("the link to no note is not dimmed", ink (116, 132, 172)).
- `ci/`: the CI commands from `.github/workflows/ci.yml` (fmt, clippy `-D warnings`, test, xtask layers,
  rustdoc `-D warnings`), all exit 0. `ci/test-run1-flake.log` is a first test run in which the untouched
  `instance::tests::a_dropped_primary_frees_the_lock` (single-instance lock) failed once with `NoAnswer`
  under load; the rerun (`ci/test.log`) passed every test.
