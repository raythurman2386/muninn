# Published W7 media

Optimized stills and loops from the W6 v0.8.2 Tidepool captures
(`/workspace/muninn-web-assets`, captured from the v0.8.2 linux x86_64
release binary). PNG masters and GIFs are not in this tree: the site
uses AVIF/WebP stills and WebM+MP4 loops with AVIF/WebP posters.

## Frame choices

- **Board.** The published stills are `board-drawn` (the Launch Plan
  board with a red ink circle around Launch day and an arrow to Retro)
  and `board-palette` (the stroke/fill palette open). Captures used
  `make_vault.py --fills none`. Muninn 0.8.2 still lifts every fill to
  a 3:1 contrast against the paper (the #88 display-only repair), which
  turns Excalidraw pastel fills muddy on white paper and greys the ink
  on night paper, so fill is not shown.
- **Kanban.** The published stills and the kanban loop are a close-up
  of the three lanes (To do, Doing, Done), cropped at
  `(348, 160, 1508, 780)` on the 1600×1000 capture (1160×620, and
  696×372 at 0.6). That frame leaves out the compact “+ Add lane”
  control, which has no room for its label at the 1200-px capture
  window and sits clipped against the right edge. Files are named
  `kanban-lanes-*` and `loop-kanban-lanes-*`.

All stills and loops show the synthetic Tidepool vault, not real data.
