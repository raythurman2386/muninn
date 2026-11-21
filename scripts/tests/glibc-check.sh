#!/bin/sh
# Offline fixture test for scripts/ci/check-glibc.sh - a fake readelf prints
# sample version info, so no real binary or toolchain is needed.
# Run from anywhere: sh scripts/tests/glibc-check.sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
CHECK="$ROOT/scripts/ci/check-glibc.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/muninn-glibc-check-test.XXXXXX")
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

# The guard only checks that --binary exists; the fake readelf supplies the
# version info, so any regular file stands in for the release binary.
BINARY="$TMP/muninn"
printf 'not really an ELF\n' > "$BINARY"

FAKEBIN="$TMP/fakebin"
mkdir -p "$FAKEBIN"
cat > "$FAKEBIN/readelf" <<'EOF'
#!/bin/sh
if [ "${MUNINN_FAKE_READELF_FAIL:-0}" = 1 ]; then
  printf 'readelf: fake failure\n' >&2
  exit 1
fi
cat "$MUNINN_FAKE_READELF_OUT"
EOF
chmod 755 "$FAKEBIN/readelf"

run_check() {
  out=$1
  shift
  if "$@" > "$out" 2>&1; then
    status=0
  else
    status=$?
  fi
  printf '%s' "$status"
}

# --- version info fixtures ----------------------------------------------------
VV="$TMP/version-info"
cat > "$VV" <<'EOF'
Version needs section '.gnu.version_r' contains 2 entries:
 0x0010: Rev: 1  Flags: none  Index: 3  Cnt: 1  Name: libc.so.6
  0x0050:   Name: GLIBC_2.34  Flags: none  Version: 6
  0x0060:   Name: GLIBC_2.39  Flags: none  Version: 7
  0x0070:   Name: GLIBC_2.2.5  Flags: none  Version: 2
EOF

ABOVE="$TMP/version-info-above"
cat > "$ABOVE" <<'EOF'
Version needs section '.gnu.version_r' contains 1 entry:
  0x0050:   Name: GLIBC_2.34  Flags: none  Version: 6
  0x0060:   Name: GLIBC_2.43  Flags: none  Version: 7
EOF

EMPTY="$TMP/version-info-empty"
printf "Version symbols section '.gnu.version' contains 0 entries:\n" > "$EMPTY"

# --- at the policy passes -----------------------------------------------------
status=$(MUNINN_READELF="$FAKEBIN/readelf" MUNINN_FAKE_READELF_OUT="$VV" \
  run_check "$TMP/at.out" sh "$CHECK" --binary "$BINARY" --max GLIBC_2.39)
[ "$status" -eq 0 ] || { cat "$TMP/at.out" >&2; fail "GLIBC_2.39 at the policy was rejected (exit $status)"; }
grep -q 'GLIBC_2.39' "$TMP/at.out" || fail "pass message did not name the measured max"
pass "GLIBC_2.39 at the policy passes"

# --- below the policy passes and picks the real maximum -----------------------
status=$(MUNINN_READELF="$FAKEBIN/readelf" MUNINN_FAKE_READELF_OUT="$VV" \
  run_check "$TMP/below.out" sh "$CHECK" --binary "$BINARY" --max GLIBC_2.44)
[ "$status" -eq 0 ] || { cat "$TMP/below.out" >&2; fail "GLIBC_2.39 under a 2.44 policy was rejected"; }
grep -q 'GLIBC_2.39' "$TMP/below.out" || fail "pass message did not report the maximum, not the policy"
if grep -q 'GLIBC_2.43' "$TMP/below.out"; then
  fail "2.43 leaked from an unrelated fixture"
fi
pass "a lower maximum than the policy passes"

# --- bare version for --max is accepted --------------------------------------
status=$(MUNINN_READELF="$FAKEBIN/readelf" MUNINN_FAKE_READELF_OUT="$VV" \
  run_check "$TMP/bare.out" sh "$CHECK" --binary "$BINARY" --max 2.39)
[ "$status" -eq 0 ] || { cat "$TMP/bare.out" >&2; fail "--max 2.39 (bare) was rejected"; }
pass "--max accepts a bare version (2.39)"

# --- above the policy fails and names both numbers ----------------------------
status=$(MUNINN_READELF="$FAKEBIN/readelf" MUNINN_FAKE_READELF_OUT="$ABOVE" \
  run_check "$TMP/above.out" sh "$CHECK" --binary "$BINARY" --max GLIBC_2.39)
[ "$status" -ne 0 ] || { cat "$TMP/above.out" >&2; fail "GLIBC_2.43 was accepted under a 2.39 policy"; }
grep -q 'GLIBC_2.43' "$TMP/above.out" || fail "failure did not name the measured maximum"
grep -q 'GLIBC_2.39' "$TMP/above.out" || fail "failure did not name the policy"
grep -qi 'drift' "$TMP/above.out" || fail "failure did not explain the runner/baseline implication"
pass "GLIBC_2.43 above a 2.39 policy fails and names both versions"

# --- no GLIBC symbols fails ---------------------------------------------------
status=$(MUNINN_READELF="$FAKEBIN/readelf" MUNINN_FAKE_READELF_OUT="$EMPTY" \
  run_check "$TMP/empty.out" sh "$CHECK" --binary "$BINARY" --max GLIBC_2.39)
[ "$status" -ne 0 ] || { cat "$TMP/empty.out" >&2; fail "empty version info was accepted"; }
grep -qi 'no GLIBC' "$TMP/empty.out" || fail "empty version info error was unclear"
pass "version info without GLIBC_* symbols fails"

# --- a failing readelf fails --------------------------------------------------
status=$(MUNINN_READELF="$FAKEBIN/readelf" MUNINN_FAKE_READELF_OUT="$VV" MUNINN_FAKE_READELF_FAIL=1 \
  run_check "$TMP/readelf-fail.out" sh "$CHECK" --binary "$BINARY" --max GLIBC_2.39)
[ "$status" -ne 0 ] || { cat "$TMP/readelf-fail.out" >&2; fail "a failing readelf was ignored"; }
grep -q 'fake failure' "$TMP/readelf-fail.out" || fail "readelf stderr was swallowed"
pass "a failing readelf fails the check"

# --- a missing binary fails ---------------------------------------------------
status=$(MUNINN_READELF="$FAKEBIN/readelf" MUNINN_FAKE_READELF_OUT="$VV" \
  run_check "$TMP/missing.out" sh "$CHECK" --binary "$TMP/does-not-exist" --max GLIBC_2.39)
[ "$status" -ne 0 ] || { cat "$TMP/missing.out" >&2; fail "a missing binary was accepted"; }
grep -q 'not found' "$TMP/missing.out" || fail "missing-binary error was unclear"
pass "a missing binary fails"

# --- usage errors exit 2 ------------------------------------------------------
for args in '' '--binary' '--max' '--binary x' '--max GLIBC_2.39' '--bogus'; do
  # shellcheck disable=SC2086
  status=$(run_check "$TMP/usage.out" sh "$CHECK" $args)
  [ "$status" -eq 2 ] || { cat "$TMP/usage.out" >&2; fail "usage '$args' exited $status, want 2"; }
  grep -q 'usage:' "$TMP/usage.out" || fail "usage '$args' printed no usage line"
done
pass "missing or unknown arguments exit 2 with usage"

printf 'All check-glibc fixture tests passed.\n'
