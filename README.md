<p align="center">
  <img src="assets/logo.svg" alt="Muninn logo" width="96" height="96" />
</p>

# Muninn

A local-first Markdown vault with an Obsidian-style wikilink graph, built with
[Rune](https://github.com/anomalyco/rune).

Muninn treats a folder of `.md` files as the whole database. Every note is a file you can read,
move, diff or put in version control. Links are written as `[[Note]]`, `[[Note|alias]]` or
`[[Note#heading]]`; the app indexes them and draws the result as a graph.

| Write | Graph |
| --- | --- |
| ![Muninn in Write mode](docs/notes.png) | ![Muninn's global knowledge graph](docs/graph.png) |

## Quick start

Muninn needs Rust **1.99.1** (the pinned toolchain in `rust-toolchain.toml`). On Linux, install
the usual windowing headers first:

```sh
sudo apt-get install -y pkg-config libxcb-render0-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev
```

Run it against the committed sample vault:

```sh
cargo run -p muninn-app -- --vault vault
```

Or point it at your own notes:

```sh
cargo run -p muninn-app -- --vault ~/notes
MUNINN_VAULT=~/notes cargo run -p muninn-app
```

Without `--vault` or `MUNINN_VAULT`, Muninn opens `~/Muninn` and creates it when needed.

## What it does

- **Write / Read / Graph** modes for the same note.
- Autosave after a short idle, with a `Saved` indicator in the status bar.
- Searchable note list grouped by folder with collapsing sections, a links rail with outgoing links
  and backlinks (including the line that made each link), and a one-click local graph.
- **Global** and **Local** graph views. Local keeps the open note and its immediate neighbours.
- Dragging a graph node pins it while you drag; the force layout settles around it. Drag the
  background to pan, scroll to zoom at the pointer, Shift+scroll to pan, pinch or Ctrl/Cmd+scroll
  to zoom.
- Unresolved wikilinks are drawn faintly in Read mode. Clicking one creates the note and opens
  it in Write mode. Path-qualified links (`[[folder/note]]`) resolve to that folder; a bare
  title shared by several notes resolves deterministically and displays with its folder.

In **Local** mode the graph keeps the open note and its immediate neighbours:

![Muninn's local graph](docs/local.png)

## Keys

| Shortcut | Action |
| --- | --- |
| `⌘K` / `Ctrl+K` | Focus search |
| `⌘N` / `Ctrl+N` | New note |
| `⌘S` / `Ctrl+S` | Save now (autosave is already on) |
| `⌘E` / `Ctrl+E` | Toggle Write / Read |
| `⌘G` / `Ctrl+G` | Toggle Graph |
| `⌘L` / `Ctrl+L` | Toggle Global / Local graph |
| `⌘1` / `⌘2` / `⌘3` | Write / Read / Graph |
| `Esc` | Leave the graph |

## Crates

The workspace is split by responsibility so each layer can be tested and reused on its own:

```text
muninn-app        binary: CLI, scenes, Rune entry point
    │
muninn-ui         application state, shortcuts, views, graph canvas
    │
muninn-graph      deterministic force layout and camera math (no Rune)
    │
muninn-core       vault scanning, wikilink parsing, indexes, preview
```

| Crate | Responsibility |
| --- | --- |
| `crates/muninn-core` | Files, notes, wikilinks, backlinks, search, Markdown preview rewrite. No UI dependencies. |
| `crates/muninn-graph` | Pure layout and camera math. Takes a `Vault`, produces positions. |
| `crates/muninn-ui` | `State`, Rune views, key handling, autosave. Headlessly testable. |
| `crates/muninn-app` | The `muninn` binary: argument parsing and `rune::run`. |
| `xtask` | `cargo run -p xtask -- check`, layering checks, bundling, snapshots. |

Dependency direction is enforced by `cargo run -p xtask -- layers`. See
[ARCHITECTURE.md](ARCHITECTURE.md) for the data flow and invariants.

## Development

```sh
cargo run -p xtask -- check     # fmt, clippy, tests, layering
cargo run -p xtask -- test
cargo run -p xtask -- fmt
cargo run -p xtask -- layers
cargo run -p xtask -- run --vault vault
```

Rune is pinned to a git revision because it is not published to crates.io. To develop Muninn
against a local Rune checkout:

```sh
./scripts/use-local-rune.sh
# or
RUNE_DIR=/path/to/rune ./scripts/use-local-rune.sh
```

The script writes a gitignored `.cargo/config.toml` patch. Remove that file to return to the
pinned revision.

### Headless snapshots

The Rune runtime can render scenes without a window:

```sh
target/debug/muninn --vault vault --snap notes.png --scene notes
target/debug/muninn --vault vault --snap graph.png --scene graph
```

Scenes are `notes`, `preview`, `graph`, `local`, `light` and `empty`. `cargo run -p xtask --
snapshots` writes all of them to `target/snapshots/`.

## Releasing

Releases are cut from tags:

```sh
git tag -a v0.1.0 -m "v0.1.0"
git push origin v0.1.0
```

The release workflow builds Linux, macOS and Windows archives, writes checksums and publishes a
GitHub release. Tags containing a hyphen (`v0.2.0-rc.1`) are marked as prereleases.

## License

MIT. See [LICENSE](LICENSE).
