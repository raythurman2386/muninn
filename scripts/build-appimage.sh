#!/bin/sh
# Build a single-file Linux AppImage from a Muninn release binary.
#
# Usage:
#   scripts/build-appimage.sh --binary PATH --tag TAG --arch x86_64|aarch64 --output DIR
#
# Both appimagetool and the type-2 runtime are pinned by version and verified
# SHA-256 below - no "continuous" artifact is downloaded. Set
# MUNINN_APPIMAGETOOL / MUNINN_APPIMAGE_RUNTIME to reuse existing files (offline
# fixture tests) and MUNINN_APPIMAGE_CACHE to move the cache.
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

# Pinned appimagetool 1.9.1, sha256 sums verified from the upstream release.
TOOL_VERSION=1.9.1
TOOL_X86_64_SHA=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
TOOL_AARCH64_SHA=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158

# Pinned type-2 runtime 20251108, also verified from the upstream release.
# Without this appimagetool fetches type2-runtime/continuous at build time.
RUNTIME_VERSION=20251108
RUNTIME_X86_64_SHA=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
RUNTIME_AARCH64_SHA=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444

fail() { printf 'build-appimage: %s\n' "$*" >&2; exit 1; }
usage() {
  printf 'Usage: %s --binary PATH --tag TAG --arch x86_64|aarch64 --output DIR\n' "$0" >&2
}

BINARY=
TAG=
ARCH=
OUTPUT=
while [ "$#" -gt 0 ]; do
  case "$1" in
    --binary) [ "$#" -ge 2 ] || { usage; exit 2; }; BINARY=$2; shift 2 ;;
    --tag)    [ "$#" -ge 2 ] || { usage; exit 2; }; TAG=$2;    shift 2 ;;
    --arch)   [ "$#" -ge 2 ] || { usage; exit 2; }; ARCH=$2;   shift 2 ;;
    --output) [ "$#" -ge 2 ] || { usage; exit 2; }; OUTPUT=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; usage; exit 2 ;;
  esac
done

[ -n "$BINARY" ] || { printf 'missing --binary\n' >&2; usage; exit 2; }
[ -n "$TAG" ]    || { printf 'missing --tag\n' >&2; usage; exit 2; }
[ -n "$ARCH" ]   || { printf 'missing --arch\n' >&2; usage; exit 2; }
[ -n "$OUTPUT" ] || { printf 'missing --output\n' >&2; usage; exit 2; }

case "$ARCH" in
  x86_64)
    TOOL_SHA=$TOOL_X86_64_SHA; TOOL_NAME="appimagetool-${ARCH}.AppImage"
    RUNTIME_SHA=$RUNTIME_X86_64_SHA; RUNTIME_NAME="runtime-${ARCH}" ;;
  aarch64)
    TOOL_SHA=$TOOL_AARCH64_SHA; TOOL_NAME="appimagetool-${ARCH}.AppImage"
    RUNTIME_SHA=$RUNTIME_AARCH64_SHA; RUNTIME_NAME="runtime-${ARCH}" ;;
  *) fail "unsupported --arch '$ARCH' (expected x86_64 or aarch64)" ;;
esac

[ -f "$BINARY" ] || fail "binary not found: $BINARY"
[ -x "$BINARY" ] || fail "binary is not executable: $BINARY"
for asset in assets/muninn.desktop assets/logo.png LICENSE; do
  [ -f "$ROOT/$asset" ] || fail "missing build asset: $ROOT/$asset"
done

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    fail "no sha256sum or shasum available to verify pinned AppImage artifacts"
  fi
}

# --- obtain and verify the pinned appimagetool and type-2 runtime -------------
CACHE=${MUNINN_APPIMAGE_CACHE:-$ROOT/target/appimage}
mkdir -p "$CACHE"

if [ -n "${MUNINN_APPIMAGETOOL:-}" ]; then
  TOOL=$MUNINN_APPIMAGETOOL
  [ -x "$TOOL" ] || fail "MUNINN_APPIMAGETOOL is not executable: $TOOL"
