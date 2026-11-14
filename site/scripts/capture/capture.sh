#!/usr/bin/env bash
# Re-record Muninn's marketing stills and loops from a release binary.
#
#   site/scripts/capture/capture.sh WORKDIR [TAG]
#
#   WORKDIR   throwaway directory (release, runtime, run dirs, masters, assets)
#   TAG       release tag, default v0.8.2
#
# Env:
#   MUNINN_BIN=/path/muninn    use this binary instead of downloading TAG
#   CAPTURE_DISPLAY=:93        private Xvfb display (must be free)
#   CAPTURE_FETCH_RUNTIME=1    when the host glibc is older than the binary needs
#                              (the release links GLIBC_2.43) or has no lavapipe,
#                              unpack Debian's libc6 and mesa-vulkan-drivers into
#                              WORKDIR/tools (user level, no root, nothing installed)
#   CAPTURE_FETCH_ONLY=1       fetch and verify the runtime .debs, then exit
#   CAPTURE_SKIP_SIGSTORE=1    skip the mandatory sigstore check of checksums.txt
#   LIBC6_DEB_URL, MESA_VK_DEB_URL         override those package URLs (https only;
#                                          requires the matching *_SHA256)
#   LIBC6_DEB_SHA256, MESA_VK_DEB_SHA256   required when a URL is overridden
#
# Needs: gh, Xvfb, dbus-run-session, ffmpeg (libx264, libvpx-vp9), python3 with
# python-xlib and Pillow (WebP + AVIF), and `sigstore` (pip) unless MUNINN_BIN
# or CAPTURE_SKIP_SIGSTORE=1 is set. No secrets are read or written. Every
# process this starts is in its own session and is stopped by the scripts;
# nothing else is signalled.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
W="$(mkdir -p "$1" && cd "$1" && pwd)"
TAG="${2:-v0.8.2}"
VER="${TAG#v}"
PY="${PYTHON:-python3}"
mkdir -p "$W/rel" "$W/tools" "$W/run"

# Hashes are of the exact .debs used for the 0.8.2 captures, then confirmed
# against Debian's signed InRelease/Packages (libc6: testing; mesa: trixie).
LIBC6_DEB_DEFAULT_URL="https://deb.debian.org/debian/pool/main/g/glibc/libc6_2.43-7_amd64.deb"
LIBC6_DEB_DEFAULT_SHA256="5c71715c51103beb4fbba9fe9f8dff604af6610dfa71c70bff342f54a7860803"
LIBC6_DEB_DIST="testing"
MESA_VK_DEB_DEFAULT_URL="https://deb.debian.org/debian/pool/main/m/mesa/mesa-vulkan-drivers_25.0.7-2+deb13u1_amd64.deb"
MESA_VK_DEB_DEFAULT_SHA256="caf2f7d296b7522efe74e40a74bce762a3ce84c33b53b5e54e1248ac2e0a13a5"
MESA_VK_DEB_DIST="trixie"
DEBIAN_KEYRING="/usr/share/keyrings/debian-archive-keyring.gpg"

check_https() {
  case "$1" in
    https://*) ;;
    *) echo "FAIL: refusing non-https URL: $1" >&2; exit 1 ;;
  esac
}

# Resolve a package URL/hash pair. A URL override is refused unless the
# matching SHA256 env var is also set.
resolve_deb() {
  # name default_url default_sha url_env sha_env -> sets REPLY_URL REPLY_SHA
  local default_url="$2" default_sha="$3"
  local url_var="$4" sha_var="$5"
  local url="${!url_var:-}" sha="${!sha_var:-}"
  if [ -n "$url" ]; then
    if [ -z "$sha" ]; then
      echo "FAIL: $url_var is set but $sha_var is not; refusing the override" >&2
      exit 1
    fi
    REPLY_URL="$url"
    REPLY_SHA="$sha"
  else
    REPLY_URL="$default_url"
    REPLY_SHA="${sha:-$default_sha}"
  fi
  check_https "$REPLY_URL"
}

fetch_deb() {
  local dest="$1" url="$2" sha="$3"
  if [ -f "$dest" ]; then
    echo "$sha  $dest" | sha256sum -c --strict
    return
  fi
  curl -fsSL --proto '=https' --tlsv1.2 -o "$dest.part" "$url"
  echo "$sha  $dest.part" | sha256sum -c --strict
  mv "$dest.part" "$dest"
}

