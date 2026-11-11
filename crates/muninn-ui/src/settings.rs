//! Small, human-readable preferences persisted outside the vault.
//!
//! Muninn keeps notes as files. The only cross-session UI state besides the
//! crash-recovery copies and the sync journal (also outside the vault) is this
//! JSON file:
//!
//! ```text
//! $XDG_CONFIG_HOME/muninn/settings.json      (fallback: ~/.config/muninn/)
//! ```
//!
//! It stores the theme choice, collapsed sidebar folders, the last vault, the
//! last note and mode/scope, the recent-vault MRU list, the autosave delay,
//! the default body inserted into new notes and the Reduce motion choice.
//! Decoding is intentionally forgiving: a corrupt or partially-written file
//! falls back to defaults instead of failing startup. Writes use the same temp-file-then-rename pattern as note saves, so
//! a crash leaves either the old complete file or the new one.

use std::env;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use crate::{BoardSession, BoardView};

use crate::{Mode, Scope};

/// Default idle window before autosave, in seconds. The settings modal can
/// tune it between 0.05 and 10 seconds.
pub const DEFAULT_AUTOSAVE_DELAY: f64 = 0.55;

/// Shortest autosave idle window, in seconds.
pub const AUTOSAVE_MIN_DELAY: f64 = 0.05;

/// Longest autosave idle window, in seconds.
pub const AUTOSAVE_MAX_DELAY: f64 = 10.0;

/// How many recently opened vaults are remembered (MRU, newest first).
pub const RECENT_VAULTS_MAX: usize = 5;

/// Longest default note template kept, in characters. The settings modal and
/// the loader both clamp to this so a runaway paste cannot bloat
/// `settings.json` or every note created from it.
pub(crate) const NOTE_TEMPLATE_MAX_CHARS: usize = 4096;

/// Truncate `template` to [`NOTE_TEMPLATE_MAX_CHARS`] without splitting a
/// multi-byte character.
#[must_use]
pub(crate) fn clamp_note_template(template: &str) -> String {
    if template.len() <= NOTE_TEMPLATE_MAX_CHARS {
        return template.to_owned();
    }
    template.chars().take(NOTE_TEMPLATE_MAX_CHARS).collect()
}

/// Distinguishes temp files written by the same process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Which palette the window follows. `Omarchy` (the default) tracks the
/// shared Omarchy system theme (`~/.local/state/omarchy/current/theme/colors.toml`)
/// and falls back to the system appearance when that file is absent, so
/// non-Omarchy systems see no change. `Follow` is the old "follow the system"
/// behaviour. `Dark`/`Light` pin the appearance.
///
/// Serialized as `"omarchy"` / `"system"` / `"dark"` / `"light"`; a bare
/// boolean (`"true"`/`"false"`) still parses as dark/light for old files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    Omarchy,
    Follow,
    Dark,
    Light,
}

impl ThemeMode {
    /// Every choice in the order the settings segmented control shows them.
    pub const ALL: [ThemeMode; 4] = [
        ThemeMode::Omarchy,
        ThemeMode::Follow,
        ThemeMode::Dark,
        ThemeMode::Light,
    ];

    #[must_use]
    pub fn id(self) -> u8 {
        match self {
            ThemeMode::Omarchy => 0,
            ThemeMode::Follow => 1,
            ThemeMode::Dark => 2,
            ThemeMode::Light => 3,
        }
    }

    /// The next mode for the theme button's cycle: System → Dark → Light →
    /// System. Omarchy is the system default, not a switcher stop; from
    /// Omarchy the cycle diverges to System.
    #[must_use]
    pub fn next(self) -> ThemeMode {
        match self {
            ThemeMode::Omarchy => ThemeMode::Follow,
            ThemeMode::Follow => ThemeMode::Dark,
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Follow,
        }
    }

    #[must_use]
    pub fn as_key(self) -> &'static str {
        match self {
            ThemeMode::Omarchy => "omarchy",
            ThemeMode::Follow => "system",
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Omarchy => "Omarchy",
            ThemeMode::Follow => "System",
            ThemeMode::Dark => "Dark",
            ThemeMode::Light => "Light",
        }
    }
}

/// The stored spelling for a persisted `theme` value. A bare boolean is
/// accepted for forward/backward compatibility.
fn parse_theme_mode(value: Option<&str>) -> ThemeMode {
    match value {
        Some("dark") => ThemeMode::Dark,
        Some("light") => ThemeMode::Light,
        Some("omarchy") => ThemeMode::Omarchy,
        Some("system") => ThemeMode::Follow,
        Some(other) => match other.parse::<bool>() {
            Ok(true) => ThemeMode::Dark,
            Ok(false) => ThemeMode::Light,
            Err(_) => ThemeMode::default(),
        },
        None => ThemeMode::default(),
    }
}

