//! Per-frame work: autosave and graph settling.
//!
//! The Rune runtime warns when `poll` returns true while nothing visible
//! changed, so this returns true **only** when something the user can see
//! changed since the last frame: a save that reached disk (or surfaced a new
//! error), a graph node that actually moved, or a camera fit that actually
//! moved the view. Merely being dirty, or having layout heat left, is not a
//! visible change and must not wake the window.

use rune::Tx;

use crate::{Mode, State};

pub fn poll(s: &mut State, t: f64, tx: &mut Tx) -> bool {
    s.now = t;
    let mut changed = false;
    if s.dirty() {
        let since = *s.dirty_at.get_or_insert(t);
        if t - since >= 0.55 {
            let before_error = s.error.clone();
            let saved = s.save_now();
            // A save that reached disk flips the status bar from "Saving…"
            // to "Saved" and refreshes links; a new error replaces it with the
            // error text. A repeated save with no new result is invisible.
            if saved || s.error != before_error {
                changed = true;
            }
            if !saved && s.error.is_some() {
                // The write failed (dirty stays true). Back off for another
                // idle window instead of retrying — and reporting — every
                // frame until the user fixes the cause.
                s.dirty_at = Some(t);
            }
        }
    } else {
        s.dirty_at = None;
    }
    if s.mode == Mode::Graph {
        let moved = s.graph.step();
        if moved {
            changed = true;
        }
        // Refit only when the layout moved or a fit is pending. Heat alone
        // (positions static) must not recompute bounds or wake the view.
        if s.auto_fit && (moved || s.needs_fit) && s.refit() {
            changed = true;
        }
    }
    if changed {
        tx.instant();
    }
    changed
}
