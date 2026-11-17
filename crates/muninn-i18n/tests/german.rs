//! The German (`de-DE`) pack: plural forms, representative copy, dates, and a
//! no-English-fallback check over both catalogs.
//!
//! The fallback test is the important one: a copied English sentence would
//! still render, so the chatty checks above it are not enough. Every key or
//! message whose German text is byte-identical to English must appear in
//! [`INVARIANT_KEYS`] or [`INVARIANT_MESSAGES`], and each allowlist row must
//! still be identical (a stale row fails, so the list cannot grow unused).
//!
//! German shares many software terms with English — `Status`, `Link`, `Start`,
//! `Design`, `Text` — and nouns are capitalised, which is why this list is
//! longer than Spanish's. Every row names the invariant it relies on.
//!
//! Tests that use the counted/relative helpers set the thread language;
//! [`GermanGuard`] restores it even after a panic.

use muninn_i18n::{
    DE, EN_US, Key, Lang, MsgKey, date_names, files_skipped, notes, phrase, relative_day, set_lang,
    t_in,
};

/// Ties the thread override to one language and restores it on drop.
struct GermanGuard {
    before: Lang,
}

impl GermanGuard {
    fn enter() -> Self {
        let before = Lang::current();
        set_lang(Lang::De);
        Self { before }
    }
}

impl Drop for GermanGuard {
    fn drop(&mut self) {
        set_lang(self.before);
    }
}

/// Keys whose German spelling is intentionally the English one: proper nouns,
/// product/keycap terms, and cognates German spells the same way.
///
/// Every row is checked below: if one stops being identical the test fails, so
/// this stays a list of real invariants rather than a place to hide copy.
const INVARIANT_KEYS: &[(&str, &str)] = &[
    ("common.app_name", "proper noun: Muninn"),
    ("tray.tooltip", "proper noun: Muninn"),
    ("sync.provider.github.name", "proper noun: GitHub"),
    ("sync.provider.drive.name", "proper noun: Google Drive"),
    ("sync.provider.drive.short_name", "proper noun: Drive"),
    ("sync.provider.tailscale.name", "proper noun: Tailscale"),
    ("settings.theme.omarchy", "proper noun: Omarchy"),
    ("help.group.kanban", "product term: Kanban"),
    ("settings.tab.kanban", "product term: Kanban"),
    ("palette.hint.kanban", "product term: Kanban"),
    ("palette.section.kanbans", "product term: Kanban"),
    ("kanban.page.title", "product term: Kanban"),
    ("mode.kanban", "product term: Kanban"),
    ("sidebar.kanbans", "product term: KANBANS"),
    ("help.group.graph", "cognate: German spells Graph the same"),
    (
        "settings.tab.graph",
        "cognate: German spells Graph the same",
    ),
    ("mode.graph", "cognate: German spells Graph the same"),
    ("preview.graph", "cognate: German spells GRAPH the same"),
    (
        "graph.scope.global",
        "cognate: German spells Global the same",
    ),
    (
        "settings.tab.editor",
        "cognate: German spells Editor the same",
    ),
    (
        "help.group.navigation",
        "cognate: German spells Navigation the same",
    ),
    (
        "settings.appearance.language.system",
        "established term: System",
    ),
    ("settings.theme.system", "established term: System"),
    ("settings.motion.system", "established term: System"),
    ("kanban.section.app.title", "established term: App"),
    ("kanban.choice.enter", "keycap: Enter is printed on the key"),
    (
        "kanban.choice.shift_enter",
        "keycap chord: Shift+Enter is printed on the keys",
    ),
    ("link.name.fallback", "established term: Link"),
    ("sync.field.repository", "established git term: Repository"),
    ("sync.field.branch", "established git term: Branch"),
    ("board.tool.hand", "cognate: German spells Hand the same"),
    (
        "board.tool.ellipse",
        "cognate: German spells Ellipse the same",
    ),
    ("board.tool.text", "cognate: German spells Text the same"),
    ("kanban.filter.text", "cognate: German spells Text the same"),
    (
        "kanban.filter.status",
        "cognate: German spells Status the same",
    ),
    (
        "kanban.picker.minute",
        "cognate: German spells Minute the same",
    ),
    (
        "board.palette.swatch.orange",
        "cognate: German spells orange the same",
    ),
    (
        "board.palette.swatch.transparent",
        "cognate: German spells transparent the same",
    ),
    ("preview.links", "section label: LINKS is the same word"),
    (
        "preview.backlinks",
        "section label: BACKLINKS is the same word",
    ),
];

