//! Locale identity, detection, plural rules and the process/thread language
//! seam.
//!
//! This is a separate integration binary because the seam test owns the
//! process-wide language while it runs. Tests in `catalog.rs` and
//! `messages.rs` use explicit `t_in`/`phrase` calls or their own thread
//! override, and this file's other tests never read [`Lang::current`], so the
//! global cannot make them flaky.

use muninn_i18n::{
    Key, Lang, LocaleTag, MsgKey, PluralCategory, date_names, locale_from, phrase, t_in,
};

#[test]
fn os_spellings_parse_to_canonical_tags() {
    let cases = [
        ("en-US", Some(LocaleTag::EnUs)),
        ("en_US", Some(LocaleTag::EnUs)),
        ("en_US.UTF-8", Some(LocaleTag::EnUs)),
        ("EN_us.utf8", Some(LocaleTag::EnUs)),
        ("en", Some(LocaleTag::EnUs)),
        ("fr-FR", Some(LocaleTag::Fr)),
        ("fr_FR.UTF-8", Some(LocaleTag::Fr)),
        ("fr", Some(LocaleTag::Fr)),
        ("FR-fr", Some(LocaleTag::Fr)),
        ("de_DE@euro", Some(LocaleTag::De)),
        ("de-DE", Some(LocaleTag::De)),
        ("es_ES.UTF-8", Some(LocaleTag::Es)),
        ("es-ES", Some(LocaleTag::Es)),
        ("es", Some(LocaleTag::Es)),
        ("es_MX.UTF-8", Some(LocaleTag::Es)),
        ("ES-ar", Some(LocaleTag::Es)),
        ("pt_BR.utf8", Some(LocaleTag::PtBr)),
        ("pt-BR", Some(LocaleTag::PtBr)),
        ("pt", Some(LocaleTag::PtBr)),
        ("PT_br", Some(LocaleTag::PtBr)),
        ("C", None),
        ("C.UTF-8", None),
        ("POSIX", None),
        ("posix", None),
        ("", None),
        ("   ", None),
        ("pt-PT", None),
        ("pt_PT.UTF-8", None),
        ("pt-AO", None),
        ("ja-JP", None),
        ("zh-Hans", None),
    ];
    for (input, expected) in cases {
        assert_eq!(LocaleTag::parse(input), expected, "parsing {input:?}");
    }
}

#[test]
fn every_tag_has_a_canonical_spelling_that_parses_back() {
    for &tag in LocaleTag::ALL {
        assert_eq!(
            LocaleTag::parse(tag.tag()),
            Some(tag),
            "{} is not round-trippable",
            tag.tag()
        );
        assert!(tag.tag().contains('-'), "{} is not BCP 47-ish", tag.tag());
    }
}

#[test]
fn environment_precedence_is_posix_order() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    };

    // LC_ALL wins over both.
    assert_eq!(
        locale_from(env(&[
            ("LC_ALL", "en_US.UTF-8"),
            ("LC_MESSAGES", "fr_FR.UTF-8"),
            ("LANG", "de_DE.UTF-8"),
        ])),
        Some(LocaleTag::EnUs)
    );
    // Empty LC_ALL is unset; LC_MESSAGES is next.
    assert_eq!(
        locale_from(env(&[
            ("LC_ALL", ""),
            ("LC_MESSAGES", "de_DE@euro"),
            ("LANG", "fr_FR.UTF-8"),
        ])),
        Some(LocaleTag::De)
    );
    // Then LANG.
    assert_eq!(
        locale_from(env(&[("LANG", "pt_BR.utf8")])),
        Some(LocaleTag::PtBr)
    );
    // A set but unparseable value decides; LANG is not consulted after C.
    assert_eq!(
        locale_from(env(&[("LC_ALL", "C"), ("LANG", "fr_FR.UTF-8")])),
        None
    );
    assert_eq!(
        locale_from(env(&[("LC_MESSAGES", "pt-PT"), ("LANG", "pt_BR")])),
        None
    );
    // Nothing set at all.
    assert_eq!(locale_from(env(&[])), None);
}

#[test]
fn plural_categories_at_zero_one_and_two_for_every_wave_one_tag() {
    for &tag in LocaleTag::ALL {
        let expected = if tag == LocaleTag::Fr {
            [
                PluralCategory::One,
                PluralCategory::One,
                PluralCategory::Other,
            ]
        } else {
            [
                PluralCategory::Other,
                PluralCategory::One,
                PluralCategory::Other,
            ]
        };
        for (n, want) in [0, 1, 2].into_iter().zip(expected) {
            assert_eq!(tag.plural_category(n), want, "{} at {n}", tag.tag());
        }
    }
    // The `Lang` seam follows the same rules; French 0 = one is what the
    // counted helpers pick up once the French pack is selected.
    assert_eq!(Lang::EnUs.plural_category(1), PluralCategory::One);
    assert_eq!(Lang::EnUs.plural_category(0), PluralCategory::Other);
    assert_eq!(Lang::Es.plural_category(1), PluralCategory::One);
    assert_eq!(Lang::Es.plural_category(0), PluralCategory::Other);
    assert_eq!(Lang::Fr.plural_category(0), PluralCategory::One);
    assert_eq!(Lang::Fr.plural_category(1), PluralCategory::One);
    assert_eq!(Lang::Fr.plural_category(2), PluralCategory::Other);
    assert_eq!(Lang::Pseudo.locale_tag(), LocaleTag::EnUs);
}