/// Non-secret sync preferences. Access tokens never belong in this struct.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncPrefs {
    /// Set when the stored object cannot be trusted. Sync stays off.
    pub disabled_reason: Option<String>,
    pub github_client_id: String,
    pub google_client_id: String,
    pub vaults: Vec<VaultSyncBinding>,
}

/// One provider binding for one canonical vault. The credential store key is
/// `provider + binding_id`; the secret itself is not here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VaultSyncBinding {
    pub identity: String,
    pub provider: String,
    pub binding_id: String,
    pub account: String,
    pub github_owner: String,
    pub github_repo: String,
    pub github_branch: String,
    pub github_prefix: String,
    /// Store name inside the app's Drive data folder. Devices that share a
    /// vault use the same name.
    pub drive_store: String,
    /// `host` or `peer`. Empty for other providers.
    pub tailnet_role: String,
    /// Peer: the host's full MagicDNS name, for example `minis.tail1234.ts.net`.
    pub tailnet_host: String,
    /// Host: loopback port that `tailscale serve` proxies to.
    pub tailnet_port: u16,
    /// Host: optional comma-separated `Tailscale-User-Login` allow-list.
    pub tailnet_allowed: String,
    pub last_success_unix_ms: Option<u64>,
    pub last_head: String,
}

impl SyncPrefs {
    fn to_json(&self) -> Value {
        json!({
            "version": 1,
            "disabled_reason": self.disabled_reason,
            "github_client_id": self.github_client_id,
            "google_client_id": self.google_client_id,
            "vaults": self.vaults.iter().map(VaultSyncBinding::to_json).collect::<Vec<_>>(),
        })
    }

    fn from_value(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        if !value.is_object() {
            return Self::invalid("Sync settings were not an object, so sync is off.");
        }
        if value.get("version").and_then(Value::as_u64) != Some(1) {
            return Self::invalid("Sync settings version is not supported, so sync is off.");
        }
        let mut prefs = Self {
            disabled_reason: value
                .get("disabled_reason")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_owned),
            github_client_id: value
                .get("github_client_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            google_client_id: value
                .get("google_client_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            vaults: Vec::new(),
        };
        let Some(vaults) = value.get("vaults").and_then(Value::as_array) else {
            return Self::invalid("Sync vault list was missing, so sync is off.");
        };
        for item in vaults {
            match VaultSyncBinding::from_value(item) {
                Some(binding) => prefs.vaults.push(binding),
                None => {
                    prefs.disabled_reason = Some(
                        "A sync binding was invalid and was not loaded. Sync is off until it is replaced."
                            .to_owned(),
                    );
                    prefs.vaults.clear();
                    return prefs;
                }
            }
        }
        prefs
    }

    fn invalid(reason: &str) -> Self {
        Self {
            disabled_reason: Some(reason.to_owned()),
            ..Self::default()
        }
    }
}

impl VaultSyncBinding {
    fn to_json(&self) -> Value {
        json!({
            "identity": self.identity,
            "provider": self.provider,
            "binding_id": self.binding_id,
            "account": self.account,
            "github_owner": self.github_owner,
            "github_repo": self.github_repo,
            "github_branch": self.github_branch,
            "github_prefix": self.github_prefix,
            "drive_store": self.drive_store,
            "tailnet_role": self.tailnet_role,
            "tailnet_host": self.tailnet_host,
            "tailnet_port": self.tailnet_port,
            "tailnet_allowed": self.tailnet_allowed,
            "last_success_unix_ms": self.last_success_unix_ms,
            "last_head": self.last_head,
        })
    }

