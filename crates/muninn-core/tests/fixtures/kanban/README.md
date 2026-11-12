# Kanban fixtures

Every file here is hand-written from `docs/kanban-format.md`. No file was produced by Obsidian or copied from the Obsidian Kanban plugin, its tests, or its fixtures.

A provenance line inside a `.md` fixture would sit in front of the frontmatter and stop the file being a kanban note, so the provenance is listed here instead. Git stores this directory without line-ending conversion so CRLF, the byte-order mark and the missing final newline survive checkout.

`bad-unterminated.md` and `bad-json.md` are deliberately unreadable as boards. Every other `.md` file is a valid kanban note and must round-trip byte for byte.

| File | What it is |
|---|---|
| `archive-de.md` | Hand-written from `docs/kanban-format.md`: a German done-lane marker and archive heading. |
| `archive-en.md` | Hand-written from `docs/kanban-format.md`: an English archive. |
| `archive-ja.md` | Hand-written from `docs/kanban-format.md`: a Japanese done-lane marker and archive heading. |
| `bad-json.md` | Hand-written from `docs/kanban-format.md`: a footer whose JSON does not parse. |
| `bad-unterminated.md` | Hand-written from `docs/kanban-format.md`: a settings footer with no closing `%%`. |
| `block-ids.md` | Hand-written from `docs/kanban-format.md`: cards with block ids. |
| `bom.md` | Hand-written from `docs/kanban-format.md`: a UTF-8 byte-order mark and a quoted value. |
| `card-closed-fence.md` | Hand-written from `docs/kanban-format.md`: a closed indented fence, then a column-0 card. |
| `card-open-fence.md` | Hand-written from `docs/kanban-format.md`: an unclosed indented fence, then the next lane. |
| `card-paragraph.md` | Hand-written from `docs/kanban-format.md`: a blank line and an indented paragraph that belong to the card. |
| `card-text.md` | Hand-written from `docs/kanban-format.md`: wikilinks, an embed, dates, a time, a tag and an inline field. |
| `crlf.md` | Hand-written from `docs/kanban-format.md`: CRLF line endings. |
| `done-lane.md` | Hand-written from `docs/kanban-format.md`: a done lane. |
| `edit-board.md` | Hand-written from `docs/kanban-format.md`: lanes with a multi-line card, a block id, a paragraph between lanes, a lane limit and a done lane, for editing (slice 4). |
| `fence-column0-card.md` | Hand-written from `docs/kanban-format.md`: an indented fence still open when a column-0 fence line appears. |
| `fence-longer-close.md` | Hand-written from `docs/kanban-format.md`: a fence closed by a longer run of the same mark, then another lane. |
| `fence-opaque.md` | Hand-written from `docs/kanban-format.md`: fenced code that looks like lanes, cards and a settings footer. |
| `fence-unclosed.md` | Hand-written from `docs/kanban-format.md`: a column-0 fence that never closes. |
| `footer-nested.md` | Hand-written from `docs/kanban-format.md`: a settings object with nested objects and an array of objects. |
| `frontmatter-extra.md` | Hand-written from `docs/kanban-format.md`: extra frontmatter keys and a quoted `kanban-plugin` value. |
| `indent-deep-line.md` | Hand-written from `docs/kanban-format.md`: one continuation line indented by more than two spaces. |
| `indent-mixed.md` | Hand-written from `docs/kanban-format.md`: continuation lines that mix a tab indent and a two-space indent. |
| `lanes-and-limits.md` | Hand-written from `docs/kanban-format.md`: several lanes, including limits and a written `(0)`. |
| `multibyte.md` | Hand-written from `docs/kanban-format.md`: multibyte lane and card text. |
| `multiline-cards.md` | Hand-written from `docs/kanban-format.md`: a multi-line card and a nested checklist. |
| `new-kanban.md` | Hand-written from `docs/kanban-format.md`: the New Kanban layout. |
| `no-final-newline.md` | Hand-written from `docs/kanban-format.md`: no final newline. |
| `over-limit.md` | Hand-written from `docs/kanban-format.md`: a lane holding more cards than its limit, a two-line card, a paragraph between lanes and a done lane. |
| `passthrough.md` | Hand-written from `docs/kanban-format.md`: a paragraph and a level-3 heading between lanes. |
| `plain-list.md` | Hand-written from `docs/kanban-format.md`: list items with no checkbox. |
| `settings-footer.md` | Hand-written from `docs/kanban-format.md`: a settings footer with several keys. |
| `spec-example.md` | Hand-written from `docs/kanban-format.md`: the complete example in the spec. |
| `task-open-fence.md` | Hand-written from `docs/kanban-format.md`: a column-0 task while an indented fence is still open, then another lane. |
