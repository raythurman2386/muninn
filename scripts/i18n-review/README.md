# i18n review tooling

Deterministic maintainer tooling for a Muninn language pack. It has no network
access, no third-party packages, and does not run on its own in the app.

It is a **triage aid, not a sign-off**: it catches omissions, broken
placeholders, localised file-format tokens, wrong numbers, missing keycaps,
glossary drift, length outliers and a few language-specific typography rules.
Semantic quality, tone and destructive-copy nuance still need a bilingual
reviewer (human or LLM).

## Workflow

Run the dump in a worktree that ships the target language (`Lang::SHIPPED`):

```sh
cargo run -q -p muninn-i18n --example catalog_dump -- --list
cargo run -q -p muninn-i18n --example catalog_dump -- es > /tmp/muninn-es.tsv
python3 scripts/i18n-review/review.py /tmp/muninn-es.tsv es /tmp/muninn-es-review.md
```

The TSV has one row per `Key` and `MsgKey`: `kind`, stable id, English, target.
Message placeholders are represented by control-character sentinels so the
dump can compare argument positions without guessing at a template.

The report groups findings into **critical** (placeholder/file-token/number/
pt-PT/French-apostrophe failures) and **warnings** (glossary, length, English
function words, terminal punctuation). Warnings are pointers for review, not
automatic failures.

## What it checks

- every `{n}` placeholder is present exactly once on both sides;
- file-format tokens (`YYYY-MM-DD`, `@{`, `@@{`, `MUNINN_*`, paths) survive;
- numeric literals match;
- keycaps/chords and modifier glyphs survive (`⌘`, `Ctrl`, `Shift`, `Enter`,
  `R D O A L P`, …);
- terminal punctuation and ellipses match;
- English function words do not leak into the target;
- the per-language glossary terms are used consistently;
- French prose uses U+2019 and NBSP before `: ; ! ?` / inside `« »`;
- pt-PT vocabulary does not leak into pt-BR (`ecrã`, `ficheiro`, `guardar`, …);
- target/English length ratios for short labels flag overflow risk.

The glossary maps live in `review.py`. A new language pack should add its map
there; the rest of the checks are language-agnostic.

## Tests

```sh
python3 -m unittest discover -s scripts/i18n-review -p 'test_*.py'
```

## Google Translate / other MT providers

Not included. The official Google Cloud Translation API needs a project key and
billing, and the undocumented web endpoint is not appropriate for repository
tooling. A maintainer can add an optional provider that reads a key from the
environment, but it must stay advisory: back-translation misses nuance and can
make a bad translation look plausible. Scraping the web endpoint violates its
terms and is intentionally unsupported.