    fn from_value(value: &Value) -> Option<Self> {
        let provider = value.get("provider").and_then(Value::as_str)?;
        if provider != "github" && provider != "google_drive" && provider != "tailscale" {
            return None;
        }
        let text = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        let tailnet_role = text("tailnet_role");
        if provider == "tailscale" && tailnet_role != "host" && tailnet_role != "peer" {
            return None;
        }
        let binding_id = value.get("binding_id").and_then(Value::as_str)?;
        if binding_id.len() < 8 {
            return None;
        }
        Some(Self {
            identity: value.get("identity").and_then(Value::as_str)?.to_owned(),
            provider: provider.to_owned(),
            binding_id: binding_id.to_owned(),
            account: value
                .get("account")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            github_owner: value
                .get("github_owner")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            github_repo: value
                .get("github_repo")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            github_branch: value
                .get("github_branch")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            github_prefix: value
                .get("github_prefix")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            drive_store: value
                .get("drive_store")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            tailnet_role,
            tailnet_host: text("tailnet_host"),
            tailnet_port: value
                .get("tailnet_port")
                .and_then(Value::as_u64)
                .and_then(|port| u16::try_from(port).ok())
                .unwrap_or(0),
            tailnet_allowed: text("tailnet_allowed"),
            last_success_unix_ms: value.get("last_success_unix_ms").and_then(Value::as_u64),
            last_head: value
                .get("last_head")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        })
    }
}

/// The Reduce motion preference (Settings › Graph).
///
/// Serialized as `"system"` / `"on"` / `"off"`. A missing or unknown value,
/// including every file written before this setting existed, loads as
/// [`ReduceMotion::System`].
///
/// [`ReduceMotion::resolve`] is the one rule for whether motion is reduced:
/// `MUNINN_REDUCED_MOTION` forces it on (for tests, CI and scripted demos)
/// whatever is chosen here; otherwise `On` and `Off` pin it and `System`
/// follows the operating system's reduce-motion preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReduceMotion {
    #[default]
    System,
    On,
    Off,
}

impl ReduceMotion {
    /// Every choice in the order the settings segmented control shows them.
    pub const ALL: [ReduceMotion; 3] = [ReduceMotion::System, ReduceMotion::On, ReduceMotion::Off];

    #[must_use]
    pub fn as_key(self) -> &'static str {
        match self {
            ReduceMotion::System => "system",
            ReduceMotion::On => "on",
            ReduceMotion::Off => "off",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ReduceMotion::System => "System",
            ReduceMotion::On => "On",
            ReduceMotion::Off => "Off",
        }
    }

    /// Position in [`ReduceMotion::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0)
    }

    #[must_use]
    pub fn from_key(value: Option<&str>) -> ReduceMotion {
        match value {
            Some("on") => ReduceMotion::On,
            Some("off") => ReduceMotion::Off,
            _ => ReduceMotion::System,
        }
    }

    /// Whether motion is reduced, given `env_forced` (`MUNINN_REDUCED_MOTION`
    /// is set) and `system` (the OS preference). The environment flag can
    /// only force reduced motion on; it never turns motion back on.
    #[must_use]
    pub fn resolve(self, env_forced: bool, system: bool) -> bool {
        env_forced
            || match self {
                ReduceMotion::System => system,
                ReduceMotion::On => true,
                ReduceMotion::Off => false,
            }
    }
}

/// Preferences shared by every vault.
///
/// `theme` picks the palette source (see [`ThemeMode`]). `collapsed` and the
/// mode/scope/last-note fields are written whenever the user changes them, so
/// a restart restores the session without a separate "save workspace" step.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Which palette the window follows; restored by [`State`](crate::State).
    pub theme: ThemeMode,
    /// Collapsed sidebar folders (vault-relative, `""` for the root).
    pub collapsed: Vec<String>,
    /// Most recently opened vault, if any.
    pub last_vault: Option<PathBuf>,
    /// Vault-relative path of the note that was open when the session ended.
    pub selected_rel: Option<String>,
    pub mode: Mode,
    pub scope: Scope,
    /// Idle seconds before autosave. Defaults to [`DEFAULT_AUTOSAVE_DELAY`].
    pub autosave_delay: f64,
    /// Recent vaults, most recently opened first, capped at
    /// [`RECENT_VAULTS_MAX`].
    pub recent_vaults: Vec<PathBuf>,
    /// Markdown inserted at the top of each new note, exactly as typed (a
    /// missing trailing newline is added when the note is created). Empty —
    /// the default — keeps the original heading-only note body. Clamped to
    /// 4096 characters.
    pub note_template: String,
    /// Per-vault sync bindings. Tokens are not stored here.
    pub sync: SyncPrefs,
    /// Settings › Graph › Reduce motion. Defaults to [`ReduceMotion::System`].
    pub reduce_motion: ReduceMotion,
    /// Settings › Appearance › Show tray icon. Off by default; files saved
    /// before the setting existed load as off.
    pub tray_icon: bool,
    /// Settings › Appearance › Close to tray. Off by default. Closing the
    /// window hides it only while this is on and a tray icon was created.
    pub close_to_tray: bool,
    /// Settings › Appearance › Start Muninn at login. Off by default.
    pub auto_start: bool,
    /// Settings › Appearance › the global show-Muninn shortcut. Off by default.
    pub show_shortcut: bool,
    /// Board slice 3: the board open when the session ended and its view
    /// (`{"rel": …, "eye": [x, y], "zoom": z}`). Files saved before boards
    /// load as `None`.
    pub board: Option<BoardSession>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Omarchy,
            collapsed: Vec::new(),
            last_vault: None,
            selected_rel: None,
            mode: Mode::Editor,
            scope: Scope::Global,
            autosave_delay: DEFAULT_AUTOSAVE_DELAY,
            recent_vaults: Vec::new(),
            note_template: String::new(),
            sync: SyncPrefs::default(),
            reduce_motion: ReduceMotion::System,
            tray_icon: false,
            close_to_tray: false,
            auto_start: false,
            show_shortcut: false,
            board: None,
        }
    }
}

