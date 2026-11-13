//! Kanban slice 12: the archive page and the actions around it.
//!
//! - A card's menu has **Archive**, and `Cmd`/`Ctrl`-clicking its checkbox
//!   archives it (a plain click still toggles the done mark). A card in a done
//!   lane archives the same way.
//! - The header's **Archive** button opens a page of the archive as it is in
//!   the same note: each archived card with **Restore** (to the first lane),
//!   a lane menu (**Restore to…**), and **Delete**, plus the count.
//! - Every action is one `edit_kanban` call, so it is one undo step, shows the
//!   board as unsaved and follows `⌘S` / autosave. Deleting or restoring reads
//!   the archive from the same parse the page showed, never from a second
//!   copy.
//! - The timestamp and archive limit come from the board footer, then
//!   `kanban_defaults` in `settings.json`, then the built-in value. A date is
//!   written through slice 9's formatter and is ordinary card text.
//! - Viewing the archive writes nothing.
//! - Known limitation: the checkbox reads the modifier keys when its press
//!   handler runs (the frame after the button went down), not when the button
//!   went down. Releasing `Cmd`/`Ctrl` within that one frame can archive where
//!   a plain toggle was meant (or toggle where an archive was meant). The
//!   card menu's **Archive** has no such window.

use muninn_core::{ArchiveSettings, Kanban, scan_card_text};
use rune::{El, col, row, text};
use rune_kit::Kit;

use crate::kanban_dates::local_today;
use crate::{ContextMenu, MenuTarget, State};

impl State {
    /// The archive settings the shown board writes with: its footer first,
    /// then `kanban_defaults` from `settings.json`, then the built-in value.
    #[must_use]
    pub fn kanban_archive_settings(&self) -> ArchiveSettings {
        ArchiveSettings::resolve(
            self.kanban_board().and_then(Kanban::footer_json),
            &self.settings.kanban_defaults,
        )
    }

    /// The header's Archive button: show the archive page, or the lanes.
    pub fn toggle_kanban_archive(&mut self) {
        self.kanban_archive_open = !self.kanban_archive_open;
        self.menu = None;
    }

    pub fn close_kanban_archive(&mut self) {
        self.kanban_archive_open = false;
        self.menu = None;
    }

    /// **Archive** in a card's menu, and `Cmd`/`Ctrl`-clicking its checkbox:
    /// move the card's lines into the archive as one undo step.
    pub fn archive_card(&mut self, lane: usize, index: usize) {
        let Some((lane, index)) = self.settle_card_edit(lane, index) else {
            return;
        };
        let settings = self.kanban_archive_settings();
        let archived = self.edit_kanban("archive the card", |board| {
            board.archive_card_with(lane, index, &settings, Some(local_today()))
        });
        if archived && self.kanban_selected == Some((lane, index)) {
            self.kanban_selected = None;
        }
    }

    /// **Restore** on an archived card: back to the end of the first lane.
    pub fn restore_archived_card(&mut self, index: usize) {
        self.restore_archived_card_to(index, 0);
    }

    /// **Restore to…** on an archived card: back to the end of `lane`.
    pub fn restore_archived_card_to(&mut self, index: usize, lane: usize) {
        self.menu = None;
        let Some(len) = self.kanban_lane_len(lane) else {
            return;
        };
        let restored = self.edit_kanban("restore the archived card", |board| {
            board.unarchive_card(index, lane)
        });
        if restored {
            self.kanban_selected = Some((lane, len));
        }
    }

    /// **Delete** on an archived card: remove only its lines.
    pub fn delete_archived_card(&mut self, index: usize) {
        self.menu = None;
        self.edit_kanban("delete the archived card", |board| {
            board.delete_archived_card(index)
        });
    }

    /// **Restore to…**: the lane menu under archive card `index`.
    pub fn open_archive_restore(&mut self, index: usize) {
        self.menu = Some(ContextMenu {
            target: MenuTarget::ArchiveRestore(index),
            x: 0.0,
            y: 0.0,
        });
    }
}

/// A button in the archive page: `Restore`, `Restore to…`, `Delete`, `Close`.
fn archive_button(
    k: &Kit,
    key: String,
    label: &'static str,
    color: rune::Color,
    action: impl Fn(&mut State) + 'static,
) -> El {
    row()
        .key(key)
        .h(26.0)
        .px(9.0)
        .gap(5.0)
        .radius(6.0)
        .center_y()
        .border(1.0, k.c.border_muted)
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(label)
        .on_press(action)
        .child(text(label).font(k.ty.ui_compact).color(color))
}

