#!/usr/bin/env bash
# Refresh the Rune vendor patches Muninn must apply itself.
#
#   scripts/vendor-rune.sh /path/to/rune-checkout
#   RUNE_DIR=/path/to/rune-checkout scripts/vendor-rune.sh
#
# Rune's own `[patch.crates-io]` only applies inside Rune's workspace; a git
# dependency does not carry it. The app keeps copies under `vendor/` and lists
# them in the root `Cargo.toml`, so this script copies the eight directories from
# a Rune checkout at the pinned revision. Run it whenever the Rune pin moves.
# (`vendor/tiny-skia-path` is not here: Rune reaches it as a renamed path
# dependency inside its own tree, which resolves through the git checkout.)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
rune_dir="${1:-${RUNE_DIR:-}}"
if [[ -z "$rune_dir" ]]; then
  echo "usage: $0 /path/to/rune-checkout" >&2
  exit 2
fi
rune_dir="$(cd "$rune_dir" && pwd)"

crates=(
  accesskit_atspi_common
  accesskit_macos
  accesskit_windows
  naga
  parley_engine
  sctk-adwaita
  wgpu-hal
  winit
)

for crate in "${crates[@]}"; do
  src="$rune_dir/vendor/$crate"
  if [[ ! -d "$src" ]]; then
    echo "missing Rune vendor directory: $src" >&2
    exit 1
  fi
  rm -rf "$root/vendor/$crate"
  cp -a "$src" "$root/vendor/$crate"
done

echo "updated ${#crates[@]} Rune vendor directories from $rune_dir"