impl Settings {
    /// Load the settings for the user's real config directory.
    #[must_use]
    pub fn load() -> Settings {
        Self::load_from(&config_home())
    }

    /// Load from an explicit config root (the directory that contains
    /// `muninn/settings.json`). Tests use this to stay out of the user's files.
    ///
    /// A missing, unreadable or corrupt file is not an error: defaults are the
    /// safe startup state.
    #[must_use]
    pub fn load_from(config_home: &Path) -> Settings {
        let Ok(text) = fs::read_to_string(settings_path(config_home)) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            return Self::default();
        };
        Self::from_value(&value)
    }

    /// Persist below an explicit config root.
    pub fn save_to(&self, config_home: &Path) -> io::Result<()> {
        let path = settings_path(config_home);
        let delay = if self.autosave_delay.is_finite() {
            self.autosave_delay
                .clamp(AUTOSAVE_MIN_DELAY, AUTOSAVE_MAX_DELAY)
        } else {
            DEFAULT_AUTOSAVE_DELAY
        };
        let value = json!({
            "theme": self.theme.as_key(),
            "collapsed": self.collapsed,
            "last_vault": self
                .last_vault
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            "selected_rel": self.selected_rel,
            "mode": self.mode.as_key(),
            "scope": self.scope.as_key(),
            "autosave_delay": delay,
            "recent_vaults": self
                .recent_vaults
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            "note_template": clamp_note_template(&self.note_template),
            "sync": self.sync.to_json(),
            "reduce_motion": self.reduce_motion.as_key(),
            "tray_icon": self.tray_icon,
            "close_to_tray": self.close_to_tray,
            "auto_start": self.auto_start,
            "show_shortcut": self.show_shortcut,
            "board": self.board.as_ref().map(|b| {
                let mut value = json!({ "rel": b.rel });
                if let Some(view) = b.view {
                    value["eye"] = json!(view.eye);
                    value["zoom"] = json!(view.zoom);
                }
                value
            }),
        });
        let bytes = serde_json::to_vec_pretty(&value)?;
        atomic_write(&path, &bytes)
    }

    /// Promote `path` to the front of the recent-vault list, de-duplicating and
    /// capping the list. Comparison is lexical: the app always records the path
    /// it just opened, so a vault cannot appear twice under two spellings that
    /// differ only by `..` unless the user actually typed both.
    pub fn push_recent(&mut self, path: &Path) {
        let path = path.to_path_buf();
        self.recent_vaults.retain(|known| known != &path);
        self.recent_vaults.insert(0, path);
        self.recent_vaults.truncate(RECENT_VAULTS_MAX);
    }

    fn from_value(value: &Value) -> Settings {
        let mut settings = Settings {
            theme: parse_theme_mode(value.get("theme").and_then(Value::as_str)),
            ..Settings::default()
        };
        if let Some(items) = value.get("collapsed").and_then(Value::as_array) {
            settings.collapsed = items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect();
        }
        settings.last_vault = value
            .get("last_vault")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        settings.selected_rel = value
            .get("selected_rel")
            .and_then(Value::as_str)
            .filter(|rel| !rel.is_empty())
            .map(str::to_owned);
        settings.mode = match value.get("mode").and_then(Value::as_str) {
            Some("preview") => Mode::Preview,
            Some("graph") => Mode::Graph,
            Some("board") => Mode::Board,
            _ => Mode::Editor,
        };
        settings.scope = match value.get("scope").and_then(Value::as_str) {
            Some("local") => Scope::Local,
            _ => Scope::Global,
        };
        if let Some(delay) = value.get("autosave_delay").and_then(Value::as_f64)
            && delay.is_finite()
        {
            settings.autosave_delay = delay.clamp(AUTOSAVE_MIN_DELAY, AUTOSAVE_MAX_DELAY);
        }
        if let Some(items) = value.get("recent_vaults").and_then(Value::as_array) {
            for item in items {
                if let Some(path) = item.as_str().filter(|path| !path.is_empty()) {
                    let path = PathBuf::from(path);
                    if !settings.recent_vaults.contains(&path) {
                        settings.recent_vaults.push(path);
                    }
                }
            }
            settings.recent_vaults.truncate(RECENT_VAULTS_MAX);
        }
        settings.note_template = value
            .get("note_template")
            .and_then(Value::as_str)
            .map_or_else(String::new, clamp_note_template);
        settings.sync = SyncPrefs::from_value(value.get("sync"));
        settings.reduce_motion =
            ReduceMotion::from_key(value.get("reduce_motion").and_then(Value::as_str));
        let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
        settings.tray_icon = flag("tray_icon");
        settings.close_to_tray = flag("close_to_tray");
        settings.auto_start = flag("auto_start");
        settings.show_shortcut = flag("show_shortcut");
        settings.board = value.get("board").and_then(parse_board_session);
        settings
    }
}

