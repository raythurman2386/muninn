# Kanban slice 1 smoke proof

Output of `dbus-run-session -- scripts/ui-smoke.sh` for the kanban note model, including every existing step plus `headless_kanban_model` and `x11_kanban_model`.

Run from commit `6bf8e311c5ebf95ecfe3e316f9f37412d32ef46a` on `cursor/muninn-kanban-s1-model`.

The windowed steps used Mesa lavapipe. `vulkaninfo --summary` reported device `llvmpipe (LLVM 20.1.2, 256 bits)`, driver `llvmpipe`, Mesa 25.2.8. Headless replay set `WGPU_BACKEND=vulkan` to that ICD. The x11 screenshot matched the headless Write shot (mean absolute difference 0).
