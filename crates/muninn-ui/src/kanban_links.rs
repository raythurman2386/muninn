//! Kanban slice 8: links on a card, and a rename that does not clobber the page.
//!
//! - **Open a link.** A `[[link]]` in a card opens that note, through the same
//!   resolution Write and Read use. The page commits an open editor first; a
//!   refused commit stays put.
//! - **Autocomplete.** `[[` in the card editor offers notes, using the Write
//!   editor's `unclosed_link` detection.
//! - **Embeds.** A note embed (`![[Note]]`) renders as a link. The format doc
//!   allows the embed in the card text and does not say to inline the note, so
//!   the note is not copied in. An attachment embed keeps the reader's
//!   filename placeholder. Dates, times and tags stay as text.
//! - **Copy link to card.** The card gains a block id (`^card-N`) when it has
//!   none. That id is written once and is not changed afterwards. The copied
//!   text is `[[Note#^id]]`.
//! - **New note from card.** A note is created in the folder New note would
//!   use, with the note template in front of the card text. The card's text
//!   becomes a link to that note.
//!
//! Renaming a note the open kanban links to, and renaming the kanban itself,
//! is handled next to the other note renames. A dirty kanban is left
//! unrewritten and named in a notice. A renamed kanban keeps its new path in
//! settings.

use std::path::Path;

use muninn_core::atomic_write;
use rune::Tx;

use crate::kanban_edit::CardCommit;
use crate::{MenuTarget, State};

impl State {
    /// A wikilink clicked in a card. An open editor is committed first.
    pub fn open_card_link(&mut self, url: &str, tx: &mut Tx) {
        if self.commit_card_edit() == CardCommit::Refused {
            return;
        }
        self.close_picker();
        if !self.mode.is_note_mode() {
            self.set_mode(self.last_note_mode);
        }
        self.open_link(url, tx);
    }

    /// Copy link to card, from the menu that is open on that card.
    pub fn copy_open_card_link(&mut self) -> Option<String> {
        let (lane, index) = match self.menu.as_ref().map(|menu| menu.target.clone()) {
            Some(MenuTarget::Card(lane, index)) => (lane, index),
            _ => return None,
        };
        self.menu = None;
        self.copy_card_link(lane, index)
    }

    /// Give the card a block id if it needs one, and return `[[Note#^id]]`.
    /// Nothing is copied when the card is gone or the edit is refused.
    pub fn copy_card_link(&mut self, lane: usize, index: usize) -> Option<String> {
        let (lane, index) = self.settle_card_edit(lane, index)?;
        let stem = {
            let rel = self.kanban.as_ref()?.rel.clone();
            let id = self.note_id(&rel)?;
            self.vault.notes.get(id)?.stem().to_owned()
        };
        let mut assigned = None;
        let ok = self.edit_kanban("copy the card link", |board| {
            assigned = Some(board.assign_block_id(lane, index)?);
            Ok(())
        });
        let id = assigned.filter(|_| ok)?;
        let link = format!("[[{stem}#^{id}]]");
        self.log_event("kanban", &format!("link {link}"));
        Some(link)
    }

    /// New note from card: the card text becomes a note, then a link to it.
    pub fn new_note_from_card(&mut self, lane: usize, index: usize) {
        let Some((lane, index)) = self.settle_card_edit(lane, index) else {
            return;
        };
        let Some(text) = self
            .kanban_card_at(lane, index)
            .map(|card| card.text().to_owned())
        else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }
        let title = text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("Untitled");
        let folder = self.current_folder();
        let path = match self.vault.create_note_in(title, &folder) {
            Ok(path) => path,
            Err(e) => {
                self.log_event("kanban", &format!("note from card not created: {e}"));
                self.notice = Some(format!("Could not create a note: {e}"));
                return;
            }
        };
        let body = note_from_card_body(&self.settings.note_template, &text);
        if let Err(e) = atomic_write(&path, body.as_bytes()) {
            self.log_event("kanban", &format!("note from card not written: {e}"));
            self.notice = Some(format!("Could not create a note: {e}"));
            return;
        }
        let stem = vault_stem(&self.vault.root, &path);
        let link = format!("[[{stem}]]");
        if !self.edit_kanban("make a note from the card", |board| {
            board.edit_card(lane, index, &link)
        }) {
            return;
        }
        self.log_event("kanban", &format!("note from card {stem}"));
        // Index the new note. The kanban is dirty, so the page keeps the edit.
        let keep = self.selected_path.clone();
        self.reload(keep);
    }
}

/// The note template, then the card text, each ending in a newline.
fn note_from_card_body(template: &str, card: &str) -> String {
    let mut body = String::new();
    if !template.is_empty() {
        body.push_str(template);
        if !body.ends_with('\n') {
            body.push('\n');
        }
    }
    body.push_str(card);
    if !body.ends_with('\n') {
        body.push('\n');
    }
    body
}

fn vault_stem(root: &Path, path: &Path) -> String {
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    rel.strip_suffix(".md").unwrap_or(&rel).to_owned()
}
