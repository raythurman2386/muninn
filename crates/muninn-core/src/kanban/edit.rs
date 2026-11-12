//! Edits splice byte ranges and re-parse.
//!
//! Each method builds the next source from the current one, changing only the
//! span the edit is about, then parses that string. A parse failure leaves
//! the model as it was. New lines use the file's line ending.

use std::io;
use std::ops::Range;

use serde_json::Value;

use super::{Card, Kanban, invalid_data, invalid_input};

const ENGLISH_MARKER: &str = "**Complete**";

impl Kanban {
    /// Insert a card at `index` in `lane` (`index == cards.len()` appends).
    ///
    /// The new line is `- [ ] ` plus `text`. A newline in `text` starts a
    /// continuation, indented the way existing cards in the file indent
    /// theirs, or by two spaces when none do.
    pub fn add_card(&mut self, lane: usize, index: usize, text: &str) -> io::Result<()> {
        let at = self.insert_offset(lane, index)?;
        let indent = self.continuation_indent();
        let block = render_card(self.nl, ' ', text, None, &indent);
        self.commit(insert_block(&self.source, at, block, self.nl))
    }

    /// Replace a card's markdown. The checkbox character and the block id
    /// stay. Continuation lines are indented like the rest of the file.
    pub fn edit_card(&mut self, lane: usize, index: usize, text: &str) -> io::Result<()> {
        let card = self.card(lane, index)?;
        let indent = card
            .indent
            .clone()
            .unwrap_or_else(|| self.continuation_indent());
        let block = render_card(self.nl, card.mark, text, card.block_id.as_deref(), &indent);
        let span = card.span.clone();
        let block = keep_open_ending(&self.source, span.end, block, self.nl);
        self.commit(splice(&self.source, span, &block))
    }

    /// Remove one card's lines. Surrounding blank lines stay.
    pub fn delete_card(&mut self, lane: usize, index: usize) -> io::Result<()> {
        let span = self.card(lane, index)?.span.clone();
        self.commit(splice(&self.source, span, ""))
    }

    /// Move a card. `to_index` is the index the card has in the destination
    /// lane after the move: `0` is first, and the last valid index puts it
    /// last (`cards.len()` when the destination is another lane,
    /// `cards.len() - 1` when it is the same lane, because the card is taken
    /// out first).
    ///
    /// The card's own lines are inserted unchanged.
    pub fn move_card(
        &mut self,
        from_lane: usize,
        from_index: usize,
        to_lane: usize,
        to_index: usize,
    ) -> io::Result<()> {
        if from_lane == to_lane && from_index == to_index {
            return Ok(());
        }
        let from_len = self.lane_len(from_lane)?;
        let to_len = self.lane_len(to_lane)?;
        if from_index >= from_len {
            return Err(invalid_input("no such card"));
        }
        let max = if from_lane == to_lane {
            from_len - 1
        } else {
            to_len
        };
        if to_index > max {
            return Err(invalid_input("card index out of range"));
        }
        let raw = self.card(from_lane, from_index)?.span.clone();
        let block = self.source[raw.clone()].to_owned();
        let removed = splice(&self.source, raw, "");
        let parsed = Kanban::parse(&removed)?;
        let at = parsed.insert_offset(to_lane, to_index)?;
        self.commit(insert_block(&removed, at, block, parsed.nl))
    }

    /// Insert a lane at `index` (`index == lanes.len()` appends, before an
    /// archive or footer).
    ///
    /// The new text is `## Title` or `## Title (N)`, a blank line, the
    /// English done-lane marker when `done` is set, then two blank lines. A
    /// limit of `0` is not written. Existing lanes are not re-spaced.
    pub fn add_lane(
        &mut self,
        index: usize,
        title: &str,
        limit: Option<u32>,
        done: bool,
    ) -> io::Result<()> {
        if title.contains(['\n', '\r']) {
            return Err(invalid_input("a lane title is one line"));
        }
        if index > self.lanes.len() {
            return Err(invalid_input("lane index out of range"));
        }
        let at = self.lane_insert_offset(index)?;
        let open_ended =
            at == self.source.len() && !self.source.is_empty() && !self.source.ends_with('\n');
        let block = lane_block(self.nl, title, limit, done, open_ended);
        if open_ended {
            self.commit(splice(&self.source, at..at, &block))
        } else {
            self.commit(insert_block(&self.source, at, block, self.nl))
        }
    }

