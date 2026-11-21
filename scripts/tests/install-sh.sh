#!/bin/sh
# Fixture test for install.sh - all assets are local, no network is used.
# Run from anywhere: sh scripts/tests/install-sh.sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
INSTALLER="$ROOT/install.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/muninn-install-test.XXXXXX")
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

TAG=v0.0.0
RELEASES="$TMP/releases"
GOOD="$RELEASES/$TAG"
STAGE="$TMP/stage/muninn_$TAG"
PLATFORMS="linux_x86_64 linux_aarch64 macos_aarch64"

# --- build tiny fake release assets ------------------------------------------
mkdir -p "$STAGE/assets" "$GOOD"
cat > "$STAGE/muninn" <<'EOF'
#!/bin/sh
printf 'muninn 0.0.0\n'
EOF
chmod 755 "$STAGE/muninn"
printf '[Desktop Entry]\nName=Muninn\nExec=muninn\n' > "$STAGE/assets/muninn.desktop"
printf 'fake png payload\n' > "$STAGE/assets/logo.png"
printf '<svg/>\n' > "$STAGE/assets/logo.svg"

for platform in $PLATFORMS; do
  name="muninn_${TAG}_${platform}.tar.gz"
  tar -czf "$GOOD/$name" -C "$TMP/stage" "muninn_$TAG"
done
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$GOOD" && sha256sum *.tar.gz > checksums.txt)
else
  (cd "$GOOD" && shasum -a 256 *.tar.gz > checksums.txt)
fi

# A PATH with only the tools install.sh needs: cosign and gh are deliberately
# absent so default-mode signature behaviour is deterministic.
SHIM="$TMP/shim"
mkdir -p "$SHIM"
for tool in sh uname mktemp rm rmdir curl sed head awk grep find tar gzip mkdir cp chmod dirname basename sort cat sha256sum shasum; do
  location=$(command -v "$tool" 2>/dev/null) || continue
  ln -s "$location" "$SHIM/$tool"
done

# A gh that is installed but unusable (not logged in), to exercise the
# "verifier present but failing" path without touching a real gh config.
FAKEBIN="$TMP/fakebin"
mkdir -p "$FAKEBIN"
cat > "$FAKEBIN/gh" <<'EOF'
#!/bin/sh
printf 'gh: not logged in; run gh auth login\n' >&2
exit 1
EOF
chmod 755 "$FAKEBIN/gh"

# --- install ------------------------------------------------------------------
PREFIX="$TMP/prefix"
MUNINN_INSTALL_BASE_URL="file://$RELEASES" sh "$INSTALLER" \
  --version "$TAG" --prefix "$PREFIX" --no-verify > "$TMP/install.out" 2>&1 ||
  { cat "$TMP/install.out" >&2; fail "install failed"; }
[ -x "$PREFIX/bin/muninn" ] || fail "bin/muninn was not installed"
[ "$("$PREFIX/bin/muninn")" = "muninn 0.0.0" ] || fail "installed binary did not run"
[ -f "$PREFIX/share/muninn/install-receipt" ] || fail "install receipt missing"
grep -Fqx "$PREFIX/bin/muninn" "$PREFIX/share/muninn/install-receipt" ||
  fail "receipt does not list the binary"
case "$(uname -s)" in
  Linux)
    [ -f "$PREFIX/share/applications/muninn.desktop" ] || fail "desktop file missing"
    [ -f "$PREFIX/share/icons/hicolor/512x512/apps/muninn.png" ] || fail "PNG icon missing"
    [ -f "$PREFIX/share/icons/hicolor/scalable/apps/muninn.svg" ] || fail "SVG icon missing"
    ;;
esac
pass "install, receipt and binary"

# --- default mode: checksum verified, signature warning ----------------------
DEFAULT_PREFIX="$TMP/prefix-default"
PATH="$SHIM" MUNINN_INSTALL_BASE_URL="file://$RELEASES" sh "$INSTALLER" \
  --version "$TAG" --prefix "$DEFAULT_PREFIX" > "$TMP/default.out" 2>&1 ||
  { cat "$TMP/default.out" >&2; fail "default-mode install failed"; }
grep -q 'SHA-256 checksum verified' "$TMP/default.out" || fail "checksum was not verified by default"
grep -qi 'signature was NOT checked' "$TMP/default.out" || fail "missing unsigned-build warning"
[ -x "$DEFAULT_PREFIX/bin/muninn" ] || fail "default-mode binary missing"
pass "default mode verifies checksum and warns about the unchecked signature"

# --- default mode with an unusable gh still installs --------------------------
GH_PREFIX="$TMP/prefix-gh-fail"
PATH="$FAKEBIN:$SHIM" MUNINN_INSTALL_BASE_URL="file://$RELEASES" sh "$INSTALLER" \
  --version "$TAG" --prefix "$GH_PREFIX" > "$TMP/gh-fail.out" 2>&1 ||
  { cat "$TMP/gh-fail.out" >&2; fail "default mode aborted because gh was unusable"; }
grep -q 'SHA-256 checksum verified' "$TMP/gh-fail.out" || fail "checksum was not verified in gh-failure case"
grep -qi 'signature verification failed' "$TMP/gh-fail.out" || fail "missing signature-failure warning"
grep -qi 'checksum passed' "$TMP/gh-fail.out" || fail "warning did not say the checksum passed"
grep -q 'gh auth login' "$TMP/gh-fail.out" || fail "verifier stderr was swallowed"
[ -x "$GH_PREFIX/bin/muninn" ] || fail "gh-failure install did not write a binary"
pass "unusable gh in default mode warns but installs"

