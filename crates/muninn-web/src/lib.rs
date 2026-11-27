//! Muninn in the browser: the OPFS-backed vault demo (#240, slices 5–6b).
//!
//! The crate is deliberately thin. The whole application — state, views, the
//! editor, graph, kanban and board renderers — is `muninn-ui` compiled with
//! `default-features = false` (the browser configuration from slice 4). This
//! crate supplies only what a wasm module adds on top:
//!
//! - the committed `vault/` sample as embedded bytes ([`SAMPLE_SOURCES`]), used
//!   to seed OPFS on the first run,
//! - [`store::OpfsStore`]: the synchronous [`muninn_core::VaultStore`] over an
//!   in-memory mirror, with a dirty journal the async side drains to OPFS,
//! - the OPFS boot in `opfs`: mount, snapshot-load, Web Lock, `persist()`,
//!   and the first-run seed,
//! - the `#[wasm_bindgen(start)]` entry that boots asynchronously, then runs
//!   the same `AppSpec` + Rune call the native binary runs.
//!
//! Edits, renames, moves and deletes now land in the store and survive a
//! reload. Between flushes OPFS lags the mirror: that window is the crash
//! window, and slice 6c's recovery store is where it gets a safety net.
//!
//! Build the static folder with `scripts/web-build.sh muninn-web`.

#![forbid(unsafe_code)]

mod sample;
mod store;

pub use sample::{SAMPLE_ROOT, SAMPLE_SOURCES};
pub use store::{FlushPlan, OpfsStore};

#[cfg(target_arch = "wasm32")]
mod opfs;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use muninn_ui::{State, poll, view};
    use rune::AppSpec;
    use rune_theme_opencode as theme;

    use crate::{SAMPLE_ROOT, SAMPLE_SOURCES};

    thread_local! {
        /// The state the async boot finished preparing. `init` takes it when
        /// Rune asks for the app's first frame; wasm is single-threaded, so a
        /// thread-local is the right home for a non-`Send` UI state.
        static READY: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    /// Install the embedded sample into a state that starts with no folder.
    ///
    /// This is the honest fallback when OPFS cannot be mounted at all: the
    /// demo still renders the sample, read-only, with the error in the status
    /// bar. In-memory settings (`State::with_state_home` persists nothing)
    /// keep the browser from touching a native config path.
    fn fallback_state() -> State {
        let mut state = State::with_state_home(
            std::path::PathBuf::from(SAMPLE_ROOT),
            std::path::PathBuf::from("/Muninn sample/.state"),
        );
        state.install_sources(SAMPLE_SOURCES);
        state.apply_saved_language();
        state
    }

    /// Rune's `init`: hand over the state the async boot built, or fall back
    /// to the sample if boot could not store one.
    fn init() -> State {
        READY
            .with(|slot| slot.borrow_mut().take())
            .unwrap_or_else(fallback_state)
    }

    /// The browser entry point: boot OPFS, then start the same app the native
    /// binary runs.
    ///
    /// `rune::run` wants a synchronous `init`, while opening OPFS, taking the
    /// Web Lock and loading the vault are all Promise-based. The boot runs on a
    /// `spawn_local` task, builds the `State` against the store, parks it in
    /// [`READY`], and only then calls `rune::run`; its `init` takes the parked
    /// state. No blocking, no worker and no COOP/CORP headers.
    ///
    /// No `canvas_id`: Rune appends its own canvas, covers the viewport and
    /// locks body scrolling, which is what a whole-page demo wants. Fonts and
    /// families are the OpenCode theme's, so the web rendering matches the
    /// desktop app rather than Rune's stock look.
    #[wasm_bindgen::prelude::wasm_bindgen(start)]
    pub fn start() {
        wasm_bindgen_futures::spawn_local(async {
            let opened = crate::opfs::open().await;
            let mut state = State::with_state_home(
                std::path::PathBuf::from(SAMPLE_ROOT),
                std::path::PathBuf::from("/Muninn sample/.state"),
            );
            let mut installed = false;
            let mut install_error = None;
            if let Some(store) = opened.store {
                match state.install_store(store) {
                    Ok(()) => installed = true,
                    Err(error) => {
                        // The store was mounted but could not become the
                        // vault (a corrupt root, a refused store). Drop the
                        // flush session and fall back to the sample.
                        crate::opfs::deactivate();
                        install_error = Some(error);
                    }
                }
            }
            if !installed {
                state.install_sources(SAMPLE_SOURCES);
            }
            // The first-run welcome card belongs to a folder that did not
            // exist; a browser vault is not that, even when it is empty.
            state.welcome = false;
            // Land on Welcome, the sample vault's own tour, rather than the
            // alphabetically first note. Pressing a mode key or opening
            // another note is the ordinary app from here.
            if let Some(id) = state.vault.resolve("Welcome") {
                state.load_note(id, false);
            }
            state.apply_saved_language();
            // Status messages go last: opening a note clears `state.error`, so
            // setting the read-only banner before the initial load would make
            // it vanish on the first frame.
            if let Some(error) = install_error {
                state.error = Some(error);
            }
            if let Some(notice) = opened.notice {
                // The first-run/read-only sample notice already explains the
                // state; otherwise the lock posture gets the status line.
                state.notice = Some(notice);
            } else if opened.read_only && installed {
                state.notice = Some(crate::opfs::READ_ONLY_MESSAGE.to_owned());
            }
            if let Some(error) = opened.error {
                state.error = Some(error);
            }
            READY.with(|slot| *slot.borrow_mut() = Some(state));

            // The generated page keeps its loading line until boot has the
            // vault in hand; removing it here means a slow OPFS walk or a
            // first-run seed still says what it is doing, while a synchronous
            // `init` would have cleared it before the first await.
            if let Some(window) = web_sys::window()
                && let Some(status) = window
                    .document()
                    .and_then(|document| document.get_element_by_id("status"))
            {
                status.remove();
            }

            let spec = AppSpec::<State> {
                fonts: theme::FONTS,
                families: theme::FAMILIES,
                poll: Some(poll_and_flush),
                on_key: Some(muninn_ui::on_key),
                ..AppSpec::new("Muninn", (1180.0, 780.0), init, view)
            };
            rune::run(spec);
        });
    }

    /// The app's poll plus the vault's write-behind drain. Keeping the wrapper
    /// here, not in `muninn-ui`, is what lets the shared UI stay storage
    /// agnostic: the browser build is the only host with a flush to schedule.
    fn poll_and_flush(state: &mut State, t: f64, tx: &mut rune::Tx) -> bool {
        crate::opfs::schedule_flush();
        if let Some(error) = crate::opfs::take_flush_error()
            && state.error.is_none()
        {
            state.error = Some(error);
        }
        poll(state, t, tx)
    }
}
