//! The Spanish (`es-ES`) pack: plural forms, representative copy, dates, and a
//! no-English-fallback check over both catalogs.
//!
//! The fallback test is the important one: a copied English sentence would
//! still render, so the chatty checks above it are not enough. Every key or
//! message whose Spanish text is byte-identical to English must appear in
//! [`INVARIANT_KEYS`] or [`INVARIANT_MESSAGES`], and each allowlist row must
//! still be identical (a stale row fails, so the list cannot grow unused).
//!
//! Tests that use the counted/relative helpers set the thread language;
//! [`SpanishGuard`] restores it even after a panic.

use muninn_i18n::{
    EN_US, ES, Key, Lang, MsgKey, date_names, files_skipped, notes, phrase, relative_day, set_lang,
    t_in,
};

/// Ties the thread override to one language and restores it on drop.
struct SpanishGuard {
    before: Lang,
}

impl SpanishGuard {
    fn enter() -> Self {
        let before = Lang::current();
        set_lang(Lang::Es);
        Self { before }
    }
}

impl Drop for SpanishGuard {
    fn drop(&mut self) {
        set_lang(self.before);
    }
}

/// Keys whose Spanish spelling is intentionally the English one: proper nouns,
/// a product term, keycaps, and two cognates that Spanish spells the same way.
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
    ("error.document_kanban", "file-format term: kanban"),
    ("sidebar.kanbans", "product term: KANBANS"),
    ("kanban.choice.enter", "keycap: Enter"),
    ("kanban.choice.shift_enter", "keycap chord: Shift+Enter"),
    ("settings.tab.editor", "cognate: Spanish spells it Editor"),
    ("graph.scope.global", "cognate: Spanish spells it Global"),
    ("graph.scope.local", "cognate: Spanish spells it Local"),
];

/// Templates that are only separators plus a product name or a count, so the
/// Spanish sentence is the English sentence by construction. A sentence with
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
    ("msg.graph.node_parent", "title and parent in parentheses"),
    ("msg.kanban.filter_range", "two dates joined by an en dash"),
    (
        "msg.card_label.in_lane",
        "lane and card label joined by · and :",
    ),
    ("msg.import.join", "a leading separator and one count"),
    (
        "msg.palette.kanban_folder",
        "product term and folder joined by ·",
    ),
];

#[test]
fn plural_helpers_render_zero_one_and_two_naturally() {
    let _guard = SpanishGuard::enter();
    assert_eq!(notes(0), "0 notas");
    assert_eq!(notes(1), "1 nota");
    assert_eq!(notes(2), "2 notas");
    // A second family, to prove the singular/plural pair is per key.
    assert_eq!(files_skipped(0), "0 archivos omitidos");
    assert_eq!(files_skipped(1), "1 archivo omitido");
    assert_eq!(files_skipped(2), "2 archivos omitidos");
    assert_eq!(muninn_i18n::boards(1), "1 tablero");
    assert_eq!(muninn_i18n::boards(3), "3 tableros");
    assert_eq!(muninn_i18n::targets(1), "1 destino");
    assert_eq!(muninn_i18n::targets(2), "2 destinos");
    assert_eq!(muninn_i18n::archived_cards(1), "1 tarjeta archivada");
    assert_eq!(muninn_i18n::archived_cards(3), "3 tarjetas archivadas");
}

#[test]
fn relative_days_follow_the_spanish_branches() {
    let _guard = SpanishGuard::enter();
    assert_eq!(relative_day(-2), "hace 2 días");
    assert_eq!(relative_day(-1), "ayer");
    assert_eq!(relative_day(0), "hoy");
    assert_eq!(relative_day(1), "mañana");
    assert_eq!(relative_day(2), "dentro de 2 días");
}

