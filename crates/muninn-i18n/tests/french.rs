//! The French (`fr-FR`) pack: plural forms, representative copy, dates,
//! typography and a no-English-fallback check over both catalogs.
//!
//! The fallback test is the important one: a copied English sentence would
//! still render, so the chatty checks above it are not enough. Every key or
//! message whose French text is byte-identical to English must appear in
//! [`INVARIANT_KEYS`] or [`INVARIANT_MESSAGES`], and each allowlist row must
//! still be identical (a stale row fails, so the list cannot grow unused).
//!
//! French typography is pinned here too: prose uses U+2019 for elisions and a
//! no-break space U+00A0 before `:` `;` `!` `?` and inside `« »`. A regression
//! to an ASCII apostrophe or a plain space fails the sweep.
//!
//! Tests that use the counted/relative helpers set the thread language;
//! [`FrenchGuard`] restores it even after a panic.

use muninn_i18n::{
    EN_US, ES, FR, Key, Lang, MsgKey, date_names, files_skipped, notes, phrase, relative_day,
    set_lang, t_in,
};

/// Ties the thread override to one language and restores it on drop.
struct FrenchGuard {
    before: Lang,
}

impl FrenchGuard {
    fn enter() -> Self {
        let before = Lang::current();
        set_lang(Lang::Fr);
        Self { before }
    }
}

impl Drop for FrenchGuard {
    fn drop(&mut self) {
        set_lang(self.before);
    }
}

/// Keys whose French spelling is intentionally the English one: proper nouns,
/// product terms, keycaps, file-format terms and cognates French spells the
/// same way.
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
    ("palette.section.kanbans", "product term: Kanbans"),
    ("palette.section.notes", "cognate: French spells it Notes"),
    (
        "palette.section.actions",
        "cognate: French spells it Actions",
    ),
    (
        "help.group.navigation",
        "cognate: French spells it Navigation",
    ),
    ("kanban.page.title", "product term: Kanban"),
    ("mode.kanban", "product term: Kanban"),
    ("sidebar.kanbans", "product term: KANBANS"),
    ("error.document_kanban", "file-format term: kanban"),
    ("kanban.choice.enter", "keycap: Enter"),
    ("kanban.choice.shift_enter", "keycap chord: Shift+Enter"),
    ("graph.scope.global", "cognate: French spells it Global"),
    ("graph.scope.local", "cognate: French spells it Local"),
    ("kanban.view.table", "cognate: French spells it Table"),
    ("kanban.view.column.date", "cognate: French spells it Date"),
    ("kanban.filter.date", "cognate: French spells it Date"),
    ("kanban.picker.minute", "cognate: French spells it Minute"),
    (
        "board.tool.rectangle",
        "cognate: French spells it Rectangle",
    ),
    ("board.tool.ellipse", "cognate: French spells it Ellipse"),
    (
        "import.dialog.filter_documents",
        "cognate: French spells it Documents",
    ),
    (
        "settings.intensity.intense",
        "cognate: French spells it Intense",
    ),
    (
        "board.palette.swatch.orange",
        "cognate: French spells it orange",
    ),
    (
        "board.palette.swatch.transparent",
        "cognate: French spells it transparent",
    ),
];

/// Templates that are only separators plus a product name or a count, so the
/// French sentence is the English sentence by construction. A sentence with
/// words in it must not appear here.
const INVARIANT_MESSAGES: &[(&str, &str)] = &[
    ("msg.note.one", "cognate: French spells note the same"),
    ("msg.note.other", "cognate: French spells notes the same"),
    ("msg.status.counts", "counts joined by ·"),
    ("msg.note.links", "counts joined by ·"),
    ("msg.note.links.folder", "folder and counts joined by ·"),
    ("msg.sidebar.summary", "counts joined by ·"),
    ("msg.graph.links", "counts joined by ·"),
    (
        "msg.settings.version",
        "cognate: French spells Version the same",
    ),
    (
        "msg.date.relative_date",
        "date and relative day joined by ·",
    ),
    ("msg.graph.node_parent", "title and parent in parentheses"),
    ("msg.kanban.filter_range", "two dates joined by an en dash"),
    ("msg.import.join", "a leading separator and one count"),
    (
        "msg.palette.kanban_folder",
        "product term and folder joined by ·",
    ),
];

