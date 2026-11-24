//! #159: a test-only pseudo-locale and a render check that bites.
//!
//! `xtask i18n-lint` catches an English literal at a call site, but it cannot
//! see a sentence assembled at draw time (`format!("...")`, a `match`, a
//! `&str` table) or one an extraction simply forgot. This check renders the
//! copy-heavy screens with [`Lang::Pseudo`], where every catalog key resolves
//! to its English wrapped in `⟦ … ⟧`, and asserts that every user-visible
//! string is either bracketed copy or documented non-copy.
//!
//! The pseudo language is a verification aid, not a shippable locale: nothing
//! selects it in a release build, and the default ([`Lang::EnUs`]) renders
//! byte-identical English.

use muninn_core::testing::TempVault;
use muninn_i18n::{
    Key, Lang, PSEUDO_CLOSE, PSEUDO_OPEN, add_card_to_lane, already_exists,
    attachment_not_in_vault, autosave_help, autostart_reg_exit, autostart_reg_failed, board_canvas,
    board_could_not_read, board_kept_as_is, board_name_invalid, board_name_taken,
    board_note_link_missing, board_quit_refused, board_renamed_embeds_failed,
    board_scene_key_object, broken_links, browser_failed, cannot_open_without_note, card_actions,
    card_actions_in_lane, card_label, card_label_in_lane, checklist_label, checklist_progress,
    collapse_lane, collapsed_cards, collapsed_lane_label, could_not_be_read,
    could_not_convert_kanban, could_not_create_board, could_not_create_kanban, could_not_do,
    could_not_open_vault, could_not_restore, could_not_save, day_label, delete_lane,
    delete_lane_text, deleted, device_code, expand_lane, files_skipped, filter_by_tag,
    focus_exit_help, focus_help, graph_node_parent, graph_stats_with_tags, import_document_notice,
    import_notice, import_progress, import_unsupported, kanban_could_not_save_copy,
    kanban_filter_from, kanban_filter_range, kanban_filter_to, kanban_quit_refused, lane_actions,
    lane_cards, lane_controls, lane_label, lane_region_label, lines, link_description, link_impact,
    linked_note_hint, links_not_updated, login_item_error, more_edits, moved_links_failed,
    new_folder_in, new_note_in, no_file_opener, no_longer_in_trash, no_note_named,
    not_a_board_in_vault, not_a_file, not_a_folder, not_a_trash_path, not_saved, note_label_root,
    note_links, note_not_in_vault, notes, outside_vault, palette_kanban_folder, recovered_copies,
    recovered_for, referrers, refusing_unsafe_recovery_path, relative_date, relative_day,
    renamed_links_failed, restore_to, scan_cancelled, scan_ms, scan_progress, set_lang,
    shared_key_copied, shortcut_failed, shortcut_on, show_with_chord, sidebar_summary,
    sign_in_failed, status_counts, sync_authentication_needed, sync_conflicts,
    sync_credentials_unavailable, sync_dirty_buffer, sync_excluded, sync_file_error,
    sync_key_rotation_failed, sync_limit_exceeded, sync_listing_incomplete, sync_local_changed,
    sync_not_configured, sync_nothing_saved, sync_now_after_error, sync_oauth_scope_rejected,
    sync_offline, sync_outcome_unknown, sync_permission_denied, sync_previous_credential,
    sync_provider_error, sync_rate_limited_after, sync_remote_changed, sync_remote_scope_not_found,
    sync_status, sync_unsupported, t, t_in, tailnet_hosting, tailnet_hosting_ready, targets,
    theme_label, trash_empty, trash_summary, tray_error, unsaved_saves, version,
    welcome_vault_created,
};
use muninn_ui::{LanguageChoice, Settings, SettingsTab, State, ThemeMode, on_key, poll, view};
use rune::{Env, Runtime};

/// The typed query below, drawn back by the palette's search field. It is user
/// input, not copy.
const PALETTE_QUERY: &str = "zzzzzz";

/// Puts this thread in [`Lang::Pseudo`] and back to [`Lang::EnUs`] on drop,
/// including when a test panics.
///
/// The language is thread-local, so tests in this binary cannot interfere with
/// each other: the guard must run on the same thread that renders the frames,
/// which it does because `Runtime::frame` is synchronous.
struct PseudoLanguage;

impl PseudoLanguage {
    fn enter() -> Self {
        set_lang(Lang::Pseudo);
        Self
    }
}

