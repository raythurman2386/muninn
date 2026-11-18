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
//! It stores the theme choice, the interface language, collapsed sidebar
//! folders, the last vault, the last note and mode/scope, the recent-vault MRU
//! list, the autosave delay, the default body inserted into new notes, the
//! Reduce motion choice, and the graph effect preset with its intensity.
//! Decoding is intentionally forgiving:
//! a corrupt or partially-written file falls back to defaults instead of
//! failing startup. Writes use the same temp-file-then-rename pattern as note
//! saves, so a crash leaves either the old complete file or the new one.

use std::env;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use muninn_core::KanbanDefaults;
use muninn_i18n::{Key as TextKey, Lang, LocaleTag, t};

use crate::{BoardSession, BoardView};

use crate::{Mode, Scope};

/// Default idle window before autosave, in seconds. The Settings page can
/// tune it between 0.05 and 10 seconds.
pub const DEFAULT_AUTOSAVE_DELAY: f64 = 0.55;

/// Shortest autosave idle window, in seconds.
pub const AUTOSAVE_MIN_DELAY: f64 = 0.05;

/// Longest autosave idle window, in seconds.
pub const AUTOSAVE_MAX_DELAY: f64 = 10.0;

/// How many recently opened vaults are remembered (MRU, newest first).
pub const RECENT_VAULTS_MAX: usize = 5;

/// Longest default note template kept, in characters. The Settings page and
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
            ThemeMode::Omarchy => t(TextKey::SettingsThemeOmarchy),
            ThemeMode::Follow => t(TextKey::SettingsThemeSystem),
            ThemeMode::Dark => t(TextKey::SettingsThemeDark),
            ThemeMode::Light => t(TextKey::SettingsThemeLight),
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

/// The UI language choice (Settings › Appearance).
///
/// Serialized as `"ui_language": "system" | "<bcp47 tag>"`, such as
/// `"system"` or `"en-US"`. [`LanguageChoice::System`] is the default and
/// follows the OS locale; only an explicit choice is persisted as a tag, so a
/// settings file written by an older build (no key) loads as System.
///
/// Loading is deliberately forgiving: an unknown spelling, a tag for a
/// language this build does not ship (a settings file copied from a newer
/// build, or `de-DE` before the German pack), a number, or a malformed value
/// all load as System. Falling back to the OS or English is safer than
/// refusing to start, and matches how the other settings enums decode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LanguageChoice {
    /// Follow the OS locale, falling back to English.
    #[default]
    System,
    /// An explicit language, which stays selected across restarts.
    Language(Lang),
}

