//! Per-frame work: autosave, crash-recovery copies, external-change watch and
//! graph settling.
//!
//! The Rune runtime warns when `poll` returns true while nothing visible
//! changed, so this returns true **only** when something the user can see
//! changed since the last frame: a save that reached disk (or surfaced a new
//! error), an external edit that raised a conflict, a vault reload, a graph
//! node that actually moved, or a camera fit that actually moved the view.
//! Merely being dirty, writing a recovery copy, or having layout heat left is
//! not a visible change and must not wake the window.

use rune::Tx;

use crate::{Mode, State};

pub fn poll(s: &mut State, t: f64, tx: &mut Tx) -> bool {
    s.now = t;
    let mut changed = false;

    // M2: query-derived state (search highlight, in-note find matches) is
    // recomputed here, before this frame's commit, because the view only
    // borrows `State` and typing is committed after the key handler runs.
    if s.search_tick() {
        changed = true;
    }
    if s.find_tick() {
        changed = true;
    }
    if s.complete_tick() {
        changed = true;
    }

    // Watch first: an external edit to the dirty note must raise its conflict
    // before autosave gets a chance to report something less specific. The
    // watcher itself only touches metadata.
    if s.watch_tick(t) {
        changed = true;
    }

    if s.dirty() {
        let since = *s.dirty_at.get_or_insert(t);
        if t - since >= s.autosave_delay() {
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

    // SAF-05: copy a dirty buffer to the XDG recovery file after a short idle.
    // This is invisible on screen, so it must not report a change.
    s.recovery_tick(t);

    if s.mode == Mode::Graph {
        // DES-03: `graph_step` honours MUNINN_REDUCED_MOTION (heat pinned to
        // zero, no movement); pan, zoom and the initial fit still work.
        let moved = s.graph_step();
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