    /// Replace the lane title. A ` (N)` limit on the same line stays.
    pub fn rename_lane(&mut self, index: usize, title: &str) -> io::Result<()> {
        if title.contains(['\n', '\r']) {
            return Err(invalid_input("a lane title is one line"));
        }
        let span = self.lane(index)?.title_span.clone();
        self.commit(splice(&self.source, span, title))
    }

    /// Remove a lane, from its title line up to the next lane, the archive,
    /// or the footer. Other lanes' bytes are unchanged.
    pub fn delete_lane(&mut self, index: usize) -> io::Result<()> {
        let span = self.lane(index)?.span.clone();
        self.commit(splice(&self.source, span, ""))
    }

    /// Set the lane limit. `None` and `Some(0)` both clear a written suffix
    /// and never write `(0)`.
    pub fn set_lane_limit(&mut self, index: usize, limit: Option<u32>) -> io::Result<()> {
        let lane = self.lane(index)?;
        let limit = limit.filter(|n| *n > 0);
        let next = if let Some(limit) = limit {
            if let Some(number) = lane.limit_number.clone() {
                splice(&self.source, number, &limit.to_string())
            } else {
                let at = content_end(&self.source, lane.title_line.clone());
                splice(&self.source, at..at, &format!(" ({limit})"))
            }
        } else if let Some(suffix) = lane.limit_suffix.clone() {
            splice(&self.source, suffix, "")
        } else {
            return Ok(());
        };
        self.commit(next)
    }

    /// Flip the checkbox. An open card becomes `x`; any done character
    /// becomes a space. One character changes.
    pub fn toggle_done(&mut self, lane: usize, index: usize) -> io::Result<()> {
        let card = self.card(lane, index)?;
        let at = card.mark_at;
        let len = card.mark.len_utf8();
        let replacement = if card.mark == ' ' { "x" } else { " " };
        self.commit(splice(&self.source, at..at + len, replacement))
    }

    /// Set one footer key. An existing value is replaced in place; a new key
    /// is appended before the object's closing brace. Other keys keep their
    /// text and order. A file with no footer gains one.
    pub fn set_setting(&mut self, key: &str, value: &Value) -> io::Result<()> {
        if key.is_empty() {
            return Err(invalid_input("a setting key is not empty"));
        }
        if self.ends_in_fence && self.footer.is_none() {
            return Err(invalid_data("the note ends inside an unclosed code fence"));
        }
        if let Some(span) = self.footer.as_ref().map(|footer| footer.json_span.clone()) {
            let json = set_json_key(&self.source[span.clone()], key, value)?;
            self.commit(splice(&self.source, span, &json))
        } else {
            let key_text =
                serde_json::to_string(key).map_err(|error| invalid_input(&error.to_string()))?;
            let value_text =
                serde_json::to_string(value).map_err(|error| invalid_input(&error.to_string()))?;
            let object = format!("{{{key_text}:{value_text}}}");
            self.commit(append_footer(&self.source, self.nl, &object))
        }
    }

    /// Remove one footer key and the comma that separates it. The other keys
    /// stay as they were. Errors when the footer or the key is absent.
    pub fn remove_setting(&mut self, key: &str) -> io::Result<()> {
        let Some(span) = self.footer.as_ref().map(|footer| footer.json_span.clone()) else {
            return Err(invalid_input("this note has no settings footer"));
        };
        let json = remove_json_key(&self.source[span.clone()], key)?;
        self.commit(splice(&self.source, span, &json))
    }

    fn commit(&mut self, next: String) -> io::Result<()> {
        let parsed = Kanban::parse(&next)?;
        *self = parsed;
        Ok(())
    }

    fn lane(&self, index: usize) -> io::Result<&super::Lane> {
        self.lanes
            .get(index)
            .ok_or_else(|| invalid_input("no such lane"))
    }

