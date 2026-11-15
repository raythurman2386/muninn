//! #155: the command palette draws its copy from the catalog.
//!
//! Extraction guard, not a translation test: the dialog label, the section
//! headings, the empty state and the footer still render exactly the English
//! they did before the move. The keyword search data is checked in the crate
//! (`overlay::tests`), where `PaletteAction::keywords` and `matches` live.

use muninn_core::testing::TempVault;
use muninn_i18n::{Key, t};
use muninn_ui::{State, ThemeMode, on_key, poll, view};
use rune::{Env, Runtime};

fn palette() -> (TempVault, Runtime<State>) {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut state =
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    state.theme = ThemeMode::Dark;
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 1.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt.update(|s, _| s.open_palette());
    for _ in 0..6 {
        rt.frame(1.0 / 60.0);
    }
    (tmp, rt)
}

#[test]
fn the_palette_card_and_footer_use_their_catalog_copy() {
    let (_tmp, rt) = palette();
    let elements = rt.elements();

    let card = elements
        .iter()
        .find(|element| element.key.as_deref() == Some("palette"))
        .expect("the palette card is mounted");
    assert_eq!(
        card.label.as_deref(),
        Some(t(Key::PaletteTitle)),
        "the dialog label does not come from the catalog"
    );
    assert_eq!(t(Key::PaletteTitle), "Command palette");

    // The empty query still lists the action group (and the recent note's
    // group), so their uppercase headings must be the catalog text.
    for key in [Key::PaletteSectionActions, Key::PaletteSectionRecent] {
        let heading = t(key).to_uppercase();
        assert!(
            elements
                .iter()
                .filter_map(|element| element.text.as_deref())
                .any(|text| text == heading),
            "missing section heading {heading:?}"
        );
    }

    let footer = t(Key::PaletteFooter);
    assert_eq!(footer, "↑↓ move · Enter open · Esc close");
    assert!(
        elements
            .iter()
            .filter_map(|element| element.text.as_deref())
            .any(|text| text == footer),
        "the footer does not come from the catalog"
    );
}

#[test]
fn the_empty_state_and_its_list_label_use_the_catalog_copy() {
    let (_tmp, mut rt) = palette();
    rt.update(|s, _| s.palette_field.replace_text("zzzzzz"));
    for _ in 0..6 {
        rt.frame(1.0 / 60.0);
    }
    let elements = rt.elements();
    let empty = t(Key::PaletteEmpty);
    assert_eq!(empty, "No notes or actions match");

    let results = elements
        .iter()
        .find(|element| element.key.as_deref() == Some("palette-scroll"))
        .expect("the results list is mounted");
    assert_eq!(
        results.label.as_deref(),
        Some(empty),
        "the empty list keeps its accessible label"
    );
    assert!(
        elements
            .iter()
            .filter_map(|element| element.text.as_deref())
            .any(|text| text == empty),
        "the empty-state row does not come from the catalog"
    );
}
