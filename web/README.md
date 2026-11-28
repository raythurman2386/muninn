# Serving the browser vault

`Dockerfile.web` (repo root) builds the wasm bundle from source and serves it
with nginx on port 80. It is the deployment artifact for `muninn-web`; the
vault itself is still the static folder `scripts/web-build.sh` produces, and
the image is a thin packaging of that folder.

This slice ships the image, not a hosted service. Hosting is a separate
decision tracked on #240; anyone can run the image on their own machine or
server today.

## Build and run

From the repo root:

```sh
docker build -f Dockerfile.web -t muninn-web .
docker run --rm -p 8080:80 muninn-web
```

Then open <http://localhost:8080/>. The container is healthy when `GET /`
returns 2xx:

```sh
docker inspect --format '{{.State.Health.Status}}' <container>
curl -sI http://localhost:8080/ | grep -iE 'content-security-policy|cross-origin'
```

The build needs network access (crates.io, the pinned Rune revision on
GitHub) and a few GB of disk for the Rust and wasm-bindgen build caches.
Stage 2 contains only nginx and the static files; Rust and the source never
reach the runtime image.

## The sync worker

The folder carries a second wasm-bindgen bundle beside the page: `worker.js`
(a module shim) and `worker/` (`sync_worker.js`, `sync_worker_bg.wasm` and the
worker's own inline-JS snippets). It is built by the same
`scripts/web-build.sh` invocation as the page; the page's `app_bg.wasm` does
not contain the engine. The worker is in its own subfolder because
wasm-bindgen's snippet folder names depend on the crate, not the contents, and
both sides import `muninn_lock` from one. The page starts the worker on demand
from the Sync surface (slice 8b); nothing runs it at boot.

A CSP detail: a worker inherits the page's Content-Security-Policy, and
`worker-src` falls back to `script-src`, so the generated `'self'
'wasm-unsafe-eval'` list covers both `worker.js` and the worker's wasm
instantiation.

## Google Drive sign-in (slice 9)

Drive sync needs no relay and no redirect page: the page loads Google Identity
Services' client from `https://accounts.google.com/gsi/client` on the first
Connect and Google opens its own popup. That means this image's headers carry
two deliberate settings:

- `script-src` admits `https://accounts.google.com` (the GIS loader sends
  `Cross-Origin-Resource-Policy: cross-origin`, so `require-corp` still
  admits it) and `connect-src` admits `https://api.github.com`,
  `https://www.googleapis.com` and `https://oauth2.googleapis.com` for the two
  sync providers' data planes.
- `Cross-Origin-Opener-Policy` is **`same-origin-allow-popups`**. The OAuth
  popup reports back through `window.opener`; `same-origin` would sever it and
  Drive sign-in could not complete. `crossOriginIsolated` is therefore not set
  on this image, so the sync worker's `Atomics.wait` sleeper uses its
  documented bounded-spin fallback. WebGPU and the rest of the app do not need
  isolation.

The client id is a build input, not a secret. A build with no
`MUNINN_GOOGLE_WEB_CLIENT_ID` is honestly unconfigured: the Drive surface says
so and Connect sends nothing. The registration steps (a **Web application**
OAuth client bound to this deployment's origins) are in
`docs/web.md` at slice 9. Never set `MUNINN_GOOGLE_CLIENT_SECRET` for this
image and never use the Desktop client id; the #238 canary enforces that the
secret cannot reach a wasm artifact.

## What nginx sends

- `Content-Type: application/wasm` for `app_bg.wasm` and the sync worker's
  `worker/sync_worker_bg.wasm` (declared in `web/nginx/default.conf`; the stock
  nginx table also knows it).
- A per-build Content-Security-Policy. `web/nginx/csp-hash.py` hashes the
  inline boot script in the built `index.html`; script-src is
  `'self' 'wasm-unsafe-eval'` plus that hash, with no `'unsafe-inline'`.
  `worker-src` falls back to script-src, so `worker.js` and its wasm
  initialisation are covered by the same `'self'` and `'wasm-unsafe-eval'`.
- `Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy:
  require-corp` and `Cross-Origin-Resource-Policy: same-origin`, which turn
  on `crossOriginIsolated`. WebGPU does not require it; SharedArrayBuffer and
  some future worker/WASM-thread paths do. Nothing in the bundle is
  cross-origin, so the stricter policy is free.
- `no-cache` (revalidate) for `index.html`, `app.js`, `app_bg.wasm`,
  `worker.js`, `worker/sync_worker.js`, `worker/sync_worker_bg.wasm` and the
  `snippets/` folders because those names survive a rebuild; a week of caching
  for the stable Inter faces.
- gzip. `app_bg.wasm`, `app.js` and the sync worker's two files are
  pre-compressed into `.gz` files at image build time and served by
  `gzip_static`; HTML and any other text fall back to on-the-fly gzip. The
  stock nginx image has no brotli module.

## GoForge

- Dockerfile Path: `Dockerfile.web`
- Build Context: `.` (repo root)
- Health check: `GET /` must return 2xx (the image declares this as a
  `HEALTHCHECK`).

No GitHub workflow builds or publishes this image; that is a release decision
on #240, not part of slice 7.

## Checking the CSP without a browser

After `bash scripts/web-build.sh muninn-web --out target/web`:

```sh
python3 web/nginx/csp-hash.py --html target/web/index.html \
  --out /tmp/security-headers.conf
python3 web/nginx/csp-hash.py --html target/web/index.html \
  --out /tmp/security-headers.conf --check
```
