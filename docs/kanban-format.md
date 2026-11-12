# Muninn kanban note format

Status: draft 1, for Muninn 0.8.0. This document is Muninn's own description of how a kanban board is stored in a markdown note. It is written to be compatible with files made by the Obsidian Kanban community plugin. It describes only behaviour and the syntax a user can see in a file. Anything not yet settled is marked **Open point**. Implementers must not guess at an open point; they report it instead.

## 1. Terms

| Term | Meaning |
|---|---|
| **Kanban note** | A `.md` note in the vault whose frontmatter marks it as a kanban (section 3). It is still a note in every other way: it has a title, it can be linked, and its links count. |
| **Lane** | A column on the board. It has a title and an optional lane limit. |
| **Card** | One item in a lane. Its text is markdown and may span several lines. |
| **Done mark** | A card's checkbox state: open or done. |
| **Lane limit** | An optional maximum card count for a lane, stored in the lane's title line. |
| **Done-lane marker** | A line under a lane's title which says that cards entering that lane become done. |
| **Archive** | A final section of the file holding cards taken off the board. |
| **Settings footer** | A block at the end of the file holding the board's own settings as JSON. |
| **Passthrough** | Any text Muninn does not model. It is kept exactly as found. |

## 2. File layout, top to bottom

1. Frontmatter (required).
2. Zero or more lanes, each made of a title line, an optional done-lane marker, then cards.
3. An optional archive.
4. An optional settings footer.

Blank lines and passthrough text may appear between any of these parts.

A complete example:

````markdown
---

kanban-plugin: board

---

## To do

- [ ] Write the release notes
- [ ] Review [[Roadmap]] with the team
  second line of the same card


## Doing (3)

- [ ] Fix the sync bug ^card-7f2


## Done

**Complete**
- [x] Ship 0.7.1


***

## Archive

- [x] Old task

%% kanban:settings
```
{"kanban-plugin":"board"}
```
%%
````

## 3. Frontmatter and detection

- **Frontmatter.** The file starts with a line `---`, then frontmatter lines, then a closing `---` line.
- **Detection.** The note is a kanban note when the frontmatter has the key `kanban-plugin`. The usual value is `board`.
- **Other values.** The values `list`, `table` and `basic` also appear in the wild.
  - **Open point F-1:** what the values other than `board` mean for display. In 0.8.0 they are detected as kanban notes and kept unchanged.
- **Other frontmatter.** Keys besides `kanban-plugin` may be present. They are passthrough and are kept byte for byte, including order, comments, quoting and blank lines.
- **Reader.** Muninn reads frontmatter with its own minimal flat reader: `key: value` lines and simple `- item` lists. Anything more complex is passthrough.
- **Edge cases** that must still detect: a value quoted as `'board'` or `"board"`; CRLF line endings; a UTF-8 byte-order mark before the first `---`.

## 4. Lanes

- **Title line.** A lane starts with a level-2 heading line: `## ` followed by the lane title.
- **Lane limit.** If the title ends with a space and a whole number in parentheses, for example `## Doing (3)`, that number is the lane limit and the title is the text before it (`Doing`). Without such a suffix there is no limit.
  - A limit of `0` means no limit.
  - **Open point L-1:** whether `0` is ever written to a file. Muninn never writes `(0)`.
- **Done-lane marker.** If the first non-blank line after the title line is exactly `**Complete**` (or the same marker in another language, section 8), the lane is a done lane.
- **Extent.** A lane's cards are the list items that follow, up to the next level-2 heading, the archive rule (section 6), or the settings footer.
- **Other headings.** Level-1 and level-3+ headings are not lanes. They are passthrough inside the lane they appear in.
- **Writing a new lane.** Muninn writes `## Title`, or `## Title (N)` when it has a limit, then one blank line, then the done-lane marker if any, then the cards, then blank lines before the next lane.
  - **Open point L-2:** how many blank lines go between lanes. Muninn writes two blank lines after a lane's last card when it creates a lane, and never changes the spacing of existing lanes.

## 5. Cards

- **Card line.** A card is a markdown task-list item at the start of a line: `- [ ] text` (open) or `- [x] text` (done).
- **Done mark.** Any single character between the brackets other than a space counts as done, and Muninn keeps that exact character.
  - **Open point C-1:** whether characters other than `x` and `X` carry any extra meaning. Muninn treats them as done, displays them as done, and never rewrites them.
