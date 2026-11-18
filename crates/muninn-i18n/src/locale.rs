//! Locale identity, OS detection and the plural categories Muninn claims.
//!
//! A [`LocaleTag`] is the identity of a language the UI can be set to, and the
//! one value detection, persistence and date/plural data all agree on. It is
//! deliberately *not* [`crate::Lang`]: `Lang` is what has a catalog in this
//! build, so a tag can be recognised (`fr-FR`) while no French catalog exists
//! yet. [`Lang::from_tag`](crate::Lang::from_tag) is the bridge, and it only
//! accepts shipped variants.
//!
//! # Detection
//!
//! Rune at the pinned revision has no OS-locale accessor (`Cx` exposes dark
//! mode and reduced motion but not locale), and `NSLocale` /
//! `GetUserDefaultLocaleName` are not exposed either. Environment variables
//! are therefore the source on every platform, in POSIX precedence order:
//! `LC_ALL`, then `LC_MESSAGES`, then `LANG`. On macOS, where the Finder locale
//! is not in the environment of an app launched from the Dock, and on Windows,
//! where those variables are usually absent, detection falls back to English.
//! That fallback is the same as `Lang::default()`, and an explicit setting
//! always wins.

use std::env;

/// One wave-1 LTR locale Muninn can identify.
///
/// The variants are fixed by the translation issues (#193–#196); a locale
/// pack adds its [`crate::Lang`] variant and its catalog, not a new tag
/// spelling here. [`LocaleTag::parse`] accepts the spellings an OS actually
/// exports (`en_US.UTF-8`, `de_DE@euro`, `pt_BR.utf8`) as well as a bare
/// language code and a BCP 47 tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LocaleTag {
    /// English (United States), `en-US`.
    EnUs,
    /// Spanish (Spain), `es-ES`.
    Es,
    /// French (France), `fr-FR`.
    Fr,
    /// German (Germany), `de-DE`.
    De,
    /// Portuguese (Brazil), `pt-BR`.
    PtBr,
}

impl LocaleTag {
    /// Every tag in the order a locale pack lists them.
    pub const ALL: &'static [LocaleTag] = &[
        LocaleTag::EnUs,
        LocaleTag::Es,
        LocaleTag::Fr,
        LocaleTag::De,
        LocaleTag::PtBr,
    ];

    /// The canonical BCP 47 spelling, such as `"pt-BR"`.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            LocaleTag::EnUs => "en-US",
            LocaleTag::Es => "es-ES",
            LocaleTag::Fr => "fr-FR",
            LocaleTag::De => "de-DE",
            LocaleTag::PtBr => "pt-BR",
        }
    }

    /// The February 2026 wave-1 CLDR plural rule for `n`.
    ///
    /// Every wave-1 language has a two-category rule: English, Spanish, German
    /// and Brazilian Portuguese use `one` for exactly 1; French also uses
    /// `one` for 0 (`0 jour`, `1 jour`, `2 jours`). The signature takes `u64`
    /// so a caller cannot pass a negative count, and the enum has no `Few`,
    /// `Many` or `Two` yet: Polish, Russian and Arabic need those and the rule
    /// hook here is documented in `docs/i18n.md`.
    #[must_use]
    pub const fn plural_category(self, n: u64) -> PluralCategory {
        match self {
            LocaleTag::EnUs | LocaleTag::Es | LocaleTag::De | LocaleTag::PtBr => {
                if n == 1 {
                    PluralCategory::One
                } else {
                    PluralCategory::Other
                }
            }
            LocaleTag::Fr => {
                if n == 0 || n == 1 {
                    PluralCategory::One
                } else {
                    PluralCategory::Other
                }
            }
        }
    }

    /// Parse an OS locale spelling or a BCP 47 tag.
    ///
    /// Handled: `en_US.UTF-8`, `fr-FR`, `de_DE@euro`, `pt_BR.utf8`, a bare
    /// `es` or `pt`, and any case. The language subtag is matched, then a
    /// region where it matters:
    ///
    /// - `C`, `POSIX`, `C.UTF-8` and the empty string are `None` (the POSIX
    ///   default is English, i.e. the caller's fallback).
    /// - `pt` with no region is Brazilian Portuguese. That is a deliberate
    ///   choice for wave 1: a bare `pt` is ambiguous, and `pt-BR` is the pack
    ///   that ships. It is a one-line change here when a European Portuguese
    ///   pack exists.
    /// - `pt-PT` and any other `pt-<region>` except `pt-BR`/`pt_BR` are
    ///   `None`, never silently Brazilian: choosing a locale for the user is
    ///   worse than falling back to English.
    /// - `es` with **any** region (`es-ES`, `es-MX`, `es-AR`) is Spanish:
    ///   wave 1 ships one Spanish catalog, and an unshipped regional variant
    ///   is closer to the shipped pack than to English. The same rule maps
    ///   every `en-<region>` to [`LocaleTag::EnUs`].
    /// - Any other language (`ja-JP`, `zh-Hans`) is `None`; it is not a
    ///   wave-1 pack, and English is the documented fallback.
    ///
    /// # Examples
    ///
    /// ```
    /// use muninn_i18n::LocaleTag;
    ///
    /// assert_eq!(LocaleTag::parse("pt_BR.utf8"), Some(LocaleTag::PtBr));
    /// assert_eq!(LocaleTag::parse("de_DE@euro"), Some(LocaleTag::De));
    /// assert_eq!(LocaleTag::parse("es_MX.UTF-8"), Some(LocaleTag::Es));
    /// assert_eq!(LocaleTag::parse("pt-PT"), None);
    /// assert_eq!(LocaleTag::parse("C"), None);
    /// ```
    #[must_use]
    pub fn parse(value: &str) -> Option<LocaleTag> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        // Drop the codeset (`.UTF-8`) and locale modifier (`@euro`): neither
        // changes which language the locale is.
        let base = value.split(['.', '@']).next().unwrap_or(value);
        if base.is_empty() {
            return None;
        }
        let normalized = base.replace('_', "-").to_ascii_lowercase();
        if normalized == "c" || normalized == "posix" {
            return None;
        }
        let mut parts = normalized.split('-');
        let language = parts.next()?;
        let region = parts.next();
        match language {
            "en" => Some(LocaleTag::EnUs),
            "es" => Some(LocaleTag::Es),
            "fr" => Some(LocaleTag::Fr),
            "de" => Some(LocaleTag::De),
            "pt" => match region {
                None | Some("br") => Some(LocaleTag::PtBr),
                Some(_) => None,
            },
            _ => None,
        }
    }
}

