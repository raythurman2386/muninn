//! Sorting a lane's cards.
//!
//! A sort moves each card's lines whole, slot by slot: the lane's cards stay
//! where cards were, and passthrough lines between them stay in place.

use std::cmp::Ordering;
use std::io;

use super::{Kanban, invalid_data};

/// What [`Kanban::sort_cards`] orders a lane by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CardOrder {
    /// The card's markdown, ignoring case.
    Text,
    /// The card's first date (`@{…}` or `@[[…]]`, then its first `@@{…}`
    /// time), compared as written. Dates in the default `YYYY-MM-DD` form
    /// sort by day.
    Date,
    /// The card's tags (`#word` outside code), in order, ignoring case.
    Tags,
}

impl CardOrder {
    /// The sort key for a card's text. `None` sorts after every key.
    fn key(self, text: &str) -> Option<String> {
        match self {
            Self::Text => Some(text.to_lowercase()),
            Self::Date => card_date(text),
            Self::Tags => {
                let tags = card_tags(text);
                (!tags.is_empty()).then(|| tags.join(" ").to_lowercase())
            }
        }
    }
}

impl Kanban {
    /// Sort the cards of `lane`. The sort is stable, and cards with no date
    /// or no tags go last, in their original order. Each card's lines are
    /// moved whole; nothing changes when the lane is already in order.
    pub fn sort_cards(&mut self, lane: usize, order: CardOrder) -> io::Result<()> {
        let mut keyed: Vec<(Option<String>, usize)> = self
            .lane(lane)?
            .cards
            .iter()
            .enumerate()
            .map(|(index, card)| (order.key(&card.text), index))
            .collect();
        keyed.sort_by(|(a, _), (b, _)| match (a, b) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        });
        let order: Vec<usize> = keyed.into_iter().map(|(_, index)| index).collect();
        self.reorder_cards(lane, &order)
    }

    /// Put card `order[slot]` in each slot of `lane`. A slot keeps its own
    /// line ending, so a card moved off the file's unterminated last line
    /// gains one and the card moved there loses it.
    fn reorder_cards(&mut self, lane: usize, order: &[usize]) -> io::Result<()> {
        if order.iter().enumerate().all(|(slot, &index)| slot == index) {
            return Ok(());
        }
        let cards = &self.lane(lane)?.cards;
        let mut next = self.source.clone();
        for (slot, &from) in order.iter().enumerate().rev() {
            let span = cards[slot].span.clone();
            let ending = line_ending(&self.source[span.clone()]);
            let raw = &self.source[cards[from].span.clone()];
            let body = &raw[..raw.len() - line_ending(raw).len()];
            next.replace_range(span, &format!("{body}{ending}"));
        }
        let parsed = Kanban::parse(&next)?;
        let whole = parsed
            .lane(lane)?
            .cards
            .iter()
            .map(|card| &card.text)
            .eq(order.iter().map(|&index| &cards[index].text));
        if !whole {
            return Err(invalid_data("the sort did not keep every card whole"));
        }
        *self = parsed;
        Ok(())
    }
}

fn line_ending(text: &str) -> &'static str {
    if text.ends_with("\r\n") {
        "\r\n"
    } else if text.ends_with('\n') {
        "\n"
    } else {
        ""
    }
}

/// The first date, with the first time after a space when there is one.
fn card_date(text: &str) -> Option<String> {
    let mut date = None;
    let mut time = None;
    for (at, _) in text.match_indices('@') {
        let rest = &text[at..];
        if let Some(found) = braced(rest, "@@{", "}") {
            time = time.or(Some(found));
        } else if date.is_none() && !text[..at].ends_with('@') {
            date = braced(rest, "@{", "}").or_else(|| braced(rest, "@[[", "]]"));
        }
    }
    date.map(|date| match time {
        Some(time) => format!("{date} {time}"),
        None => date.to_owned(),
    })
}

fn braced<'a>(rest: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let inner = rest.strip_prefix(open)?;
    let value = &inner[..inner.find(close)?];
    (!value.is_empty() && !value.contains('\n')).then_some(value)
}

/// `#word` tags at the start of a line or after whitespace, outside inline
/// code and fenced blocks. A tag is letters, digits, `_`, `-` and `/`, and
/// is not only digits.
fn card_tags(text: &str) -> Vec<&str> {
    let mut tags = Vec::new();
    let mut fenced = false;
    for line in text.lines() {
        let lead = line.trim_start();
        if lead.starts_with("```") || lead.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut code = false;
        let mut before = ' ';
        for (at, ch) in line.char_indices() {
            if ch == '`' {
                code = !code;
            } else if ch == '#' && !code && before.is_whitespace() {
                let word = &line[at + 1..];
                let len = word
                    .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '/')))
                    .unwrap_or(word.len());
                let tag = &word[..len];
                if tag.chars().any(|c| !c.is_ascii_digit()) {
                    tags.push(tag);
                }
            }
            before = ch;
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_is_the_first_date_then_the_first_time() {
        assert_eq!(card_date("ship @{2026-11-12}"), Some("2026-11-12".into()));
        assert_eq!(
            card_date("at @@{09:30} on @[[2026-01-02]] @{2027-01-01}"),
            Some("2026-01-02 09:30".into())
        );
        assert_eq!(card_date("only a time @@{14:30}"), None);
        assert_eq!(card_date("mail me@{x"), None);
    }

    #[test]
    fn tags_skip_code_headings_and_numbers() {
        assert_eq!(
            card_tags("#Ops fix `#not` #12 a#b #ui/menu\n```\n#fenced\n```\n#last"),
            ["Ops", "ui/menu", "last"]
        );
        assert!(card_tags("## not a tag").is_empty());
    }
}
