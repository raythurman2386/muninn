---
name: verify-site
description: Verify the Muninn marketing site (site/, Astro static build served by nginx via Dockerfile.marketing). Use after any change under site/, Dockerfile.marketing, .dockerignore or .github/workflows/site.yml.
---

# Verify the marketing site

The site is an Astro project rooted at `site/`. It builds to `site/dist/` and ships as the nginx image in `Dockerfile.marketing`, which GoForge auto-deploys from `main` to muninn.ravenwoodsoftware.dev. Every merge to `main` goes live.

## Checks

1. **Build and types.** In `site/`, run `npm ci`, `npx --no-install astro check` (must report 0 errors) and `npm run build`.
2. **Pixel parity with the last static page.** Run `bash site/scripts/verify-w1.sh`. It serves the old `site/` from `OLD_REF` (default `c8af582`) and the new `dist/` on 127.0.0.1. It takes full-page headless Chrome screenshots at 1440x900 and 390x844 in dark and light, plus reduced motion, and requires 0 differing pixels. Options:
   - `LIGHTHOUSE=1` runs the pinned Lighthouse devDependency with Umami blocked.
   - `NEW_URL=http://127.0.0.1:PORT/` compares a running server instead of `dist/`.
   - `NEW_DIR`, `SKIP_BUILD=1` and `PROOF_DIR` are documented in the script header.
   - `CHROME_PATH` points at a system Chrome; nothing downloads a browser.
   - Only raise `MAX_DIFF_PIXELS` for an intended visual change, and say so in the PR.
3. **Served behaviour.** Run `bash site/scripts/smoke-http.sh BASE_URL` against the image or any nginx running `site/nginx/default.conf`. It checks status codes, content types, cache headers (immutable `/_astro/*`, 1 h `/assets/*`, no-cache HTML), security headers including CSP, gzip, mp4 Range (206), the branded 404, `/privacy/`, `/robots.txt`, `/sitemap-index.xml` and the favicon set.
4. **Privacy text.** After a build, run `npm run verify:privacy` in `site/`. It diffs the rendered `/privacy/` text against `PRIVACY.md` (normalized). They must be identical: the page is built from that file, not a copy.
5. **Image.** CI (`.github/workflows/site.yml`) builds `Dockerfile.marketing` and asserts `EXPOSE 80` and the exact `HEALTHCHECK`. It runs the container, waits for healthy and runs `smoke-http.sh`. GoForge depends on that port and health check, so never change them silently. `PRIVACY.md` must stay in the Docker build context (see `.dockerignore`) because `/privacy/` reads it at build time.

## Rules

- Serve only on 127.0.0.1 and use headless browsers. Do not use Docker on a shared box's docker0 bridge; the CI runner does the docker build and run.
- The Umami script stays blocked in every automated browser and Lighthouse run, so tests never record pageviews.
- Do not merge or deploy without the owner's OK. When a deploy changes asset URLs (for example the first Astro deploy removed `/styles.css` and `/main.js`), ask for a Cloudflare cache purge.
