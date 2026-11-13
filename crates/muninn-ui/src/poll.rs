//! Per-frame work: autosave, crash-recovery copies, external-change watch and
//! graph settling.
//!
//! The Rune runtime warns when `poll` returns true while nothing visible
//! changed, so this returns true **only** when something the user can see
//! changed since the last frame: a save that reached disk (or surfaced a new
//! error), an external edit that raised a conflict, a vault reload, a graph
//! node that actually moved, a camera glide that moved the view, or a camera
//! fit that actually moved the view.
//! Merely being dirty, writing a recovery copy, or having layout heat left is
//! not a visible change and must not wake the window.

use rune::Tx;

use crate::{Mode, State};

/// How often `poll` wakes while a sync is in flight.
pub(crate) const SYNC_WAKE: std::time::Duration = std::time::Duration::from_millis(250);

pub fn poll(s: &mut State, t: f64, tx: &mut Tx) -> bool {
    s.now = t;
    // A large vault is still scanning: advance one bounded chunk and keep the
    // progress bar moving. Autosave/watch/graph all wait for a real vault, and
    // the status bar has nothing to save while the placeholder is empty.
    if s.loading() {
        let changed = s.scan_tick();
        if changed {
            tx.instant();
        }
        return changed;
    }
    let mut changed = false;
    // Board slice 6: the text editor commits once it loses the keyboard
    // (a click on the sidebar, a dialog), so it never strands Board keys.
    if s.board_text.is_some() && !s.board_text_field.focused {
        s.commit_board_text();
        changed = true;
    }
    // GK MF-B (#43): a board rename keeps the keyboard only while its field
    // is on screen.
    if s.board_rename_keep_on_screen() {
        changed = true;
    }
    // Remember the open board for the next launch once a frame drew it; this
    // writes settings only, nothing on screen changes.
    s.board_shown_tick(tx);

    // M2: query-derived state (search highlight, in-note find matches) is
    // recomputed here, before this frame's commit, because the view only
    // borrows `State` and typing is committed after the key handler runs.
    if s.search_tick() {
        changed = true;
    }
    if s.find_tick() {
        changed = true;
    }
    if s.kanban_filter_tick() {
        changed = true;
    }
    if s.kanban_find_tick() {
        changed = true;
    }
    if s.poll_sync() {
        changed = true;
    }
    // A hidden window (closed to the tray) only polls on a timer, so keep
    // checking while a run or sign-in is in flight; otherwise its result and
    // the tray's status row would wait for the window to show again.
    if s.sync_busy() {
        tx.wake_after(SYNC_WAKE);
    }
    // The tray's status row follows the sync status line. It is not drawn in
    // the window, so it never sets `changed`.
    s.sync_tray();
    if s.complete_tick() {
        changed = true;
    }
    // v0.5: the palette highlights a row from its current list; keep the
    // selection valid after typing and scroll it into view.
    if s.palette_tick() {
        changed = true;
    }
    if let Some(key) = s.palette_take_reveal() {
        tx.scroll_into_view(key);
        changed = true;
    }

    // Watch first: an external edit to the dirty note must raise its conflict
    // before autosave gets a chance to report something less specific. The
    // watcher itself only touches metadata.
    if s.watch_tick(t) {
        changed = true;
    }

    // Omarchy: `omarchy-theme-set` rewrites the shared palette file; follow
    // the switch live without a restart.
    if s.omarchy_tick() {
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

    // Slice 7: boards another app changed, then autosave of the open board
    // (after the watcher, so a change on disk is flagged before a save).
    if s.board_watch_tick(t) {
        changed = true;
    }
    if s.board_autosave_tick(t) {
        changed = true;
    }
    // Kanban slice 7: the same for the Kanban page.
    if s.kanban_watch_tick(t) {
        changed = true;
    }
    if s.kanban_autosave_tick(t) {
        changed = true;
    }

    // Slice 7: a view change asked for a settings write; do it once the
    // view rests (one write per held key or stroke, not per event).
    s.settings_tick(t);

    // SAF-05: copy a dirty buffer to the XDG recovery file after a short idle.
    // This is invisible on screen, so it must not report a change.
    s.recovery_tick(t);
    // #38 review B2: the same for an unsaved board, after each change.
    // #40 recheck B1: the copy may still need a poll (typing at rest), and
    // slice 7 keeps the board watcher, autosave and settings write on time:
    // a resting window polls only at a wake it asked for.
    let recovery = s.board_recovery_poll(t);
    let kanban_recovery = s.kanban_recovery_poll(t);
    let wakes = [recovery, s.board_wake(t), kanban_recovery, s.kanban_wake(t)];
    if let Some(wake) = wakes.into_iter().flatten().min() {
        tx.wake_after(wake);
    }

    if s.mode == Mode::Graph {
        // A node click or scope switch eases the camera over a few frames.
        // Ask for the next frame only while that glide is in flight, so
        // reduced motion (which lands the camera at once) never wakes for it.
        if s.advance_camera_glide() {
            changed = true;
        }
        if s.camera_gliding() {
            tx.wake_after(std::time::Duration::ZERO);
        }
        // DES-03: `graph_step` honours `State::reduced_motion` (heat pinned to
        // zero, no movement); pan, zoom and the initial fit still work.
        let moved = s.graph_step();
        if moved {
            changed = true;
        }
        // Refit only when the layout moved or a fit is pending. Heat alone
        // (positions static) must not recompute bounds or wake the view. A
        // scope switch eases; entering the graph still fits at once so the
        // first frame (and snapshots) lands on the settled framing.
        if s.auto_fit && (moved || s.needs_fit) {
            let fit_changed = if s.smooth_fit {
                s.refit_glide()
            } else {
                s.refit()
            };
            if fit_changed {
                changed = true;
            }
        }
    }
    if changed {
        tx.instant();
    }
    changed
}
