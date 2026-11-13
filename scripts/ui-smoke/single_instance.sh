#!/usr/bin/env bash
# The x11_single_instance step (#42), run by scripts/ui-smoke.sh inside
# `dbus-run-session`: a private session bus with sni_watcher.py as the only
# tray host, so the count of registered StatusNotifierItems is the count of
# Muninn tray icons. It never sees the desktop's bus or tray.
#
# The step exports: bin harness out DISPLAY vault_a vault_b vaults_tree
# appearance_tree, plus the isolated HOME / XDG_* / XDG_RUNTIME_DIR.
set -uo pipefail
here="$(CDPATH= cd -- "$(dirname "$0")" && pwd)"
s="$out/screens" logs="$out/logs"
settings="$XDG_CONFIG_HOME/muninn/settings.json"
checks=0 passed=0 watcher_pid= app_pid=

case "${DBUS_SESSION_BUS_ADDRESS:-}" in
  unix:path=/tmp/dbus-*|unix:path="$XDG_RUNTIME_DIR"*|unix:abstract=*|unix:dir=*|unix:tmpdir=*) ;;
  *) echo "refusing: not a private dbus-run-session bus (${DBUS_SESSION_BUS_ADDRESS:-unset})"; exit 1 ;;
esac
echo "private bus: $DBUS_SESSION_BUS_ADDRESS"

stop() {
  [ -n "$app_pid" ] && kill "$app_pid" 2>/dev/null
  [ -n "$watcher_pid" ] && kill "$watcher_pid" 2>/dev/null
}

items() { # the registered tray items, one per line
  gdbus call --session --dest org.kde.StatusNotifierWatcher --object-path /StatusNotifierWatcher \
    --method org.freedesktop.DBus.Properties.Get org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems \
    2>/dev/null | grep -o ':[0-9]*\.[0-9]*' || true
}
count() { items | grep -c . || true; }
windows() { python3 "$harness" x11-count Muninn; }
apps() { pgrep -f -c -x "$bin.*" || true; }
shot() { import -window root "$s/x11-single-$1.png"; }
trap 'stop; rm -f "$out/work-switch.png"' EXIT

expect() { # expect N LABEL: exactly N tray items, still N a moment later
  local want="$1" label="$2" i got
  checks=$((checks + 1))
  for i in $(seq 50); do got="$(count)"; [ "$got" = "$want" ] && break; sleep 0.1; done
  sleep 1.5
  got="$(count)"
  if [ "$got" = "$want" ]; then
    passed=$((passed + 1))
    echo "ok   $want SNI item(s): $label [$(items | tr '\n' ' ')] windows=$(windows) processes=$(apps)"
  else
    echo "FAIL want $want SNI item(s), got $got: $label [$(items | tr '\n' ' ')]"
  fi
}
assert() { # assert LABEL COMMAND...: a non-count check
  local label="$1"; shift
  checks=$((checks + 1))
  if "$@"; then passed=$((passed + 1)); echo "ok   $label"; else echo "FAIL $label"; fi
}

start_watcher() {
  python3 "$here/sni_watcher.py" >>"$logs/x11-single-watcher.log" 2>&1 &
  watcher_pid=$!
  local i
  for i in $(seq 50); do grep -q watching "$logs/x11-single-watcher.log" 2>/dev/null && return 0; sleep 0.1; done
  echo "the test watcher did not take org.kde.StatusNotifierWatcher"; return 1
}
restart_watcher() {
  kill "$watcher_pid"; wait "$watcher_pid" 2>/dev/null
  : >"$logs/x11-single-watcher.log"
  start_watcher
}

launch() { # launch LOG: the first instance, in the background; waits for its window
  "$bin" --vault "$vault_a" >>"$logs/$1" 2>&1 &
  app_pid=$!
  wid="$(python3 "$harness" x11-wait Muninn 30)" || return 1
  sleep 2
}
setting() { python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get(sys.argv[2]))' "$settings" "$1"; }
alive() { kill -0 "$1" 2>/dev/null; }
mapped() { [ "$(windows)" = "$1" ]; }
wait_mapped() { local i; for i in $(seq 100); do mapped "$1" && return 0; sleep 0.1; done; return 1; }

start_watcher || exit 1

# 1. One launch: one icon.
launch x11-single-a.log || exit 1
first_pid="$app_pid"
expect 1 "after launch (pid $first_pid)"
shot 1-launch

# 2. Close to tray, then launch again as a launcher would: the second process
#    hands over and exits 0, and the hidden window comes back.
python3 "$harness" x11-close "$wid" || exit 1
assert "closing hides the window (close to tray)" wait_mapped 0
assert "the app keeps running in the tray" alive "$first_pid"
shot 2-hidden
second_rc=0
XDG_ACTIVATION_TOKEN=smoke-token timeout 20 "$bin" --vault "$vault_a" >>"$logs/x11-single-b.log" 2>&1 || second_rc=$?
assert "the second launch exits 0 (got $second_rc)" [ "$second_rc" = 0 ]
assert "the hidden window is shown again" wait_mapped 1
assert "one Muninn process" [ "$(apps)" = 1 ]
expect 1 "after a second launch"
shot 3-second-launch

