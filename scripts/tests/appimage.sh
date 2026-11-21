#!/bin/sh
# Offline fixture test for scripts/build-appimage.sh.
# Uses fake appimagetool/runtime via MUNINN_APPIMAGETOOL and
# MUNINN_APPIMAGE_RUNTIME: no network, no FUSE.
# Run from anywhere: sh scripts/tests/appimage.sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
BUILD="$ROOT/scripts/build-appimage.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/muninn-appimage-test.XXXXXX")
cleanup() {
  if [ -n "$TMP" ] && [ -d "$TMP" ]; then
    rm -rf "$TMP"
  fi
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
pass() { printf 'ok - %s\n' "$*"; }

# --- fake appimagetool: log argv, snapshot the AppDir, create the output -----
FAKEBIN="$TMP/fakebin"
mkdir -p "$FAKEBIN"
cat > "$FAKEBIN/appimagetool" <<'EOF'
#!/bin/sh
set -eu
printf 'ARCH=%s\n' "${ARCH:-}" >> "$MUNINN_FAKE_LOG"
printf 'ARGS' >> "$MUNINN_FAKE_LOG"
for a in "$@"; do printf '\t%s' "$a" >> "$MUNINN_FAKE_LOG"; done
printf '\n' >> "$MUNINN_FAKE_LOG"

src= dest= runtime=
while [ "$#" -gt 0 ]; do
  case "$1" in
    --no-appstream|-n) shift ;;
    --runtime-file)
      [ "$#" -ge 2 ] || { printf 'fake appimagetool: --runtime-file without value\n' >&2; exit 1; }
      runtime=$2; shift 2 ;;
    -*) shift ;;
    *) if [ -z "$src" ]; then src=$1; elif [ -z "$dest" ]; then dest=$1; fi; shift ;;
  esac
done
[ -n "$src" ]  || { printf 'fake appimagetool: no SOURCE\n' >&2; exit 1; }
[ -n "$dest" ] || { printf 'fake appimagetool: no DESTINATION\n' >&2; exit 1; }
[ -n "$runtime" ] || { printf 'fake appimagetool: --runtime-file was not passed\n' >&2; exit 1; }
[ -f "$runtime" ] || { printf 'fake appimagetool: runtime file missing: %s\n' "$runtime" >&2; exit 1; }
printf 'RUNTIME=%s\n' "$runtime" >> "$MUNINN_FAKE_LOG"

# Leave a copy of the AppDir behind for the parent test to inspect.
rm -rf "$MUNINN_FAKE_APPDIR_COPY"
cp -R "$src" "$MUNINN_FAKE_APPDIR_COPY"
printf 'fake AppImage payload\n' > "$dest"
printf 'fake appimagetool: wrote %s\n' "$dest" >&2
EOF
chmod 755 "$FAKEBIN/appimagetool"

# A fake release binary with a recognisable payload.
BIN="$TMP/muninn-bin"
cat > "$BIN" <<'EOF'
#!/bin/sh
printf 'muninn 0.0.0\n'
EOF
chmod 755 "$BIN"

LOG="$TMP/fake.log"
COPY="$TMP/appdir-copy"
CACHE="$TMP/cache"
OUT="$TMP/out"
RUNTIME_FIXTURE="$TMP/runtime-fixture"
printf 'fake type-2 runtime payload\n' > "$RUNTIME_FIXTURE"
: > "$LOG"

run_build() {
  MUNINN_APPIMAGETOOL="$FAKEBIN/appimagetool" \
  MUNINN_APPIMAGE_RUNTIME="$RUNTIME_FIXTURE" \
  MUNINN_FAKE_LOG="$LOG" \
  MUNINN_FAKE_APPDIR_COPY="$COPY" \
  MUNINN_APPIMAGE_CACHE="$CACHE" \
    sh "$BUILD" "$@"
}