    fn lane_len(&self, index: usize) -> io::Result<usize> {
        Ok(self.lane(index)?.cards.len())
    }

    fn card(&self, lane: usize, index: usize) -> io::Result<&Card> {
        self.lane(lane)?
            .cards
            .get(index)
            .ok_or_else(|| invalid_input("no such card"))
    }

    /// Byte offset at which a card inserted at `index` should start.
    fn insert_offset(&self, lane: usize, index: usize) -> io::Result<usize> {
        let lane = self.lane(lane)?;
        if index > lane.cards.len() {
            return Err(invalid_input("card index out of range"));
        }
        if let Some(card) = lane.cards.get(index) {
            return Ok(card.span.start);
        }
        if let Some(last) = lane.cards.last() {
            return Ok(last.span.end);
        }
        let after = lane.marker_end.unwrap_or(lane.title_line.end);
        if self.source[after..lane.span.end].starts_with(self.nl) {
            Ok(after + self.nl.len())
        } else {
            Ok(after)
        }
    }

    fn lane_insert_offset(&self, index: usize) -> io::Result<usize> {
        if let Some(lane) = self.lanes.get(index) {
            return Ok(lane.span.start);
        }
        if let Some(archive) = &self.archive {
            return Ok(archive.span.start);
        }
        if let Some(footer) = &self.footer {
            return Ok(footer.span.start);
        }
        Ok(self.source.len())
    }

    fn continuation_indent(&self) -> String {
        let found = self
            .lanes
            .iter()
            .flat_map(|lane| lane.cards.iter())
            .chain(self.archive.iter().flat_map(|archive| archive.cards.iter()))
            .find_map(|card| card.indent.clone());
        found.unwrap_or_else(|| "  ".to_owned())
    }
}

/// The bytes of a new lane. `open_ended` is a file with no final newline:
/// two blank lines before the heading, and no new final newline.
fn lane_block(nl: &str, title: &str, limit: Option<u32>, done: bool, open_ended: bool) -> String {
    let mut block = String::new();
    if open_ended {
        block.push_str(nl);
        block.push_str(nl);
        block.push_str(nl);
    }
    block.push_str("## ");
    block.push_str(title);
    if let Some(limit) = limit.filter(|n| *n > 0) {
        block.push_str(&format!(" ({limit})"));
    }
    if open_ended {
        if done {
            block.push_str(nl);
            block.push_str(nl);
            block.push_str(ENGLISH_MARKER);
        }
        return block;
    }
    block.push_str(nl);
    block.push_str(nl);
    if done {
        block.push_str(ENGLISH_MARKER);
        block.push_str(nl);
    }
    block.push_str(nl);
    block.push_str(nl);
    block
}

fn render_card(nl: &str, mark: char, text: &str, block_id: Option<&str>, indent: &str) -> String {
    let text = text.replace('\r', "");
    let mut lines = text.split('\n');
    let first = lines.next().unwrap_or("");
    let mut out = format!("- [{mark}] {first}");
    if let Some(id) = block_id.filter(|id| !id.is_empty()) {
        out.push(' ');
        out.push('^');
        out.push_str(id);
    }
    out.push_str(nl);
    for line in lines {
        if !line.is_empty() {
            out.push_str(indent);
        }
        out.push_str(line);
        out.push_str(nl);
    }
    out
}

fn splice(source: &str, range: Range<usize>, with: &str) -> String {
    let mut next = String::with_capacity(source.len() - (range.end - range.start) + with.len());
    next.push_str(&source[..range.start]);
    next.push_str(with);
    next.push_str(&source[range.end..]);
    next
}

/// Put `block` at `at`. A block landing mid-line gets a leading newline, and
/// a block that would swallow the following line gets a trailing one.
fn insert_block(source: &str, at: usize, mut block: String, nl: &str) -> String {
    if at > 0 && !source[..at].ends_with('\n') {
        block.insert_str(0, nl);
    }
    if at < source.len() && !block.ends_with('\n') {
        block.push_str(nl);
    }
    if at == source.len() && !source.is_empty() && !source.ends_with('\n') && block.ends_with(nl) {
        block.truncate(block.len() - nl.len());
    }
    splice(source, at..at, &block)
}

