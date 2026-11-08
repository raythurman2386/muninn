//! Argument-parsing tests for the headless CLI. End-to-end exit codes and
//! output live in `tests/cli.rs`, which runs the real binary.

use std::path::PathBuf;

use super::*;

#[test]
fn cli_parses_check_list_links_help_and_trace() {
    match cli(&["--check".to_owned(), "/tmp/v".to_owned()]) {
        Cli::Check(path) => assert_eq!(path, PathBuf::from("/tmp/v")),
        _ => panic!("--check should select the check command"),
    }
    match cli(&["--list-links=/tmp/v".to_owned()]) {
        Cli::ListLinks(path) => assert_eq!(path, PathBuf::from("/tmp/v")),
        _ => panic!("--list-links= should select the list command"),
    }
    assert!(matches!(cli(&["--check".to_owned()]), Cli::Error(_)));
    assert!(matches!(cli(&["--trace".to_owned()]), Cli::Run(None)));
    assert!(matches!(
        cli(&["--theme".to_owned(), "light".to_owned()]),
        Cli::Run(Some(ThemeMode::Light))
    ));
    assert!(matches!(cli(&["--theme=wobble".to_owned()]), Cli::Error(_)));
    assert!(has_flag(
        &["--vault".to_owned(), "/v".to_owned(), "--trace".to_owned()],
        "--trace"
    ));
}
