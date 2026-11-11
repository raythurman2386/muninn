#!/usr/bin/env bash
# UI smoke harness: launch the real Muninn binary headless, drive it, screenshot each
# step, and check the vault files afterwards. Linux only (built and run on minis).
#
#   scripts/ui-smoke.sh [OUT_DIR]            default OUT_DIR: target/ui-smoke
#   UI_SMOKE_STEPS="headless_modes" scripts/ui-smoke.sh    run a subset (names below)
#
# Everything runs against a throwaway HOME/XDG dirs and a throwaway vault under
# OUT_DIR/work (deleted on exit unless UI_SMOKE_KEEP=1). It never reads or writes your
# real settings or vault, never opens a window on your desktop (WAYLAND_DISPLAY is
# unset; windows go to a private Xvfb display), and never uses the network, except the
# opt-in lavapipe fetch below.
#
# Two layers:
#  1. headless_*: Rune's own offscreen tooling (`--script` input replay, `--snap` PNG,
#     `--tree` element JSON) on the app binary. Needs any wgpu Vulkan adapter (a GPU
#     render node or lavapipe); no display server.
#  2. x11_*: the normal windowed app (winit/X11) on Xvfb, keys injected with XTEST,
#     screenshots with ImageMagick `import`, closed through WM_DELETE_WINDOW. Xvfb has
#     no DRI3, so GPU Vulkan drivers cannot present there; this layer uses lavapipe,
#     Mesa's CPU Vulkan driver (Arch package `vulkan-swrast`). Found from, in order:
#     $UI_SMOKE_VK_ICD, /usr/share/vulkan/icd.d/lvp_icd*.json, or, with
#     UI_SMOKE_FETCH_LAVAPIPE=1, the vulkan-swrast package matching the installed mesa,
#     downloaded from the configured pacman mirror into OUT_DIR/tools, checked against
#     the pacman sync database's SHA-256, and unpacked there (no root, nothing
#     installed system-wide).
#
# Requirements: cargo, Xvfb (xorg-server-xvfb), python3 with python-xlib and Pillow, ImageMagick
# (`import`), sha256sum. Missing ones fail the step that needs them with the package name.
#
# Steps (UI_SMOKE_STEPS, space-separated, default all, in this order):
#   build           cargo build the app and the board_roundtrip example
#   seed            vault: two linked .md notes, Plan.excalidraw (the core test fixture)
#                   and Smoke.excalidraw (one element of each kind, for the pixel checks)
#   cli_check       `muninn --check`: 2 notes, 0 broken links (the board is not a note)
#   headless_modes  one input script (scripts/ui-smoke/modes.json) through Graph, Read,
#                   Write, Board by ⌘4, Esc out, Board by its mode button, a board row
#                   picked, Graph, Board by the command palette ("whiteboard"), Read,
#                   Write, Board again: a PNG and tree per mark, the active mode button
#                   asserted from each tree
#   headless_restart  enter Board, quit, relaunch with the same settings: the app opens
#                   in Board (asserted from the relaunch tree and settings.json)
#   x11_modes       windowed app on Xvfb: launch shot, Ctrl+3/2/1/4, a click on the Board
#                   button, the palette, each with a shot and settings.json "mode"
#                   asserted; clean close (exit 0) in Board, then a relaunch that must
#                   come back in Board and close cleanly again
#   headless_board  Board slice 3, offscreen: scripts/ui-smoke/board.json opens the smoke
#                   board (crates/muninn-core/tests/fixtures/board/smoke.excalidraw);
#                   board_pixels.py checks each shape's colour box against where the fit
#                   puts it, then that a drag moves it by the drag, three wheel notches
#                   and "+" scale it about the canvas centre, Left moves it 60 pt, a trip
#                   to Write and back leaves it in place, "0" refits it, and a relaunch on
#                   the saved settings draws it exactly where it was left
#   headless_big_restart  the same restart in a 160-note vault, which scans in chunks:
#                   the relaunch must still open Smoke at its saved view (pixels as
#                   "back") and settings.json must still name it afterwards
#   x11_board       the same checks in the windowed app on Xvfb with XTEST input (row
#                   click, button-1 drag, wheel buttons, "=", Left, Ctrl+1/4, a clean
#                   close and relaunch, "0")
#   board_untouched the app left the seeded boards byte-identical (it only reads boards);
#                   then the board_roundtrip example (load → compare → atomic save →
#                   reload, on a hidden scratch copy) passes on that file
# Later slices add board steps (enter Board mode, draw, select, move, undo, text, save,
# reload, round-trip the saved file): write a `step_NAME` function and add NAME to STEPS.
#
# Output: OUT_DIR/screens/*.png, OUT_DIR/trees/*.json, OUT_DIR/logs/*.log, and
# OUT_DIR/summary.txt (one PASS/FAIL line per step). Exit status is non-zero if any step
# failed.
set -uo pipefail

