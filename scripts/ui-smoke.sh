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
#                   and Smoke.excalidraw (one element of each kind, for the pixel checks),
#                   and Tools.excalidraw (the board the draw steps draw on)
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
#   headless_big_write  the same 160-note vault relaunched in Write: the scan ends, Cmd+2
#                   writes settings, and settings.json must still name Smoke and its
#                   view (the board is restored, not yet drawn); then Cmd+4 draws it
#                   at that view
#   x11_board       the same checks in the windowed app on Xvfb with XTEST input (row
#                   click, button-1 drag, wheel buttons, "=", Left, Ctrl+1/4, a clean
#                   close and relaunch, "0")
#   headless_draw   Board slice 4, offscreen: draws a rectangle, ellipse, diamond, line,
#                   arrow and freehand stroke on Tools.excalidraw (fixture draw.excalidraw)
#                   with tool letters and pointer strokes, checks the ink per tool
#                   (board_draw.py pixels: edges, insides, ellipse vs diamond, the
#                   arrowhead, nothing else changed), saves with Cmd+S, checks the file
#                   (board_draw.py file: Excalidraw fields, geometry through the fit
#                   camera, old elements value- and byte-identical to s1's save, then the
#                   s1 round-trip), and relaunches onto the saved drawing
#   x11_draw        the same on Xvfb with XTEST, also proving the file is written by
#                   Ctrl+S and not before
#   headless_board_safety  #38 review, offscreen: typing "road1x+0" in the focused
#                   sidebar search on Board stays in the search (no tool, no zoom: the
#                   canvas is unchanged); a seeded SAF-05 recovery copy of a removed
#                   Tools board is listed by the launch banner, Restore opens it on Board
#                   flagged as removed (its extra rectangle drawn at the fit), and Keep
#                   mine writes it back (the copy then goes); Save copy writes
#                   "Tools (conflict …).excalidraw"; over a file that holds something
#                   else it is flagged as changed and Load theirs keeps their file
#   x11_board_safety the same with real XTEST input: draw, remove the file, Ctrl+1/4 keeps
#                   the drawing, Keep mine writes it; draw, another app rewrites the file,
#                   closing exits 0 with a recovery copy and their file untouched; the
#                   relaunch banner restores it flagged, Save copy writes it; and with the
#                   recovery folder read-only a close is refused (the app stays, the window
#                   comes back) until the folder is writable again
#   board_untouched the app left the boards it did not edit (Plan, Smoke) byte-identical,
#                   and no hidden temp file in the vault (it only reads boards);
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
STEPS="${UI_SMOKE_STEPS:-build seed cli_check headless_modes headless_restart headless_board headless_big_restart headless_big_write headless_draw headless_board_safety x11_modes x11_board x11_draw x11_board_safety board_untouched}"

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
draw="$root/scripts/ui-smoke/board_draw.py"
safety="$root/scripts/ui-smoke/board_safety.py"
draw_board="$vault/Tools.excalidraw"
draw_fixture="$root/crates/muninn-core/tests/fixtures/board/draw.excalidraw"
resave="$root/target/debug/examples/board_resave"
# The checkers import each other; keep __pycache__ out of the tree.
export PYTHONDONTWRITEBYTECODE=1

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
    cargo build --locked -p muninn-core --example board_roundtrip --example board_resave \
      --manifest-path "$root/Cargo.toml"
}

