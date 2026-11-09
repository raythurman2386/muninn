# Demo clips

Short, captioned-free captures of the v0.5 moments, recorded headlessly with
Rune's built-in tooling (no window, no GPU needed beyond Rune's offscreen
renderer). Each clip is a script of timestamped input replayed against
`world`/scene state, streamed into ffmpeg by `--video`.

Regenerate from the repo root (build the debug binary first with
`cargo build -p muninn-app`):

```sh
target/debug/muninn --vault vault --scene graph \
  --script scripts/demos/graph-glide.json \
  --video docs/demos/graph-glide.mp4 --keys --pointer

target/debug/muninn --vault vault --scene graph \
  --script scripts/demos/graph-effects.json \
  --video docs/demos/graph-effects.mp4 --keys --pointer

target/debug/muninn --vault vault --scene notes \
  --script scripts/demos/palette.json \
  --video docs/demos/palette.mp4 --keys --pointer

MUNINN_OMARCHY_COLORS= target/debug/muninn --vault vault --scene notes \
  --script scripts/demos/theme.json \
  --video docs/demos/theme.mp4 --keys --pointer

XDG_STATE_HOME=/tmp/demo/state XDG_CONFIG_HOME=/tmp/demo/config \
MUNINN_OMARCHY_COLORS= target/debug/muninn --vault vault --scene notes \
  --script scripts/demos/e2e.json \
  --video docs/demos/e2e.mp4 --fps 30 --keys --pointer
```

- `--keys` draws keycaps for script keys/chords; `--pointer` ripples where the
  script taps.
- `--duration auto` ends at the script's `end` mark (or 1 s after the last
  event); frames stream straight into ffmpeg as H.264.
- The theme clip sets `MUNINN_OMARCHY_COLORS=` (empty) so the segmented
  theme control renders instead of the "Following Omarchy" note — the
  documented escape hatch for non-Omarchy hosts.
- Scripts tap graph nodes by element key (`graph-node-N`, from a
  `--tree` dump) rather than coordinates, so they stay stable if the
  layout drifts.

## Clips

- `graph-glide.mp4` — clicking graph nodes glides the camera between notes.
- `graph-effects.mp4` — Settings Graph effects cycles Classic → Electric → Off (and reduced motion in companion stills).
- `palette.mp4` — `⌘⇧P` opens the command palette, Enter jumps to the
  local graph.
- `theme.mp4` — changing the theme live in Settings (Omarchy → Dark → Light).
- `e2e.mp4` — a full end-to-end tour: search, open, create a note, glide
  through the graph, command palette into the local graph, theme change,
  and the Help overlay. It runs under isolated `XDG_STATE_HOME`/`XDG_CONFIG_HOME`
  so it starts from the default state even if you re-record from a dirty machine.

GIF versions (`*.gif`, 720 px, 12 fps) are palette-encoded
`ffmpeg -i <clip>.mp4` variants for README embedding.
