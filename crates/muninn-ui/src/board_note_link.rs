//! Board slice 8 (#73): linking elements to vault notes.
//!
//! - With one or more elements selected, **Link note** in the toolbar or
//!   `⌘L` / `Ctrl+L` opens a small prompt under the toolbar. Type a note name
//!   (its stem) or a vault-relative path; Enter resolves it against the vault
//!   and writes a `muninn://note/…` link on every selected element as one
//!   history step ([`muninn_core::Board::set_note_link`]). An empty value
//!   clears the link. An unknown note keeps the prompt open with the reason
//!   under it.
//! - `⌘Enter` / `Ctrl+Enter` opens the selected element's linked note, and so
//!   does `⌘click` / `Ctrl+click` on a linked element. A click without the
//!   modifier still only selects, and a `link` that is not a note link (an
//!   external URL) is never opened.
//! - A linked element is marked on the canvas (`board_scene`), and the hint
//!   above the canvas names the linked note while a linked element is
//!   selected.
//!
//! The link lives in Excalidraw's own `link` element field, so the file stays
//! an ordinary Excalidraw scene; see `muninn_core` `board::link`. While the
//! prompt is open [`State::is_typing`] is true, so no Board key reaches the
//! canvas, and leaving Board (including opening the note) closes it.

use muninn_core::Shape;
use rune::{El, Key, KeyEvent, Tx, col, row, text, text_input};
use rune_kit::Kit;

use crate::{Mode, State};

/// A link prompt being typed. Its text lives on
/// [`State::board_note_link_field`] so Rune always has a valid field to edit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardNoteLink {
    /// Why the last Enter was refused, shown under the field.
    pub error: Option<String>,
}

impl State {
    /// Open the note-link prompt for the current selection. `false` when
    /// Board is not showing, no board is open, or nothing is selected.
    pub fn begin_board_note_link(&mut self) -> bool {
        if self.mode != Mode::Board {
            return false;
        }
        let Some(open) = self.board_open.as_ref() else {
            return false;
        };
        let Some(model) = open.model.as_ref() else {
            return false;
        };
        let selection = open.edit.selection.clone();
        let Some(first) = selection.first() else {
            return false;
        };
        // Prefill only when every selected element already links the same
        // note; otherwise the field starts empty (typing sets a new link).
        let shared = model
            .shape(first)
            .and_then(Shape::note_link)
            .filter(|link| {
                selection.iter().all(|id| {
                    model.shape(id).and_then(Shape::note_link).as_deref() == Some(link.as_str())
                })
            });
        let shared = shared.unwrap_or_default();

        // One text field at a time, as the rename prompt does.
        self.commit_board_text();
        self.cancel_rename_board();
        self.cancel_new_note();
        self.cancel_rename_note();
        self.cancel_new_folder();
        self.cancel_rename_folder();
        self.dismiss_palette();
        self.menu = None;
        self.board_palette = false;
        self.vault_field.focused = false;
        self.set_text_focus(false, false);
        self.board_note_link_field.replace_text(&shared);
        self.board_note_link_field.set_selection(0, shared.len());
        self.board_note_link_field.focused = true;
        self.board_note_link = Some(BoardNoteLink { error: None });
        true
    }

    /// Close the prompt without touching the selection or the board.
    pub fn cancel_board_note_link(&mut self) {
        self.board_note_link = None;
        self.board_note_link_field.focused = false;
    }

    /// Enter in the prompt: resolve the typed note name and link the
    /// selection, or keep the prompt open with why not.
    pub fn submit_board_note_link(&mut self) {
        if self.board_note_link.is_none() {
            return;
        }
        let typed = self.board_note_link_field.text.trim().to_owned();
        if typed.is_empty() {
            self.apply_board_note_link(None);
            return;
        }
        // A note name and a vault-relative path both work; a typed `.md` is
        // the file name, not part of the stem the link stores.
        let target = typed.strip_suffix(".md").unwrap_or(&typed).trim();
        let Some(id) = self.vault.resolve(target) else {
            let error = format!("No note named \u{201c}{typed}\u{201d} in this vault");
            self.log_event("board", &format!("note link refused: {error}"));
            if let Some(prompt) = &mut self.board_note_link {
                prompt.error = Some(error);
            }
            self.board_note_link_field.focused = true;
            return;
        };
        let stem = self.vault.note(id).stem().to_owned();
        self.apply_board_note_link(Some(&stem));
    }

