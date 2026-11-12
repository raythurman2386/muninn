# Kanban fixtures

Hand-written from `docs/kanban-format.md`. No file in this folder was produced by Obsidian or copied from the Obsidian Kanban plugin, its tests, or its fixtures.

`bad-unterminated.md` and `bad-json.md` are deliberately unreadable as boards. Every other `.md` file is a valid kanban note and must round-trip byte for byte. `crlf.md` uses CRLF, `bom.md` starts with a UTF-8 byte-order mark, and `no-final-newline.md` has no final newline. Git stores this directory without line-ending conversion so those bytes survive checkout.
