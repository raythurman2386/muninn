# Getting Started

Three habits make a vault useful:

1. Write a note when something is worth keeping.
2. Link it to a note you already have with `[[double brackets]]`.
3. Let the [[Backlinks]] accumulate. They become the map.

## Make a note

Press `⌘N` (or `Ctrl+N`), type a name and press Enter. Muninn writes `Name.md`
in the current folder and opens it. The `+` beside a folder creates the note
there instead. `⌘I` / `Ctrl+I` imports a Word, PDF, Markdown or other document
as a new note, with its images and the original file beside it.

## Save it

Muninn saves after a short idle (0.55 s by default). `⌘S` / `Ctrl+S` saves now;
the status bar says when the note is saved. There is no separate database.

## Link it

Type `[[` and a title. [[Wikilinks]] are resolved by filename,
case-insensitively. `[[welcome]]` and `[[Welcome]]` are the same note. A link
to a note that does not exist yet is drawn faintly in Read mode; click it and
Muninn creates the note.

## Read it

The **Read** mode renders the Markdown. The links rail on the right shows
outgoing links and backlinks, with the line that made each backlink. Press
`⌘E` / `Ctrl+E` to switch between Write and Read.

## Find your way

`⌘K` / `Ctrl+K` searches notes. `⌘P` / `Ctrl+P` inserts a wikilink at the
caret. `⌘1`…`⌘5` / `Ctrl+1`…`Ctrl+5` switches Write, Read, Graph, Board and
Kanban; [[Boards and Kanban]] explains the last two. `⌘?` / `Ctrl+?` opens the
full shortcut map.

Related: [[Markdown]], [[Wikilinks]] and [[Daily Notes]].
