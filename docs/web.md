# Web: architecture decision and staged plan

Status: records the #240 architecture decision (Option A below) and stages the
WASM port. The security boundary is #238. This is a plan, not a promise of order:
each slice is a separate PR with its own review, and the tracker checkboxes live
on #240.

## Current state (surveyed 26 Nov 2026)

**The framework is web-capable.** The pinned Rune (`68d1f4d`) is not the
"experimental web target" #240 was written against:

- `wasm-bindgen` is a dependency of `rune-core`, `rune`, `rune-kit`,
  `rune-text`, `rune-image` and `rune-spell`.
- `App::canvas_id` mounts the app onto a page `<canvas>`
  (`app.rs:983`), with `fit_content_height` for inline embedding.
- A hidden **semantic DOM layer** mirrors headings, links and buttons over the
  canvas, so screen readers and browser Find-in-page work on a GPU-drawn UI.
- Web clipboard (copy events, PNG), web fonts (Inter, cut + full faces,
  staged loading), `Key::ContextMenu` and `OsCursor` all work on web.
- `rune-gpu` has WebGPU/WebGL health checks and a fallback path.
- `rune/scripts/web-build.sh` is a complete packaging path: `wasm-bindgen`
  CLI, optional `wasm-opt`, generated HTTPS-ready HTML, a `--dev --watch`
  loop, bundle size reporting and `--remap-path-prefix` so builder paths stay
  out of the artifact. Muninn's tree does not pull `rune-luau`, so **no WASI
  SDK is needed**; a Muninn web build is plain Rust → wasm-bindgen.

**Muninn's portability ladder** (CI-enforced where it exists):

| Crate | wasm32 status |
| --- | --- |
| `muninn-core` | **clean**, enforced by `xtask wasm-check` in CI (#237/#242) |
| `muninn-graph` | **clean**, same guard |
| `muninn-i18n` | zero dependencies, pure data — trivially portable, not yet guarded |
| `muninn-import` | one dependency (`anydoc`); needs a check, likely gateable |
| `muninn-ui` | ~21 of ~52 modules touch fs / clock / threads (see the boundary) |
| `muninn-sync` | hard-native: blocking `reqwest`, `keyring`, loopback auth, Tailnet host |

`muninn-core` has ~137 filesystem call sites and five clock reads
(`SystemTime`/`Instant`). All of them sit behind `Vault::load` /
`walk` and the write paths — a single seam, not a scatter.

## The decision (#240): reuse `muninn-ui`

**Option A — chosen.** The browser front end reuses `muninn-ui` behind a
default `native` cargo feature, with a thin `muninn-web` crate for the
wasm entry point and web-only surfaces (OPFS, Web Locks, OAuth in the
browser). This replaces the ARCHITECTURE.md line that recommended a separate
front end; that recommendation predates the Rune web maturity above.

Why: the original argument for a separate front end was framework risk. That
risk has moved — the gating questions are now Muninn's own seams (fs, clock,
sync), which **every** architecture needs anyway, and which improve native
testability (a clock argument and a source-provider seam make headless tests
honest). Option B would rebuild the entire app UI (editor, reader, graph,
kanban, board, settings) in a second stack and double every future UI change;
for this project that is the expensive path, not the safe one.

**The early gate (slice 5) makes the decision reversible at the only point
where it is cheap to reverse.** The read-only demo is built *through*
feature-gated `muninn-ui`, not as a throwaway. If the demo fails on bundle
size, WebGPU/WebGL coverage, IME or a11y grounds, falling back to a separate
front end then costs only the demo itself — slices 2–4 are required by either
architecture and survive the pivot.

