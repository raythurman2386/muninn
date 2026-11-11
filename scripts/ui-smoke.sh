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
# Requirements: cargo, Xvfb (xorg-server-xvfb), python3 with python-xlib, ImageMagick
# (`import`), sha256sum. Missing ones fail the step that needs them with the package name.
#
# Steps (UI_SMOKE_STEPS, space-separated, default all, in this order):
#   build           cargo build the app and the board_roundtrip example
#   seed            vault: two linked .md notes and Plan.excalidraw (the core test fixture)
#   cli_check       `muninn --check`: 2 notes, 0 broken links (the board is not a note)
#   headless_modes  Ctrl/⌘+3, +2, +1 → Graph, Read, Write: a PNG and tree per mode, the
#                   active mode button asserted from the tree
#   x11_modes       windowed app on Xvfb: launch shot, Ctrl+3/2/1 with a shot each,
#                   settings.json "mode" asserted after each, clean close (exit 0)
#   board_roundtrip the seeded board is byte-identical after the app runs, and
#                   board_roundtrip (load → compare → atomic save → reload) passes on it
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
STEPS="${UI_SMOKE_STEPS:-build seed cli_check headless_modes x11_modes board_roundtrip}"

rm -rf "$out/screens" "$out/trees" "$out/logs" "$out/work" "$out/summary.txt"
mkdir -p "$out/screens" "$out/trees" "$out/logs"
work="$out/work"
vault="$work/vault"
bin="$root/target/debug/muninn"
roundtrip="$root/target/debug/examples/board_roundtrip"
board="$vault/Plan.excalidraw"
fixture="$root/crates/muninn-core/tests/fixtures/board/scene.excalidraw"

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
  sha256sum "$board" | tee "$work/board.sha256"
  find "$vault" -type f | sort
}

step_cli_check() {
  isolate
  local report
  report="$("$bin" --check "$vault")" || return 1
  echo "$report"
  grep -qx 'notes: 2' <<<"$report" && grep -qx 'broken links: 0' <<<"$report"
}

step_headless_modes() {
  isolate
  command -v vulkaninfo >/dev/null && vulkaninfo --summary 2>/dev/null | grep -E 'deviceName|driverName'
  local mode
  for mode in graph preview editor; do
    "$bin" --vault "$vault" --script "$root/scripts/ui-smoke/modes.json" \
      --at "mark:$mode+0.9" --scale 1 \
      --snap "$out/screens/headless-$mode.png" --tree "$out/trees/headless-$mode.json" || return 1
    python3 "$harness" tree-mode "$out/trees/headless-$mode.json" "$mode" || return 1
  done
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

  "$bin" --vault "$vault" >"$out/logs/x11-app.log" 2>&1 &
  app_pid=$!
  local wid
  wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
  sleep 2
  import -window "$wid" "$out/screens/x11-launch.png" || return 1
  python3 "$harness" settings-mode "$XDG_CONFIG_HOME/muninn/settings.json" editor || true

  local mode key
  for mode in graph preview editor; do
    case "$mode" in graph) key=3 ;; preview) key=2 ;; editor) key=1 ;; esac
    python3 "$harness" x11-chord "$wid" "ctrl+$key" || return 1
    sleep 1.5
    import -window "$wid" "$out/screens/x11-$mode.png" || return 1
    python3 "$harness" settings-mode "$XDG_CONFIG_HOME/muninn/settings.json" "$mode" || return 1
  done

  python3 "$harness" x11-close "$wid" || return 1
  local status=124
  for i in $(seq 100); do
    if ! kill -0 "$app_pid" 2>/dev/null; then wait "$app_pid"; status=$?; break; fi
    sleep 0.1
  done
  app_pid=
  cp "$XDG_CONFIG_HOME/muninn/settings.json" "$out/logs/x11-settings.json" 2>/dev/null
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
  echo "app exit status: $status"
  [ "$status" = 0 ]
}

step_board_roundtrip() {
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
