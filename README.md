# Proof: #42 single instance (cursor/muninn-42-single-instance @ 6c4aceb)

Run on minis, headless: own Xvfb `:97`, lavapipe, isolated HOME/XDG dirs and
`XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY`/`WAYLAND_SOCKET`/`DISPLAY`/`DBUS_SESSION_BUS_ADDRESS`
unset, and a private `dbus-run-session` bus whose only tray host is
`scripts/ui-smoke/sni_watcher.py`. Nothing touched the desktop session, bus or tray.

- `logs/x11_single_instance.log`: the new step, **21/21 checks**, exactly one
  StatusNotifierItem after launch, after a second launch (exit 0, hidden window shown
  again, one process), across Show tray icon off/on/on (0/1/1), a tray-host restart,
  a vault-switch launch, and kill -9 + relaunch.
- `logs/ui-smoke-summary.txt`: the full ui-smoke run with the hardened `isolate()`,
  **26/26 steps PASS**.
- `logs/control-60902d0-x11_single_instance.log`: negative control, the same step on
  the base binary (main 60902d0): the second launch does not hand over (it runs as a
  second app until `timeout` kills it, rc 124) and the hidden window never comes back.
- `logs/cargo-test-summary.txt`: `cargo test --locked --workspace --all-targets`,
  558 passed, 0 failed, 2 ignored.
- `screens/x11-single-*.png`: 1 launch, 2 hidden in the tray, 3 after the second
  launch, 4 Settings › Appearance, 5 tray off, 6 tray on again, 7 vault B after the
  vault-switch launch, 8 relaunch after kill -9. `screens/single-settings-*.png`: the
  headless dumps whose trees give the click targets.

Rerun: `UI_SMOKE_STEPS="build x11_single_instance" scripts/ui-smoke.sh`
