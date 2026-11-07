# Knowledge Graph

The graph is the same link index the [[Backlinks]] panel uses, drawn as nodes and edges. Each
node is a note; each line is a [[Wikilinks]] link.

The layout is a small force simulation:

- nodes push each other apart,
- links pull their ends together,
- a gentle gravity keeps the map on screen.

See [[Force Layout]] for the numbers, and [[Local Graph]] for the focused view.

## Why it helps

A list of notes tells you what you wrote. A graph tells you what talks to what. The shape of a
vault is often a surprise: a note you thought was central turns out to be a leaf.

```text
Welcome ── Getting Started ── Wikilinks ── Backlinks
   │              │                 │
   └── Knowledge Graph ─────────────┘
```

The fenced block above is a drawing, not a link. Brackets inside code stay brackets.
