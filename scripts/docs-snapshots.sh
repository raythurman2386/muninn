#!/bin/sh
# Re-render the README stills and the Settings-tab gallery from the sample
# vault. Every run uses a throwaway copy of `vault/` and an isolated
# HOME/XDG tree, so it never reads or writes the user's real notes or
# settings. The PNGs are 2x (2360x1560) like the previous docs images.
#
#   scripts/docs-snapshots.sh [OUT_DIR]     (default: docs/)
set -eu
root="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
out="${1:-$root/docs}"
out="$(CDPATH= cd -- "$out" 2>/dev/null && pwd || printf '%s' "$out")"
mkdir -p "$out" "$out/settings-tabs"

# Build before isolating HOME so cargo keeps using the real cache.
cargo build --locked -p muninn-app --manifest-path "$root/Cargo.toml"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
bin="${MUNINN_BIN:-$target_dir/debug/muninn}"

work="$(mktemp -d "${TMPDIR:-/tmp}/muninn-docs-shots.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM
vault="$work/vault"
cp -R "$root/vault" "$vault"
export HOME="$work/home"
export XDG_CONFIG_HOME="$work/config"
export XDG_STATE_HOME="$work/state"
export XDG_CACHE_HOME="$work/cache"
export XDG_DATA_HOME="$work/data"
unset MUNINN_VAULT MUNINN_REDUCED_MOTION MUNINN_OMARCHY_COLORS || true
mkdir -p "$HOME" "$XDG_CONFIG_HOME/muninn" "$XDG_STATE_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME"
# Start on the note the README talks about, with the documented dark theme.
printf '{"theme":"dark","last_vault":"%s","selected_rel":"Knowledge Graph.md"}\n' \
  "$vault" > "$XDG_CONFIG_HOME/muninn/settings.json"

# scene:output-name:seconds-to-settle
for spec in notes:notes:1 graph:graph:4 local:local:4 docs-board:board:3 docs-kanban:kanban:3; do
  scene=${spec%%:*}
  rest=${spec#*:}
  name=${rest%%:*}
  at=${rest##*:}
  "$bin" --vault "$vault" --snap "$out/$name.png" --scene "$scene" \
    --size 1180x780 --scale 2 --at "$at" --theme dark
  printf 'wrote %s\n' "$out/$name.png"
done

for scene in settings-vaults settings-editor settings-appearance settings-graph settings-sync; do
  "$bin" --vault "$vault" --snap "$out/settings-tabs/$scene.png" --scene "$scene" \
    --size 1180x780 --scale 2 --at 1 --theme dark
  printf 'wrote %s\n' "$out/settings-tabs/$scene.png"
done