/// The saved board: a non-empty `rel`, and a view only when `eye` and `zoom`
/// are both present and usable.
fn parse_board_session(value: &Value) -> Option<BoardSession> {
    let rel = value.get("rel")?.as_str().filter(|rel| !rel.is_empty())?;
    let eye = value.get("eye").and_then(Value::as_array).and_then(|eye| {
        let x = eye.first()?.as_f64()?;
        let y = eye.get(1)?.as_f64()?;
        (eye.len() == 2).then_some([x as f32, y as f32])
    });
    let zoom = value.get("zoom").and_then(Value::as_f64);
    let view = match (eye, zoom) {
        (Some(eye), Some(zoom)) => BoardView::new(eye, zoom as f32),
        _ => None,
    };
    Some(BoardSession {
        rel: rel.to_owned(),
        view,
    })
}

/// The XDG config root: `$XDG_CONFIG_HOME`, else `$HOME/.config`
/// (`%USERPROFILE%` on Windows), else a temp fallback.
#[must_use]
pub fn config_home() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", &[".config"], "muninn-config")
}

/// Shared XDG-root resolution for config and state directories.
pub(crate) fn xdg_dir(variable: &str, home_suffix: &[&str], fallback: &str) -> PathBuf {
    if let Some(value) = env::var_os(variable)
        && !value.is_empty()
    {
        return PathBuf::from(value);
    }
    if let Some(home) = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|home| !home.is_empty())
    {
        let mut path = PathBuf::from(home);
        for part in home_suffix {
            path.push(part);
        }
        return path;
    }
    env::temp_dir().join(fallback)
}

fn settings_path(config_home: &Path) -> PathBuf {
    config_home.join("muninn").join("settings.json")
}

