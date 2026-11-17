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
    Key, Lang, PSEUDO_CLOSE, PSEUDO_OPEN, autosave_help, board_canvas, board_name_taken,
    board_quit_refused, card_label, could_not_be_read, could_not_open_vault, could_not_restore,
    could_not_save, deleted, files_skipped, filter_by_tag, import_document_notice, import_progress,
    import_unsupported, link_impact, linked_note_hint, more_edits, no_note_named, note_links,
    notes, referrers, refusing_unsafe_recovery_path, restore_to, scan_cancelled, scan_ms, set_lang,
    show_with_chord, sidebar_summary, status_counts, sync_conflicts, sync_excluded, sync_status, t,
    t_in, targets, theme_label, trash_empty, trash_summary, version,
};
use muninn_ui::{SettingsTab, State, ThemeMode, on_key, poll, view};
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
    let mut state =
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    state.theme = ThemeMode::Dark;
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

/// Part 1 of #191 moved the count, error, sync, board and settings sentences
/// onto `MsgKey` templates, so they render bracketed like catalog copy. This
/// pins one sample per converted family: a helper that falls back to English
/// fails here even if no screen happens to draw it.
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

/// The TODO list stays honest: every deferred sample is still English and
/// still allowed, so a later part knows exactly which row to delete.
#[test]
fn deferred_helpers_are_still_documented() {
    for &(sample, helper) in DEFERRED_HELPERS {
        assert!(
            !is_catalog_copy(sample),
            "{helper} renders bracketed copy; delete its TODO(#191) row"
        );
        assert_eq!(
            allowlisted(sample),
            Some(helper),
            "{helper} lost its documented allowance"
        );
    }
}

// ---- The allowlist -------------------------------------------------------
//
// Everything below is non-copy that the three screens legitimately draw. It is
// deliberately narrow: each rule names what it covers, so a future hardcoded
// sentence has nowhere to hide. A typed helper whose sentence goes through a
// `MsgKey` template is bracketed like catalog copy, so it needs no row here;
// the rows that remain are helpers that part 1 of #191 did not convert yet
// (see `DEFERRED_HELPERS`). If a new helper shows up, add a rule that names it
// rather than a bare string.

/// Helper-output allowances that this part of #191 does **not** convert yet.
///
/// Each entry is a rendered sample and the helper it comes from. They stay
/// allowlisted until their own part converts them; that conversion removes the
/// matching row from [`allowlisted`] and adds the helper to
/// `converted_helpers_render_bracketed` below.
///
/// TODO(#191): convert and delete the row for each entry.
const DEFERRED_HELPERS: &[(&str, &str)] = &[
    ("New note in ⟦Projects⟧", "muninn_i18n::new_note_in"),
    ("New folder in ⟦Projects⟧", "muninn_i18n::new_folder_in"),
];

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

/// `muninn_i18n::new_note_in` / `new_folder_in`: the English frame carries a
/// catalog place name, which is bracketed here.
fn is_new_in(text: &str, prefix: &str) -> bool {
    text.strip_prefix(prefix)
        .is_some_and(|place| place.starts_with(PSEUDO_OPEN) && place.ends_with(PSEUDO_CLOSE))
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
    if is_fixture_note(trimmed) {
        return Some("the test vault's note title, path or body (file data)");
    }
    if is_new_in(trimmed, "New note in ") {
        return Some("muninn_i18n::new_note_in");
    }
    if is_new_in(trimmed, "New folder in ") {
        return Some("muninn_i18n::new_folder_in");
    }
    None
}
