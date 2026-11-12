//! Kanban slice 6: dragging cards and lanes on the Kanban page.
//!
//! - A card drags by its body, a lane by its header (a collapsed lane by its
//!   strip). As on Board, a press that travels less than
//!   [`CLICK_TRAVEL`](crate::board_edit::CLICK_TRAVEL) stays a click, and the
//!   pressed element holds the pointer until the release (Rune's capture).
//! - While a card is dragged it is dimmed, and a bar shows where it will land:
//!   in the lane nearest the pointer across, before the first card whose
//!   middle is below the pointer. Cards are measured as drawn, so cards of any
//!   height count. A collapsed lane takes the card at its end and is outlined.
//!   A dragged lane is dimmed and a bar between lanes shows where it will go.
//! - The release is one edit: slice 5's card move (a card entering a done lane
//!   is marked done) or [`muninn_core::Kanban::move_lane`], which moves the
//!   lane's lines and its `list-collapse` entry. It is one undo step, shows the
//!   kanban as unsaved and is written only by `⌘S`. A release where the card or
//!   lane already is records nothing.
//! - Esc during a drag ends it (Rune cancels the gesture): nothing moves, as
//!   the board is not touched until the release.
//! - When a drag starts, an open card editor or lane field is committed first
//!   and the dragged card or lane is found again (slices 4 and 5). When that
//!   commit is refused the drag does nothing and the editor keeps its text.
//! - The keyboard has the same moves: slice 5's Alt+arrows for cards, and
//!   Alt+Shift+Left / Right (or the lane menu's **Move lane left / right**)
//!   for the lane holding the selected card.

use std::collections::HashMap;

use muninn_core::Kanban;
use rune::{Drag, Phase, PointerButton, Tx};

use crate::State;
use crate::board_edit::CLICK_TRAVEL;

/// What is being dragged: a card (lane, index) or a lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dragged {
    Card(usize, usize),
    Lane(usize),
}

/// A drag past the click distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KanbanDrag {
    /// `None` when an open editor refused to close: the gesture does nothing.
    pub item: Option<Dragged>,
    /// Where a release would put it: a card's lane and the number of cards
    /// drawn above the spot, or the number of lanes left of the spot.
    pub slot: Option<(usize, usize)>,
}

/// Where lanes and cards were last laid out, in window points, reported by
/// each element as it moves. Entries for lanes or cards no longer drawn are
/// never read: the board decides which ones count.
#[derive(Clone, Debug, Default)]
pub struct KanbanRects {
    lanes: HashMap<usize, [f32; 4]>,
    cards: HashMap<(usize, usize), [f32; 4]>,
}

impl KanbanRects {
    pub(crate) fn set_lane(&mut self, lane: usize, rect: [f32; 4]) {
        self.lanes.insert(lane, rect);
    }

    pub(crate) fn set_card(&mut self, lane: usize, index: usize, rect: [f32; 4]) {
        self.cards.insert((lane, index), rect);
    }

    /// The card slot at window point `at`: the lane nearest across, and in
    /// it the number of cards whose middle is above `at` (all of them in a
    /// collapsed lane).
    fn card_slot(&self, board: &Kanban, at: [f32; 2]) -> Option<(usize, usize)> {
        let (lane, _) = board
            .lanes()
            .iter()
            .enumerate()
            .filter_map(|(i, lane)| Some((i, lane, self.lanes.get(&i)?)))
            .map(|(i, lane, r)| (i, lane, (r[0] - at[0]).max(at[0] - r[0] - r[2]).max(0.0)))
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(i, lane, _)| (i, lane))?;
        let len = board.lanes()[lane].cards().len();
        if board.lanes()[lane].collapsed() {
            return Some((lane, len));
        }
        let above = (0..len)
            .filter_map(|j| self.cards.get(&(lane, j)))
            .filter(|r| r[1] + r[3] / 2.0 < at[1])
            .count();
        Some((lane, above))
    }

    /// The lane slot at `at`: how many lanes have their middle left of it.
    fn lane_slot(&self, lanes: usize, at: [f32; 2]) -> usize {
        (0..lanes)
            .filter_map(|i| self.lanes.get(&i))
            .filter(|r| r[0] + r[2] / 2.0 < at[0])
            .count()
    }

    /// A lane's drawn height, for the bar that shows where it would go.
    pub(crate) fn lane_height(&self, lane: usize) -> Option<f32> {
        self.lanes.get(&lane).map(|r| r[3])
    }
}

