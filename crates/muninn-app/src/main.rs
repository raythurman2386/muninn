//! Muninn: a local-first Markdown vault with a wikilink graph, built on Rune.
//!
//! The binary is deliberately small. Vault logic lives in `muninn-core`, layout math in
//! `muninn-graph`, and state plus views in `muninn-ui`.
//!
//! ```sh
//! cargo run -p muninn-app -- --vault vault
//! cargo run -p muninn-app -- --vault ~/notes
//! ```
//!
//! Without `--vault` or `$MUNINN_VAULT`, Muninn reopens the last vault recorded
//! in `$XDG_CONFIG_HOME/muninn/settings.json`, then falls back to `~/Muninn`.
//! A missing last vault folder is created empty on first run.

use std::path::PathBuf;

use muninn_ui::{Mode, Scope, Settings, State, on_key, poll, view};
use rune::AppSpec;
use rune_theme_opencode as theme;

fn main() {
    // Match the shipped `muninn.desktop` (`StartupWMClass` + hicolor icon) so
    // the OS can associate this window with the logo.
    rune::set_app_id("muninn");
    let spec = AppSpec::<State> {
        fonts: theme::FONTS,
        families: theme::FAMILIES,
        min_size: Some((840.0, 560.0)),
        scenes: scenes(),
        poll: Some(poll),
        on_key: Some(on_key),
        ..AppSpec::new("Muninn", (1180.0, 780.0), init, view)
    };
    rune::App::new(spec)
        .on_close(|s, _tx| {
            // Best-effort: flush the open note on the normal close path. Autosave is
            // the crash net for SIGTERM/SIGKILL, which never run this hook; Rune's
            // `AppHandle::quit` / `Tx::quit` also bypass it.
            //
            // With a conflict pending (SAF-02), `save_now` refuses to overwrite the
            // changed file and returns `false`. That is intentional: closing must not
            // force-overwrite work the user has not resolved.
            s.save_now();
            // LIF-01: settings are also written on each toggle/open, but the close
            // hook captures the final note/mode if the window is closed right after
            // a change that did not reach a mutator.
            s.save_settings();
        })
        .run();
}

fn init() -> State {
    let settings = Settings::load();
    State::new(vault_path(rune::extra_args(), &settings))
}

/// `--vault` wins over `$MUNINN_VAULT`, which wins over the last vault in
/// settings, which falls back to `~/Muninn`.
fn vault_path(args: &[String], settings: &Settings) -> PathBuf {
    if let Some(path) = vault_arg(args) {
        return path;
    }
    if let Some(path) = std::env::var_os("MUNINN_VAULT")
        && !path.is_empty()
    {
        return PathBuf::from(path);
    }
    settings.last_vault.clone().unwrap_or_else(default_vault)
}

fn vault_arg(args: &[String]) -> Option<PathBuf> {
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--vault"
            && let Some(path) = args.get(i + 1)
        {
            return Some(PathBuf::from(path));
        }
        if let Some(path) = arg.strip_prefix("--vault=") {
            return Some(PathBuf::from(path));
        }
        i += 1;
    }
    None
}

fn default_vault() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(
            || PathBuf::from("vault"),
            |home| PathBuf::from(home).join("Muninn"),
        )
}

type Scene = (&'static str, fn(&mut State));

fn scenes() -> &'static [Scene] {
    &[
        ("notes", |s| s.set_mode(Mode::Editor)),
        ("preview", |s| s.set_mode(Mode::Preview)),
        ("graph", |s| {
            s.scope = Scope::Global;
            s.set_mode(Mode::Graph);
            s.graph.reheat();
        }),
        ("local", |s| {
            if let Some(id) = s.vault.resolve("Knowledge Graph") {
                s.load_note(id, false);
            }
            s.open_local_graph();
        }),
        ("light", |s| s.dark = Some(false)),
        ("empty", |s| {
            s.selected = None;
            s.selected_path = None;
            s.editor.replace_text("");
            s.saved.clear();
            s.set_mode(Mode::Editor);
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_flag_accepts_both_spellings() {
        let separate = vec!["--vault".to_owned(), "/tmp/notes".to_owned()];
        assert_eq!(vault_arg(&separate), Some(PathBuf::from("/tmp/notes")));
        let joined = vec!["--vault=/tmp/notes".to_owned()];
        assert_eq!(vault_arg(&joined), Some(PathBuf::from("/tmp/notes")));
        assert_eq!(vault_arg(&["--other".to_owned()]), None);
    }

    #[test]
    fn the_last_public_scene_names_remain_stable() {
        let names: Vec<&str> = scenes().iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            ["notes", "preview", "graph", "local", "light", "empty"]
        );
    }

    /// The `on_close` hook only calls `State::save_now`, so this proves the close path
    /// flushes a dirty buffer without needing a window or a Rune `Tx`.
    #[test]
    fn close_flush_writes_a_dirty_buffer() {
        use std::fs;
        use std::sync::atomic::{AtomicUsize, Ordering};

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "muninn-app-close-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).expect("temp vault");
        let note = dir.join("Note.md");
        fs::write(&note, "original").expect("fixture");

        let mut state = State::with_state_home(dir.clone(), dir.with_extension("state"));
        assert_eq!(state.vault.notes.len(), 1);
        state.editor.replace_text("edited on close");
        assert!(state.dirty());

        assert!(state.save_now(), "dirty note should be written");
        assert!(!state.dirty());
        assert_eq!(fs::read_to_string(&note).expect("read"), "edited on close");

        drop(state);
        drop(fs::remove_dir_all(&dir));
    }
}