# --- happy path ---------------------------------------------------------------
TAG=v0.0.0-ci
run_build --binary "$BIN" --tag "$TAG" --arch x86_64 --output "$OUT" \
  > "$TMP/build.out" 2>&1 || { cat "$TMP/build.out" >&2; fail "build failed"; }

expected="muninn_v0.0.0-ci_linux_x86_64.AppImage"
[ -f "$OUT/$expected" ] || fail "expected output $OUT/$expected was not created"
grep -q "$OUT/$expected" "$TMP/build.out" || fail "final path was not printed"

# The fake tool recorded the invocation and the selected arch.
grep -q '^ARCH=x86_64$' "$LOG" || fail "appimagetool did not receive ARCH=x86_64"
grep -q -- '--no-appstream' "$LOG" || fail "appimagetool was not called with --no-appstream"
grep -q -- '--runtime-file' "$LOG" || fail "appimagetool was not called with --runtime-file"
grep -Fq "RUNTIME=$RUNTIME_FIXTURE" "$LOG" || fail "appimagetool was not given the pinned runtime override"
grep -q "$expected" "$LOG" || fail "appimagetool was not given the normalised output name"

# AppDir layout.
[ -x "$COPY/usr/bin/muninn" ] || fail "usr/bin/muninn missing or not executable"
[ -f "$COPY/usr/share/applications/muninn.desktop" ] || fail "desktop file missing"
[ -f "$COPY/usr/share/icons/hicolor/512x512/apps/muninn.png" ] || fail "512x512 icon missing"
[ -f "$COPY/usr/share/licenses/muninn/LICENSE" ] || fail "LICENSE missing"
[ -x "$COPY/AppRun" ] || fail "AppRun missing or not executable"
[ -f "$COPY/muninn.desktop" ] || fail "root muninn.desktop missing"
[ -f "$COPY/muninn.png" ] || fail "root muninn.png missing"
[ -e "$COPY/.DirIcon" ] || fail "root .DirIcon missing"

# Contents of the copied binary, desktop file, license and AppRun.
cmp -s "$BIN" "$COPY/usr/bin/muninn" || fail "usr/bin/muninn is not a copy of the binary"
cmp -s "$ROOT/assets/muninn.desktop" "$COPY/usr/share/applications/muninn.desktop" ||
  fail "desktop file is not a copy of assets/muninn.desktop"
cmp -s "$ROOT/LICENSE" "$COPY/usr/share/licenses/muninn/LICENSE" || fail "LICENSE was not copied"
grep -q 'usr/bin' "$COPY/AppRun" || fail "AppRun does not prepend usr/bin to PATH"
grep -q 'exec muninn "\$@"' "$COPY/AppRun" || fail "AppRun does not exec muninn with arguments"
pass "AppDir layout, invocation and output filename"

# --- bad arch is rejected -----------------------------------------------------
BAD_OUT="$TMP/bad-arch-out"
if run_build --binary "$BIN" --tag "$TAG" --arch riscv64 --output "$BAD_OUT" \
    > "$TMP/bad-arch.out" 2>&1; then
  cat "$TMP/bad-arch.out" >&2
  fail "unsupported arch was accepted"
fi
grep -q 'unsupported --arch' "$TMP/bad-arch.out" || fail "bad arch error was unclear"
[ ! -d "$BAD_OUT" ] || fail "bad arch created an output directory"
pass "unsupported arch fails clearly"

# --- missing binary is rejected ----------------------------------------------
if run_build --binary "$TMP/no-such-binary" --tag "$TAG" --arch x86_64 --output "$TMP/missing-out" \
    > "$TMP/missing.out" 2>&1; then
  cat "$TMP/missing.out" >&2
  fail "missing binary was accepted"
fi
grep -q 'binary not found' "$TMP/missing.out" || fail "missing binary error was unclear"
pass "missing binary fails clearly"

printf 'All AppImage fixture tests passed.\n'