step_seed() {
  isolate
  mkdir -p "$vault"
  printf '# Welcome\n\nThis vault is a UI smoke fixture. See [[Second]] and the Plan board.\n' >"$vault/Welcome.md"
  printf '# Second\n\nBack to [[Welcome]].\n\n- one\n- two\n' >"$vault/Second.md"
  cp "$fixture" "$board"
  cp "$smoke_fixture" "$smoke_board"
  # Tools is the board the draw steps edit; board_untouched checks the other two.
  cp "$draw_fixture" "$draw_board"
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

# #37 review: the same big vault relaunched in Write. The restored board is not
# drawn, the scan ends, and a settings write must still keep it.
step_headless_big_write() {
  isolate
  local big="$work/big-vault" s="$out/screens" t="$out/trees" m="$work/measure"
  [ -f "$m/back.json" ] && [ -d "$big" ] || { echo "run headless_big_restart first"; return 1; }
  export XDG_CONFIG_HOME="$work/config-big-write"
  rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME/muninn"
  python3 -c 'import json,sys; v=json.load(open(sys.argv[1])); v["last_vault"]=sys.argv[3]; v["recent_vaults"]=[sys.argv[3]]; v["mode"]="editor"; json.dump(v, open(sys.argv[2], "w"), indent=2)' \
    "$work/config-board-back/muninn/settings.json" "$XDG_CONFIG_HOME/muninn/settings.json" "$big" || return 1
  local want
  want="$(python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))["board"], sort_keys=True))' "$XDG_CONFIG_HOME/muninn/settings.json")" || return 1
  printf '{"events":[{"t":2.5,"type":"key","name":"2","cmd":true},{"t":3.0,"type":"mark","name":"written"},{"t":3.4,"type":"key","name":"4","cmd":true},{"t":4.6,"type":"mark","name":"board"}]}' \
    >"$work/big-write.json"
  "$bin" --vault "$big" --script "$work/big-write.json" --at "mark:written+0.2" --scale 1 \
    --tree "$t/big-write.json" || return 1
  python3 "$harness" settings-mode "$XDG_CONFIG_HOME/muninn/settings.json" preview || return 1
  python3 -c 'import json,sys; b=json.load(open(sys.argv[1])).get("board"); print("saved board after the scan in Write", b); assert b is not None and json.dumps(b, sort_keys=True)==sys.argv[2], "the board to restore was forgotten"' \
    "$XDG_CONFIG_HOME/muninn/settings.json" "$want" || return 1
  "$bin" --vault "$big" --script "$work/big-write.json" --at "mark:board+0.2" --scale 1 \
    --snap "$s/big-write-board.png" --tree "$t/big-write-board.json" || return 1
  python3 "$harness" tree-mode "$t/big-write-board.json" board || return 1
  python3 "$pixels" same "$s/big-write-board.png" "$t/big-write-board.json" "$m/back.json" "$m/big-write.json"
}

# Board slice 4: draw one element with each tool on Tools.excalidraw by injected
# pointer and key input, check the ink with board_draw.py, save with Cmd+S, check
# the file (new elements' fields and geometry, old elements value- and
# byte-identical to s1's own save, s1 round-trip), and relaunch onto it.
draw_run() { # draw_run NAME SCRIPT MARK: a fresh Tools board and settings, run to MARK
  cp "$draw_fixture" "$draw_board"
  # Its own state too: a dirty board keeps a recovery copy there (#38 review B2),
  # and a later run would list it in the launch banner.
  export XDG_CONFIG_HOME="$work/config-draw-$1" XDG_STATE_HOME="$work/state-draw-$1"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  "$bin" --vault "$vault" --script "$2" --at "mark:$3+0.25" --scale 1 \
    --snap "$out/screens/draw-$1.png" --tree "$out/trees/draw-$1.json"
}

step_headless_draw() {
  isolate
  python3 -c 'import PIL' 2>/dev/null || { echo "missing python-pillow"; return 1; }
  local s="$out/screens" t="$out/trees" m="$work/draw"
  mkdir -p "$m"
  printf '{"events":[{"t":0.3,"type":"key","name":"4","cmd":true},{"t":0.8,"type":"tap","key":"board-row-2"},{"t":2.0,"type":"mark","name":"before"}]}' \
    >"$m/open.json"
  draw_run before "$m/open.json" before || return 1
  python3 "$harness" tree-text "$t/draw-before.json" board-surface-title Tools || return 1
  python3 "$draw" script "$t/draw-before.json" "$m/draw.json" || return 1
  draw_run drawn "$m/draw.json" drawn || return 1
  python3 "$harness" tree-text "$t/draw-drawn.json" board-save-state Unsaved || return 1
  python3 "$draw" pixels "$s/draw-before.png" "$s/draw-drawn.png" "$t/draw-before.json" || return 1
  draw_run saved "$m/draw.json" saved || return 1
  python3 "$harness" tree-text "$t/draw-saved.json" board-save-state Saved || return 1
  cp "$draw_board" "$out/logs/draw-saved.excalidraw"
  "$resave" "$draw_fixture" "$m/resaved.excalidraw" || return 1
  python3 "$draw" file "$draw_board" "$draw_fixture" "$m/resaved.excalidraw" "$t/draw-before.json" || return 1
  "$roundtrip" "$draw_board" || return 1
  # Relaunch on the saved run's settings: Board, Tools and its view come back and
  # the drawing is read from the file.
  export XDG_CONFIG_HOME="$work/config-draw-saved"
  "$bin" --vault "$vault" --at 1.5 --scale 1 \
    --snap "$s/draw-reloaded.png" --tree "$t/draw-reloaded.json" || return 1
  python3 "$harness" tree-text "$t/draw-reloaded.json" board-surface-title Tools || return 1
  python3 "$draw" same "$s/draw-drawn.png" "$s/draw-reloaded.png" "$t/draw-before.json"
}

must_fail() { # must_fail LABEL COMMAND...: a negative control
  if "${@:2}" >"$work/negative.log" 2>&1; then
    echo "negative control passed but must fail: $1"; cat "$work/negative.log"; return 1
  fi
  echo "ok negative control fails: $1 ($(grep -m1 FAIL "$work/negative.log" || tail -1 "$work/negative.log"))"
}

no_key() { # no_key TREE KEY: the tree has no element KEY
  ! python3 "$harness" tree-center "$1" "$2" >/dev/null 2>&1 || { echo "$1 still has $2"; return 1; }
  echo "ok $1: no $2"
}

# #38 review: a run on Tools with its own config and state, to MARK.
safety_run() { # safety_run NAME SCRIPT MARK
  export XDG_CONFIG_HOME="$work/config-safety-$1"
  rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
  "$bin" --vault "$vault" --script "$2" --at "mark:$3+0.25" --scale 1 \
    --snap "$out/screens/safety-$1.png" --tree "$out/trees/safety-$1.json"
}

# Fresh state with a seeded recovery copy of Tools; THEIRS: "gone" removes the
# file, "fixture" leaves the fixture there (it then differs from the copy).
seed_safety() { # seed_safety THEIRS
  export XDG_STATE_HOME="$work/state-safety"
  rm -rf "$XDG_STATE_HOME"; mkdir -p "$XDG_STATE_HOME"
  cp "$draw_fixture" "$draw_board"
  [ "$1" = gone ] && rm -f "$draw_board"
  python3 "$safety" seed-recovery "$draw_fixture" "$vault" "$XDG_STATE_HOME" Tools.excalidraw "$work/safety/copy.json"
}

step_headless_board_safety() {
  isolate
  python3 -c 'import PIL' 2>/dev/null || { echo "missing python-pillow"; return 1; }
  local s="$out/screens" t="$out/trees" m="$work/safety" f
  mkdir -p "$m"
  # Search focus: Board keys stay out of the focused sidebar search.
  cp "$draw_fixture" "$draw_board"
  printf '{"events":[{"t":0.3,"type":"key","name":"4","cmd":true},{"t":0.8,"type":"tap","key":"board-row-2"},{"t":2.0,"type":"mark","name":"before"},{"t":2.3,"type":"tap","key":"search-box"},{"t":2.6,"type":"type","text":"road1x+0","every":0.06},{"t":3.4,"type":"mark","name":"typed"}]}' \
    >"$m/search.json"
  safety_run search-before "$m/search.json" before || return 1
  safety_run search-typed "$m/search.json" typed || return 1
  python3 "$harness" tree-text "$t/safety-search-typed.json" search "road1x+0" || return 1
  python3 "$draw" same "$s/safety-search-before.png" "$s/safety-search-typed.png" "$t/safety-search-before.json" || return 1
  must_fail "the search text before typing" python3 "$harness" tree-text "$t/safety-search-before.json" search "road1x+0" || return 1

  # B2 + B1: restore a removed board from its recovery copy; Keep mine.
  printf '{"events":[{"t":1.0,"type":"mark","name":"banner"},{"t":1.3,"type":"tap","key":"recovery-restore-0"},{"t":2.8,"type":"mark","name":"restored"},{"t":3.2,"type":"tap","key":"board-%s"},{"t":4.0,"type":"mark","name":"done"}]}' keep-mine \
    >"$m/keep.json"
  seed_safety gone || return 1
  safety_run banner "$m/keep.json" banner || return 1
  python3 "$safety" text "$t/safety-banner.json" recovery-banner "1 board" || return 1
  python3 "$safety" text "$t/safety-banner.json" recovery-banner "Tools.excalidraw" || return 1
  safety_run restored "$m/keep.json" restored || return 1
  python3 "$harness" tree-mode "$t/safety-restored.json" board || return 1
  python3 "$safety" text "$t/safety-restored.json" board-conflict "Removed" || return 1
  python3 "$harness" tree-text "$t/safety-restored.json" board-save-state Unsaved || return 1
  python3 "$safety" rect "$s/safety-restored.png" "$t/safety-restored.json" "$m/copy.json" || return 1
  [ ! -e "$draw_board" ] || { echo "restoring wrote the board"; return 1; }
  echo "ok restoring did not write the board"
  safety_run kept "$m/keep.json" done || return 1
  python3 "$safety" file "$draw_board" 6 recovered-rect || return 1
  "$roundtrip" "$draw_board" || return 1
  python3 "$safety" recovery "$XDG_STATE_HOME" Tools.excalidraw gone || return 1
  python3 "$harness" tree-text "$t/safety-kept.json" board-save-state Saved || return 1
  no_key "$t/safety-kept.json" board-conflict || return 1
  must_fail "the drawing on the banner shot" python3 "$safety" rect "$s/safety-banner.png" "$t/safety-restored.json" "$m/copy.json" || return 1
  must_fail "the rectangle as absent on the restored shot" python3 "$safety" rect "$s/safety-restored.png" "$t/safety-restored.json" "$m/copy.json" absent || return 1
  must_fail "the fixture as the kept file" python3 "$safety" file "$draw_fixture" 6 recovered-rect || return 1

  # Save copy instead.
  sed 's/board-keep-mine/board-save-copy/' "$m/keep.json" >"$m/copy-run.json"
  seed_safety gone || return 1
  safety_run copied "$m/copy-run.json" done || return 1
  f="$(find "$vault" -maxdepth 1 -name 'Tools (conflict *).excalidraw' | head -1)"
  [ -n "$f" ] || { echo "no conflict copy"; return 1; }
  python3 "$safety" file "$f" 6 recovered-rect || return 1
  [ ! -e "$draw_board" ] || { echo "Save copy wrote the original"; return 1; }
  python3 "$safety" recovery "$XDG_STATE_HOME" Tools.excalidraw gone || return 1
  rm -f "$vault"/Tools\ \(conflict*

  # Over a file that holds something else: flagged as changed; Load theirs.
  sed 's/board-keep-mine/board-load-theirs/' "$m/keep.json" >"$m/theirs.json"
  seed_safety fixture || return 1
  local before
  before="$(sha256sum <"$draw_board")"
  safety_run changed "$m/theirs.json" restored || return 1
  python3 "$safety" text "$t/safety-changed.json" board-conflict "Changed" || return 1
  safety_run theirs "$m/theirs.json" done || return 1
  [ "$(sha256sum <"$draw_board")" = "$before" ] || { echo "Load theirs changed their file"; return 1; }
  echo "ok their file is untouched"
  python3 "$safety" recovery "$XDG_STATE_HOME" Tools.excalidraw gone || return 1
  python3 "$safety" rect "$s/safety-theirs.png" "$t/safety-restored.json" "$m/copy.json" absent || return 1
  cp "$draw_fixture" "$draw_board"
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

# The draw checks in the windowed app on Xvfb with XTEST: tool letters, button-1
# strokes, Ctrl+S (the file must not change before it and must after), a clean
# close, the file checks, and a relaunch that shows the saved drawing.
step_x11_draw() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local tree="$out/trees/draw-before.json"
  [ -f "$tree" ] || { echo "run headless_draw first (needs $tree)"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n
  for n in $(seq 90 130); do [ -e "/tmp/.X$n-lock" ] || [ -e "/tmp/.X11-unix/X$n" ] || break; done
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-draw.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  export XDG_CONFIG_HOME="$work/config-x11-draw"
  rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
  cp "$draw_fixture" "$draw_board"
  local wid s="$out/screens" m="$work/draw-x11" fixture_sha
  mkdir -p "$m"
  fixture_sha="$(sha256sum <"$draw_fixture")"
  launch() {
    "$bin" --vault "$vault" >>"$out/logs/x11-draw-app.log" 2>&1 &
    app_pid=$!
    wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
    sleep 2
  }
  shot() { sleep 1.5; import -window "$wid" "$s/x11-draw-$1.png"; }
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
  local row
  row="$(python3 "$harness" tree-center "$tree" board-row-2)" || return 1
  launch || return 1
  python3 "$harness" x11-chord "$wid" ctrl+4 && sleep 1 || return 1
  python3 "$harness" x11-click "$wid" $row && shot 1-before || return 1
  python3 "$draw" x11 "$wid" "$tree" && shot 2-drawn || return 1
  python3 "$draw" pixels "$s/x11-draw-1-before.png" "$s/x11-draw-2-drawn.png" "$tree" || return 1
  [ "$(sha256sum <"$draw_board")" = "$fixture_sha" ] || { echo "the board was written before Ctrl+S"; return 1; }
  echo "ok the file is unchanged before Ctrl+S"
  python3 "$harness" x11-chord "$wid" ctrl+s && shot 3-saved || return 1
  [ "$(sha256sum <"$draw_board")" != "$fixture_sha" ] || { echo "Ctrl+S did not write the board"; return 1; }
  echo "ok Ctrl+S wrote the board"
  close_app || return 1
  cp "$draw_board" "$out/logs/x11-draw-saved.excalidraw"
  "$resave" "$draw_fixture" "$m/resaved.excalidraw" || return 1
  python3 "$draw" file "$draw_board" "$draw_fixture" "$m/resaved.excalidraw" "$tree" || return 1
  "$roundtrip" "$draw_board" || return 1
  launch || return 1
  shot 4-reloaded || return 1
  python3 "$harness" settings-mode "$XDG_CONFIG_HOME/muninn/settings.json" board || return 1
  python3 "$draw" same "$s/x11-draw-2-drawn.png" "$s/x11-draw-4-reloaded.png" "$tree" || return 1
  close_app || return 1
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

step_x11_board_safety() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local tree="$out/trees/safety-search-before.json" restored="$out/trees/safety-restored.json" changed="$out/trees/safety-changed.json"
  [ -f "$tree" ] && [ -f "$changed" ] || { echo "run headless_board_safety first"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"; chmod -R u+w "$work" 2>/dev/null' EXIT
  local n
  for n in $(seq 90 130); do [ -e "/tmp/.X$n-lock" ] || [ -e "/tmp/.X11-unix/X$n" ] || break; done
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-safety.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  export XDG_CONFIG_HOME="$work/config-x11-safety" XDG_STATE_HOME="$work/state-x11-safety"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  cp "$draw_fixture" "$draw_board"
  local wid s="$out/screens" theirs="$work/safety/theirs.excalidraw"
  sed 's/keep me/theirs/' "$draw_fixture" >"$theirs"
  launch() {
    "$bin" --vault "$vault" >>"$out/logs/x11-safety-app.log" 2>&1 &
    app_pid=$!
    wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
    sleep 2
  }
  shot() { sleep 1.5; import -window "$wid" "$s/x11-safety-$1.png"; }
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
  reenter() { python3 "$harness" x11-chord "$wid" ctrl+1 && sleep 1 && python3 "$harness" x11-chord "$wid" ctrl+4 && sleep 1; }
  draw_one() { # draw_one DX: a rectangle with the R tool at canvas (300+DX, 330)
    python3 "$harness" x11-chord "$wid" r || return 1
    python3 "$harness" x11-drag "$wid" $(( cx + 300 + $1 )) $(( cy + 330 )) 90 60 || return 1
    python3 "$harness" x11-chord "$wid" v
  }
  local row keep copy restore cx cy
  row="$(python3 "$harness" tree-center "$tree" board-row-2)" || return 1
  keep="$(python3 "$harness" tree-center "$restored" board-keep-mine)" || return 1
  copy="$(python3 "$harness" tree-center "$changed" board-save-copy)" || return 1
  restore="$(python3 "$harness" tree-center "$out/trees/safety-banner.json" recovery-restore-0)" || return 1
  read -r cx cy _ <<<"$(python3 -c 'import json,sys; sys.path.insert(0, sys.argv[2]); import board_pixels as b; r=b.canvas_rect(sys.argv[1]); print(int(r[0]), int(r[1]))' "$tree" "$root/scripts/ui-smoke")"

  # B1: draw, remove the file, leave and re-enter Board, Keep mine.
  launch || return 1
  python3 "$harness" x11-chord "$wid" ctrl+4 && sleep 1 || return 1
  python3 "$harness" x11-click "$wid" $row && shot 1-open || return 1
  draw_one 0 && shot 2-drawn || return 1
  python3 "$safety" recovery "$XDG_STATE_HOME" Tools.excalidraw 6 || return 1
  rm "$draw_board"
  reenter && shot 3-removed || return 1
  python3 "$draw" same "$s/x11-safety-2-drawn.png" "$s/x11-safety-3-removed.png" "$tree" || return 1
  echo "ok the drawing is still on the canvas after the file was removed"
  must_fail "the drawn and the open shots" python3 "$draw" same "$s/x11-safety-1-open.png" "$s/x11-safety-3-removed.png" "$tree" || return 1
  python3 "$harness" x11-click "$wid" $keep && shot 4-kept || return 1
  python3 "$safety" file "$draw_board" 6 || return 1
  "$roundtrip" "$draw_board" || return 1
  python3 "$safety" recovery "$XDG_STATE_HOME" Tools.excalidraw gone || return 1

  # B2: draw again, another app rewrites the file, quit: a recovery copy, theirs kept.
  draw_one 120 && shot 5-drawn || return 1
  cp "$theirs" "$draw_board"
  reenter && shot 6-changed || return 1
  python3 "$draw" same "$s/x11-safety-5-drawn.png" "$s/x11-safety-6-changed.png" "$tree" || return 1
  close_app || return 1
  cmp -s "$theirs" "$draw_board" || { echo "quitting wrote over their file"; return 1; }
  echo "ok quitting left their file alone"
  python3 "$safety" recovery "$XDG_STATE_HOME" Tools.excalidraw 7 || return 1
  launch && shot 7-banner || return 1
  python3 "$harness" x11-click "$wid" $restore && shot 8-restored || return 1
  python3 "$harness" settings-mode "$XDG_CONFIG_HOME/muninn/settings.json" board || return 1
  python3 "$harness" x11-click "$wid" $copy && shot 9-copied || return 1
  local f
  f="$(find "$vault" -maxdepth 1 -name 'Tools (conflict *).excalidraw' | head -1)"
  [ -n "$f" ] || { echo "Save copy wrote no copy"; return 1; }
  python3 "$safety" file "$f" 7 || return 1
  "$roundtrip" "$f" || return 1
  cmp -s "$theirs" "$draw_board" || { echo "Save copy wrote over their file"; return 1; }
  python3 "$safety" recovery "$XDG_STATE_HOME" Tools.excalidraw gone || return 1
  close_app || return 1
  rm -f "$f"

  # The refusal: no save and no recovery copy possible, so a close is refused.
  export XDG_CONFIG_HOME="$work/config-x11-refuse" XDG_STATE_HOME="$work/state-x11-refuse"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME/muninn/recovery"
  chmod a-w "$XDG_STATE_HOME/muninn/recovery"
  cp "$draw_fixture" "$draw_board"
  launch || return 1
  python3 "$harness" x11-chord "$wid" ctrl+4 && sleep 1 || return 1
  python3 "$harness" x11-click "$wid" $row || return 1
  draw_one 0 || return 1
  cp "$theirs" "$draw_board"
  python3 "$harness" x11-close "$wid" || return 1
  sleep 3
  kill -0 "$app_pid" 2>/dev/null || { echo "the app quit with an unsaveable, unrecoverable board"; return 1; }
  echo "ok the close was refused: the app is still running"
  wid="$(python3 "$harness" x11-wait Muninn 15)" || { echo "the window did not come back"; return 1; }
  shot 10-refused || return 1
  echo "ok the window came back"
  cmp -s "$theirs" "$draw_board" || { echo "the refused close wrote over their file"; return 1; }
  chmod u+w "$XDG_STATE_HOME/muninn/recovery"
  close_app || return 1
  python3 "$safety" recovery "$XDG_STATE_HOME" Tools.excalidraw 6 || return 1
  cmp -s "$theirs" "$draw_board" || return 1
  cp "$draw_fixture" "$draw_board"
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

step_board_untouched() {
  sha256sum -c "$work/board.sha256" || { echo "the app changed the board file"; return 1; }
  "$roundtrip" "$board" || return 1
  # The checker's scratch save is hidden and removed; nothing else appears in the vault.
  find "$vault" -name '.roundtrip-*' | grep . && { echo "scratch file left behind"; return 1; }
  # Nor any atomic-write temp file from the draw steps' saves.
  find "$vault" -name '.*' -type f | grep . && { echo "hidden temp file left behind"; return 1; }
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
