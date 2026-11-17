//! The Brazilian Portuguese (`pt-BR`) pack: plural forms, representative copy,
//! dates, and a no-English-fallback check over both catalogs.
//!
//! The fallback test is the important one: a copied English sentence would
//! still render, so the chatty checks above it are not enough. Every key or
//! message whose Portuguese text is byte-identical to English must appear in
//! [`INVARIANT_KEYS`] or [`INVARIANT_MESSAGES`], and each allowlist row must
//! still be identical (a stale row fails, so the list cannot grow unused).
//!
//! Tests that use the counted/relative helpers set the thread language;
//! [`PortugueseGuard`] restores it even after a panic.

use muninn_i18n::{
    EN_US, ES, Key, Lang, MsgKey, PT_BR, date_names, files_skipped, notes, phrase, relative_day,
    set_lang, t_in,
};

/// Ties the thread override to one language and restores it on drop.
struct PortugueseGuard {
    before: Lang,
}

impl PortugueseGuard {
    fn enter() -> Self {
        let before = Lang::current();
        set_lang(Lang::PtBr);
        Self { before }
    }
}

impl Drop for PortugueseGuard {
    fn drop(&mut self) {
        set_lang(self.before);
    }
}

/// Keys whose Portuguese spelling is intentionally the English one: proper
/// nouns, product terms, keycaps, and words Portuguese and English share.
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
    ("kanban.page.title", "product term: Kanban"),
    ("mode.kanban", "product term: Kanban"),
    ("error.document_kanban", "file-format term: kanban"),
    ("sidebar.kanbans", "product term: KANBANS"),
    ("kanban.choice.enter", "keycap: Enter"),
    ("kanban.choice.shift_enter", "keycap chord: Shift+Enter"),
    (
        "settings.tab.editor",
        "cognate: Portuguese spells it Editor",
    ),
    ("sync.field.branch", "git term: branch, not ramificação"),
    ("graph.scope.global", "cognate: Portuguese spells it Global"),
    ("graph.scope.local", "cognate: Portuguese spells it Local"),
    (
        "kanban.filter.status",
        "cognate: Portuguese spells it Status",
    ),
    ("link.name.fallback", "glossary: link stays link"),
    ("preview.links", "glossary: LINKS stays LINKS"),
    ("preview.backlinks", "glossary: BACKLINKS stays BACKLINKS"),
    ("sidebar.links_ok", "status chip: links ok stays as is"),
];

/// Templates that are only separators plus a product name or a count, so the
/// Portuguese sentence is the English sentence by construction. A sentence
/// with words in it must not appear here.
const INVARIANT_MESSAGES: &[(&str, &str)] = &[
    ("msg.link.one", "glossary link plus a count"),
    ("msg.link.other", "glossary link plus a count"),
    ("msg.backlink.one", "glossary backlink plus a count"),
    ("msg.backlink.other", "glossary backlink plus a count"),
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
    let _guard = PortugueseGuard::enter();
    assert_eq!(notes(0), "0 notas");
    assert_eq!(notes(1), "1 nota");
    assert_eq!(notes(2), "2 notas");
    // A second family, to prove the singular/plural pair is per key.
    assert_eq!(files_skipped(0), "0 arquivos ignorados");
    assert_eq!(files_skipped(1), "1 arquivo ignorado");
    assert_eq!(files_skipped(2), "2 arquivos ignorados");
    assert_eq!(muninn_i18n::boards(1), "1 quadro");
    assert_eq!(muninn_i18n::boards(3), "3 quadros");
    assert_eq!(muninn_i18n::targets(1), "1 destino");
    assert_eq!(muninn_i18n::targets(2), "2 destinos");
    assert_eq!(muninn_i18n::archived_cards(1), "1 cartão arquivado");
    assert_eq!(muninn_i18n::archived_cards(3), "3 cartões arquivados");
}

