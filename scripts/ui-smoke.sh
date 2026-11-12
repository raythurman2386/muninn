#!/usr/bin/env bash
# UI smoke harness: launch the real Muninn binary headless, drive it, screenshot each
# step, and check the vault files afterwards. Linux only (built and run on minis).
#
#   scripts/ui-smoke.sh [OUT_DIR]            default OUT_DIR: target/ui-smoke
#   UI_SMOKE_STEPS="headless_modes" scripts/ui-smoke.sh    run a subset (names below)
#   UI_SMOKE_SLOW=3 scripts/ui-smoke.sh      stretch the X11 pauses (a busy machine)
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
#   build           cargo build the app, the board_roundtrip example and the kanban_roundtrip example
#   seed            vault: two linked .md notes, Plan.excalidraw (the core test fixture)
#                   and Smoke.excalidraw (one element of each kind, for the pixel checks),
#                   and Tools.excalidraw (the board the draw steps draw on)
#   cli_check       `muninn --check`: 2 notes, 0 broken links (the board is not a note);
#                   `muninn --version` / `-V` print `muninn <Cargo.toml version>`
#   headless_modes  one input script (scripts/ui-smoke/modes.json) through Graph, Read,
#                   Write, Board by ⌘4, Esc out, Board by its mode button, a board row
#                   picked, Graph, Board by the command palette ("whiteboard"), Read,
#                   Write, Board again, then Kanban by ⌘5, Esc out, Kanban by its mode
#                   button: a PNG and tree per mark, the active mode button asserted
#                   from each tree
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
#   headless_edit   Board slice 5, offscreen, on a fresh Tools.excalidraw: click selects
#                   keep-rect, a drag moves it, Esc and a marquee select keep-text,
#                   Shift+arrows and an arrow nudge it, Delete, Cmd+Z, Cmd+Shift+Z,
#                   Cmd+S, Cmd+Z; board_edit.py pixels checks every mark (outlines
#                   only around the selection, the fill and the text where the plan
#                   puts them, nothing else changed), the save-state reads Saved then
#                   Unsaved, and board_edit.py file checks the save (moved geometry,
#                   bumped version/versionNonce/updated, the text a tombstone, the
#                   rest byte-identical to s1's save) and the s1 round-trip; negative
#                   controls (wrong shots, the original file, a changed untouched
#                   byte, an unbumped version) must FAIL
#   x11_edit        the same on Xvfb with XTEST (plus Shift+click to add, Ctrl+Y to
#                   redo): the file is unchanged until Ctrl+S, the undo after it
#                   reads Unsaved, and quitting flushes that undo into the file
#   headless_text   Board slice 6, offscreen, on a fresh Tools.excalidraw: T, a click,
#                   typing "r1 dü", Enter, "ok" (tool letters, a digit, non-ASCII and a
#                   newline), Esc, then a drag that would draw a rectangle had a typed
#                   key picked a tool; a double click on keep-text, "!", Esc; Cmd+Z,
#                   Cmd+Shift+Z, Cmd+S; board_text.py pixels checks each mark (the new
#                   text's ink on two lines at the click, keep-text's ink longer, back,
#                   longer, nothing else drawn), the tree has the editor only while it
#                   is open, and board_text.py file checks the save (text/originalText,
#                   Excalidraw's text fields, measured geometry at the click, keep-text's
#                   bumped version, the rest byte-identical to s1's save) and the s1
#                   round-trip; negative controls (wrong shots, the original file, a
#                   changed untouched byte, wrong text) must FAIL
#   x11_text        the same on Xvfb with XTEST (ü typed through a remapped spare
#                   keycode, Ctrl+Y redo): the file is unchanged until Ctrl+S
#   headless_sync   Board slice 7, offscreen, autosave on (the default) on a fresh
#                   Tools.excalidraw: T, a click, "auto", Esc and no Cmd+S. Just after
#                   Esc the save-state reads Unsaved and the file is the fixture; 3 s
#                   later it reads Saved and board_sync.py file checks the autosave
#                   (the fixture's elements and top-level values exact, "auto"
#                   appended) and the s1 round-trip; with "board_autosave": false the
#                   same run leaves the file untouched; negative controls must FAIL.
#                   (The headless clock is virtual, so another app's write mid-run is
#                   checked under X11 and by the board_sync unit tests.)
#   x11_sync        on Xvfb: another app rewrites the open, clean board in place
#                   and its longer keep-text shows with no click (pixels, session log);
#                   text typed then is autosaved on top of their version; then, with
#                   autosave off, an unsaved nudge plus another app's write is flagged
#                   (session log), and neither Ctrl+S nor quitting writes over their
#                   file, while the drawing gets a recovery copy; then (#41 GK2)
#                   typing in keep-text while another app rewrites it is flagged,
#                   never reloaded under the editor, and never autosaved over;
#                   then (#41 GK2b) the same three ways with an ATOMIC save by the
#                   other app (temp + rename): change under typing, delete under
#                   typing, change under an open editor with nothing typed
#   x11_chrome      on Xvfb: select, deselect, select again; after each click 12
#                   quick window grabs and a settled one show the key hint bar and
#                   zoom controls exactly where they were (no slide); a copy with the
#                   chrome moved 20 px must FAIL
#                   then (slice 8) the notes sidebar toggled open and shut with
#                   Ctrl+\\: the canvas narrows, so the chrome may move, but every quick
#                   grab shows the bottom strip as before or as settled, never between;
#                   a blend half way must FAIL
#   headless_new_board  slice 8 on a vault with no boards: entering Board writes no
#                   file, collapses the notes sidebar and offers "New board"; New board
#                   writes an empty "Untitled board.excalidraw" in the folder New note
#                   uses (the selected note's) and opens it, a second one is "Untitled board 2"; Ctrl+\\ reopens the
#                   sidebar, and leaving Board shows it
#   x11_new_board   on Xvfb: the same vault, Board, click "New board" in the empty
#                   state (the file appears and opens), then the sidebar toggle button
#                   and Ctrl+\\; then (GK MF1) Ctrl+K and Ctrl+4: the hidden search field
#                   lets go, so R and a drag draw a rectangle that Ctrl+S saves
#   x11_board_rename  #43 (Raymond: "I can't seem to update the name of the new board"):
#                   New board, draw, F2, type a name, Enter: the file is renamed on disk,
#                   the old path is gone, the board stays open, the watchers do not flag
#                   it, and a new rectangle + Ctrl+S saves to the new path; then a double
#                   click on the title and Rename in the board row's right-click menu
#                   rename it again; F2 to the multibyte "é ünïcode" (12 bytes; GK B-1
#                   crashed here) and back; no recovery copy is left under any old name
#   x11_single_instance  #42 (two "M" tray icons): on Xvfb and a private D-Bus session
#                   (dbus-run-session) whose only tray host is scripts/ui-smoke/sni_watcher.py,
#                   with Show tray icon and Close to tray on. Asserts exactly one registered
#                   StatusNotifierItem after launch; after closing to the tray and a second
#                   launch (which exits 0 and brings the hidden window back, one process);
#                   across Settings › Appearance › Show tray icon off/on/on (0/1/1); across a
#                   tray-host restart; across a launch naming another vault (the running app
#                   switches); and across kill -9 and a relaunch (no stale lock or icon).
#                   Needs dbus-run-session, gdbus and python-gobject besides the x11 tools.
#                   Rerun alone: UI_SMOKE_STEPS="build x11_single_instance" scripts/ui-smoke.sh
#   headless_kanban_model  Kanban slice 1, offscreen, on its own vault (the hand-written
#                   fixtures plus Roadmap.md and 2026-11-11.md; the shared seed vault is
#                   not touched). kanban_roundtrip is byte-identical on every valid fixture
#                   and a rename-lane diff matches kanban-rename.diff. `muninn --check`
#                   and `--list-links` see the card links. scripts/ui-smoke/kanban_model.json
#                   is generated for the note ids: the kanban opens in Write, then Read,
#                   then Roadmap shows it in the backlinks. Screenshots kanban-s1-write.png,
#                   kanban-s1-read.png, kanban-s1-backlinks.png. Every kanban file's sha256
#                   is unchanged.
#   x11_kanban_model  the same vault, windowed (`--new-instance`) on the private Xvfb with
#                   lavapipe: open the kanban note, screenshot kanban-s1-x11.png, close
#                   through WM_DELETE_WINDOW (exit 0), file unchanged
#   headless_kanban  Kanban slice 2, offscreen, on its own vault $work/kanban-view-vault
#                   (slice 1's vault is $work/kanban-vault): Agenda.md (a plain note,
#                   selected at launch) and slice 1's hand-written fixtures as
#                   Release.md (lanes-and-limits), Overloaded.md (over-limit: a lane
#                   past its limit, a two-line card, passthrough, a done lane) and
#                   Torn.md (bad-json). scripts/ui-smoke/kanban.json, with the note ids
#                   filled in from the app's own sidebar: Ctrl+5 on Agenda (the empty
#                   state), Release then Ctrl+5, Overloaded then Ctrl+5, Torn then
#                   Ctrl+5, and Open as markdown (Write). One run per mark with fresh
#                   settings; the trees are checked for the mode, lane titles in file
#                   order, counts, n/N, Over limit, Complete, card text, no
#                   passthrough, the unreadable message. Screenshots kanban-empty.png,
#                   kanban-readonly.png, kanban-wip-over.png, kanban-unreadable.png.
#                   Then its own vault $work/kanban-links-vault: Cmd+K, "links", Enter
#                   opens Links.md in Kanban mode, and in kanban-links.png the card
#                   [[Nowhere]] (no such note) is drawn dimmed, [[Agenda]] in the accent.
#                   Every file's sha256 is unchanged.
#   headless_kanban_restart  Release in Kanban, then Ctrl+[ back to Agenda (the page
#                   keeps Release); settings.json says kanban / Release.md / Agenda.md;
#                   a relaunch on those settings opens Kanban on Release
#                   (kanban-restart.png). Files unchanged.
#   x11_kanban      the same vault, windowed (`--new-instance`) on the private Xvfb with
#                   lavapipe: click Release, Ctrl+5, screenshot kanban-x11.png (matches
#                   the headless kanban-readonly.png), settings.json says kanban,
#                   close through WM_DELETE_WINDOW (exit 0), files unchanged
#   headless_new_kanban  Kanban slice 3, offscreen, on its own vault $work/kanban-new-vault
#                   (Agenda, an empty Blank, Plans/Notes, someone's Plans/Untitled Kanban.md
#                   and Release from lanes-and-limits). New Kanban from the palette twice:
#                   Untitled Kanban.md, then Untitled Kanban 2.md, each byte-identical to
#                   the spec's New-file layout (fixture new-kanban.md) and open in Kanban
#                   (kanban-new.png); with Plans/Notes selected the existing
#                   Plans/Untitled Kanban.md is kept (sha256) and Plans/Untitled Kanban 2.md
#                   is made; Convert empty note to Kanban is offered on Blank and writes
#                   the layout, and on Agenda (text) is not offered and Enter changes
#                   nothing; the Kanbans section lists all five in path order
#                   (kanban-sidebar.png), five notes-list rows carry the glyph, and the
#                   palette has New Kanban and each kanban by name. The outside-the-vault
#                   folder refusal is the core guard's tests (the scan never follows a
#                   symlink, so the UI can't pick such a folder). Pre-existing files'
#                   sha256 unchanged, no temp file left.
#   x11_new_kanban  windowed (`--new-instance`) on the private Xvfb with lavapipe: Ctrl+Shift+P,
#                   "new kanban", Enter: Untitled Kanban.md is the layout, settings.json
#                   says kanban / Untitled Kanban.md, the session log says created
#                   (kanban-new-x11.png), close through WM_DELETE_WINDOW (exit 0)
#   x11_kanban_open  windowed (`--new-instance`): click Release in the notes list (Write, with
#                   Open as kanban: kanban-open-button.png), click Open as kanban (Kanban on
#                   Release), Open as markdown (Write on Release: kanban-as-markdown.png),
#                   each shot matched against a headless run of the same clicks; then
#                   Ctrl+K, "release", Enter (a pick from the note search) opens Release
#                   in Kanban mode (kanban-open-search.png) and Esc leaves it; then
#                   Ctrl+K, Ctrl+Shift+P, "overloaded", Down, Enter opens the Overloaded
#                   kanban in Kanban mode, and Esc leaves it (the page had the keyboard,
#                   not the search field). Every file's sha256 unchanged, exit 0
#   headless_kanban_edit  slice 4, offscreen on its own vault (edit-board.md as Board.md):
#                   + Add card, Edit from the card menu (multibyte text, Shift+Enter, a
#                   second line), Duplicate, Insert card before/after, Split card, Delete,
#                   the checkbox, a card added in the Done lane (born done), Mark all cards
#                   done, Cmd+Z, Cmd+Shift+Z; each lane checked from the trees
#                   (kanban-card-new.png, kanban-card-editing.png, kanban-card-split.png,
#                   kanban-undo.png); Board.md byte-identical before Cmd+S, equal to
#                   ui-smoke/kanban-edit-saved.md after it (kanban-saved.png), and undo
#                   back to the start + Cmd+S writes the fixture's bytes again
#   x11_kanban_edit  windowed (`--new-instance`) on the private Xvfb with lavapipe: from
#                   Write, Ctrl+K, "board", Enter opens the Board kanban in Kanban mode
#                   (kanban-cmdk.png, settings.json says kanban / Board.md) with the
#                   keyboard on the page (Esc leaves it; Ctrl+5 returns); + Add card,
#                   type, Enter, the card menu, Edit, type, Enter: file unchanged; Ctrl+S:
#                   Board.md equals ui-smoke/kanban-edit-x11.md (kanban-saved-x11.png);
#                   close through WM_DELETE_WINDOW (exit 0), bytes still the saved ones
#   The s4-s6 steps that check "unchanged until Ctrl+S" run with "board_autosave":
#   false (explicit_save). x11 steps take UI_SMOKE_DISPLAY (a display number) if set,
#   else the first free display from :90.
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
STEPS="${UI_SMOKE_STEPS:-build seed cli_check headless_modes headless_restart headless_board headless_big_restart headless_big_write headless_draw headless_board_safety headless_edit headless_text headless_sync headless_new_board x11_modes x11_board x11_draw x11_board_safety x11_edit x11_text x11_sync x11_chrome x11_new_board x11_board_rename x11_single_instance headless_kanban_model x11_kanban_model headless_kanban headless_kanban_restart x11_kanban headless_new_kanban x11_new_kanban x11_kanban_open headless_kanban_edit x11_kanban_edit board_untouched}"

rm -rf "$out/screens" "$out/trees" "$out/logs" "$out/work" "$out/summary.txt"
mkdir -p "$out/screens" "$out/trees" "$out/logs"
work="$out/work"
vault="$work/vault"
bin="$root/target/debug/muninn"
roundtrip="$root/target/debug/examples/board_roundtrip"
kanban_roundtrip="$root/target/debug/examples/kanban_roundtrip"
kanban_fixtures="$root/crates/muninn-core/tests/fixtures/kanban"
board="$vault/Plan.excalidraw"
fixture="$root/crates/muninn-core/tests/fixtures/board/scene.excalidraw"
smoke_board="$vault/Smoke.excalidraw"
smoke_fixture="$root/crates/muninn-core/tests/fixtures/board/smoke.excalidraw"
pixels="$root/scripts/ui-smoke/board_pixels.py"
draw="$root/scripts/ui-smoke/board_draw.py"
safety="$root/scripts/ui-smoke/board_safety.py"
edit="$root/scripts/ui-smoke/board_edit.py"
texts="$root/scripts/ui-smoke/board_text.py"
syncs="$root/scripts/ui-smoke/board_sync.py"
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
  # Each step (its own subshell) gets its own runtime dir, and no session bus:
  # Muninn's single-instance lock and socket (#42) live there, so a step can
  # never hand its launch to a Muninn running on the desktop, and the tray
  # (zbus falls back to $XDG_RUNTIME_DIR/bus) can never reach the desktop's bar.
  export XDG_RUNTIME_DIR="$work/runtime/$BASHPID"
  unset MUNINN_VAULT MUNINN_REDUCED_MOTION WAYLAND_DISPLAY WAYLAND_SOCKET DISPLAY || true
  unset DBUS_SESSION_BUS_ADDRESS XDG_ACTIVATION_TOKEN DESKTOP_STARTUP_ID || true
  mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME"
  mkdir -p -m 700 "$work/runtime" && mkdir -p -m 700 "$XDG_RUNTIME_DIR"
}

pick_display() { # the X display for an x11 step: UI_SMOKE_DISPLAY (a number), else the first free from 90
  if [ -n "${UI_SMOKE_DISPLAY:-}" ]; then
    local d="${UI_SMOKE_DISPLAY#:}"
    if [ -e "/tmp/.X$d-lock" ] || [ -e "/tmp/.X11-unix/X$d" ]; then echo "display :$d is in use" >&2; return 1; fi
    echo "$d"; return 0
  fi
  local n
  for n in $(seq 90 130); do [ -e "/tmp/.X$n-lock" ] || [ -e "/tmp/.X11-unix/X$n" ] || { echo "$n"; return 0; }; done
  echo "no free X display in :90-:130" >&2; return 1
}

reopen_sidebar() { # $work/reopen.json: Cmd+\ at 0.3 s, for a relaunch straight into Board
  printf '{"events":[{"t":0.3,"type":"key","name":"\\\\","cmd":true}]}' >"$work/reopen.json"
}

