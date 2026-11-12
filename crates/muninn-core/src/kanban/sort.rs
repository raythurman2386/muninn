//! Sorting a lane's cards.
//!
//! A sort moves each card's lines whole, slot by slot: the lane's cards stay
//! where cards were, and passthrough lines between them stay in place.

use std::cmp::Ordering;
use std::io;

use super::dates::DateSettings;
use super::text::scan_card_text;
use super::{Kanban, invalid_data};

/// What [`Kanban::sort_cards`] orders a lane by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CardOrder {
    /// The card's markdown, ignoring case.
    Text,
    /// The card's first date (`@{…}` or `@[[…]]`, then its first `@@{…}`
    /// time), compared as the civil date and time it parses to. The date is
    /// read with the board's `date-format`, so the written text does not
    /// decide the order. A card with no date sorts after every dated card.
    Date,
    /// The card's tags (`#word` outside code), in order, ignoring case.
    Tags,
}

/// One sort key. Every card in a sort uses the same variant, so the derived
/// order is the comparison the caller asked for.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Date(i64),
    Text(String),
    Tags(String),
}

impl CardOrder {
    /// The sort key for a card's text. `None` sorts after every key.
    fn key(self, text: &str, dates: &DateSettings) -> Option<Key> {
        match self {
            Self::Text => Some(Key::Text(text.to_lowercase())),
            Self::Date => scan_card_text(text, dates).sort_value(dates).map(Key::Date),
            Self::Tags => {
                let found = scan_card_text(text, dates);
                let tags: Vec<&str> = found.tags.iter().map(|tag| tag.tag.as_str()).collect();
                (!tags.is_empty()).then(|| Key::Tags(tags.join(" ").to_lowercase()))
            }
        }
    }
}

impl Kanban {
    /// Sort the cards of `lane` with the board's own date settings. The sort
    /// is stable, and cards with no date or no tags go last, in their
    /// original order. Each card's lines are moved whole; nothing changes
    /// when the lane is already in order.
    pub fn sort_cards(&mut self, lane: usize, order: CardOrder) -> io::Result<()> {
        let dates = self.date_settings();
        self.sort_cards_with(lane, order, &dates)
    }

    /// [`Kanban::sort_cards`] with date settings resolved by the caller, so
    /// the app-wide `kanban_defaults` reach sorting too.
    pub fn sort_cards_with(
        &mut self,
        lane: usize,
        order: CardOrder,
        dates: &DateSettings,
    ) -> io::Result<()> {
        let mut keyed: Vec<(Option<Key>, usize)> = self
            .lane(lane)?
            .cards
            .iter()
            .enumerate()
            .map(|(index, card)| (order.key(&card.text, dates), index))
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