#[test]
fn plural_helpers_treat_zero_as_singular_and_two_as_plural() {
    let _guard = FrenchGuard::enter();
    // CLDR French: 0 and 1 are `one`, 2 and up are `other`.
    assert_eq!(notes(0), "0 note");
    assert_eq!(notes(1), "1 note");
    assert_eq!(notes(2), "2 notes");
    // A second family, to prove the singular/plural pair is per key and that
    // the adjective agrees with the counted noun.
    assert_eq!(files_skipped(0), "0 fichier ignoré");
    assert_eq!(files_skipped(1), "1 fichier ignoré");
    assert_eq!(files_skipped(2), "2 fichiers ignorés");
    assert_eq!(muninn_i18n::boards(1), "1 tableau");
    assert_eq!(muninn_i18n::boards(3), "3 tableaux");
    assert_eq!(muninn_i18n::targets(1), "1 cible");
    assert_eq!(muninn_i18n::targets(2), "2 cibles");
    assert_eq!(muninn_i18n::archived_cards(1), "1 carte archivée");
    assert_eq!(muninn_i18n::archived_cards(3), "3 cartes archivées");
}

#[test]
fn relative_days_follow_the_french_branches() {
    let _guard = FrenchGuard::enter();
    assert_eq!(relative_day(-2), "il y a 2 jours");
    assert_eq!(relative_day(-1), "hier");
    assert_eq!(relative_day(0), "aujourd’hui");
    assert_eq!(relative_day(1), "demain");
    assert_eq!(relative_day(2), "dans 2 jours");
}

#[test]
fn representative_copy_families_are_french() {
    let _guard = FrenchGuard::enter();
    // The instructed button/menu register: the infinitive.
    assert_eq!(t_in(Lang::Fr, Key::Save), "Enregistrer");
    assert_eq!(t_in(Lang::Fr, Key::Cancel), "Annuler");
    assert_eq!(t_in(Lang::Fr, Key::Close), "Fermer");
    assert_eq!(t_in(Lang::Fr, Key::AddCard), "Ajouter une carte");
    assert_eq!(t_in(Lang::Fr, Key::CommonNewNote), "Nouvelle note");
    assert_eq!(t_in(Lang::Fr, Key::CommonOpen), "Ouvrir");
    assert_eq!(t_in(Lang::Fr, Key::CommonDelete), "Supprimer");
    assert_eq!(t_in(Lang::Fr, Key::RecoveryRestore), "Restaurer");

    // Settings and Help.
    assert_eq!(t_in(Lang::Fr, Key::SettingsTitle), "Paramètres");
    assert_eq!(t_in(Lang::Fr, Key::SettingsTabAppearance), "Apparence");
    assert_eq!(
        t_in(Lang::Fr, Key::SettingsAppearanceLanguageTitle),
        "Langue"
    );
    assert_eq!(t_in(Lang::Fr, Key::HelpGroupNavigation), "Navigation");
    assert_eq!(
        t_in(Lang::Fr, Key::HelpSidebarOpenThisHelp),
        "Ouvrir cette aide"
    );
    assert_eq!(t_in(Lang::Fr, Key::TrashTitle), "Corbeille");
    assert_eq!(t_in(Lang::Fr, Key::PreviewBacklinks), "RÉTROLIENS");

    // Kanban, Board and the command palette.
    assert_eq!(t_in(Lang::Fr, Key::KanbanLaneAdd), "Ajouter une colonne");
    assert_eq!(t_in(Lang::Fr, Key::KanbanCardText), "Texte de la carte");
    assert_eq!(t_in(Lang::Fr, Key::KanbanViewColumnCard), "Carte");
    assert_eq!(t_in(Lang::Fr, Key::BoardToolRectangle), "Rectangle");
    assert_eq!(t_in(Lang::Fr, Key::PaletteTitle), "Palette de commandes");

    // Status, errors, sync and the tray.
    assert_eq!(t_in(Lang::Fr, Key::BrokenTitle), "Liens rompus");
    assert_eq!(
        t_in(Lang::Fr, Key::StartupCouldNotOpen),
        "Impossible d’ouvrir le coffre"
    );
    assert_eq!(
        t_in(Lang::Fr, Key::SyncErrorCancelled),
        "Synchronisation annulée"
    );
    assert_eq!(t_in(Lang::Fr, Key::TrayNotConnected), "Non connecté.");
    assert_eq!(t_in(Lang::Fr, Key::TrashEmpty), "La corbeille est vide");

    // Parameterised sentences keep their arguments in a French frame.
    assert_eq!(
        phrase(Lang::Fr, MsgKey::CouldNotSave, &["disque plein"]),
        "Impossible d’enregistrer\u{a0}: disque plein"
    );
    assert_eq!(
        phrase(Lang::Fr, MsgKey::RestoreTo, &["À faire"]),
        "Restaurer dans À faire"
    );
    assert_eq!(
        phrase(Lang::Fr, MsgKey::RelativeInDays, &["3"]),
        "dans 3 jours"
    );
    assert_eq!(
        phrase(
            Lang::Fr,
            MsgKey::StatusCounts,
            &["12 mots", "2 liens", "0 rétroliens"]
        ),
        "12 mots · 2 liens · 0 rétroliens"
    );
    assert_eq!(
        phrase(Lang::Fr, MsgKey::LinkedNoteHint, &["Plan"]),
        "Note liée\u{a0}: Plan · ⌘Enter ouvre"
    );
    assert_eq!(
        phrase(Lang::Fr, MsgKey::ImportNeedsOcrOne, &["scan.pdf", "2", "9"]),
        "Impossible d’importer scan.pdf\u{a0}: la page 2 sur 9 est une image numérisée et nécessite l’OCR, que Muninn ne fait pas. Exportez d’abord une version texte."
    );
    assert_eq!(muninn_i18n::import_notice(1, 0, false), "1 fichier importé");
    assert_eq!(
        muninn_i18n::import_notice(2, 1, true),
        "2 fichiers importés · 1 note · les fichiers non Markdown restent des pièces jointes"
    );
}

