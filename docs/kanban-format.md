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
| **Date token** | A date in card text: `@{…}` or the link form `@[[…]]`. |
| **Time token** | A time in card text: `@@{…}`. |

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
  - A non-blank continuation keeps its whole lead in the card text when that lead does not begin with the card indent and is at least as wide, in columns, as the indent. Every other non-blank continuation is stored without the card indent. Writing the card puts the indent back in front of those lines. A blank line is empty, or it holds only spaces and tabs. A blank continuation is stored and written with nothing in front of it.
  - Editing a card whose narrowest lead is one space keeps that one-space indent, and an edited line is indented by one space. One space does not reach the two columns occupied by `- `, so after a blank line, or when the new line begins with `*` or `1.`, another reader can place that line outside the list item. Muninn still keeps it on the card, and reading the written card yields the same text. A card added to the file is indented by at least two columns, even when the file's indent is one space.
- **Block id.** A card's first line may end with a space, a caret and an identifier, such as ` ^card-7f2`. This is the block id that Obsidian uses for block links (see Obsidian's public help on block references). It belongs to the card, moves with it, and is not shown as card text.
  - An id is unique in the note: no two cards, in the lanes or the archive, share one. Adding or editing a card whose text names an id another card already has is refused, and the file is not changed.
  - Editing a card's text without an id keeps the card's id, so a `[[Note#^id]]` link still points at it. Giving the text a different id replaces the card's id and breaks links to the old one; that is the only edit that does.
- **Card text.** The text after the checkbox, plus its continuation lines, is markdown. It may contain wikilinks `[[Note]]`, embeds `![[Note]]`, tags, dates and times (section 7).
- **List items without a checkbox** (`- text`).
  - **Open point C-2:** whether these are cards. Muninn keeps them as passthrough and does not show them as cards in 0.8.0.
- **Writing a new card.** `- [ ] ` + text. The text is not blank. A line of only spaces or tabs is written as a blank continuation. The continuation indent is taken from the first card in file order that has one, walking the lanes and then the archive. If none do, Muninn uses two spaces. If that indent is only spaces and is narrower than two columns, a new card uses two spaces.
- **Where a new card goes.** In a lane with cards, a card inserted at a position goes before the card at that position, or after the last card when the position is the end. In a lane with no cards it goes after the lane's title, its done-lane marker and any text the lane holds before the next lane, archive or footer (section 10 rule 3). That includes indented text: a card is never placed directly above an indented line, because the reader would take that line as the card's continuation.
- **Block id in new text.** The text of a new or edited card is read with the same rule as the file: a space, a caret and an identifier at the end of its first line are the card's block id, not text. Muninn writes that id as the card's id, replacing an id the card already had. A caret anywhere else, including at the end of a continuation line, is text. Because the first line cannot end in that shape as text, an operation that copies a line into a new card without meaning to name an id (splitting a card) is refused whole rather than assigning one.

## 6. Archive

- **Start.** The archive begins with a thematic-break line `***`, followed (after optional blank lines) by a level-2 heading whose text is the archive word `Archive`, or the archive word in another language (section 8).
- **Contents.** The archive's items are cards in the same syntax as lane cards.
- **A heading named "Archive" without the `***` line before it.**
  - **Open point A-1:** whether this is the archive or an ordinary lane. Muninn treats it as an ordinary lane, so it is never lost.
- **Archiving with a date.** Some boards are set to add the date to a card's text when it is archived. That added date is just part of the card text.

## 7. Dates, times and tags inside card text

This is user-visible text inside a card. It is read for display and sorting
without changing the file. Only an edit the user asks for writes a token.

- **Date:** `@{2026-11-11}`, an at-sign then the date in braces.
- **Date as a link:** `@[[2026-11-11]]`, an at-sign then a wikilink to a daily note. It is also a normal wikilink and counts as one.
- **Time:** `@@{14:30}`, two at-signs then the time in braces.
- **Triggers.** The footer keys `date-trigger` (default `@`) and `time-trigger` (default `@@`) hold the literal text before the braces. A trigger is not a regular expression; an empty trigger is ignored. The longest trigger is tried first, so `@@` is a time and not an `@` date followed by `@`.
  - **D-1 (resolved).** Trigger and format setting keys and values are given in section 9. A trigger has no word-boundary rule in the file; any `@` followed by `{…}` or `[[…]]` is a date token. The card editor opens the picker only at the start of the text or after whitespace.