#[test]
fn relative_days_follow_the_portuguese_branches() {
    let _guard = PortugueseGuard::enter();
    assert_eq!(relative_day(-2), "há 2 dias");
    assert_eq!(relative_day(-1), "ontem");
    assert_eq!(relative_day(0), "hoje");
    assert_eq!(relative_day(1), "amanhã");
    assert_eq!(relative_day(2), "em 2 dias");
}

#[test]
fn representative_copy_families_are_portuguese() {
    let _guard = PortugueseGuard::enter();
    assert_eq!(t_in(Lang::PtBr, Key::Save), "Salvar");
    assert_eq!(t_in(Lang::PtBr, Key::Cancel), "Cancelar");
    assert_eq!(t_in(Lang::PtBr, Key::Close), "Fechar");
    assert_eq!(t_in(Lang::PtBr, Key::AddCard), "Adicionar cartão");
    assert_eq!(t_in(Lang::PtBr, Key::CommonNewNote), "Nova nota");

    // Settings and Help.
    assert_eq!(t_in(Lang::PtBr, Key::SettingsTitle), "Configurações");
    assert_eq!(t_in(Lang::PtBr, Key::SettingsTabAppearance), "Aparência");
    assert_eq!(
        t_in(Lang::PtBr, Key::SettingsAppearanceLanguageTitle),
        "Idioma"
    );
    assert_eq!(t_in(Lang::PtBr, Key::HelpGroupNavigation), "Navegação");
    assert_eq!(
        t_in(Lang::PtBr, Key::HelpSidebarOpenThisHelp),
        "Abrir esta Ajuda"
    );

    // Kanban, Board and the command palette.
    assert_eq!(t_in(Lang::PtBr, Key::KanbanLaneAdd), "Adicionar coluna");
    assert_eq!(t_in(Lang::PtBr, Key::KanbanCardText), "Texto do cartão");
    assert_eq!(t_in(Lang::PtBr, Key::KanbanViewColumnCard), "Cartão");
    assert_eq!(t_in(Lang::PtBr, Key::BoardToolRectangle), "Retângulo");
    assert_eq!(t_in(Lang::PtBr, Key::PaletteTitle), "Paleta de comandos");

    // Status, errors, sync and the tray.
    assert_eq!(t_in(Lang::PtBr, Key::BrokenTitle), "Links não resolvidos");
    assert_eq!(
        t_in(Lang::PtBr, Key::StartupCouldNotOpen),
        "Não foi possível abrir o cofre"
    );
    assert_eq!(
        t_in(Lang::PtBr, Key::SyncErrorCancelled),
        "Sincronização cancelada"
    );
    assert_eq!(t_in(Lang::PtBr, Key::TrayNotConnected), "Sem conexão.");
    assert_eq!(t_in(Lang::PtBr, Key::TrashEmpty), "A lixeira está vazia");

    // Parameterised sentences keep their arguments in a Portuguese frame.
    assert_eq!(
        phrase(Lang::PtBr, MsgKey::CouldNotSave, &["disco cheio"]),
        "Não foi possível salvar: disco cheio"
    );
    assert_eq!(
        phrase(Lang::PtBr, MsgKey::RestoreTo, &["A fazer"]),
        "Restaurar em A fazer"
    );
    assert_eq!(
        phrase(Lang::PtBr, MsgKey::RelativeInDays, &["3"]),
        "em 3 dias"
    );
    assert_eq!(
        phrase(
            Lang::PtBr,
            MsgKey::StatusCounts,
            &["12 palavras", "2 links", "0 backlinks"]
        ),
        "12 palavras · 2 links · 0 backlinks"
    );
    assert_eq!(
        phrase(
            Lang::PtBr,
            MsgKey::ImportNeedsOcrOne,
            &["scan.pdf", "2", "9"]
        ),
        "Não foi possível importar scan.pdf: a página 2 de 9 é uma imagem digitalizada e precisa de OCR, que o Muninn não faz. Exporte primeiro uma versão em texto."
    );
    assert_eq!(
        muninn_i18n::import_notice(1, 0, false),
        "1 arquivo importado"
    );
    assert_eq!(
        muninn_i18n::import_notice(2, 1, true),
        "2 arquivos importados · 1 nota · arquivos que não são Markdown continuam como anexos"
    );
}

