# Architecture

Muninn is a small desktop app with a deliberately boring data model: **the file is the source of
truth**. There is no database, migration step or background index writer. The vault is scanned
when it is opened or reloaded, links are re-indexed after a save, and everything the UI shows is
derived from that scan.

## Crate graph

```text
muninn-app ──► muninn-ui ──► muninn-graph ──► muninn-core
     │              │
     └──────────────┴──► rune, rune-kit, rune-theme-opencode
```

| Crate | May depend on | Why |
| --- | --- | --- |
| `muninn-core` | nothing in the workspace | Pure domain model; fast unit tests. |
| `muninn-graph` | `muninn-core` | Needs note ids and links, nothing else. |
| `muninn-ui` | `muninn-core`, `muninn-graph`, Rune | Owns state, rendering and input. |
| `muninn-app` | `muninn-ui`, Rune | Entry point only. |
| `xtask` | tooling crates | Never linked into the app. |

`cargo run -p xtask -- layers` reads `cargo metadata` and rejects any new `muninn-*` edge that
is not in this table. That check is part of `xtask check` and CI.

## Data flow

1. **Scan.** `muninn_core::Vault::load` walks the root (up to 12 levels, skipping dotfiles),
   reads every `.md` file and records its title, relative path and source.
2. **Index.** `Vault::reindex` resolves every `[[wikilink]]` through a lower-cased title/path
   index and writes each note's `links` and `backlinks` lists. Both are unique and sorted.
3. **Layout.** `muninn_graph::Graph::sync` mirrors the vault as nodes and undirected,
   deduplicated edges. `Graph::step` runs one deterministic force step.
4. **Render.** `muninn_ui::view` builds the Rune element tree from `State`. The graph camera
   publishes pan/zoom through `Live` values so a drag redraws without rebuilding the tree.
5. **Save.** `muninn_ui::poll` notices an edited buffer, waits 0.55 s of idle, writes the file
   and re-indexes. A failed write becomes a status-bar error, never a silent change.

## Invariants

- **Note ids are indexes.** `Note::id` is always the position in `Vault::notes`. The graph uses
  the same ids for nodes and edges.
- **Links are resolved, not stored.** Changing a title re-resolves on the next `reindex`; no
  cross-file state needs repairing.
- **Determinism.** The initial graph layout depends only on note order and title hash. Tests
  compare settled positions exactly.
- **No hidden state on disk.** Only `.md` files are written. The sample vault is ordinary
  content used by tests and snapshots.
- **Code is not a link.** Wikilinks inside inline code and fenced blocks are ignored by the
  parser and preserved by the preview rewriter.

## Parsing rules

`[[target]]`, `[[target|alias]]` and `[[target#heading]]` are supported. A target resolves by:

1. exact title (case-insensitive),
2. vault-relative path without `.md`,
3. bare filename.

Links to the current note are allowed in the source but do not produce self-edges in the graph.
Unresolved targets stay in the source, render faintly, and create a note when clicked.

## Scaling notes

- New non-UI behaviour belongs in `muninn-core` or `muninn-graph` with unit tests.
- New screens belong in `muninn-ui`; keep `muninn-app` to wiring.
- If a second front end appears (CLI, web, mobile), it should consume `muninn-core` and
  `muninn-graph` directly rather than growing `muninn-ui`.
- Add a crate only when it has a clear owner and dependency direction; update `allowed_edges`
  in `xtask/src/main.rs` at the same time.
