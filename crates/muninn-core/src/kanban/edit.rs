//! Edits splice byte ranges and re-parse.
//!
//! Each method builds the next source from the current one, changing only the
//! span the edit is about, then parses that string. A parse failure leaves
//! the model as it was. New lines use the file's line ending.

use std::io;
use std::ops::Range;

use serde_json::Value;

use super::parse::{
    fence_closes, indent_columns, is_blank, is_indented, line_keeps_lead, opens_fence,
    split_block_id,
};
use super::{Card, Kanban, LIST_COLLAPSE, invalid_data, invalid_input};

const ENGLISH_MARKER: &str = "**Complete**";

impl Kanban {
    /// Insert a card at `index` in `lane` (`index == cards.len()` appends).
    ///
    /// The new line is `- [ ] ` plus `text`. A trailing ` ^id` on its first
    /// line is the card's block id, not text. A newline in `text` starts a
    /// continuation, indented the way existing cards in the file indent
    /// theirs, or by two spaces when none do. In a lane with no cards the
    /// card goes after the lane's leading passthrough text.
    pub fn add_card(&mut self, lane: usize, index: usize, text: &str) -> io::Result<()> {
        let (text, block_id) = card_text_and_id(&normalised_card_text(text));
        let at = self.insert_offset(lane, index)?;
        let indent = padded_new_card_indent(self.continuation_indent());
        let block = render_card(self.nl, ' ', &text, block_id.as_deref(), &indent);
        let next = insert_block(&self.source, at, block, self.nl);
        self.accept_card(next, lane, index, &text, block_id.as_deref())
    }

    /// Replace a card's markdown. The checkbox character stays. A trailing
    /// ` ^id` on the first line replaces the card's block id; without one the
    /// id stays. Continuation lines are indented like the rest of the file.
    pub fn edit_card(&mut self, lane: usize, index: usize, text: &str) -> io::Result<()> {
        let card = self.card(lane, index)?;
        let (text, requested_id) = card_text_and_id(&normalised_card_text(text));
        let block_id = requested_id.or_else(|| card.block_id.clone());
        if text == card.text && block_id.as_deref() == card.block_id.as_deref() {
            return Ok(());
        }
        let mark = card.mark;
        let span = card.span.clone();
        let indent = card
            .indent
            .clone()
            .unwrap_or_else(|| self.continuation_indent());
        let block = render_card(self.nl, mark, &text, block_id.as_deref(), &indent);
        let block = keep_open_ending(&self.source, span.end, block, self.nl);
        let next = splice(&self.source, span, &block);
        self.accept_card(next, lane, index, &text, block_id.as_deref())
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
        let card = self.card(from_lane, from_index)?;
        let text = card.text.clone();
        let block_id = card.block_id.clone();
        let raw = card.span.clone();
        let block = self.source[raw.clone()].to_owned();
        let removed = splice(&self.source, raw, "");
        let parsed = Kanban::parse(&removed)?;
        let at = parsed.insert_offset(to_lane, to_index)?;
        let next = insert_block(&removed, at, block, parsed.nl);
        self.accept_card(next, to_lane, to_index, &text, block_id.as_deref())
    }

    /// Insert a lane at `index` (`index == lanes.len()` appends, before an
    /// archive or footer).
    ///
    /// The new text is `## Title` or `## Title (N)`, a blank line, the
    /// English done-lane marker when `done` is set, then a blank line and one
    /// more when a lane follows (a lane appended after the last ends as New
    /// Kanban's last lane does). A limit of `0` is not written. A title that
    /// ends in ` (N)` is refused, and so is a note that ends inside an open
    /// fence. Existing lanes are not re-spaced. A `list-collapse` setting
    /// gains a `false` at `index`, so later lanes keep their entries.
    pub fn add_lane(
        &mut self,
        index: usize,
        title: &str,
        limit: Option<u32>,
        done: bool,
    ) -> io::Result<()> {
        check_lane_title(title)?;
        self.refuse_open_fence()?;
        if index > self.lanes.len() {
            return Err(invalid_input("lane index out of range"));
        }
        let at = self.lane_insert_offset(index)?;
        let open_ended =
            at == self.source.len() && !self.source.is_empty() && !self.source.ends_with('\n');
        let last = index == self.lanes.len();
        let block = lane_block(self.nl, title, limit, done, open_ended, last);
        let next = if open_ended {
            splice(&self.source, at..at, &block)
        } else {
            insert_block(&self.source, at, block, self.nl)
        };
        self.commit_lanes(next, index, true)
    }