- **Reading.** A token is read outside fenced code and inline code (section 10 rule 4). Inline code is a run of backticks up to the next run of the same length; an unmatched run is ordinary text. A token that never closes, or whose braces or brackets hold nothing, is ordinary text. The first date and the first time in the text are the card's values for display and sorting. The first time pairs with the first date even when the time appears earlier.
- **Date format.** `date-format` (default `YYYY-MM-DD`) says how a new date is written inside the braces. `time-format` (default `HH:mm`) does the same for a time. Whitespace around a format is ignored, so ` DD.MM.YYYY` and `DD.MM.YYYY ` write the same token.
- **Format tokens.** Both formats use these tokens. Tokens are case-sensitive. A `[...]` block is literal text, so `[at ]HH:mm` shows `at 14:30`. Any character that is not a token, and any unknown token, is copied as written.

  | Token | Meaning | Example for 2026-11-12 14:30:05 |
  |---|---|---|
  | `YYYY` | 4-digit year | `2026` |
  | `YY` | 2-digit year | `26` |
  | `MMMM` | Full English month | `November` |
  | `MMM` | 3-letter English month | `Nov` |
  | `MM` / `M` | Month, padded / not | `11` / `11` |
  | `DD` / `D` | Day of month, padded / not | `12` / `12` |
  | `Do` | Day with an English ordinal | `12th` |
  | `dddd` | Full English weekday | `Thursday` |
  | `ddd` / `dd` | 3- / 2-letter English weekday | `Thu` / `Th` |
  | `d` | Weekday number, Sunday is 0 | `4` |
  | `HH` / `H` | Hour, 24-hour, padded / not | `14` / `14` |
  | `hh` / `h` | Hour, 12-hour, padded / not | `02` / `2` |
  | `mm` / `m` | Minute, padded / not | `30` / `30` |
  | `ss` / `s` | Second, padded / not | `05` / `5` |
  | `A` / `a` | `AM`/`PM` or `am`/`pm` | `PM` / `pm` |

  Month and weekday names are English; Muninn has no locale table. The
  2-digit year `69`–`99` is 1969–1999 and `00`–`68` is 2000–2068 when a
  written date is read back for sorting.
  - **D-2 (resolved).** This token set is the one Muninn supports. Unknown
    tokens stay literal, which also means a stray `a` outside `[...]` is the
    morning/afternoon token; write `[a]` for a literal letter.
- **Display.** `date-display-format` (default `YYYY-MM-DD`) formats the date
  row at the card's foot; a time is shown with `time-format`.
  `show-relative-date` (default `false`) adds `today`, `tomorrow`,
  `yesterday`, `in N days` or `N days ago`, counted from the local calendar
  day. `hide-date-in-title` (default
  `true`) keeps the first date and time tokens out of the card's main text;
  they still appear at the foot. Display never rewrites the file.
  `link-date-to-daily-note` (default `false`) makes the displayed date a link
  to a note named by the token's inner text. A link-form date is already a
  link. The token's bytes in the file are unchanged either way.
- **Sorting by date.** Sorting uses the parsed day and time, not the shown
  string: a `DD.MM.YYYY` date sorts by its day, not by the written digits.
  A card with no date sorts after every dated card, in its original order.
- **Editing.** A picker opened from a card's menu writes one token for that
  kind: if the card already has a date (or time) token, the new token
  replaces it in place; otherwise it is appended with a space when needed.
  Replacing a link-form date keeps the link form, and the target's folder,
  `#heading` and `|alias` text stay as written with only the date part
  updated. A picker opened from the card editor replaces the typed trigger at
  the caret, so an editor can add a token next to an existing one. The
  picker's Clear removes the card's date or time token (in the card editor,
  the token in the text being edited); nothing else in the card changes.
