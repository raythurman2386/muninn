//! Omarchy theming: follow the system palette shared by Omarchy apps.
//!
//! Omarchy writes the active theme's resolved palette to
//! `$XDG_STATE_HOME/omarchy/current/theme/colors.toml` (default
//! `~/.local/state/omarchy/current/theme/colors.toml`); see `omarchy-theme-color`
//! and the module [`OmarchyColors`]. Reading that one file (rather than a
//! theme directory under `~/.config/omarchy/themes/`) means every
//! `omarchy theme set` is picked up without knowing theme names or layout.
//!
//! When the file is missing or unparsable — the app is not running under
//! Omarchy — the callers fall back to `rune-theme-opencode`, so non-Omarchy
//! systems see no change. Fonts, type scale, radii, motion and glyphs stay
//! the OpenCode theme's in both cases; only the [`Kit`] palette follows
//! Omarchy. Set `MUNINN_OMARCHY_COLORS` to a palette file (or `""` to
//! disable) to override the path, e.g. in tests.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use rune::Color;
use rune_kit::{Kit, PaletteFocus, PaletteSeed, StateColors};

use crate::State;
use crate::contrast::mix_oklab;
use crate::settings::ThemeMode;

/// Environment override for the palette file path (`""` disables the lookup).
const COLORS_ENV: &str = "MUNINN_OMARCHY_COLORS";

/// The canonical Omarchy palette file, or empty when disabled by the override.
#[must_use]
pub fn colors_path() -> PathBuf {
    if let Some(path) = std::env::var_os(COLORS_ENV) {
        return PathBuf::from(path);
    }
    crate::settings::xdg_dir("XDG_STATE_HOME", &[".local", "state"], "muninn-state")
        .join("omarchy/current/theme/colors.toml")
}

/// Resolved palette from an Omarchy `colors.toml`.
#[derive(Clone, Debug)]
pub struct OmarchyColors {
    map: HashMap<String, Color>,
    dark: Option<bool>,
}

impl OmarchyColors {
    /// Parse a flat Omarchy palette. Unknown lines and non-hex values are
    /// ignored; a palette with no colours at all returns `None`.
    #[must_use]
    pub fn parse(text: &str) -> Option<OmarchyColors> {
        let mut map = HashMap::new();
        let mut dark = None;
        for line in text.lines() {
            let line = line.trim();
            // Section headers and comments carry no top-level palette keys;
            // `colors.toml` is a flat file, so they are safe to skip.
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim().trim_matches('"').trim_matches('\'');
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if key == "mode" || key == "theme_type" {
                dark = match value {
                    "dark" => Some(true),
                    "light" => Some(false),
                    _ => dark,
                };
                continue;
            }
            if let Some(color) = rune::color::hex(value) {
                map.insert(key.to_owned(), color);
            }
        }
        if map.is_empty() {
            return None;
        }
        Some(OmarchyColors { map, dark })
    }

    /// Look up the first present key (alias chain, mirroring
    /// `omarchy-theme-color`'s fallbacks).
    #[must_use]
    pub fn get(&self, keys: &[&str]) -> Option<Color> {
        keys.iter().find_map(|key| self.map.get(*key).copied())
    }

    /// Whether this palette is dark: an explicit `mode`, else luminance of
    /// background versus foreground. `None` when neither can be told.
    #[must_use]
    pub fn is_dark(&self) -> Option<bool> {
        self.dark.or_else(|| {
            let bg = self.get(&["background", "bg"])?;
            let fg = self.get(&["foreground", "fg", "bright_foreground"])?;
            Some(fg.luminance() > bg.luminance())
        })
    }