impl LanguageChoice {
    /// The stored spelling: `"system"`, else the language's BCP 47 tag.
    #[must_use]
    pub const fn as_key(self) -> &'static str {
        match self {
            LanguageChoice::System => "system",
            LanguageChoice::Language(lang) => lang.tag(),
        }
    }

    /// Parse the `ui_language` value. Missing and unknown values are
    /// [`LanguageChoice::System`]; a known tag must name a shipped language,
    /// otherwise it is unknown too (see the type docs).
    #[must_use]
    pub fn from_key(value: Option<&str>) -> LanguageChoice {
        match value {
            Some(tag) if tag != "system" => Lang::from_tag(tag)
                .map_or(LanguageChoice::System, |lang| {
                    LanguageChoice::Language(lang)
                }),
            _ => LanguageChoice::System,
        }
    }

    /// The picker's label: catalog copy for System, the endonym otherwise.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            LanguageChoice::System => t(TextKey::SettingsAppearanceLanguageSystem),
            LanguageChoice::Language(lang) => lang.endonym(),
        }
    }

    /// Position in the picker: System, then [`Lang::SHIPPED`] in order.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            LanguageChoice::System => 0,
            LanguageChoice::Language(lang) => Lang::SHIPPED
                .iter()
                .position(|shipped| *shipped == lang)
                .map_or(0, |index| index + 1),
        }
    }

    /// The choice at a picker position. Out-of-range positions, and a
    /// non-shipped language (only reachable from a test), are System.
    #[must_use]
    pub fn from_index(index: usize) -> LanguageChoice {
        index
            .checked_sub(1)
            .and_then(|position| Lang::SHIPPED.get(position))
            .map_or(LanguageChoice::System, |lang| {
                LanguageChoice::Language(*lang)
            })
    }

    /// The language to draw in: an explicit choice as-is, System as the
    /// detected locale when this build ships it, else English.
    ///
    /// `detected` is a parameter rather than an environment read so tests can
    /// exercise System without mutating the process environment. The app
    /// passes [`muninn_i18n::system_locale`].
    #[must_use]
    pub fn resolve(self, detected: Option<LocaleTag>) -> Lang {
        match self {
            LanguageChoice::System => detected
                .and_then(|tag| Lang::from_tag(tag.tag()))
                .unwrap_or(Lang::EnUs),
            LanguageChoice::Language(lang) => lang,
        }
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
            return Self::invalid(t(TextKey::SyncErrorSettingsNotObject));
        }
        if value.get("version").and_then(Value::as_u64) != Some(1) {
            return Self::invalid(t(TextKey::SyncErrorSettingsVersion));
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
            return Self::invalid(t(TextKey::SyncErrorSettingsVaultList));
        };
        for item in vaults {
            match VaultSyncBinding::from_value(item) {
                Some(binding) => prefs.vaults.push(binding),
                None => {
                    prefs.disabled_reason =
                        Some(t(TextKey::SyncErrorSettingsBindingInvalid).to_owned());
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
            ReduceMotion::System => t(TextKey::SettingsMotionSystem),
            ReduceMotion::On => t(TextKey::SettingsMotionOn),
            ReduceMotion::Off => t(TextKey::SettingsMotionOff),
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

/// The knowledge-graph effect preset (Settings › Graph).
///
/// The preset picks one `rune-fx` family for **both** treatments the graph
/// needs: the node border (selection and direct neighbours) and the active
/// edge line. Keeping the two in one family is what makes a preset read as a
/// single style rather than two unrelated effects.
///
/// Serialized as `"classic"` / `"electric"` / `"off"` (`"minimal"` is accepted
/// on load and written back as `"off"`). A missing or unknown value, including
/// every file written before this setting existed, loads as
/// [`GraphEffects::Classic`] — today's graph look — so an upgrade never
/// changes the picture by itself.
///
/// A `Glow` preset is deliberately absent: `rune-fx`'s glow family only draws
/// a rounded-rect bloom, so it can treat the node border but not an arbitrary
/// (diagonal) edge segment. Offering it would mean faking one of the pair.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphEffects {
    /// Solid lines and the accent card: the original graph look.
    #[default]
    Classic,
    /// Electric arcs on edges and an electric border on the selection and its
    /// direct neighbours, all from `rune_fx::ElectricStyle`.
    Electric,
    /// Plain lines, no effect layers (`"off"` in `settings.json`, also
    /// accepted as the legacy key `"minimal"`).
    Off,
}

impl GraphEffects {
    /// Every choice in the order the settings segmented control shows them.
    pub const ALL: [GraphEffects; 3] = [
        GraphEffects::Classic,
        GraphEffects::Electric,
        GraphEffects::Off,
    ];

    #[must_use]
    pub fn as_key(self) -> &'static str {
        match self {
            GraphEffects::Classic => "classic",
            GraphEffects::Electric => "electric",
            GraphEffects::Off => "off",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            GraphEffects::Classic => t(TextKey::SettingsEffectsClassic),
            GraphEffects::Electric => t(TextKey::SettingsEffectsElectric),
            GraphEffects::Off => t(TextKey::SettingsEffectsOff),
        }
    }

    /// Position in [`GraphEffects::ALL`], for the segmented control.
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0)
    }

    /// Parse a persisted `graph_effects` value. Unknown values fall back to
    /// [`GraphEffects::Classic`], as a missing key does.
    #[must_use]
    pub fn from_key(value: Option<&str>) -> GraphEffects {
        match value {
            Some("electric") => GraphEffects::Electric,
            Some("off") | Some("minimal") => GraphEffects::Off,
            _ => GraphEffects::Classic,
        }
    }
}