root="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
out="$(mkdir -p "${1:-$root/target/ui-smoke}" && CDPATH= cd -- "${1:-$root/target/ui-smoke}" && pwd)"
harness="$root/scripts/ui-smoke/harness.py"
STEPS="${UI_SMOKE_STEPS:-build seed cli_check headless_modes headless_restart headless_board headless_big_restart x11_modes x11_board board_untouched}"

rm -rf "$out/screens" "$out/trees" "$out/logs" "$out/work" "$out/summary.txt"
mkdir -p "$out/screens" "$out/trees" "$out/logs"
work="$out/work"
vault="$work/vault"
bin="$root/target/debug/muninn"
roundtrip="$root/target/debug/examples/board_roundtrip"
board="$vault/Plan.excalidraw"
fixture="$root/crates/muninn-core/tests/fixtures/board/scene.excalidraw"
smoke_board="$vault/Smoke.excalidraw"
smoke_fixture="$root/crates/muninn-core/tests/fixtures/board/smoke.excalidraw"
pixels="$root/scripts/ui-smoke/board_pixels.py"

# Build with the real cargo home before HOME is isolated (as settings-tab-snapshots.sh).
real_home="$HOME"
export CARGO_HOME="${CARGO_HOME:-$real_home/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$real_home/.rustup}"

isolate() {
  export HOME="$work/home"
  export XDG_CONFIG_HOME="$work/config" XDG_STATE_HOME="$work/state"
  export XDG_CACHE_HOME="$work/cache" XDG_DATA_HOME="$work/data"
  unset MUNINN_VAULT MUNINN_REDUCED_MOTION WAYLAND_DISPLAY WAYLAND_SOCKET DISPLAY || true
  mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME"
}