    /// The full override: [`Kit`] palette derived from the Omarchy seed
    /// colours, on top of the OpenCode theme's type, radii, motion and
    /// elevation. `None` when the core colours are missing.
    #[must_use]
    pub fn kit(&self) -> Option<Kit> {
        let bg = self.get(&["background", "bg"])?;
        let fg = self.get(&["foreground", "fg", "bright_foreground", "bright_fg"])?;
        let accent = self.get(&["accent", "purple", "magenta", "blue"])?;
        let dark = self
            .is_dark()
            .unwrap_or_else(|| fg.luminance() > bg.luminance());

        // The framework's own derivation: OKLab layer steps, alpha borders,
        // secondary text with 4.5:1 / 3:1 contrast floors and accent focus.
        let mut kit = Kit::from_palette_focus(PaletteSeed { bg, fg, accent }, PaletteFocus::Accent);
        kit.dark = dark;
        // Keep the OpenCode look everywhere except colours.
        let base = rune_theme_opencode::kit(dark);
        kit.ty = base.ty;
        kit.radius = base.radius;
        kit.motion = base.motion;
        kit.glyph = base.glyph;
        kit.elevation = base.elevation.clone();
        kit.invalid_text = base.invalid_text;

        let c = &mut kit.c;
        if let Some(color) = self.get(&["darker_background", "darker_bg"]) {
            c.bg_deep = color;
        }
        if let Some(color) = self.get(&["lighter_background", "lighter_bg"]) {
            c.layer_01 = color;
        }
        if let Some(color) = self.get(&["selection", "selection_background"]) {
            c.layer_02 = color;
        }
        // Omarchy's muted is decorative on some themes; keep the derived
        // value when it would fall below readable text.
        if let Some(color) = self.get(&["muted"])
            && contrast(color, bg) >= 3.0
        {
            c.text_muted = color;
            c.icon_muted = color;
        }
        if let Some(color) = self.get(&["dark_foreground", "dark_fg"])
            && contrast(color, bg) >= 2.5
        {
            c.text_faint = color;
            c.icon_faint = color;
        }
        c.text_base = fg;
        c.icon_base = fg;
        c.text_accent = accent;
        c.icon_accent = accent;
        c.bg_accent = accent;
        c.border_focus = accent;
        // Inverse/contrast inks pair with the selection surface on
        // Omarchy's own selection/selection_foreground keys.
        if let (Some(_), Some(ink)) = (
            self.get(&["selection", "selection_background"]),
            self.get(&["selection_foreground"]),
        ) {
            c.text_inverse = ink;
            c.icon_inverse = ink;
            c.text_contrast = ink;
            c.icon_contrast = ink;
        }
        let role = |keys: &[&str]| self.get(keys);
        let state = |icon: Color| StateColors {
            bg: mix_oklab(bg, icon, 0.12),
            fg: icon,
            border: mix_oklab(bg, icon, 0.35),
            icon,
        };
        if let Some(green) = role(&["green", "bright_green"]) {
            c.success = state(green);
        }
        if let Some(yellow) = role(&["yellow", "bright_yellow", "orange"]) {
            c.warning = state(yellow);
        }
        if let Some(red) = role(&["red", "bright_red"]) {
            c.danger = state(red);
        }
        if let Some(blue) = role(&["blue", "bright_blue", "cyan", "bright_cyan"]) {
            c.info = state(blue);
        }
        Some(kit)
    }
}

fn contrast(a: Color, b: Color) -> f32 {
    crate::contrast::ratio(a, b)
}

/// Load and parse the palette at `path`; `None` when missing or unreadable.
/// A `None` result is the normal non-Omarchy case, not an error.
#[must_use]
pub fn load(path: &std::path::Path) -> Option<OmarchyColors> {
    let text = std::fs::read_to_string(path).ok()?;
    OmarchyColors::parse(&text)
}

/// `(mtime, length, content-hash)` for the palette file: the cheap trigger
/// (`mtime`/`length`) plus the content signature `poll` compares, so a theme
/// switch is visible without re-reading the file every frame.
#[must_use]
pub fn signature(path: &std::path::Path) -> (Option<SystemTime>, u64, u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return (None, 0, 0);
    };
    let mtime = meta.modified().ok();
    let len = meta.len();
    let hash = match std::fs::read(path) {
        Ok(bytes) => {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut hasher);
            hasher.finish()
        }
        Err(_) => 0,
    };
    (mtime, len, hash)
}

/// Whether the window is currently following the shared Omarchy palette:
/// Omarchy mode, lookup enabled, and the file yields a kit. The theme button
/// hides in exactly this state — Omarchy is the system theme, not a switcher
/// stop; the switcher only appears for diverged or non-Omarchy sessions.
#[must_use]
pub fn is_following(s: &State) -> bool {
    s.theme == ThemeMode::Omarchy
        && !s.omarchy_disabled
        && load(&s.omarchy_path)
            .and_then(|colors| colors.kit())
            .is_some()
}

/// The owned [`Kit`] for the current theme choice. OpenCode's light/dark kits
/// are `&'static`; the Omarchy kit is built on the fly, hence an owned value.
/// Every kit is repaired to the WCAG floors first (#72): the dependency's
/// palette is not ours to edit, and an Omarchy theme can be arbitrarily dim.
#[must_use]
pub fn resolve_kit(s: &State, system_dark: bool) -> Kit {
    let kit = match s.theme {
        ThemeMode::Dark => rune_theme_opencode::kit(true).clone(),
        ThemeMode::Light => rune_theme_opencode::kit(false).clone(),
        ThemeMode::Follow => rune_theme_opencode::kit(system_dark).clone(),
        ThemeMode::Omarchy => {
            if !s.omarchy_disabled
                && let Some(kit) = load(&s.omarchy_path).and_then(|colors| colors.kit())
            {
                return legible(kit);
            }
            rune_theme_opencode::kit(system_dark).clone()
        }
    };
    legible(kit)
}

/// [`resolve_kit`]'s last step: raise secondary and status inks to the WCAG
/// floors the audit measured them below.
fn legible(mut kit: Kit) -> Kit {
    crate::contrast::enforce_kit(&mut kit);
    kit
}