explicit_save() { # board autosave off in $XDG_CONFIG_HOME: the s4-s6 steps check the file is unchanged until Ctrl+S
  mkdir -p "$XDG_CONFIG_HOME/muninn"
  printf '{"board_autosave": false}\n' >"$XDG_CONFIG_HOME/muninn/settings.json"
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
      --example kanban_roundtrip --manifest-path "$root/Cargo.toml"
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
  local report version want
  report="$("$bin" --check "$vault")" || return 1
  echo "$report"
  grep -qx 'notes: 2' <<<"$report" && grep -qx 'broken links: 0' <<<"$report" || return 1
  # GK MF-C (#43): --version prints the workspace version and exits without
  # a window (DISPLAY and Wayland are unset here, so a launch would fail).
  want="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -n 1)"
  version="$(timeout 20 "$bin" --version)" || return 1
  echo "$version"
  [[ "$version" == "muninn $want" ]] || { echo "want 'muninn $want'"; return 1; }
  [[ "$(timeout 20 "$bin" -V)" == "muninn $want" ]]
}

# mark in modes.json → the mode it must show
HEADLESS_MARKS="graph:graph preview:preview editor:editor board-shortcut:board board-esc:editor
board-button:board board-selected:board graph-after:graph board-palette:board
preview-after:preview editor-after:editor end-in-board:board
kanban-shortcut:kanban kanban-esc:editor kanban-button:kanban"

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
  "$bin" --vault "$vault" --script "$work/board-open.json" \
    --at "mark:$1+0.2" --scale 1 \
    --snap "$out/screens/board-$1.png" --tree "$out/trees/board-$1.json"
}

step_headless_board() {
  isolate
  python3 -c 'import PIL' 2>/dev/null || { echo "missing python-pillow"; return 1; }
  local mark
  # Slice 8: these marks were measured with the notes sidebar open; reopen it.
  python3 "$harness" sidebar-open "$root/scripts/ui-smoke/board.json" "$work/board-open.json" || return 1
  reopen_sidebar
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
  "$bin" --vault "$vault" --script "$work/reopen.json" --at 1.5 --scale 1 \
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
  reopen_sidebar
  "$bin" --vault "$big" --script "$work/reopen.json" --at 2.5 --scale 1 \
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
  printf '{"events":[{"t":2.5,"type":"key","name":"2","cmd":true},{"t":3.0,"type":"mark","name":"written"},{"t":3.4,"type":"key","name":"4","cmd":true},{"t":3.8,"type":"key","name":"\\\\","cmd":true},{"t":4.6,"type":"mark","name":"board"}]}' \
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
  explicit_save
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
  python3 "$harness" sidebar-open "$m/open.json" "$m/open.json" || return 1
  draw_run before "$m/open.json" before || return 1
  python3 "$harness" tree-text "$t/draw-before.json" board-surface-title Tools || return 1
  python3 "$draw" script "$t/draw-before.json" "$m/draw.json" || return 1
  python3 "$harness" sidebar-open "$m/draw.json" "$m/draw.json" || return 1
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
  reopen_sidebar
  "$bin" --vault "$vault" --script "$work/reopen.json" --at 1.5 --scale 1 \
    --snap "$s/draw-reloaded.png" --tree "$t/draw-reloaded.json" || return 1
  python3 "$harness" tree-text "$t/draw-reloaded.json" board-surface-title Tools || return 1
  python3 "$draw" same "$s/draw-drawn.png" "$s/draw-reloaded.png" "$t/draw-before.json"
}

# Board slice 5: select, move, delete, undo and redo on a fresh Tools board.
edit_run() { # edit_run NAME SCRIPT MARK: a fresh Tools board and settings, run to MARK
  cp "$draw_fixture" "$draw_board"
  export XDG_CONFIG_HOME="$work/config-edit-$1" XDG_STATE_HOME="$work/state-edit-$1"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  explicit_save
  "$bin" --vault "$vault" --script "$2" --at "mark:$3+0.25" --scale 1 \
    --snap "$out/screens/edit-$1.png" --tree "$out/trees/edit-$1.json"
}