/// Write `data` beside `path` and rename it into place. The settings file is
/// tiny, but a power loss during a plain write could still leave half a JSON
/// object; the rename keeps every observed version complete.
fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings");
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_file_name(format!(".{name}.tmp-{}-{counter}", std::process::id()));
    let result = (|| {
        let mut file = File::create(&temp)?;
        file.write_all(data)?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn temp_config(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "muninn-settings-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp config");
        dir
    }

    #[test]
    fn settings_round_trip_through_disk() {
        let home = temp_config("round-trip");
        let settings = Settings {
            theme: ThemeMode::Dark,
            collapsed: vec!["folder".to_owned(), String::new()],
            last_vault: Some(PathBuf::from("/tmp/notes")),
            selected_rel: Some("folder/Alpha.md".to_owned()),
            mode: Mode::Preview,
            scope: Scope::Local,
            autosave_delay: 0.8,
            recent_vaults: vec![PathBuf::from("/tmp/notes"), PathBuf::from("/tmp/other")],
            note_template: "---\ntags: []\n---".to_owned(),
            sync: SyncPrefs::default(),
            reduce_motion: ReduceMotion::On,
            tray_icon: true,
            close_to_tray: true,
            auto_start: true,
            show_shortcut: true,
            board: Some(BoardSession {
                rel: "sketches/Plan.excalidraw".to_owned(),
                view: BoardView::new([-120.37, 44.5], 1.3),
            }),
        };
        settings.save_to(&home).expect("save");
        assert_eq!(Settings::load_from(&home), settings);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn tray_slice_two_switches_are_off_by_default_and_in_old_files() {
        let defaults = Settings::default();
        assert!(!defaults.close_to_tray && !defaults.auto_start && !defaults.show_shortcut);
        let old = Settings::from_value(&serde_json::json!({ "theme": "dark", "tray_icon": true }));
        assert!(old.tray_icon);
        assert!(!old.close_to_tray && !old.auto_start && !old.show_shortcut);
        let each = Settings::from_value(&serde_json::json!({ "auto_start": true }));
        assert!(each.auto_start && !each.close_to_tray && !each.show_shortcut);
    }

    #[test]
    fn the_saved_board_loads_with_or_without_a_view() {
        assert_eq!(Settings::default().board, None);
        let old = Settings::from_value(&serde_json::json!({ "mode": "board" }));
        assert_eq!(old.board, None);
        let rel_only =
            Settings::from_value(&serde_json::json!({ "board": { "rel": "A.excalidraw" } }));
        assert_eq!(
            rel_only.board,
            Some(BoardSession {
                rel: "A.excalidraw".to_owned(),
                view: None
            })
        );
        let full = Settings::from_value(
            &serde_json::json!({ "board": { "rel": "A.excalidraw", "eye": [3.5, -2.0], "zoom": 2.0 } }),
        );
        assert_eq!(full.board.unwrap().view, BoardView::new([3.5, -2.0], 2.0));
        for bad in [
            serde_json::json!({ "board": { "rel": "" } }),
            serde_json::json!({ "board": "A.excalidraw" }),
            serde_json::json!({ "board": { "eye": [0, 0], "zoom": 1 } }),
        ] {
            assert_eq!(Settings::from_value(&bad).board, None, "{bad}");
        }
        let broken_view = Settings::from_value(
            &serde_json::json!({ "board": { "rel": "A.excalidraw", "eye": [0], "zoom": -1 } }),
        );
        assert_eq!(broken_view.board.unwrap().view, None);
    }

    #[test]
    fn corrupt_settings_fall_back_to_defaults() {
        let home = temp_config("corrupt");
        let path = settings_path(&home);
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(&path, "{this is not json").expect("garbage");
        assert_eq!(Settings::load_from(&home), Settings::default());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn note_template_is_empty_by_default_and_clamped() {
        assert!(Settings::default().note_template.is_empty());

        let long = "x".repeat(NOTE_TEMPLATE_MAX_CHARS + 10);
        let settings = Settings::from_value(&serde_json::json!({ "note_template": long }));
        assert_eq!(
            settings.note_template.chars().count(),
            NOTE_TEMPLATE_MAX_CHARS
        );
    }

    #[test]
    fn recent_vaults_are_mru_and_capped() {
        let mut settings = Settings::default();
        for index in 0..7 {
            settings.push_recent(Path::new(&format!("/vaults/{index}")));
        }
        assert_eq!(settings.recent_vaults.len(), RECENT_VAULTS_MAX);
        assert_eq!(settings.recent_vaults[0], PathBuf::from("/vaults/6"));
        assert_eq!(settings.recent_vaults[4], PathBuf::from("/vaults/2"));

        // Opening an older vault again promotes it without duplicating it.
        settings.push_recent(Path::new("/vaults/4"));
        assert_eq!(settings.recent_vaults[0], PathBuf::from("/vaults/4"));
        assert_eq!(
            settings
                .recent_vaults
                .iter()
                .filter(|path| path.as_path() == Path::new("/vaults/4"))
                .count(),
            1
        );
    }

    #[test]
    fn malformed_sync_disables_sync_and_settings_never_store_tokens() {
        let home = temp_config("sync-bad");
        fs::create_dir_all(home.join("muninn")).unwrap();
        let raw = r#"{"sync":{"version":1,"vaults":[{"provider":"icloud"}]}}"#;
        fs::write(home.join("muninn/settings.json"), raw).unwrap();
        let loaded = Settings::load_from(&home);
        assert!(loaded.sync.disabled_reason.is_some());
        assert!(loaded.sync.vaults.is_empty());
        loaded.save_to(&home).unwrap();
        let text = fs::read_to_string(home.join("muninn/settings.json")).unwrap();
        assert!(!text.contains("access_token"));
        assert!(!text.contains("refresh_token"));
        assert!(!text.contains("icloud"));
        let _ = fs::remove_dir_all(&home);
    }
}