cleanup() {
  [ -n "${app_pid:-}" ] && kill "$app_pid" 2>/dev/null
  [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid" 2>/dev/null
  [ "${UI_SMOKE_KEEP:-0}" = 1 ] || rm -rf "$work"
}
trap cleanup EXIT INT TERM

need() { # need COMMAND PACKAGE
  command -v "$1" >/dev/null 2>&1 || { echo "missing $1 (install $2)"; return 1; }
}

step_build() {
  cargo build --locked -p muninn-app --manifest-path "$root/Cargo.toml" &&
    cargo build --locked -p muninn-core --example board_roundtrip --manifest-path "$root/Cargo.toml"
}

step_seed() {
  isolate
  mkdir -p "$vault"
  printf '# Welcome\n\nThis vault is a UI smoke fixture. See [[Second]] and the Plan board.\n' >"$vault/Welcome.md"
  printf '# Second\n\nBack to [[Welcome]].\n\n- one\n- two\n' >"$vault/Second.md"
  cp "$fixture" "$board"
  cp "$smoke_fixture" "$smoke_board"
  sha256sum "$board" "$smoke_board" | tee "$work/board.sha256"
  find "$vault" -type f | sort
}

step_cli_check() {
  isolate
  local report
  report="$("$bin" --check "$vault")" || return 1
  echo "$report"
  grep -qx 'notes: 2' <<<"$report" && grep -qx 'broken links: 0' <<<"$report"
}

# mark in modes.json → the mode it must show
HEADLESS_MARKS="graph:graph preview:preview editor:editor board-shortcut:board board-esc:editor
board-button:board board-selected:board graph-after:graph board-palette:board
preview-after:preview editor-after:editor end-in-board:board"

step_headless_modes() {
  isolate
  command -v vulkaninfo >/dev/null && vulkaninfo --summary 2>/dev/null | grep -E 'deviceName|driverName'
  local pair mark mode
  for pair in $HEADLESS_MARKS; do
    mark="${pair%%:*}" mode="${pair#*:}"
    "$bin" --vault "$vault" --script "$root/scripts/ui-smoke/modes.json" \
      --at "mark:$mark+0.9" --scale 1 \
      --snap "$out/screens/headless-$mark.png" --tree "$out/trees/headless-$mark.json" || return 1
    python3 "$harness" tree-mode "$out/trees/headless-$mark.json" "$mode" || return 1
  done
}

step_headless_restart() {
  isolate
  # Its own settings, so the earlier steps' last mode cannot make this pass.
  export XDG_CONFIG_HOME="$work/config-restart"
  mkdir -p "$XDG_CONFIG_HOME"
  printf '{"events":[{"t":0.5,"type":"key","name":"4","cmd":true}]}' >"$work/enter-board.json"
  "$bin" --vault "$vault" --script "$work/enter-board.json" --at 1.2 --scale 1 \
    --snap "$out/screens/restart-1-enter-board.png" || return 1
  python3 "$harness" settings-mode "$XDG_CONFIG_HOME/muninn/settings.json" board || return 1
  "$bin" --vault "$vault" --at 1.0 --scale 1 \
    --snap "$out/screens/restart-2-relaunch.png" --tree "$out/trees/restart-relaunch.json" || return 1
  python3 "$harness" tree-mode "$out/trees/restart-relaunch.json" board
}

# Board slice 3: the smoke board drawn, panned, zoomed, kept across a mode switch
# and a restart, and refitted. Each mark is its own run with fresh settings, as the
# script starts from a first launch; board_pixels.py checks every screenshot.
BOARD_MARKS="fit panned wheeled plus arrow write back refit"

board_snap() { # board_snap MARK: replay board.json to MARK with its own settings
  export XDG_CONFIG_HOME="$work/config-board-$1"
  rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
  "$bin" --vault "$vault" --script "$root/scripts/ui-smoke/board.json" \
    --at "mark:$1+0.2" --scale 1 \
    --snap "$out/screens/board-$1.png" --tree "$out/trees/board-$1.json"
}

step_headless_board() {
  isolate
  python3 -c 'import PIL' 2>/dev/null || { echo "missing python-pillow"; return 1; }
  local mark
  for mark in $BOARD_MARKS; do board_snap "$mark" || return 1; done
  local s="$out/screens" t="$out/trees" m="$work/measure"
  mkdir -p "$m"
  python3 "$harness" tree-mode "$t/board-fit.json" board || return 1
  python3 "$pixels" fit "$s/board-fit.png" "$t/board-fit.json" "$smoke_fixture" "$m/fit.json" || return 1
  python3 "$pixels" moved "$s/board-panned.png" "$t/board-panned.json" "$m/fit.json" 120 60 "$m/panned.json" || return 1
  # Three wheel notches at the canvas centre, 1.08 each (board_camera.rs WHEEL_IN).
  python3 "$pixels" zoomed "$s/board-wheeled.png" "$t/board-wheeled.json" "$m/panned.json" centre centre 1.259712 "$m/wheeled.json" || return 1
  python3 "$pixels" zoomed "$s/board-plus.png" "$t/board-plus.json" "$m/wheeled.json" centre centre 1.25 "$m/plus.json" || return 1
  python3 "$pixels" moved "$s/board-arrow.png" "$t/board-arrow.json" "$m/plus.json" 60 0 "$m/arrow.json" || return 1
  python3 "$harness" tree-mode "$t/board-write.json" editor || return 1
  python3 "$pixels" same "$s/board-back.png" "$t/board-back.json" "$m/arrow.json" "$m/back.json" || return 1
  python3 "$pixels" same "$s/board-refit.png" "$t/board-refit.json" "$m/fit.json" "$m/refit.json" || return 1
  # Restart: the run that stopped at "back" saved Board, Smoke and its view; a plain
  # relaunch on those settings must draw the board exactly there.
  export XDG_CONFIG_HOME="$work/config-board-back"
  python3 "$harness" settings-mode "$XDG_CONFIG_HOME/muninn/settings.json" board || return 1
  python3 -c 'import json,sys; b=json.load(open(sys.argv[1]))["board"]; print("saved board", b); assert b["rel"]=="Smoke.excalidraw" and "eye" in b' \
    "$XDG_CONFIG_HOME/muninn/settings.json" || return 1
  "$bin" --vault "$vault" --at 1.5 --scale 1 \
    --snap "$s/board-restart.png" --tree "$t/board-restart.json" || return 1
  python3 "$harness" tree-mode "$t/board-restart.json" board || return 1
  python3 "$pixels" same "$s/board-restart.png" "$t/board-restart.json" "$m/back.json" "$m/restart.json"
}

# #37 M1: a vault over the synchronous scan limit (100 notes) scans in chunks after
# launch; finishing that scan must keep the restored board and its view.
step_headless_big_restart() {
  isolate
  local big="$work/big-vault" s="$out/screens" t="$out/trees" m="$work/measure" i
  [ -f "$m/back.json" ] || { echo "run headless_board first (needs its back.json)"; return 1; }
  rm -rf "$big"; mkdir -p "$big/notes"
  for i in $(seq -w 1 160); do printf '# Note %s\n' "$i" >"$big/notes/n$i.md"; done
  cp "$smoke_fixture" "$big/Smoke.excalidraw"
  export XDG_CONFIG_HOME="$work/config-big"
  rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME/muninn"
  # The headless_board "back" settings (Board, Smoke at its view), pointed at this vault:
  # a board is only restored into the vault it was saved from.
  python3 -c 'import json,sys; v=json.load(open(sys.argv[1])); v["last_vault"]=sys.argv[3]; v["recent_vaults"]=[sys.argv[3]]; json.dump(v, open(sys.argv[2], "w"), indent=2)' \
    "$work/config-board-back/muninn/settings.json" "$XDG_CONFIG_HOME/muninn/settings.json" "$big" || return 1
  "$bin" --vault "$big" --at 2.5 --scale 1 \
    --snap "$s/big-restart.png" --tree "$t/big-restart.json" || return 1
  python3 "$harness" tree-mode "$t/big-restart.json" board || return 1
  python3 "$pixels" same "$s/big-restart.png" "$t/big-restart.json" "$m/back.json" "$m/big-restart.json" || return 1
  python3 -c 'import json,sys; b=json.load(open(sys.argv[1]))["board"]; print("saved board after the scan", b); assert b and b["rel"]=="Smoke.excalidraw" and "eye" in b' \
    "$XDG_CONFIG_HOME/muninn/settings.json"
}

find_lavapipe() {
  if [ -n "${UI_SMOKE_VK_ICD:-}" ]; then echo "$UI_SMOKE_VK_ICD"; return; fi
  local icd
  for icd in /usr/share/vulkan/icd.d/lvp_icd*.json; do
    [ -f "$icd" ] && { echo "$icd"; return; }
  done
  [ "${UI_SMOKE_FETCH_LAVAPIPE:-0}" = 1 ] || return 1
  fetch_lavapipe
}

fetch_lavapipe() { # user-level copy of vulkan-swrast matching the installed mesa
  need pacman pacman >&2 || return 1
  local version file mirror tools="$out/tools" want got
  version="$(pacman -Q mesa | awk '{print $2}')" || return 1
  file="vulkan-swrast-$version-x86_64.pkg.tar.zst"
  want="$(tar -xOf /var/lib/pacman/sync/extra.db "vulkan-swrast-$version/desc" | awk '/%SHA256SUM%/{getline; print}')"
  [ -n "$want" ] || { echo "vulkan-swrast $version is not in the extra sync db" >&2; return 1; }
  mkdir -p "$tools/root"
  if [ ! -f "$tools/$file" ]; then
    mirror="$(sed -n 's/^Server *= *//p' /etc/pacman.d/mirrorlist | head -1 | sed 's/\$repo/extra/; s/\$arch/x86_64/')"
    curl -sfL -o "$tools/$file" "$mirror/${file//:/%3A}" || { echo "download failed" >&2; return 1; }
  fi
  got="$(sha256sum "$tools/$file" | awk '{print $1}')"
  [ "$got" = "$want" ] || { echo "sha256 mismatch for $file" >&2; rm -f "$tools/$file"; return 1; }
  tar -C "$tools/root" -xf "$tools/$file" usr/lib/libvulkan_lvp.so usr/share/vulkan/icd.d/lvp_icd.json
  python3 - "$tools" <<'PY'
import json, sys
tools = sys.argv[1]
icd = json.load(open(f"{tools}/root/usr/share/vulkan/icd.d/lvp_icd.json"))
icd["ICD"]["library_path"] = f"{tools}/root/usr/lib/libvulkan_lvp.so"
json.dump(icd, open(f"{tools}/lvp_icd.json", "w"), indent=2)
PY
  echo "fetched $file (sha256 $got) into $tools" >&2
  echo "$tools/lvp_icd.json"
}

step_x11_modes() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib' 2>/dev/null || { echo "missing python-xlib"; return 1; }
  local icd
  icd="$(find_lavapipe)" || {
    echo "missing lavapipe: install vulkan-swrast, or set UI_SMOKE_VK_ICD, or UI_SMOKE_FETCH_LAVAPIPE=1"
    return 1
  }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  # Steps run in a subshell, so this fires when the step ends, pass or fail.
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  echo "vulkan icd: $icd"
  command -v vulkaninfo >/dev/null && vulkaninfo --summary 2>/dev/null | grep -E 'deviceName|driverName'

  # A private display well above the ones a desktop session (Xwayland) uses.
  local n
  for n in $(seq 90 130); do [ -e "/tmp/.X$n-lock" ] || [ -e "/tmp/.X11-unix/X$n" ] || break; done
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY"

  local settings="$XDG_CONFIG_HOME/muninn/settings.json" wid
  launch() { # launch LOG: start the windowed app and wait for its window
    "$bin" --vault "$vault" >>"$out/logs/$1" 2>&1 &
    app_pid=$!
    wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
    sleep 2
  }
  shot() { sleep 1.5; import -window "$wid" "$out/screens/x11-$1.png"; }
  expect() { python3 "$harness" settings-mode "$settings" "$1"; }
  chord() { python3 "$harness" x11-chord "$wid" "$1"; }
  close_app() { # close_app: WM_DELETE_WINDOW, then require exit status 0
    python3 "$harness" x11-close "$wid" || return 1
    local i status=124
    for i in $(seq 100); do
      if ! kill -0 "$app_pid" 2>/dev/null; then wait "$app_pid"; status=$?; break; fi
      sleep 0.1
    done
    app_pid=
    echo "app exit status: $status"
    [ "$status" = 0 ]
  }

  # Fresh settings, so the launch is a first run (Write) whatever ran before.
  export XDG_CONFIG_HOME="$work/config-x11"
  rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
  settings="$XDG_CONFIG_HOME/muninn/settings.json"
  launch x11-app.log || return 1
  import -window "$wid" "$out/screens/x11-0-launch.png" || return 1

  chord ctrl+3 && shot 1-graph && expect graph || return 1
  chord ctrl+2 && shot 2-read && expect preview || return 1
  chord ctrl+1 && shot 3-write && expect editor || return 1
  chord ctrl+4 && shot 4-board-shortcut && expect board || return 1
  chord esc && shot 5-board-esc && expect editor || return 1
  # The Board button's centre, from the headless tree (same 1180x780 window, scale 1).
  local tree="$out/trees/headless-editor.json" xy
  [ -f "$tree" ] || { echo "run headless_modes first (needs $tree)"; return 1; }
  xy="$(python3 "$harness" tree-center "$tree" mode-board)" || return 1
  python3 "$harness" x11-click "$wid" $xy && shot 6-board-button && expect board || return 1
  chord ctrl+3 && shot 7-graph && expect graph || return 1
  chord ctrl+shift+p && python3 "$harness" x11-type "$wid" whiteboard && sleep 0.5 &&
    chord enter && shot 8-board-palette && expect board || return 1
  chord ctrl+2 && shot 9-read && expect preview || return 1
  chord ctrl+1 && shot 10-write && expect editor || return 1
  chord ctrl+4 && shot 11-board-before-quit && expect board || return 1
  close_app || return 1

  # Relaunch: the last mode (Board) is restored.
  launch x11-app-relaunch.log || return 1
  import -window "$wid" "$out/screens/x11-12-relaunch-board.png" || return 1
  expect board || return 1
  close_app || return 1
  cp "$settings" "$out/logs/x11-settings.json" 2>/dev/null
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

# The same board checks in the windowed app on Xvfb, with real XTEST input: a click
# on the board row, a button-1 drag, wheel notches (button 5, which zooms in as in
# Graph), "=", Left, Ctrl+1/4,
# a clean close and a relaunch that must land on the same view, then "0".
step_x11_board() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local tree="$out/trees/board-fit.json"
  [ -f "$tree" ] || { echo "run headless_board first (needs $tree)"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n
  for n in $(seq 90 130); do [ -e "/tmp/.X$n-lock" ] || [ -e "/tmp/.X11-unix/X$n" ] || break; done
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-board.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  export XDG_CONFIG_HOME="$work/config-x11-board"
  rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
  local wid s="$out/screens" m="$work/measure-x11"
  mkdir -p "$m"
  launch() {
    "$bin" --vault "$vault" >>"$out/logs/x11-board-app.log" 2>&1 &
    app_pid=$!
    wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
    sleep 2
  }
  shot() { sleep 1.5; import -window "$wid" "$s/x11-board-$1.png"; }
  close_app() {
    python3 "$harness" x11-close "$wid" || return 1
    local i status=124
    for i in $(seq 100); do
      if ! kill -0 "$app_pid" 2>/dev/null; then wait "$app_pid"; status=$?; break; fi
      sleep 0.1
    done
    app_pid=
    echo "app exit status: $status"
    [ "$status" = 0 ]
  }
  local row cx cy
  row="$(python3 "$harness" tree-center "$tree" board-row-1)" || return 1
  read -r cx cy < <(python3 "$harness" tree-center "$tree" board-canvas) || return 1
  launch || return 1
  # Let the Board page finish appearing first: a click during its entrance can miss
  # the row (seen once in about ten runs).
  python3 "$harness" x11-chord "$wid" ctrl+4 && sleep 1 || return 1
  python3 "$harness" x11-click "$wid" $row && shot 1-fit || return 1
  python3 "$pixels" fit "$s/x11-board-1-fit.png" "$tree" "$smoke_fixture" "$m/fit.json" || return 1
  python3 "$harness" x11-drag "$wid" "$cx" "$cy" 120 60 && shot 2-panned || return 1
  python3 "$pixels" moved "$s/x11-board-2-panned.png" "$tree" "$m/fit.json" 120 60 "$m/panned.json" || return 1
  python3 "$harness" x11-wheel "$wid" "$cx" "$cy" 5 1 && shot 3-wheeled || return 1
  python3 "$pixels" zoomed "$s/x11-board-3-wheeled.png" "$tree" "$m/panned.json" "$cx" "$cy" steps:1.08:1:2 "$m/wheeled.json" || return 1
  python3 "$harness" x11-chord "$wid" equal && shot 4-plus || return 1
  python3 "$pixels" zoomed "$s/x11-board-4-plus.png" "$tree" "$m/wheeled.json" centre centre 1.25 "$m/plus.json" || return 1
  python3 "$harness" x11-chord "$wid" Left && shot 5-arrow || return 1
  python3 "$pixels" moved "$s/x11-board-5-arrow.png" "$tree" "$m/plus.json" 60 0 "$m/arrow.json" || return 1
  python3 "$harness" x11-chord "$wid" ctrl+1 && shot 6-write || return 1
  python3 "$harness" settings-mode "$XDG_CONFIG_HOME/muninn/settings.json" editor || return 1
  python3 "$harness" x11-chord "$wid" ctrl+4 && shot 7-back || return 1
  python3 "$pixels" same "$s/x11-board-7-back.png" "$tree" "$m/arrow.json" "$m/back.json" || return 1
  close_app || return 1
  launch || return 1
  shot 8-relaunch || return 1
  python3 "$harness" settings-mode "$XDG_CONFIG_HOME/muninn/settings.json" board || return 1
  python3 "$pixels" same "$s/x11-board-8-relaunch.png" "$tree" "$m/back.json" "$m/relaunch.json" || return 1
  python3 "$harness" x11-chord "$wid" 0 && shot 9-refit || return 1
  python3 "$pixels" same "$s/x11-board-9-refit.png" "$tree" "$m/fit.json" "$m/refit.json" || return 1
  close_app || return 1
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

step_board_untouched() {
  sha256sum -c "$work/board.sha256" || { echo "the app changed the board file"; return 1; }
  "$roundtrip" "$board" || return 1
  # The checker's scratch save is hidden and removed; nothing else appears in the vault.
  find "$vault" -name '.roundtrip-*' | grep . && { echo "scratch file left behind"; return 1; }
  return 0
}

failed=0
for name in $STEPS; do
  log="$out/logs/$name.log"
  if ! declare -F "step_$name" >/dev/null; then
    echo "FAIL $name (no such step)" | tee -a "$out/summary.txt"; failed=1; continue
  fi
  start=$(date +%s)
  if ( "step_$name" ) >"$log" 2>&1; then result=PASS; else result=FAIL; failed=1; fi
  echo "$result $name ($(( $(date +%s) - start ))s) logs/$name.log" | tee -a "$out/summary.txt"
done
exit "$failed"