    /// Set (`Some`) or clear (`None`) the selection's note link as one edit,
    /// then close the prompt. `false` when there is nothing to edit.
    fn apply_board_note_link(&mut self, target: Option<&str>) -> bool {
        let ids = self.board_selection().to_vec();
        if ids.is_empty() {
            return false;
        }
        let now = crate::board_tools::now_ms();
        let edit = {
            let Some(open) = &mut self.board_open else {
                return false;
            };
            let Some(model) = &mut open.model else {
                return false;
            };
            model.set_note_link(&ids, target, &mut self.board_ids, now)
        };
        if let Some(open) = &mut self.board_open {
            // An unchanged link records nothing; the prompt still closes.
            open.record(edit);
        }
        self.cancel_board_note_link();
        true
    }

    /// The vault-relative path of the first selected element's note link.
    #[must_use]
    pub fn board_selected_note_target(&self) -> Option<String> {
        let open = self.board_open.as_ref()?;
        let model = open.model.as_ref()?;
        open.edit
            .selection
            .iter()
            .find_map(|id| model.shape(id).and_then(Shape::note_link))
    }

    /// A label for the hint: the linked note's title, or the missing target
    /// (a link whose note was renamed, moved or deleted).
    #[must_use]
    pub fn board_selected_note_label(&self) -> Option<String> {
        let target = self.board_selected_note_target()?;
        Some(match self.vault.resolve(&target) {
            Some(id) => self.vault.note(id).title.clone(),
            None => format!("{target} (missing)"),
        })
    }

    /// `⌘Enter`: open the selected element's linked note. `false` when the
    /// selection has no note link, or its note is gone.
    pub fn open_board_note_link(&mut self) -> bool {
        let Some(target) = self.board_selected_note_target() else {
            return false;
        };
        self.open_board_note_target(&target)
    }

    /// `⌘click`: open the note linked to `element`, if any.
    pub fn open_board_note_link_for(&mut self, element: &str) -> bool {
        let Some(target) = self
            .board_open
            .as_ref()
            .and_then(|open| open.model.as_ref())
            .and_then(|model| model.shape(element))
            .and_then(Shape::note_link)
        else {
            return false;
        };
        self.open_board_note_target(&target)
    }

    /// Resolve a stored target against the vault and open it in Write. A
    /// target that no longer resolves leaves Board as it is.
    fn open_board_note_target(&mut self, target: &str) -> bool {
        let Some(id) = self.vault.resolve(target) else {
            self.log_event("board", &format!("note link is missing: {target}"));
            return false;
        };
        self.open_note(id);
        true
    }
}

/// The in-canvas prompt under the toolbar, when one is open.
pub(crate) fn prompt(s: &State, k: &Kit) -> Option<El> {
    let prompt = s.board_note_link.as_ref()?;
    let field = row()
        .key("board-note-link-row")
        .w_full()
        .h(32.0)
        .gap(6.0)
        .center_y()
        .child(
            text_input(&s.board_note_link_field, k.ty.ui_compact)
                .key("board-note-link-field")
                .label("Note name or path")
                .single_line()
                .grow(1.0)
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .on_edit(|s: &mut State| &mut s.board_note_link_field)
                .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                    if e.key == Key::Escape {
                        s.cancel_board_note_link();
                        tx.blur();
                        return true;
                    }
                    // The prompt's own chord must not fall through to the
                    // graph's Global / Local toggle behind it.
                    if e.is_command('l') {
                        return true;
                    }
                    crate::shortcut(s, e, tx)
                })
                .on_submit(|s: &mut State, _| s.submit_board_note_link()),
        );
    let mut card = col()
        .key("board-note-link-prompt")
        .role("group")
        .label("Link note")
        .pin(Some(12.0), Some(56.0), None, None)
        .w(320.0)
        .padding(8.0)
        .gap(4.0)
        .radius(7.0)
        // Opaque, like the Board palette panel (#72 contrast): a translucent
        // layer takes its colour from arbitrary board paper, and `text_faint`
        // on it could drop below 4.5:1.
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        .child(
            row()
                .h(18.0)
                .gap(6.0)
                .center_y()
                .child(text("Link note").font(k.ty.caption()).color(k.c.text_muted))
                .child(
                    text("a name or vault path · empty clears")
                        .font(k.ty.caption())
                        .color(k.c.text_faint),
                ),
        )
        .child(field);
    if let Some(error) = &prompt.error {
        card = card.child(
            text(error.clone())
                .key("board-note-link-error")
                .font(k.ty.caption())
                .color(k.c.danger.fg)
                .padding(2.0),
        );
    }
    Some(card)
}
