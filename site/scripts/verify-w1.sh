#!/usr/bin/env bash
# W1 proof for the Astro port of the Muninn marketing page.
#
#   1. npm ci, npm run build, astro check (must all pass)
#   2. extract the old static site/ from OLD_REF (default c8af582, the last static-site commit)
#   3. serve old and new on 127.0.0.1 (random ports), take full-page headless Chrome screenshots
#      at 1440x900 and 390x844 in dark and light, and compare them exactly (0 differing pixels by
#      default; raise MAX_DIFF_PIXELS only for intended visual changes)
#   4. optional Lighthouse (LIGHTHOUSE=1, the pinned devDependency), with Umami blocked so no
#      pageview is sent
#
# Env: PROOF_DIR (default site/.proof-w1), OLD_REF, CHROME_PATH (default /usr/bin/google-chrome),
#      MAX_DIFF_PIXELS (default 0), LIGHTHOUSE (0/1), NEW_URL (compare a running server, e.g.
#      the nginx image, instead of serving dist/), SKIP_BUILD=1 (reuse an existing dist/),
#      NEW_DIR (compare another build directory, e.g. a mutated copy, instead of dist/).
# Everything stays local: no Docker, no network services beyond 127.0.0.1, headless browser only.
set -euo pipefail

SITE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_DIR="$(git -C "$SITE_DIR" rev-parse --show-toplevel)"
PROOF_DIR="${PROOF_DIR:-$SITE_DIR/.proof-w1}"
OLD_REF="${OLD_REF:-c8af582}"
export CHROME_PATH="${CHROME_PATH:-/usr/bin/google-chrome}"
export ASTRO_TELEMETRY_DISABLED=1

mkdir -p "$PROOF_DIR"
cd "$SITE_DIR"

if [ "${SKIP_BUILD:-0}" != 1 ]; then
  echo "== npm ci"
  npm ci --no-audit --no-fund
  echo "== npm run build"
  npm run build 2>&1 | tee "$PROOF_DIR/build.log"
  echo "== astro check"
  npx --no-install astro check 2>&1 | tee "$PROOF_DIR/astro-check.log"
fi

echo "== old site from $OLD_REF"
rm -rf "$PROOF_DIR/old"
mkdir -p "$PROOF_DIR/old"
git -C "$REPO_DIR" archive "$OLD_REF" site | tar -x -C "$PROOF_DIR/old"

echo "== screenshots + pixel diff"
node scripts/verify-w1.mjs --old "$PROOF_DIR/old/site" --new "${NEW_DIR:-$SITE_DIR/dist}" --out "$PROOF_DIR"
