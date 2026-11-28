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

## What nginx sends

- `Content-Type: application/wasm` for `app_bg.wasm` (declared in
  `web/nginx/default.conf`; the stock nginx table also knows it).
- A per-build Content-Security-Policy. `web/nginx/csp-hash.py` hashes the
  inline boot script in the built `index.html`; script-src is
  `'self' 'wasm-unsafe-eval'` plus that hash, with no `'unsafe-inline'`.
- `Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy:
  require-corp` and `Cross-Origin-Resource-Policy: same-origin`, which turn
  on `crossOriginIsolated`. WebGPU does not require it; SharedArrayBuffer and
  some future worker/WASM-thread paths do. Nothing in the bundle is
  cross-origin, so the stricter policy is free.
- `no-cache` (revalidate) for `index.html`, `app.js`, `app_bg.wasm` and
  `snippets/` because those names survive a rebuild; a week of caching for
  the stable Inter faces.
- gzip. `app_bg.wasm` and `app.js` are pre-compressed into `.gz` files at
  image build time and served by `gzip_static`; HTML and any other text fall
  back to on-the-fly gzip. The stock nginx image has no brotli module.

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