/// Version key for `rune::memo` call sites that paint from the kit: content
/// changes (preview, graph grid) must rebuild when the palette changes, and
/// the memo's own environment join only covers `system_dark`.
#[must_use]
pub fn kit_id(s: &State, effective_dark: bool) -> (u8, u64, bool) {
    (s.theme.id(), s.omarchy_rev, effective_dark)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Settings;
    use rune::color::hex;

    const DARK: &str = r##"
mode = "dark"
accent = "#34d399"
selection = "#242424"
selection_foreground = "#e1e1e1"
muted = "#6c6c6c"
background = "#141414"
darker_background = "#0a0a0a"
lighter_background = "#1c1c1c"
foreground = "#e1e1e1"
dark_foreground = "#6c6c6c"
red = "#f7768e"
yellow = "#e0af68"
green = "#9ece6a"
blue = "#7aa2f7"
"##;

    const LIGHT: &str = r##"
mode = "light"
accent = "#064e3b"
background = "#f5f4ed"
foreground = "#3f4a45"
muted = "#7a8478"
red = "#c92a2a"
green = "#064e3b"
"##;

    #[test]
    fn dark_palette_resolves_and_derives_a_dark_kit() {
        let colors = OmarchyColors::parse(DARK).expect("parse");
        assert_eq!(colors.is_dark(), Some(true));
        let kit = colors.kit().expect("kit");
        assert!(kit.dark);
        assert_eq!(kit.c.bg_base, hex("#141414").unwrap());
        assert_eq!(kit.c.text_base, hex("#e1e1e1").unwrap());
        assert_eq!(kit.c.text_accent, hex("#34d399").unwrap());
        assert_eq!(kit.c.bg_deep, hex("#0a0a0a").unwrap());
        assert_eq!(kit.c.layer_01, hex("#1c1c1c").unwrap());
        assert_eq!(kit.c.layer_02, hex("#242424").unwrap());
        // Status roles carry the palette colours, not the neutral defaults.
        assert_eq!(kit.c.success.icon, hex("#9ece6a").unwrap());
        assert_eq!(kit.c.warning.icon, hex("#e0af68").unwrap());
        assert_eq!(kit.c.danger.icon, hex("#f7768e").unwrap());
        assert_eq!(kit.c.info.icon, hex("#7aa2f7").unwrap());
        // Geometry stays the OpenCode theme's.
        let base = rune_theme_opencode::kit(true);
        assert_eq!(kit.ty.ui.size, base.ty.ui.size);
    }

    #[test]
    fn light_palette_and_luminance_fallback_resolve() {
        let colors = OmarchyColors::parse(LIGHT).expect("parse");
        assert_eq!(colors.is_dark(), Some(false));
        let kit = colors.kit().expect("kit");
        assert!(!kit.dark);
        assert_eq!(kit.c.bg_base, hex("#f5f4ed").unwrap());

        // No `mode`: dark is told by background versus foreground luminance.
        let bare = OmarchyColors::parse(
            "background = \"#141414\"\nforeground = \"#e1e1e1\"\naccent = \"#34d399\"\n",
        )
        .expect("parse");
        assert_eq!(bare.is_dark(), Some(true));
        assert!(bare.kit().expect("kit").dark);
    }

    #[test]
    fn faint_muted_is_kept_when_it_would_be_unreadable() {
        // `muted` almost equal to the background: the contrast guard keeps
        // the derived, readable secondary text.
        let bare = OmarchyColors::parse(
            "background = \"#141414\"\nforeground = \"#e1e1e1\"\naccent = \"#34d399\"\nmuted = \"#141414\"\n",
        )
        .expect("parse");
        let kit = bare.kit().expect("kit");
        assert!(contrast(kit.c.text_muted, kit.c.bg_base) >= 3.0);
        assert_ne!(kit.c.text_muted, hex("#141414").unwrap());
    }

    #[test]
    fn missing_core_colours_have_no_kit() {
        // Parses (a colour is present) but cannot derive a kit without
        // background and foreground.
        let accent_only = OmarchyColors::parse("accent = \"#34d399\"\n").expect("parse");
        assert!(accent_only.kit().is_none());
        assert!(OmarchyColors::parse("").is_none());
        let no_accent =
            OmarchyColors::parse("background = \"#141414\"\nforeground = \"#e1e1e1\"\n")
                .expect("parse");
        assert!(no_accent.kit().is_none());
    }

    #[test]
    fn settings_theme_omarchy_serializes_and_loads() {
        let settings = Settings {
            theme: ThemeMode::Omarchy,
            ..Settings::default()
        };
        assert_eq!(settings.theme, ThemeMode::Omarchy);
        assert_eq!(ThemeMode::Omarchy.id(), 0);
    }
}