- **Tag:** `#word`, as in ordinary Obsidian tags. A tag starts at the start of
  a line or after whitespace, and its characters are letters (any script),
  digits, `_`, `-` and `/`. `#parent/child` is one tag. A tag that is only
  digits (`#123`) is not a tag, a heading marker (`# Heading`, `##`) is not a
  tag, and a `#` inside a wikilink target (`[[Note#heading]]`) is not a tag.
  Inside code (section 10 rule 4) it is not a tag either. Case is kept for
  display and matching is case-insensitive, so `#Ops` and `#ops` are one tag
  and the first spelling seen is the one shown.
  - **Frontmatter.** A note's `tags:` or `tag:` property also contributes
    tags, as a scalar (`tags: work`, comma- or space-separated) or a list
    (`tags:` then `- work` lines, or `tags: [work, home]`). Quoted values are
    unquoted. The property is read-only here and is never rewritten by a
    display setting.
  - **Kanban cards.** A card's tags are the note body's tags: a card is a
    line of the same file, so it needs no separate rule. The vault index
    reads ordinary notes, kanban cards and frontmatter the same way.
  - **Display.** `hide-tags-in-title` (default `false`) keeps the tags out of
    the card's main text and `show-tags-at-bottom` (default `false`) draws
    tag chips at the card's foot. Display never rewrites the file. Clicking a
    chip fills the Find bar with `#tag`; it does not filter cards in this
    version. The Find bar searches the note's text buffer, so after unsaved
    card edits its matches can lag the board until the edit is saved.
  - **Completion.** In the card editor, typing `#` at a word boundary offers
    the vault's tags, like `[[` offers notes. Accepting inserts `#tag`; Escape
    closes the list and leaves the typed `#`. `#` inside a link target, a URL,
    an email or a heading marker does not open the list.
- **Inline fields:** `[key:: value]` or a line that begins `key:: value`.
  - **T-1 (resolved).** A field's key is the text before `::`, with
    surrounding space removed; the value is the rest of the line, trimmed and
    kept as markdown. A bracket field ends at the first `]` outside inline code
    and outside a `[[wikilink]]`; a line field ends at the line. Fields inside fenced
    code or inline code are ordinary text, not fields. Matching is
    case-insensitive and the first occurrence of a key wins: frontmatter is
    read before the body, and within each, file order decides. A later
    duplicate is ignored for display. A frontmatter list value is shown as
    its items joined with `, `. Inline fields and their values are
    display-only: Muninn never writes, rewrites or normalises one.
- **Metadata from a linked note.** A card's first wikilink selects the note
  to read: `[[Note]]`, `![[Note]]`, `[[Note#Heading]]`, `[[Note#^id]]` and
  `[[Note|alias]]` all select `Note`. The first link is the only one; a
  missing target draws nothing and a later link does not stand in for it. For
  each key in the board's `linked-page-metadata` setting (section 9), the
  value from the linked note's frontmatter or an inline field is shown at the
  card's foot, after the dates and tags, as `key: value`. `containsMarkdown`
  renders the value through the reader: a link works, a note embed becomes a
  link, and an attachment embed keeps the reader's filename placeholder;
  otherwise it is plain text. A missing note or key draws nothing, unless
  `show-missing-metadata` asks for `key: —`. This is display-only: viewing,
  scrolling or a dirty board never changes the card, the linked note or any
  other byte.

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
  - When the object has the same key more than once, a JSON reader uses the last member. Muninn changes that last member's value. Removing the key removes every member with that key, so no earlier copy becomes effective. Earlier copies keep their bytes.
