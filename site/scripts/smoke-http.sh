#!/usr/bin/env bash
# HTTP smoke test for the served marketing site (the Docker image in CI, or a local nginx).
# Usage: site/scripts/smoke-http.sh http://127.0.0.1:8080
# Checks status codes, content types, cache headers, security headers (including CSP),
# gzip, video Range support, /privacy/, sitemap, robots.txt and the branded 404.
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
expect "404 has CSP" "$h" '^content-security-policy:'
body404=$(curl -sS "$BASE/definitely-not-here")
expect "404 is branded" "$body404" 'Page not found'

# W3: CSP and the rest of the security headers on /
h=$(hdr "$BASE/")
expect "/ CSP header" "$h" '^content-security-policy:'
expect "/ CSP default-src self" "$h" "content-security-policy:.*default-src 'self'"
if grep -qiE "content-security-policy:.*unsafe-inline" <<<"$h"; then echo "FAIL / CSP contains unsafe-inline"; fail=1; else echo "ok   / CSP has no unsafe-inline"; fi
expect "/ CSP allows Umami" "$h" "content-security-policy:.*umami-app\\.ravenwoodsoftware\\.dev"
expect "/ permissions-policy" "$h" '^permissions-policy: camera=\(\), microphone=\(\), geolocation=\(\), payment=\(\), usb=\(\)'

h=$(hdr "$BASE/privacy/")
expect "/privacy/ 200" "$h" '^HTTP/[0-9.]+ 200'
expect "/privacy/ text/html" "$h" '^content-type: text/html'
privacy=$(curl -sS "$BASE/privacy/")
expect "/privacy/ has the policy heading" "$privacy" 'Privacy Policy'
expect "/privacy/ has the effective date" "$privacy" 'Effective date: November 9, 2026'
expect "/privacy/ has the Umami tag once" "$(grep -o 'umami-app.ravenwoodsoftware.dev/script.js' <<<"$privacy" | wc -l)" '^1$'
# The page must render the repo file, not a hand-copied fork: a distinctive sentence.
expect "/privacy/ matches PRIVACY.md" "$privacy" 'Muninn does not create a Muninn account'

h=$(hdr "$BASE/robots.txt")
expect "/robots.txt 200" "$h" '^HTTP/[0-9.]+ 200'
expect "/robots.txt text/plain" "$h" '^content-type: text/plain'
robots=$(curl -sS "$BASE/robots.txt")
expect "/robots.txt has a Sitemap" "$robots" 'Sitemap: https://muninn.ravenwoodsoftware.dev/sitemap-index.xml'
expect "/robots.txt keeps content signals" "$robots" 'Content-Signal: search=yes,ai-train=no,use=reference'

h=$(hdr "$BASE/sitemap-index.xml")
expect "/sitemap-index.xml 200" "$h" '^HTTP/[0-9.]+ 200'
expect "/sitemap-index.xml xml" "$h" '^content-type: application/(xml|xml-sitemap|rss\+xml)|text/xml'
sitemap=$(curl -sS "$BASE/sitemap-index.xml")
expect "/sitemap-index.xml is a sitemap index" "$sitemap" 'sitemapindex'

h=$(hdr "$BASE/sitemap.xml")
expect "/sitemap.xml 200 (alias)" "$h" '^HTTP/[0-9.]+ 200'

h=$(hdr "$BASE/favicon.ico")
expect "/favicon.ico 200" "$h" '^HTTP/[0-9.]+ 200'
h=$(hdr "$BASE/favicon.svg")
expect "/favicon.svg 200" "$h" '^HTTP/[0-9.]+ 200'
expect "/favicon.svg svg" "$h" '^content-type: image/svg\+xml'
h=$(hdr "$BASE/site.webmanifest")
expect "/site.webmanifest 200" "$h" '^HTTP/[0-9.]+ 200'
expect "/site.webmanifest type" "$h" '^content-type: application/manifest\+json'
h=$(hdr "$BASE/assets/og.png")
expect "/assets/og.png 200" "$h" '^HTTP/[0-9.]+ 200'
expect "/assets/og.png png" "$h" '^content-type: image/png'

if [ "$fail" -ne 0 ]; then echo "SMOKE: FAIL"; exit 1; fi
echo "SMOKE: PASS"
