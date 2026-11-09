//! Settings › Appearance › the global show-Muninn shortcut.
//!
//! Rune registers system-wide shortcuts at the pinned revision
//! (`AppHandle::register_shortcut`: Carbon on macOS, `RegisterHotKey` on
//! Windows, X11 grabs, the xdg-desktop-portal GlobalShortcuts on Wayland).
//! Muninn binds one fixed chord, [`SHOW_SHORTCUT`], that shows and focuses
//! the window, including one closed to the tray.
//!
//! Registration can wait on a Wayland consent dialog, so the binary's
//! [`ShortcutPort`] binds on a worker thread and reports back through
//! [`State::shortcut_result`]. Tests use a fake port; headless runs install
//! none and never register.

use crate::State;

/// The chord, in Rune's accelerator syntax.
pub const SHOW_SHORTCUT: &str = "ctrl+alt+m";

/// The chord as the switch label shows it.
#[must_use]
pub const fn show_shortcut_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌃⌥M"
    } else {
        "Ctrl+Alt+M"
    }
}

/// Binds and releases the show-Muninn shortcut.
pub trait ShortcutPort {
    /// Start registering [`SHOW_SHORTCUT`]. The outcome arrives later through
    /// [`State::shortcut_result`].
    fn bind(&self);
    /// Release the registration made by the last successful `bind`.
    fn unbind(&self);
}

/// Where the registration stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ShortcutPhase {
    /// Not registered.
    #[default]
    Off,
    /// A `bind` is in flight.
    Pending,
    /// Registered.
    Bound,
    /// The last `bind` failed; not retried until the switch changes.
    Failed(String),
}

/// The shortcut as [`State`] holds it.
#[derive(Default)]
pub(crate) struct ShortcutLink {
    port: Option<Box<dyn ShortcutPort>>,
    phase: ShortcutPhase,
}

impl State {
    /// Settings › Appearance › show-Muninn shortcut (the saved choice).
    #[must_use]
    pub fn show_shortcut(&self) -> bool {
        self.settings.show_shortcut
    }

    /// Turn the shortcut on or off, persist the choice, and register or
    /// release it. Changing the switch is also the retry after a failure.
    pub fn set_show_shortcut(&mut self, on: bool) {
        self.settings.show_shortcut = on;
        if matches!(self.shortcut.phase, ShortcutPhase::Failed(_)) {
            self.shortcut.phase = ShortcutPhase::Off;
        }
        self.save_settings();
        self.sync_shortcut();
    }

    /// Install how this process registers shortcuts; binds now when the
    /// setting is on.
    pub fn install_shortcut(&mut self, port: Box<dyn ShortcutPort>) {
        self.shortcut.port = Some(port);
        self.shortcut.phase = ShortcutPhase::Off;
        self.sync_shortcut();
    }

    /// Where the registration stands.
    #[must_use]
    pub fn shortcut_phase(&self) -> &ShortcutPhase {
        &self.shortcut.phase
    }

    /// A `bind` finished. A success that arrives after the switch was turned
    /// off is released at once.
    pub fn shortcut_result(&mut self, result: Result<(), String>) {
        self.shortcut.phase = match result {
            Ok(()) => {
                self.log_event("shortcut", SHOW_SHORTCUT);
                ShortcutPhase::Bound
            }
            Err(error) if self.settings.show_shortcut => {
                self.log_event("shortcut-error", &error);
                ShortcutPhase::Failed(error)
            }
            Err(_) => ShortcutPhase::Off,
        };
        self.sync_shortcut();
    }

    /// Bring the registration in line with the setting. One `bind` at a time.
    fn sync_shortcut(&mut self) {
        let Some(port) = self.shortcut.port.as_ref() else {
            return;
        };
        match (self.settings.show_shortcut, &self.shortcut.phase) {
            (true, ShortcutPhase::Off) => {
                port.bind();
                self.shortcut.phase = ShortcutPhase::Pending;
            }
            (false, ShortcutPhase::Bound) => {
                port.unbind();
                self.shortcut.phase = ShortcutPhase::Off;
            }
            _ => {}
        }
    }
}
