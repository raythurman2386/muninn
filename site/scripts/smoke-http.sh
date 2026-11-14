#!/usr/bin/env bash
# HTTP smoke test for the served marketing site (the Docker image in CI, or a local nginx).
# Usage: site/scripts/smoke-http.sh http://127.0.0.1:8080
# Checks status codes, content types, cache headers, security headers, gzip and video Range support.
set -euo pipefail
BASE="${1:?usage: smoke-http.sh BASE_URL}"
BASE="${BASE%/}"
fail=0

hdr() { curl -sS -o /dev/null -D - "$@" | tr -d '\r'; }
expect() { # name, haystack, regex
  if grep -qiE "$3" <<<"$2"; then echo "ok   $1"; else echo "FAIL $1 (want /$3/)"; fail=1; fi
}

h=$(hdr "$BASE/")
expect "/ 200" "$h" '^HTTP/[0-9.]+ 200'
expect "/ text/html" "$h" '^content-type: text/html'
expect "/ no-cache" "$h" '^cache-control: no-cache'
expect "/ nosniff" "$h" '^x-content-type-options: nosniff'
expect "/ referrer-policy" "$h" '^referrer-policy: strict-origin-when-cross-origin'
expect "/ x-frame-options" "$h" '^x-frame-options: SAMEORIGIN'
expect "/ no server version" "$h" '^server: nginx\s*$'

body=$(curl -sS "$BASE/")
expect "/ is the Astro build" "$body" '/_astro/[^"]+\.css'
expect "/ has the Umami tag once" "$(grep -o 'umami-app.ravenwoodsoftware.dev/script.js' <<<"$body" | wc -l)" '^1$'

h=$(hdr -H 'Accept-Encoding: gzip' "$BASE/")
expect "/ gzip" "$h" '^content-encoding: gzip'

css=$(grep -oE '/_astro/[^"]+\.css' <<<"$body" | head -1)
h=$(hdr -H 'Accept-Encoding: gzip' "$BASE$css")
expect "$css 200" "$h" '^HTTP/[0-9.]+ 200'
expect "$css text/css" "$h" '^content-type: text/css'
expect "$css immutable" "$h" '^cache-control: public, max-age=31536000, immutable'
expect "$css gzip" "$h" '^content-encoding: gzip'

h=$(hdr "$BASE/assets/fonts/InterVariable-latin.woff2")
expect "font 200" "$h" '^HTTP/[0-9.]+ 200'
expect "font font/woff2" "$h" '^content-type: font/woff2'
expect "font cache 1h" "$h" '^cache-control: public, max-age=3600'

h=$(hdr "$BASE/assets/img/dark/preview-1600.webp")
expect "still image/webp" "$h" '^content-type: image/webp'

h=$(hdr "$BASE/assets/logo.svg")
expect "logo image/svg+xml" "$h" '^content-type: image/svg\+xml'

v="$BASE/assets/video/dark-hero-tour.mp4"
h=$(hdr "$v")
expect "video 200" "$h" '^HTTP/[0-9.]+ 200'
expect "video video/mp4" "$h" '^content-type: video/mp4'
expect "video accept-ranges" "$h" '^accept-ranges: bytes'
h=$(hdr -H 'Range: bytes=0-1023' "$v")
expect "video Range 206" "$h" '^HTTP/[0-9.]+ 206'
expect "video Range content-range" "$h" '^content-range: bytes 0-1023/[0-9]+'
expect "video Range length" "$h" '^content-length: 1024'
h=$(hdr -H 'Range: bytes=1000000-' "$v")
expect "video open Range 206" "$h" '^HTTP/[0-9.]+ 206'

h=$(hdr "$BASE/definitely-not-here")
expect "missing page 404" "$h" '^HTTP/[0-9.]+ 404'
expect "404 keeps nosniff" "$h" '^x-content-type-options: nosniff'

# /sponsor/ and /press/ (W8). Kept at the end so a merge with W3/W4's
# extra checks applies cleanly above this block.
body=$(curl -sS "$BASE/sponsor/")
h=$(hdr "$BASE/sponsor/")
expect "/sponsor/ 200" "$h" '^HTTP/[0-9.]+ 200'
expect "/sponsor/ text/html" "$h" '^content-type: text/html'
expect "/sponsor/ heading" "$body" 'Sponsor Muninn'
expect "/sponsor/ GitHub Sponsors" "$body" 'https://github.com/sponsors/raythurman2386'
expect "/sponsor/ no payment form" "$(grep -ciE '<form' <<<"$body")" '^0$'
expect "/sponsor/ has the Umami tag once" "$(grep -o 'umami-app.ravenwoodsoftware.dev/script.js' <<<"$body" | wc -l)" '^1$'
expect "/sponsor/ og:image" "$body" 'property="og:image" content="https://muninn.ravenwoodsoftware.dev/assets/og/sponsor.png"'

body=$(curl -sS "$BASE/press/")
h=$(hdr "$BASE/press/")
expect "/press/ 200" "$h" '^HTTP/[0-9.]+ 200'
expect "/press/ text/html" "$h" '^content-type: text/html'
expect "/press/ heading" "$body" '<h1>Press</h1>'
expect "/press/ logo svg" "$body" '/assets/logo.svg'
expect "/press/ logo png" "$body" '/assets/logo.png'
expect "/press/ version" "$body" 'v0\.8\.2'
expect "/press/ has the Umami tag once" "$(grep -o 'umami-app.ravenwoodsoftware.dev/script.js' <<<"$body" | wc -l)" '^1$'
expect "/press/ og:image" "$body" 'property="og:image" content="https://muninn.ravenwoodsoftware.dev/assets/og/press.png"'

h=$(hdr "$BASE/assets/og.png")
expect "og home 200" "$h" '^HTTP/[0-9.]+ 200'
expect "og home png" "$h" '^content-type: image/png'
h=$(hdr "$BASE/assets/og/sponsor.png")
expect "og sponsor 200" "$h" '^HTTP/[0-9.]+ 200'
expect "og sponsor png" "$h" '^content-type: image/png'
h=$(hdr "$BASE/assets/logo.png")
expect "logo png 200" "$h" '^HTTP/[0-9.]+ 200'
expect "logo png type" "$h" '^content-type: image/png'

if [ "$fail" -ne 0 ]; then echo "SMOKE: FAIL"; exit 1; fi
echo "SMOKE: PASS"
