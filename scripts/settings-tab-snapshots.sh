#!/bin/sh
# Render one screenshot per Settings tab against a throwaway copy of the
# sample vault and an isolated home, so nothing reads or writes the user's
# real vault or settings.json.
set -eu
root="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/muninn-settings-shots.XXXXXX")"
vault="$work/vault"
cp -R "$root/vault" "$vault"
export HOME="$work/home"
export XDG_CONFIG_HOME="$work/config"
export XDG_STATE_HOME="$work/state"
export XDG_CACHE_HOME="$work/cache"
export XDG_DATA_HOME="$work/data"
unset MUNINN_VAULT MUNINN_REDUCED_MOTION || true
mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME"
out="${1:-$root/target/settings-tab-snapshots}"
mkdir -p "$out"
cargo build -p muninn-app --manifest-path "$root/Cargo.toml"
bin="$root/target/debug/muninn"
for scene in settings-vaults settings-editor settings-appearance settings-graph settings-sync; do
  "$bin" --vault "$vault" --snap "$out/$scene.png" --scene "$scene"
  echo "wrote $out/$scene.png"
done