else
  TOOL="$CACHE/$TOOL_NAME"
  if [ ! -f "$TOOL" ]; then
    URL="https://github.com/AppImage/appimagetool/releases/download/${TOOL_VERSION}/${TOOL_NAME}"
    printf 'downloading %s\n' "$URL" >&2
    curl -fsSL -o "$TOOL.tmp.$$" "$URL" || { rm -f "$TOOL.tmp.$$"; fail "failed to download $URL"; }
    mv "$TOOL.tmp.$$" "$TOOL"
  fi
  actual=$(sha256_file "$TOOL")
  [ "$actual" = "$TOOL_SHA" ] || fail "SHA-256 mismatch for $TOOL: expected $TOOL_SHA, got $actual"
  chmod 755 "$TOOL"
fi

if [ -n "${MUNINN_APPIMAGE_RUNTIME:-}" ]; then
  RUNTIME=$MUNINN_APPIMAGE_RUNTIME
  [ -f "$RUNTIME" ] || fail "MUNINN_APPIMAGE_RUNTIME does not exist: $RUNTIME"
else
  RUNTIME="$CACHE/$RUNTIME_NAME"
  if [ ! -f "$RUNTIME" ]; then
    URL="https://github.com/AppImage/type2-runtime/releases/download/${RUNTIME_VERSION}/${RUNTIME_NAME}"
    printf 'downloading %s\n' "$URL" >&2
    curl -fsSL -o "$RUNTIME.tmp.$$" "$URL" || { rm -f "$RUNTIME.tmp.$$"; fail "failed to download $URL"; }
    mv "$RUNTIME.tmp.$$" "$RUNTIME"
  fi
  actual=$(sha256_file "$RUNTIME")
  [ "$actual" = "$RUNTIME_SHA" ] || fail "SHA-256 mismatch for $RUNTIME: expected $RUNTIME_SHA, got $actual"
  chmod 755 "$RUNTIME"
fi

# --- assemble the AppDir in a scratch dir under the tool cache ----------------
WORK=$(mktemp -d "$CACHE/appdir.XXXXXX")
cleanup() { [ -n "$WORK" ] && [ -d "$WORK" ] && rm -rf "$WORK"; }
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

APPDIR="$WORK/AppDir"
mkdir -p "$APPDIR/usr/bin" \
  "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/icons/hicolor/512x512/apps" \
  "$APPDIR/usr/share/licenses/muninn"

cp "$BINARY" "$APPDIR/usr/bin/muninn"
chmod 755 "$APPDIR/usr/bin/muninn"
cp "$ROOT/assets/muninn.desktop" "$APPDIR/usr/share/applications/muninn.desktop"
cp "$ROOT/assets/logo.png" "$APPDIR/usr/share/icons/hicolor/512x512/apps/muninn.png"
cp "$ROOT/LICENSE" "$APPDIR/usr/share/licenses/muninn/LICENSE"
cp "$ROOT/assets/muninn.desktop" "$APPDIR/muninn.desktop"
cp "$ROOT/assets/logo.png" "$APPDIR/muninn.png"
ln -s muninn.png "$APPDIR/.DirIcon"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
# Resolve our own directory so the AppImage works from any working directory.
APPDIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PATH="$APPDIR/usr/bin:$PATH"
export PATH
exec muninn "$@"
EOF
chmod 755 "$APPDIR/AppRun"

# --- invoke appimagetool and normalise the output name -----------------------
# Tags reach us as "v0.9.1"; the asset name carries a single "v" prefix.
version=${TAG#v}
version=${version#V}
name="muninn_v${version}_linux_${ARCH}.AppImage"

BUILD_OUT="$WORK/out"
mkdir -p "$BUILD_OUT"
printf 'using pinned type-2 runtime %s: %s\n' "$RUNTIME_VERSION" "$RUNTIME" >&2
( cd "$BUILD_OUT" && APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$ARCH" \
    "$TOOL" --no-appstream --runtime-file "$RUNTIME" \
    "$APPDIR" "$BUILD_OUT/$name" )

if [ ! -f "$BUILD_OUT/$name" ]; then
  # Some appimagetool builds ignore DEST; rename the single file they emit.
  produced=$(find "$BUILD_OUT" -maxdepth 1 -type f -name '*.AppImage')
  [ -n "$produced" ] || fail "appimagetool produced no AppImage"
  [ "$(printf '%s\n' "$produced" | wc -l | tr -d ' ')" -eq 1 ] ||
    fail "appimagetool produced more than one AppImage"
  mv "$produced" "$BUILD_OUT/$name"
fi

mkdir -p "$OUTPUT"
dest="$OUTPUT/$name"
mv "$BUILD_OUT/$name" "$dest"
printf '%s\n' "$dest"