#[test]
fn the_french_date_table_is_monday_first_and_lower_case() {
    let names = date_names(Lang::Fr);
    assert!(
        !std::ptr::eq(names, date_names(Lang::EnUs)),
        "French must not reuse the English table"
    );
    assert!(
        !std::ptr::eq(names, date_names(Lang::Es)),
        "French must not reuse the Spanish table"
    );
    assert_eq!(names.week_start, 1, "fr-FR weeks start on Monday");
    assert_eq!(names.months[0], "janvier");
    assert_eq!(names.months[11], "décembre");
    assert_eq!(names.weekdays[0], "dimanche");
    assert_eq!(names.weekdays[6], "samedi");
    assert_eq!(
        names.weekdays_short,
        ["dim", "lun", "mar", "mer", "jeu", "ven", "sam"]
    );
    assert_eq!(
        names.weekdays_narrow,
        ["D", "L", "M", "M", "J", "V", "S"],
        "CLDR French repeats M (mardi and mercredi share an initial)"
    );
    assert_eq!(names.am, "AM");
    assert_eq!(names.pm, "PM");
    // The grid rotates: Monday is column 0, Sunday column 6.
    assert_eq!(names.short_weekday(0), "lun");
    assert_eq!(names.short_weekday(6), "dim");
    assert_eq!(names.narrow_weekday(0), "L");
    assert_eq!(names.narrow_weekday(6), "D");
    // The picker's narrow header really does show the repeated M, and the two
    // short labels that disambiguate it stay distinct.
    assert_eq!(names.narrow_weekday(1), "M");
    assert_eq!(names.narrow_weekday(2), "M");
    assert_eq!(names.short_weekday(1), "mar");
    assert_eq!(names.short_weekday(2), "mer");
    // The English and Spanish tables are untouched.
    assert_eq!(*date_names(Lang::EnUs), EN_US);
    assert_eq!(*date_names(Lang::Es), ES);
    assert_eq!(FR.months[0], "janvier");
}

