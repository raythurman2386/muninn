//! #293: dragging a note row in the vault sidebar onto a folder header.
//!
//! - The row holds the pointer from its first press. As on Kanban, a press that
//!   travels less than [`CLICK_TRAVEL`](crate::board_edit::CLICK_TRAVEL) stays a
//!   click, so the row keeps opening its note and keeps its keyboard focus.
//! - While the pointer is over a folder header (the vault-root header included:
//!   it is the same element built with the empty folder) that header is filled
//!   with the existing hover colour. The header's own layout rect is measured
//!   as drawn and bound by the element, so a sidebar scroll does not need a
//!   second geometry calculation.
//! - The release is one call to [`State::move_note_to`] and nothing else. The
//!   locked-note refusal, the SAF-06 link-impact dialog and the "already
//!   exists" clash refusal are exactly the ones the Move menu goes through;
//!   dropping anywhere else, or cancelling with Esc, moves nothing.
//! - Moving stays a keyboard path: the Move picker is untouched, and a drag
//!   adds no visible control and no new text.

use std::path::PathBuf;

use muninn_i18n::{Key as TextKey, t};
use rune::{Drag, Phase, PointerButton, Tx};

use crate::State;
use crate::board_edit::CLICK_TRAVEL;

/// A press on a note row, held until the release. The note is held by its
/// vault path — the stable identity across reloads — and resolved back to an
/// id at the release, so a drag from a search-filtered list moves that note,
/// and a watcher reload mid-drag cannot move whatever note now sits at the
/// old id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarDrag {
    /// The dragged note's path, resolved to the current id at the release.
    pub path: PathBuf,
    /// The folder whose header is under the pointer, if any.
    pub hover: Option<String>,
    /// Past the click distance: a drag, not yet a click.
    pub moving: bool,
}

impl State {
    /// #293: the live drawn rect of folder `folder`'s header, in window points.
    ///
    /// The header binds it with [`rune::El::drawn_rect_into`], which writes
    /// only when the rect moves (a scroll included) and runs no handler, so a
    /// drag can hit-test the header as drawn without a view rebuild.
    pub(crate) fn folder_header_rect(&self, folder: &str) -> rune::Live<[f32; 4]> {
        self.folder_header_rects
            .borrow_mut()
            .entry(folder.to_owned())
            .or_insert_with(|| rune::Live::new([0.0; 4]))
            .clone()
    }

    /// #293: the sidebar rebuilds its list each frame; remember which headers
    /// it drew so a folder hidden by the search filter cannot be hit through a
    /// rect left over from before the filter changed.
    pub(crate) fn clear_drawn_folder_headers(&self) {
        self.drawn_folder_headers.borrow_mut().clear();
    }

    /// #293: called by each folder header as the sidebar is built.
    pub(crate) fn mark_folder_header(&self, folder: &str) {
        self.drawn_folder_headers
            .borrow_mut()
            .insert(folder.to_owned());
    }

    /// #293: is `folder` the header highlighted for the held drag?
    pub(crate) fn sidebar_drop_target(&self, folder: &str) -> bool {
        self.sidebar_drag
            .as_ref()
            .is_some_and(|drag| drag.moving && drag.hover.as_deref() == Some(folder))
    }

    /// One phase of a drag on a note row.
    pub(crate) fn drag_sidebar_note(&mut self, id: usize, d: &Drag, tx: &Tx) {
        if tx.button() != PointerButton::Primary {
            return;
        }
        if d.phase == Phase::Down {
            // #134: the same guard `begin_move_note` applies on the Move menu
            // path. A locked note whose body is not readable would have to be
            // re-encrypted under a new id, so the move is refused here too.
            if self
                .vault
                .notes
                .get(id)
                .is_some_and(|note| note.is_locked() && note.readable_source().is_none())
            {
                self.error = Some(t(TextKey::LockRefusedRename).to_owned());
                return;
            }
            // Hold the path, not the id: vault ids are scan positions, and a
            // watcher reload between this press and the release reassigns them.
            let path = match self.vault.notes.get(id) {
                Some(note) => note.path.clone(),
                None => return,
            };
            self.sidebar_drag = Some(SidebarDrag {
                path,
                hover: None,
                moving: false,
            });
            return;
        }
        let Some(mut drag) = self.sidebar_drag.take() else {
            return;
        };
        let up = d.phase == Phase::Up;
        if !drag.moving {
            if up || d.cancelled() {
                return;
            }
            if d.dx.abs().max(d.dy.abs()) < CLICK_TRAVEL {
                self.sidebar_drag = Some(drag);
                return;
            }
            drag.moving = true;
        }
        let hover = self.sidebar_drop_folder(d.window_point());
        if !up {
            self.sidebar_drag = Some(SidebarDrag { hover, ..drag });
            return;
        }
        if d.cancelled() {
            return;
        }
        // The one edit: the Move menu's own entry point. A release that is not
        // over a drawn folder header changes nothing.
        if let Some(folder) = hover {
            self.drop_dragged_note(drag, &folder);
        }
    }

