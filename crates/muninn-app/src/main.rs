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
//!
//! `--check` and `--list-links` are headless vault inspectors; they return
//! before any Rune window or GPU context exists. `--trace` adds every autosave
//! tick to `$XDG_STATE_HOME/muninn/session.log`. `--theme omarchy|system|dark|light`
//! overrides the persisted theme for this session only (Omarchy is the
//! default: follow the shared Omarchy palette, else the system appearance).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use muninn_core::{Vault, parse_links};
use muninn_ui::{Mode, Scope, Settings, State, ThemeMode, on_key, poll, view};
use rune::AppSpec;
use rune_theme_opencode as theme;

/// `--trace` is parsed before Rune starts, but `init` is a plain function
/// pointer; this carries the flag between them.
static TRACE: AtomicBool = AtomicBool::new(false);

/// `--theme` is carried the same way: an unsaved session override that does
/// not touch the persisted theme preference.
static THEME: std::sync::Mutex<Option<ThemeMode>> = std::sync::Mutex::new(None);

fn main() {
    // Rune consumes `--help` for its own headless-tooling usage before
    // `extra_args`, so answer the app's help from the raw process arguments.
    if std::env::args_os()
        .skip(1)
        .any(|arg| arg == std::ffi::OsStr::new("--help") || arg == std::ffi::OsStr::new("-h"))
    {
        print_help(&mut std::io::stdout());
        return;
    }
    let args = rune::extra_args();
    match cli(args) {
        Cli::Check(path) => {
            let mut out = std::io::stdout().lock();
            std::process::exit(check_vault(&path, &mut out));
        }
        Cli::ListLinks(path) => {
            let mut out = std::io::stdout().lock();
            std::process::exit(list_links(&path, &mut out));
        }
        Cli::Error(message) => {
            eprintln!("muninn: {message}\n");
            print_help(&mut std::io::stderr());
            std::process::exit(2);
        }
        Cli::Run(session_theme) => {
            *THEME.lock().expect("theme lock") = session_theme;
        }
    }
    TRACE.store(has_flag(args, "--trace"), Ordering::Relaxed);
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
    let mut state = State::new(vault_path(rune::extra_args(), &settings));
    state.set_trace(TRACE.load(Ordering::Relaxed));
    if let Some(theme) = THEME.lock().expect("theme lock").take() {
        state.theme = theme;
    }
    state
}

/// The headless CLI surface, parsed from Rune's extra arguments before any
/// window or GPU context exists.
enum Cli {
    Run(Option<ThemeMode>),
    Check(PathBuf),
    ListLinks(PathBuf),
    Error(String),
}

