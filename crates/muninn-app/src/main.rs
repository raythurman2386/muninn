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
//! Without `--vault`, Muninn opens `$MUNINN_VAULT` or `~/Muninn`.

use std::path::PathBuf;

use muninn_ui::{Mode, Scope, State, on_key, poll, view};
use rune::AppSpec;
use rune_theme_opencode as theme;

fn main() {
    rune::run(AppSpec::<State> {
        fonts: theme::FONTS,
        families: theme::FAMILIES,
        min_size: Some((840.0, 560.0)),
        scenes: scenes(),
        poll: Some(poll),
        on_key: Some(on_key),
        ..AppSpec::new("Muninn", (1180.0, 780.0), init, view)
    });
}

fn init() -> State {
    State::new(vault_path(rune::extra_args()))
}

fn vault_path(args: &[String]) -> PathBuf {
    vault_arg(args).unwrap_or_else(default_vault)
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
    if let Some(path) = std::env::var_os("MUNINN_VAULT") {
        return PathBuf::from(path);
    }
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
}