#[test]
fn the_portuguese_date_table_is_sunday_first_and_lower_case() {
    let names = date_names(Lang::PtBr);
    assert!(
        !std::ptr::eq(names, date_names(Lang::EnUs)),
        "Portuguese must not reuse the English table"
    );
    assert!(
        !std::ptr::eq(names, date_names(Lang::Es)),
        "Portuguese must not reuse the Spanish table"
    );
    assert_eq!(names.week_start, 0, "pt-BR weeks start on Sunday");
    assert_eq!(names.months[0], "janeiro");
    assert_eq!(names.months[11], "dezembro");
    assert_eq!(names.weekdays[0], "domingo");
    assert_eq!(names.weekdays[6], "sábado");
    assert_eq!(
        names.weekdays_short,
        ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"]
    );
    assert_eq!(
        names.weekdays_narrow,
        ["D", "S", "T", "Q", "Q", "S", "S"],
        "the pt-BR narrow row is CLDR's, repeated letters included"
    );
    assert_eq!(names.am, "AM");
    assert_eq!(names.pm, "PM");
    // Sunday-first: the grid does not rotate, so column 0 is domingo.
    assert_eq!(names.short_weekday(0), "dom");
    assert_eq!(names.short_weekday(6), "sáb");
    assert_eq!(names.narrow_weekday(0), "D");
    assert_eq!(names.narrow_weekday(6), "S");
    // The English and Spanish tables are untouched.
    assert_eq!(*date_names(Lang::EnUs), EN_US);
    assert_eq!(date_names(Lang::EnUs).months[0], "January");
    assert_eq!(ES.months[0], "enero");
    assert_eq!(PT_BR.months[0], "janeiro");
}

#[test]
fn portuguese_static_copy_keeps_the_no_placeholder_rule() {
    // The two Help rows quote the on-disk token syntax; those braces are data,
    // exactly as in English, and every other Portuguese key must stay
    // brace-free.
    const TOKEN_EXAMPLES: [&str; 2] = [
        "kanban.setting.date_trigger.help",
        "kanban.setting.time_trigger.help",
    ];
    for &key in Key::ALL {
        let text = t_in(Lang::PtBr, key);
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
            "{} has a placeholder in Portuguese; arguments belong in a typed helper: {text:?}",
            key.id()
        );
    }
}

#[test]
fn portuguese_templates_keep_every_placeholder_once() {
    // A sentinel no Portuguese sentence contains, so a missing or repeated
    // placeholder cannot hide behind natural words.
    for &key in MsgKey::ALL {
        let args: Vec<String> = (0..key.arity())
            .map(|index| format!("\u{1}{index}\u{1}"))
            .collect();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let rendered = phrase(Lang::PtBr, key, &refs);
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
        let portuguese = t_in(Lang::PtBr, key);
        assert!(
            !portuguese.is_empty(),
            "{} has no Portuguese text",
            key.id()
        );
        assert!(
            !portuguese.starts_with(muninn_i18n::PSEUDO_OPEN),
            "{} carries a pseudo marker",
            key.id()
        );
        if portuguese == t_in(Lang::EnUs, key) {
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
            t_in(Lang::PtBr, *key),
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
        let portuguese = phrase(Lang::PtBr, key, &args);
        assert!(
            !portuguese.is_empty(),
            "{} has no Portuguese template",
            key.id()
        );
        assert!(
            !portuguese.contains(muninn_i18n::PSEUDO_OPEN),
            "{} carries a pseudo marker",
            key.id()
        );
        if portuguese == phrase(Lang::EnUs, key, &args) {
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
            phrase(Lang::PtBr, *key, &args),
            phrase(Lang::EnUs, *key, &args),
            "{id} is no longer a separator-only template ({reason}); drop the allowlist row"
        );
    }
}