/// A card that was the unterminated last line stays unterminated when rewritten.
fn keep_open_ending(source: &str, span_end: usize, mut block: String, nl: &str) -> String {
    if span_end == source.len() && !source.ends_with('\n') && block.ends_with(nl) {
        block.truncate(block.len() - nl.len());
    }
    block
}

fn content_end(source: &str, line: Range<usize>) -> usize {
    let text = &source[..line.end];
    if text.ends_with("\r\n") {
        line.end - 2
    } else if text.ends_with('\n') {
        line.end - 1
    } else {
        line.end
    }
}

fn append_footer(source: &str, nl: &str, json: &str) -> String {
    let mut block = String::new();
    let terminated = source.ends_with('\n');
    let blank = source.ends_with(&format!("{nl}{nl}"));
    if !source.is_empty() && !terminated {
        block.push_str(nl);
    }
    if !source.is_empty() && !blank {
        block.push_str(nl);
    }
    for line in ["%% kanban:settings", "```", json, "```", "%%"] {
        block.push_str(line);
        block.push_str(nl);
    }
    if !source.is_empty() && !terminated {
        block.truncate(block.len() - nl.len());
    }
    let mut next = String::with_capacity(source.len() + block.len());
    next.push_str(source);
    next.push_str(&block);
    next
}

struct Member {
    key: String,
    /// Byte offset of the key's opening quote.
    key_at: usize,
    value: Range<usize>,
    /// Comma that precedes this member, when it is not the first.
    comma_before: Option<usize>,
    /// Comma that follows this member, when it is not the last.
    comma_after: Option<usize>,
}

fn set_json_key(line: &str, key: &str, value: &Value) -> io::Result<String> {
    let (members, close) = scan_object(line)?;
    let encoded =
        serde_json::to_string(value).map_err(|error| invalid_input(&error.to_string()))?;
    if let Some(member) = members.iter().find(|member| member.key == key) {
        return Ok(splice(line, member.value.clone(), &encoded));
    }
    let key_text = serde_json::to_string(key).map_err(|error| invalid_input(&error.to_string()))?;
    let insertion = if members.is_empty() {
        format!("{key_text}:{encoded}")
    } else {
        format!(",{key_text}:{encoded}")
    };
    Ok(splice(line, close..close, &insertion))
}

fn remove_json_key(line: &str, key: &str) -> io::Result<String> {
    let (members, _) = scan_object(line)?;
    let Some(index) = members.iter().position(|member| member.key == key) else {
        return Err(invalid_input("no such setting"));
    };
    let member = &members[index];
    let range = if let Some(comma) = member.comma_before {
        comma..member.value.end
    } else if let Some(comma) = member.comma_after {
        member.key_at..comma + 1
    } else {
        member.key_at..member.value.end
    };
    Ok(splice(line, range, ""))
}

/// Members of a JSON object line, and the byte offset of its closing `}`.
fn scan_object(line: &str) -> io::Result<(Vec<Member>, usize)> {
    let bytes = line.as_bytes();
    let mut index = skip_ws(line, 0);
    if bytes.get(index) != Some(&b'{') {
        return Err(invalid_data("settings footer JSON must be an object"));
    }
    index += 1;
    let mut members = Vec::new();
    loop {
        index = skip_ws(line, index);
        if bytes.get(index) == Some(&b'}') {
            return Ok((members, index));
        }
        if bytes.get(index) != Some(&b'"') {
            return Err(invalid_data("settings footer JSON does not parse"));
        }
        let key_at = index;
        let key_end = string_end(line, index)?;
        let key: String = serde_json::from_str(&line[index..key_end])
            .map_err(|_| invalid_data("settings footer JSON does not parse"))?;
        index = skip_ws(line, key_end);
        if bytes.get(index) != Some(&b':') {
            return Err(invalid_data("settings footer JSON does not parse"));
        }
        index = skip_ws(line, index + 1);
        let value_start = index;
        index = value_end(line, index)?;
        let value = value_start..index;
        index = skip_ws(line, index);
        let comma_after = if bytes.get(index) == Some(&b',') {
            let at = index;
            index += 1;
            Some(at)
        } else {
            None
        };
        let comma_before = members
            .last()
            .and_then(|member: &Member| member.comma_after);
        members.push(Member {
            key,
            key_at,
            value,
            comma_before,
            comma_after,
        });
        if comma_after.is_none() {
            index = skip_ws(line, index);
            if bytes.get(index) != Some(&b'}') {
                return Err(invalid_data("settings footer JSON does not parse"));
            }
            return Ok((members, index));
        }
    }
}