# Confirm the pinned sha256 is the one in Debian's signed Packages (best-effort:
# skipped if debian-archive-keyring is not installed).
verify_deb_against_debian() {
  local dist="$1" basename="$2" sha="$3"
  if [ ! -f "$DEBIAN_KEYRING" ]; then
    echo "note: $DEBIAN_KEYRING missing; skipped signed Packages check for $basename" >&2
    return 0
  fi
  if ! command -v gpg >/dev/null || ! command -v xz >/dev/null; then
    echo "note: gpg or xz missing; skipped signed Packages check for $basename" >&2
    return 0
  fi
  local meta="$W/tools/debian-meta/$dist"
  mkdir -p "$meta"
  if [ ! -f "$meta/Packages" ]; then
    curl -fsSL --proto '=https' --tlsv1.2 -o "$meta/InRelease" \
      "https://deb.debian.org/debian/dists/${dist}/InRelease"
    local status="$meta/gpg.status"
    if ! gpg --no-default-keyring --keyring "$DEBIAN_KEYRING" --batch --status-fd 3 \
         --verify "$meta/InRelease" 3>"$status" 2>/dev/null; then
      echo "FAIL: gpg could not verify Debian $dist InRelease" >&2
      exit 1
    fi
    if ! grep -q '^\[GNUPG:\] VALIDSIG ' "$status"; then
      echo "FAIL: no VALIDSIG on Debian $dist InRelease" >&2
      exit 1
    fi
    local pkg_sha
    pkg_sha="$("$PY" - "$meta/InRelease" <<'PY'
import sys
from pathlib import Path
lines = Path(sys.argv[1]).read_text().splitlines()
i = lines.index("SHA256:")
for line in lines[i + 1:]:
    if not line.startswith(" "):
        break
    h, _s, n = line.split()
    if n == "main/binary-amd64/Packages.xz":
        print(h)
        break
else:
    raise SystemExit("no Packages.xz SHA256 in InRelease")
PY
)"
    curl -fsSL --proto '=https' --tlsv1.2 -o "$meta/Packages.xz" \
      "https://deb.debian.org/debian/dists/${dist}/main/binary-amd64/Packages.xz"
    echo "$pkg_sha  $meta/Packages.xz" | sha256sum -c --strict
    xz -dkc "$meta/Packages.xz" > "$meta/Packages"
  fi
  if ! "$PY" - "$meta/Packages" "$basename" "$sha" <<'PY'
import sys
from pathlib import Path
packages, basename, want = sys.argv[1], sys.argv[2], sys.argv[3]
cur = {}
for line in Path(packages).read_text(errors="replace").splitlines() + [""]:
    if line == "":
        fn = (cur.get("Filename") or "").rsplit("/", 1)[-1]
        if fn == basename:
            got = cur.get("SHA256", "")
            if got != want:
                raise SystemExit(f"Debian Packages SHA256 for {basename} is {got}, expected {want}")
            print(f"ok: {basename} SHA256 matches signed {sys.argv[1]}")
            raise SystemExit(0)
        cur = {}
        continue
    if line.startswith(" ") or ": " not in line:
        continue
    k, v = line.split(": ", 1)
    cur[k] = v
raise SystemExit(f"{basename} not found in signed Packages")
PY
  then
    echo "FAIL: Debian Packages check for $basename" >&2
    exit 1
  fi
}

if [ -z "${MUNINN_BIN:-}" ]; then
  gh release download "$TAG" -R raythurman2386/muninn -D "$W/rel" --clobber \
    -p "muninn_${TAG}_linux_x86_64.tar.gz" -p checksums.txt -p checksums.txt.sigstore.json
  (cd "$W/rel" && sha256sum -c --ignore-missing checksums.txt)
  if [ "${CAPTURE_SKIP_SIGSTORE:-0}" = 1 ]; then
    echo "note: CAPTURE_SKIP_SIGSTORE=1; cosign bundle not verified" >&2
  else
    SIGSTORE_BIN="${SIGSTORE:-sigstore}"
    if ! command -v "$SIGSTORE_BIN" >/dev/null && [ "$SIGSTORE_BIN" = sigstore ]; then
      echo "FAIL: sigstore is required to verify checksums.txt (install with pip, or set CAPTURE_SKIP_SIGSTORE=1, or pass MUNINN_BIN)" >&2
      exit 1
    fi
    (cd "$W/rel" && "$SIGSTORE_BIN" verify identity \
      --bundle checksums.txt.sigstore.json \
      --cert-identity "https://github.com/raythurman2386/muninn/.github/workflows/release.yml@refs/tags/$TAG" \
      --cert-oidc-issuer "https://token.actions.githubusercontent.com" \
      checksums.txt)
  fi
  tar -C "$W/rel" -xzf "$W/rel/muninn_${TAG}_linux_x86_64.tar.gz"
  MUNINN_BIN="$W/rel/muninn_${TAG}_linux_x86_64/muninn"