/// Templates that are only separators, a count or a product name, so the
/// German sentence is the English sentence by construction. A sentence with
/// words in it must not appear here.
const INVARIANT_MESSAGES: &[(&str, &str)] = &[
    ("msg.status.counts", "counts joined by ·"),
    ("msg.note.links", "counts joined by ·"),
    ("msg.note.links.folder", "folder and counts joined by ·"),
    ("msg.sidebar.summary", "counts joined by ·"),
    ("msg.graph.links", "counts joined by ·"),
    ("msg.card_label.text", "state and first line joined by :"),
    (
        "msg.date.relative_date",
        "date and relative day joined by ·",
    ),
    ("msg.settings.version", "Version is the same word in German"),
    ("msg.graph.node_parent", "title and parent in parentheses"),
    ("msg.kanban.filter_range", "two dates joined by an en dash"),
    (
        "msg.card_label.in_lane",
        "lane and card label joined by · and :",
    ),
    ("msg.import.join", "a leading separator and one count"),
    (
        "msg.sync.error.offline",
        "Offline is the established German term",
    ),
    (
        "msg.palette.kanban_folder",
        "product term and folder joined by ·",
    ),
];

#[test]
fn plural_helpers_render_zero_one_and_two_naturally() {
    let _guard = GermanGuard::enter();
    assert_eq!(notes(0), "0 Notizen");
    assert_eq!(notes(1), "1 Notiz");
    assert_eq!(notes(2), "2 Notizen");
    // A second family, to prove the singular/plural pair is per key.
    assert_eq!(files_skipped(0), "0 Dateien übersprungen");
    assert_eq!(files_skipped(1), "1 Datei übersprungen");
    assert_eq!(files_skipped(2), "2 Dateien übersprungen");
    assert_eq!(muninn_i18n::boards(1), "1 Tafel");
    assert_eq!(muninn_i18n::boards(3), "3 Tafeln");
    assert_eq!(muninn_i18n::targets(1), "1 Ziel");
    assert_eq!(muninn_i18n::targets(2), "2 Ziele");
    assert_eq!(muninn_i18n::archived_cards(1), "1 archivierte Karte");
    assert_eq!(muninn_i18n::archived_cards(3), "3 archivierte Karten");
}

#[test]
fn relative_days_follow_the_german_branches() {
    let _guard = GermanGuard::enter();
    assert_eq!(relative_day(-2), "vor 2 Tagen");
    assert_eq!(relative_day(-1), "gestern");
    assert_eq!(relative_day(0), "heute");
    assert_eq!(relative_day(1), "morgen");
    assert_eq!(relative_day(2), "in 2 Tagen");
}