/// How strong the graph effects are (Settings › Graph).
///
/// The graph effect preset ([`GraphEffects`]) picks the effect family; this
/// picks how much of it to draw. The compositor scales the arc's amplitude,
/// glow, forks and strand count with [`GraphEffectIntensity::scale`], so the
/// same preset reads as a faint shimmer, today's picture, or a brighter storm.
///
/// Serialized as `"subtle"` / `"balanced"` / `"intense"`. A missing or unknown
/// value, including every file written before this setting existed, loads as
/// [`GraphEffectIntensity::Balanced`] — a multiplier of 1.0, so an upgrade
/// never changes the picture by itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphEffectIntensity {
    /// Quiet: thin arcs, little glow, one strand.
    Subtle,
    /// The original look, and the default for old files.
    #[default]
    Balanced,
    /// Bright: wide arcs, more glow and forks (strands stay within the
    /// compositor's 1..=3 limit).
    Intense,
}

impl GraphEffectIntensity {
    /// Every choice in the order the settings segmented control shows them.
    pub const ALL: [GraphEffectIntensity; 3] = [
        GraphEffectIntensity::Subtle,
        GraphEffectIntensity::Balanced,
        GraphEffectIntensity::Intense,
    ];

    #[must_use]
    pub fn as_key(self) -> &'static str {
        match self {
            GraphEffectIntensity::Subtle => "subtle",
            GraphEffectIntensity::Balanced => "balanced",
            GraphEffectIntensity::Intense => "intense",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            GraphEffectIntensity::Subtle => t(TextKey::SettingsIntensitySubtle),
            GraphEffectIntensity::Balanced => t(TextKey::SettingsIntensityBalanced),
            GraphEffectIntensity::Intense => t(TextKey::SettingsIntensityIntense),
        }
    }

    /// Position in [`GraphEffectIntensity::ALL`], for the segmented control.
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|level| *level == self)
            .unwrap_or(1)
    }

    /// Parse a persisted `graph_effect_intensity` value. Unknown values fall
    /// back to [`GraphEffectIntensity::Balanced`], as a missing key does.
    #[must_use]
    pub fn from_key(value: Option<&str>) -> GraphEffectIntensity {
        match value {
            Some("subtle") => GraphEffectIntensity::Subtle,
            Some("intense") => GraphEffectIntensity::Intense,
            _ => GraphEffectIntensity::Balanced,
        }
    }

    /// The multiplier the FX compositor applies to the effect's shape and
    /// glow. [`GraphEffectIntensity::Balanced`] is exactly 1.0.
    #[must_use]
    pub fn scale(self) -> f32 {
        match self {
            GraphEffectIntensity::Subtle => 0.55,
            GraphEffectIntensity::Balanced => 1.0,
            GraphEffectIntensity::Intense => 1.65,
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
    /// Board slice 7: an open board with unsaved drawing is saved once it
    /// rests. On by default; `"board_autosave": false` in `settings.json`
    /// leaves saving to `⌘S` (and the quit save).
    pub board_autosave: bool,
    /// Kanban slice 2: the vault-relative path of the kanban note last shown
    /// in Kanban mode. Files saved before kanbans load as `None`.
    pub kanban: Option<String>,
    /// Kanban slice 7: unsaved kanban edits are saved once they rest, as a
    /// board's are. On by default; `"kanban_autosave": false` in
    /// `settings.json` leaves saving to `⌘S` (and the quit save).
    pub kanban_autosave: bool,
    /// Kanban slice 9: app-wide board defaults, used when a board's settings
    /// footer does not set a key. The object is absent until the user sets
    /// one, which means every built-in default.
    pub kanban_defaults: KanbanDefaults,
    /// Settings › Graph › Graph effects. Defaults to [`GraphEffects::Classic`]
    /// so existing files and fresh installs keep today's look.
    pub graph_effects: GraphEffects,
    /// Settings › Graph › Graph effect intensity. Defaults to
    /// [`GraphEffectIntensity::Balanced`], the look every file had before the
    /// setting existed.
    pub graph_effect_intensity: GraphEffectIntensity,
    /// Settings › Appearance › Language. Defaults to
    /// [`LanguageChoice::System`], so a fresh install follows the OS locale;
    /// only an explicit pick is persisted as a tag.
    pub ui_language: LanguageChoice,
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
            board_autosave: true,
            kanban: None,
            kanban_autosave: true,
            kanban_defaults: KanbanDefaults::default(),
            graph_effects: GraphEffects::Classic,
            graph_effect_intensity: GraphEffectIntensity::Balanced,
            ui_language: LanguageChoice::System,
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
        let mut value = json!({
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
            "board_autosave": self.board_autosave,
            "kanban": self.kanban,
            "kanban_autosave": self.kanban_autosave,
            "graph_effects": self.graph_effects.as_key(),
            "graph_effect_intensity": self.graph_effect_intensity.as_key(),
            "ui_language": self.ui_language.as_key(),
            "board": self.board.as_ref().map(|b| {
                let mut value = json!({ "rel": b.rel });
                if let Some(view) = b.view {
                    value["eye"] = json!(view.eye);
                    value["zoom"] = json!(view.zoom);
                }
                value
            }),
        });
        // An absent `kanban_defaults` is the same as an empty one; keeping it
        // out of old files means a settings write changes only what it must.
        if !self.kanban_defaults.is_empty() {
            value["kanban_defaults"] = self.kanban_defaults.to_json();
        }
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
            Some("kanban") => Mode::Kanban,
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
        settings.board_autosave = value
            .get("board_autosave")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        settings.kanban_autosave = value
            .get("kanban_autosave")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        settings.board = value.get("board").and_then(parse_board_session);
        settings.kanban = value
            .get("kanban")
            .and_then(Value::as_str)
            .filter(|rel| !rel.is_empty())
            .map(str::to_owned);
        settings.kanban_defaults = KanbanDefaults::from_json(value.get("kanban_defaults"));
        settings.graph_effects =
            GraphEffects::from_key(value.get("graph_effects").and_then(Value::as_str));
        settings.graph_effect_intensity = GraphEffectIntensity::from_key(
            value.get("graph_effect_intensity").and_then(Value::as_str),
        );
        settings.ui_language =
            LanguageChoice::from_key(value.get("ui_language").and_then(Value::as_str));
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
            board_autosave: false,
            kanban: Some("plans/Sprint.md".to_owned()),
            kanban_autosave: false,
            kanban_defaults: muninn_core::KanbanDefaults {
                date_trigger: Some("@d".to_owned()),
                hide_date_in_title: Some(false),
                ..muninn_core::KanbanDefaults::default()
            },
            graph_effects: GraphEffects::Electric,
            graph_effect_intensity: GraphEffectIntensity::Intense,
            ui_language: LanguageChoice::Language(Lang::EnUs),
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
    fn board_autosave_is_on_by_default_and_in_old_files() {
        assert!(Settings::default().board_autosave);
        assert!(Settings::from_value(&serde_json::json!({ "mode": "board" })).board_autosave);
        let off = Settings::from_value(&serde_json::json!({ "board_autosave": false }));
        assert!(!off.board_autosave);
    }

    #[test]
    fn kanban_autosave_is_on_by_default_and_in_old_files() {
        assert!(Settings::default().kanban_autosave);
        assert!(Settings::from_value(&serde_json::json!({ "mode": "kanban" })).kanban_autosave);
        let off = Settings::from_value(&serde_json::json!({ "kanban_autosave": false }));
        assert!(!off.kanban_autosave && off.board_autosave);
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
    fn mode_keys_are_appended_and_each_loads_back() {
        let modes = [
            Mode::Editor,
            Mode::Preview,
            Mode::Graph,
            Mode::Board,
            Mode::Kanban,
        ];
        for (i, mode) in modes.into_iter().enumerate() {
            assert_eq!(mode as usize, i, "a new mode goes at the end");
            let value = serde_json::json!({ "mode": mode.as_key() });
            assert_eq!(Settings::from_value(&value).mode, mode);
        }
        assert_eq!(Mode::Kanban.as_key(), "kanban");
    }

    #[test]
    fn the_saved_kanban_is_a_non_empty_path() {
        assert_eq!(Settings::default().kanban, None);
        let old = Settings::from_value(&serde_json::json!({ "mode": "board" }));
        assert_eq!(old.kanban, None);
        let saved = Settings::from_value(&serde_json::json!({ "kanban": "plans/Sprint.md" }));
        assert_eq!(saved.kanban.as_deref(), Some("plans/Sprint.md"));
        for bad in [
            serde_json::json!({ "kanban": "" }),
            serde_json::json!({ "kanban": 3 }),
            serde_json::json!({ "kanban": { "rel": "A.md" } }),
        ] {
            assert_eq!(Settings::from_value(&bad).kanban, None, "{bad}");
        }
    }

    #[test]
    fn kanban_defaults_are_absent_by_default_and_load_untyped_keys_out() {
        assert!(Settings::default().kanban_defaults.is_empty());
        let old = Settings::from_value(&json!({ "theme": "dark" }));
        assert!(old.kanban_defaults.is_empty());
        let set = Settings::from_value(&json!({
            "kanban_defaults": {
                "date-trigger": "@d",
                "hide-date-in-title": false,
                "date-format": 3,
                "unknown": "kept out",
            }
        }));
        assert_eq!(set.kanban_defaults.date_trigger.as_deref(), Some("@d"));
        assert_eq!(set.kanban_defaults.hide_date_in_title, Some(false));
        assert_eq!(set.kanban_defaults.date_format, None);
        assert!(!set.kanban_defaults.is_empty());

        // An empty object writes no key, so an old settings file keeps its bytes.
        let home = temp_config("kanban-defaults");
        Settings::default().save_to(&home).expect("save");
        let text = fs::read_to_string(settings_path(&home)).expect("read");
        assert!(!text.contains("kanban_defaults"), "{text}");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn kanban_defaults_keep_unknown_and_mistyped_keys_through_a_save() {
        let home = temp_config("kanban-defaults-raw");
        let settings = Settings {
            kanban_defaults: muninn_core::KanbanDefaults::from_json(Some(&json!({
                "date-trigger": "@d",
                "date-format": 3,
                "hide-date-in-title": "no",
                "unknown": {"x": 1},
            }))),
            ..Settings::default()
        };
        settings.save_to(&home).expect("save");
        let text = fs::read_to_string(settings_path(&home)).expect("read");
        assert!(text.contains("\"date-format\": 3"), "{text}");
        assert!(text.contains("\"hide-date-in-title\": \"no\""), "{text}");
        assert!(text.contains("\"unknown\""), "{text}");
        let loaded = Settings::load_from(&home);
        let out = loaded.kanban_defaults.to_json();
        assert_eq!(out["date-trigger"], "@d");
        assert_eq!(out["date-format"], 3);
        assert_eq!(out["hide-date-in-title"], "no");
        assert_eq!(out["unknown"]["x"], 1);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn kanban_defaults_keep_raw_linked_page_metadata_through_a_save() {
        let home = temp_config("kanban-defaults-metadata-raw");
        let raw = json!([
            {"key": "status", "label": "Status", "containsMarkdown": "yes"}
        ]);
        let settings = Settings {
            kanban_defaults: muninn_core::KanbanDefaults::from_json(Some(&json!({
                "linked-page-metadata": raw,
            }))),
            ..Settings::default()
        };
        settings.save_to(&home).expect("save");
        let text = fs::read_to_string(settings_path(&home)).expect("read");
        assert!(text.contains("\"label\": \"Status\""), "{text}");
        assert!(text.contains("\"containsMarkdown\": \"yes\""), "{text}");
        let loaded = Settings::load_from(&home);
        assert_eq!(
            loaded.kanban_defaults.to_json()["linked-page-metadata"],
            raw,
            "extra fields, order and the wrong type survive the round trip"
        );
        let _ = fs::remove_dir_all(&home);
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

    #[test]
    fn graph_effects_defaults_to_classic() {
        assert_eq!(Settings::default().graph_effects, GraphEffects::Classic);
        // A pre-feature file has no key at all.
        assert_eq!(
            Settings::from_value(&serde_json::json!({ "theme": "dark" })).graph_effects,
            GraphEffects::Classic
        );
        // A value this build does not know is the same as no value.
        assert_eq!(
            Settings::from_value(&serde_json::json!({ "graph_effects": "neon-soup" }))
                .graph_effects,
            GraphEffects::Classic
        );
    }

    #[test]
    fn graph_effects_keys_round_trip_and_minimal_is_an_alias_for_off() {
        for (key, expect) in [
            ("classic", GraphEffects::Classic),
            ("electric", GraphEffects::Electric),
            ("off", GraphEffects::Off),
            ("minimal", GraphEffects::Off),
        ] {
            let home = temp_config(&format!("fx-{key}"));
            let settings = Settings::from_value(&serde_json::json!({ "graph_effects": key }));
            assert_eq!(settings.graph_effects, expect, "key {key}");
            settings.save_to(&home).expect("save");
            assert_eq!(Settings::load_from(&home).graph_effects, expect);
            let text = fs::read_to_string(settings_path(&home)).expect("read");
            // `minimal` persists as the canonical `off` spelling.
            assert!(
                text.contains(&format!("\"graph_effects\": \"{}\"", expect.as_key())),
                "{text}"
            );
            let _ = fs::remove_dir_all(&home);
        }
    }

    #[test]
    fn graph_effect_intensity_defaults_to_balanced_for_old_and_unknown_files() {
        assert_eq!(
            Settings::default().graph_effect_intensity,
            GraphEffectIntensity::Balanced
        );
        assert_eq!(GraphEffectIntensity::Balanced.scale(), 1.0);
        // A file written before this setting existed has no key at all.
        assert_eq!(
            Settings::from_value(&serde_json::json!({ "graph_effects": "electric" }))
                .graph_effect_intensity,
            GraphEffectIntensity::Balanced
        );
        // A value this build does not know is the same as no value.
        assert_eq!(
            Settings::from_value(&serde_json::json!({ "graph_effect_intensity": "ludicrous" }))
                .graph_effect_intensity,
            GraphEffectIntensity::Balanced
        );
    }

    #[test]
    fn language_defaults_to_system_and_old_files_have_no_key() {
        assert_eq!(Settings::default().ui_language, LanguageChoice::System);
        // A file written before this setting existed has no key at all.
        assert_eq!(
            Settings::from_value(&serde_json::json!({ "theme": "dark" })).ui_language,
            LanguageChoice::System
        );
        assert_eq!(LanguageChoice::from_key(None), LanguageChoice::System);
        assert_eq!(
            LanguageChoice::from_key(Some("system")),
            LanguageChoice::System
        );
    }

    #[test]
    fn language_round_trips_and_corrupt_values_are_system() {
        let home = temp_config("language");
        let settings = Settings {
            ui_language: LanguageChoice::Language(Lang::EnUs),
            ..Settings::default()
        };
        settings.save_to(&home).expect("save");
        let text = fs::read_to_string(settings_path(&home)).expect("read");
        assert!(text.contains("\"ui_language\": \"en-US\""), "{text}");
        assert_eq!(Settings::load_from(&home).ui_language, settings.ui_language);

        for bad in [
            serde_json::json!({ "ui_language": "de-DE" }),
            serde_json::json!({ "ui_language": "klingon" }),
            serde_json::json!({ "ui_language": 3 }),
            serde_json::json!({ "ui_language": null }),
        ] {
            assert_eq!(
                Settings::from_value(&bad).ui_language,
                LanguageChoice::System,
                "{bad}"
            );
        }
        // An OS spelling of a shipped language is recognised and later written
        // back in canonical form.
        assert_eq!(
            Settings::from_value(&serde_json::json!({ "ui_language": "en_US.UTF-8" })).ui_language,
            LanguageChoice::Language(Lang::EnUs)
        );
        // The first non-English pack round-trips, and an OS spelling or a
        // regional variant is canonicalised to the shipped tag.
        let spanish = Settings {
            ui_language: LanguageChoice::Language(Lang::Es),
            ..Settings::default()
        };
        spanish.save_to(&home).expect("save");
        let text = fs::read_to_string(settings_path(&home)).expect("read");
        assert!(text.contains("\"ui_language\": \"es-ES\""), "{text}");
        assert_eq!(
            Settings::load_from(&home).ui_language,
            LanguageChoice::Language(Lang::Es)
        );
        for spelling in ["es_ES.UTF-8", "es-MX", "es"] {
            assert_eq!(
                Settings::from_value(&serde_json::json!({ "ui_language": spelling })).ui_language,
                LanguageChoice::Language(Lang::Es),
                "{spelling}"
            );
        }

        // French round-trips the same way; a Canadian OS spelling is
        // canonicalised to the shipped fr-FR tag.
        let french = Settings {
            ui_language: LanguageChoice::Language(Lang::Fr),
            ..Settings::default()
        };
        french.save_to(&home).expect("save");
        let text = fs::read_to_string(settings_path(&home)).expect("read");
        assert!(text.contains("\"ui_language\": \"fr-FR\""), "{text}");
        assert_eq!(
            Settings::load_from(&home).ui_language,
            LanguageChoice::Language(Lang::Fr)
        );
        for spelling in ["fr_FR.UTF-8", "fr-CA", "fr"] {
            assert_eq!(
                Settings::from_value(&serde_json::json!({ "ui_language": spelling })).ui_language,
                LanguageChoice::Language(Lang::Fr),
                "{spelling}"
            );
        }

        let system = Settings {
            ui_language: LanguageChoice::System,
            ..Settings::default()
        };
        system.save_to(&home).expect("save");
        let text = fs::read_to_string(settings_path(&home)).expect("read");
        assert!(text.contains("\"ui_language\": \"system\""), "{text}");
        assert_eq!(
            Settings::load_from(&home).ui_language,
            LanguageChoice::System
        );
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn system_resolves_only_shipped_languages() {
        use muninn_i18n::LocaleTag;

        assert_eq!(LanguageChoice::System.resolve(None), Lang::EnUs);
        assert_eq!(
            LanguageChoice::System.resolve(Some(LocaleTag::EnUs)),
            Lang::EnUs
        );
        // A shipped pack is resolved.
        assert_eq!(
            LanguageChoice::System.resolve(Some(LocaleTag::Es)),
            Lang::Es
        );
        assert_eq!(
            LanguageChoice::System.resolve(Some(LocaleTag::Fr)),
            Lang::Fr
        );
        // Recognised but not shipped in this build: English, the documented
        // fallback (not a silent German UI).
        assert_eq!(
            LanguageChoice::System.resolve(Some(LocaleTag::De)),
            Lang::EnUs
        );
        // An explicit choice ignores detection.
        assert_eq!(
            LanguageChoice::Language(Lang::EnUs).resolve(Some(LocaleTag::Fr)),
            Lang::EnUs
        );
        assert_eq!(
            LanguageChoice::Language(Lang::Fr).resolve(Some(LocaleTag::EnUs)),
            Lang::Fr
        );
        assert_eq!(
            LanguageChoice::Language(Lang::Es).resolve(Some(LocaleTag::EnUs)),
            Lang::Es
        );
        // The picker is System followed by the shipped endonyms.
        assert_eq!(LanguageChoice::from_index(0), LanguageChoice::System);
        assert_eq!(
            LanguageChoice::from_index(1),
            LanguageChoice::Language(Lang::EnUs)
        );
        assert_eq!(
            LanguageChoice::from_index(2),
            LanguageChoice::Language(Lang::Es)
        );
        assert_eq!(
            LanguageChoice::from_index(3),
            LanguageChoice::Language(Lang::Fr)
        );
        assert_eq!(LanguageChoice::from_index(4), LanguageChoice::System);
        assert_eq!(
            LanguageChoice::from_index(usize::MAX),
            LanguageChoice::System
        );
    }

    #[test]
    fn graph_effect_intensity_keys_round_trip_and_scale_in_order() {
        assert_eq!(
            GraphEffectIntensity::ALL.map(GraphEffectIntensity::index),
            [0, 1, 2]
        );
        let mut last = 0.0;
        for level in GraphEffectIntensity::ALL {
            let home = temp_config(&format!("fx-intensity-{}", level.as_key()));
            let settings = Settings::from_value(&serde_json::json!({
                "graph_effect_intensity": level.as_key(),
            }));
            assert_eq!(settings.graph_effect_intensity, level);
            settings.save_to(&home).expect("save");
            assert_eq!(Settings::load_from(&home).graph_effect_intensity, level);
            let text = fs::read_to_string(settings_path(&home)).expect("read");
            assert!(
                text.contains(&format!(
                    "\"graph_effect_intensity\": \"{}\"",
                    level.as_key()
                )),
                "{text}"
            );
            // Subtle < Balanced < Intense, and Balanced is exactly no change.
            assert!(level.scale() > last, "{} scales up", level.as_key());
            last = level.scale();
            let _ = fs::remove_dir_all(&home);
        }
        assert_eq!(GraphEffectIntensity::from_key(None).scale(), 1.0);
    }
}
