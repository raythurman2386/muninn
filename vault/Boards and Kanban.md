# Boards and Kanban

Both modes are files, not a separate store.

## Kanban

A Kanban note is Markdown with a small frontmatter marker:

```markdown
---
kanban-plugin: board
---

## To do

- [ ] Write the release notes
- [ ] Review [[Getting Started]]
```

Open [[Launch Week]] and press `⌘5` / `Ctrl+5`. Drag cards between lanes, type a
date or tag in the card text, switch Board / List / Table from the header, and
archive finished cards. The note remains readable Markdown.

## Board

A board is an `.excalidraw` file. Press `⌘4` / `Ctrl+4` to see the boards in
this vault; `Muninn board.excalidraw` is a sample with rectangles, diamonds,
ellipses, arrows, lines, freehand and text. Double-click a shape to write its
label, use the toolbar to change stroke and fill, and press `⌘S` to save.

Both modes are optional. Notes, boards and kanbans can live in the same
folders and are picked up by the same vault scan.

Related: [[Getting Started]], [[Welcome]].
