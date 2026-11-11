//! Board rename in the app (#43; Raymond: "I can't seem to update the name of
//! the new board").
//!
//! Three ways in, one inline name field:
//! - **F2** on Board (when no text field has the keyboard) renames the open
//!   board in its Boards-panel row, or in its title when it has no row;
//! - **Rename** in the right-click menu of a board row, in the Board panel
//!   or in the notes sidebar's Boards section;
//! - a **double click** on the open board's title above the canvas.
//!
//! The field holds the name without `.excalidraw` (it stays implicit), all
//! of it selected; Enter renames, Esc cancels. A taken or invalid name is
//! refused with the reason under the field, which stays open for a fix.
//! A rename only starts where its field is drawn, and is cancelled when
//! that field goes away, so it never holds the keyboard out of sight.
//!
//! The rename itself ([`State::rename_board`]): pending board text is
//! committed and the open board saved first (a refused save renames
//! nothing); [`muninn_core::Vault::rename_board`] does one atomic
//! `fs::rename` inside the vault; then the open board, its selection, view,
//! recovery copy and the watchers' baselines move to the new path, so
//! nothing is lost, re-offered or mistaken for another app's delete; and
//! notes that embed or link the board are rewritten like a note rename's
//! inbound links.

use std::path::{Path, PathBuf};

use rune::{El, Key, KeyEvent, Tx, col, row, text, text_input};
use rune_kit::Kit;

use crate::{Mode, State};

/// Where a board's rename field is drawn (and which menu asked for it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardPlace {
    /// A row in Board mode's Boards panel (F2 uses this one).
    Panel,
    /// A row in the notes sidebar's Boards section.
    Sidebar,
    /// The open board's title above the canvas.
    Header,
}

/// A board rename being typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardRename {
    pub path: PathBuf,
    pub place: BoardPlace,
    /// Why the last Enter was refused, shown under the field.
    pub error: Option<String>,
}

impl State {
    /// Open the inline name field for the board at `path`, drawn at `place`.
    pub fn begin_rename_board(&mut self, path: PathBuf, place: BoardPlace) -> bool {
        self.menu = None;
        // GK MF-B: a rename has the keyboard (`is_typing`), so it only starts
        // where its field will be drawn. A board with no row there (in a
        // hidden folder, or opened from outside the vault through an embed)
        // is refused instead of taking Board's keys into an unseen field.
        let listed = match place {
            BoardPlace::Sidebar => {
                self.search.text.trim().is_empty()
                    && self.sidebar_boards.iter().any(|b| b.path == path)
            }
            BoardPlace::Panel | BoardPlace::Header => self.board_rename_shows(&path, place),
        };
        if !listed {
            self.log_event(
                "board",
                &format!(
                    "rename not offered: {} has no {place:?} row",
                    path.display()
                ),
            );
            return false;
        }
        // One text field at a time: a board text editor commits, a note or
        // folder prompt goes, as they do for each other.
        self.commit_board_text();
        self.cancel_new_note();
        self.cancel_rename_note();
        self.cancel_new_folder();
        self.cancel_rename_folder();
        if place == BoardPlace::Sidebar {
            self.reveal_sidebar();
        }
        self.dismiss_palette();
        self.vault_field.focused = false;
        self.editor.focused = false;
        self.search.focused = false;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.board_rename_field.replace_text(stem.as_str());
        // All of it selected: typing replaces the name, arrows edit it.
        self.board_rename_field.set_selection(0, stem.len());
        self.board_rename_field.focused = true;
        self.board_rename = Some(BoardRename {
            path,
            place,
            error: None,
        });
        true
    }

    /// F2 on Board: rename the open board in its panel row, or in its title
    /// when it has no row (a board in a hidden folder, or one opened from
    /// outside the vault through an embed).
    pub fn begin_rename_open_board(&mut self, place: BoardPlace) -> bool {
        let Some(path) = self.board_open.as_ref().map(|open| open.path.clone()) else {
            return false;
        };
        let place = if place == BoardPlace::Panel && !self.board_rename_shows(&path, place) {
            BoardPlace::Header
        } else {
            place
        };
        self.begin_rename_board(path, place)
    }