step_headless_edit() {
  isolate
  python3 -c 'import PIL' 2>/dev/null || { echo "missing python-pillow"; return 1; }
  local s="$out/screens" t="$out/trees" m="$work/edit" mark
  mkdir -p "$m"
  printf '{"events":[{"t":0.3,"type":"key","name":"4","cmd":true},{"t":0.8,"type":"tap","key":"board-row-2"},{"t":2.0,"type":"mark","name":"before"}]}' \
    >"$m/open.json"
  edit_run before "$m/open.json" before || return 1
  python3 "$harness" tree-text "$t/edit-before.json" board-surface-title Tools || return 1
  python3 "$edit" script "$t/edit-before.json" "$draw_fixture" "$m/edit.json" || return 1
  python3 "$edit" pixels before "$s/edit-before.png" "$s/edit-before.png" "$t/edit-before.json" "$draw_fixture" || return 1
  for mark in selected moved marquee nudged deleted undone redone dirty saved; do
    edit_run "$mark" "$m/edit.json" "$mark" || return 1
    python3 "$edit" pixels "$mark" "$s/edit-$mark.png" "$s/edit-before.png" "$t/edit-before.json" "$draw_fixture" || return 1
  done
  python3 "$harness" tree-text "$t/edit-nudged.json" board-save-state Unsaved || return 1
  python3 "$harness" tree-text "$t/edit-saved.json" board-save-state Saved || return 1
  python3 "$harness" tree-text "$t/edit-dirty.json" board-save-state Unsaved || return 1
  # The "saved" run was last: the file holds the save (the run stopped before Cmd+Z).
  cp "$draw_board" "$out/logs/edit-saved.excalidraw"
  "$resave" "$draw_fixture" "$m/resaved.excalidraw" || return 1
  python3 "$edit" file "$draw_board" "$draw_fixture" "$m/resaved.excalidraw" "$t/edit-before.json" || return 1
  "$roundtrip" "$draw_board" || return 1
  # Negative controls: each check must catch the wrong input.
  must_fail "outline on the unselected shot" python3 "$edit" pixels selected "$s/edit-before.png" "$s/edit-before.png" "$t/edit-before.json" "$draw_fixture" || return 1
  must_fail "move on the selected shot" python3 "$edit" pixels moved "$s/edit-selected.png" "$s/edit-before.png" "$t/edit-before.json" "$draw_fixture" || return 1
  must_fail "nudge on the marquee shot" python3 "$edit" pixels nudged "$s/edit-marquee.png" "$s/edit-before.png" "$t/edit-before.json" "$draw_fixture" || return 1
  must_fail "delete on the undone shot" python3 "$edit" pixels deleted "$s/edit-undone.png" "$s/edit-before.png" "$t/edit-before.json" "$draw_fixture" || return 1
  must_fail "undo on the redone shot" python3 "$edit" pixels undone "$s/edit-redone.png" "$s/edit-before.png" "$t/edit-before.json" "$draw_fixture" || return 1
  must_fail "the original file" python3 "$edit" file "$draw_fixture" "$draw_fixture" "$m/resaved.excalidraw" "$t/edit-before.json" || return 1
  must_fail "the tombstone check on the saved file" python3 "$edit" file "$draw_board" "$draw_fixture" "$m/resaved.excalidraw" "$t/edit-before.json" no || return 1
  python3 -c 'import sys; t=open(sys.argv[1]).read(); open(sys.argv[2],"w").write(t.replace("\"https://example.com/keep\"", "\"https://example.com/keep\" ", 1))' \
    "$draw_board" "$m/bytes.excalidraw" || return 1
  must_fail "a changed byte in an untouched element" python3 "$edit" file "$m/bytes.excalidraw" "$draw_fixture" "$m/resaved.excalidraw" "$t/edit-before.json" || return 1
  python3 -c 'import json,sys; v=json.load(open(sys.argv[1])); v["elements"][0]["version"]=12; json.dump(v, open(sys.argv[2],"w"), indent=2)' \
    "$draw_board" "$m/unbumped.excalidraw" || return 1
  must_fail "an unbumped version" python3 "$edit" file "$m/unbumped.excalidraw" "$draw_fixture" "$m/resaved.excalidraw" "$t/edit-before.json" || return 1
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
  explicit_save
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
  python3 "$harness" sidebar-open "$m/search.json" "$m/search.json" || return 1
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
  n="$(pick_display)" || return 1
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
  shot() { sleep "$(python3 -c "print(1.5 * max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"; import -window "$wid" "$out/screens/x11-$1.png"; }
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
  n="$(pick_display)" || return 1
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
  shot() { sleep "$(python3 -c "print(1.5 * max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"; import -window "$wid" "$s/x11-board-$1.png"; }
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
  # Slice 8: reopen the notes sidebar, as the headless trees these clicks come from.
  python3 "$harness" x11-chord "$wid" ctrl+backslash && sleep 1 || return 1
  python3 "$harness" x11-click "$wid" $row && shot 1-fit || return 1
  python3 "$pixels" fit "$s/x11-board-1-fit.png" "$tree" "$smoke_fixture" "$m/fit.json" || return 1
  # The hand tool pans (a plain drag on empty canvas is the slice 5 marquee).
  python3 "$harness" x11-chord "$wid" h || return 1
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
  python3 "$harness" x11-chord "$wid" ctrl+backslash || return 1
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
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-draw.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  export XDG_CONFIG_HOME="$work/config-x11-draw"
  rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
  explicit_save
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
  shot() { sleep "$(python3 -c "print(1.5 * max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"; import -window "$wid" "$s/x11-draw-$1.png"; }
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
  # Slice 8: reopen the notes sidebar, as the headless trees these clicks come from.
  python3 "$harness" x11-chord "$wid" ctrl+backslash && sleep 1 || return 1
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
  python3 "$harness" x11-chord "$wid" ctrl+backslash || return 1
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
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-safety.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  export XDG_CONFIG_HOME="$work/config-x11-safety" XDG_STATE_HOME="$work/state-x11-safety"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  explicit_save
  cp "$draw_fixture" "$draw_board"
  local wid s="$out/screens" theirs="$work/safety/theirs.excalidraw"
  sed 's/keep me/theirs/' "$draw_fixture" >"$theirs"
  launch() {
    "$bin" --vault "$vault" >>"$out/logs/x11-safety-app.log" 2>&1 &
    app_pid=$!
    wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
    sleep 2
  }
  shot() { sleep "$(python3 -c "print(1.5 * max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"; import -window "$wid" "$s/x11-safety-$1.png"; }
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
  # Slice 8: reopen the notes sidebar, as the headless trees these clicks come from.
  python3 "$harness" x11-chord "$wid" ctrl+backslash && sleep 1 || return 1
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
  # Slice 8: this relaunch opens straight into Board, sidebar collapsed. The Restore
  # point comes from the headless banner tree (Write, sidebar shown) and Save copy
  # from the headless restored tree (Board, collapsed): match each in turn.
  python3 "$harness" x11-chord "$wid" ctrl+backslash && sleep 1 || return 1
  python3 "$harness" x11-click "$wid" $restore && sleep 1 || return 1
  python3 "$harness" x11-chord "$wid" ctrl+backslash && shot 8-restored || return 1
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
  explicit_save
  chmod a-w "$XDG_STATE_HOME/muninn/recovery"
  cp "$draw_fixture" "$draw_board"
  launch || return 1
  python3 "$harness" x11-chord "$wid" ctrl+4 && sleep 1 || return 1
  # Slice 8: reopen the notes sidebar, as the headless trees these clicks come from.
  python3 "$harness" x11-chord "$wid" ctrl+backslash && sleep 1 || return 1
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

step_x11_edit() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local tree="$out/trees/edit-before.json"
  [ -f "$tree" ] || { echo "run headless_edit first (needs $tree)"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-edit.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  export XDG_CONFIG_HOME="$work/config-x11-edit" XDG_STATE_HOME="$work/state-x11-edit"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  explicit_save
  cp "$draw_fixture" "$draw_board"
  local wid s="$out/screens" m="$work/edit-x11" fixture_sha saved_sha
  mkdir -p "$m"
  fixture_sha="$(sha256sum <"$draw_fixture")"
  launch() {
    "$bin" --vault "$vault" >>"$out/logs/x11-edit-app.log" 2>&1 &
    app_pid=$!
    wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
    sleep 2
  }
  shot() { sleep "$(python3 -c "print(1.5 * max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"; import -window "$wid" "$s/x11-edit-$1.png"; }
  check() { python3 "$edit" pixels "$1" "$s/x11-edit-$2.png" "$s/x11-edit-0-before.png" "$tree" "$draw_fixture"; }
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
  local row rect moved text from to
  row="$(python3 "$harness" tree-center "$tree" board-row-2)" || return 1
  rect="$(python3 "$edit" point "$tree" "$draw_fixture" rect)" || return 1
  moved="$(python3 "$edit" point "$tree" "$draw_fixture" rect-moved)" || return 1
  text="$(python3 "$edit" point "$tree" "$draw_fixture" text)" || return 1
  from="$(python3 "$edit" point "$tree" "$draw_fixture" marquee-from)" || return 1
  to="$(python3 "$edit" point "$tree" "$draw_fixture" marquee-to)" || return 1
  launch || return 1
  python3 "$harness" x11-chord "$wid" ctrl+4 && sleep 1 || return 1
  python3 "$harness" x11-click "$wid" $row && shot 0-before || return 1
  check before 0-before || return 1
  python3 "$harness" x11-click "$wid" $rect && shot 1-selected || return 1
  check selected 1-selected || return 1
  # shellcheck disable=SC2086
  python3 "$harness" x11-drag "$wid" $rect 100 80 && shot 2-moved || return 1
  check moved 2-moved || return 1
  python3 "$edit" x11-shift-click "$wid" $text && shot 3-both || return 1
  check both 3-both || return 1
  python3 "$harness" x11-chord "$wid" esc || return 1
  set -- $from $to
  python3 "$harness" x11-drag "$wid" "$1" "$2" $(( $3 - $1 )) $(( $4 - $2 )) && shot 4-marquee || return 1
  check marquee 4-marquee || return 1
  python3 "$harness" x11-chord "$wid" shift+Right || return 1
  python3 "$harness" x11-chord "$wid" shift+Right || return 1
  python3 "$harness" x11-chord "$wid" shift+Down || return 1
  python3 "$harness" x11-chord "$wid" Right && shot 5-nudged || return 1
  check nudged 5-nudged || return 1
  python3 "$harness" x11-chord "$wid" Delete && shot 6-deleted || return 1
  check deleted 6-deleted || return 1
  python3 "$harness" x11-chord "$wid" ctrl+z && shot 7-undone || return 1
  check undone 7-undone || return 1
  python3 "$harness" x11-chord "$wid" ctrl+y && shot 8-redone || return 1
  check redone 8-redone || return 1
  [ "$(sha256sum <"$draw_board")" = "$fixture_sha" ] || { echo "the board was written before Ctrl+S"; return 1; }
  echo "ok the file is unchanged before Ctrl+S"
  python3 "$harness" x11-chord "$wid" ctrl+s && shot 9-saved || return 1
  check saved 9-saved || return 1
  saved_sha="$(sha256sum <"$draw_board")"
  [ "$saved_sha" != "$fixture_sha" ] || { echo "Ctrl+S did not write the board"; return 1; }
  cp "$draw_board" "$out/logs/x11-edit-saved.excalidraw"
  "$resave" "$draw_fixture" "$m/resaved.excalidraw" || return 1
  python3 "$edit" file "$out/logs/x11-edit-saved.excalidraw" "$draw_fixture" "$m/resaved.excalidraw" "$tree" || return 1
  "$roundtrip" "$out/logs/x11-edit-saved.excalidraw" || return 1
  # Undo after the save: unsaved again, the file still holds the save, and
  # quitting flushes the undo (s4's quit save).
  python3 "$harness" x11-chord "$wid" ctrl+z && shot 10-dirty || return 1
  check dirty 10-dirty || return 1
  [ "$(sha256sum <"$draw_board")" = "$saved_sha" ] || { echo "the undo wrote the board"; return 1; }
  echo "ok the undo after the save left the file alone"
  close_app || return 1
  cp "$draw_board" "$out/logs/x11-edit-quit.excalidraw"
  python3 "$edit" file "$draw_board" "$draw_fixture" "$m/resaved.excalidraw" "$tree" no || return 1
  "$roundtrip" "$draw_board" || return 1
  must_fail "the quit save is not the Ctrl+S save" python3 "$edit" file "$draw_board" "$draw_fixture" "$m/resaved.excalidraw" "$tree" || return 1
  must_fail "outline on the unselected X11 shot" check selected 0-before || return 1
  must_fail "delete on the undone X11 shot" check deleted 7-undone || return 1
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

text_run() { # text_run NAME SCRIPT MARK: a fresh Tools board and settings, run to MARK
  cp "$draw_fixture" "$draw_board"
  export XDG_CONFIG_HOME="$work/config-text-$1" XDG_STATE_HOME="$work/state-text-$1"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  explicit_save
  "$bin" --vault "$vault" --script "$2" --at "mark:$3+0.25" --scale 1 \
    --snap "$out/screens/text-$1.png" --tree "$out/trees/text-$1.json"
}

text_negatives() { # text_negatives PREFIX SAVED TREE RESAVED: shared negative controls
  local s="$out/screens" m="$work/text-neg"
  mkdir -p "$m"
  must_fail "new text on the before shot" python3 "$texts" pixels typing "$s/$1before.png" "$s/$1before.png" "$3" "$draw_fixture" || return 1
  must_fail "the edit on the undone shot" python3 "$texts" pixels edited "$s/$1undone.png" "$s/$1before.png" "$3" "$draw_fixture" || return 1
  must_fail "the undo on the redone shot" python3 "$texts" pixels undone "$s/$1redone.png" "$s/$1before.png" "$3" "$draw_fixture" || return 1
  must_fail "the original file" python3 "$texts" file "$draw_fixture" "$draw_fixture" "$4" "$3" || return 1
  python3 -c 'import sys; t=open(sys.argv[1]).read(); open(sys.argv[2],"w").write(t.replace("\"https://example.com/keep\"", "\"https://example.com/keep\" ", 1))' \
    "$2" "$m/bytes.excalidraw" || return 1
  must_fail "a changed byte in an untouched element" python3 "$texts" file "$m/bytes.excalidraw" "$draw_fixture" "$4" "$3" || return 1
  python3 -c 'import json,sys; v=json.load(open(sys.argv[1])); v["elements"][-1]["originalText"]="r1 du\nok"; json.dump(v, open(sys.argv[2],"w"), indent=2)' \
    "$2" "$m/wrong.excalidraw" || return 1
  must_fail "originalText without the non-ASCII character" python3 "$texts" file "$m/wrong.excalidraw" "$draw_fixture" "$4" "$3" || return 1
}

step_headless_text() {
  isolate
  python3 -c 'import PIL' 2>/dev/null || { echo "missing python-pillow"; return 1; }
  local s="$out/screens" t="$out/trees" m="$work/text" mark
  mkdir -p "$m"
  printf '{"events":[{"t":0.3,"type":"key","name":"4","cmd":true},{"t":0.8,"type":"tap","key":"board-row-2"},{"t":2.0,"type":"mark","name":"before"}]}' \
    >"$m/open.json"
  python3 "$harness" sidebar-open "$m/open.json" "$m/open.json" || return 1
  text_run before "$m/open.json" before || return 1
  python3 "$harness" tree-text "$t/text-before.json" board-surface-title Tools || return 1
  python3 "$texts" script "$t/text-before.json" "$draw_fixture" "$m/text.json" || return 1
  python3 "$harness" sidebar-open "$m/text.json" "$m/text.json" || return 1
  python3 "$texts" pixels before "$s/text-before.png" "$s/text-before.png" "$t/text-before.json" "$draw_fixture" || return 1
  for mark in typing committed editing edited undone redone saved; do
    text_run "$mark" "$m/text.json" "$mark" || return 1
    python3 "$texts" pixels "$mark" "$s/text-$mark.png" "$s/text-before.png" "$t/text-before.json" "$draw_fixture" || return 1
  done
  python3 "$harness" tree-center "$t/text-typing.json" board-text-editor >/dev/null || { echo "no editor while typing"; return 1; }
  python3 "$harness" tree-center "$t/text-editing.json" board-text-editor >/dev/null || { echo "no editor after the double click"; return 1; }
  echo "ok the editor is open at typing and editing"
  no_key "$t/text-committed.json" board-text-editor || return 1
  no_key "$t/text-edited.json" board-text-editor || return 1
  python3 "$harness" tree-text "$t/text-committed.json" board-save-state Unsaved || return 1
  python3 "$harness" tree-text "$t/text-saved.json" board-save-state Saved || return 1
  cp "$draw_board" "$out/logs/text-saved.excalidraw"
  "$resave" "$draw_fixture" "$m/resaved.excalidraw" || return 1
  python3 "$texts" file "$draw_board" "$draw_fixture" "$m/resaved.excalidraw" "$t/text-before.json" || return 1
  "$roundtrip" "$draw_board" || return 1
  text_negatives text- "$out/logs/text-saved.excalidraw" "$t/text-before.json" "$m/resaved.excalidraw" || return 1
}

step_x11_text() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local tree="$out/trees/text-before.json"
  [ -f "$tree" ] || { echo "run headless_text first (needs $tree)"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-text.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  export XDG_CONFIG_HOME="$work/config-x11-text" XDG_STATE_HOME="$work/state-x11-text"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  explicit_save
  cp "$draw_fixture" "$draw_board"
  local wid s="$out/screens" m="$work/text-x11" fixture_sha
  mkdir -p "$m"
  fixture_sha="$(sha256sum <"$draw_fixture")"
  "$bin" --vault "$vault" >>"$out/logs/x11-text-app.log" 2>&1 &
  app_pid=$!
  wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
  sleep 2
  shot() { sleep "$(python3 -c "print(1.5 * max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"; import -window "$wid" "$s/x11-text-$1.png"; }
  check() { python3 "$texts" pixels "$1" "$s/x11-text-$2.png" "$s/x11-text-before.png" "$tree" "$draw_fixture"; }
  local row new text from to
  row="$(python3 "$harness" tree-center "$tree" board-row-2)" || return 1
  new="$(python3 "$texts" point "$tree" "$draw_fixture" new)" || return 1
  text="$(python3 "$texts" point "$tree" "$draw_fixture" text)" || return 1
  from="$(python3 "$texts" point "$tree" "$draw_fixture" drag-from)" || return 1
  to="$(python3 "$texts" point "$tree" "$draw_fixture" drag-to)" || return 1
  python3 "$harness" x11-chord "$wid" ctrl+4 && sleep 1 || return 1
  # Slice 8: reopen the notes sidebar, as the headless trees these clicks come from.
  python3 "$harness" x11-chord "$wid" ctrl+backslash && sleep 1 || return 1
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $row && shot before || return 1
  check before before || return 1
  python3 "$texts" x11-type "$wid" t || return 1
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $new || return 1
  python3 "$texts" x11-type "$wid" "$(printf 'r1 dü\nok')" && shot typing || return 1
  check typing typing || return 1
  python3 "$harness" x11-chord "$wid" esc || return 1
  set -- $from $to
  python3 "$harness" x11-drag "$wid" "$1" "$2" $(( $3 - $1 )) $(( $4 - $2 )) && shot committed || return 1
  check committed committed || return 1
  # shellcheck disable=SC2086
  python3 "$texts" x11-double-click "$wid" $text && shot editing || return 1
  check editing editing || return 1
  python3 "$texts" x11-type "$wid" '!' || return 1
  python3 "$harness" x11-chord "$wid" esc && shot edited || return 1
  check edited edited || return 1
  python3 "$harness" x11-chord "$wid" ctrl+z && shot undone || return 1
  check undone undone || return 1
  python3 "$harness" x11-chord "$wid" ctrl+y && shot redone || return 1
  check redone redone || return 1
  [ "$(sha256sum <"$draw_board")" = "$fixture_sha" ] || { echo "the board was written before Ctrl+S"; return 1; }
  echo "ok the file is unchanged before Ctrl+S"
  python3 "$harness" x11-chord "$wid" ctrl+s && shot saved || return 1
  check saved saved || return 1
  cp "$draw_board" "$out/logs/x11-text-saved.excalidraw"
  "$resave" "$draw_fixture" "$m/resaved.excalidraw" || return 1
  python3 "$texts" file "$out/logs/x11-text-saved.excalidraw" "$draw_fixture" "$m/resaved.excalidraw" "$tree" || return 1
  "$roundtrip" "$out/logs/x11-text-saved.excalidraw" || return 1
  text_negatives x11-text- "$out/logs/x11-text-saved.excalidraw" "$tree" "$m/resaved.excalidraw" || return 1
  python3 "$harness" x11-close "$wid" || return 1
  for i in $(seq 100); do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.1; done
  app_pid=
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}


sync_run() { # sync_run NAME SCRIPT MARK: a fresh Tools board and settings (autosave on), run to MARK
  cp "$draw_fixture" "$draw_board"
  export XDG_CONFIG_HOME="$work/config-sync-$1" XDG_STATE_HOME="$work/state-sync-$1"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  "$bin" --vault "$vault" --script "$2" --at "mark:$3+0.25" --scale 1 \
    --snap "$out/screens/sync-$1.png" --tree "$out/trees/sync-$1.json"
}

step_headless_sync() {
  isolate
  python3 -c 'import PIL' 2>/dev/null || { echo "missing python-pillow"; return 1; }
  local t="$out/trees" m="$work/sync"
  mkdir -p "$m"
  printf '{"events":[{"t":0.3,"type":"key","name":"4","cmd":true},{"t":0.8,"type":"tap","key":"board-row-2"},{"t":2.0,"type":"mark","name":"before"}]}' \
    >"$m/open.json"
  sync_run before "$m/open.json" before || return 1
  python3 "$harness" tree-text "$t/sync-before.json" board-surface-title Tools || return 1
  python3 "$syncs" script "$t/sync-before.json" "$draw_fixture" "$m/sync.json" || return 1
  sync_run typed "$m/sync.json" typed || return 1
  python3 "$harness" tree-text "$t/sync-typed.json" board-save-state Unsaved || return 1
  python3 "$syncs" same "$draw_board" "$draw_fixture" || { echo "written before the drawing rested"; return 1; }
  sync_run autosaved "$m/sync.json" autosaved || return 1
  python3 "$harness" tree-text "$t/sync-autosaved.json" board-save-state Saved || return 1
  cp "$draw_board" "$out/logs/sync-autosaved.excalidraw"
  python3 "$syncs" file "$draw_board" "$draw_fixture" || return 1
  "$roundtrip" "$draw_board" || return 1
  must_fail "the original file as the autosave" python3 "$syncs" file "$draw_fixture" "$draw_fixture" || return 1
  must_fail "Saved on the typed tree" python3 "$harness" tree-text "$t/sync-typed.json" board-save-state Saved || return 1
  # The same run with board autosave off writes nothing (Cmd+S only).
  cp "$draw_fixture" "$draw_board"
  export XDG_CONFIG_HOME="$work/config-sync-off" XDG_STATE_HOME="$work/state-sync-off"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  explicit_save
  "$bin" --vault "$vault" --script "$m/sync.json" --at "mark:end" --scale 1 --tree "$t/sync-off.json" || return 1
  python3 "$harness" tree-text "$t/sync-off.json" board-save-state Unsaved || return 1
  python3 "$syncs" same "$draw_board" "$draw_fixture" || { echo "autosaved with board_autosave off"; return 1; }
  echo "ok with board_autosave off the file is untouched"
}

step_x11_sync() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local tree="$out/trees/sync-before.json"
  [ -f "$tree" ] || { echo "run headless_sync first (needs $tree)"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-sync.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  local wid s="$out/screens" m="$work/sync-x11" slow row new rect
  mkdir -p "$m"
  slow="$(python3 -c "print(max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"
  shot() { sleep "$(python3 -c "print(1.5 * $slow)")"; import -window "$wid" "$s/x11-sync-$1.png"; }
  rest() { sleep "$(python3 -c "print($1 * $slow)")"; }
  row="$(python3 "$harness" tree-center "$tree" board-row-2)" || return 1
  new="$(python3 "$syncs" point "$tree" "$draw_fixture" new)" || return 1
  rect="$(python3 "$syncs" point "$tree" "$draw_fixture" rect)" || return 1
  launch() { # launch NAME: the app on Tools, with this run's settings
    "$bin" --vault "$vault" >>"$out/logs/x11-sync-app.log" 2>&1 &
    app_pid=$!
    wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
    sleep 2
    python3 "$harness" x11-chord "$wid" ctrl+4 && sleep 1 || return 1
    # shellcheck disable=SC2086
    python3 "$harness" x11-click "$wid" $row && shot "$1" || return 1
  }
  quit() {
    python3 "$harness" x11-close "$wid" || return 1
    for i in $(seq 100); do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.1; done
    app_pid=
  }
  # 1. Autosave on (the default). Another app changes the open, clean board:
  #    it is re-read and their text shows, with no click.
  export XDG_CONFIG_HOME="$work/config-x11-sync" XDG_STATE_HOME="$work/state-x11-sync"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  cp "$draw_fixture" "$draw_board"
  launch before || return 1
  python3 "$syncs" external "$draw_fixture" "$draw_board" || return 1
  cp "$draw_board" "$m/theirs.excalidraw"
  rest 2 && shot reloaded || return 1
  python3 "$syncs" theirs "$s/x11-sync-reloaded.png" "$s/x11-sync-before.png" "$tree" "$draw_fixture" || return 1
  must_fail "their text on the before shot" python3 "$syncs" theirs "$s/x11-sync-before.png" "$s/x11-sync-before.png" "$tree" "$draw_fixture" || return 1
  grep -q "reloaded the open board" "$XDG_STATE_HOME/muninn/session.log" || { echo "no reload in the session log"; return 1; }
  echo "ok the clean board was re-read from the other app's write"
  # 2. Text typed on it is autosaved once it rests, on top of their version.
  python3 "$texts" x11-type "$wid" t || return 1
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $new || return 1
  python3 "$texts" x11-type "$wid" auto || return 1
  python3 "$harness" x11-chord "$wid" esc || return 1
  rest 3 && shot autosaved || return 1
  cp "$draw_board" "$out/logs/x11-sync-autosaved.excalidraw"
  python3 "$syncs" file "$draw_board" "$m/theirs.excalidraw" || { echo "not autosaved over their version"; return 1; }
  "$roundtrip" "$draw_board" || return 1
  must_fail "the fixture as the base of the autosave" python3 "$syncs" file "$draw_board" "$draw_fixture" || return 1
  quit || return 1
  # 3. Autosave off: an unsaved nudge, then another app writes the file. The
  #    drawing is kept and flagged, and neither Ctrl+S nor quitting writes
  #    over their file.
  export XDG_CONFIG_HOME="$work/config-x11-sync-dirty" XDG_STATE_HOME="$work/state-x11-sync-dirty"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  explicit_save
  cp "$draw_fixture" "$draw_board"
  launch dirty-before || return 1
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $rect || return 1
  python3 "$harness" x11-chord "$wid" Right && sleep 1 || return 1
  python3 "$syncs" external "$draw_fixture" "$draw_board" || return 1
  cp "$draw_board" "$m/theirs-dirty.excalidraw"
  rest 2 && shot dirty-flagged || return 1
  grep -q "open board changed on disk; kept unsaved" "$XDG_STATE_HOME/muninn/session.log" || { echo "the change was not flagged"; return 1; }
  python3 "$harness" x11-chord "$wid" ctrl+s && shot dirty-save-refused || return 1
  python3 "$syncs" same "$draw_board" "$m/theirs-dirty.excalidraw" || { echo "Ctrl+S wrote over their file"; return 1; }
  quit || return 1
  python3 "$syncs" same "$draw_board" "$m/theirs-dirty.excalidraw" || { echo "quitting wrote over their file"; return 1; }
  ls "$XDG_STATE_HOME/muninn/recovery/" | grep -q 'Tools' || { echo "no recovery copy of the kept drawing"; return 1; }
  echo "ok the dirty board was flagged, their file was never written over, and the drawing has a recovery copy"
  # 4. #41 review GK2, autosave on: typing in keep-text's editor while another
  #    app rewrites that same element. Their change is flagged (not reloaded
  #    under the editor), and Esc's commit is never autosaved over theirs.
  export XDG_CONFIG_HOME="$work/config-x11-sync-text" XDG_STATE_HOME="$work/state-x11-sync-text"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  cp "$draw_fixture" "$draw_board"
  local text
  text="$(python3 "$texts" point "$tree" "$draw_fixture" text)" || return 1
  launch text-before || return 1
  # shellcheck disable=SC2086
  python3 "$texts" x11-double-click "$wid" $text || return 1
  python3 "$texts" x11-type "$wid" " LOCAL" && rest 1 || return 1
  python3 "$syncs" external "$draw_fixture" "$draw_board" || return 1
  cp "$draw_board" "$m/theirs-text.excalidraw"
  rest 2 && shot text-flagged || return 1
  grep -q "open board changed on disk; kept unsaved" "$XDG_STATE_HOME/muninn/session.log" || { echo "their change under the open editor was not flagged"; return 1; }
  grep -q "reloaded the open board" "$XDG_STATE_HOME/muninn/session.log" && { echo "the board was reloaded under the open editor"; return 1; }
  python3 "$harness" x11-chord "$wid" esc && rest 3 && shot text-after-esc || return 1
  python3 "$syncs" same "$draw_board" "$m/theirs-text.excalidraw" || { echo "autosave wrote over their edit"; return 1; }
  ls "$XDG_STATE_HOME/muninn/recovery/" | grep -q 'Tools' || { echo "no recovery copy of the typed text"; return 1; }
  grep -rq "keep me LOCAL" "$XDG_STATE_HOME/muninn/recovery/" || { echo "the recovery copy lacks the typed text"; return 1; }
  quit || return 1
  python3 "$syncs" same "$draw_board" "$m/theirs-text.excalidraw" || { echo "quitting wrote over their edit"; return 1; }
  echo "ok typing under another app's change to the same text: flagged, theirs never written over, mine in the recovery copy"
  # 5. #41 review GK2b: the same with an ATOMIC save by the other app (temp file
  #    renamed over the board, as most apps save). The rename changes the
  #    folder's time, so the vault watcher sees it first; it must follow the
  #    same rule. (a) they change keep-text while I type in it; (b) they delete
  #    it while I type in it; (c) they change it while its editor is open with
  #    nothing typed.
  local case kind log
  # UI_SMOKE_ATOMIC_CASES picks some (the failing-first runs take one each).
  for case in ${UI_SMOKE_ATOMIC_CASES:-a:change:typed b:delete:typed c:change:open}; do
    kind="$(cut -d: -f2 <<<"$case")"
    export XDG_CONFIG_HOME="$work/config-x11-sync-atomic-${case%%:*}" XDG_STATE_HOME="$work/state-x11-sync-atomic-${case%%:*}"
    rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
    log="$XDG_STATE_HOME/muninn/session.log"
    cp "$draw_fixture" "$draw_board"
    launch "atomic-${case%%:*}-before" || return 1
    # shellcheck disable=SC2086
    python3 "$texts" x11-double-click "$wid" $text || return 1
    if [ "${case##*:}" = typed ]; then python3 "$texts" x11-type "$wid" " LOCAL" || return 1; fi
    rest 1
    python3 "$syncs" external-atomic "$draw_fixture" "$draw_board" "$kind" || return 1
    cp "$draw_board" "$m/theirs-atomic.excalidraw"
    rest 2 && shot "atomic-${case%%:*}-theirs" || return 1
    grep -q "reloaded the open board" "$log" && { echo "($case) the board was reloaded under the open editor"; return 1; }
    if [ "${case##*:}" = typed ]; then
      grep -q "open board changed on disk; kept unsaved" "$log" || { echo "($case) their atomic save under the typing was not flagged"; return 1; }
    fi
    python3 "$harness" x11-chord "$wid" esc && rest 3 && shot "atomic-${case%%:*}-after-esc" || return 1
    python3 "$syncs" same "$draw_board" "$m/theirs-atomic.excalidraw" || { echo "($case) Esc and autosave wrote over their atomic save"; return 1; }
    if [ "${case##*:}" = typed ]; then
      grep -rq "keep me LOCAL" "$XDG_STATE_HOME/muninn/recovery/" 2>/dev/null || { echo "($case) the typed LOCAL is not in the recovery copy"; return 1; }
    else
      grep -q "reloaded the open board" "$log" || { echo "($case) their change was not shown once the editor closed"; return 1; }
      python3 "$syncs" theirs "$s/x11-sync-atomic-c-after-esc.png" "$s/x11-sync-before.png" "$tree" "$draw_fixture" || return 1
    fi
    quit || return 1
    python3 "$syncs" same "$draw_board" "$m/theirs-atomic.excalidraw" || { echo "($case) quitting wrote over their atomic save"; return 1; }
    echo "ok ($case) their atomic save: never reloaded under the editor, never written over$( [ "${case##*:}" = typed ] && echo ', mine in the recovery copy' || echo ', shown once the editor closed')"
  done
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

step_x11_chrome() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local tree="$out/trees/sync-before.json"
  [ -f "$tree" ] || { echo "run headless_sync first (needs $tree)"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-chrome.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  local wid s="$out/screens/chrome" chrome="$root/scripts/ui-smoke/board_chrome.py" row rect new
  mkdir -p "$s"
  row="$(python3 "$harness" tree-center "$tree" board-row-2)" || return 1
  rect="$(python3 "$syncs" point "$tree" "$draw_fixture" rect)" || return 1
  new="$(python3 "$syncs" point "$tree" "$draw_fixture" new)" || return 1
  export XDG_CONFIG_HOME="$work/config-x11-chrome" XDG_STATE_HOME="$work/state-x11-chrome"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  cp "$draw_fixture" "$draw_board"
  "$bin" --vault "$vault" >>"$out/logs/x11-chrome-app.log" 2>&1 &
  app_pid=$!
  wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
  sleep 2
  python3 "$harness" x11-chord "$wid" ctrl+4 && sleep 1 || return 1
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $row && sleep 3 || return 1
  # Select, deselect, select again: each click, then 12 quick grabs (about
  # every 40-60 ms) and one once settled, all against the shot before it.
  local step base
  for step in select:rect deselect:new reselect:rect; do
    local name="${step%%:*}" at
    at="$rect"; [ "${step#*:}" = new ] && at="$new"
    # shellcheck disable=SC2086
    python3 "$chrome" x11-burst "$wid" $at "$s" "$name" 12 || return 1
    base="$s/$name-base.png"
    python3 "$chrome" still "$tree" "$base" "$s/$name"-[0-9][0-9].png "$s/$name-settled.png" || { echo "$name: the chrome moved"; return 1; }
  done
  cmp -s "$s/select-base.png" "$s/select-settled.png" && { echo "the select click changed nothing on screen"; return 1; }
  python3 "$chrome" shifted "$s/select-base.png" "$s/shifted.png" 20 || return 1
  must_fail "the chrome moved 20 px" python3 "$chrome" still "$tree" "$s/select-base.png" "$s/shifted.png" || return 1
  # Slice 8: the notes sidebar toggle. The canvas narrows or widens, so the chrome
  # may move, but in one step: no grab shows it between before and settled.
  for step in sidebar-open sidebar-shut; do
    python3 "$chrome" x11-key-burst "$wid" ctrl+backslash "$s" "$step" 12 || return 1
    python3 "$chrome" jump "$s/$step-base.png" "$s/$step-settled.png" "$s/$step"-[0-9][0-9].png || { echo "$step: the chrome slid"; return 1; }
  done
  cmp -s "$s/sidebar-open-base.png" "$s/sidebar-open-settled.png" && { echo "Ctrl+\\ changed nothing on screen"; return 1; }
  python3 "$chrome" midway "$s/sidebar-open-base.png" "$s/sidebar-open-settled.png" "$s/sidebar-midway.png" || return 1
  must_fail "a frame half way through the toggle" python3 "$chrome" jump "$s/sidebar-open-base.png" "$s/sidebar-open-settled.png" "$s/sidebar-midway.png" || return 1
  python3 "$harness" x11-close "$wid" || return 1
  for i in $(seq 100); do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.1; done
  app_pid=
  cmp -s "$draw_fixture" "$draw_board" || { echo "selecting wrote the board"; return 1; }
  echo "ok the hint bar and zoom controls stayed put through select, deselect and select again, and jumped (never slid) with the sidebar toggle"
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

# Slice 8: a vault with no boards. Each run is its own launch on its own settings.
empty_vault="$work/empty-vault"

empty_run() { # empty_run NAME SCRIPT MARK
  export XDG_CONFIG_HOME="$work/config-new-$1" XDG_STATE_HOME="$work/state-new-$1"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  "$bin" --vault "$empty_vault" --script "$2" --at "mark:$3+0.25" --scale 1 \
    --snap "$out/screens/new-$1.png" --tree "$out/trees/new-$1.json"
}

listing() { (cd "$empty_vault" && find . -type f -not -path './.muninn*' | sort | xargs sha256sum); }

step_headless_new_board() {
  isolate
  local t="$out/trees" m="$work/new"
  rm -rf "$empty_vault" "$m"; mkdir -p "$empty_vault/Ideas" "$m"
  printf '# Start\n\nNo boards here yet.\n' >"$empty_vault/Start.md"
  printf '# Idea\n' >"$empty_vault/Ideas/Idea.md"
  listing >"$m/before.txt"
  # 1. Board on a vault with no boards: nothing written, sidebar collapsed, New board offered.
  printf '{"events":[{"t":0.5,"type":"key","name":"4","cmd":true},{"t":1.5,"type":"mark","name":"empty"}]}' >"$m/empty.json"
  empty_run empty "$m/empty.json" empty || return 1
  python3 "$harness" tree-mode "$t/new-empty.json" board || return 1
  python3 "$harness" tree-text "$t/new-empty.json" board-new-empty "New board" || return 1
  python3 "$harness" tree-text "$t/new-empty.json" board-list-empty "No boards in this vault yet" || return 1
  python3 "$harness" tree-text "$t/new-empty.json" board-surface-title "No boards yet" || return 1
  no_key "$t/new-empty.json" search-box || return 1
  listing >"$m/after-open.txt"
  diff "$m/before.txt" "$m/after-open.txt" || { echo "opening Board wrote into the vault"; return 1; }
  find "$empty_vault" -name '*.excalidraw' | grep . && { echo "a board appeared without New board"; return 1; }
  echo "ok entering Board on a vault with no boards wrote nothing"
  # 2. New board in the empty state: an empty Untitled board, open. With no board open
  #    it goes where New note would: the selected note's folder (Ideas/Idea.md is the
  #    note a fresh launch shows), never the vault root by surprise.
  printf '{"events":[{"t":0.5,"type":"key","name":"4","cmd":true},{"t":1.5,"type":"tap","key":"board-new-empty"},{"t":2.5,"type":"mark","name":"created"}]}' >"$m/create.json"
  empty_run created "$m/create.json" created || return 1
  [ -f "$empty_vault/Ideas/Untitled board.excalidraw" ] || { echo "no Untitled board.excalidraw"; ls -R "$empty_vault"; return 1; }
  python3 "$harness" tree-text "$t/new-created.json" board-surface-title "Untitled board" || return 1
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); assert d["type"]=="excalidraw" and d["elements"]==[], d; print("ok empty scene", sys.argv[1])' \
    "$empty_vault/Ideas/Untitled board.excalidraw" || return 1
  "$roundtrip" "$empty_vault/Ideas/Untitled board.excalidraw" || return 1
  must_fail "the empty-state tree as a created board" python3 "$harness" tree-text "$t/new-empty.json" board-surface-title "Untitled board" || return 1
  # 3. New board again from the panel header: never overwrites, takes the next name.
  printf '{"events":[{"t":0.5,"type":"key","name":"4","cmd":true},{"t":1.5,"type":"tap","key":"board-new"},{"t":2.5,"type":"mark","name":"second"}]}' >"$m/second.json"
  empty_run second "$m/second.json" second || return 1
  [ -f "$empty_vault/Ideas/Untitled board 2.excalidraw" ] || { echo "no Untitled board 2"; ls -R "$empty_vault"; return 1; }
  python3 "$harness" tree-text "$t/new-second.json" board-surface-title "Untitled board 2" || return 1
  # 4. Ctrl+\ reopens the notes sidebar on Board; Graph (leaving Board) shows it.
  printf '{"events":[{"t":0.5,"type":"key","name":"4","cmd":true},{"t":1.0,"type":"mark","name":"shut"},{"t":1.8,"type":"key","name":"\\\\","cmd":true},{"t":2.4,"type":"mark","name":"open"},{"t":3.2,"type":"key","name":"\\\\","cmd":true},{"t":3.6,"type":"key","name":"3","cmd":true},{"t":4.2,"type":"mark","name":"left"}]}' >"$m/toggle.json"
  empty_run shut "$m/toggle.json" shut || return 1
  empty_run open "$m/toggle.json" open || return 1
  empty_run left "$m/toggle.json" left || return 1
  no_key "$t/new-shut.json" search-box || return 1
  python3 "$harness" tree-center "$t/new-open.json" search-box || { echo "Ctrl+\\ did not reopen the sidebar"; return 1; }
  python3 "$harness" tree-mode "$t/new-left.json" graph || return 1
  python3 "$harness" tree-center "$t/new-left.json" search-box || { echo "leaving Board did not restore the sidebar"; return 1; }
  ls -R "$empty_vault"
}

step_x11_new_board() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local tree="$out/trees/new-empty.json"
  [ -f "$tree" ] || { echo "run headless_new_board first (needs $tree)"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-new.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  local wid s="$out/screens" slow at toggle
  slow="$(python3 -c "print(max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"
  shot() { sleep "$(python3 -c "print(1.5 * $slow)")"; import -window "$wid" "$s/x11-new-$1.png"; }
  at="$(python3 "$harness" tree-center "$tree" board-new-empty)" || return 1
  toggle="$(python3 "$harness" tree-center "$tree" sidebar-toggle)" || return 1
  rm -rf "$empty_vault"; mkdir -p "$empty_vault/Ideas"
  printf '# Start\n\nNo boards here yet.\n' >"$empty_vault/Start.md"
  printf '# Idea\n' >"$empty_vault/Ideas/Idea.md"
  export XDG_CONFIG_HOME="$work/config-x11-new" XDG_STATE_HOME="$work/state-x11-new"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  "$bin" --vault "$empty_vault" >>"$out/logs/x11-new-app.log" 2>&1 &
  app_pid=$!
  wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
  sleep 2
  shot 0-write || return 1
  python3 "$harness" x11-chord "$wid" ctrl+4 && shot 1-board-empty || return 1
  find "$empty_vault" -name '*.excalidraw' | grep . && { echo "a board appeared just by entering Board"; return 1; }
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $at && shot 2-created || return 1
  [ -f "$empty_vault/Ideas/Untitled board.excalidraw" ] || { echo "clicking New board made no file"; ls -R "$empty_vault"; return 1; }
  grep -q 'created Ideas/Untitled board.excalidraw' "$XDG_STATE_HOME/muninn/session.log" || { echo "no 'created' line in the session log"; return 1; }
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $toggle && shot 3-sidebar-open || return 1
  cmp -s "$s/x11-new-2-created.png" "$s/x11-new-3-sidebar-open.png" && { echo "the sidebar toggle changed nothing"; return 1; }
  python3 "$harness" x11-chord "$wid" ctrl+backslash && shot 4-sidebar-shut || return 1
  python3 "$harness" x11-chord "$wid" ctrl+3 && shot 5-graph || return 1
  # GK MF1: search focused (sidebar shown in Graph), then Board collapses the
  # sidebar; the hidden field must let go, so R picks the rectangle tool and a
  # drag on the canvas draws one (with search still focused, R went into the
  # search box and the drag only drew a marquee).
  local cx cy
  read -r cx cy _ <<<"$(python3 -c 'import json,sys; sys.path.insert(0, sys.argv[2]); import board_pixels as b; r=b.canvas_rect(sys.argv[1]); print(int(r[0]), int(r[1]))' "$out/trees/new-created.json" "$root/scripts/ui-smoke")"
  python3 "$harness" x11-chord "$wid" ctrl+k && sleep 0.5 || return 1
  python3 "$harness" x11-chord "$wid" ctrl+4 && shot 6-board-from-search || return 1
  python3 "$harness" x11-chord "$wid" r || return 1
  python3 "$harness" x11-drag "$wid" $(( cx + 300 )) $(( cy + 300 )) 120 80 || return 1
  python3 "$harness" x11-chord "$wid" v || return 1
  python3 "$harness" x11-chord "$wid" ctrl+s && shot 7-drawn || return 1
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); n=[e["type"] for e in d["elements"] if not e.get("isDeleted")]; assert n==["rectangle"], ("after Ctrl+K then Ctrl+4, R and a drag should draw one rectangle", n); print("ok R drew a rectangle after leaving search for Board")' \
    "$empty_vault/Ideas/Untitled board.excalidraw" || return 1
  python3 "$harness" x11-close "$wid" || return 1
  for i in $(seq 100); do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.1; done
  app_pid=
  "$roundtrip" "$empty_vault/Ideas/Untitled board.excalidraw" || return 1
  ls -R "$empty_vault"
  echo "ok New board under X11 made and opened Untitled board; the sidebar toggled by button and Ctrl+\\"
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

step_x11_board_rename() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local tree="$out/trees/new-empty.json" made="$out/trees/new-created.json"
  [ -f "$made" ] || { echo "run headless_new_board first (needs $made)"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-rename.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  local wid s="$out/screens" slow at cx cy tx ty tw th rx ry rw rh d="$empty_vault/Ideas"
  slow="$(python3 -c "print(max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"
  shot() { sleep "$(python3 -c "print(1.5 * $slow)")"; import -window "$wid" "$s/x11-rename-$1.png"; }
  rects() { python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print([e["type"] for e in d["elements"] if not e.get("isDeleted")].count("rectangle"))' "$1"; }
  at="$(python3 "$harness" tree-center "$tree" board-new-empty)" || return 1
  read -r cx cy _ <<<"$(python3 -c 'import json,sys; sys.path.insert(0, sys.argv[2]); import board_pixels as b; r=b.canvas_rect(sys.argv[1]); print(int(r[0]), int(r[1]))' "$made" "$root/scripts/ui-smoke")"
  read -r tx ty tw th <<<"$(python3 "$harness" tree-rect "$made" board-surface-title)" || return 1
  read -r rx ry rw rh <<<"$(python3 "$harness" tree-rect "$made" board-row-0)" || return 1
  draw() { # draw DX: a rectangle with the R tool on the canvas
    python3 "$harness" x11-chord "$wid" r || return 1
    python3 "$harness" x11-drag "$wid" $(( cx + 260 + $1 )) $(( cy + 260 )) 110 70 || return 1
    python3 "$harness" x11-chord "$wid" v
  }
  rm -rf "$empty_vault"; mkdir -p "$d"
  printf '# Start\n\nNo boards here yet.\n' >"$empty_vault/Start.md"
  printf '# Idea\n\n![[Untitled board.excalidraw]]\n' >"$d/Idea.md"
  export XDG_CONFIG_HOME="$work/config-x11-rename" XDG_STATE_HOME="$work/state-x11-rename"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME/muninn" "$XDG_STATE_HOME"
  # Explicit save, so the drawing is still unsaved when F2 renames it.
  printf '{"board_autosave": false}\n' >"$XDG_CONFIG_HOME/muninn/settings.json"
  local log="$XDG_STATE_HOME/muninn/session.log"
  "$bin" --vault "$empty_vault" >>"$out/logs/x11-rename-app.log" 2>&1 &
  app_pid=$!
  wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
  sleep 2
  python3 "$harness" x11-chord "$wid" ctrl+4 && sleep 1 || return 1
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $at && shot 0-created || return 1
  [ -f "$d/Untitled board.excalidraw" ] || { echo "New board made no file"; ls -R "$empty_vault"; return 1; }
  draw 0 && shot 1-drawn || return 1
  [ "$(rects "$d/Untitled board.excalidraw")" = 0 ] || { echo "the drawing was saved before the rename"; return 1; }
  # 1. F2 on Board: type a name, Enter.
  python3 "$harness" x11-chord "$wid" F2 && shot 2-f2-field || return 1
  python3 "$harness" x11-type "$wid" Roadmap || return 1
  python3 "$harness" x11-chord "$wid" enter && shot 3-renamed || return 1
  [ -f "$d/Roadmap.excalidraw" ] || { echo "F2 + Roadmap + Enter renamed nothing"; ls -R "$empty_vault"; return 1; }
  [ -e "$d/Untitled board.excalidraw" ] && { echo "the old path is still there"; return 1; }
  [ "$(rects "$d/Roadmap.excalidraw")" = 1 ] || { echo "the unsaved rectangle was not saved before the rename"; return 1; }
  grep -q 'renamed Ideas/Untitled board.excalidraw to Ideas/Roadmap.excalidraw' "$log" || { echo "no 'renamed' line in the session log"; return 1; }
  grep -q '!\[\[Roadmap.excalidraw\]\]' "$d/Idea.md" || { echo "the embed in Idea.md did not follow"; cat "$d/Idea.md"; return 1; }
  echo "ok F2 renamed Ideas/Untitled board.excalidraw to Ideas/Roadmap.excalidraw (drawing saved first; embed followed)"
  # 2. The watchers look (every 1.5 s) and do not flag Muninn's own rename.
  sleep 4
  grep -q 'changed on disk' "$log" && { echo "the rename was flagged as a change on disk"; grep 'changed on disk' "$log"; return 1; }
  # 3. Still open and editable: a second rectangle saves to the new path.
  draw 160 || return 1
  python3 "$harness" x11-chord "$wid" ctrl+s && shot 4-saved || return 1
  [ "$(rects "$d/Roadmap.excalidraw")" = 2 ] || { echo "Ctrl+S did not save to the new path ($(rects "$d/Roadmap.excalidraw") rectangles)"; return 1; }
  [ -e "$d/Untitled board.excalidraw" ] && { echo "a save went back to the old path"; return 1; }
  echo "ok the renamed board stayed open, and Ctrl+S saved 2 rectangles to Roadmap.excalidraw"
  # 4. A double click on the title above the canvas.
  python3 "$harness" x11-click "$wid" $(( tx + 12 )) $(( ty + th / 2 )) 1 2 && shot 5-title-field || return 1
  python3 "$harness" x11-type "$wid" Sketch || return 1
  python3 "$harness" x11-chord "$wid" enter && shot 6-title-renamed || return 1
  [ -f "$d/Sketch.excalidraw" ] && [ ! -e "$d/Roadmap.excalidraw" ] || { echo "double-click on the title did not rename"; ls -R "$empty_vault"; return 1; }
  echo "ok a double click on the title renamed Roadmap to Sketch"
  # 5. Rename in the board row's right-click menu (its first item, under the row).
  python3 "$harness" x11-click "$wid" $(( rx + rw / 2 )) $(( ry + rh / 2 )) 3 && shot 7-menu || return 1
  python3 "$harness" x11-click "$wid" $(( rx + rw / 2 )) $(( ry + rh + 21 )) && shot 8-menu-field || return 1
  python3 "$harness" x11-type "$wid" Plan || return 1
  python3 "$harness" x11-chord "$wid" enter && shot 9-menu-renamed || return 1
  [ -f "$d/Plan.excalidraw" ] && [ ! -e "$d/Sketch.excalidraw" ] || { echo "Rename in the row menu did not rename"; ls -R "$empty_vault"; return 1; }
  echo "ok Rename in the row's right-click menu renamed Sketch to Plan"
  # 4. GK B-1: a multibyte name whose byte length (12) put the old
  # `.excalidraw` check mid-character crashed the app on Enter. F2 to it,
  # then back to Plan for the checks below.
  python3 "$harness" x11-chord "$wid" F2 || return 1
  python3 "$harness" x11-type-unicode "$wid" "é ünïcode" || return 1
  python3 "$harness" x11-chord "$wid" enter && shot 10-multibyte || return 1
  kill -0 "$app_pid" 2>/dev/null || { echo "the app exited on a multibyte name"; tail -n 20 "$log"; return 1; }
  [ -f "$d/é ünïcode.excalidraw" ] && [ ! -e "$d/Plan.excalidraw" ] || { echo "F2 + 'é ünïcode' + Enter did not rename"; ls -R "$empty_vault"; return 1; }
  grep -q '!\[\[é ünïcode.excalidraw\]\]' "$d/Idea.md" || { echo "the embed did not follow the multibyte name"; cat "$d/Idea.md"; return 1; }
  echo "ok F2 renamed Plan to 'é ünïcode' (12 bytes) without a crash; embed followed"
  python3 "$harness" x11-chord "$wid" F2 || return 1
  python3 "$harness" x11-type "$wid" Plan || return 1
  python3 "$harness" x11-chord "$wid" enter || return 1
  [ -f "$d/Plan.excalidraw" ] && [ ! -e "$d/é ünïcode.excalidraw" ] || { echo "F2 back to Plan did not rename"; ls -R "$empty_vault"; return 1; }
  python3 "$harness" x11-close "$wid" || return 1
  for i in $(seq 100); do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.1; done
  app_pid=
  [ "$(rects "$d/Plan.excalidraw")" = 2 ] || { echo "Plan.excalidraw lost drawing"; return 1; }
  "$roundtrip" "$d/Plan.excalidraw" || return 1
  find "$XDG_STATE_HOME" -iname '*untitled_board*' -o -iname '*roadmap*' -o -iname '*sketch*' | grep . && { echo "a recovery copy is left under an old name"; return 1; }
  grep -q '!\[\[Plan.excalidraw\]\]' "$d/Idea.md" || { echo "the embed did not follow every rename"; cat "$d/Idea.md"; return 1; }
  grep 'renamed' "$log"
  ls -R "$empty_vault"
  echo "ok board rename under X11: F2, title double click and row menu; the file moved each time and the board stayed open"
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

step_x11_single_instance() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  need dbus-run-session dbus || return 1
  need gdbus glib2 || return 1
  python3 -c 'import Xlib; from gi.repository import Gio' 2>/dev/null || { echo "missing python-xlib or python-gobject"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  trap '[ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local single="$work/single"
  rm -rf "$single"; mkdir -p "$single/a" "$single/b"
  printf '# Alpha\n\nVault A of the single-instance step.\n' >"$single/a/Alpha.md"
  printf '# Beta\n\nVault B of the single-instance step.\n' >"$single/b/Beta.md"
  export XDG_CONFIG_HOME="$work/config-single" XDG_STATE_HOME="$work/state-single"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME/muninn" "$XDG_STATE_HOME"
  # Where Settings' tabs and the Show tray icon switch are: headless dumps
  # (no window, no bus, no lock) at the window's size.
  local mark
  for mark in vaults appearance; do
    "$bin" --vault "$single/a" --scene "settings-$mark" --scale 1 \
      --snap "$out/screens/single-settings-$mark.png" --tree "$out/trees/single-settings-$mark.json" || return 1
  done
  printf '{"tray_icon": true, "close_to_tray": true}\n' >"$XDG_CONFIG_HOME/muninn/settings.json"
  local n
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-single.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd, runtime dir $XDG_RUNTIME_DIR"
  bin="$bin" harness="$harness" out="$out" vault_a="$single/a" vault_b="$single/b" \
    vaults_tree="$out/trees/single-settings-vaults.json" \
    appearance_tree="$out/trees/single-settings-appearance.json" \
    dbus-run-session -- bash "$root/scripts/ui-smoke/single_instance.sh" || return 1
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

# Kanban slice 1 uses its own vault. The shared seed vault stays at two notes.
kanban_vault_path() { echo "$work/kanban-vault"; }

seed_kanban_vault() {
  local kvault="$1" f base
  rm -rf "$kvault"
  mkdir -p "$kvault"
  for f in "$kanban_fixtures"/*.md; do
    base="$(basename "$f")"
    case "$base" in
      README.md|bad-*) continue ;;
    esac
    cp "$f" "$kvault/$base"
  done
  printf '# Roadmap\n\nThe plan the cards link to.\n' >"$kvault/Roadmap.md"
  printf '# 2026-11-11\n\nA daily note.\n' >"$kvault/2026-11-11.md"
}

kanban_hashes() { # kanban_hashes VAULT FILE
  (
    cd "$1" || exit 1
    find . -name '*.md' -type f | sort | while IFS= read -r f; do sha256sum "$f"; done
  ) >"$2"
}

step_headless_kanban_model() {
  isolate
  local icd=""
  icd="$(find_lavapipe || true)"
  if [ -n "$icd" ]; then
    export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd" WGPU_BACKEND=vulkan
    echo "vulkan icd: $icd"
  fi
  command -v vulkaninfo >/dev/null && vulkaninfo --summary 2>/dev/null | grep -E 'deviceName|driverName|driverInfo'
  local kvault f base out_text notes report links id road
  kvault="$(kanban_vault_path)"
  seed_kanban_vault "$kvault"
  for f in "$kanban_fixtures"/*.md; do
    base="$(basename "$f")"
    case "$base" in
      README.md|bad-*) continue ;;
    esac
    "$kanban_roundtrip" "$f" || return 1
  done
  printf 'rename-lane 1 Busy\n' >"$work/kanban-rename.edit"
  out_text="$("$kanban_roundtrip" "$kanban_fixtures/lanes-and-limits.md" --edit "$work/kanban-rename.edit")" || return 1
  printf '%s\n' "$out_text" | sed -n '/^--- kanban$/,$p' >"$work/kanban-rename.actual"
  diff -u "$root/scripts/ui-smoke/kanban-rename.diff" "$work/kanban-rename.actual" || return 1
  echo "ok rename-lane diff matches"
  report="$("$bin" --check "$kvault" || true)"
  echo "$report"
  notes="$(find "$kvault" -name '*.md' -type f | wc -l | tr -d ' ')"
  grep -qx "notes: $notes" <<<"$report" || { echo "expected notes: $notes"; return 1; }
  grep -qx 'broken links: 0' <<<"$report" || return 1
  links="$("$bin" --list-links "$kvault")" || return 1
  echo "$links"
  grep -qx 'card-text.md -> Roadmap.md' <<<"$links" || return 1
  grep -qx 'card-text.md -> 2026-11-11.md' <<<"$links" || return 1
  grep -qx 'spec-example.md -> Roadmap.md' <<<"$links" || return 1
  grep -q 'Hidden' <<<"$links" && { echo "a wikilink inside code was indexed"; return 1; }
  # Note ids come from the app's own sidebar, not a second copy of the sort.
  printf '{"events":[{"t":0.8,"type":"mark","name":"list"}]}' >"$work/kanban-list.json"
  export XDG_CONFIG_HOME="$work/config-kanban-list"
  rm -rf "$XDG_CONFIG_HOME"
  mkdir -p "$XDG_CONFIG_HOME"
  "$bin" --vault "$kvault" --script "$work/kanban-list.json" \
    --at "mark:list+0.3" --scale 1 --tree "$out/trees/kanban-s1-list.json" || return 1
  read -r id road <<<"$(python3 - "$out/trees/kanban-s1-list.json" <<'PY'
import json, sys

def walk(node):
    if isinstance(node, dict):
        yield node
        for value in node.values():
            yield from walk(value)
    elif isinstance(node, list):
        for value in node:
            yield from walk(value)

def window_height(tree):
    node = tree.get("accessibility") or tree
    bounds = node.get("bounds") or [0, 0, 0, 0]
    return bounds[3]

def row_id(tree, title, on_screen=False):
    height = window_height(tree)
    for node in walk(tree):
        key = str(node.get("key") or "")
        name = str(node.get("name") or "")
        if key.startswith("note-row-") and (name == title or name.startswith(title + " ")):
            if on_screen:
                bounds = node.get("bounds") or [0, 0, 0, 0]
                y, h = bounds[1], bounds[3]
                if y < 0 or y + h > height:
                    raise SystemExit(f"{title} row is outside the window (y={y}, h={h}, window={height})")
            return key.removeprefix("note-row-")
    raise SystemExit(f"no note row titled {title!r}")

tree = json.load(open(sys.argv[1]))
print(row_id(tree, "card-text", True), row_id(tree, "Roadmap"))
PY
)"
  echo "card-text id $id, Roadmap id $road"
  printf '%s\n' "$id" >"$work/kanban-note-id"
  printf '%s\n' "$road" >"$work/kanban-road-id"
  # Roadmap's row sits below the sidebar fold. Open it from the card's
  # outgoing rail link, which is on screen, so the kanban shows in its backlinks.
  printf '{"events":[{"t":0.6,"type":"tap","key":"note-row-%s"},{"t":1.2,"type":"mark","name":"write"},{"t":2.0,"type":"key","name":"2","cmd":true},{"t":2.6,"type":"mark","name":"read"},{"t":3.4,"type":"tap","key":"rail-link-1-%s"},{"t":4.4,"type":"mark","name":"backlinks"}]}' \
    "$id" "$road" >"$work/kanban-model.json"
  kanban_hashes "$kvault" "$work/kanban.sha256"
  local mark mode
  for mark in write:editor read:preview backlinks:preview; do
    mode="${mark#*:}"
    mark="${mark%%:*}"
    export XDG_CONFIG_HOME="$work/config-kanban-$mark"
    rm -rf "$XDG_CONFIG_HOME"
    mkdir -p "$XDG_CONFIG_HOME"
    "$bin" --vault "$kvault" --script "$work/kanban-model.json" \
      --at "mark:$mark+0.4" --scale 1 \
      --snap "$out/screens/kanban-s1-$mark.png" --tree "$out/trees/kanban-s1-$mark.json" || return 1
    python3 "$harness" tree-mode "$out/trees/kanban-s1-$mark.json" "$mode" || return 1
  done
  python3 - "$out/trees" <<'PY' || return 1
import json, sys
trees = sys.argv[1]

def walk(node):
    if isinstance(node, dict):
        yield node
        for value in node.values():
            yield from walk(value)
    elif isinstance(node, list):
        for value in node:
            yield from walk(value)

def load(name):
    return json.load(open(f"{trees}/kanban-s1-{name}.json"))

def blob(name):
    return json.dumps(load(name))

def rail_has(name, needle):
    for node in walk(load(name)):
        if not isinstance(node, dict):
            continue
        key = str(node.get("key") or "")
        if key.startswith("rail-link-") and needle in json.dumps(node):
            print(f"ok {name}: {key} mentions {needle}")
            return
    raise SystemExit(f"FAIL {name}: no rail link mentions {needle}")

write = blob("write")
for needle in ("card-text", "kanban-plugin", "[[Roadmap]]"):
    if needle not in write:
        raise SystemExit(f"FAIL write tree missing {needle!r}")
    print(f"ok write tree has {needle!r}")
read = blob("read")
for needle in ("To do", "Roadmap", "card-text"):
    if needle not in read:
        raise SystemExit(f"FAIL read tree missing {needle!r}")
    print(f"ok read tree has {needle!r}")
rail_has("write", "Roadmap")
rail_has("backlinks", "card-text")
PY
  (cd "$kvault" && sha256sum -c "$work/kanban.sha256") || { echo "the headless run changed a kanban file"; return 1; }
  echo "ok kanban files unchanged"
}

step_x11_kanban_model() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local tree="$out/trees/kanban-s1-write.json"
  [ -f "$tree" ] || { echo "run headless_kanban_model first (needs $tree)"; return 1; }
  local icd kvault id xy
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  echo "vulkan icd: $icd"
  command -v vulkaninfo >/dev/null && vulkaninfo --summary 2>/dev/null | grep -E 'deviceName|driverName|driverInfo'
  kvault="$(kanban_vault_path)"
  [ -d "$kvault" ] || seed_kanban_vault "$kvault"
  id="$(cat "$work/kanban-note-id")"
  xy="$(python3 "$harness" tree-center "$tree" "note-row-$id")" || return 1
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-kanban.log" 2>&1 &
  xvfb_pid=$!
  local i
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  kanban_hashes "$kvault" "$work/kanban-x11.sha256"
  export XDG_CONFIG_HOME="$work/config-x11-kanban" XDG_STATE_HOME="$work/state-x11-kanban"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  "$bin" --new-instance --vault "$kvault" >>"$out/logs/x11-kanban-app.log" 2>&1 &
  app_pid=$!
  local wid
  wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
  sleep 2
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $xy || return 1
  sleep "$(python3 -c "print(1.5 * max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"
  import -window "$wid" "$out/screens/kanban-s1-x11.png" || return 1
  python3 - "$out/screens/kanban-s1-write.png" "$out/screens/kanban-s1-x11.png" <<'PY' || return 1
import sys
from PIL import Image, ImageChops
headless, windowed = (Image.open(path).convert("RGB") for path in sys.argv[1:])
if headless.size != windowed.size:
    raise SystemExit(f"size {headless.size} vs {windowed.size}")
diff = ImageChops.difference(headless, windowed)
mean = sum(diff.resize((1, 1), Image.Resampling.BOX).getpixel((0, 0))) / 3
print(f"x11 vs headless write mean abs diff {mean:.2f}")
if mean > 12:
    raise SystemExit(f"the windowed kanban note does not match the headless Write shot ({mean:.2f})")
print("ok x11 screenshot matches the headless Write shot")
PY
  python3 "$harness" x11-close "$wid" || return 1
  local status=124
  for i in $(seq 100); do
    if ! kill -0 "$app_pid" 2>/dev/null; then wait "$app_pid"; status=$?; break; fi
    sleep 0.1
  done
  app_pid=
  echo "app exit status: $status"
  [ "$status" = 0 ] || return 1
  (cd "$kvault" && sha256sum -c "$work/kanban-x11.sha256") || { echo "the windowed app changed a kanban file"; return 1; }
  echo "ok x11 kanban note opened and the file is unchanged"
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

# Kanban slice 2: the read-only Kanban page on its own vault.
kanban_view_vault="$work/kanban-view-vault"

seed_kanban_view_vault() {
  rm -rf "$kanban_view_vault"
  mkdir -p "$kanban_view_vault"
  printf '# Agenda\n\nA plain note.\n' >"$kanban_view_vault/Agenda.md"
  cp "$kanban_fixtures/lanes-and-limits.md" "$kanban_view_vault/Release.md"
  cp "$kanban_fixtures/over-limit.md" "$kanban_view_vault/Overloaded.md"
  cp "$kanban_fixtures/bad-json.md" "$kanban_view_vault/Torn.md"
  kanban_hashes "$kanban_view_vault" "$work/kanban-view.sha256"
}

kanban_view_unchanged() { # kanban_view_unchanged WHAT
  (cd "$kanban_view_vault" && sha256sum -c "$work/kanban-view.sha256") || { echo "$1 changed a file"; return 1; }
  echo "ok every file unchanged after $1"
}

kanban_gpu() { # lavapipe when present, as the slice 1 steps; print the adapter
  local icd=""
  icd="$(find_lavapipe || true)"
  if [ -n "$icd" ]; then
    export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd" WGPU_BACKEND=vulkan
    echo "vulkan icd: $icd (WGPU_BACKEND=vulkan)"
  fi
  command -v vulkaninfo >/dev/null && vulkaninfo --summary 2>/dev/null | grep -E 'deviceName|driverName|driverInfo'
  return 0
}

kanban_row_ids() { # print "ROW" for each title, from the app's own sidebar tree $1
  python3 - "$@" <<'PY'
import json, sys
tree = json.load(open(sys.argv[1]))
def walk(node):
    if isinstance(node, dict):
        yield node
        for value in node.values():
            yield from walk(value)
    elif isinstance(node, list):
        for value in node:
            yield from walk(value)
rows = {}
for node in walk(tree.get("accessibility") or tree):
    key = str(node.get("key") or "")
    if key.startswith("note-row-"):
        rows[str(node.get("name") or "").split(" ")[0]] = key
print(" ".join(rows[title] for title in sys.argv[2:]))
PY
}

step_headless_kanban() {
  isolate
  kanban_gpu
  seed_kanban_view_vault
  printf '{"events":[{"t":0.8,"type":"mark","name":"list"}]}' >"$work/kanban-view-list.json"
  export XDG_CONFIG_HOME="$work/config-kanban-view-list"
  rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
  "$bin" --vault "$kanban_view_vault" --script "$work/kanban-view-list.json" \
    --at "mark:list+0.3" --scale 1 --tree "$out/trees/kanban-list.json" || return 1
  local release overloaded torn
  read -r release overloaded torn <<<"$(kanban_row_ids "$out/trees/kanban-list.json" Release Overloaded Torn)"
  [ -n "$torn" ] || { echo "missing note rows"; return 1; }
  echo "rows: Release $release, Overloaded $overloaded, Torn $torn"
  sed -e "s/@RELEASE@/$release/" -e "s/@OVERLOADED@/$overloaded/" -e "s/@TORN@/$torn/" \
    "$root/scripts/ui-smoke/kanban.json" >"$work/kanban-view.json"
  local pair mark mode
  for pair in empty:kanban readonly:kanban wip-over:kanban unreadable:kanban write:editor; do
    mark="${pair%%:*}" mode="${pair#*:}"
    export XDG_CONFIG_HOME="$work/config-kanban-view-$mark"
    rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
    "$bin" --vault "$kanban_view_vault" --script "$work/kanban-view.json" \
      --at "mark:$mark+0.4" --scale 1 \
      --snap "$out/screens/kanban-$mark.png" --tree "$out/trees/kanban-$mark.json" || return 1
    python3 "$harness" tree-mode "$out/trees/kanban-$mark.json" "$mode" || return 1
  done
  python3 - "$out/trees" <<'PY' || return 1
import json, sys
trees = sys.argv[1]

def walk(node):
    if isinstance(node, dict):
        yield node
        for value in node.values():
            yield from walk(value)
    elif isinstance(node, list):
        for value in node:
            yield from walk(value)

def texts(name):
    """key -> text for every keyed element, and all text, in the tree."""
    keyed, every = {}, []
    for node in walk(json.load(open(f"{trees}/kanban-{name}.json")).get("elements", [])):
        text = node.get("text")
        if isinstance(text, str):
            every.append(text)
            if node.get("key"):
                keyed[node["key"]] = text
        elif node.get("key"):
            keyed.setdefault(node["key"], None)
    return keyed, "\n".join(every)

def expect(name, lanes, counts, over=(), complete=(), cards=(), absent=()):
    keyed, every = texts(name)
    got = [keyed.get(f"kanban-lane-{i}-title") for i in range(len(lanes) + 1)]
    if got != list(lanes) + [None]:
        raise SystemExit(f"FAIL {name}: lanes {got}, expected {list(lanes)}")
    got = [keyed.get(f"kanban-lane-{i}-count") for i in range(len(lanes))]
    if got != list(counts):
        raise SystemExit(f"FAIL {name}: counts {got}, expected {list(counts)}")
    for i in range(len(lanes)):
        if (keyed.get(f"kanban-lane-{i}-over") == "Over limit") != (i in over):
            raise SystemExit(f"FAIL {name}: lane {i} over-limit flag is wrong")
        if (f"kanban-lane-{i}-complete" in keyed) != (i in complete):
            raise SystemExit(f"FAIL {name}: lane {i} Complete indicator is wrong")
    for needle in cards:
        if needle not in every:
            raise SystemExit(f"FAIL {name}: card text {needle!r} not shown")
    for needle in absent:
        if needle in every:
            raise SystemExit(f"FAIL {name}: {needle!r} is shown")
    if complete and "Complete" not in every:
        raise SystemExit(f"FAIL {name}: no Complete label")
    print(f"ok {name}: lanes {list(lanes)}, counts {list(counts)}, over {list(over)}, complete {list(complete)}")

keyed, _ = texts("empty")
if keyed.get("kanban-placeholder-title") != "No kanban open" or "kanban-lane-0" in keyed:
    raise SystemExit("FAIL empty: no empty state")
print("ok empty: No kanban open")
expect("readonly", ["Backlog", "Doing", "Review", "Done"], ["1", "2/3", "1", "1"],
       cards=["Write the threat model", "Parser", "Writer", "Spell the open points",
              "Choose the branch name"], absent=["(3)", "(0)", "- [ ]"])
expect("wip-over", ["Ideas", "Doing", "Shipped"], ["1", "3/2", "1"], over=[1], complete=[2],
       cards=["Sketch the lane header", "Parse the board", "Draw the lanes", "Label the cards",
              "with a second line", "Detect kanban notes"],
       absent=["A paragraph between lanes", "**Complete**"])
keyed, _ = texts("unreadable")
if keyed.get("kanban-placeholder-title") != "This note can't be shown as a kanban":
    raise SystemExit("FAIL unreadable: no unreadable message")
if "kanban-open-markdown" not in keyed or "kanban-lane-0" in keyed:
    raise SystemExit("FAIL unreadable: no Open as markdown, or lanes drawn")
print("ok unreadable: message and Open as markdown")
keyed, every = texts("write")
if "kanban-page" in keyed or "kanban:settings" not in every:
    raise SystemExit("FAIL write: Torn is not open as markdown")
print("ok write: Torn open in Write")
PY
  kanban_view_unchanged "the headless kanban runs" || return 1
  kanban_links
}

# A card's wikilink to no note reads dimmed (Read's missing-link style), one to a
# note in the accent: Links.md has a card [[Agenda]] and a card [[Nowhere]], opened
# with Cmd+K, "links", Enter; the ink in each card is measured in the screenshot.
kanban_links() {
  local v="$work/kanban-links-vault" t="$out/trees" m="$work/kanban-links"
  rm -rf "$v" "$m"; mkdir -p "$v" "$m"
  printf '# Agenda\n\nA plain note.\n' >"$v/Agenda.md"
  printf -- '---\nkanban-plugin: board\n---\n\n## Links\n\n- [ ] [[Agenda]]\n- [ ] [[Nowhere]]\n' >"$v/Links.md"
  printf '{"events":[{"t":0.6,"type":"key","name":"k","cmd":true},{"t":1.0,"type":"type","text":"links"},{"t":1.6,"type":"key","name":"Enter"},{"t":2.2,"type":"mark","name":"links"}]}' \
    >"$m/links.json"
  export XDG_CONFIG_HOME="$work/config-kanban-links"; rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
  "$bin" --vault "$v" --script "$m/links.json" --at "mark:links+0.6" --scale 1 \
    --snap "$out/screens/kanban-links.png" --tree "$t/kanban-links.json" || return 1
  python3 "$harness" tree-mode "$t/kanban-links.json" kanban || return 1
  local found missing
  found="$(python3 "$harness" tree-rect "$t/kanban-links.json" kanban-card-0-0)" || return 1
  missing="$(python3 "$harness" tree-rect "$t/kanban-links.json" kanban-card-0-1)" || return 1
  # shellcheck disable=SC2086
  python3 - "$out/screens/kanban-links.png" $found $missing <<'PY'
import sys
from collections import Counter
from PIL import Image
img = Image.open(sys.argv[1]).convert("RGB")
r = [int(v) for v in sys.argv[2:]]

def ink(x, y, w, h):
    """The mean colour of the card's text pixels, right of its checkbox."""
    box = [img.getpixel((i, j)) for i in range(x + 30, x + w - 4) for j in range(y + 4, y + h - 4)]
    bg = Counter(box).most_common(1)[0][0]
    far = [p for p in box if sum(abs(a - b) for a, b in zip(p, bg)) > 90]
    if not far:
        raise SystemExit("no text in the card")
    return tuple(sum(c[i] for c in far) / len(far) for i in range(3))

found, missing = ink(*r[:4]), ink(*r[4:])
tint = lambda c: max(c) - min(c)
print(f"[[Agenda]] ink {tuple(round(c) for c in found)}, [[Nowhere]] ink {tuple(round(c) for c in missing)}")
if tint(found) < 40:
    raise SystemExit("the link to a note is not drawn in the accent")
if tint(missing) > 20 or sum(missing) >= sum(found) + 60:
    raise SystemExit("the link to no note is not dimmed")
print("ok a card's link to no note is dimmed, one to a note is the accent")
PY
}

step_headless_kanban_restart() {
  isolate
  kanban_gpu
  [ -f "$work/kanban-view.sha256" ] || seed_kanban_view_vault
  local release
  if [ -f "$out/trees/kanban-list.json" ]; then
    release="$(kanban_row_ids "$out/trees/kanban-list.json" Release)"
  else
    echo "run headless_kanban first (needs its sidebar tree)"; return 1
  fi
  export XDG_CONFIG_HOME="$work/config-kanban-restart"
  rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
  # Back (Ctrl+[) from Release to Agenda on the Kanban page: the page keeps
  # Release, so the relaunch must restore it from settings, not the selection.
  printf '{"events":[{"t":0.6,"type":"tap","key":"%s"},{"t":1.2,"type":"key","name":"5","cmd":true},{"t":1.8,"type":"key","name":"[","cmd":true},{"t":2.4,"type":"mark","name":"left"}]}' \
    "$release" >"$work/kanban-restart.json"
  "$bin" --vault "$kanban_view_vault" --script "$work/kanban-restart.json" --at "mark:left+0.4" --scale 1 \
    --snap "$out/screens/kanban-restart-1-left.png" --tree "$out/trees/kanban-restart-left.json" || return 1
  python3 "$harness" tree-mode "$out/trees/kanban-restart-left.json" kanban || return 1
  python3 "$harness" tree-text "$out/trees/kanban-restart-left.json" kanban-title Release || return 1
  python3 - "$XDG_CONFIG_HOME/muninn/settings.json" <<'PY' || return 1
import json, sys
v = json.load(open(sys.argv[1]))
got = (v.get("mode"), v.get("kanban"), v.get("selected_rel"))
print("saved", got)
assert got == ("kanban", "Release.md", "Agenda.md"), got
PY
  "$bin" --vault "$kanban_view_vault" --at 1.0 --scale 1 \
    --snap "$out/screens/kanban-restart.png" --tree "$out/trees/kanban-restart.json" || return 1
  python3 "$harness" tree-mode "$out/trees/kanban-restart.json" kanban || return 1
  python3 "$harness" tree-text "$out/trees/kanban-restart.json" kanban-title Release || return 1
  python3 "$harness" tree-text "$out/trees/kanban-restart.json" kanban-lane-1-count 2/3 || return 1
  kanban_view_unchanged "the restart"
}

step_x11_kanban() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local tree="$out/trees/kanban-list.json"
  [ -f "$tree" ] && [ -f "$out/screens/kanban-readonly.png" ] || { echo "run headless_kanban first (needs $tree)"; return 1; }
  local icd release xy
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  echo "vulkan icd: $icd"
  command -v vulkaninfo >/dev/null && vulkaninfo --summary 2>/dev/null | grep -E 'deviceName|driverName|driverInfo'
  [ -f "$work/kanban-view.sha256" ] || seed_kanban_view_vault
  release="$(kanban_row_ids "$tree" Release)"
  xy="$(python3 "$harness" tree-center "$tree" "$release")" || return 1
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n i
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-kanban-view.log" 2>&1 &
  xvfb_pid=$!
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY, vulkan icd $icd"
  export XDG_CONFIG_HOME="$work/config-x11-kanban-view" XDG_STATE_HOME="$work/state-x11-kanban-view"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  "$bin" --new-instance --vault "$kanban_view_vault" >>"$out/logs/x11-kanban-view-app.log" 2>&1 &
  app_pid=$!
  local wid
  wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
  sleep 2
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $xy || return 1
  sleep 1
  python3 "$harness" x11-chord "$wid" ctrl+5 || return 1
  sleep "$(python3 -c "print(1.5 * max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"
  python3 "$harness" settings-mode "$XDG_CONFIG_HOME/muninn/settings.json" kanban || return 1
  python3 -c 'import json,sys; k=json.load(open(sys.argv[1])).get("kanban"); print("saved kanban", k); assert k=="Release.md"' \
    "$XDG_CONFIG_HOME/muninn/settings.json" || return 1
  import -window "$wid" "$out/screens/kanban-x11.png" || return 1
  python3 - "$out/screens/kanban-readonly.png" "$out/screens/kanban-x11.png" <<'PY' || return 1
import sys
from PIL import Image, ImageChops
headless, windowed = (Image.open(path).convert("RGB") for path in sys.argv[1:])
if headless.size != windowed.size:
    raise SystemExit(f"size {headless.size} vs {windowed.size}")
diff = ImageChops.difference(headless, windowed)
mean = sum(diff.resize((1, 1), Image.Resampling.BOX).getpixel((0, 0))) / 3
print(f"x11 vs headless kanban mean abs diff {mean:.2f}")
if mean > 12:
    raise SystemExit(f"the windowed Kanban page does not match the headless shot ({mean:.2f})")
print("ok x11 screenshot matches the headless Kanban shot")
PY
  python3 "$harness" x11-close "$wid" || return 1
  local status=124
  for i in $(seq 100); do
    if ! kill -0 "$app_pid" 2>/dev/null; then wait "$app_pid"; status=$?; break; fi
    sleep 0.1
  done
  app_pid=
  echo "app exit status: $status"
  [ "$status" = 0 ] || return 1
  kanban_view_unchanged "the windowed app"
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

# Kanban slice 3: New Kanban, Convert, the Kanbans section and the palette's kanbans,
# on their own vault (slice 1 uses $work/kanban-vault, slice 2 $work/kanban-view-vault).
kanban_new_vault="$work/kanban-new-vault"

seed_kanban_new_vault() {
  rm -rf "$kanban_new_vault"
  mkdir -p "$kanban_new_vault/Plans"
  printf '# Agenda\n\nA plain note.\n' >"$kanban_new_vault/Agenda.md"
  : >"$kanban_new_vault/Blank.md"
  printf '# Notes\n\nPlanning notes.\n' >"$kanban_new_vault/Plans/Notes.md"
  printf '# Mine\n\nSomeone else'"'"'s note. It stays as it is.\n' >"$kanban_new_vault/Plans/Untitled Kanban.md"
  cp "$kanban_fixtures/lanes-and-limits.md" "$kanban_new_vault/Release.md"
  (cd "$kanban_new_vault" && sha256sum Agenda.md Release.md Plans/Notes.md "Plans/Untitled Kanban.md") >"$work/kanban-new.sha256"
}

kanban_new_run() { # kanban_new_run NAME SCRIPT MARK: a fresh launch on the slice 3 vault, to MARK
  export XDG_CONFIG_HOME="$work/config-kanban-new-$1" XDG_STATE_HOME="$work/state-kanban-new-$1"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  "$bin" --vault "$kanban_new_vault" --script "$2" --at "mark:$3+0.25" --scale 1 \
    --snap "$out/screens/kanban-new-$1.png" --tree "$out/trees/kanban-new-$1.json"
}

palette_script() { # palette_script FILE QUERY [TAP]: tap TAP (a note row), Ctrl+Shift+P, type QUERY, mark "typed", Enter, mark "done"
  local tap=""
  [ -n "${3:-}" ] && tap="{\"t\":0.6,\"type\":\"tap\",\"key\":\"$3\"},"
  printf '{"events":[%s{"t":1.2,"type":"key","name":"p","cmd":true,"shift":true},{"t":1.6,"type":"type","text":"%s"},{"t":2.2,"type":"mark","name":"typed"},{"t":2.6,"type":"key","name":"Enter"},{"t":3.4,"type":"mark","name":"done"}]}' \
    "$tap" "$2" >"$1"
}

same_as_layout() { # same_as_layout FILE: byte-identical to the spec's New-file layout
  cmp "$1" "$kanban_fixtures/new-kanban.md" || { echo "$1 is not the New Kanban layout"; return 1; }
  echo "ok $1 is the New Kanban layout byte for byte"
}

step_headless_new_kanban() {
  isolate
  kanban_gpu
  seed_kanban_new_vault
  local t="$out/trees" m="$work/kanban-new"
  mkdir -p "$m"
  printf '{"events":[{"t":0.8,"type":"mark","name":"list"}]}' >"$m/list.json"
  kanban_new_run list "$m/list.json" list || return 1
  local agenda blank notes
  read -r agenda blank notes <<<"$(kanban_row_ids "$t/kanban-new-list.json" Agenda Blank Notes)"
  [ -n "$notes" ] || { echo "missing note rows"; return 1; }
  echo "rows: Agenda $agenda, Blank $blank, Notes $notes"
  # 1. New Kanban twice from the palette (Agenda, at the root, is selected at launch).
  palette_script "$m/new.json" "new kanban"
  kanban_new_run first "$m/new.json" done || return 1
  same_as_layout "$kanban_new_vault/Untitled Kanban.md" || return 1
  python3 "$harness" tree-mode "$t/kanban-new-first.json" kanban || return 1
  python3 "$harness" tree-text "$t/kanban-new-first.json" kanban-title "Untitled Kanban" || return 1
  python3 "$harness" tree-text "$t/kanban-new-first.json" kanban-lane-2-title "Done" || return 1
  cp "$out/screens/kanban-new-first.png" "$out/screens/kanban-new.png"
  kanban_new_run second "$m/new.json" done || return 1
  same_as_layout "$kanban_new_vault/Untitled Kanban 2.md" || return 1
  python3 "$harness" tree-text "$t/kanban-new-second.json" kanban-title "Untitled Kanban 2" || return 1
  [ ! -e "$kanban_new_vault/Untitled Kanban 3.md" ] || { echo "a third file appeared"; return 1; }
  # 2. In Plans, where "Untitled Kanban.md" is someone's note: it is kept, the next name is used.
  palette_script "$m/plans.json" "new kanban" "$notes"
  kanban_new_run plans "$m/plans.json" done || return 1
  same_as_layout "$kanban_new_vault/Plans/Untitled Kanban 2.md" || return 1
  python3 "$harness" tree-text "$t/kanban-new-plans.json" kanban-detail "Plans/Untitled Kanban 2.md" || return 1
  # 3. Convert empty note to Kanban on Blank (empty): offered, writes the layout, opens it.
  palette_script "$m/convert.json" "convert" "$blank"
  # A run keeps replaying its script after the shot, so the look at the list runs
  # the same script without its Enter.
  sed 's/,{"t":2.6,"type":"key","name":"Enter"}//' "$m/convert.json" >"$m/convert-look.json"
  kanban_new_run convert-offered "$m/convert-look.json" typed || return 1
  python3 "$harness" tree-text "$t/kanban-new-convert-offered.json" palette-item-0 "Convert empty note to Kanban" || return 1
  [ ! -s "$kanban_new_vault/Blank.md" ] || { echo "Blank changed without Enter"; return 1; }
  kanban_new_run convert "$m/convert.json" done || return 1
  same_as_layout "$kanban_new_vault/Blank.md" || return 1
  python3 "$harness" tree-mode "$t/kanban-new-convert.json" kanban || return 1
  python3 "$harness" tree-text "$t/kanban-new-convert.json" kanban-title "Blank" || return 1
  # 4. On Agenda (has text) Convert is not offered, and Enter changes nothing.
  palette_script "$m/refuse.json" "convert" "$agenda"
  kanban_new_run refuse-typed "$m/refuse.json" typed || return 1
  kanban_new_run refuse "$m/refuse.json" done || return 1
  no_key "$t/kanban-new-refuse-typed.json" palette-item-0 || return 1
  python3 -c 'import json,sys; t=json.dumps(json.load(open(sys.argv[1]))); assert "\"0 results\"" in t, "palette is not empty"; print("ok the palette shows 0 results for convert on Agenda")' \
    "$t/kanban-new-refuse-typed.json" || return 1
  python3 "$harness" tree-mode "$t/kanban-new-refuse.json" editor || return 1
  # 5. The Kanbans section and the palette list every kanban by name.
  printf '{"events":[{"t":0.8,"type":"mark","name":"sidebar"},{"t":1.2,"type":"key","name":"p","cmd":true,"shift":true},{"t":1.8,"type":"mark","name":"palette"}]}' >"$m/lists.json"
  kanban_new_run sidebar "$m/lists.json" sidebar || return 1
  kanban_new_run palette "$m/lists.json" palette || return 1
  cp "$out/screens/kanban-new-sidebar.png" "$out/screens/kanban-sidebar.png"
  python3 - "$t/kanban-new-sidebar.json" "$t/kanban-new-palette.json" <<'PY' || return 1
import json, sys

def walk(node):
    if isinstance(node, dict):
        yield node
        for value in node.values():
            yield from walk(value)
    elif isinstance(node, list):
        for value in node:
            yield from walk(value)

# The index's path order, case-folded: "untitled kanban 2.md" sorts before
# "untitled kanban.md" (a space before a dot), Plans/ before Release.
want = ["Blank", "Untitled Kanban 2", "Release", "Untitled Kanban 2", "Untitled Kanban"]
nodes = list(walk(json.load(open(sys.argv[1])).get("elements", [])))
got = [n.get("text") for n in nodes if str(n.get("key") or "").startswith("sidebar-kanban-title-")]
print("Kanbans section:", got)
if got != want:
    raise SystemExit(f"FAIL Kanbans section {got}, expected {want} (index path order)")
if not any(n.get("key") == "sidebar-kanbans" for n in nodes):
    raise SystemExit("FAIL no sidebar-kanbans")
glyphs = sorted(n["key"] for n in nodes if str(n.get("key") or "").endswith("-kanban") and str(n["key"]).startswith("note-row-"))
if len(glyphs) != 5:
    raise SystemExit(f"FAIL {len(glyphs)} notes-list kanban glyphs, expected 5: {glyphs}")
print("ok five notes-list rows carry the kanban glyph")
# The tree is flat, in draw order: a palette row belongs to the section header
# drawn before it, and its label is the row's title.
section, rows = None, []
for n in json.load(open(sys.argv[2])).get("elements", []):
    key = str(n.get("key") or "")
    if key.startswith("palette-section-"):
        section = key.removeprefix("palette-section-")
    elif key.startswith("palette-item-"):
        rows.append((section, n.get("label")))
kanban_rows = sorted(label for section, label in rows if section == "kanbans")
print("palette kanbans:", kanban_rows)
if kanban_rows != sorted(want):
    raise SystemExit(f"FAIL palette kanbans {kanban_rows}, expected {sorted(want)}")
actions = [label for section, label in rows if section == "actions"]
if "New Kanban" not in actions:
    raise SystemExit(f"FAIL no New Kanban in the palette actions {actions}")
if "Convert empty note to Kanban" in actions:
    raise SystemExit("FAIL Convert offered on Agenda, which has text")
print("ok the palette has New Kanban and every kanban by name")
PY
  # 6. A folder that resolves outside the vault is refused. The scan never follows a
  #    symlink, so no indexed note can sit in such a folder and the UI can't pick one;
  #    the guard is the core one, exercised here by its tests.
  cargo test --locked -q -p muninn-core --lib --manifest-path "$root/Cargo.toml" -- \
    kanban::tests::new_kanban 2>&1 | tail -n 4
  [ "${PIPESTATUS[0]}" = 0 ] || return 1
  # Pre-existing files are untouched; Blank changed only by Convert (checked above).
  (cd "$kanban_new_vault" && sha256sum -c "$work/kanban-new.sha256") || { echo "a pre-existing file changed"; return 1; }
  find "$kanban_new_vault" -name '.*' | grep . && { echo "a temp file was left"; return 1; }
  (cd "$kanban_new_vault" && find . -type f | sort)
}

step_x11_new_kanban() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd"
  echo "vulkan icd: $icd"
  command -v vulkaninfo >/dev/null && vulkaninfo --summary 2>/dev/null | grep -E 'deviceName|driverName|driverInfo'
  local v="$work/kanban-x11-new-vault"
  rm -rf "$v"; mkdir -p "$v"
  printf '# Agenda\n\nA plain note.\n' >"$v/Agenda.md"
  cp "$kanban_fixtures/lanes-and-limits.md" "$v/Release.md"
  (cd "$v" && sha256sum Agenda.md Release.md) >"$work/kanban-x11-new.sha256"
  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n i
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-kanban-new.log" 2>&1 &
  xvfb_pid=$!
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY"
  export XDG_CONFIG_HOME="$work/config-x11-kanban-new" XDG_STATE_HOME="$work/state-x11-kanban-new"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  "$bin" --new-instance --vault "$v" >>"$out/logs/x11-kanban-new-app.log" 2>&1 &
  app_pid=$!
  local wid pause
  wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
  pause="$(python3 -c "print(1.5 * max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"
  sleep 2
  [ ! -e "$v/Untitled Kanban.md" ] || { echo "a kanban appeared before New Kanban"; return 1; }
  python3 "$harness" x11-chord "$wid" ctrl+shift+p || return 1
  python3 "$harness" x11-type "$wid" "new kanban" && sleep 0.5 || return 1
  python3 "$harness" x11-chord "$wid" enter || return 1
  sleep "$pause"
  python3 "$harness" settings-mode "$XDG_CONFIG_HOME/muninn/settings.json" kanban || return 1
  python3 -c 'import json,sys; k=json.load(open(sys.argv[1])).get("kanban"); print("saved kanban", k); assert k=="Untitled Kanban.md", k' \
    "$XDG_CONFIG_HOME/muninn/settings.json" || return 1
  same_as_layout "$v/Untitled Kanban.md" || return 1
  grep -q 'created Untitled Kanban.md' "$XDG_STATE_HOME/muninn/session.log" || { echo "no 'created' line in the session log"; return 1; }
  import -window "$wid" "$out/screens/kanban-new-x11.png" || return 1
  python3 "$harness" x11-close "$wid" || return 1
  local status=124
  for i in $(seq 100); do
    if ! kill -0 "$app_pid" 2>/dev/null; then wait "$app_pid"; status=$?; break; fi
    sleep 0.1
  done
  app_pid=
  echo "app exit status: $status"
  [ "$status" = 0 ] || return 1
  (cd "$v" && sha256sum -c "$work/kanban-x11-new.sha256") || { echo "a pre-existing file changed"; return 1; }
  ls "$v"
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

step_x11_kanban_open() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd" WGPU_BACKEND=vulkan
  echo "vulkan icd: $icd"
  command -v vulkaninfo >/dev/null && vulkaninfo --summary 2>/dev/null | grep -E 'deviceName|driverName|driverInfo'
  local v="$work/kanban-open-vault" t="$out/trees" m="$work/kanban-open"
  rm -rf "$v" "$m"; mkdir -p "$v" "$m"
  printf '# Agenda\n\nA plain note.\n' >"$v/Agenda.md"
  cp "$kanban_fixtures/lanes-and-limits.md" "$v/Release.md"
  cp "$kanban_fixtures/over-limit.md" "$v/Overloaded.md"
  (cd "$v" && sha256sum Agenda.md Release.md Overloaded.md) >"$work/kanban-open.sha256"
  # Where to click, from headless runs of the same vault in the same 1180x780 window.
  printf '{"events":[{"t":0.8,"type":"mark","name":"list"}]}' >"$m/list.json"
  export XDG_CONFIG_HOME="$work/config-kanban-open-list"; rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
  "$bin" --vault "$v" --script "$m/list.json" --at "mark:list+0.3" --scale 1 --tree "$t/kanban-open-list.json" || return 1
  local release
  release="$(kanban_row_ids "$t/kanban-open-list.json" Release)"
  [ -n "$release" ] || { echo "no Release row"; return 1; }
  printf '{"events":[{"t":0.6,"type":"tap","key":"%s"},{"t":1.2,"type":"mark","name":"note"},{"t":1.6,"type":"tap","key":"open-as-kanban"},{"t":2.4,"type":"mark","name":"kanban"},{"t":2.8,"type":"tap","key":"kanban-open-markdown"},{"t":3.6,"type":"mark","name":"markdown"}]}' \
    "$release" >"$m/open.json"
  local mark mode
  for pair in note:editor kanban:kanban markdown:editor; do
    mark="${pair%%:*}" mode="${pair#*:}"
    export XDG_CONFIG_HOME="$work/config-kanban-open-$mark"; rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
    "$bin" --vault "$v" --script "$m/open.json" --at "mark:$mark+0.4" --scale 1 \
      --snap "$out/screens/kanban-open-headless-$mark.png" --tree "$t/kanban-open-$mark.json" || return 1
    python3 "$harness" tree-mode "$t/kanban-open-$mark.json" "$mode" || return 1
  done
  python3 "$harness" tree-text "$t/kanban-open-note.json" open-as-kanban "Open as kanban" || return 1
  python3 "$harness" tree-text "$t/kanban-open-kanban.json" kanban-title "Release" || return 1
  no_key "$t/kanban-open-kanban.json" open-as-kanban || return 1
  no_key "$t/kanban-open-markdown.json" kanban-page || return 1
  python3 "$harness" tree-text "$t/kanban-open-markdown.json" open-as-kanban "Open as kanban" || return 1
  no_key "$t/kanban-open-list.json" open-as-kanban || { echo "Agenda (plain) shows Open as kanban"; return 1; }
  local row_xy button_xy markdown_xy
  row_xy="$(python3 "$harness" tree-center "$t/kanban-open-list.json" "$release")" || return 1
  button_xy="$(python3 "$harness" tree-center "$t/kanban-open-note.json" open-as-kanban)" || return 1
  markdown_xy="$(python3 "$harness" tree-center "$t/kanban-open-kanban.json" kanban-open-markdown)" || return 1

  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n i
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-kanban-open.log" 2>&1 &
  xvfb_pid=$!
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY"
  export XDG_CONFIG_HOME="$work/config-x11-kanban-open" XDG_STATE_HOME="$work/state-x11-kanban-open"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  local settings="$XDG_CONFIG_HOME/muninn/settings.json"
  "$bin" --new-instance --vault "$v" >>"$out/logs/x11-kanban-open-app.log" 2>&1 &
  app_pid=$!
  local wid pause
  wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
  pause="$(python3 -c "print(1.5 * max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"
  sleep 2
  saved() { # saved MODE KANBAN
    python3 "$harness" settings-mode "$settings" "$1" || return 1
    python3 -c 'import json,sys; v=json.load(open(sys.argv[1])); got=(v.get("selected_rel"), v.get("kanban")); print("saved", got); assert (got[1] or "")==sys.argv[2] and (sys.argv[3]=="" or got[0]==sys.argv[3]), got' \
      "$settings" "$2" "${3:-}"
  }
  like() { # like HEADLESS X11: the windowed shot matches the headless one
    python3 - "$1" "$2" <<'PY'
import sys
from PIL import Image, ImageChops
a, b = (Image.open(p).convert("RGB") for p in sys.argv[1:])
if a.size != b.size:
    raise SystemExit(f"size {a.size} vs {b.size}")
mean = sum(ImageChops.difference(a, b).resize((1, 1), Image.Resampling.BOX).getpixel((0, 0))) / 3
print(f"{sys.argv[2]} vs headless mean abs diff {mean:.2f}")
if mean > 12:
    raise SystemExit(f"the windowed page does not match the headless shot ({mean:.2f})")
PY
  }
  # 1. The notes list opens a kanban note as a note, with Open as kanban.
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $row_xy && sleep "$pause" || return 1
  import -window "$wid" "$out/screens/kanban-open-button.png" || return 1
  saved editor "" Release.md || return 1
  like "$out/screens/kanban-open-headless-note.png" "$out/screens/kanban-open-button.png" || return 1
  # 2. Open as kanban: Kanban mode on that note.
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $button_xy && sleep "$pause" || return 1
  import -window "$wid" "$out/screens/kanban-open-kanban.png" || return 1
  saved kanban Release.md Release.md || return 1
  like "$out/screens/kanban-open-headless-kanban.png" "$out/screens/kanban-open-kanban.png" || return 1
  # 3. Open as markdown: back to Write on the same note.
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $markdown_xy && sleep "$pause" || return 1
  import -window "$wid" "$out/screens/kanban-as-markdown.png" || return 1
  saved editor Release.md Release.md || return 1
  like "$out/screens/kanban-open-headless-markdown.png" "$out/screens/kanban-as-markdown.png" || return 1
  # 4. B1: Ctrl+K, "release", Enter picks the Release kanban from the note search:
  #    Kanban mode on it, from Write, with the keyboard on the page (Esc leaves it;
  #    had the search field kept the keyboard, Esc would only have let go of it).
  python3 "$harness" x11-chord "$wid" ctrl+k && sleep 0.5 || return 1
  python3 "$harness" x11-type "$wid" release && sleep 0.5 || return 1
  python3 "$harness" x11-chord "$wid" enter && sleep "$pause" || return 1
  import -window "$wid" "$out/screens/kanban-open-search.png" || return 1
  saved kanban Release.md Release.md || return 1
  python3 "$harness" x11-chord "$wid" esc && sleep "$pause" || return 1
  saved editor Release.md Release.md || return 1
  echo "ok a kanban picked from Ctrl+K search opened in Kanban mode with the keyboard"
  # 5. Cmd+K first (the search field takes the keyboard), then the palette, the
  #    Overloaded kanban (Down past its note row), Enter: Kanban mode on it.
  python3 "$harness" x11-chord "$wid" ctrl+k && sleep 0.5 || return 1
  python3 "$harness" x11-chord "$wid" ctrl+shift+p || return 1
  python3 "$harness" x11-type "$wid" overloaded && sleep 0.5 || return 1
  import -window "$wid" "$out/screens/kanban-open-palette.png" || return 1
  python3 "$harness" x11-chord "$wid" Down && python3 "$harness" x11-chord "$wid" enter && sleep "$pause" || return 1
  import -window "$wid" "$out/screens/kanban-open-from-palette.png" || return 1
  saved kanban Overloaded.md Overloaded.md || return 1
  # 6. A key the Kanban page handles: Esc leaves it. Had the search field kept the
  #    keyboard, Esc would only have let go of it and the mode would stay kanban.
  python3 "$harness" x11-chord "$wid" esc && sleep "$pause" || return 1
  import -window "$wid" "$out/screens/kanban-open-esc.png" || return 1
  saved editor Overloaded.md Overloaded.md || return 1
  echo "ok focus landed on the Kanban page after the palette pick"
  python3 "$harness" x11-close "$wid" || return 1
  local status=124
  for i in $(seq 100); do
    if ! kill -0 "$app_pid" 2>/dev/null; then wait "$app_pid"; status=$?; break; fi
    sleep 0.1
  done
  app_pid=
  echo "app exit status: $status"
  [ "$status" = 0 ] || return 1
  (cd "$v" && sha256sum -c "$work/kanban-open.sha256") || { echo "a kanban file changed"; return 1; }
  echo "ok every kanban file unchanged"
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
}

# Slice 4: card edits on edit-board.md. Each run starts from the fixture's bytes,
# with its own settings, and stops at its mark (before the script's later keys).
kanban_edit_run() { # kanban_edit_run VAULT SCRIPT MARK [SHOT]
  cp "$kanban_fixtures/edit-board.md" "$1/Board.md"
  export XDG_CONFIG_HOME="$work/config-kanban-edit-$3" XDG_STATE_HOME="$work/state-kanban-edit-$3"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  "$bin" --vault "$1" --script "$2" --at "mark:$3+0.25" --scale 1 \
    ${4:+--snap "$out/screens/$4.png"} --tree "$out/trees/kanban-edit-$3.json"
}

kanban_cards() { # kanban_cards TREE LANE "Open: A|Done: B|...": the lane's cards, in order
  python3 - "$@" <<'PY'
import json, re, sys, unicodedata
path, lane, want = sys.argv[1], sys.argv[2], sys.argv[3].split("|") if sys.argv[3] else []
def walk(node):
    if isinstance(node, dict):
        yield node
        for v in node.values():
            yield from walk(v)
    elif isinstance(node, list):
        for v in node:
            yield from walk(v)
tree = json.load(open(path))
card = re.compile(rf"kanban-card-{lane}-(\d+)$")
got = {}
for n in walk(tree.get("accessibility")):
    m = card.match(str(n.get("key") or ""))
    if m:
        got[int(m.group(1))] = unicodedata.normalize("NFC", n.get("name") or "")
got = [got[i] for i in sorted(got)]
want = [unicodedata.normalize("NFC", w) for w in want]
if got != want:
    raise SystemExit(f"{path}: lane {lane} is {got}, not {want}")
print(f"ok {path}: lane {lane} {got}")
PY
}

step_headless_kanban_edit() {
  isolate
  kanban_gpu
  local v="$work/kanban-edit-vault" t="$out/trees" m="$work/kanban-edit" original="$kanban_fixtures/edit-board.md"
  rm -rf "$v" "$m"; mkdir -p "$v" "$m"
  printf '# Roadmap\n\nThe plan the cards link to.\n' >"$v/Roadmap.md"
  printf '{"events":[{"t":0.8,"type":"mark","name":"list"}]}' >"$m/list.json"
  kanban_edit_run "$v" "$m/list.json" list || return 1
  local board
  board="$(kanban_row_ids "$t/kanban-edit-list.json" Board)"
  [ -n "$board" ] || { echo "no Board row"; return 1; }
  sed "s/@BOARD@/$board/" "$root/scripts/ui-smoke/kanban-edit.json" >"$m/edit.json"
  local pair mark shot
  for pair in open: card-new:kanban-card-new edited: card-editing:kanban-card-editing duplicated: \
    inserted-before: inserted-after: card-split:kanban-card-split deleted: toggled: born-done: \
    all-done: undo:kanban-undo redo: start:; do
    mark="${pair%%:*}" shot="${pair#*:}"
    kanban_edit_run "$v" "$m/edit.json" "$mark" "$shot" || return 1
    python3 "$harness" tree-mode "$t/kanban-edit-$mark.json" kanban || return 1
  done
  local todo="Open: Write the release notes|Open: Review [[Roadmap]]"
  local split="Open: First|Open: Second|Open: Write the release notes|Open: Write the release notes|Open: Review [[Roadmap]]|Open: with the team|Open: Plan the demo 漢字 🎉 é"
  local after="Open: First|Open: Write the release notes|Open: Write the release notes|Open: Review [[Roadmap]]|Open: with the team|Open: Plan the demo 漢字 🎉 é"
  local c="kanban_cards $t/kanban-edit"
  $c-open.json 0 "$todo" && $c-open.json 2 "Done: Ship 0.7.1|Open: Tag the release" &&
    $c-card-new.json 0 "$todo|Open: Plan the demo" &&
    $c-edited.json 0 "$todo|Open: Plan the demo 漢字 🎉 é" &&
    $c-duplicated.json 0 "Open: Write the release notes|$todo|Open: Plan the demo 漢字 🎉 é" &&
    $c-inserted-before.json 0 "Open: First|Open: Write the release notes|$todo|Open: Plan the demo 漢字 🎉 é" &&
    $c-inserted-after.json 0 "Open: First|Open: Second|Open: Write the release notes|$todo|Open: Plan the demo 漢字 🎉 é" &&
    $c-card-split.json 0 "$split" && $c-deleted.json 0 "$after" &&
    $c-deleted.json 1 "Open: Fix the sync bug" && $c-toggled.json 1 "Done: Fix the sync bug" &&
    $c-born-done.json 2 "Done: Ship 0.7.1|Open: Tag the release|Done: Write the notes" &&
    $c-all-done.json 2 "Done: Ship 0.7.1|Done: Tag the release|Done: Write the notes" &&
    $c-undo.json 2 "Done: Ship 0.7.1|Open: Tag the release|Done: Write the notes" &&
    $c-redo.json 2 "Done: Ship 0.7.1|Done: Tag the release|Done: Write the notes" &&
    $c-redo.json 0 "$after" && $c-start.json 0 "$todo" &&
    $c-start.json 2 "Done: Ship 0.7.1|Open: Tag the release" || return 1
  python3 - "$t/kanban-edit-card-editing.json" <<'PY' || return 1
import json, sys
def walk(node):
    if isinstance(node, dict):
        yield node
        for v in node.values():
            yield from walk(v)
    elif isinstance(node, list):
        for v in node:
            yield from walk(v)
values = [n.get("value") for n in walk(json.load(open(sys.argv[1])).get("accessibility"))
          if n.get("role") == "multiline-text-input" and n.get("name") == "Card text"]
want = "Plan the demo \u6f22\u5b57 \U0001f389 e\u0301\nsecond line"
if values != [want]:
    raise SystemExit(f"the card editor holds {values!r}, not {want!r}")
print(f"ok the card editor holds {want!r} (Shift+Enter made the line break)")
PY
  python3 "$harness" tree-text "$t/kanban-edit-redo.json" kanban-status Unsaved || return 1
  python3 -c 'import json,sys; s=[e.get("text") for e in json.load(open(sys.argv[1]))["elements"] if e.get("key")=="kanban-status"]; assert s==[""], s; print("ok no unsaved indicator on open")' \
    "$t/kanban-edit-open.json" || return 1
  # After the save, undoing back to the start differs from the file on disk again.
  python3 "$harness" tree-text "$t/kanban-edit-start.json" kanban-status Unsaved || return 1
  # Nothing is written before Cmd+S: the run to "presave" leaves the fixture's bytes.
  kanban_edit_run "$v" "$m/edit.json" presave || return 1
  cmp "$v/Board.md" "$original" || { echo "Board.md changed before Cmd+S"; return 1; }
  echo "ok sha256 before Cmd+S: $(sha256sum <"$v/Board.md" | cut -d' ' -f1) (the fixture's)"
  # Cmd+S writes every edit, byte for byte.
  kanban_edit_run "$v" "$m/edit.json" saved kanban-saved || return 1
  python3 "$harness" tree-text "$t/kanban-edit-saved.json" kanban-status Saved || return 1
  cp "$v/Board.md" "$out/logs/kanban-edit-saved.md"
  cmp "$v/Board.md" "$root/scripts/ui-smoke/kanban-edit-saved.md" || { diff "$root/scripts/ui-smoke/kanban-edit-saved.md" "$v/Board.md"; return 1; }
  echo "ok Board.md after Cmd+S is kanban-edit-saved.md byte for byte"
  # Undo back to the start and Cmd+S: the original bytes again.
  kanban_edit_run "$v" "$m/edit.json" restored || return 1
  cmp "$v/Board.md" "$original" || { diff "$original" "$v/Board.md"; return 1; }
  echo "ok undo to the start + Cmd+S restores the fixture's bytes"
}

step_x11_kanban_edit() {
  isolate
  need Xvfb xorg-server-xvfb || return 1
  need import imagemagick || return 1
  python3 -c 'import Xlib, PIL' 2>/dev/null || { echo "missing python-xlib or python-pillow"; return 1; }
  local icd
  icd="$(find_lavapipe)" || { echo "missing lavapipe (see x11_modes)"; return 1; }
  export VK_DRIVER_FILES="$icd" VK_ICD_FILENAMES="$icd" WGPU_BACKEND=vulkan
  echo "vulkan icd: $icd"
  command -v vulkaninfo >/dev/null && vulkaninfo --summary 2>/dev/null | grep -E 'deviceName|driverName|driverInfo'
  local v="$work/kanban-edit-x11-vault" t="$out/trees" m="$work/kanban-edit-x11"
  rm -rf "$v" "$m"; mkdir -p "$v" "$m"
  printf '# Agenda\n\nA plain note.\n' >"$v/Agenda.md"
  printf '# Roadmap\n\nThe plan the cards link to.\n' >"$v/Roadmap.md"
  cp "$kanban_fixtures/edit-board.md" "$v/Board.md"
  # Where to click, from a headless run of the same steps in the same 1180x780 window:
  # Write, Cmd+K, "board", Enter (a kanban picked from the note search opens in
  # Kanban mode), + Add card, the new card's menu, Edit.
  cat >"$m/steps.json" <<'JSON'
{"events":[
  {"t":0.6,"type":"key","name":"1","cmd":true},
  {"t":1.0,"type":"key","name":"k","cmd":true},
  {"t":1.4,"type":"type","text":"board"},
  {"t":2.0,"type":"key","name":"Enter"},
  {"t":2.6,"type":"mark","name":"cmdk"},
  {"t":3.0,"type":"tap","key":"kanban-lane-0-add"},
  {"t":3.4,"type":"type","text":"Plan the demo"},
  {"t":4.2,"type":"key","name":"Enter"},
  {"t":4.6,"type":"mark","name":"added"},
  {"t":5.0,"type":"tap","key":"kanban-card-0-2-menu"},
  {"t":5.6,"type":"mark","name":"menu"}
]}
JSON
  local mark
  for mark in cmdk added menu; do
    export XDG_CONFIG_HOME="$work/config-kanban-edit-x11-$mark"; rm -rf "$XDG_CONFIG_HOME"; mkdir -p "$XDG_CONFIG_HOME"
    "$bin" --vault "$v" --script "$m/steps.json" --at "mark:$mark+0.4" --scale 1 \
      --snap "$out/screens/kanban-edit-headless-$mark.png" --tree "$t/kanban-edit-x11-$mark.json" || return 1
    python3 "$harness" tree-mode "$t/kanban-edit-x11-$mark.json" kanban || return 1
  done
  python3 "$harness" tree-text "$t/kanban-edit-x11-cmdk.json" kanban-title Board || return 1
  cmp "$v/Board.md" "$kanban_fixtures/edit-board.md" || { echo "a headless run wrote Board.md"; return 1; }
  local add_xy menu_xy edit_xy
  add_xy="$(python3 "$harness" tree-center "$t/kanban-edit-x11-cmdk.json" kanban-lane-0-add)" || return 1
  menu_xy="$(python3 "$harness" tree-center "$t/kanban-edit-x11-added.json" kanban-card-0-2-menu)" || return 1
  edit_xy="$(python3 "$harness" tree-center "$t/kanban-edit-x11-menu.json" menu-edit)" || return 1

  trap '[ -n "${app_pid:-}" ] && kill "$app_pid"; [ -n "${xvfb_pid:-}" ] && kill "$xvfb_pid"' EXIT
  local n i
  n="$(pick_display)" || return 1
  Xvfb ":$n" -screen 0 1280x860x24 -dpi 96 -nolisten tcp >"$out/logs/xvfb-kanban-edit.log" 2>&1 &
  xvfb_pid=$!
  for i in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  echo "Xvfb on $DISPLAY"
  export XDG_CONFIG_HOME="$work/config-x11-kanban-edit" XDG_STATE_HOME="$work/state-x11-kanban-edit"
  rm -rf "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"; mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME"
  local settings="$XDG_CONFIG_HOME/muninn/settings.json"
  "$bin" --new-instance --vault "$v" >>"$out/logs/x11-kanban-edit-app.log" 2>&1 &
  app_pid=$!
  local wid pause
  wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
  pause="$(python3 -c "print(1.5 * max(float('${UI_SMOKE_SLOW:-1}' or 1), 1))")"
  sleep 2
  # 1. Write, then Ctrl+K, "board", Enter: the kanban opens in Kanban mode, with the
  #    keyboard on the page (Esc would leave it; here Ctrl+S must reach it).
  python3 "$harness" x11-chord "$wid" ctrl+1 && sleep 0.5 || return 1
  python3 "$harness" settings-mode "$settings" editor || return 1
  python3 "$harness" x11-chord "$wid" ctrl+k && sleep 0.5 || return 1
  python3 "$harness" x11-type "$wid" board && sleep 0.5 || return 1
  python3 "$harness" x11-chord "$wid" enter && sleep "$pause" || return 1
  import -window "$wid" "$out/screens/kanban-cmdk.png" || return 1
  python3 "$harness" settings-mode "$settings" kanban || return 1
  python3 -c 'import json,sys; v=json.load(open(sys.argv[1])); assert v.get("kanban")=="Board.md", v.get("kanban"); print("saved kanban", v.get("kanban"))' "$settings" || return 1
  #    Esc leaves the page (had the search field kept the keyboard, Esc would only
  #    have let go of it), and Ctrl+5 comes back to the same kanban.
  python3 "$harness" x11-chord "$wid" esc && sleep "$pause" || return 1
  python3 "$harness" settings-mode "$settings" editor || return 1
  python3 "$harness" x11-chord "$wid" ctrl+5 && sleep "$pause" || return 1
  python3 "$harness" settings-mode "$settings" kanban || return 1
  # 2. + Add card, type, Enter; its menu, Edit, type, Enter; Ctrl+S.
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $add_xy && sleep 0.5 || return 1
  python3 "$harness" x11-type "$wid" "Plan the demo" && python3 "$harness" x11-chord "$wid" enter && sleep 0.5 || return 1
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $menu_xy && sleep "$pause" || return 1
  # shellcheck disable=SC2086
  python3 "$harness" x11-click "$wid" $edit_xy && sleep 0.5 || return 1
  python3 "$harness" x11-type "$wid" " edited" && sleep 0.3 || return 1
  import -window "$wid" "$out/screens/kanban-card-editing-x11.png" || return 1
  python3 "$harness" x11-chord "$wid" enter && sleep 0.5 || return 1
  cmp "$v/Board.md" "$kanban_fixtures/edit-board.md" || { echo "Board.md written before Ctrl+S"; return 1; }
  echo "ok Board.md unchanged before Ctrl+S"
  python3 "$harness" x11-chord "$wid" ctrl+s && sleep "$pause" || return 1
  import -window "$wid" "$out/screens/kanban-saved-x11.png" || return 1
  cp "$v/Board.md" "$out/logs/x11-kanban-edit-saved.md"
  cmp "$v/Board.md" "$root/scripts/ui-smoke/kanban-edit-x11.md" || { diff "$root/scripts/ui-smoke/kanban-edit-x11.md" "$v/Board.md"; return 1; }
  echo "ok Board.md after Ctrl+S is kanban-edit-x11.md byte for byte"
  python3 "$harness" x11-close "$wid" || return 1
  local status=124
  for i in $(seq 100); do
    if ! kill -0 "$app_pid" 2>/dev/null; then wait "$app_pid"; status=$?; break; fi
    sleep 0.1
  done
  app_pid=
  echo "app exit status: $status"
  [ "$status" = 0 ] || return 1
  cmp "$v/Board.md" "$root/scripts/ui-smoke/kanban-edit-x11.md" || { echo "closing changed Board.md"; return 1; }
  echo "ok closing left the saved bytes"
  kill "$xvfb_pid" 2>/dev/null; wait "$xvfb_pid" 2>/dev/null; xvfb_pid=
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
