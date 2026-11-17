# Wikilinks

A wikilink is two square brackets around a title:

```markdown
[[Getting Started]]
[[Getting Started|start here]]
[[Knowledge Graph#Why it helps]]
```

The third form links to a heading. Muninn opens the note and moves to that
heading; if the heading is missing, the note opens at the top.

## Resolution

| Written | Resolves to |
| --- | --- |
| `[[Welcome]]` | Welcome.md |
| `[[welcome]]` | Welcome.md |
| `[[folder/Note]]` | folder/Note.md |
| `[[Note#Part]]` | Note.md, at Part |

Links are parsed outside inline code such as `[[not a link]]` and outside fenced blocks.
Everything else becomes a normal Markdown link, which is why the [[Markdown]] preview can
handle clicks without a special parser.

See also [[Backlinks]] and [[Knowledge Graph]].
