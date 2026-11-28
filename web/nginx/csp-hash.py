#!/usr/bin/env python3
"""Fill the nginx CSP template with the hashes of the inline scripts in the
built index.html.

The browser bundle's page carries one inline <script type="module"> boot shim
(written by scripts/web-build.sh): it picks WebGPU or WebGL2 and starts
app.js. With no 'unsafe-inline' in script-src, that exact body needs a CSP
hash. Generating the hash at image build time means the policy cannot drift
from the page.

    python3 web/nginx/csp-hash.py --html target/web/index.html \
        --out web/nginx/security-headers.conf

    python3 web/nginx/csp-hash.py --html target/web/index.html \
        --out web/nginx/security-headers.conf --check

`--check` fails if the generated file is missing a hash for a current inline
script (the same contract as site/scripts/csp.mjs for the marketing site).
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEFAULT_TEMPLATE = HERE / "security-headers.conf.in"

# <script ...>body</script>, non-greedy.
INLINE_SCRIPT = re.compile(r"<script\b([^>]*)>(.*?)</script>", re.IGNORECASE | re.DOTALL)
# A tag with a src loads a file; CSP hashes only cover inline bodies.
HAS_SRC = re.compile(r"\bsrc\s*=", re.IGNORECASE)
# Only the Content-Security-Policy line is whitespace-tidied.
CSP_LINE = re.compile(r"^.*Content-Security-Policy.*$", re.MULTILINE)


def csp_hashes(html: str) -> list[str]:
    """Return one hash per non-empty inline <script> in document order."""
    hashes: list[str] = []
    for match in INLINE_SCRIPT.finditer(html):
        attrs, body = match.group(1), match.group(2)
        if HAS_SRC.search(attrs) or not body.strip():
            continue
        digest = base64.b64encode(hashlib.sha256(body.encode("utf-8")).digest()).decode("ascii")
        hashes.append("'sha256-" + digest + "'")
    return hashes


def tidy_csp(line: str) -> str:
    """Collapse the double spaces and space-before-semicolon a missing token leaves."""
    line = re.sub(r"[ \t]{2,}", " ", line)
    return line.replace(" ;", ";")


def render(template: str, hashes: list[str]) -> str:
    out = template.replace("__SCRIPT_HASHES__", " ".join(hashes))
    return CSP_LINE.sub(lambda m: tidy_csp(m.group(0)), out)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--html", required=True, help="built index.html")
    parser.add_argument("--out", required=True, help="nginx snippet to write or check")
    parser.add_argument("--template", type=Path, default=DEFAULT_TEMPLATE)
    parser.add_argument("--check", action="store_true", help="verify --out already matches")
    args = parser.parse_args(argv)

    html = Path(args.html).read_text(encoding="utf-8")
    hashes = csp_hashes(html)
    generated = render(args.template.read_text(encoding="utf-8"), hashes)
    out = Path(args.out)

    if args.check:
        if not out.is_file():
            print(f"{out}: missing", file=sys.stderr)
            return 1
        current = out.read_text(encoding="utf-8")
        missing = [h for h in hashes if h not in current]
        if missing:
            print(f"{out}: stale, missing {' '.join(missing)}", file=sys.stderr)
            return 1
        print(f"{out}: {len(hashes)} inline script hash(es) match")
        return 0

    out.write_text(generated, encoding="utf-8")
    print(f"{out}: {len(hashes)} inline script hash(es)")
    if not hashes:
        print("warning: index.html has no inline script; the CSP has no script hash", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