fn skip_ws(line: &str, mut index: usize) -> usize {
    while let Some(ch) = line[index..].chars().next() {
        if !ch.is_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

fn string_end(line: &str, start: usize) -> io::Result<usize> {
    let bytes = line.as_bytes();
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => {
                index += 1;
                if index >= bytes.len() {
                    break;
                }
                index += 1;
            }
            b'"' => return Ok(index + 1),
            byte if byte < 0x80 => index += 1,
            _ => {
                index += line[index..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(1);
            }
        }
    }
    Err(invalid_data("settings footer JSON does not parse"))
}

fn value_end(line: &str, index: usize) -> io::Result<usize> {
    let bytes = line.as_bytes();
    let Some(kind) = bytes.get(index).copied() else {
        return Err(invalid_data("settings footer JSON does not parse"));
    };
    match kind {
        b'"' => string_end(line, index),
        b'{' => container_end(line, index, true),
        b'[' => container_end(line, index, false),
        b't' => literal_end(line, index, "true"),
        b'f' => literal_end(line, index, "false"),
        b'n' => literal_end(line, index, "null"),
        b'-' | b'0'..=b'9' => number_end(line, index),
        _ => Err(invalid_data("settings footer JSON does not parse")),
    }
}

fn literal_end(line: &str, index: usize, literal: &str) -> io::Result<usize> {
    if line[index..].starts_with(literal) {
        Ok(index + literal.len())
    } else {
        Err(invalid_data("settings footer JSON does not parse"))
    }
}

fn number_end(line: &str, index: usize) -> io::Result<usize> {
    let bytes = line.as_bytes();
    let mut cursor = index;
    while cursor < bytes.len()
        && matches!(
            bytes[cursor],
            b'+' | b'-' | b'.' | b'e' | b'E' | b'0'..=b'9'
        )
    {
        cursor += 1;
    }
    if cursor == index {
        return Err(invalid_data("settings footer JSON does not parse"));
    }
    Ok(cursor)
}

/// End offset of a `{...}` or `[...]` value. Objects are key/value members;
/// arrays are values. Nested containers go back through [`value_end`].
fn container_end(line: &str, index: usize, object: bool) -> io::Result<usize> {
    let close = if object { b'}' } else { b']' };
    let bytes = line.as_bytes();
    let mut cursor = skip_ws(line, index + 1);
    if bytes.get(cursor) == Some(&close) {
        return Ok(cursor + 1);
    }
    loop {
        if object {
            if bytes.get(cursor) != Some(&b'"') {
                return Err(invalid_data("settings footer JSON does not parse"));
            }
            cursor = string_end(line, cursor)?;
            cursor = skip_ws(line, cursor);
            if bytes.get(cursor) != Some(&b':') {
                return Err(invalid_data("settings footer JSON does not parse"));
            }
            cursor = skip_ws(line, cursor + 1);
        }
        cursor = value_end(line, cursor)?;
        cursor = skip_ws(line, cursor);
        match bytes.get(cursor).copied() {
            Some(b',') => {
                cursor = skip_ws(line, cursor + 1);
                if bytes.get(cursor) == Some(&close) {
                    return Err(invalid_data("settings footer JSON does not parse"));
                }
            }
            Some(byte) if byte == close => return Ok(cursor + 1),
            _ => return Err(invalid_data("settings footer JSON does not parse")),
        }
    }
}