fi
if [ "${CAPTURE_FETCH_ONLY:-0}" != 1 ]; then
  sha256sum "$MUNINN_BIN"
fi

LOADER_ARGS=() ICD_ARGS=()
if [ "${CAPTURE_FETCH_RUNTIME:-0}" = 1 ]; then
  R="$W/tools/root"; L="$R/usr/lib/x86_64-linux-gnu"
  resolve_deb libc6 "$LIBC6_DEB_DEFAULT_URL" "$LIBC6_DEB_DEFAULT_SHA256" LIBC6_DEB_URL LIBC6_DEB_SHA256
  libc6_url="$REPLY_URL" libc6_sha="$REPLY_SHA"
  resolve_deb mesa "$MESA_VK_DEB_DEFAULT_URL" "$MESA_VK_DEB_DEFAULT_SHA256" MESA_VK_DEB_URL MESA_VK_DEB_SHA256
  mesa_url="$REPLY_URL" mesa_sha="$REPLY_SHA"

  if [ ! -e "$L/ld-linux-x86-64.so.2" ]; then
    fetch_deb "$W/tools/libc6.deb" "$libc6_url" "$libc6_sha"
    if [ -z "${LIBC6_DEB_URL:-}" ]; then
      verify_deb_against_debian "$LIBC6_DEB_DIST" "libc6_2.43-7_amd64.deb" "$libc6_sha"
    fi
    dpkg-deb -x "$W/tools/libc6.deb" "$R"
  fi
  if [ ! -e "$L/libvulkan_lvp.so" ]; then
    fetch_deb "$W/tools/mesa-vk.deb" "$mesa_url" "$mesa_sha"
    if [ -z "${MESA_VK_DEB_URL:-}" ]; then
      verify_deb_against_debian "$MESA_VK_DEB_DIST" "mesa-vulkan-drivers_25.0.7-2+deb13u1_amd64.deb" "$mesa_sha"
    fi
    dpkg-deb -x "$W/tools/mesa-vk.deb" "$R"
  fi
  "$PY" - "$L/libvulkan_lvp.so" "$W/tools/lvp_icd.json" <<'PY'
import json, sys
json.dump({"ICD": {"api_version": "1.4.305", "library_path": sys.argv[1]}, "file_format_version": "1.0.0"},
          open(sys.argv[2], "w"), indent=2)
PY
  LOADER_ARGS=(--loader "$L/ld-linux-x86-64.so.2" --libdir "$L")
  ICD_ARGS=(--vk-icd "$W/tools/lvp_icd.json")
else
  for icd in /usr/share/vulkan/icd.d/lvp_icd*.json; do [ -f "$icd" ] && ICD_ARGS=(--vk-icd "$icd") && break; done
fi

if [ "${CAPTURE_FETCH_ONLY:-0}" = 1 ]; then
  echo "fetch-only: runtime ready under $W/tools"
  exit 0
fi

"$PY" "$HERE/capture.py" --bin "$MUNINN_BIN" --out "$W" --run-dir "$W/run" \
  --display "${CAPTURE_DISPLAY:-:93}" "${LOADER_ARGS[@]}" "${ICD_ARGS[@]}"
"$PY" "$HERE/encode.py" --masters "$W/masters" --out "$W/assets" --version "$VER"
"$PY" "$HERE/contact_sheet.py" "$W/assets" "$W/assets/contact-sheet.png"

if pgrep -f -- "--vault $W/run/" >/dev/null; then
  echo "FAIL: a capture app is still running" >&2; exit 1
fi
echo "done: $W/assets (manifest.json, contact-sheet.png)"