/// The Archive page: the header's count and Close, then one row per archived
/// card as the same file holds it.
pub(crate) fn archive_panel(s: &State, k: &Kit, board: &Kanban) -> El {
    let cards = board.archive().map_or(&[][..], |archive| archive.cards());
    let count = match cards.len() {
        1 => "1 archived card".to_owned(),
        n => format!("{n} archived cards"),
    };
    let dates = s.kanban_date_settings();
    let tags = s.kanban_tag_settings();
    let mut panel = col().key("kanban-archive-panel").w_full().gap(8.0).child(
        row()
            .w_full()
            .gap(10.0)
            .center_y()
            .child(
                text("Archive")
                    .key("kanban-archive-title")
                    .font(k.ty.ui_medium)
                    .color(k.c.text_base),
            )
            .child(
                text(count)
                    .key("kanban-archive-count")
                    .font(k.ty.caption())
                    .color(k.c.text_faint)
                    .grow(1.0),
            )
            .child(archive_button(
                k,
                "kanban-archive-close".to_owned(),
                "Close",
                k.c.text_base,
                State::close_kanban_archive,
            )),
    );
    if cards.is_empty() {
        return scroll_page(
            panel.child(
                text("No archived cards.")
                    .key("kanban-archive-empty")
                    .font(k.ty.caption())
                    .color(k.c.text_faint),
            ),
        );
    }
    for (index, card) in cards.iter().enumerate() {
        // The same display form as a lane card: a date or tag the board hides
        // stays in the file, not in the row.
        let found = scan_card_text(card.text(), &dates);
        let shown = found.without(
            card.text(),
            dates.hide_date_in_title,
            tags.hide_tags_in_title,
        );
        let entry = row()
            .key(format!("kanban-archive-card-{index}"))
            .w_full()
            .padding(10.0)
            .gap(8.0)
            .radius(8.0)
            .items_start()
            .fill(k.c.layer_02)
            .border(1.0, k.c.border_muted)
            .role("listitem")
            .label(shown.clone())
            .child(
                col().grow(1.0).min_w(0.0).child(
                    text(shown)
                        .key(format!("kanban-archive-text-{index}"))
                        .font(k.ty.ui_compact)
                        .color(if card.is_done() {
                            k.c.text_faint
                        } else {
                            k.c.text_base
                        })
                        .wrap_anywhere()
                        .w_full(),
                ),
            )
            .child(archive_button(
                k,
                format!("kanban-archive-restore-{index}"),
                "Restore",
                k.c.text_accent,
                move |s: &mut State| s.restore_archived_card(index),
            ))
            .child(archive_button(
                k,
                format!("kanban-archive-lanes-{index}"),
                "Restore to…",
                k.c.text_base,
                move |s: &mut State| s.open_archive_restore(index),
            ))
            .child(archive_button(
                k,
                format!("kanban-archive-delete-{index}"),
                "Delete",
                k.c.danger.fg,
                move |s: &mut State| s.delete_archived_card(index),
            ));
        let mut item = col()
            .key(format!("kanban-archive-item-{index}"))
            .w_full()
            .gap(4.0)
            .child(entry);
        if s.menu
            .as_ref()
            .is_some_and(|menu| menu.target == MenuTarget::ArchiveRestore(index))
        {
            item = item.child(archive_lane_picker(s, k, index));
        }
        panel = panel.child(item);
    }
    scroll_page(panel)
}

/// The archive page's viewport: a long archive scrolls, like the lanes do, so
/// every row stays reachable. `grow` and `min_h(0.0)` give it the body's
/// bounded height instead of growing to fit its contents.
fn scroll_page(panel: El) -> El {
    col()
        .key("kanban-archive-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll_xy()
        .child(panel)
}

/// Restore to…: every lane by title, under the archived card.
fn archive_lane_picker(s: &State, k: &Kit, index: usize) -> El {
    let lanes = s.kanban_board().map(Kanban::lanes).unwrap_or_default();
    let mut list = col()
        .key(format!("kanban-archive-lanes-{index}-menu"))
        .w_full()
        .gap(2.0)
        .py(4.0)
        .radius(8.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base);
    for (lane, other) in lanes.iter().enumerate() {
        let label = other.title().to_owned();
        list = list.child(
            row()
                .key(format!("kanban-archive-lane-{index}-{lane}"))
                .w_full()
                .h(28.0)
                .px(10.0)
                .radius(6.0)
                .center_y()
                .cursor(rune::Cursor::Pointer)
                .hover(k.c.hover)
                .press(k.c.pressed)
                .role("button")
                .label(format!("Restore to {}", other.title()))
                .on_press(move |s: &mut State| s.restore_archived_card_to(index, lane))
                .child(text(label).font(k.ty.ui_compact).color(k.c.text_base)),
        );
    }
    list.child(
        row()
            .key(format!("kanban-archive-lanes-{index}-cancel"))
            .w_full()
            .h(28.0)
            .px(10.0)
            .radius(6.0)
            .center_y()
            .cursor(rune::Cursor::Pointer)
            .hover(k.c.hover)
            .press(k.c.pressed)
            .role("button")
            .label("Cancel")
            .on_press(|s: &mut State| s.close_menu())
            .child(text("Cancel").font(k.ty.ui_compact).color(k.c.text_base)),
    )
}