#[test]
fn representative_copy_families_are_spanish() {
    let _guard = SpanishGuard::enter();
    assert_eq!(t_in(Lang::Es, Key::Save), "Guardar");
    assert_eq!(t_in(Lang::Es, Key::Cancel), "Cancelar");
    assert_eq!(t_in(Lang::Es, Key::Close), "Cerrar");
    assert_eq!(t_in(Lang::Es, Key::AddCard), "Añadir tarjeta");
    assert_eq!(t_in(Lang::Es, Key::CommonNewNote), "Nueva nota");

    // Settings and Help.
    assert_eq!(t_in(Lang::Es, Key::SettingsTitle), "Ajustes");
    assert_eq!(t_in(Lang::Es, Key::SettingsTabAppearance), "Apariencia");
    assert_eq!(
        t_in(Lang::Es, Key::SettingsAppearanceLanguageTitle),
        "Idioma"
    );
    assert_eq!(t_in(Lang::Es, Key::HelpGroupNavigation), "Navegación");
    assert_eq!(
        t_in(Lang::Es, Key::HelpSidebarOpenThisHelp),
        "Abrir esta Ayuda"
    );

    // Kanban, Board and the command palette.
    assert_eq!(t_in(Lang::Es, Key::KanbanLaneAdd), "Añadir columna");
    assert_eq!(t_in(Lang::Es, Key::KanbanCardText), "Texto de la tarjeta");
    assert_eq!(t_in(Lang::Es, Key::KanbanViewColumnCard), "Tarjeta");
    assert_eq!(t_in(Lang::Es, Key::BoardToolRectangle), "Rectángulo");
    assert_eq!(t_in(Lang::Es, Key::PaletteTitle), "Paleta de comandos");

    // Status, errors, sync and the tray.
    assert_eq!(t_in(Lang::Es, Key::BrokenTitle), "Enlaces rotos");
    assert_eq!(
        t_in(Lang::Es, Key::StartupCouldNotOpen),
        "No se pudo abrir la bóveda"
    );
    assert_eq!(
        t_in(Lang::Es, Key::SyncErrorCancelled),
        "Sincronización cancelada"
    );
    assert_eq!(t_in(Lang::Es, Key::TrayNotConnected), "Sin conexión.");
    assert_eq!(t_in(Lang::Es, Key::TrashEmpty), "La papelera está vacía");

    // Parameterised sentences keep their arguments in a Spanish frame.
    assert_eq!(
        phrase(Lang::Es, MsgKey::CouldNotSave, &["disco lleno"]),
        "No se pudo guardar: disco lleno"
    );
    assert_eq!(
        phrase(Lang::Es, MsgKey::RestoreTo, &["Por hacer"]),
        "Restaurar en Por hacer"
    );
    assert_eq!(
        phrase(Lang::Es, MsgKey::RelativeInDays, &["3"]),
        "dentro de 3 días"
    );
    assert_eq!(
        phrase(
            Lang::Es,
            MsgKey::StatusCounts,
            &["12 palabras", "2 enlaces", "0 enlaces entrantes"]
        ),
        "12 palabras · 2 enlaces · 0 enlaces entrantes"
    );
    assert_eq!(
        phrase(Lang::Es, MsgKey::ImportNeedsOcrOne, &["scan.pdf", "2", "9"]),
        "No se pudo importar scan.pdf: la página 2 de 9 es una imagen escaneada y necesita OCR, que Muninn no hace. Exporta primero una versión de texto."
    );
    assert_eq!(
        muninn_i18n::import_notice(1, 0, false),
        "Se importó 1 archivo"
    );
    assert_eq!(
        muninn_i18n::import_notice(2, 1, true),
        "Se importaron 2 archivos · 1 nota · los archivos que no son Markdown siguen como adjuntos"
    );
}

#[test]
fn the_spanish_date_table_is_monday_first_and_lower_case() {
    let names = date_names(Lang::Es);
    assert!(
        !std::ptr::eq(names, date_names(Lang::EnUs)),
        "Spanish must not reuse the English table"
    );
    assert_eq!(names.week_start, 1, "es-ES weeks start on Monday");
    assert_eq!(names.months[0], "enero");
    assert_eq!(names.months[11], "diciembre");
    assert_eq!(names.weekdays[0], "domingo");
    assert_eq!(names.weekdays[6], "sábado");
    assert_eq!(
        names.weekdays_short,
        ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"]
    );
    assert_eq!(
        names.weekdays_narrow,
        ["D", "L", "M", "X", "J", "V", "S"],
        "Spanish uses X for miércoles so M stays martes"
    );
    assert_eq!(names.am, "a. m.");
    assert_eq!(names.pm, "p. m.");
    // The grid rotates: Monday is column 0, Sunday column 6.
    assert_eq!(names.short_weekday(0), "lun");
    assert_eq!(names.short_weekday(6), "dom");
    assert_eq!(names.narrow_weekday(0), "L");
    assert_eq!(names.narrow_weekday(6), "D");
    // The English table is untouched.
    assert_eq!(*date_names(Lang::EnUs), EN_US);
    assert_eq!(date_names(Lang::EnUs).months[0], "January");
    assert_eq!(ES.months[0], "enero");
}

#[test]
fn spanish_static_copy_keeps_the_no_placeholder_rule() {
    // The two Help rows quote the on-disk token syntax; those braces are data,
    // exactly as in English, and every other Spanish key must stay brace-free.
    const TOKEN_EXAMPLES: [&str; 2] = [
        "kanban.setting.date_trigger.help",
        "kanban.setting.time_trigger.help",
    ];
    for &key in Key::ALL {
        let text = t_in(Lang::Es, key);
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
            "{} has a placeholder in Spanish; arguments belong in a typed helper: {text:?}",
            key.id()
        );
    }
}

#[test]
fn spanish_templates_keep_every_placeholder_once() {
    // A sentinel no Spanish sentence contains, so a missing or repeated
    // placeholder cannot hide behind natural words.
    for &key in MsgKey::ALL {
        let args: Vec<String> = (0..key.arity())
            .map(|index| format!("\u{1}{index}\u{1}"))
            .collect();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let rendered = phrase(Lang::Es, key, &refs);
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
        let spanish = t_in(Lang::Es, key);
        assert!(!spanish.is_empty(), "{} has no Spanish text", key.id());
        assert!(
            !spanish.starts_with(muninn_i18n::PSEUDO_OPEN),
            "{} carries a pseudo marker",
            key.id()
        );
        if spanish == t_in(Lang::EnUs, key) {
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
            t_in(Lang::Es, *key),
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
        let spanish = phrase(Lang::Es, key, &args);
        assert!(!spanish.is_empty(), "{} has no Spanish template", key.id());
        assert!(
            !spanish.contains(muninn_i18n::PSEUDO_OPEN),
            "{} carries a pseudo marker",
            key.id()
        );
        if spanish == phrase(Lang::EnUs, key, &args) {
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
            phrase(Lang::Es, *key, &args),
            phrase(Lang::EnUs, *key, &args),
            "{id} is no longer a separator-only template ({reason}); drop the allowlist row"
        );
    }
}
