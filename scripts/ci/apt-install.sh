#!/bin/sh
# Install packages with bounded retries so one flaky apt mirror cannot fail a
# build. Usage: scripts/ci/apt-install.sh PACKAGE...
#
# Test-only overrides (the workflow uses the defaults):
#   MUNINN_APT_GET      apt-get binary/path          (default: apt-get)
#   MUNINN_APT_ATTEMPTS maximum attempts             (default: 3)
#   MUNINN_APT_BACKOFF  seconds between attempts     (default: 5)
# This script never escalates: callers that need root wrap it in sudo.
set -eu

fail() { printf 'apt-install: %s\n' "$*" >&2; exit 2; }

APT_GET=${MUNINN_APT_GET:-apt-get}
ATTEMPTS=${MUNINN_APT_ATTEMPTS:-3}
BACKOFF=${MUNINN_APT_BACKOFF:-5}

case "$ATTEMPTS" in
  '' | *[!0-9]*) fail "MUNINN_APT_ATTEMPTS must be a positive integer (got '$ATTEMPTS')" ;;
esac
[ "$ATTEMPTS" -ge 1 ] 2>/dev/null || fail "MUNINN_APT_ATTEMPTS must be a positive integer (got '$ATTEMPTS')"

case "$BACKOFF" in
  '' | *[!0-9]*) fail "MUNINN_APT_BACKOFF must be a non-negative integer (got '$BACKOFF')" ;;
esac

[ "$#" -gt 0 ] || fail "usage: $0 PACKAGE..."

attempt=1
while [ "$attempt" -le "$ATTEMPTS" ]; do
  printf 'apt-install: attempt %s/%s: running %s update\n' "$attempt" "$ATTEMPTS" "$APT_GET"
  if "$APT_GET" update; then
    printf 'apt-install: attempt %s/%s: installing: %s\n' "$attempt" "$ATTEMPTS" "$*"
    if "$APT_GET" install -y --no-install-recommends --fix-missing "$@"; then
      printf 'apt-install: packages installed successfully on attempt %s\n' "$attempt"
      exit 0
    else
      status=$?
      printf 'apt-install: attempt %s/%s: apt-get install failed (exit %s)\n' \
        "$attempt" "$ATTEMPTS" "$status" >&2
    fi
  else
    status=$?
    printf 'apt-install: attempt %s/%s: apt-get update failed (exit %s)\n' \
      "$attempt" "$ATTEMPTS" "$status" >&2
  fi

  if [ "$attempt" -lt "$ATTEMPTS" ]; then
    if [ "$BACKOFF" -gt 0 ]; then
      printf 'apt-install: retrying in %ss\n' "$BACKOFF"
      sleep "$BACKOFF"
    fi
  fi
  attempt=$((attempt + 1))
done

fail "all $ATTEMPTS attempt(s) failed; no packages installed"
