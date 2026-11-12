# Kanban slice 8 proof

PR head: `47d457a0e04b801ef725af5204e88bae70c96159` on `cursor/muninn-kanban-s8-links`.

Base: `cursor/muninn-kanban-s6-drag` at `bf694c05869ea73ed1c10587775788adbdbc6930`.

Backend: lavapipe (`/usr/share/vulkan/icd.d/lvp_icd.json`), `WGPU_BACKEND=vulkan`. The windowed step used a private Xvfb, `--new-instance`, and its own `XDG_RUNTIME_DIR`.

Screenshots (the slice's two):

- `screens/kanban-link-complete.png` — typing `[[` in a card offers Roadmap.
- `screens/kanban-links-renamed-x11.png` — after renaming Roadmap to Plan, the card shows the new link.

`summary.txt` is the full `scripts/ui-smoke.sh` run. `logs/` has one log per step.