#[test]
fn representative_copy_families_are_german() {
    let _guard = GermanGuard::enter();
    assert_eq!(t_in(Lang::De, Key::Save), "Speichern");
    assert_eq!(t_in(Lang::De, Key::Cancel), "Abbrechen");
    assert_eq!(t_in(Lang::De, Key::Close), "Schließen");
    assert_eq!(t_in(Lang::De, Key::AddCard), "Karte hinzufügen");
    assert_eq!(t_in(Lang::De, Key::CommonNewNote), "Neue Notiz");

    // Settings and Help.
    assert_eq!(t_in(Lang::De, Key::SettingsTitle), "Einstellungen");
    assert_eq!(t_in(Lang::De, Key::SettingsTabAppearance), "Darstellung");
    assert_eq!(
        t_in(Lang::De, Key::SettingsAppearanceLanguageTitle),
        "Sprache"
    );
    assert_eq!(t_in(Lang::De, Key::HelpGroupNavigation), "Navigation");
    assert_eq!(
        t_in(Lang::De, Key::HelpSidebarOpenThisHelp),
        "Diese Hilfe öffnen"
    );

    // Kanban, Board and the command palette.
    assert_eq!(t_in(Lang::De, Key::KanbanLaneAdd), "Spalte hinzufügen");
    assert_eq!(t_in(Lang::De, Key::KanbanCardText), "Kartentext");
    assert_eq!(t_in(Lang::De, Key::KanbanViewColumnCard), "Karte");
    assert_eq!(t_in(Lang::De, Key::BoardToolRectangle), "Rechteck");
    assert_eq!(t_in(Lang::De, Key::PaletteTitle), "Befehlspalette");

    // Status, errors, sync and the tray.
    assert_eq!(t_in(Lang::De, Key::BrokenTitle), "Fehlerhafte Links");
    assert_eq!(
        t_in(Lang::De, Key::StartupCouldNotOpen),
        "Tresor konnte nicht geöffnet werden"
    );
    assert_eq!(
        t_in(Lang::De, Key::SyncErrorCancelled),
        "Synchronisierung abgebrochen"
    );
    assert_eq!(t_in(Lang::De, Key::TrayNotConnected), "Nicht verbunden.");
    assert_eq!(t_in(Lang::De, Key::TrashEmpty), "Der Papierkorb ist leer");

    // Parameterised sentences keep their arguments in a German frame.
    assert_eq!(
        phrase(Lang::De, MsgKey::CouldNotSave, &["Datenträger voll"]),
        "Konnte nicht speichern: Datenträger voll"
    );
    assert_eq!(
        phrase(Lang::De, MsgKey::RestoreTo, &["Zu erledigen"]),
        "Wiederherstellen nach Zu erledigen"
    );
    assert_eq!(
        phrase(Lang::De, MsgKey::RelativeInDays, &["3"]),
        "in 3 Tagen"
    );
    assert_eq!(
        phrase(
            Lang::De,
            MsgKey::StatusCounts,
            &["12 Wörter", "2 Links", "0 Backlinks"]
        ),
        "12 Wörter · 2 Links · 0 Backlinks"
    );
    assert_eq!(
        phrase(Lang::De, MsgKey::ImportNeedsOcrOne, &["scan.pdf", "2", "9"]),
        "Konnte scan.pdf nicht importieren: Seite 2 von 9 ist ein gescanntes Bild und benötigt OCR, das Muninn nicht durchführt. Exportieren Sie zuerst eine Textversion."
    );
    assert_eq!(
        muninn_i18n::import_notice(1, 0, false),
        "1 Datei importiert"
    );
    assert_eq!(
        muninn_i18n::import_notice(2, 1, true),
        "2 Dateien importiert · 1 Notiz · Dateien, die kein Markdown sind, bleiben Anhänge"
    );
}

#[test]
fn the_german_date_table_is_monday_first_and_capitalised() {
    let names = date_names(Lang::De);
    assert!(
        !std::ptr::eq(names, date_names(Lang::EnUs)),
        "German must not reuse the English table"
    );
    assert_eq!(names.week_start, 1, "de-DE weeks start on Monday");
    assert_eq!(names.months[0], "Januar");
    assert_eq!(names.months[11], "Dezember");
    assert_eq!(names.weekdays[0], "Sonntag");
    assert_eq!(names.weekdays[6], "Samstag");
    assert_eq!(
        names.weekdays_short,
        ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"]
    );
    assert_eq!(
        names.weekdays_narrow,
        ["S", "M", "D", "M", "D", "F", "S"],
        "CLDR German repeats M for Montag/Mittwoch and D for Dienstag/Donnerstag"
    );
    assert_eq!(names.am, "AM");
    assert_eq!(names.pm, "PM");
    // The grid rotates: Monday is column 0, Sunday column 6.
    assert_eq!(names.short_weekday(0), "Mo");
    assert_eq!(names.short_weekday(6), "So");
    assert_eq!(names.narrow_weekday(0), "M");
    assert_eq!(names.narrow_weekday(6), "S");
    // The English and Spanish tables are untouched.
    assert_eq!(*date_names(Lang::EnUs), EN_US);
    assert_eq!(date_names(Lang::EnUs).months[0], "January");
    assert_eq!(date_names(Lang::Es).months[0], "enero");
    assert_eq!(DE.months[0], "Januar");
}

