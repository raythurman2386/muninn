//! Headless coverage for the first-run welcome banner's graph hint and the
//! save-state indicator's clean/error tone distinction.

use std::path::PathBuf;

use muninn_core::testing::TempVault;
use muninn_ui::{ConflictReason, ConflictState, Mode, State, on_key, poll, view};
use rune::{ElementInfo, Env, Runtime};

fn runtime(root: PathBuf, state_home: PathBuf) -> Runtime<State> {
    let mut rt = Runtime::new(
        State::with_state_home(root, state_home),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn note_vault() -> TempVault {
    TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]].\n"),
        ("Beta.md", "# Beta\n\nBack to [[Alpha]].\n"),
    ])
}

fn element<'a>(elements: &'a [ElementInfo], key: &str) -> &'a ElementInfo {
    elements
        .iter()
        .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
        .unwrap_or_else(|| panic!("element {key} missing"))
}

#[test]
fn welcome_banner_previews_links_and_opens_the_graph() {
    let tmp = TempVault::new(&[]);
    let root = tmp.path().join("fresh-vault");
    let mut rt = runtime(root, tmp.path().join("state"));
    assert!(
        rt.state.welcome,
        "a created vault arms the first-run banner"
    );

    let (x, y) = rt.find("welcome-graph-hint").expect("graph hint pill");
    assert!(rt.find("welcome-new-note").is_some());
    assert!(rt.find("welcome-dismiss").is_some());

    let elements = rt.elements();
    assert!(
        elements
            .iter()
            .any(|element| element.text.as_deref() == Some("[[a note]] → graph")),
        "the pill names the link → graph connection"
    );

    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.mode, Mode::Graph, "the pill opens the link graph");
}

#[test]
fn saved_state_is_calm_and_error_state_is_danger_with_retry() {
    let tmp = note_vault();
    let mut rt = runtime(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.load_note(alpha, false);
    rt.state.last_save = Some(1.0);
    rt.invalidate();
    rt.frame(0.0);

    let clean = rt.elements();
    assert!(rt.find("status-state-saved").is_some());
    let clean_text = element(&clean, "status-text");
    let clean_dot = element(&clean, "status-dot");
    assert_eq!(clean_text.text.as_deref(), Some("Saved"));
    assert_ne!(
        clean_text.text_color,
        Some(clean_dot.fill),
        "a clean save keeps the words muted"
    );

    let error = "Could not save: disk is full".to_owned();
    rt.state.editor.insert("x");
    rt.state.error = Some(error.clone());
    rt.invalidate();
    rt.frame(0.1);

    let failed = rt.elements();
    assert!(
        rt.find("status-state-danger").is_some(),
        "a failed save keys the danger tone"
    );
    let failed_text = element(&failed, "status-text");
    let failed_dot = element(&failed, "status-dot");
    assert_eq!(failed_text.text.as_deref(), Some(error.as_str()));
    assert_eq!(
        failed_text.text_color,
        Some(failed_dot.fill),
        "error words and dot share the danger colour"
    );
    assert!(
        rt.find("retry-save").is_some(),
        "a failed save surfaces Retry"
    );
}

#[test]
fn conflict_pending_uses_the_danger_tone_and_explicit_actions() {
    let tmp = note_vault();
    let mut rt = runtime(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.load_note(alpha, false);
    rt.state.conflict = Some(ConflictState {
        note_id: alpha,
        expected: None,
        observed: None,
        reason: ConflictReason::Modified,
    });
    rt.invalidate();
    rt.frame(0.0);

    let elements = rt.elements();
    let text = element(&elements, "conflict-text");
    let dot = element(&elements, "conflict-dot");
    assert_eq!(text.text.as_deref(), Some("External change detected"));
    assert_eq!(
        text.text_color,
        Some(dot.fill),
        "a pending conflict is danger-coloured in both dot and words"
    );
    assert!(rt.find("conflict-keep-mine").is_some());
    assert!(rt.find("conflict-load-theirs").is_some());
    assert!(rt.find("conflict-save-copy").is_some());
}