impl State {
    /// One phase of a drag on a card or a lane header.
    pub(crate) fn drag_kanban(&mut self, item: Dragged, d: &Drag, tx: &Tx) {
        if tx.button() != PointerButton::Primary {
            return;
        }
        if d.phase == Phase::Down {
            self.kanban_drag = None;
            return;
        }
        let up = d.phase == Phase::Up;
        if self.kanban_drag.is_none() {
            if up || d.cancelled() || d.dx.abs().max(d.dy.abs()) < CLICK_TRAVEL {
                return;
            }
            self.menu = None;
            let item = match item {
                Dragged::Card(lane, index) => self
                    .settle_card_edit(lane, index)
                    .map(|(l, i)| Dragged::Card(l, i)),
                Dragged::Lane(lane) => self.settle_lane_edit(lane).map(Dragged::Lane),
            };
            self.kanban_drag = Some(KanbanDrag { item, slot: None });
        }
        let Some(drag) = self.kanban_drag else {
            return;
        };
        let slot = drag
            .item
            .and_then(|item| self.drop_slot(item, d.window_point()));
        if !up {
            self.kanban_drag = Some(KanbanDrag { slot, ..drag });
            return;
        }
        self.kanban_drag = None;
        if d.cancelled() {
            return;
        }
        match (drag.item, slot) {
            (Some(Dragged::Card(lane, index)), Some((to_lane, slot))) => {
                // The slot counts the dragged card when it is above the spot.
                let to_index = if to_lane == lane && slot > index {
                    slot - 1
                } else {
                    slot
                };
                if (to_lane, to_index) != (lane, index) {
                    self.move_card_now(lane, index, to_lane, to_index);
                }
            }
            (Some(Dragged::Lane(lane)), Some((_, slot))) => {
                let to = if slot > lane { slot - 1 } else { slot };
                self.move_lane_now(lane, to);
            }
            _ => {}
        }
    }

    fn drop_slot(&self, item: Dragged, at: [f32; 2]) -> Option<(usize, usize)> {
        let board = self.kanban_board()?;
        match item {
            Dragged::Card(..) => self.kanban_rects.card_slot(board, at),
            Dragged::Lane(_) => Some((0, self.kanban_rects.lane_slot(board.lanes().len(), at))),
        }
    }

    /// Alt+Shift+Left / Right and the lane menu: move a lane one place.
    pub fn step_kanban_lane(&mut self, lane: usize, right: bool) {
        let Some(lane) = self.settle_lane_edit(lane) else {
            return;
        };
        let lanes = self.kanban_board().map_or(0, |b| b.lanes().len());
        let to = if right {
            Some(lane + 1).filter(|&to| to < lanes)
        } else {
            lane.checked_sub(1)
        };
        if let Some(to) = to {
            self.move_lane_now(lane, to);
        }
    }

    /// The move itself, on indices that match the board as it is now. The
    /// selected card stays selected in its lane.
    fn move_lane_now(&mut self, from: usize, to: usize) {
        if self.edit_kanban("move the lane", |b| b.move_lane(from, to)) {
            let follow = |l: usize| match l {
                l if l == from => to,
                l if from < l && l <= to => l - 1,
                l if to <= l && l < from => l + 1,
                l => l,
            };
            self.kanban_selected = self.kanban_selected.map(|(l, i)| (follow(l), i));
        }
    }