What stays **native-only, permanently** (the boundary, mostly from #238):

- The Desktop Google client secret (`MUNINN_GOOGLE_CLIENT_SECRET`) — never in a
  wasm bundle; web uses a public Web-application client id with PKCE/GIS.
- `muninn-sync`'s native transports (blocking `reqwest`), the OS keyring,
  loopback auth, the Tailscale host.
- Tray, autostart, self-update, native file dialogs, the vault-filesystem
  lease code (`fsutil`).
- On web these are replaced by new, web-only code (OPFS, Web Locks, GIS) —
  never by sharing the native implementation.

## Target: feature parity (recorded 27 Nov 2026)

The web build is a peer of the desktop app, not a viewer. By the end of the
stage it carries the same note functionality — Write, Read, Graph, Board and
Kanban over the same shared UI, a durable local vault, sync, attachments,
encryption — nothing that touches notes is desktop-only. What stays
desktop-only is OS integration that has no browser meaning (tray, autostart,
the global shortcut, self-update); that is the "complements" in the target:
the same vault, two doors.

The parity bar reshapes the late slices:

- Slice 6 owns **attachments and boards**, not just notes: the source seam
  today reports `![[pic.png]]` broken and lists no boards (per the slice-2
  handoff), so the OPFS source must expose non-note files and board files.
- Slice 6 also owns the **web store** behind the honest refusals of 4b:
  crash recovery, settings persistence, and the `LockRevs` high-water mark.
- **Tailscale is the one open parity question.** The commit-store host it
  needs is unreachable from a browser without a relay, so full parity needs
  either a self-hosted relay (slice 10) or an explicit "Tailscale is a
  desktop door" decision from Raymond. Slices 5-9 are required under any
  answer and proceed first.

## Slices

Slice 1 is merged (#237/#242: `xtask wasm-check`, the `wasm32 check (core,
graph)` CI job). The order below follows #240 with the decision applied.

### 2. `Vault::from_sources` + clock seam (enabling refactor)

Add a source seam so `muninn-core` can build a `Vault` from bytes a caller
supplies, and make time an argument.

- `Vault::load(root)` becomes a wrapper over `Vault::from_sources(...)`, where
  native feeds it the walked filesystem and web feeds it OPFS-read bytes.
  Writes move behind a small sink trait (atomic write, rename, trash,
  recovery) so the same index/preview logic serves both.
- Clock: the five `SystemTime`/`Instant` reads take a `now` argument (or an
  injected clock), per #240's "timestamps as arguments".
- Acceptance: the wasm guard stays green; native behavior is byte-identical
  (the existing suite); at least one test drives `from_sources` with
  non-filesystem sources.

### 3. Target gating + secret canary (#238)

- Gate the `option_env!` Google secret and every native-only module behind
  `#[cfg(feature = "native")]` (the new default feature) so a wasm build cannot
  link them by accident.
- CI canary: build the wasm target with
  `MUNINN_GOOGLE_CLIENT_SECRET=wasm-canary`; fail if the sentinel appears in
  any `.wasm`/`.rlib`. Build native and assert the real secret still resolves.
- Record the boundary in `SECURITY.md`.
- Acceptance: per #238 — no sentinel in the wasm artifact, native unchanged,
  no web OAuth path references the Desktop client id.

### 4. `muninn-ui` `native` feature

Move the ~21 native-coupled modules behind the feature:
`watch`, `doc_sync`, `board_sync`, `kanban_sync`, `sync_ui`, `recovery`,
`autostart`, `self_update`, `tray`, `lock_session`, `import_document`,
`omarchy`, and the fs/clock/threads spots in `state`, `settings`,
`kanban_edit`, `board_view`, `board_tools`, `board_edit`, `kanban_view`,
`kanban_dates`, `sidebar_drag`, `update`. The views themselves (editor,
reader, graph, kanban rendering, board rendering) are feature-neutral and
stay shared.

- Extend `xtask wasm-check` to
  `cargo check -p muninn-ui --no-default-features --target wasm32-unknown-unknown`
  (plus `muninn-i18n`, and `muninn-import` once seamed), so the guard that CI
  already runs grows with the port.
- Acceptance: the guard covers ui-no-native and is green; native builds and
  tests are unchanged (`--all-features` CI path identical).

### 5. `muninn-web` crate + read-only sample-vault demo — **the gate**

- A `cdylib` crate with a `#[wasm_bindgen]` entry that mounts the existing
  `view()` onto `<canvas id="…">`, renders the committed `vault/` sample
  read-only (no storage writes: the source seam supplies bytes, the sink
  trait is a no-op recorder).
- A `scripts/web-build.sh` modeled on Rune's (workspace-generic, no WASI
  dependency), producing a static folder behind nginx locally.
- **Measure and record**: bundle size (raw + gzip; budget suggestion: start
  worrying past ~8–10 MB gzip), WebGPU vs WebGL fallback behavior across
  Chrome/Firefox/Safari, IME typing in the editor, the semantic-DOM a11y
  spot-check.