    /// Whether a name field for `path` at `place` is drawn right now: the
    /// Board panel lists it, the shown sidebar lists it (not while a search
    /// hides its Boards section), or it is the open board's title on Board.
    pub(crate) fn board_rename_shows(&self, path: &Path, place: BoardPlace) -> bool {
        match place {
            BoardPlace::Panel => {
                self.mode == Mode::Board && self.boards.iter().any(|b| b.file.path == path)
            }
            BoardPlace::Sidebar => {
                self.sidebar_shown()
                    && self.search.text.trim().is_empty()
                    && self.sidebar_boards.iter().any(|b| b.path == path)
            }
            BoardPlace::Header => {
                self.mode == Mode::Board
                    && self
                        .board_open
                        .as_ref()
                        .is_some_and(|open| open.path == path)
            }
        }
    }

    /// Per frame (GK MF-B): a rename whose field is no longer drawn (its row
    /// went with a re-list, say, after another app deleted the board) is
    /// cancelled, so it never keeps the keyboard out of sight.
    pub(crate) fn board_rename_keep_on_screen(&mut self) -> bool {
        let gone = self
            .board_rename
            .as_ref()
            .is_some_and(|r| !self.board_rename_shows(&r.path, r.place));
        if gone {
            self.cancel_rename_board();
        }
        gone
    }

    pub fn cancel_rename_board(&mut self) {
        self.board_rename = None;
        self.board_rename_field.focused = false;
    }

    /// Enter in the name field: rename, or keep the field open with why not.
    pub fn submit_rename_board(&mut self) {
        let Some(prompt) = self.board_rename.clone() else {
            return;
        };
        let name = self.board_rename_field.text.clone();
        match self.rename_board(&prompt.path, &name) {
            Ok(_) => self.cancel_rename_board(),
            Err(why) => {
                self.log_event("board", &format!("rename refused: {why}"));
                if let Some(prompt) = &mut self.board_rename {
                    prompt.error = Some(why);
                }
                self.board_rename_field.focused = true;
            }
        }
    }

