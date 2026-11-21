#!/bin/sh
# Fail when a Linux binary needs a newer glibc than the release policy allows,
# so a runner-image upgrade cannot silently raise the published floor.
#
# Usage: scripts/ci/check-glibc.sh --binary PATH --max GLIBC_2.39
#
# The maximum `GLIBC_*` version named in the binary's version info is compared
# with --max using `sort -V`. Both `GLIBC_2.39` and `2.39` are accepted.
#
# Test-only override (the workflow uses the default):
#   MUNINN_READELF  readelf binary/path        (default: readelf)
set -eu

READELF=${MUNINN_READELF:-readelf}
BINARY=
MAX=

usage() {
  printf 'usage: %s --binary PATH --max GLIBC_<version>\n' "$0" >&2
  exit 2
}

fail() {
  printf 'check-glibc: %s\n' "$*" >&2
  exit 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --binary)
      [ "$#" -ge 2 ] || usage
      BINARY=$2
      shift 2
      ;;
    --max)
      [ "$#" -ge 2 ] || usage
      MAX=$2
      shift 2
      ;;
    -h | --help)
      usage
      ;;
    *)
      printf 'check-glibc: unknown argument: %s\n' "$1" >&2
      usage
      ;;
  esac
done

[ -n "$BINARY" ] || usage
[ -n "$MAX" ] || usage

# Normalise the policy to a bare version and reject nonsense before measuring.
POLICY=${MAX#GLIBC_}
case "$POLICY" in
  '' | *[!0-9.]* | .* | *. | *..*)
    printf 'check-glibc: --max must look like GLIBC_2.39 (got %s)\n' "$MAX" >&2
    usage
    ;;
esac

[ -f "$BINARY" ] || fail "binary not found: $BINARY"

# readelf writes diagnostics to stderr; keep them on failure so the cause is
# visible in CI instead of just "no versions found".
if ! info=$("$READELF" --version-info "$BINARY" 2>&1); then
  printf '%s\n' "$info" >&2
  fail "$READELF --version-info failed for $BINARY"
fi

versions=$(printf '%s\n' "$info" | sed -n 's/.*\(GLIBC_[0-9][0-9.]*\).*/\1/p')
[ -n "$versions" ] ||
  fail "no GLIBC_* symbols found in $BINARY (static build, wrong file, or unreadable version info)"

measured=$(printf '%s\n' "$versions" | sort -V | tail -n 1)
highest=$(printf '%s\n%s\n' "$measured" "GLIBC_$POLICY" | sort -V | tail -n 1)

if [ "$highest" != "GLIBC_$POLICY" ]; then
  printf 'check-glibc: %s requires %s but the policy is GLIBC_%s (measured max %s, policy GLIBC_%s)\n' \
    "$BINARY" "$measured" "$POLICY" "$measured" "$POLICY" >&2
  printf 'check-glibc: baseline drift: release builds must stay on a glibc %s runner image (Ubuntu 24.04), not a newer one\n' \
    "$POLICY" >&2
  exit 1
fi

printf 'check-glibc: %s needs %s, within the %s policy - ok\n' "$BINARY" "$measured" "GLIBC_$POLICY"