    /// What the page draws for a drag in lane `lane`: the card slot shown
    /// there, the dragged card in it, and whether the lane is the one dragged.
    pub(crate) fn kanban_drag_in(&self, lane: usize) -> (Option<usize>, Option<usize>, bool) {
        let Some(KanbanDrag {
            item: Some(item),
            slot,
        }) = self.kanban_drag
        else {
            return (None, None, false);
        };
        match item {
            Dragged::Card(l, i) => (
                slot.filter(|s| s.0 == lane).map(|s| s.1),
                (l == lane).then_some(i),
                false,
            ),
            Dragged::Lane(l) => (None, None, l == lane),
        }
    }

    /// The lane slot a lane drag shows, and the dragged lane.
    pub(crate) fn kanban_lane_drop(&self) -> Option<(usize, usize)> {
        match self.kanban_drag? {
            KanbanDrag {
                item: Some(Dragged::Lane(lane)),
                slot: Some((_, slot)),
            } => Some((lane, slot)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = "---\nkanban-plugin: board\n---\n\n## A\n\n- [ ] short\n- [ ] tall\n  two\n  three\n- [ ] last\n\n## B\n\n- [ ] b\n\n## C\n\n- [ ] c\n\n%% kanban:settings\n```\n{\"list-collapse\":[false,false,true]}\n```\n%%\n";

    fn rects() -> KanbanRects {
        let mut r = KanbanRects::default();
        for (i, x) in [0.0, 300.0, 600.0].into_iter().enumerate() {
            r.set_lane(i, [x, 0.0, 260.0, 400.0]);
        }
        // Lane A: 40, 120 (tall) and 40 points high, 8 apart.
        r.set_card(0, 0, [10.0, 50.0, 240.0, 40.0]);
        r.set_card(0, 1, [10.0, 98.0, 240.0, 120.0]);
        r.set_card(0, 2, [10.0, 226.0, 240.0, 40.0]);
        r.set_card(1, 0, [310.0, 50.0, 240.0, 40.0]);
        // Lane C is collapsed: a card rect left over from before is not read.
        r.set_card(2, 0, [610.0, 50.0, 240.0, 40.0]);
        r
    }

    #[test]
    fn a_card_slot_follows_drawn_heights() {
        let board = Kanban::parse(BOARD).expect("parse");
        let r = rects();
        // The tall card's middle is at 158: above it is slot 1, below it 2.
        assert_eq!(r.card_slot(&board, [100.0, 150.0]), Some((0, 1)));
        assert_eq!(r.card_slot(&board, [100.0, 200.0]), Some((0, 2)));
        assert_eq!(r.card_slot(&board, [100.0, 10.0]), Some((0, 0)));
        assert_eq!(r.card_slot(&board, [100.0, 390.0]), Some((0, 3)));
        // Between lanes, the nearer one; past the last lane, the last lane.
        assert_eq!(r.card_slot(&board, [285.0, 10.0]), Some((1, 0)));
        assert_eq!(r.card_slot(&board, [275.0, 80.0]), Some((0, 1)));
        // A collapsed lane takes the card at its end.
        assert_eq!(r.card_slot(&board, [620.0, 10.0]), Some((2, 1)));
        assert_eq!(r.card_slot(&board, [2000.0, 10.0]), Some((2, 1)));
    }

    #[test]
    fn a_lane_slot_counts_lanes_left_of_the_pointer() {
        let r = rects();
        assert_eq!(r.lane_slot(3, [10.0, 0.0]), 0);
        assert_eq!(r.lane_slot(3, [200.0, 0.0]), 1);
        assert_eq!(r.lane_slot(3, [740.0, 0.0]), 3);
        // A lane no longer on the board is not counted.
        assert_eq!(r.lane_slot(2, [740.0, 0.0]), 2);
    }
}