- Acceptance (from #240): a browser demo renders the sample vault before
  storage or sync work begins. **Go/no-go review here**: this is where the
  Option A/B decision is re-confirmed against measured numbers.

### 6. OPFS vault — the parity slice

- An OPFS-backed source/sink implementing the slice-2 seams;
  `navigator.storage.persist()` on first write; **Web Locks API** for the
  single-writer guarantee (the native fs lease does not exist in a browser);
  export/import (download a zip; File System Access where available).
- **Attachments and boards** (parity bar): the source seam exposes non-note
  files (embeds resolve, sizes read) and `.excalidraw` board files, so the
  web vault renders boards and embeds exactly like the desktop one.
- **The web store** (parity bar): crash recovery, settings persistence and
  the `LockRevs` high-water mark move from the 4b honest refusals onto the
  OPFS store.
- Acceptance: create/rename/move/delete round-trip in OPFS survives reload;
  two tabs cannot hold the write lock; export/import is lossless; an
  attachment embed and a board render identically to desktop.

### 7. Static serving image

- Dockerfile + nginx config with `application/wasm`, CSP, COOP/CORP (needed
  for WebGPU), healthcheck — same shape as `Dockerfile.marketing`.
- Hosting is a separate decision (#240 non-goal: no new hosted service without
  one); the image is buildable and self-hostable first.

### 8–9. Sync in the browser

- GitHub via an async transport or a Web Worker + device-flow relay (slice 8).
- Google Drive via GIS web OAuth with a **public Web-application client id**
  and PKCE — never `MUNINN_GOOGLE_CLIENT_SECRET`, never the Desktop client id
  (slice 9, after the #238 canary exists).

#### 8a. The PAT transport — what landed, and the seam that remains

The auth decision is PAT entry with no hosted service: a probe showed
`api.github.com` is CORS-open, while GitHub's OAuth endpoints are browser
blocked, so device flow is impossible in page JS. 8a therefore builds the
transport, not a sign-in flow.

Two foundations were then verified in a real Chrome under the app's dev-server
conditions: a sync XHR from a dedicated worker to `api.github.com` returns 200
with a readable body, and a dedicated worker's sync OPFS access handle
(`getFileHandle` → `createSyncAccessHandle` → `write` → `flush` → `getSize` →
`close`) works. A third probe fixed the sleeper: on a non-isolated dev origin
(no COOP/COEP) `SharedArrayBuffer` is undefined, so `Atomics.wait` is available
only on the slice-7 image and the bounded spin fallback is required.

Landed and gated:

- `muninn-sync`'s non-default `web` feature compiles the browser's PAT surface
  for `wasm32`: the HTTP boundary and its status mapping, the GitHub client,
  the credential contract with its memory store, the journal's state-home API
  and the state-key table. `clients.rs`, the keyring, the secret, the lease,
  Drive, Tailnet and the engine stay native; `xtask wasm-check` compiles the
  new surface.
- `muninn-web` carries the page↔worker message contract, the pure response
  parsing and the state-key pin (natively tested), plus a `worker` feature with
  the worker's synchronous XHR transport (pinned to `api.github.com`), the
  dedicated `muninn-sync-state/` OPFS state home, the `Atomics.wait` sleeper
  with a bounded non-isolated fallback, and the run-scoped PAT store.

Not landed: the **worker entry** that calls `run_sync_with_state_home`. The
engine's *state* IO was seamed in #324, but its *vault* IO is still `std::fs`:
`inventory`, `read_local_bytes`, `apply_plan_with` and the filesystem lease all
take `RunRequest.root` as a real path, and a browser has no path to hand them —
the vault lives behind `muninn_core`'s `VaultStore`/`MemoryStore` seam. A worker
entry today would accept a run and fail at its first inventory. The honest next
step is a vault-byte seam in the engine (the same shape as #324, for vault
reads/writes), after which the entry is a thin message loop over the machinery
above; §10's Tailscale question is unaffected.

Two smaller recorded differences of the web twin:

- **Sleeper.** `Atomics.wait` needs `crossOriginIsolated`. The slice-7 image
  sets COOP/COEP and is isolated; a plain dev server is not, so the fallback
  spins for at most 250 ms of a requested delay and then returns (a long
  `Retry-After` is retried sooner rather than blocking the worker). The worst
  case is documented on the sleeper.
- **Lease.** There is no filesystem lease. The page's boot already holds a
  `navigator.locks` lock for the vault (one writing tab), and a worker
  processes one message at a time; the run entry additionally takes the named
  Web Lock `muninn-sync-run` across tabs. No lease file and no timeout to
  steal.
- **State durability.** `StateHome` is synchronous and
  `createSyncAccessHandle()` is a Promise, so the worker answers the engine from
  a memory mirror and flushes with sync handles when the run yields. A worker
  killed mid-run loses that run's state writes (the previous flush survives);
  a later slice can pre-open handles for the four fixed keys to narrow that
  window. The browser's live sync gate (8b) must include a mid-run reload.

### 10. Tailscale parity — the open decision.

The desktop's third sync provider hosts a commit store on a Muninn device
reached over the tailnet; a browser cannot reach it directly. Full parity
(recorded target above) needs one of:

- a **self-hosted relay** (the parked server idea): a small Muninn-hosted
  relay service that proxies the browser to a tailnet device's commit store —
  a new hosted surface, so it needs Raymond's explicit OK and a security
  review; or
- an explicit **"Tailscale is a desktop door"** decision, recorded here the
  way tray/autostart are: the web build ships GitHub and Google Drive sync,
  and Tailscale sync stays a reason to also run the desktop app.

Either way the decision lands before the first web release, so the release
notes can say exactly what syncs where.

## Risks (revisited)

- **WebGPU coverage** — #240's gating risk, now testable at slice 5 with the
  WebGL fallback; the demo is the measurement, not the hope.
- **Bundle size** — rune + markdown + fonts; `wasm-opt` and the size report
  are already in the build script; the demo records the real number.
- **Storage eviction** — OPFS can be evicted; `persist()` plus export/import
  is the answer, and the UI must say so.
- **IME / mobile keyboards** — rune's changelog shows web input work; the demo
  includes a typing pass.
- **Rune pin drift** — the pin moves fast; that is exactly why the wasm guard
  must keep growing (slices 3–4) so drift fails CI instead of surprise.
- **Single writer** — Web Locks, with the same recovery-copy thinking as the
  native lease.
