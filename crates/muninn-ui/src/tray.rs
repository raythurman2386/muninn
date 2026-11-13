//! The optional system tray icon (Settings › Appearance › Show tray icon).
//!
//! Rune owns the native icon ([`rune::Tray`]: StatusNotifierItem on Linux,
//! `Shell_NotifyIcon` on Windows, not available on macOS at the pinned
//! revision). Muninn owns the menu and what each pick does. The menu is flat
//! and small (Rune's tray has no submenus or checkmarks at the pinned
//! revision): show the window, start a new note, Sync now, the current sync
//! status as a disabled row, and quit.
//!
//! With Settings › Appearance › Close to tray on, closing the window hides it
//! instead of quitting, but only while a tray icon was actually created
//! ([`State::closes_to_tray`]). Rune cannot tell whether a tray host shows
//! the icon (stock GNOME has none), so this is opt-in.
//!
//! [`State`] holds the live tray behind [`TrayPort`] and creates it through a
//! [`TrayFactory`] the binary installs, so tests drive the whole on/off and
//! refresh cycle without a desktop session. Headless runs install no factory
//! and never create a tray.

use rune::{TrayEvent, TrayIcon, TrayItem, TraySpec};

use crate::State;

/// Menu id: show and focus the window.
pub const TRAY_SHOW: &str = "show";
/// Menu id: show the window with the new-note field open.
pub const TRAY_NEW_NOTE: &str = "new-note";
/// Menu id: run Sync Now for this vault.
pub const TRAY_SYNC_NOW: &str = "sync-now";
/// Menu id of the disabled sync status row.
pub const TRAY_SYNC_STATUS: &str = "sync-status";
/// Menu id: save and quit.
pub const TRAY_QUIT: &str = "quit";

/// The tray icon: `assets/logo.png` scaled to 64 × 64 straight RGBA
/// (`magick assets/logo.png -resize 64x64 -depth 8 RGBA:assets/tray-icon-64.rgba`).
const ICON_RGBA: &[u8] = include_bytes!("../../../assets/tray-icon-64.rgba");
const ICON_SIZE: u32 = 64;

/// Longest sync status shown in the menu row, in characters.
const STATUS_MAX: usize = 60;

/// What a tray event asks the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayAction {
    /// Show and focus the window. Carries the activation token a Wayland tray
    /// host hands over with a primary click, when there is one.
    Show(Option<String>),
    /// Show the window and open the new-note field ([`State::tray_new_note`]).
    NewNote,
    /// Run the Sync tab's Sync Now ([`State::tray_sync_now`]).
    SyncNow,
    /// Flush the open note and settings, then quit ([`State::flush_before_quit`]).
    Quit,
}

/// Map a Rune tray event to a Muninn action. The status row and unknown ids
/// do nothing.
#[must_use]
pub fn tray_action(event: &TrayEvent) -> Option<TrayAction> {
    match event {
        TrayEvent::Activate { token } => Some(TrayAction::Show(token.clone())),
        TrayEvent::Menu(id) => match id.as_str() {
            TRAY_SHOW => Some(TrayAction::Show(None)),
            TRAY_NEW_NOTE => Some(TrayAction::NewNote),
            TRAY_SYNC_NOW => Some(TrayAction::SyncNow),
            TRAY_QUIT => Some(TrayAction::Quit),
            _ => None,
        },
        _ => None,
    }
}

/// The tray menu for the given sync status line. `sync_ready` enables Sync
/// now ([`State::sync_ready`]).
#[must_use]
pub fn tray_menu(sync_status: &str, sync_ready: bool) -> Vec<TrayItem> {
    vec![
        TrayItem::action(TRAY_SHOW, "Show Muninn"),
        TrayItem::action(TRAY_NEW_NOTE, "New note"),
        TrayItem::action(TRAY_SYNC_NOW, "Sync now").enabled(sync_ready),
        TrayItem::Separator,
        TrayItem::action(TRAY_SYNC_STATUS, tray_status_label(sync_status)).enabled(false),
        TrayItem::Separator,
        TrayItem::action(TRAY_QUIT, "Quit Muninn"),
    ]
}

