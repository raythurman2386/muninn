//! Muninn in the browser: the read-only sample-vault demo (#240, slice 5).
//!
//! The crate is deliberately thin. The whole application — state, views, the
//! editor, graph, kanban and board renderers — is `muninn-ui` compiled with
//! `default-features = false` (the browser configuration from slice 4). This
//! crate supplies only what a wasm module adds on top:
//!
//! - the committed `vault/` sample as embedded bytes ([`SAMPLE_SOURCES`]),
//! - a `State` built in memory through [`muninn_core::Vault::from_sources`],
//!   the same seam a browser store will use in slice 6, and
//! - the `#[wasm_bindgen(start)]` entry that runs the same `AppSpec` + Rune
//!   call the native binary runs.
//!
//! Nothing here writes. Saves and filesystem actions reach the ordinary guarded
//! paths in `muninn-ui`, fail against the virtual root, and surface their usual
//! errors; the page describes the demo as read-only. OPFS, Web Locks and the
//! write sinks are slice 6 and are not part of this crate.
//!
//! Build the static folder with `scripts/web-build.sh muninn-web`.

#![forbid(unsafe_code)]

mod sample;

pub use sample::{SAMPLE_ROOT, SAMPLE_SOURCES};

#[cfg(target_arch = "wasm32")]
mod web {
    use std::path::PathBuf;

    use muninn_ui::{State, on_key, poll, view};
    use rune::AppSpec;
    use rune_theme_opencode as theme;

    use crate::{SAMPLE_ROOT, SAMPLE_SOURCES};

    /// Install the embedded sample vault into a state that starts with no
    /// folder: in-memory settings (`State::with_state_home` persists nothing),
    /// then the source-built vault, then the sample's own introduction note.
    ///
    /// `install_sources` runs the same tail as a finished startup scan: it
    /// builds the graph, clears the placeholder's startup error, and opens the
    /// first note. A fresh state has no open board, so it cannot refuse; if it
    /// ever did, the placeholder (empty, with an honest error) stays.
    fn init() -> State {
        let root = PathBuf::from(SAMPLE_ROOT);
        let mut state = State::with_state_home(root, PathBuf::from("/Muninn sample/.state"));
        if state.install_sources(SAMPLE_SOURCES) {
            // Land on Welcome, the sample vault's own tour, rather than the
            // alphabetically first Backlinks. Pressing a mode key or opening
            // another note is the ordinary app from here.
            if let Some(id) = state.vault.resolve("Welcome") {
                state.load_note(id, false);
            }
        }
        state.apply_saved_language();
        state
    }

    /// The browser entry point: the same app the native binary runs.
    ///
    /// No `canvas_id`: Rune appends its own canvas, covers the viewport and
    /// locks body scrolling, which is what a whole-page demo wants. Fonts and
    /// families are the OpenCode theme's, so the web rendering matches the
    /// desktop app rather than Rune's stock look.
    #[wasm_bindgen::prelude::wasm_bindgen(start)]
    pub fn start() {
        let spec = AppSpec::<State> {
            fonts: theme::FONTS,
            families: theme::FAMILIES,
            poll: Some(poll),
            on_key: Some(on_key),
            ..AppSpec::new("Muninn", (1180.0, 780.0), init, view)
        };
        rune::run(spec);
    }
}