impl Drop for PseudoLanguage {
    fn drop(&mut self) {
        set_lang(Lang::EnUs);
    }
}

/// A small vault, in the app's dark theme, already framed once.
fn runtime() -> (PseudoLanguage, TempVault, Runtime<State>) {
    let language = PseudoLanguage::enter();
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    // Build with Pseudo as the saved choice so startup (which applies the
    // setting) and every cached sentence agree with the guard; entering the
    // language only after construction would leave Sync's startup status in
    // English.
    let settings = Settings {
        theme: ThemeMode::Dark,
        ui_language: LanguageChoice::Language(Lang::Pseudo),
        ..Settings::default()
    };
    let config = tmp.path().with_extension("config");
    let state = State::with_settings(
        tmp.path().to_path_buf(),
        tmp.path().with_extension("state"),
        config,
        settings,
    );
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 1.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    (language, tmp, rt)
}

fn settle(rt: &mut Runtime<State>) {
    for _ in 0..6 {
        rt.frame(1.0 / 60.0);
    }
}

/// Every string the frame drew, visible text and accessible label, in draw
/// order. Both matter: a hardcoded literal in a label is just as unlocalised as
/// one in a text node.
fn drawn_strings(rt: &Runtime<State>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .flat_map(|element| [element.text, element.label])
        .flatten()
        .collect()
}

/// Whether `text` is catalog copy: the pseudo catalog brackets every key, so a
/// lexically bracketed string proves it came through `t`/`t_in`.
fn is_catalog_copy(text: &str) -> bool {
    text.starts_with(PSEUDO_OPEN) && text.ends_with(PSEUDO_CLOSE)
}

/// The strings in `texts` that are neither catalog copy nor allowlisted.
///
/// Shared by the screen checks and the synthetic test below, so the mechanism
/// that makes the screens fail is itself tested.
fn copy_violations<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    texts
        .into_iter()
        .filter(|text| !is_catalog_copy(text) && allowlisted(text).is_none())
        .collect()
}

