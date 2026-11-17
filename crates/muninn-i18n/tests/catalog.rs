//! The catalog's contract: stable ids, text for every key, and English output
//! that matches today's call sites.
//!
//! Missing English text is a compile error (the `en_us` match is exhaustive),
//! so there is no runtime "key has no text" test.

use std::collections::BTreeSet;

use muninn_i18n::{DateNames, EN_US, Key, Lang, files_skipped, notes, relative_day, t, t_in};

#[test]
fn the_catalog_is_not_empty() {
    assert!(!Key::ALL.is_empty(), "the catalog must seed some keys");
}

/// A file-format token is part of the on-disk contract, not display copy: every
/// shipped language keeps `YYYY-MM-DD`, `@{` and `@@{` exactly as the parser
/// and file formatter write them. A pack that localises one (for example
/// `AAAA-MM-DD`) tells the user to type something the app rejects.
#[test]
fn file_format_tokens_stay_english() {
    for &lang in Lang::SHIPPED {
        for (key, token) in [
            (Key::KanbanFilterDateError, "YYYY-MM-DD"),
            (Key::KanbanSettingDateTriggerHelp, "@{"),
            (Key::KanbanSettingTimeTriggerHelp, "@@{"),
        ] {
            let text = t_in(lang, key);
            assert!(
                text.contains(token),
                "{} in {} lost the file-format token {token:?}: {text:?}",
                key.id(),
                lang.tag()
            );
        }
    }
}

#[test]
fn ids_are_unique_and_dotted() {
    let mut seen = BTreeSet::new();
    for key in Key::ALL {
        let id = key.id();
        assert!(
            seen.insert(id),
            "duplicate catalog id {id:?} (from {key:?})"
        );
        assert!(
            id.contains('.'),
            "catalog id {id:?} (from {key:?}) is not dotted"
        );
        for segment in id.split('.') {
            assert!(
                !segment.is_empty()
                    && segment
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "catalog id {id:?} (from {key:?}) has a non-snake-case segment"
            );
        }
    }
}

#[test]
fn the_help_groups_are_six_and_distinct() {
    let groups: Vec<&str> = Key::ALL
        .iter()
        .map(|key| key.id())
        .filter(|id| id.starts_with("help.group."))
        .collect();
    assert_eq!(groups.len(), 6, "the Help page has six groups: {groups:?}");
    assert_eq!(
        groups.iter().collect::<BTreeSet<_>>().len(),
        6,
        "group ids repeat: {groups:?}"
    );
}

#[test]
fn the_kanban_sections_are_six_and_distinct() {
    let titles: Vec<&str> = Key::ALL
        .iter()
        .map(|key| key.id())
        .filter(|id| id.starts_with("kanban.section.") && id.ends_with(".title"))
        .collect();
    assert_eq!(
        titles.len(),
        6,
        "the settings schemas have six sections: {titles:?}"
    );
    assert_eq!(
        titles.iter().collect::<BTreeSet<_>>().len(),
        6,
        "section ids repeat: {titles:?}"
    );
    let texts: BTreeSet<&str> = titles
        .iter()
        .map(|id| {
            Key::ALL
                .iter()
                .find(|key| key.id() == *id)
                .map(|key| t(*key))
                .expect("a title id belongs to a key")
        })
        .collect();
    assert_eq!(texts.len(), 6, "section titles repeat: {texts:?}");
}

#[test]
fn every_key_has_text_without_placeholders() {
    // Two help lines quote the on-disk token syntax, whose braces are data,
    // not placeholders (docs/i18n.md §2 keeps the file format untranslated).
    const TOKEN_EXAMPLES: [&str; 2] = [
        "kanban.setting.date_trigger.help",
        "kanban.setting.time_trigger.help",
    ];
    for &key in Key::ALL {
        let text = t(key);
        assert!(!text.is_empty(), "{} has no text", key.id());
        if TOKEN_EXAMPLES.contains(&key.id()) {
            assert!(
                text.contains('{') && text.contains('}'),
                "{} is listed as a file-format example but lost its token: {text:?}",
                key.id()
            );
            continue;
        }
        assert!(
            !text.contains('{') && !text.contains('}'),
            "{} contains a placeholder; arguments belong in a typed helper: {text:?}",
            key.id()
        );
    }
}

