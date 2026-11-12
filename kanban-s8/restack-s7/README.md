# Kanban slice 8 restacked onto slice 7

PR head: `3b841bff123195b9647bdaa90acbe607d001c483` on `cursor/muninn-kanban-s8-links`.

Base: `cursor/muninn-kanban-s7-sync` at `811ca27869db7def99326ba576b073ff1afb26ca`.

Full `scripts/ui-smoke.sh`: 45 PASS, 0 FAIL, exit 0. Lavapipe (`/usr/share/vulkan/icd.d/lvp_icd.json`), `WGPU_BACKEND=vulkan`. Each step used its own `XDG_RUNTIME_DIR`. Windowed steps used a private Xvfb and `--new-instance`. The single-instance step used `dbus-run-session`.

No new screenshots. The dirty-rename notice is unchanged: headless kanban links asserts `Links in Board were not updated`.

`summary.txt` is that run. `logs/` is one file per step.