/// `Sync: ` plus the first sentence of the Settings › Sync status line,
/// capped at 60 characters.
#[must_use]
pub fn tray_status_label(status: &str) -> String {
    let status = status.trim();
    let first = status
        .find(". ")
        .map_or(status, |end| &status[..=end])
        .trim();
    let first = if first.is_empty() {
        "Not connected."
    } else {
        first
    };
    let label = if first.chars().count() > STATUS_MAX {
        let cut: String = first.chars().take(STATUS_MAX - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        first.to_owned()
    };
    format!("Sync: {label}")
}

/// Whether Rune has a tray backend for this build (Linux with D-Bus, Windows).
#[must_use]
pub const fn tray_supported() -> bool {
    cfg!(any(target_os = "linux", target_os = "windows"))
}

/// Muninn's tray spec: app id, title, the logo and `menu`.
#[must_use]
pub fn tray_spec(menu: Vec<TrayItem>) -> TraySpec {
    TraySpec::new("muninn", "Muninn", tray_icon())
        .tooltip("Muninn")
        .menu(menu)
}

/// The logo as a [`TrayIcon`].
#[must_use]
pub fn tray_icon() -> TrayIcon {
    TrayIcon::rgba(ICON_SIZE, ICON_SIZE, ICON_RGBA.to_vec())
}

/// A live tray. Dropping it removes the icon.
pub trait TrayPort {
    /// Replace the menu.
    fn set_menu(&self, menu: Vec<TrayItem>);
}

impl TrayPort for rune::Tray {
    fn set_menu(&self, menu: Vec<TrayItem>) {
        rune::Tray::set_menu(self, menu);
    }
}

/// Creates a live tray with the given menu, or says why it could not.
pub type TrayFactory = Box<dyn Fn(Vec<TrayItem>) -> Result<Box<dyn TrayPort>, String>>;

/// The tray as [`State`] holds it.
#[derive(Default)]
pub(crate) struct TrayLink {
    factory: Option<TrayFactory>,
    live: Option<Box<dyn TrayPort>>,
    /// The menu last handed to the live tray.
    menu: Vec<TrayItem>,
    /// Why the last creation failed. Cleared when the setting changes, so a
    /// failure is not retried every frame.
    error: Option<String>,
}

impl State {
    /// Settings › Appearance › Show tray icon (the saved choice).
    #[must_use]
    pub fn tray_icon(&self) -> bool {
        self.settings.tray_icon
    }

    /// Turn the tray icon on or off, persist the choice, and add or remove the
    /// icon now.
    pub fn set_tray_icon(&mut self, on: bool) {
        self.settings.tray_icon = on;
        self.tray.error = None;
        self.save_settings();
        self.sync_tray();
    }

    /// Install how this process makes a tray (the binary passes one built on
    /// [`rune::Tray`]); creates it now when the setting is on.
    pub fn install_tray(&mut self, factory: TrayFactory) {
        self.tray.factory = Some(factory);
        self.tray.error = None;
        self.sync_tray();
    }

    /// Whether a tray icon is live in this process.
    #[must_use]
    pub fn tray_shown(&self) -> bool {
        self.tray.live.is_some()
    }

    /// Why the tray icon could not be added, while the setting is on.
    #[must_use]
    pub fn tray_error(&self) -> Option<&str> {
        self.tray.error.as_deref()
    }

    /// Bring the tray in line with the setting and the sync status: create or
    /// drop it, and refresh its menu when the status row changed. Cheap; `poll`
    /// calls it every tick.
    pub fn sync_tray(&mut self) {
        if !self.settings.tray_icon || !tray_supported() {
            self.tray.live = None;
            self.tray.menu.clear();
            return;
        }
        let menu = tray_menu(self.sync_status(), self.sync_ready());
        if let Some(live) = self.tray.live.as_ref() {
            if self.tray.menu != menu {
                live.set_menu(menu.clone());
                self.tray.menu = menu;
            }
            return;
        }
        if self.tray.error.is_some() {
            return;
        }
        let Some(factory) = self.tray.factory.as_ref() else {
            return;
        };
        match factory(menu.clone()) {
            Ok(live) => {
                self.tray.live = Some(live);
                self.tray.menu = menu;
            }
            Err(error) => {
                self.log_event("tray-error", &error);
                self.tray.error = Some(error);
            }
        }
    }

    /// The tray's New note: close Settings and Help so the sidebar shows,
    /// then open the same new-note field as Cmd/Ctrl+N.
    pub fn tray_new_note(&mut self) {
        self.close_settings();
        self.help = false;
        self.new_note();
    }

    /// The tray's Sync now: the Sync tab's Sync Now when a provider is
    /// connected and idle, then refresh the menu at once (the window may be
    /// hidden, so the next poll can be a while).
    pub fn tray_sync_now(&mut self) {
        if self.sync_ready() {
            self.sync_now();
        }
        self.sync_tray();
    }

    /// Settings › Appearance › Close to tray (the saved choice).
    #[must_use]
    pub fn close_to_tray(&self) -> bool {
        self.settings.close_to_tray
    }

    /// Turn Close to tray on or off and persist the choice.
    pub fn set_close_to_tray(&mut self, on: bool) {
        self.settings.close_to_tray = on;
        self.save_settings();
    }

    /// Whether closing the window should hide it instead of quitting: Close
    /// to tray is on, the tray icon is on, and a tray was actually created.
    /// With the tray off, unsupported or failed, closing quits as before.
    #[must_use]
    pub fn closes_to_tray(&self) -> bool {
        self.settings.close_to_tray && self.settings.tray_icon && self.tray_shown()
    }

    /// The tray's Quit: the same flush as closing the window (the open note
    /// and settings), which Rune's quit skips.
    ///
    /// Board slice 4: unsaved drawing is saved on the close path too. When it
    /// cannot be saved (the file changed or vanished), its recovery copy is
    /// brought up to date instead, as a note's is. Returns `false` only when
    /// neither worked: the caller must not quit then (see
    /// `board_safety.rs`), and [`State::error`] says why.
    ///
    /// Kanban slice 7: unsaved kanban edits follow the board's rule
    /// (`kanban_sync.rs`): saved, else kept in their recovery copy; only
    /// when neither works, or the open card's text is refused, is the quit
    /// refused, with the reason on the Kanban page.
    pub fn flush_before_quit(&mut self) -> bool {
        self.save_now();
        let board = self.flush_board_before_quit();
        let kanban = self.flush_kanban_before_quit();
        self.save_settings();
        board && kanban
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_64_square_rgba() {
        assert_eq!(ICON_RGBA.len(), 64 * 64 * 4);
        assert!(
            ICON_RGBA
                .iter()
                .skip(3)
                .step_by(4)
                .any(|&alpha| alpha == 255),
            "the logo has opaque pixels"
        );
    }

    #[test]
    fn menu_ids_map_to_actions() {
        let menu = |id: &str| tray_action(&TrayEvent::Menu(id.to_owned()));
        assert_eq!(menu(TRAY_SHOW), Some(TrayAction::Show(None)));
        assert_eq!(menu(TRAY_NEW_NOTE), Some(TrayAction::NewNote));
        assert_eq!(menu(TRAY_SYNC_NOW), Some(TrayAction::SyncNow));
        assert_eq!(menu(TRAY_QUIT), Some(TrayAction::Quit));
        assert_eq!(menu(TRAY_SYNC_STATUS), None);
        assert_eq!(menu("other"), None);
        assert_eq!(
            tray_action(&TrayEvent::Activate {
                token: Some("t".into())
            }),
            Some(TrayAction::Show(Some("t".into())))
        );
    }

    #[test]
    fn the_menu_is_small_and_the_status_row_is_disabled() {
        let menu = tray_menu("Last successful sync 1a2b3c4d. Two notes changed.", true);
        assert_eq!(
            menu,
            vec![
                TrayItem::action(TRAY_SHOW, "Show Muninn"),
                TrayItem::action(TRAY_NEW_NOTE, "New note"),
                TrayItem::action(TRAY_SYNC_NOW, "Sync now"),
                TrayItem::Separator,
                TrayItem::action(TRAY_SYNC_STATUS, "Sync: Last successful sync 1a2b3c4d.")
                    .enabled(false),
                TrayItem::Separator,
                TrayItem::action(TRAY_QUIT, "Quit Muninn"),
            ]
        );
    }

    #[test]
    fn sync_now_is_greyed_when_not_ready() {
        let menu = tray_menu("Not connected.", false);
        assert!(menu.contains(&TrayItem::action(TRAY_SYNC_NOW, "Sync now").enabled(false)));
        assert!(
            menu.iter().all(|item| !matches!(item, TrayItem::Action { id, enabled: true, .. } if id == TRAY_SYNC_NOW))
        );
    }

    #[test]
    fn status_labels_keep_the_first_sentence_and_cap_length() {
        assert_eq!(tray_status_label(""), "Sync: Not connected.");
        assert_eq!(
            tray_status_label(
                "Not connected. Choose a provider and an explicit remote, then Connect."
            ),
            "Sync: Not connected."
        );
        let long = "x".repeat(200);
        let label = tray_status_label(&long);
        assert_eq!(label.chars().count(), "Sync: ".len() + STATUS_MAX);
        assert!(label.ends_with('…'));
    }
}