    /// Rename the board at `path` to `name` (no extension needed). Returns
    /// the new path, or why it was not renamed (nothing changed then).
    pub fn rename_board(&mut self, path: &Path, name: &str) -> Result<PathBuf, String> {
        let name = muninn_core::board_name(name).map_err(|e| capitalise(&e.to_string()))?;
        let is_open = self
            .board_open
            .as_ref()
            .is_some_and(|open| open.path == path);
        if is_open {
            // Typed text and unsaved drawing go to the file first, under the
            // old name (its recovery copy goes with the save). A save that is
            // refused (the file changed or vanished) renames nothing.
            self.commit_board_text();
            if !self.save_board() {
                return Err("Save the board first: it could not be saved".to_owned());
            }
        }
        let old_rel = self.vault_rel(path);
        // The open note is written first, so a rewritten embed in it is
        // read back rather than written over.
        self.flush();
        let inbound = self.vault.board_embed_referrers(path);
        let target = self
            .vault
            .rename_board(path, &name)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::AlreadyExists => {
                    format!("A board named \u{201c}{name}\u{201d} is already in this folder")
                }
                _ => capitalise(&e.to_string()),
            })?;
        if target == path {
            return Ok(target);
        }
        let new_rel = self.vault_rel(&target);
        self.board_follows_rename(path, &target, &old_rel, &new_rel);
        let failures = self.vault.rewrite_board_embeds(&inbound, &target);
        self.log_event("board", &format!("renamed {old_rel} to {new_rel}"));
        if !inbound.is_empty() {
            // Like a note rename: the vault reloads, and with it the open
            // note's text (now holding the new embed).
            self.reload(self.selected_path.clone());
            // A reload closes an open board the list does not hold (one in
            // a hidden folder, or outside the vault); it was saved above, so
            // open it again under its new name.
            if is_open && self.board_open.is_none() {
                self.select_board(target.clone());
            }
        }
        if !failures.is_empty() {
            self.error = Some(format!(
                "Renamed, but {} note(s) embedding it could not be updated: {}",
                failures.len(),
                self.format_rewrite_failures(&failures)
            ));
        }
        Ok(target)
    }

    /// Everything keyed by the board's path or vault-relative name moves to
    /// the new one: the open board, the selection and stored view, a
    /// pending restore, recovery copies, the list and the watchers'
    /// baselines (so the old path vanishing is not another app's delete).
    fn board_follows_rename(&mut self, old: &Path, new: &Path, old_rel: &str, new_rel: &str) {
        if let Some(open) = &mut self.board_open
            && open.path == old
        {
            open.path = new.to_path_buf();
        }
        if self.board_selected.as_deref() == Some(old) {
            self.board_selected = Some(new.to_path_buf());
        }
        if let Some(view) = self.board_views.remove(old) {
            self.board_views.insert(new.to_path_buf(), view);
        }
        if let Some(restore) = &mut self.board_restore
            && restore.rel == old_rel
        {
            restore.rel = new_rel.to_owned();
        }
        // A recovery copy left under the old name (a board not open now, from
        // a crash) would be offered for a file that is gone: it moves too.
        if let Ok(copy) = self.recovery.load(old_rel)
            && self.recovery.save(new_rel, &copy).is_ok()
        {
            self.recovery.clear(old_rel);
        }
        for entry in &mut self.recovered {
            if entry.rel == old_rel {
                new_rel.clone_into(&mut entry.rel);
            }
        }
        self.board_stats.remove(old);
        self.relist_boards();
        self.board_written(new);
        if let Some(open) = &self.board_open
            && open.path == new
        {
            self.refresh_board_summary(new);
        }
        self.save_settings();
    }

    fn vault_rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.vault.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// Whether `place` shows the rename field for the board at `path`.
    #[must_use]
    pub fn renaming_board_at(&self, path: &Path, place: BoardPlace) -> bool {
        self.board_rename
            .as_ref()
            .is_some_and(|r| r.path == path && r.place == place)
    }

    /// The field's place is about to go out of sight: leaving Board takes
    /// the panel and the title with it.
    pub(crate) fn board_rename_left_board(&mut self, mode: Mode) {
        if mode != Mode::Board
            && self
                .board_rename
                .as_ref()
                .is_some_and(|r| r.place != BoardPlace::Sidebar)
        {
            self.cancel_rename_board();
        }
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + c.as_str()
    })
}

/// The inline name field (and, after a refused Enter, the reason).
pub(crate) fn rename_field(s: &State, k: &Kit) -> El {
    let error = s.board_rename.as_ref().and_then(|r| r.error.clone());
    let field = row()
        .key("board-rename-prompt")
        .w_full()
        .h(32.0)
        .px(8.0)
        .gap(6.0)
        .radius(7.0)
        .center_y()
        .fill(k.alpha(k.c.text_accent, 0.10))
        .child(
            text_input(&s.board_rename_field, k.ty.ui_compact)
                .key("board-rename-name")
                .label("Board name")
                .single_line()
                .grow(1.0)
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .on_edit(|s: &mut State| &mut s.board_rename_field)
                .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                    if e.key == Key::Escape {
                        s.cancel_rename_board();
                        tx.blur();
                        return true;
                    }
                    crate::shortcut(s, e, tx)
                })
                .on_submit(|s: &mut State, _| s.submit_rename_board()),
        )
        .child(
            text(".excalidraw")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        );
    let Some(error) = error else {
        return field;
    };
    col().w_full().gap(2.0).child(field).child(
        text(error)
            .key("board-rename-error")
            .font(k.ty.caption())
            .color(k.c.danger.fg)
            .padding(4.0),
    )
}