- **Open point S-1:** the meaning of each settings key. Keys are specified per slice, from public user documentation and observed behaviour. `list-collapse` and the date/time keys below are specified; other keys stay open. Unknown keys are always kept.
- **Settings key `list-collapse`.** The value is a JSON array of booleans with one entry per lane, in file order. `true` means the lane is shown collapsed. A missing key, a short array, or a non-boolean entry means "not collapsed" for that lane. When Muninn changes collapse state, it writes only this key's value. It keeps every other key, and the key order, byte-for-byte. If the key was absent, it is appended as the last key. When a lane is added, deleted or moved, the array is kept aligned with the lanes.
- **Settings keys for dates, times and tags.** These are the keys Muninn reads from the footer and from the app-wide defaults. Values of the wrong JSON type, and unknown keys, are ignored and kept as they are.

  | Key | Type | Built-in default | Meaning |
  |---|---|---|---|
  | `date-trigger` | string | `"@"` | Text before a date token. |
  | `time-trigger` | string | `"@@"` | Text before a time token. |
  | `date-format` | string | `"YYYY-MM-DD"` | Format written inside a new date token. |
  | `time-format` | string | `"HH:mm"` | Format written inside a new time token, and shown for a time. |
  | `date-display-format` | string | `"YYYY-MM-DD"` | Format of the date row at the card's foot. |
  | `show-relative-date` | boolean | `false` | Add `today` / `in N days` beside the date. |
  | `hide-date-in-title` | boolean | `true` | Keep the date and time tokens out of the card's main text. |
  | `link-date-to-daily-note` | boolean | `false` | Make the shown date a link to its daily note. |
  | `hide-tags-in-title` | boolean | `false` | Keep `#tag` tokens out of the card's main text. |
  | `show-tags-at-bottom` | boolean | `false` | Draw tag chips at the card's foot. |
  | `tag-colors` | array of objects | absent | A tag chip's colour, as `{ "tagKey": "work", "color": "1" }`. Kept as written; Muninn has no colour editor yet and viewing never rewrites it. |
  | `linked-page-metadata` | array of objects | absent | Keys to show from a card's first linked note, in written order, as `{ "key": "status", "containsMarkdown": false }`. An empty array, or an absent key, draws no metadata rows. A malformed entry is skipped. |
  | `show-missing-metadata` | boolean | `false` | Draw `key: —` for a configured key the linked note lacks, and for every configured key when the card's first link has no note. |

- **App-wide defaults.** `settings.json` may hold a `kanban_defaults` object
  with the same keys, including `linked-page-metadata` and
  `show-missing-metadata` (with the same wrong-type rule: an unknown key and
  a known key with the wrong JSON type are kept as written). It applies to
  every board that has no value of its own.
  A key is resolved in this order: the board's settings footer, then
  `kanban_defaults`, then the built-in default in the table. An absent
  `kanban_defaults`, or an absent key in it, means the built-in default. The
  object is written with the keys that are set; a key this version does not
  know, and a known key with the wrong JSON type, is kept as written.
- **When a footer key is written.** The date, time and tag display keys are
  read only; they are not rewritten by viewing or by an unrelated card edit.
  A picker adds its token to the card text, not to the footer. The footer
  changes only when a later settings panel writes one key, and then only that
  key's value changes (as for `list-collapse`).
- **Malformed footer.** If the footer is unterminated or its JSON does not parse, the file is reported as unreadable as a board. It is never treated as an empty board, and Muninn never writes to it.

## 10. Round-trip and passthrough rules

1. **Unedited means identical.** Reading a kanban note and writing it back without edits produces the exact original bytes. Muninn does not write an unedited board at all.
2. **Minimal diffs.** An edit changes only the lines it is about. Moving a card removes its lines from one place and inserts the same lines in another. Renaming a lane changes one line. Toggling a done mark changes one character.
3. **Passthrough.** Every line Muninn does not model is kept exactly and stays in place relative to the lane or card before it. That includes paragraphs between lanes, other headings, comments, HTML and anything unrecognised.
4. **Fenced code.** A fence opens on a line whose first non-space characters are three or more backticks, or three or more tildes, and closes on a later line that is only that same mark, at least as many of them, then whitespace. A longer run closes a shorter one: a fence opened with three backticks closes on a line of three, four or five backticks and nothing else. Lines inside the fence are not lanes, cards, an archive or a settings footer, even when the text looks like those. A fence that starts on an indented continuation line belongs to that card and moves with it, but the card still ends at the first non-blank line that is not indented, even if the fence has not closed. That line is not part of the card. A column-0 task line after a closed indented fence is the next card. Any other fence is passthrough. A column-0 fence that never closes runs through the end of the file. Muninn does not read a settings footer out of it, and does not append one while the file still ends inside that fence.
5. **Whitespace is kept.** The file's line-ending style (LF or CRLF), a byte-order mark, the presence or absence of a final newline, indentation and blank-line runs are all kept. New lines use the file's line-ending style. A lane moved to the end of a file with no final newline keeps that ending: the moved lane's trailing blank lines are dropped there rather than adding a newline after them.
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