- **Multi-line cards.** Continuation lines are indented under the first line and belong to the card. That includes nested lists and nested task items, which make up a card's checklist. A blank line that is followed by another indented continuation line belongs to the same card, so moving the card moves that paragraph too.
- **Indentation.** Only a space or a tab indents a continuation. Any other whitespace is part of the line's text. A line that does not start with a space or a tab ends the card, and that line is not part of the card. A tab moves forward to the next column that is a multiple of four, with the first column counted as zero.
  - The card indent is the narrowest space-or-tab lead among the card's non-blank continuation lines. When two leads are the same width, the earlier line wins. If that winning lead is only spaces and is wider than two columns, the indent is two spaces, and the extra spaces stay in the text.
  - A non-blank continuation keeps its whole lead in the card text when that lead does not begin with the card indent and is at least as wide, in columns, as the indent. Every other non-blank continuation is stored without the card indent. Writing the card puts the indent back in front of those lines. A blank continuation is stored and written with nothing in front of it.
  - A card whose narrowest lead is one space keeps that one-space indent, and a new line is indented by one space. One space does not reach the two columns occupied by `- `, so after a blank line, or when the new line begins with `*` or `1.`, another reader can place that line outside the list item. Muninn still keeps it on the card, and reading the written card yields the same text.
- **Block id.** A card's first line may end with a space, a caret and an identifier, such as ` ^card-7f2`. This is the block id that Obsidian uses for block links (see Obsidian's public help on block references). It belongs to the card, moves with it, and is not shown as card text.
- **Card text.** The text after the checkbox, plus its continuation lines, is markdown. It may contain wikilinks `[[Note]]`, embeds `![[Note]]`, tags, dates and times (section 7).
- **List items without a checkbox** (`- text`).
  - **Open point C-2:** whether these are cards. Muninn keeps them as passthrough and does not show them as cards in 0.8.0.
- **Writing a new card.** `- [ ] ` + text. Continuation lines are indented by the indentation that the file's existing cards use. If there are none, Muninn uses two spaces.

## 6. Archive

- **Start.** The archive begins with a thematic-break line `***`, followed (after optional blank lines) by a level-2 heading whose text is the archive word `Archive`, or the archive word in another language (section 8).
- **Contents.** The archive's items are cards in the same syntax as lane cards.
- **A heading named "Archive" without the `***` line before it.**
  - **Open point A-1:** whether this is the archive or an ordinary lane. Muninn treats it as an ordinary lane, so it is never lost.
- **Archiving with a date.** Some boards are set to add the date to a card's text when it is archived. That added date is just part of the card text.

## 7. Dates, times and tags inside card text

This is user-visible text inside a card. In slice 1 it is kept as card text only and is not interpreted.

- **Date:** `@{2026-11-11}`, an at-sign then the date in braces.
- **Date as a link:** `@[[2026-11-11]]`, an at-sign then a wikilink to a daily note. It is also a normal wikilink and counts as one.
- **Time:** `@@{14:30}`, two at-signs then the time in braces.
- **Triggers.** The `@` and `@@` triggers can be changed per board in the settings footer.
  - **Open point D-1:** the footer setting names and value format for the triggers and for the date and time formats. To be specified before the dates slice.
- **Date format.** The text inside the braces follows a user-chosen format.
  - **Open point D-2:** the set of format tokens Muninn supports when displaying dates. Unknown tokens display literally.
- **Tag:** `#word`, as in ordinary Obsidian tags. Inside code it is not a tag.
- **Inline fields:** text like `[key:: value]`.
  - **Open point T-1:** to be specified before the metadata slice. Kept as text.

## 8. Marker words in other languages

The done-lane marker and the archive heading are written in the language of the Obsidian app that last wrote the file. Muninn recognises all of these when reading:

| Language | Done-lane marker | Archive heading |
|---|---|---|
| English (also every language not listed here) | `**Complete**` | `## Archive` |
| German | `**Fertiggestellt**` | `## Archiv` |
| Italian | `**Completato**` | `## Archivio` |
| Japanese | `**完了**` | `## アーカイブ` |
| Korean | `**완료됨**` | `## 보관됨` |
| Portuguese (Brazil) | `**Concluído**` | `## Arquivado` |
| Russian | `**Выполнено**` | `## Архивировать` |
| Chinese (Simplified) | `**完成**` | `## 归档` |

- **Writing.** Muninn keeps whatever word the file already uses. When it creates a marker that wasn't there, it writes the English word.
- **Open point M-1:** this list reflects one version of the other tool. If a file uses a marker word that is not listed, the line stays passthrough. The done-lane meaning is lost in Muninn, but no text is lost.

## 9. Settings footer

- **Shape.** The footer is the last part of the file, laid out like this:

  ````
  %% kanban:settings
  ```
  {"kanban-plugin":"board"}
  ```
  %%
  ````

  1. A line `%% kanban:settings`.
  2. A line of three backticks.
  3. One line holding a JSON object.
  4. A closing line of three backticks.
  5. A line `%%`.
- **Content.** The JSON object holds the board's own settings.
- **Slice 1 behaviour.**
  - The object is kept byte for byte.
  - Changing one key replaces only that key's value text, or appends the key at the end of the object, without reordering or reformatting the others.
  - Removing a key removes only that key and its separating comma.
- **Open point S-1:** the meaning of each settings key. It will be specified per slice, from public user documentation and observed behaviour. Unknown keys are always kept.
- **Malformed footer.** If the footer is unterminated or its JSON does not parse, the file is reported as unreadable as a board. It is never treated as an empty board, and Muninn never writes to it.

## 10. Round-trip and passthrough rules

1. **Unedited means identical.** Reading a kanban note and writing it back without edits produces the exact original bytes. Muninn does not write an unedited board at all.
2. **Minimal diffs.** An edit changes only the lines it is about. Moving a card removes its lines from one place and inserts the same lines in another. Renaming a lane changes one line. Toggling a done mark changes one character.
3. **Passthrough.** Every line Muninn does not model is kept exactly and stays in place relative to the lane or card before it. That includes paragraphs between lanes, other headings, comments, HTML and anything unrecognised.
4. **Fenced code.** A fence opens on a line whose first non-space characters are three or more backticks, or three or more tildes, and closes on a later line that is only that same mark, at least as many of them, then whitespace. A longer run closes a shorter one: a fence opened with three backticks closes on a line of three, four or five backticks and nothing else. Lines inside the fence are not lanes, cards, an archive or a settings footer, even when the text looks like those. A fence that starts on an indented continuation line belongs to that card and moves with it, but the card still ends at the first non-blank line that is not indented, even if the fence has not closed. That line is not part of the card. A column-0 task line after a closed indented fence is the next card. Any other fence is passthrough. A column-0 fence that never closes runs through the end of the file. Muninn does not read a settings footer out of it, and does not append one while the file still ends inside that fence.
5. **Whitespace is kept.** The file's line-ending style (LF or CRLF), a byte-order mark, the presence or absence of a final newline, indentation and blank-line runs are all kept. New lines use the file's line-ending style.
6. **Never lose data.** A file that can't be read as a board is left untouched and shown as a normal note.
7. **New file.** Muninn's "New Kanban" writes:

   ```
   ---

   kanban-plugin: board

   ---

   ## To do



   ## Doing



   ## Done


   ```

   There is no settings footer until a setting is changed. **Open point N-1:** the exact blank-line counts in that template. It must be confirmed to open in Obsidian Kanban; only the layout above is decided.

## 11. Known differences from the other tool

When the other tool saves a board, it may drop passthrough text it does not model, such as paragraphs between lanes. Muninn can't prevent that.

## 12. Public sources used for this spec

- Obsidian Help: properties/frontmatter, block references, tags (user documentation).
- The GitHub Flavored Markdown spec, task list items.
- The user-facing behaviour and the text the Obsidian Kanban plugin writes into files.

Honest provenance: Muninn's planning agent compiled this spec. While planning compatibility it read the plugin's GPL-3.0 source. This spec is the clean-room barrier. It is written in our own words and contains no code, regexes, identifiers, comments or test strings from the plugin; only behaviour and literal file text. Builders work from this spec alone.