#[test]
fn german_static_copy_keeps_the_no_placeholder_rule() {
    // The two Help rows quote the on-disk token syntax; those braces are data,
    // exactly as in English, and every other German key must stay brace-free.
    const TOKEN_EXAMPLES: [&str; 2] = [
        "kanban.setting.date_trigger.help",
        "kanban.setting.time_trigger.help",
    ];
    for &key in Key::ALL {
        let text = t_in(Lang::De, key);
        if TOKEN_EXAMPLES.contains(&key.id()) {
            assert!(
                text.contains("@{") && text.contains('}'),
                "{} lost its file-format token: {text:?}",
                key.id()
            );
            continue;
        }
        assert!(
            !text.contains('{') && !text.contains('}'),
            "{} has a placeholder in German; arguments belong in a typed helper: {text:?}",
            key.id()
        );
    }
}

#[test]
fn german_templates_keep_every_placeholder_once() {
    // A sentinel no German sentence contains, so a missing or repeated
    // placeholder cannot hide behind natural words.
    for &key in MsgKey::ALL {
        let args: Vec<String> = (0..key.arity())
            .map(|index| format!("\u{1}{index}\u{1}"))
            .collect();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let rendered = phrase(Lang::De, key, &refs);
        assert!(
            !rendered.contains('{') && !rendered.contains('}'),
            "{} left a brace after substitution: {rendered:?}",
            key.id()
        );
        for arg in &args {
            assert_eq!(
                rendered.matches(arg.as_str()).count(),
                1,
                "{} lost or repeated a placeholder: {rendered:?}",
                key.id()
            );
        }
    }
}

#[test]
fn no_key_falls_back_to_english() {
    let mut identical = Vec::new();
    for &key in Key::ALL {
        let german = t_in(Lang::De, key);
        assert!(!german.is_empty(), "{} has no German text", key.id());
        assert!(
            !german.starts_with(muninn_i18n::PSEUDO_OPEN),
            "{} carries a pseudo marker",
            key.id()
        );
        if german == t_in(Lang::EnUs, key) {
            identical.push(key.id());
        }
    }
    for id in &identical {
        assert!(
            INVARIANT_KEYS.iter().any(|(known, _)| known == id),
            "{id} is still English; translate it or add a justified invariant row"
        );
    }
    // A row that stopped matching is stale and must be removed.
    for (id, reason) in INVARIANT_KEYS {
        let key = Key::ALL
            .iter()
            .find(|key| key.id() == *id)
            .unwrap_or_else(|| panic!("{id} is not a catalog key ({reason})"));
        assert_eq!(
            t_in(Lang::De, *key),
            t_in(Lang::EnUs, *key),
            "{id} is no longer the English spelling ({reason}); drop the allowlist row"
        );
    }
}

#[test]
fn no_message_falls_back_to_english() {
    let mut identical = Vec::new();
    for &key in MsgKey::ALL {
        let args = vec!["x"; key.arity()];
        let german = phrase(Lang::De, key, &args);
        assert!(!german.is_empty(), "{} has no German template", key.id());
        assert!(
            !german.contains(muninn_i18n::PSEUDO_OPEN),
            "{} carries a pseudo marker",
            key.id()
        );
        if german == phrase(Lang::EnUs, key, &args) {
            identical.push(key.id());
        }
    }
    for id in &identical {
        assert!(
            INVARIANT_MESSAGES.iter().any(|(known, _)| known == id),
            "{id} is still an English template; translate it or add a justified invariant row"
        );
    }
    for (id, reason) in INVARIANT_MESSAGES {
        let key = MsgKey::ALL
            .iter()
            .find(|key| key.id() == *id)
            .unwrap_or_else(|| panic!("{id} is not a message key ({reason})"));
        let args = vec!["x"; key.arity()];
        assert_eq!(
            phrase(Lang::De, *key, &args),
            phrase(Lang::EnUs, *key, &args),
            "{id} is no longer a separator-only template ({reason}); drop the allowlist row"
        );
    }
}