    /// Replace the lane title. A ` (N)` limit on the same line stays.
    pub fn rename_lane(&mut self, index: usize, title: &str) -> io::Result<()> {
        check_lane_title(title)?;
        let span = self.lane(index)?.title_span.clone();
        self.commit(splice(&self.source, span, title))
    }

    /// Remove a lane: its title line, its done-lane marker and its cards.
    /// Other text in the lane (paragraphs, other headings, comments: what the
    /// page doesn't show) stays where it was, now after the lane before, as
    /// spec section 10.3 asks; only the blank lines that led into it go.
    /// When keeping that text would change another lane (an indented line
    /// that would join the previous lane's last card, say) the whole lane
    /// goes instead, and [`Kanban::lane_delete_drops`] counts what is lost.
    /// Other lanes' bytes are unchanged, and a `list-collapse` setting loses
    /// the lane's entry. Refused while the note ends inside an open fence.
    pub fn delete_lane(&mut self, index: usize) -> io::Result<()> {
        let (next, _) = self.lane_deletion(index)?;
        self.commit_lanes(next, index, false)
    }

    /// The non-blank lines of note text (not cards) that deleting lane
    /// `index` would remove, trimmed: none when all of it can stay.
    pub fn lane_delete_drops(&self, index: usize) -> io::Result<Vec<String>> {
        Ok(self.lane_deletion(index)?.1)
    }

    /// The text after deleting lane `index`, and the note lines it drops.
    fn lane_deletion(&self, index: usize) -> io::Result<(String, Vec<String>)> {
        self.refuse_open_fence()?;
        let lane = self.lane(index)?;
        let span = lane.span.clone();
        let mut cut: Vec<Range<usize>> = vec![lane.title_line.clone()];
        cut.extend(lane.marker.clone());
        cut.extend(lane.cards.iter().map(|card| card.span.clone()));
        cut.sort_by_key(|range| range.start);
        let mut kept = String::new();
        let mut at = span.start;
        for range in cut {
            kept.push_str(&self.source[at..range.start]);
            at = range.end;
        }
        kept.push_str(&self.source[at..span.end]);
        let notes: Vec<String> = kept
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();
        let whole = splice(&self.source, span.clone(), "");
        if notes.is_empty() {
            return Ok((whole, notes));
        }
        let lead: usize = kept
            .split_inclusive('\n')
            .take_while(|line| line.trim().is_empty())
            .map(str::len)
            .sum();
        let next = splice(&self.source, span, &kept[lead..]);
        if self.only_lane_removed(&next, index) {
            Ok((next, Vec::new()))
        } else {
            Ok((whole, notes))
        }
    }