    /// #293: the release of a held drag over `folder`. The note is re-resolved
    /// from its path here — vault ids are scan positions, and a watcher reload
    /// between the press and this drop reassigns them, so the id captured at
    /// the press would move whatever note now sits at that index. A note that
    /// vanished mid-drag moves nothing.
    fn drop_dragged_note(&mut self, drag: SidebarDrag, folder: &str) {
        let Some(id) = self
            .vault
            .notes
            .iter()
            .find(|note| note.path == drag.path)
            .map(|note| note.id)
        else {
            return;
        };
        self.move_note_to(id, folder);
    }

    /// The drawn folder header under `at`, in window points.
    fn sidebar_drop_folder(&self, at: [f32; 2]) -> Option<String> {
        let drawn = self.drawn_folder_headers.borrow();
        let rects = self.folder_header_rects.borrow();
        let (folder, _) = drawn
            .iter()
            .filter_map(|folder| Some((folder, rects.get(folder)?.get())))
            .find(|(_, [x, y, w, h])| {
                *w > 0.0
                    && *h > 0.0
                    && at[0] >= *x
                    && at[0] <= x + w
                    && at[1] >= *y
                    && at[1] <= y + h
            })?;
        Some(folder.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_is_hit_only_where_it_was_drawn_and_only_while_drawn() {
        let vault = muninn_core::testing::TempVault::new(&[("Alpha.md", "# Alpha\n")]);
        let home = std::env::temp_dir().join(format!(
            "muninn-sidebar-drag-unit-state-{}",
            std::process::id()
        ));
        muninn_core::testing::remove_on_thread_exit(&home);
        let s = State::with_state_home(vault.path().to_path_buf(), home);
        s.mark_folder_header("dest");
        s.folder_header_rect("dest").set([20.0, 100.0, 200.0, 30.0]);
        // A folder hidden by the filter keeps its rect but is not a target.
        s.folder_header_rect("gone").set([20.0, 140.0, 200.0, 30.0]);
        assert_eq!(
            s.sidebar_drop_folder([30.0, 110.0]).as_deref(),
            Some("dest")
        );
        assert_eq!(s.sidebar_drop_folder([30.0, 150.0]), None);
        assert_eq!(s.sidebar_drop_folder([300.0, 110.0]), None);
        s.clear_drawn_folder_headers();
        assert_eq!(s.sidebar_drop_folder([30.0, 110.0]), None);
    }

    // #293: vault ids are scan positions, so a watcher reload between the
    // press and the release reassigns them. The drag holds the path, so the
    // drop still moves the pressed note and not whatever note now sits at
    // its old id.
    #[test]
    fn a_reload_mid_drag_still_moves_the_pressed_note() {
        let vault = muninn_core::testing::TempVault::new(&[
            ("Alpha.md", "# Alpha\n"),
            ("dest/keep.md", "# Keep\n"),
        ]);
        let home = std::env::temp_dir().join(format!(
            "muninn-sidebar-drag-reload-state-{}",
            std::process::id()
        ));
        muninn_core::testing::remove_on_thread_exit(&home);
        let mut s = State::with_state_home(vault.path().to_path_buf(), home);
        assert_eq!(
            s.vault.notes[0].rel, "Alpha.md",
            "Alpha is id 0 at the press"
        );
        let drag = SidebarDrag {
            path: vault.path().join("Alpha.md"),
            hover: Some("dest".to_owned()),
            moving: true,
        };
        // An external note lands before Alpha in the scan order and the
        // watcher reloads the vault: AAA is now id 0.
        std::fs::write(vault.path().join("AAA.md"), "# AAA\n").unwrap();
        s.reload(None);
        assert_eq!(s.vault.notes[0].rel, "AAA.md");
        s.drop_dragged_note(drag, "dest");
        assert!(vault.path().join("dest/Alpha.md").is_file());
        assert!(vault.path().join("AAA.md").is_file(), "AAA stays put");
        assert!(!vault.path().join("Alpha.md").exists());
    }
}
