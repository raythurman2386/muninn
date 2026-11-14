# Marketing captures

One command re-records every site still and loop from a Muninn **release
binary**, so the site matches what people download:

```sh
CAPTURE_FETCH_RUNTIME=1 site/scripts/capture/capture.sh /path/to/scratch v0.8.2
```

It downloads `muninn_<tag>_linux_x86_64.tar.gz` with `gh`, checks it against
`checksums.txt`, and verifies the cosign bundle with
`sigstore verify identity` (workflow identity pinned to
`release.yml@refs/tags/<tag>`). The sigstore check is required unless you pass
`MUNINN_BIN=/path/muninn` or set `CAPTURE_SKIP_SIGSTORE=1`. It then builds the
synthetic **Tidepool** vault (`make_vault.py`: a fake habit-tracker startup, no
real data, 0 broken links), drives the app, and encodes the results.

| File | What it does |
|---|---|
| `capture.sh` | Fetch and verify the release, optional user-level runtime, then the three steps below |
| `make_vault.py` | Tidepool vault: 15 notes, the *Launch Week* kanban, the *Launch Plan* board (`--paper white\|night`, `--fills pastel\|none`) |
| `xdrive.py` | Private Xvfb, isolated env, XTEST pointer and keys, `ffmpeg x11grab` stills and lossless recordings |
| `capture.py` | Scenes: `stills`, `board`, `kanban`, `graph`, `hero`, per theme (`--themes`, `--scenes`) |
| `encode.py` | 1600/960 PNG, WebP, AVIF; H.264 MP4 and VP9 WebM loops with posters; hero drop-ins; GIFs; `manifest.json`; budgets |
| `contact_sheet.py` | One PNG of every still and three frames of every loop, for review |

## Isolation

- Its own Xvfb display (`CAPTURE_DISPLAY`, default `:93`; refuses a display in use).
- `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_CACHE_HOME`,
  `XDG_RUNTIME_DIR` (0700) and `TMPDIR` under `WORKDIR/run/<scene>-<theme>/`,
  a fresh vault per scene, `dbus-run-session`, always `--new-instance`.
- The app is started with an environment allowlist (`PATH`, locale, `TZ`, plus
  the throwaway dirs and the display/Vulkan vars we set). Caller secrets such
  as `GH_TOKEN` and `GITHUB_TOKEN` are not passed through.
- Every process it starts runs in its own session and is stopped by process
  group; nothing else is signalled. Xvfb must come up before the app starts;
  a stuck ffmpeg is killed if it ignores the stop timeout.

## Rendering

The app runs windowed at 2x (`WINIT_X11_SCALE_FACTOR=2`) on Mesa **lavapipe**
(CPU Vulkan). Stills and loops use a 1200x750 logical window (2400x1500 device,
published at 1600x1000 and 960x600). The hero and its poster use 1200x794, which
publishes at 1600x1058, the size the site already uses.

The release binaries link `GLIBC_2.43` (built on `ubuntu-latest`). On an older
host, `CAPTURE_FETCH_RUNTIME=1` fetches Debian's `libc6` 2.43 and
`mesa-vulkan-drivers` over **https**, checks a pinned sha256 (and, when
`debian-archive-keyring` is present, Debian's signed `InRelease`/`Packages`),
unpacks them into `WORKDIR/tools`, and starts the binary through that
`ld-linux` with `--library-path`. Nothing is installed system-wide. A URL
override (`LIBC6_DEB_URL` / `MESA_VK_DEB_URL`) is refused unless the matching
`*_SHA256` is also set.

## Budgets

Loops 6-10 s (sped up, never slowed, to 9.5 s), at most 1.5 MB at 1600 px and
600 KB at 960 px; hero 13 s, at most 3 MB; GIFs (README and social only) at most
3 MB. `encode.py` raises the CRF until a file fits and lists any miss in
`manifest.json` under `budget_misses`.