fn cli(args: &[String]) -> Cli {
    let theme = match flag_value(args, "--theme") {
        Err(message) => return Cli::Error(message),
        Ok(None) => None,
        Ok(Some(path)) => match path.to_str() {
            Some("omarchy") => Some(ThemeMode::Omarchy),
            Some("system") => Some(ThemeMode::Follow),
            Some("dark") => Some(ThemeMode::Dark),
            Some("light") => Some(ThemeMode::Light),
            other => {
                return Cli::Error(format!(
                    "--theme needs one of omarchy|system|dark|light, got {:?}",
                    other.unwrap_or_default()
                ));
            }
        },
    };
    match flag_value(args, "--check") {
        Err(message) => return Cli::Error(message),
        Ok(Some(path)) => return Cli::Check(path),
        Ok(None) => {}
    }
    match flag_value(args, "--list-links") {
        Err(message) => return Cli::Error(message),
        Ok(Some(path)) => return Cli::ListLinks(path),
        Ok(None) => {}
    }
    Cli::Run(theme)
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

/// `--flag PATH` or `--flag=PATH`. `Ok(None)` means the flag is absent.
fn flag_value(args: &[String], flag: &str) -> Result<Option<PathBuf>, String> {
    let joined = format!("{flag}=");
    let mut i = 0;
    while i < args.len() {
        if args[i] == flag {
            return match args.get(i + 1) {
                Some(value) if !value.starts_with('-') && !value.is_empty() => {
                    Ok(Some(PathBuf::from(value)))
                }
                _ => Err(format!("{flag} needs a vault folder")),
            };
        }
        if let Some(value) = args[i].strip_prefix(&joined) {
            return if value.is_empty() {
                Err(format!("{flag} needs a vault folder"))
            } else {
                Ok(Some(PathBuf::from(value)))
            };
        }
        i += 1;
    }
    Ok(None)
}

fn print_help(out: &mut impl std::io::Write) {
    let _ = writeln!(
        out,
        "Muninn {} — a local-first Markdown vault\n\
         \n\
         Usage:\n  \
         muninn [--vault PATH] [--trace] [--theme omarchy|system|dark|light]\n  \
         muninn --check <vault>\n  \
         muninn --list-links <vault>\n  \
         muninn --help\n\
         \n\
         Options:\n  \
         --vault PATH     Open this vault (else $MUNINN_VAULT, the last vault, ~/Muninn)\n  \
         --theme MODE     Session theme override: omarchy (default), system, dark, light\n  \
         --trace          Log every autosave tick to the session log\n  \
         --check          Headless: exit 0 if clean, 1 for broken links/duplicates, 2 if unreadable\n  \
         --list-links     Headless: one `referrer -> target` line per wikilink, sorted",
        env!("CARGO_PKG_VERSION"),
    );
}

/// X-03: headless vault health check. The vault must already exist: `--check`
/// never creates a missing folder (unlike the app, which creates a first-run
/// vault). Returns the process exit code.
fn check_vault(root: &Path, out: &mut impl std::io::Write) -> i32 {
    if !root.is_dir() {
        let _ = writeln!(
            out,
            "error: {} is not a readable vault folder",
            root.display()
        );
        return 2;
    }
    let vault = match Vault::load(root) {
        Ok(vault) => vault,
        Err(error) => {
            let _ = writeln!(out, "error: could not read {}: {error}", root.display());
            return 2;
        }
    };
    let broken = vault.broken_links();
    let _ = writeln!(out, "notes: {}", vault.notes.len());
    let _ = writeln!(out, "broken links: {}", broken.len());
    for link in broken {
        for &referrer in &link.referrers {
            let _ = writeln!(out, "  {} -> {}", vault.notes[referrer].rel, link.target);
        }
    }

    let mut titles: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for note in &vault.notes {
        let entry = titles
            .entry(note.title.to_lowercase())
            .or_insert_with(|| (note.title.clone(), 0));
        entry.1 += 1;
    }
    let duplicates: Vec<(String, usize)> = titles
        .into_values()
        .filter(|(_, count)| *count > 1)
        .collect();
    let _ = writeln!(out, "duplicates: {}", duplicates.len());
    for (title, count) in &duplicates {
        let _ = writeln!(out, "  {title} ({count} notes)");
    }

    let _ = writeln!(out, "skipped: {}", vault.skipped_count());
    let deepest = vault
        .dirs
        .iter()
        .map(|dir| {
            if dir.is_empty() {
                0
            } else {
                dir.split('/').count()
            }
        })
        .max()
        .unwrap_or(0);
    if deepest >= 12 {
        let _ = writeln!(
            out,
            "warning: folders nest {deepest} levels deep; the scan stops below 12"
        );
    }

    if broken.is_empty() && duplicates.is_empty() {
        0
    } else {
        1
    }
}

/// X-03: every wikilink as `referrer -> target`, sorted. Resolved links use
/// the target note's vault-relative path; unresolved ones keep `missing:` so
/// the output is scriptable without a second lookup.
fn list_links(root: &Path, out: &mut impl std::io::Write) -> i32 {
    if !root.is_dir() {
        let _ = writeln!(
            out,
            "error: {} is not a readable vault folder",
            root.display()
        );
        return 2;
    }
    let vault = match Vault::load(root) {
        Ok(vault) => vault,
        Err(error) => {
            let _ = writeln!(out, "error: could not read {}: {error}", root.display());
            return 2;
        }
    };
    let mut lines = Vec::new();
    for note in &vault.notes {
        for link in parse_links(&note.source) {
            let target = if link.target.is_empty() {
                note.rel.clone()
            } else {
                vault.resolve(&link.target).map_or_else(
                    || format!("missing:{}", link.target),
                    |id| vault.notes[id].rel.clone(),
                )
            };
            lines.push(format!("{} -> {target}", note.rel));
        }
    }
    lines.sort();
    lines.dedup();
    for line in lines {
        let _ = writeln!(out, "{line}");
    }
    0
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
        ("light", |s| s.theme = muninn_ui::ThemeMode::Light),
        ("empty", |s| {
            s.selected = None;
            s.selected_path = None;
            s.editor.replace_text("");
            s.saved.clear();
            s.set_mode(Mode::Editor);
        }),
        ("settings-sync", |s| {
            s.sync_ui.preview = true;
            s.open_settings();
            s.sync_ui.github_owner.replace_text("octocat");
            s.sync_ui.github_repo.replace_text("notes-sync");
            s.sync_ui.github_branch.replace_text("muninn-sync");
            s.sync_ui.github_prefix.replace_text("vault");
            s.sync_ui.status =
                "Scope saved for this vault. No token is stored in settings.".to_owned();
        }),
        ("settings-sync-drive", |s| {
            s.sync_ui.preview = true;
            s.open_settings();
            s.set_sync_provider(1);
            s.sync_ui.drive_store.replace_text("Muninn");
            s.sync_ui.status =
                "Google Drive keeps an app-managed commit history in Muninn's private app data, not editable Markdown mirrors."
                    .to_owned();
        }),
        ("settings-sync-tailscale", |s| {
            s.sync_ui.preview = true;
            s.open_settings();
            s.set_sync_provider(2);
            s.set_tailnet_role(1);
            s.sync_ui.tailnet_host.replace_text("minis.tail1234.ts.net");
            s.sync_ui.status =
                "Join a host. Enter the host's full MagicDNS name and the shared key shown on the host."
                    .to_owned();
        }),
        ("settings-sync-conflict", |s| {
            s.theme = muninn_ui::ThemeMode::Light;
            s.sync_ui.preview = true;
            s.open_settings();
            s.sync_ui.status =
                "2 conflicts. Nothing was overwritten. Resolve them, then Sync Now again."
                    .to_owned();
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
            [
                "notes",
                "preview",
                "graph",
                "local",
                "light",
                "empty",
                "settings-sync",
                "settings-sync-drive",
                "settings-sync-tailscale",
                "settings-sync-conflict"
            ]
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

#[cfg(test)]
mod cli_tests;
