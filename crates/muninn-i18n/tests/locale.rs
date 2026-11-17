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
        ("es", Some(LocaleTag::Es)),
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
    // The `Lang` seam follows the same rules, including French's 0 = one,
    // which the counted helpers will pick up when the French pack lands.
    assert_eq!(Lang::EnUs.plural_category(1), PluralCategory::One);
    assert_eq!(Lang::EnUs.plural_category(0), PluralCategory::Other);
    assert_eq!(Lang::Pseudo.locale_tag(), LocaleTag::EnUs);
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
    assert_eq!(LocaleTag::parse("fr-FR"), Some(LocaleTag::Fr));
    assert_eq!(Lang::from_tag("fr-FR"), None);
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
