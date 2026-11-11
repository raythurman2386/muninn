//! The Board page (`Mode::Board`): the vault's `.excalidraw` boards and an
//! empty board surface.
//!
//! Slice 2 of the whiteboard work only wires the mode. The page lists the
//! boards found on disk with their element counts (read through
//! `muninn_core::Board`) and shows a placeholder where the canvas will go.
//! Nothing here writes a board: drawing, the camera and saving come later.

use std::path::PathBuf;

use muninn_core::{Board, BoardFile};
use rune::{Cursor, El, col, icon, row, spacer, text};
use rune_kit::Kit;
use rune_theme_opencode::icons as ic;

use crate::State;

/// One board in the vault, as last read from disk.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardSummary {
    pub file: BoardFile,
    /// Live (drawn) elements and how many of all elements Muninn keeps as-is
    /// because it does not model their type, or why the file could not be read.
    pub counts: Result<BoardCounts, String>,
}

/// Element counts for [`BoardSummary`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardCounts {
    pub live: usize,
    pub raw: usize,
}

impl BoardSummary {
    fn read(file: BoardFile) -> BoardSummary {
        let counts = Board::load(&file.path)
            .map(|board| BoardCounts {
                live: board.live_elements().count(),
                raw: board
                    .elements
                    .iter()
                    .filter(|e| e.shape().is_none())
                    .count(),
            })
            .map_err(|e| e.to_string());
        BoardSummary { file, counts }
    }

    /// `12 elements`, `1 element · 2 kept as-is`, or the read error.
    #[must_use]
    pub fn detail(&self) -> String {
        match &self.counts {
            Ok(c) if c.raw == 0 => plural(c.live, "element", "elements"),
            Ok(c) => format!(
                "{} · {} kept as-is",
                plural(c.live, "element", "elements"),
                c.raw
            ),
            Err(e) => format!("Could not read: {e}"),
        }
    }
}

impl State {
    /// Re-read the vault's boards from disk. The file is the source of truth,
    /// so this runs on entering Board mode and after a vault reload rather
    /// than caching across sessions. A selection whose file is gone clears.
    pub fn refresh_boards(&mut self) {
        self.boards = self
            .vault
            .boards()
            .into_iter()
            .map(BoardSummary::read)
            .collect();
        if let Some(path) = &self.board_selected
            && !self.boards.iter().any(|b| &b.file.path == path)
        {
            self.board_selected = None;
        }
    }

    /// Select a listed board by its absolute path (`BoardFile::path`) for the
    /// surface header.
    pub fn select_board(&mut self, path: PathBuf) {
        self.board_selected = Some(path);
    }

    /// The selected board's summary, if it is still listed.
    #[must_use]
    pub fn selected_board(&self) -> Option<&BoardSummary> {
        let path = self.board_selected.as_ref()?;
        self.boards.iter().find(|b| &b.file.path == path)
    }
}

pub fn board(s: &State, k: &Kit) -> El {
    col()
        .key("board-page")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(header(s, k))
        .child(
            row()
                .key("board-body")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .gap(16.0)
                .px(28.0)
                .pb(24.0)
                .child(board_list(s, k))
                .child(surface(s, k)),
        )
}

fn header(s: &State, k: &Kit) -> El {
    row()
        .h(58.0)
        .px(28.0)
        .gap(10.0)
        .center_y()
        .child(text("Boards").font(k.ty.ui_medium).color(k.c.text_base))
        .child(
            text(plural(s.boards.len(), "board", "boards"))
                .key("board-count")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
        .child(spacer())
}

fn board_list(s: &State, k: &Kit) -> El {
    let mut list = col()
        .key("board-list")
        .w(240.0)
        .gap(4.0)
        .focus_group()
        .role("list");
    if s.boards.is_empty() {
        list = list.child(
            text("No boards in this vault yet")
                .key("board-list-empty")
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .padding(8.0),
        );
    }
    for (i, b) in s.boards.iter().enumerate() {
        let on = s.board_selected.as_ref() == Some(&b.file.path);
        let path = b.file.path.clone();
        list = list.child(
            col()
                .key(format!("board-row-{i}"))
                .w_full()
                .px(10.0)
                .py(7.0)
                .gap(2.0)
                .radius(7.0)
                .fill(if on {
                    k.c.layer_03
                } else {
                    rune::Color::TRANSPARENT
                })
                .hover(k.c.hover)
                .cursor(Cursor::Pointer)
                .focusable(true)
                .focus_ring(2.0, 1.0, k.c.text_accent)
                .on_press(move |s: &mut State| s.select_board(path.clone()))
                .child(
                    text(b.file.title.clone())
                        .font(k.ty.ui_compact)
                        .color(k.c.text_base)
                        .truncate(),
                )
                .child(
                    text(b.detail())
                        .font(k.ty.caption())
                        .color(k.c.text_faint)
                        .truncate(),
                ),
        );
    }
    list
}

fn surface(s: &State, k: &Kit) -> El {
    let (title, detail) = match s.selected_board() {
        Some(b) => (
            b.file.title.clone(),
            format!("{} · {}", b.file.rel, b.detail()),
        ),
        None => (
            "Empty board".to_owned(),
            "Select a board to see what it holds".to_owned(),
        ),
    };
    col()
        .key("board-surface")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .radius(10.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
        .center()
        .gap(8.0)
        .child(icon(&ic::DOT_GRID, 22.0).color(k.c.icon_muted))
        .child(
            text(title)
                .key("board-surface-title")
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        )
        .child(
            text(detail)
                .key("board-surface-detail")
                .font(k.ty.caption())
                .color(k.c.text_muted),
        )
        .child(
            text("Drawing arrives in a later update. Boards are Excalidraw files in your vault.")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