    /// Whether `next` is this board less lane `index`: every other lane, the
    /// archive and the footer read back the same.
    fn only_lane_removed(&self, next: &str, index: usize) -> bool {
        let Ok(parsed) = Kanban::parse(next) else {
            return false;
        };
        let same_cards = |a: &[Card], b: &[Card]| {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(a, b)| a.text == b.text && a.mark == b.mark && a.block_id == b.block_id)
        };
        let others = self
            .lanes
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, lane)| lane);
        parsed.lanes.len() + 1 == self.lanes.len()
            && others.zip(&parsed.lanes).all(|(a, b)| {
                a.title == b.title
                    && a.written_limit == b.written_limit
                    && a.done_word == b.done_word
                    && same_cards(&a.cards, &b.cards)
            })
            && match (&self.archive, &parsed.archive) {
                (Some(a), Some(b)) => a.word == b.word && same_cards(&a.cards, &b.cards),
                (None, None) => true,
                _ => false,
            }
            && self.footer_json() == parsed.footer_json()
    }

    /// Lane add and delete would act inside a fence that never closes, so
    /// they are refused, as a new settings footer is.
    fn refuse_open_fence(&self) -> io::Result<()> {
        if self.ends_in_fence {
            return Err(invalid_data("the note ends inside an unclosed code fence"));
        }
        Ok(())
    }

    /// Move a lane. Its lines, from the title line up to the next lane, the
    /// archive or the footer, are taken out and put back unchanged before the
    /// lane now at `to`, or after the last lane. `to` is the lane's index
    /// after the move, as for [`Kanban::move_card`]. A `list-collapse`
    /// setting moves the lane's entry with it, so every lane keeps its state.
    pub fn move_lane(&mut self, from: usize, to: usize) -> io::Result<()> {
        let count = self.lanes.len();
        if from >= count || to >= count {
            return Err(invalid_input("lane index out of range"));
        }
        if from == to {
            return Ok(());
        }
        let lane = self.lane(from)?;
        let (title, cards, span) = (lane.title.clone(), lane.cards.len(), lane.span.clone());
        let block = self.source[span.clone()].to_owned();
        let removed = splice(&self.source, span, "");
        let without = Kanban::parse(&removed)?;
        let at = without.lane_insert_offset(to)?;
        let mut parsed = Kanban::parse(&insert_block(&removed, at, block, without.nl))?;
        let moved = parsed.lane(to)?;
        if parsed.lanes.len() != count || moved.title != title || moved.cards.len() != cards {
            return Err(invalid_data("the lane did not move as a whole"));
        }
        if let Some(mut flags) = parsed.collapse_flags()
            && from.min(to) < flags.len()
        {
            flags.resize(flags.len().max(from.max(to) + 1), Value::Bool(false));
            let flag = flags.remove(from);
            flags.insert(to, flag);
            parsed.set_setting(LIST_COLLAPSE, &Value::Array(flags))?;
        }
        *self = parsed;
        Ok(())
    }

    /// Add or remove the lane's done-lane marker: one line either way. A new
    /// marker is the English `**Complete**`, after the blank line under the
    /// title when there is one (as spec section 4 writes a new lane), else
    /// right under the title; existing lanes are not re-spaced. Removing one
    /// removes only its line, whatever its word. Cards keep their done marks.
    pub fn set_lane_done(&mut self, index: usize, done: bool) -> io::Result<()> {
        let lane = self.lane(index)?;
        let nl = self.nl;
        let next = match (lane.marker.clone(), done) {
            (Some(_), true) | (None, false) => return Ok(()),
            (Some(marker), false) => splice(&self.source, marker, ""),
            (None, true) => {
                let mut at = lane.title_line.end;
                let block = if !self.source[..at].ends_with('\n') {
                    format!("{nl}{ENGLISH_MARKER}")
                } else {
                    if self.source[at..lane.span.end].starts_with(nl) {
                        at += nl.len();
                    }
                    format!("{ENGLISH_MARKER}{nl}")
                };
                splice(&self.source, at..at, &block)
            }
        };
        let parsed = Kanban::parse(&next)?;
        if parsed.lane(index)?.done_word.is_some() != done {
            return Err(invalid_data(
                "the done-lane marker did not stay with the lane",
            ));
        }
        *self = parsed;
        Ok(())
    }

    /// Show lane `index` collapsed or expanded. This writes the footer's
    /// `list-collapse` key: one boolean per lane, in file order. Only that
    /// key's value changes; when it is absent it is appended as the last key.
    pub fn set_lane_collapsed(&mut self, index: usize, collapsed: bool) -> io::Result<()> {
        if self.lane(index)?.collapsed == collapsed {
            return Ok(());
        }
        let flags = self
            .lanes
            .iter()
            .enumerate()
            .map(|(i, lane)| {
                Value::Bool(if i == index {
                    collapsed
                } else {
                    lane.collapsed
                })
            })
            .collect();
        self.set_setting(LIST_COLLAPSE, &Value::Array(flags))
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
                // After the title, in place of any trailing spaces.
                let end = content_end(&self.source, lane.title_line.clone());
                let at = lane.title_span.end.min(end);
                splice(&self.source, at..end, &format!(" ({limit})"))
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

    /// The card's block id, writing ` ^card-N` on its first line when it has
    /// none. An id already on the card is returned unchanged and the file is
    /// not touched. `card-N` is the next number not used by a lane or archive
    /// card. The spec fixes the id's shape (letters, digits, dashes) and that
    /// it stays on the card; it does not fix how a new one is chosen.
    pub fn assign_block_id(&mut self, lane: usize, index: usize) -> io::Result<String> {
        let card = self.card(lane, index)?;
        if let Some(id) = card.block_id.clone() {
            return Ok(id);
        }
        let id = self.fresh_block_id();
        let span = card.span.clone();
        let mut at = self.source[span.start..span.end]
            .find('\n')
            .map_or(span.end, |i| span.start + i);
        if at > span.start && self.source.as_bytes().get(at - 1) == Some(&b'\r') {
            at -= 1;
        }
        let next = splice(&self.source, at..at, &format!(" ^{id}"));
        let parsed = Kanban::parse(&next)?;
        if parsed.card(lane, index)?.block_id.as_deref() != Some(id.as_str()) {
            return Err(invalid_data("the block id did not stay on the card"));
        }
        *self = parsed;
        Ok(id)
    }

    /// The next `card-N` that no card in this note already uses.
    fn fresh_block_id(&self) -> String {
        let mut used = Vec::new();
        for lane in &self.lanes {
            for card in &lane.cards {
                if let Some(id) = &card.block_id {
                    used.push(id.as_str());
                }
            }
        }
        if let Some(archive) = &self.archive {
            for card in &archive.cards {
                if let Some(id) = &card.block_id {
                    used.push(id.as_str());
                }
            }
        }
        let mut n = 1u32;
        loop {
            let id = format!("card-{n}");
            if !used.contains(&id.as_str()) {
                return id;
            }
            n += 1;
        }
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

    /// Keep `next`, a lane added (`added`) or removed at `index`, with the
    /// `list-collapse` entries after it moved to stay with their lanes.
    fn commit_lanes(&mut self, next: String, index: usize, added: bool) -> io::Result<()> {
        let mut parsed = Kanban::parse(&next)?;
        if let Some(mut flags) = parsed.collapse_flags()
            && index < flags.len()
        {
            if added {
                flags.insert(index, Value::Bool(false));
            } else {
                flags.remove(index);
            }
            parsed.set_setting(LIST_COLLAPSE, &Value::Array(flags))?;
        }
        *self = parsed;
        Ok(())
    }

    /// The footer's `list-collapse` array, when it has one.
    fn collapse_flags(&self) -> Option<Vec<Value>> {
        let mut json = serde_json::from_str::<Value>(self.footer_json()?).ok()?;
        match json.get_mut(LIST_COLLAPSE).map(Value::take) {
            Some(Value::Array(flags)) => Some(flags),
            _ => None,
        }
    }

    fn commit(&mut self, next: String) -> io::Result<()> {
        let parsed = Kanban::parse(&next)?;
        *self = parsed;
        Ok(())
    }

    /// Keep `next` only when the card at `lane`/`index` reads back as `text`
    /// with `block_id` as its id.
    fn accept_card(
        &mut self,
        next: String,
        lane: usize,
        index: usize,
        text: &str,
        block_id: Option<&str>,
    ) -> io::Result<()> {
        let parsed = Kanban::parse(&next)?;
        let card = parsed.card(lane, index)?;
        if card.text != text || card.block_id.as_deref() != block_id {
            return Err(invalid_data("the edit did not stay in the card"));
        }
        *self = parsed;
        Ok(())
    }

    pub(super) fn lane(&self, index: usize) -> io::Result<&super::Lane> {
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
        // Spec section 4: the title, one blank line, the marker if any, then
        // the cards; so a card goes after the title's blank line, or right
        // under the marker. An empty lane's passthrough text at column 0
        // (a line right under the title, or after the marker, a paragraph)
        // stays above the first card, in place relative to the title before
        // it (spec section 10 rule 3).
        let after = if let Some(marker) = &lane.marker {
            marker.end
        } else {
            lane.title_line.end
        };
        let body = &self.source[after..lane.span.end];
        if let Some(end) = last_text_line_end(body) {
            return Ok(after + end);
        }
        let at = if lane.marker.is_some() {
            after
        } else if body.starts_with(self.nl) {
            after + self.nl.len()
        } else {
            after
        };
        // The lane's only text may be indented. A card put before it would
        // take it as a continuation line, so the card goes after it.
        let leading = &self.source[at..lane.span.end];
        Ok(at + leading_indented_end(leading).unwrap_or(0))
    }

    /// The card's lines shaped exactly as [`Kanban::edit_card`] would write
    /// them, without changing the file. Tests use it to exercise the writer
    /// on text that an unchanged edit returns before rendering.
    #[cfg(test)]
    pub(super) fn render_card_for_test(&self, lane: usize, index: usize) -> io::Result<String> {
        let card = self.card(lane, index)?;
        let indent = card
            .indent
            .clone()
            .unwrap_or_else(|| self.continuation_indent());
        let block = render_card(
            self.nl,
            card.mark,
            &card.text,
            card.block_id.as_deref(),
            &indent,
        );
        Ok(keep_open_ending(
            &self.source,
            card.span.end,
            block,
            self.nl,
        ))
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

/// A lane title is one line and doesn't end in ` (N)`, which would read
/// back as a card limit (a limit is set with `set_lane_limit`).
fn check_lane_title(title: &str) -> io::Result<()> {
    if title.contains(['\n', '\r']) {
        return Err(invalid_input("a lane title is one line"));
    }
    if super::parse::limit_suffix(title.trim_end()).is_some() {
        return Err(invalid_input(
            "a lane title can't end in a number in brackets; that is the card limit",
        ));
    }
    Ok(())
}

/// The bytes of a new lane. `open_ended` is a file with no final newline:
/// two blank lines before the heading, and no new final newline. After the
/// heading come one blank line, the marker when `done` is set, and one blank
/// line; a lane that is not `last` adds one more blank line, so the last lane
/// ends as New Kanban's last lane does.
fn lane_block(
    nl: &str,
    title: &str,
    limit: Option<u32>,
    done: bool,
    open_ended: bool,
    last: bool,
) -> String {
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
    if !last {
        block.push_str(nl);
    }
    block
}

/// A new card has no lines of its own to preserve, so an all-space indent
/// narrower than two columns is widened to two. An edit keeps a one-space indent.
fn padded_new_card_indent(indent: String) -> String {
    if indent.chars().all(|ch| ch == ' ') && indent_columns(&indent) < 2 {
        "  ".to_owned()
    } else {
        indent
    }
}

/// Drop carriage returns, then one trailing newline, matching what a card stores.
fn normalised_card_text(text: &str) -> String {
    let mut text = text.replace('\r', "");
    if text.ends_with('\n') {
        text.pop();
    }
    text
}

/// Split a trailing ` ^id` off the first line of new card text. Later lines
/// are never searched: a caret there is text, as it is when the file is read.
fn card_text_and_id(text: &str) -> (String, Option<String>) {
    match text.split_once('\n') {
        Some((first, rest)) => {
            let (first, id) = split_block_id(first);
            (format!("{first}\n{rest}"), id)
        }
        None => split_block_id(text),
    }
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
        if !line.is_empty() && !line_keeps_lead(line, indent) {
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
/// a block that would swallow the following line gets a trailing one. A file
/// with no final newline keeps that, unless the block ends with a blank line:
/// trimming that line's ending would drop the line, and the blank line is
/// part of the block.
fn insert_block(source: &str, at: usize, mut block: String, nl: &str) -> String {
    if at > 0 && !source[..at].ends_with('\n') {
        block.insert_str(0, nl);
    }
    if at < source.len() && !block.ends_with('\n') {
        block.push_str(nl);
    }
    if at == source.len()
        && !source.is_empty()
        && !source.ends_with('\n')
        && block.ends_with(nl)
        && !ends_with_blank_line(&block, nl)
    {
        block.truncate(block.len() - nl.len());
    }
    splice(source, at..at, &block)
}

/// Whether `block` ends with an empty line, so removing its final line ending
/// would remove that line rather than the terminator of the last one.
fn ends_with_blank_line(block: &str, nl: &str) -> bool {
    block
        .strip_suffix(nl)
        .is_some_and(|rest| rest.ends_with('\n'))
}

/// When `body` begins, after blank lines, with indented passthrough, the end
/// of that block: its indented lines, the blank lines between them, and an
/// indented fenced block whole. A card put before it would take those lines
/// as its continuation.
fn leading_indented_end(body: &str) -> Option<usize> {
    let mut at = 0;
    let mut seen = false;
    let mut end = 0;
    let mut fence: Option<(u8, usize)> = None;
    for line in body.split_inclusive('\n') {
        let text = line.trim_end_matches(['\r', '\n']);
        if let Some((marker, run)) = fence {
            at += line.len();
            end = at;
            if fence_closes(text, marker, run) {
                fence = None;
            }
            continue;
        }
        if is_blank(text) {
            at += line.len();
            continue;
        }
        if !is_indented(text) {
            break;
        }
        seen = true;
        if let Some(opened) = opens_fence(text) {
            fence = Some(opened);
        }
        at += line.len();
        end = at;
    }
    seen.then_some(end)
}

/// When `body` holds a line of text at column 0, the end (after its line
/// ending) of its last line that is not blank.
fn last_text_line_end(body: &str) -> Option<usize> {
    let mut end = None;
    let mut column_zero = false;
    let mut at = 0;
    for line in body.split_inclusive('\n') {
        at += line.len();
        if !line.trim_matches([' ', '\t', '\r', '\n']).is_empty() {
            end = Some(at);
            column_zero |= !line.starts_with([' ', '\t']);
        }
    }
    end.filter(|_| column_zero)
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
    // JSON readers keep the last member when a key repeats, so that is the
    // member a setting edit is about.
    if let Some(member) = members.iter().rev().find(|member| member.key == key) {
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
    // The effective member is the last one with this key.
    let Some(index) = members.iter().rposition(|member| member.key == key) else {
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