#[test]
fn french_static_copy_keeps_the_no_placeholder_rule() {
    // The two Help rows quote the on-disk token syntax; those braces are data,
    // exactly as in English, and every other French key must stay brace-free.
    const TOKEN_EXAMPLES: [&str; 2] = [
        "kanban.setting.date_trigger.help",
        "kanban.setting.time_trigger.help",
    ];
    for &key in Key::ALL {
        let text = t_in(Lang::Fr, key);
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
            "{} has a placeholder in French; arguments belong in a typed helper: {text:?}",
            key.id()
        );
    }
}

#[test]
fn french_templates_keep_every_placeholder_once() {
    // A sentinel no French sentence contains, so a missing or repeated
    // placeholder cannot hide behind natural words.
    for &key in MsgKey::ALL {
        let args: Vec<String> = (0..key.arity())
            .map(|index| format!("\u{1}{index}\u{1}"))
            .collect();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let rendered = phrase(Lang::Fr, key, &refs);
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
fn french_prose_uses_typographic_apostrophes_and_no_break_spaces() {
    // A known caption carries the U+2019 elision: "d’exploitation".
    let caption = t_in(Lang::Fr, Key::SettingsAppearanceLanguageCaption);
    assert!(
        caption.contains('\u{2019}'),
        "the typographic apostrophe is part of the copy: {caption:?}"
    );
    assert!(
        !caption.contains('\''),
        "an ASCII apostrophe would be a typography regression: {caption:?}"
    );
    // A known error carries the U+00A0 before the colon.
    let error = phrase(Lang::Fr, MsgKey::CouldNotSave, &["disque plein"]);
    assert!(
        error.contains("\u{a0}:"),
        "a no-break space belongs before the colon: {error:?}"
    );
    // A known question carries the U+00A0 before the question mark.
    let question = t_in(Lang::Fr, Key::DeleteNoteQuestion);
    assert!(
        question.contains("\u{a0}?"),
        "a no-break space belongs before the question mark: {question:?}"
    );
    // A known quoted token uses French guillemets with no-break spaces.
    let quoted = t_in(Lang::Fr, Key::BoardParseNotExcalidraw);
    assert!(
        quoted.contains("«\u{a0}excalidraw\u{a0}»"),
        "the guillemets keep their no-break spaces: {quoted:?}"
    );
    // No French prose anywhere uses a plain ASCII apostrophe. Paths and
    // tokens never contain one, so this sweep is safe to apply to the whole
    // catalog.
    for &key in Key::ALL {
        let text = t_in(Lang::Fr, key);
        assert!(
            !text.contains('\''),
            "{} uses an ASCII apostrophe; use U+2019: {text:?}",
            key.id()
        );
    }
    for &key in MsgKey::ALL {
        let template = phrase(Lang::Fr, key, &vec!["x"; key.arity()]);
        assert!(
            !template.contains('\''),
            "{} uses an ASCII apostrophe; use U+2019: {template:?}",
            key.id()
        );
    }
}

#[test]
fn no_key_falls_back_to_english() {
    let mut identical = Vec::new();
    for &key in Key::ALL {
        let french = t_in(Lang::Fr, key);
        assert!(!french.is_empty(), "{} has no French text", key.id());
        assert!(
            !french.starts_with(muninn_i18n::PSEUDO_OPEN),
            "{} carries a pseudo marker",
            key.id()
        );
        if french == t_in(Lang::EnUs, key) {
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
            t_in(Lang::Fr, *key),
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
        let french = phrase(Lang::Fr, key, &args);
        assert!(!french.is_empty(), "{} has no French template", key.id());
        assert!(
            !french.contains(muninn_i18n::PSEUDO_OPEN),
            "{} carries a pseudo marker",
            key.id()
        );
        if french == phrase(Lang::EnUs, key, &args) {
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
            phrase(Lang::Fr, *key, &args),
            phrase(Lang::EnUs, *key, &args),
            "{id} is no longer a separator-only template ({reason}); drop the allowlist row"
        );
    }
}