#[test]
fn every_language_agrees_with_the_current_one() {
    for &key in Key::ALL {
        assert_eq!(
            t_in(Lang::EnUs, key),
            t(key),
            "{} differs between t_in(EnUs) and t",
            key.id()
        );
    }
    assert_eq!(Lang::current(), Lang::EnUs);
}

#[test]
fn files_skipped_matches_todays_status_bar() {
    assert_eq!(files_skipped(0), "0 files skipped");
    assert_eq!(files_skipped(1), "1 file skipped");
    assert_eq!(files_skipped(2), "2 files skipped");
}

#[test]
fn notes_matches_todays_plural() {
    assert_eq!(notes(0), "0 notes");
    assert_eq!(notes(1), "1 note");
    assert_eq!(notes(2), "2 notes");
}

#[test]
fn relative_day_matches_the_date_picker() {
    assert_eq!(relative_day(-2), "2 days ago");
    assert_eq!(relative_day(-1), "yesterday");
    assert_eq!(relative_day(0), "today");
    assert_eq!(relative_day(1), "tomorrow");
    assert_eq!(relative_day(2), "in 2 days");
}

#[test]
fn english_date_names_are_sunday_first() {
    assert_eq!(EN_US.months.len(), 12);
    assert_eq!(EN_US.months[0], "January");
    assert_eq!(EN_US.months[11], "December");

    assert_eq!(EN_US.weekdays.len(), 7);
    assert_eq!(EN_US.weekdays[0], "Sunday");
    assert_eq!(EN_US.weekdays[6], "Saturday");

    assert_eq!(EN_US.week_start, 0);
    assert_eq!(EN_US.am, "AM");
    assert_eq!(EN_US.pm, "PM");
}

#[test]
fn english_short_weekdays_are_sunday_first() {
    assert_eq!(
        EN_US.weekdays_short,
        ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
    );
    assert_eq!(
        EN_US.weekdays_narrow,
        ["S", "M", "T", "W", "T", "F", "S"],
        "the picker keeps the single initials Muninn has always drawn"
    );
}

#[test]
fn a_monday_first_table_moves_sunday_to_the_last_column() {
    let names = DateNames {
        week_start: 1,
        ..EN_US
    };
    // CivilDate::weekday(): Sunday 0, Monday 1, Saturday 6.
    assert_eq!(names.column(1), 0, "Monday is the first column");
    assert_eq!(names.column(0), 6, "Sunday is the last column");
    assert_eq!(names.column(6), 5, "Saturday is the sixth column");
    assert_eq!(names.short_weekday(0), "Mon");
    assert_eq!(names.short_weekday(6), "Sun");
    assert_eq!(names.narrow_weekday(0), "M");
    assert_eq!(names.narrow_weekday(6), "S");
    // The header rotates, so every column is still a distinct weekday.
    assert_eq!(
        (0..7).map(|at| names.short_weekday(at)).collect::<Vec<_>>(),
        ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
    );
}

#[test]
fn the_grid_column_wraps_around_the_week_start() {
    for week_start in 0..7u8 {
        let names = DateNames {
            week_start,
            ..EN_US
        };
        // The weekday `week_start` is always column 0, and the column before it
        // wraps to 6.
        assert_eq!(names.column(week_start), 0, "week start {week_start}");
        assert_eq!(
            names.column((week_start + 6) % 7),
            6,
            "week start {week_start}"
        );
        assert_eq!(
            names.short_weekday(0),
            EN_US.weekdays_short[week_start as usize],
            "week start {week_start}"
        );
        assert_eq!(
            names.narrow_weekday(0),
            EN_US.weekdays_narrow[week_start as usize],
            "week start {week_start}"
        );
        assert_eq!(
            names.short_weekday(6),
            EN_US.weekdays_short[usize::from(week_start + 6) % 7],
            "week start {week_start}"
        );
        for column in 0..7 {
            let weekday = (usize::from(week_start) + column) % 7;
            assert_eq!(
                names.column(weekday as u8),
                column,
                "week start {week_start}, column {column}"
            );
        }
    }
}
