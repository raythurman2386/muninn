//! Per-frame work: autosave and graph settling.

use rune::Tx;

use crate::{Mode, State};

pub fn poll(s: &mut State, t: f64, tx: &mut Tx) -> bool {
    s.now = t;
    let mut changed = false;
    if s.dirty() {
        let since = *s.dirty_at.get_or_insert(t);
        if t - since >= 0.55 {
            s.save_now();
        }
        changed = true;
    } else {
        s.dirty_at = None;
    }
    if s.mode == Mode::Graph {
        let moved = s.graph.step();
        if moved {
            changed = true;
        }
        if s.auto_fit && (moved || s.graph.heat > 0.0) {
            s.refit();
            changed = true;
        }
    }
    if changed {
        tx.instant();
    }
    changed
}
