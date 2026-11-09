#!/bin/sh
# Render Settings sync scenes against a synthetic vault and an isolated home.
# Does not read ~/Documents/Muninn or the user's real config.
set -eu
root="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/muninn-sync-shots.XXXXXX")"
vault="$work/vault"
mkdir -p "$vault"
printf '%s\n' '# Synthetic' 'A fixture note for the sync settings scene.' > "$vault/Note.md"
printf '%s\n' 'attachment' > "$vault/pic.png"
export HOME="$work/home"
export XDG_CONFIG_HOME="$work/config"
export XDG_STATE_HOME="$work/state"
export XDG_CACHE_HOME="$work/cache"
export XDG_DATA_HOME="$work/data"
unset MUNINN_VAULT || true
mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME"
out="${1:-$root/target/sync-settings-snapshots}"
mkdir -p "$out"
cargo build -p muninn-app --manifest-path "$root/Cargo.toml"
bin="$root/target/debug/muninn"
for scene in settings-sync settings-sync-drive settings-sync-tailscale settings-sync-conflict; do
  "$bin" --vault "$vault" --snap "$out/$scene.png" --scene "$scene"
  echo "wrote $out/$scene.png"
done