# 3. Settings › Appearance › Show tray icon: off, on, then on again after
#    closing and reopening Settings.
wid="$(python3 "$harness" x11-wait Muninn 10)" || exit 1
appearance_tab="$(python3 - "$vaults_tree" <<'PY'
import json, sys
tree = json.load(open(sys.argv[1]))
def walk(ns):
    for n in ns:
        yield n
        yield from walk(n.get("children", []))
for n in walk(tree["elements"]):
    if n.get("label") == "Appearance" and isinstance(n.get("rect"), list):
        x, y, w, h = n["rect"]
        print(round(x + w / 2), round(y + h / 2))
        break
else:
    raise SystemExit("no Appearance settings tab")
PY
)" # the Appearance segment, whatever the tab count
# The switch's track, from the headless tree. The windowed dialog can sit a
# few pixels off the headless one, so find the track's rows on screen in its
# column (an on track is the accent colour, an off one grey, both far from
# the dialog's dark fill) and click the middle of them.
tray_switch() {
  import -window "$wid" "$out/work-switch.png" &&
    python3 - "$appearance_tree" "$out/work-switch.png" <<'PY'
import json, sys
from PIL import Image
def walk(ns):
    for n in ns:
        yield n
        yield from walk(n.get("children", []))
tree = json.load(open(sys.argv[1]))
x, y, w, h = next(n["rect"] for n in walk(tree["elements"])
                  if n.get("path", "").endswith("/settings-tray-icon/control"))
im = Image.open(sys.argv[2]).convert("RGB")
cx = round(x + 6)  # left of the thumb's travel middle, inside the track either way
def lit(r):
    p = im.getpixel((cx, r))
    return max(p) - min(p) > 60 or min(p) > 80
runs, run = [], []
for r in range(round(y - 24), round(y + h + 24)):
    if lit(r):
        run.append(r)
    elif run:
        runs.append(run); run = []
runs.append(run)
track = max(runs, key=len)  # 16 rows of track; label text is only a few
if len(track) < h / 2:
    sys.exit("no switch track found near the tree's rect")
print(round(x + w / 2), (track[0] + track[-1]) // 2)
PY
}
open_appearance() {
  python3 "$harness" x11-chord "$wid" ctrl+comma && sleep 1 &&
    python3 "$harness" x11-click "$wid" $appearance_tab && sleep 1
}
open_appearance || exit 1
shot 4-appearance
python3 "$harness" x11-click "$wid" $(tray_switch) || exit 1
expect 0 "Show tray icon off"
assert "settings.json tray_icon is false" [ "$(setting tray_icon)" = False ]
shot 5-tray-off
python3 "$harness" x11-click "$wid" $(tray_switch) || exit 1
expect 1 "Show tray icon on"
assert "settings.json tray_icon is true" [ "$(setting tray_icon)" = True ]
python3 "$harness" x11-chord "$wid" esc && sleep 1 && open_appearance || exit 1
expect 1 "Show tray icon still on after reopening Settings"
shot 6-tray-on-again
python3 "$harness" x11-chord "$wid" esc && sleep 1

# 4. The tray host restarts (a bar reload): the app registers once again.
restart_watcher || exit 1
expect 1 "after a tray-host restart"

# 5. A launch naming another vault switches the running app to it.
third_rc=0
timeout 20 "$bin" --vault "$vault_b" >>"$logs/x11-single-c.log" 2>&1 || third_rc=$?
assert "the vault-switch launch exits 0 (got $third_rc)" [ "$third_rc" = 0 ]
switched() { [ "$(setting last_vault)" = "$(cd "$vault_b" && pwd -P)" ]; }
wait_switched() { local i; for i in $(seq 100); do switched && return 0; sleep 0.1; done; return 1; }
assert "the running app switched to vault B" wait_switched
assert "still one Muninn process" [ "$(apps)" = 1 ]
expect 1 "after a vault switch"
shot 7-vault-b

# 6. kill -9: no stale lock or icon; a relaunch starts fresh with one icon.
kill -9 "$first_pid"; wait "$first_pid" 2>/dev/null; app_pid=
expect 0 "after kill -9"
launch x11-single-d.log || exit 1
assert "the relaunch is a new process" [ "$app_pid" != "$first_pid" ]
expect 1 "after kill -9 and relaunch (pid $app_pid)"
shot 8-relaunch
kill "$app_pid"; wait "$app_pid" 2>/dev/null; app_pid=
expect 0 "after a normal quit"

echo "single instance: $passed/$checks checks passed"
[ "$passed" = "$checks" ]
