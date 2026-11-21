#!/bin/sh
# Offline fixture test for scripts/ci/apt-install.sh - fake apt-get, no network,
# no sudo. Run from anywhere: sh scripts/tests/apt-retry.sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
HELPER="$ROOT/scripts/ci/apt-install.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/muninn-apt-retry-test.XXXXXX")
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

# The fake records every subcommand and fails the first N calls of each kind.
FAKEBIN="$TMP/fakebin"
mkdir -p "$FAKEBIN"
cat > "$FAKEBIN/apt-get" <<'EOF'
#!/bin/sh
printf '%s\n' "$1" >> "$MUNINN_FAKE_LOG"
printf 'fake apt-get: %s invoked\n' "$1" >&2
calls=$(grep -c "^$1\$" "$MUNINN_FAKE_LOG") || calls=0
case "$1" in
  update) limit=${MUNINN_FAKE_UPDATE_FAIL:-0} ;;
  install) limit=${MUNINN_FAKE_INSTALL_FAIL:-0} ;;
  *) limit=0 ;;
esac
if [ "$calls" -le "$limit" ]; then
  printf 'fake apt-get: simulated %s failure %s/%s\n' "$1" "$calls" "$limit" >&2
  exit 1
fi
printf 'fake apt-get: %s succeeded\n' "$1" >&2
exit 0
EOF
chmod 755 "$FAKEBIN/apt-get"

calls() {
  n=$(grep -c "^$1\$" "$2" 2>/dev/null) || n=0
  printf '%s' "$n"
}

# --- transient install failure: two failures, success on attempt 3 -----------
LOG="$TMP/install.log"; OUT="$TMP/install.out"; : > "$LOG"
if MUNINN_APT_GET="$FAKEBIN/apt-get" MUNINN_FAKE_LOG="$LOG" \
     MUNINN_FAKE_INSTALL_FAIL=2 MUNINN_APT_ATTEMPTS=3 MUNINN_APT_BACKOFF=0 \
     sh "$HELPER" pkg-config libgl1-mesa-dev > "$OUT" 2>&1; then
  :
else
  cat "$OUT" >&2
  fail "transient install failures were not retried"
fi
[ "$(calls install "$LOG")" -eq 3 ] ||
  fail "expected exactly 3 install attempts, got $(calls install "$LOG")"
grep -q 'attempt 1/3' "$OUT" || fail "attempt 1 was not logged"
grep -q 'attempt 3/3' "$OUT" || fail "attempt 3 was not logged"
grep -q 'simulated install failure' "$OUT" || fail "failure reason was not logged"
grep -q 'apt-get install failed (exit 1)' "$OUT" || fail "install exit status was not logged"
pass "two transient install failures retry and succeed on attempt 3"

# --- transient update failure: two failures, then install -------------------
LOG="$TMP/update.log"; OUT="$TMP/update.out"; : > "$LOG"
if MUNINN_APT_GET="$FAKEBIN/apt-get" MUNINN_FAKE_LOG="$LOG" \
     MUNINN_FAKE_UPDATE_FAIL=2 MUNINN_APT_ATTEMPTS=3 MUNINN_APT_BACKOFF=0 \
     sh "$HELPER" pkg-config > "$OUT" 2>&1; then
  :
else
  cat "$OUT" >&2
  fail "transient update failures were not retried"
fi
[ "$(calls update "$LOG")" -eq 3 ] ||
  fail "expected exactly 3 update attempts, got $(calls update "$LOG")"
grep -q 'apt-get update failed (exit 1)' "$OUT" || fail "update failure reason missing"
pass "two transient update failures retry before installing"

# --- persistent failure: bounded attempts and non-zero exit -----------------
LOG="$TMP/fatal.log"; OUT="$TMP/fatal.out"; : > "$LOG"
if MUNINN_APT_GET="$FAKEBIN/apt-get" MUNINN_FAKE_LOG="$LOG" \
     MUNINN_FAKE_UPDATE_FAIL=99 MUNINN_FAKE_INSTALL_FAIL=99 \
     MUNINN_APT_ATTEMPTS=2 MUNINN_APT_BACKOFF=0 \
     sh "$HELPER" pkg-config > "$OUT" 2>&1; then
  cat "$OUT" >&2
  fail "persistent failure was swallowed"
fi
[ "$(calls update "$LOG")" -eq 2 ] ||
  fail "expected exactly 2 attempts, got $(calls update "$LOG")"
grep -q 'all 2 attempt(s) failed' "$OUT" || fail "missing final failure message"
pass "persistent failure exits non-zero after exactly MUNINN_APT_ATTEMPTS attempts"

# --- invalid attempt counts are rejected before apt-get runs ----------------
for bad in 0 abc; do
  LOG="$TMP/invalid-$bad.log"; OUT="$TMP/invalid-$bad.out"; : > "$LOG"
  if MUNINN_APT_GET="$FAKEBIN/apt-get" MUNINN_FAKE_LOG="$LOG" \
       MUNINN_APT_ATTEMPTS="$bad" MUNINN_APT_BACKOFF=0 \
       sh "$HELPER" pkg-config > "$OUT" 2>&1; then
    cat "$OUT" >&2
    fail "MUNINN_APT_ATTEMPTS=$bad was accepted"
  fi
  grep -q 'MUNINN_APT_ATTEMPTS' "$OUT" || fail "unclear error for '$bad'"
  [ "$(calls update "$LOG")" -eq 0 ] || fail "apt-get ran with MUNINN_APT_ATTEMPTS=$bad"
done
pass "zero and non-numeric MUNINN_APT_ATTEMPTS are rejected clearly"

printf 'All apt retry fixture tests passed.\n'