# --- --require-signature with an unusable gh is fatal -------------------------
if PATH="$FAKEBIN:$SHIM" MUNINN_INSTALL_BASE_URL="file://$RELEASES" sh "$INSTALLER" \
  --version "$TAG" --prefix "$TMP/prefix-gh-req" --require-signature > "$TMP/gh-req.out" 2>&1; then
  cat "$TMP/gh-req.out" >&2
  fail "--require-signature succeeded with an unusable gh"
fi
grep -q -- '--require-signature' "$TMP/gh-req.out" || fail "missing --require-signature error"
grep -qi 'signature verification' "$TMP/gh-req.out" || fail "error did not mention signature verification"
grep -q 'gh auth login' "$TMP/gh-req.out" || fail "verifier stderr was swallowed in fatal case"
pass "--require-signature fails with an unusable gh"

# --- --no-verify does not fetch checksums.txt ---------------------------------
NOCHECKSUMS="$TMP/nochecksums/$TAG"
mkdir -p "$NOCHECKSUMS"
for platform in $PLATFORMS; do
  cp "$GOOD/muninn_${TAG}_${platform}.tar.gz" "$NOCHECKSUMS/"
done
[ ! -e "$NOCHECKSUMS/checksums.txt" ] || fail "fixture accidentally has checksums.txt"
NOCHECKS_PREFIX="$TMP/prefix-nochecksums"
MUNINN_INSTALL_BASE_URL="file://$TMP/nochecksums" sh "$INSTALLER" \
  --version "$TAG" --prefix "$NOCHECKS_PREFIX" --no-verify > "$TMP/nochecksums.out" 2>&1 ||
  { cat "$TMP/nochecksums.out" >&2; fail "--no-verify required checksums.txt"; }
[ -x "$NOCHECKS_PREFIX/bin/muninn" ] || fail "no-checksums install did not write a binary"
pass "--no-verify installs without checksums.txt"

# --- tampered checksums are rejected -----------------------------------------
BAD="$TMP/bad/$TAG"
mkdir -p "$BAD"
for platform in $PLATFORMS; do
  cp "$GOOD/muninn_${TAG}_${platform}.tar.gz" "$BAD/"
  printf '%064d  muninn_%s_%s.tar.gz\n' 0 "$TAG" "$platform"
done > "$BAD/checksums.txt"
BAD_PREFIX="$TMP/prefix-bad"
if MUNINN_INSTALL_BASE_URL="file://$TMP/bad" sh "$INSTALLER" \
  --version "$TAG" --prefix "$BAD_PREFIX" > "$TMP/bad.out" 2>&1; then
  cat "$TMP/bad.out" >&2
  fail "tampered checksums were accepted"
fi
grep -q 'SHA-256 mismatch' "$TMP/bad.out" || fail "expected an explicit SHA-256 mismatch error"
[ ! -e "$BAD_PREFIX/bin/muninn" ] || fail "tampered install wrote a binary"
pass "tampered checksums rejected"

# --- --require-signature without a verifier ----------------------------------
if PATH="$SHIM" MUNINN_INSTALL_BASE_URL="file://$RELEASES" sh "$INSTALLER" \
  --version "$TAG" --prefix "$TMP/prefix-req" --require-signature > "$TMP/req.out" 2>&1; then
  cat "$TMP/req.out" >&2
  fail "--require-signature succeeded without cosign or gh"
fi
grep -q -- '--require-signature' "$TMP/req.out" || fail "missing --require-signature error"
pass "--require-signature fails without a verifier"

# --- uninstall leaves a fake config and vault untouched ----------------------
mkdir -p "$PREFIX/.config/muninn" "$PREFIX/vault"
printf '{"theme":"dark"}\n' > "$PREFIX/.config/muninn/settings.json"
printf '# Hello\n' > "$PREFIX/vault/Note.md"

sh "$INSTALLER" --prefix "$PREFIX" --uninstall > "$TMP/uninstall.out" 2>&1 ||
  { cat "$TMP/uninstall.out" >&2; fail "uninstall failed"; }
[ ! -e "$PREFIX/bin/muninn" ] || fail "binary was not removed"
[ ! -e "$PREFIX/share/muninn/install-receipt" ] || fail "receipt was not removed"
[ ! -d "$PREFIX/bin" ] || fail "empty bin directory was not pruned"
[ ! -d "$PREFIX/share/muninn" ] || fail "empty receipt directory was not pruned"
case "$(uname -s)" in
  Linux)
    [ ! -e "$PREFIX/share/applications/muninn.desktop" ] || fail "desktop file was not removed"
    [ ! -e "$PREFIX/share/icons/hicolor/512x512/apps/muninn.png" ] || fail "PNG icon was not removed"
    [ ! -e "$PREFIX/share/icons/hicolor/scalable/apps/muninn.svg" ] || fail "SVG icon was not removed"
    [ ! -d "$PREFIX/share/applications" ] || fail "empty applications directory was not pruned"
    [ ! -d "$PREFIX/share/icons" ] || fail "empty icons directory was not pruned"
    ;;
esac
[ -f "$PREFIX/.config/muninn/settings.json" ] || fail "config file was removed"
[ -f "$PREFIX/vault/Note.md" ] || fail "vault was removed"
if sh "$INSTALLER" --prefix "$PREFIX" --uninstall > "$TMP/uninstall2.out" 2>&1; then
  fail "repeated uninstall should report a missing receipt"
fi
pass "uninstall removes only receipt-listed files"

printf 'All install.sh fixture tests passed.\n'