fn assert_screen_uses_the_catalog(screen: &str, rt: &Runtime<State>) {
    let drawn = drawn_strings(rt);
    assert!(
        drawn.len() >= 40,
        "{screen} drew only {} strings; the screen did not render, so the check would be vacuous",
        drawn.len()
    );
    let violations = copy_violations(drawn.iter().map(String::as_str));
    assert!(
        violations.is_empty(),
        "{screen} draws {} string(s) that are neither catalog copy (⟦ … ⟧) nor allowlisted \
         non-copy. Migrate the copy to a `Key` (or start with a typed helper), or extend the \
         allowlist in this test with a reason:\n{}",
        violations.len(),
        violations
            .iter()
            .map(|text| format!("  {text:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The English text wrapped exactly as the pseudo catalog wraps it.
fn pseudo_english(key: Key) -> String {
    format!("{PSEUDO_OPEN}{}{PSEUDO_CLOSE}", t_in(Lang::EnUs, key))
}

#[test]
fn pseudo_text_is_the_english_in_brackets() {
    assert_eq!(Lang::current(), Lang::EnUs, "the default is English");
    assert_eq!(t(Key::Close), "Close");
    assert_eq!(t_in(Lang::Pseudo, Key::Close), "⟦Close⟧");
    assert_eq!(
        t_in(Lang::Pseudo, Key::HelpNavFocusMode),
        "⟦Focus mode: hide the chrome and centre the caret⟧"
    );
    // English output is untouched: the pseudo catalog is a separate lookup.
    for &key in Key::ALL {
        assert_eq!(
            t_in(Lang::EnUs, key),
            t(key),
            "{} changed in English",
            key.id()
        );
    }
}

#[test]
fn help_draws_catalog_copy() {
    let (_language, _tmp, mut rt) = runtime();
    rt.update(|s, _| s.open_help());
    settle(&mut rt);
    assert_screen_uses_the_catalog("Help", &rt);

    let drawn = drawn_strings(&rt);
    for key in [
        Key::HelpGroupNavigation,
        Key::HelpNavFocusMode,
        Key::HelpGroupSidebar,
        Key::HelpSidebarSettings,
    ] {
        let expected = pseudo_english(key);
        assert!(
            drawn.iter().any(|text| text == &expected),
            "Help does not draw {expected:?} from the catalog"
        );
        assert!(
            !drawn.iter().any(|text| text == t_in(Lang::EnUs, key)),
            "Help redrew {} as a bare English literal",
            key.id()
        );
    }
}

#[test]
fn settings_draws_catalog_copy() {
    let (_language, _tmp, mut rt) = runtime();
    rt.update(|s, _| s.open_settings());
    settle(&mut rt);
    assert_screen_uses_the_catalog("Settings › Vaults", &rt);

    let drawn = drawn_strings(&rt);
    for key in [
        Key::SettingsVaultsOpenTitle,
        Key::SettingsVaultsOpenCaption,
        Key::SettingsTabVaults,
        Key::SettingsTabKanban,
    ] {
        let expected = pseudo_english(key);
        assert!(
            drawn.iter().any(|text| text == &expected),
            "Settings does not draw {expected:?} from the catalog"
        );
        assert!(
            !drawn.iter().any(|text| text == t_in(Lang::EnUs, key)),
            "Settings redrew {} as a bare English literal",
            key.id()
        );
    }

    // Every tab, not just the default one, because each tab has its own copy.
    for tab in SettingsTab::ALL {
        rt.update(|s, _| s.set_settings_tab(tab));
        settle(&mut rt);
        assert_screen_uses_the_catalog(&format!("Settings › {tab:?}"), &rt);
    }
}

#[test]
fn the_command_palette_draws_catalog_copy() {
    let (_language, _tmp, mut rt) = runtime();
    rt.update(|s, _| s.open_palette());
    settle(&mut rt);
    assert_screen_uses_the_catalog("command palette", &rt);

    // The dialog label, a section heading (drawn upper-case) and the footer.
    let drawn = drawn_strings(&rt);
    for expected in [
        pseudo_english(Key::PaletteTitle),
        t(Key::PaletteSectionActions).to_uppercase(),
        t(Key::PaletteFooter).to_owned(),
    ] {
        assert!(
            drawn.iter().any(|text| text == &expected),
            "the palette does not draw {expected:?} from the catalog"
        );
    }

    // The empty state is copy too, and it is a different code path.
    rt.update(|s, _| s.palette_field.replace_text(PALETTE_QUERY));
    settle(&mut rt);
    assert_screen_uses_the_catalog("command palette (empty)", &rt);
    let empty = pseudo_english(Key::PaletteEmpty);
    assert!(
        drawn_strings(&rt).iter().any(|text| text == &empty),
        "the palette empty state does not draw {empty:?}"
    );
}

/// The check must bite even while the screens are clean: an unbracketed
/// sentence is reported, and the allowlist does not swallow real copy.
#[test]
fn the_scan_catches_hardcoded_copy() {
    let flagged = copy_violations([
        "Close",
        "This sentence never went through the catalog.",
        "⟦Close⟧",
        "⌘K",
        "⌘⇧F",
        "0.55 s",
        "log C:\\Users\\runner\\AppData\\Local\\Temp\\muninn\\session.log",
        "",
    ]);
    assert_eq!(
        flagged,
        ["Close", "This sentence never went through the catalog."],
        "the scan must report unbracketed copy and keep allowlisted non-copy"
    );
    // A converted helper's bare English no longer passes: the helper now
    // renders a bracketed sentence, so the only way to draw "scan 9 ms" is to
    // bypass the seam.
    assert!(!copy_violations(["scan 9 ms"]).is_empty());
    assert!(copy_violations(["⟦scan 9 ms⟧"]).is_empty());
}

/// The first wave of #191 moved the count, error, sync, board and settings
/// sentences onto `MsgKey` templates, so they render bracketed like catalog
/// copy. This pins one sample per family: a helper that falls back to English
/// fails here even if no screen happens to draw it. The rest of the helpers are
/// in `remaining_helpers_render_bracketed` below.
#[test]
fn converted_helpers_render_bracketed() {
    let _language = PseudoLanguage::enter();
    let outputs: Vec<(&str, String)> = vec![
        ("files_skipped", files_skipped(2)),
        ("notes", notes(1)),
        ("targets", targets(2)),
        ("referrers", referrers(2)),
        ("status_counts", status_counts(12, 2, 0)),
        ("trash_summary", trash_summary(1, "2.0 KB")),
        ("trash_empty", trash_empty().to_owned()),
        ("more_edits", more_edits(2)),
        ("deleted", deleted("Plan.md")),
        ("scan_ms", scan_ms(9.0)),
        ("restore_to", restore_to("To do")),
        ("filter_by_tag", filter_by_tag("x")),
        ("link_impact", link_impact(1, "here")),
        ("sidebar_summary", sidebar_summary(1, 0)),
        ("note_links", note_links(0, 0)),
        ("could_not_save", could_not_save("disk full")),
        ("could_not_open_vault", could_not_open_vault("denied")),
        ("could_not_be_read", could_not_be_read("board")),
        (
            "could_not_restore",
            could_not_restore("Plan.md", "name taken"),
        ),
        (
            "refusing_unsafe_recovery_path",
            refusing_unsafe_recovery_path("../x.md"),
        ),
        ("board_quit_refused", board_quit_refused("disk full")),
        ("scan_cancelled", scan_cancelled(12, 40)),
        ("import_unsupported", import_unsupported("notes.bin")),
        ("import_progress", import_progress("report.docx")),
        (
            "import_document_notice",
            import_document_notice("Quarterly Report"),
        ),
        ("sync_status", sync_status("Not connected.")),
        ("sync_excluded", sync_excluded(2)),
        ("sync_conflicts", sync_conflicts(1, "Plan.md")),
        ("board_name_taken", board_name_taken("Plan")),
        ("no_note_named", no_note_named("Plan")),
        ("linked_note_hint", linked_note_hint("Plan")),
        ("board_canvas", board_canvas("2 elements", "rectangle", 1)),
        ("card_label", card_label("Done", "Ship it")),
        ("theme_label", theme_label("Dark")),
        ("autosave_help", autosave_help(1.5)),
        ("version", version("0.8.0")),
        ("show_with_chord", show_with_chord("Ctrl+Alt+M")),
    ];
    for (helper, text) in outputs {
        assert!(
            is_catalog_copy(&text),
            "{helper} does not render bracketed copy: {text:?}"
        );
    }
}

/// The rest of #191: every remaining parameterised helper names a `MsgKey`,
/// so its output is bracketed copy like the catalog's. Each family has a
/// sample here, including the sentences that compose other phrases.
#[test]
fn remaining_helpers_render_bracketed() {
    let _language = PseudoLanguage::enter();
    let outputs: Vec<(&str, String)> = vec![
        // Dates and relative days.
        ("relative_day today", relative_day(0)),
        ("relative_day tomorrow", relative_day(1)),
        ("relative_day yesterday", relative_day(-1)),
        ("relative_day future", relative_day(3)),
        ("relative_day past", relative_day(-2)),
        (
            "relative_date",
            relative_date("2026-11-14", &relative_day(0)),
        ),
        ("day_label", day_label(14)),
        // Actions and notices.
        ("new_note_in", new_note_in("Projects")),
        ("new_folder_in", new_folder_in("Projects")),
        ("welcome_vault_created", welcome_vault_created("~/notes")),
        ("tray_error", tray_error("no dock")),
        ("login_item_error", login_item_error("denied")),
        ("scan_progress", scan_progress(3, 12)),
        ("focus_help", focus_help("⌘⇧F")),
        ("focus_exit_help", focus_exit_help("⌘⇧F")),
        ("palette_kanban_folder", palette_kanban_folder("Projects")),
        ("shortcut_failed", shortcut_failed("Ctrl+Alt+M", "taken")),
        ("shortcut_on", shortcut_on("Ctrl+Alt+M")),
        ("no_file_opener", no_file_opener().to_owned()),
        // Links and paths.
        ("links_not_updated", links_not_updated(None)),
        ("links_not_updated named", links_not_updated(Some("Board"))),
        ("links_not_updated empty", links_not_updated(Some(""))),
        (
            "renamed_links_failed",
            renamed_links_failed(1, "Plan.md: denied"),
        ),
        (
            "moved_links_failed",
            moved_links_failed(1, "Plan.md: denied"),
        ),
        ("note_not_in_vault", note_not_in_vault(7)),
        (
            "cannot_open_without_note",
            cannot_open_without_note("pic.png"),
        ),
        (
            "attachment_not_in_vault",
            attachment_not_in_vault("pic.png"),
        ),
        ("not_a_file", not_a_file("/vault/assets")),
        ("broken_links", broken_links(3)),
        ("outside_vault", outside_vault("/tmp/notes")),
        (
            "not_a_board_in_vault",
            not_a_board_in_vault("Plan.excalidraw"),
        ),
        ("already_exists", already_exists("Plan.md")),
        ("not_a_trash_path", not_a_trash_path("../Plan.md")),
        ("no_longer_in_trash", no_longer_in_trash("1/Plan.md")),
        ("not_a_folder", not_a_folder("Notes.md")),
        (
            "board_name_invalid",
            board_name_invalid(t(Key::CoreBoardNameEmpty)),
        ),
        ("board_scene_key_object", board_scene_key_object("appState")),
        (
            "board_renamed_embeds_failed",
            board_renamed_embeds_failed(1, "a.md"),
        ),
        ("board_note_link_missing", board_note_link_missing("Plan")),
        ("autostart_reg_failed", autostart_reg_failed("denied")),
        ("autostart_reg_exit", autostart_reg_exit("exit code: 1")),
        // Board and graph chrome.
        ("board_kept_as_is", board_kept_as_is("1 element", 2)),
        ("board_could_not_read", board_could_not_read("bad json")),
        ("unsaved_saves", unsaved_saves("⌘S")),
        ("not_saved", not_saved("the file was removed")),
        (
            "could_not_do",
            could_not_do(t(Key::KanbanEditAddCard), "read-only"),
        ),
        ("could_not_create_board", could_not_create_board("denied")),
        ("could_not_create_kanban", could_not_create_kanban("denied")),
        (
            "could_not_convert_kanban",
            could_not_convert_kanban("has text"),
        ),
        ("link_description", link_description("Plan")),
        ("note_label_root", note_label_root("Plan")),
        ("graph_node_parent", graph_node_parent("Plan", "Projects")),
        ("graph_stats", graph_stats_with_tags(12, 3, 30)),
        // Kanban chrome.
        ("kanban_filter_from", kanban_filter_from("2026-11-01")),
        ("kanban_filter_to", kanban_filter_to("2026-11-30")),
        (
            "kanban_filter_range",
            kanban_filter_range("2026-11-01", "2026-11-30"),
        ),
        ("lane_actions", lane_actions("To do")),
        ("lane_controls", lane_controls("To do")),
        ("collapse_lane", collapse_lane("To do")),
        ("expand_lane", expand_lane("To do")),
        ("lane_cards", lane_cards("To do")),
        ("add_card_to_lane", add_card_to_lane("To do")),
        ("lane_label", lane_label("To do", "3", false, false)),
        ("lane_label flags", lane_label("Done", "2", true, true)),
        (
            "lane_region_label",
            lane_region_label("To do", "3", true, false),
        ),
        (
            "collapsed_lane_label",
            collapsed_lane_label("To do", "3", true),
        ),
        ("collapsed_cards", collapsed_cards(3)),
        ("delete_lane", delete_lane("To do", 1)),
        ("delete_lane none", delete_lane("To do", 0)),
        ("delete_lane many", delete_lane("To do", 3)),
        (
            "delete_lane_text",
            delete_lane_text("To do", 1, &lines(1), "hidden"),
        ),
        ("card_actions", card_actions("Ship it")),
        ("card_actions empty", card_actions("")),
        ("card_actions_in_lane", card_actions_in_lane("To do")),
        ("checklist_progress", checklist_progress(1, 2)),
        ("checklist_label", checklist_label(1, 2)),
        (
            "card_label_in_lane",
            card_label_in_lane("To do", t(Key::KanbanFilterDone), "Ship it"),
        ),
        ("kanban_quit_refused", kanban_quit_refused("disk full")),
        (
            "kanban_could_not_save_copy",
            kanban_could_not_save_copy("disk full"),
        ),
        // Sync status and errors.
        (
            "device_code",
            device_code("ABCD-1234", "https://example.test"),
        ),
        ("browser_failed", browser_failed("https://example.test")),
        ("sign_in_failed", sign_in_failed("access denied")),
        ("tailnet_hosting", tailnet_hosting(8080, "tailscale serve")),
        (
            "tailnet_hosting_ready",
            tailnet_hosting_ready("tailscale serve"),
        ),
        ("shared_key_copied", shared_key_copied(60)),
        (
            "sync_previous_credential",
            sync_previous_credential("denied"),
        ),
        ("sync_nothing_saved", sync_nothing_saved("denied")),
        (
            "sync_key_rotation_failed",
            sync_key_rotation_failed("denied"),
        ),
        ("sync_now_after_error", sync_now_after_error("denied")),
        ("sync_offline", sync_offline("connection refused")),
        ("sync_rate_limited_after", sync_rate_limited_after(1500)),
        ("sync_provider_error", sync_provider_error(500, "inner")),
        (
            "sync_authentication_needed",
            sync_authentication_needed("token expired"),
        ),
        ("sync_permission_denied", sync_permission_denied("none")),
        (
            "sync_oauth_scope_rejected",
            sync_oauth_scope_rejected("drive.file"),
        ),
        (
            "sync_remote_scope_not_found",
            sync_remote_scope_not_found("branch main"),
        ),
        ("sync_remote_changed", sync_remote_changed("moved")),
        ("sync_outcome_unknown", sync_outcome_unknown("dropped")),
        (
            "sync_listing_incomplete",
            sync_listing_incomplete("page 3 of 5"),
        ),
        ("sync_unsupported", sync_unsupported("Data.xlsx", "doc")),
        ("sync_limit_exceeded", sync_limit_exceeded("too big")),
        (
            "sync_credentials_unavailable",
            sync_credentials_unavailable("locked"),
        ),
        ("sync_dirty_buffer", sync_dirty_buffer("Plan.md")),
        ("sync_local_changed", sync_local_changed("Plan.md")),
        ("sync_not_configured", sync_not_configured("missing")),
        ("sync_file_error", sync_file_error("read-only")),
        // Composition.
        ("recovered_for", recovered_for(&recovered_copies(1, 0))),
        ("import_notice", import_notice(2, 1, true)),
        ("import_notice files", import_notice(1, 0, false)),
    ];
    for (helper, text) in outputs {
        assert!(
            is_catalog_copy(&text),
            "{helper} does not render bracketed copy: {text:?}"
        );
        assert_eq!(
            text.matches(PSEUDO_OPEN).count(),
            1,
            "{helper} wraps a fragment separately: {text:?}"
        );
    }
}

// ---- The allowlist -------------------------------------------------------
//
// Everything below is non-copy that the three screens legitimately draw. It is
// deliberately narrow: each rule names what it covers, so a future hardcoded
// sentence has nowhere to hide. Every parameterised helper now renders through
// a `MsgKey` template (`phrase`/`compose`), so it is bracketed like catalog
// copy and needs no row here; the rows that remain are keycaps, Help's input
// tokens, and field/file data.

/// Key names drawn in a keycap. `keycap_text` upper-cases a single letter, so
/// letters are matched by [`is_single_letter`] instead of this list.
const KEYCAPS: &[&str] = &[
    "Alt",
    "Backspace",
    "Ctrl",
    "Del",
    "Delete",
    "Enter",
    "Esc",
    "Menu",
    "Paste",
    "Shift",
    "Space",
    "Tab",
];

/// Mouse/table gestures from Help's key column.
///
/// These are not copy: `shortcut_groups` keeps the whole column as input data
/// (its comment says "the chords are geometry, not copy") and `shortcut_label`
/// parses chords out of these very strings. They are drawn verbatim, never
/// translated.
const HELP_GESTURES: &[&str] = &[
    "Arrows",
    "Board settings",
    "Click",
    "Double-click",
    "Double-click a shape",
    "Drag with a tool",
    "Filter",
    "Right-click",
    "Wheel",
    "click",
    "double-click",
    "pinch",
    "wheel",
];

/// The parenthesised qualifiers in Help's key column, e.g. `↑ / ↓ (search)`.
/// Their tokens are input data for `shortcut_label`, like [`HELP_GESTURES`].
const HELP_CONTEXTS: &[&str] = &[
    "(Kanban)",
    "(Kanban header)",
    "(board)",
    "(card editor)",
    "(editor)",
    "(lane)",
    "(list)",
    "(nothing selected)",
    "(search)",
    "(selection)",
];

/// Default values of the Sync tab's text fields (branch and prefix). Field
/// data, not copy.
const FIELD_DEFAULTS: &[&str] = &["muninn-sync", "vault"];

/// Kanban date/time format and trigger examples. The on-disk format is not
/// translated (docs/i18n.md §2), and these are the field values that spell it.
const FORMAT_EXAMPLES: &[&str] = &["@", "@@", "HH:mm", "YYYY-MM-DD"];

fn is_single_letter(text: &str) -> bool {
    let mut chars = text.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_uppercase())
}

fn is_function_key(text: &str) -> bool {
    text.len() >= 2 && text.starts_with('F') && text[1..].chars().all(|c| c.is_ascii_digit())
}

/// A keyboard chord such as `Ctrl+5`, `Ctrl+Shift+F` or `Shift+click`.
fn is_chord(text: &str) -> bool {
    let parts: Vec<&str> = text.split('+').map(str::trim).collect();
    parts.len() >= 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && (KEYCAPS.contains(part)
                    || HELP_GESTURES.contains(part)
                    || is_single_letter(part)
                    || is_function_key(part)
                    || part.chars().all(|c| !c.is_alphanumeric())
                    || part.chars().all(|c| c.is_ascii_digit()))
        })
}

/// A glyph-prefixed chord such as `⌘K`, `⌘4`, `⇧Z` or `⌘click`.
fn is_modifier_chord(text: &str) -> bool {
    let mut chars = text.chars();
    let mut glyphs = 0;
    while matches!(chars.clone().next(), Some('⌘' | '⇧' | '⌃' | '⌥')) {
        chars.next();
        glyphs += 1;
    }
    // At least one modifier glyph, then ASCII alphanumerics only (`⌘⇧F`, `⌘4`).
    glyphs > 0
        && chars.next().is_some_and(|c| c.is_alphanumeric())
        && chars.all(|c| c.is_alphanumeric())
}

/// A bare count, e.g. the note total or a card number.
fn is_count(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

/// The Help diagnostic block: the startup summary and log tail. Version
/// numbers, the vault path, counts and log lines are data, drawn from the
/// session log rather than the catalog.
fn is_diagnostics(text: &str) -> bool {
    (text.starts_with("Muninn ") && text.contains(" · rune "))
        || (text.starts_with("vault ")
            && text.contains(" notes · ")
            && text.contains(" folders · "))
        || (text.starts_with("log ") && (text.contains('/') || text.contains('\\')))
        || (text.starts_with("20") && text.contains(" startup ") && text.contains(" vault="))
}

/// The one note this test puts in the vault, as title, path and editor body.
fn is_fixture_note(text: &str) -> bool {
    text == "Alpha" || text == "Alpha.md" || text.contains("# Alpha")
}

/// The autosave delay readout such as `0.55 s` (a number and unit).
fn is_delay_readout(text: &str) -> bool {
    text.strip_suffix(" s")
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

/// Why `text` is legitimate non-copy, or `None` if it should have come from
/// the catalog.
fn allowlisted(text: &str) -> Option<&'static str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some("empty text node");
    }
    if FORMAT_EXAMPLES.contains(&trimmed) {
        return Some("Kanban date/time format or trigger example (file format, not copy)");
    }
    if FIELD_DEFAULTS.contains(&trimmed) {
        return Some("Sync field default (field data)");
    }
    if trimmed == PALETTE_QUERY {
        return Some("the query this test typed into the palette (user input)");
    }
    if HELP_CONTEXTS.contains(&trimmed) {
        return Some("Help key-column context token (input data for shortcut_label)");
    }
    if HELP_GESTURES.contains(&trimmed) {
        return Some("Help key-column gesture token (input data for shortcut_label)");
    }
    if KEYCAPS.contains(&trimmed) {
        return Some("keycap key name");
    }
    if is_single_letter(trimmed) {
        return Some("single-letter keycap");
    }
    if is_function_key(trimmed) {
        return Some("function-key keycap");
    }
    if trimmed.chars().all(|c| !c.is_alphanumeric()) {
        return Some("symbol, glyph or punctuation");
    }
    if is_count(trimmed) {
        return Some("count");
    }
    if is_chord(trimmed) {
        return Some("keyboard chord");
    }
    if is_modifier_chord(trimmed) {
        return Some("modifier chord");
    }
    if is_delay_readout(trimmed) {
        return Some("autosave delay readout (number and unit)");
    }
    if is_diagnostics(trimmed) {
        return Some("Help diagnostics: session log, paths, versions and counts (data)");
    }
    if muninn_i18n::Lang::SHIPPED
        .iter()
        .any(|lang| lang.endonym() == trimmed)
    {
        return Some(
            "language endonym: a picker names each language in its own words, never translated",
        );
    }
    if is_fixture_note(trimmed) {
        return Some("the test vault's note title, path or body (file data)");
    }
    if std::path::Path::new(trimmed).starts_with(std::env::temp_dir()) {
        return Some("the test vault's folder path (file data)");
    }
    None
}