/// The plural category a count falls into.
///
/// Wave 1 needs only these two; the enum exists rather than a `bool` so a
/// language with `few`/`many` can add variants without changing every helper's
/// signature. A helper matches `One` and picks its singular template, and
/// `Other` covers every category a future language adds (an explicit
/// `Few`/`Many` branch lands with that pack).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PluralCategory {
    /// The singular form (`1 day`, and French `0 jour`).
    One,
    /// Every other count (`0 days`, `2 days`).
    Other,
}

/// The system locale from the process environment, or `None` for English.
///
/// Reads `LC_ALL`, then `LC_MESSAGES`, then `LANG` through [`locale_from`].
/// See the module docs for why the environment is the source on every
/// platform.
#[must_use]
pub fn system_locale() -> Option<LocaleTag> {
    locale_from(|name| env::var(name).ok())
}

/// [`system_locale`] with the environment supplied by `get`, so a test can
/// drive detection without mutating the process environment.
///
/// The first variable that is **set and non-empty** decides, in POSIX order;
/// the remaining ones are not consulted even when the value is `C` or
/// unparseable. That matches `LC_ALL`'s override semantics: a process started
/// with `LC_ALL=C` is POSIX/English, whatever `LANG` says.
///
/// # Examples
///
/// ```
/// use muninn_i18n::{LocaleTag, locale_from};
///
/// let env = |name: &str| match name {
///     "LC_MESSAGES" => Some("fr-FR".to_owned()),
///     "LANG" => Some("de_DE.UTF-8".to_owned()),
///     _ => None,
/// };
/// assert_eq!(locale_from(env), Some(LocaleTag::Fr));
/// ```
#[must_use]
pub fn locale_from(mut get: impl FnMut(&str) -> Option<String>) -> Option<LocaleTag> {
    for name in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        let Some(value) = get(name) else {
            continue;
        };
        if value.trim().is_empty() {
            continue;
        }
        return LocaleTag::parse(&value);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_round_trip() {
        for &tag in LocaleTag::ALL {
            assert_eq!(LocaleTag::parse(tag.tag()), Some(tag), "{}", tag.tag());
        }
    }

    #[test]
    fn french_zero_is_singular() {
        assert_eq!(LocaleTag::Fr.plural_category(0), PluralCategory::One);
        assert_eq!(LocaleTag::Fr.plural_category(1), PluralCategory::One);
        assert_eq!(LocaleTag::Fr.plural_category(2), PluralCategory::Other);
        assert_eq!(LocaleTag::EnUs.plural_category(0), PluralCategory::Other);
        assert_eq!(LocaleTag::EnUs.plural_category(1), PluralCategory::One);
        assert_eq!(LocaleTag::EnUs.plural_category(2), PluralCategory::Other);
    }
}
