# Markdown

Notes are CommonMark with GFM extras. The preview is the `rune-kit` Markdown element, so the
same parser that renders chat messages renders a note.

## Blocks

> Quotes get a rule and a quieter voice.
> They can hold **strong**, *emphasis* and `inline code`.

- unordered lists
- with a second item
  - and a nested one

1. ordered lists
2. count too

- [x] task lists
- [ ] work in the preview

## Code

```rust
fn main() {
    println!("fenced code is not a wikilink: [[Welcome]]");
}
```

A horizontal rule follows.

---

External links open in the browser; internal links use the `note:` scheme. See [[Wikilinks]]
and [[Code Blocks]].