#[test]
fn spanish_is_shipped_and_any_es_region_resolves_to_it() {
    assert!(
        Lang::SHIPPED.contains(&Lang::Es),
        "the Spanish pack is shipped"
    );
    assert_eq!(Lang::from_tag("es-ES"), Some(Lang::Es));
    assert_eq!(Lang::from_tag("es_ES.UTF-8"), Some(Lang::Es));
    assert_eq!(Lang::Es.tag(), "es-ES");
    assert_eq!(Lang::Es.endonym(), "Español");
    assert_eq!(Lang::Es.locale_tag(), LocaleTag::Es);
    // One Spanish catalog serves every Spanish region and a bare `es`: the
    // alternative is English, which is further away than the es-ES pack.
    for tag in ["es", "es-MX", "es_AR.UTF-8", "ES"] {
        assert_eq!(
            Lang::from_tag(tag),
            Some(Lang::Es),
            "{tag} should resolve to the Spanish pack"
        );
    }
    // A still-unshipped language stays unshipped, so the test cannot hide a
    // missing arm behind the Spanish fallback.
    assert_eq!(LocaleTag::parse("de-DE"), Some(LocaleTag::De));
    assert_eq!(Lang::from_tag("de-DE"), None);
}

#[test]
fn french_is_shipped_and_any_fr_region_resolves_to_it() {
    assert!(
        Lang::SHIPPED.contains(&Lang::Fr),
        "the French pack is shipped"
    );
    assert_eq!(Lang::from_tag("fr-FR"), Some(Lang::Fr));
    assert_eq!(Lang::from_tag("fr"), Some(Lang::Fr));
    assert_eq!(Lang::from_tag("fr_CA.UTF-8"), Some(Lang::Fr));
    assert_eq!(Lang::Fr.tag(), "fr-FR");
    assert_eq!(Lang::Fr.endonym(), "Français");
    assert_eq!(Lang::Fr.locale_tag(), LocaleTag::Fr);
    // The CLDR French rule: 0 and 1 are singular, 2 and up are plural.
    assert_eq!(Lang::Fr.plural_category(0), PluralCategory::One);
    assert_eq!(Lang::Fr.plural_category(1), PluralCategory::One);
    assert_eq!(Lang::Fr.plural_category(2), PluralCategory::Other);
    // A still-unshipped language stays unshipped, so the test cannot hide a
    // missing arm behind a regional fallback.
    assert_eq!(LocaleTag::parse("de-DE"), Some(LocaleTag::De));
    assert_eq!(Lang::from_tag("de-DE"), None);
}

#[test]
fn shipped_languages_are_complete() {
    assert!(!Lang::SHIPPED.is_empty());
    for &lang in Lang::SHIPPED {
        assert!(
            Lang::from_tag(lang.tag()).is_some(),
            "{} is listed in SHIPPED but from_tag does not accept it",
            lang.tag()
        );
        assert!(!lang.endonym().is_empty(), "{} has no endonym", lang.tag());
        for &key in Key::ALL {
            let text = t_in(lang, key);
            assert!(
                !text.is_empty(),
                "{} has no text for {}",
                key.id(),
                lang.tag()
            );
        }
        for &key in MsgKey::ALL {
            let args = vec!["x"; key.arity()];
            let rendered = phrase(lang, key, &args);
            assert!(
                !rendered.is_empty(),
                "{} has no template for {}",
                key.id(),
                lang.tag()
            );
        }
        // Every shipped language has a date table.
        let names = date_names(lang);
        assert_eq!(names.months.len(), 12);
        assert_eq!(names.weekdays.len(), 7);
        assert!((names.week_start as usize) < 7);
    }
    // A tag no pack has claimed is recognised but not shipped.
    assert_eq!(LocaleTag::parse("de-DE"), Some(LocaleTag::De));
    assert_eq!(Lang::from_tag("de-DE"), None);
    assert_eq!(Lang::from_tag("pt-PT"), None);
}

#[test]
fn english_date_names_are_byte_identical() {
    assert_eq!(*date_names(Lang::EnUs), muninn_i18n::EN_US);
    assert!(
        std::ptr::eq(date_names(Lang::EnUs), date_names(Lang::Pseudo)),
        "the lookup returns one shared table for the languages that follow EnUs"
    );
    // `Pseudo` follows EnUs: it marks copy, not dates.
    assert_eq!(*date_names(Lang::Pseudo), muninn_i18n::EN_US);
    assert_eq!(date_names(Lang::EnUs).months[10], "November");
    assert_eq!(date_names(Lang::EnUs).am, "AM");
}
