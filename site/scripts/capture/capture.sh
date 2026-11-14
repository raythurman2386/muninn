#!/usr/bin/env bash
# Re-record Muninn's marketing stills and loops from a release binary.
#
#   site/scripts/capture/capture.sh WORKDIR [TAG]
#
#   WORKDIR   throwaway directory (release, runtime, run dirs, masters, assets)
#   TAG       release tag, default v0.8.2
#
# Env:
#   MUNINN_BIN=/path/muninn   use this binary instead of downloading TAG
#   CAPTURE_DISPLAY=:93       private Xvfb display (must be free)
#   CAPTURE_FETCH_RUNTIME=1   when the host glibc is older than the binary needs
#                             (the release links GLIBC_2.43) or has no lavapipe,
#                             unpack Debian's libc6 and mesa-vulkan-drivers into
#                             WORKDIR/tools (user level, no root, nothing installed)
#   LIBC6_DEB_URL, MESA_VK_DEB_URL   override those package URLs
#
# Needs: gh, Xvfb, dbus-run-session, ffmpeg (libx264, libvpx-vp9), python3 with
# python-xlib and Pillow (WebP + AVIF); optional `sigstore` (pip) to verify the
# cosign bundle. No secrets are read or written. Every process this starts is
# in its own session and is stopped by the scripts; nothing else is signalled.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
W="$(mkdir -p "$1" && cd "$1" && pwd)"
TAG="${2:-v0.8.2}"
VER="${TAG#v}"
PY="${PYTHON:-python3}"
mkdir -p "$W/rel" "$W/tools" "$W/run"

if [ -z "${MUNINN_BIN:-}" ]; then
  gh release download "$TAG" -R raythurman2386/muninn -D "$W/rel" --clobber \
    -p "muninn_${TAG}_linux_x86_64.tar.gz" -p checksums.txt -p checksums.txt.sigstore.json
  (cd "$W/rel" && sha256sum -c --ignore-missing checksums.txt)
  if command -v sigstore >/dev/null; then
    (cd "$W/rel" && sigstore verify github --bundle checksums.txt.sigstore.json \
      --repository raythurman2386/muninn --ref "refs/tags/$TAG" checksums.txt)
  else
    echo "note: sigstore not installed; cosign bundle not verified" >&2
  fi
  tar -C "$W/rel" -xzf "$W/rel/muninn_${TAG}_linux_x86_64.tar.gz"
  MUNINN_BIN="$W/rel/muninn_${TAG}_linux_x86_64/muninn"
fi
sha256sum "$MUNINN_BIN"

LOADER_ARGS=() ICD_ARGS=()
if [ "${CAPTURE_FETCH_RUNTIME:-0}" = 1 ]; then
  R="$W/tools/root"; L="$R/usr/lib/x86_64-linux-gnu"
  if [ ! -e "$L/ld-linux-x86-64.so.2" ]; then
    curl -fsSL -o "$W/tools/libc6.deb" "${LIBC6_DEB_URL:-http://deb.debian.org/debian/pool/main/g/glibc/libc6_2.43-7_amd64.deb}"
    dpkg-deb -x "$W/tools/libc6.deb" "$R"
  fi
  if [ ! -e "$L/libvulkan_lvp.so" ]; then
    curl -fsSL -o "$W/tools/mesa-vk.deb" "${MESA_VK_DEB_URL:-http://deb.debian.org/debian/pool/main/m/mesa/mesa-vulkan-drivers_25.0.7-2+deb13u1_amd64.deb}"
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

"$PY" "$HERE/capture.py" --bin "$MUNINN_BIN" --out "$W" --run-dir "$W/run" \
  --display "${CAPTURE_DISPLAY:-:93}" "${LOADER_ARGS[@]}" "${ICD_ARGS[@]}"
"$PY" "$HERE/encode.py" --masters "$W/masters" --out "$W/assets" --version "$VER"
"$PY" "$HERE/contact_sheet.py" "$W/assets" "$W/assets/contact-sheet.png"

if pgrep -f -- "--vault $W/run/" >/dev/null; then
  echo "FAIL: a capture app is still running" >&2; exit 1
fi
echo "done: $W/assets (manifest.json, contact-sheet.png)"
