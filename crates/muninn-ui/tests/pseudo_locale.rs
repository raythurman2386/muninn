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
use muninn_i18n::{Key, Lang, PSEUDO_CLOSE, PSEUDO_OPEN, set_lang, t, t_in};
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
        "scan 9 ms",
        "log C:\\Users\\runner\\AppData\\Local\\Temp\\muninn\\session.log",
        "",
    ]);
    assert_eq!(
        flagged,
        ["Close", "This sentence never went through the catalog."],
        "the scan must report unbracketed copy and keep allowlisted non-copy"
    );
}

// ---- The allowlist -------------------------------------------------------
//
// Everything below is non-copy that the three screens legitimately draw. It is
// deliberately narrow: each rule names what it covers, so a future hardcoded
// sentence has nowhere to hide. Typed helpers from `muninn-i18n` build their
// sentences from parameters and are not covered by the pseudo catalog; each
// one that appears here is named on its rule. If a new helper shows up, add a
// rule that names it rather than a bare string.

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

/// `"1 note"` / `"2 notes"`, the shape `muninn-i18n`'s private `counted` makes.
fn is_counted(text: &str, one: &str, many: &str) -> bool {
    text.split_once(' ')
        .is_some_and(|(n, noun)| is_count(n) && (noun == one || noun == many))
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

/// The autosave delay readout (`"0.55 s"`), a number and its unit.
/// `muninn_i18n::scan_ms`: `"scan 9 ms"` (the scan duration changes by
/// machine and platform, so the rule matches the shape, not one value).
fn is_scan_ms(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("scan ") else {
        return false;
    };
    let Some(ms) = rest.strip_suffix(" ms") else {
        return false;
    };
    !ms.is_empty() && ms.chars().all(|c| c.is_ascii_digit())
}

/// The autosave-delay readout such as `0.55 s` (a number and unit).
fn is_delay_readout(text: &str) -> bool {
    text.strip_suffix(" s")
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

/// `muninn_i18n::version`: `"Version 0.8.5."`.
fn is_version(text: &str) -> bool {
    text.strip_prefix("Version ")
        .and_then(|rest| rest.strip_suffix('.'))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

/// `muninn_i18n::autosave_help`: `"Autosave writes 0.55 s after you stop
/// typing."`.
fn is_autosave_help(text: &str) -> bool {
    text.strip_prefix("Autosave writes ")
        .and_then(|rest| rest.strip_suffix(" s after you stop typing."))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

/// `muninn_i18n::new_note_in` / `new_folder_in`: the English frame carries a
/// catalog place name, which is bracketed here.
fn is_new_in(text: &str, prefix: &str) -> bool {
    text.strip_prefix(prefix)
        .is_some_and(|place| place.starts_with(PSEUDO_OPEN) && place.ends_with(PSEUDO_CLOSE))
}

/// `muninn_i18n::theme_label`: the theme name is catalog copy; the sentence
/// around it is the helper's fixed English.
fn is_theme_label(text: &str) -> bool {
    const SUFFIX: &str = " — click to cycle (System, Dark, Light)";
    text.strip_prefix("Theme: ")
        .and_then(|rest| rest.strip_suffix(SUFFIX))
        .is_some_and(|theme| theme.starts_with(PSEUDO_OPEN) && theme.ends_with(PSEUDO_CLOSE))
}

/// `muninn_i18n::show_with_chord`: `"Show Muninn with Ctrl+Alt+M"`.
fn is_show_with_chord(text: &str) -> bool {
    text.strip_prefix("Show Muninn with ")
        .is_some_and(|chord| is_chord(chord) || is_modifier_chord(chord))
}

/// `muninn_i18n::sidebar_summary`: `"1 note · 0 links"`.
fn is_sidebar_summary(text: &str) -> bool {
    let parts: Vec<&str> = text.split(" · ").collect();
    parts.len() == 2
        && is_counted(parts[0], "note", "notes")
        && is_counted(parts[1], "link", "links")
}

/// `muninn_i18n::status_counts`: `"2 words · 0 links · 0 backlinks"`.
fn is_status_counts(text: &str) -> bool {
    let parts: Vec<&str> = text.split(" · ").collect();
    parts.len() == 3
        && is_counted(parts[0], "word", "words")
        && is_counted(parts[1], "link", "links")
        && is_counted(parts[2], "backlink", "backlinks")
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
    if is_scan_ms(trimmed) {
        return Some("muninn_i18n::scan_ms");
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
    if trimmed == "Trash is empty" {
        return Some("muninn_i18n::trash_empty");
    }
    if trimmed == "no links" {
        return Some("muninn_i18n::note_links(0, 0)");
    }
    if is_version(trimmed) {
        return Some("muninn_i18n::version");
    }
    if is_autosave_help(trimmed) {
        return Some("muninn_i18n::autosave_help");
    }
    if is_new_in(trimmed, "New note in ") {
        return Some("muninn_i18n::new_note_in");
    }
    if is_new_in(trimmed, "New folder in ") {
        return Some("muninn_i18n::new_folder_in");
    }
    if is_theme_label(trimmed) {
        return Some("muninn_i18n::theme_label");
    }
    if is_show_with_chord(trimmed) {
        return Some("muninn_i18n::show_with_chord");
    }
    if is_sidebar_summary(trimmed) {
        return Some("muninn_i18n::sidebar_summary");
    }
    if is_status_counts(trimmed) {
        return Some("muninn_i18n::status_counts");
    }
    if is_counted(trimmed, "result", "results") {
        return Some("muninn_i18n::results");
    }
    None
}
