//! End-to-end tests for `muninn --check` and `--list-links`, using the real
//! binary in a headless process (no window, no GPU context).

use std::path::Path;
use std::process::Output;

use muninn_core::testing::TempVault;

fn run(args: &[&str]) -> Output {
    let binary = env!("CARGO_BIN_EXE_muninn");
    std::process::Command::new(binary)
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn check(path: &Path) -> Output {
    let path = path.to_str().expect("utf8 path");
    run(&["--check", path])
}

/// Run `--check` on a fresh fixture and return `(exit code, stdout)`.
fn check_files(files: &[(&str, &str)]) -> (i32, String) {
    let tmp = TempVault::new(files);
    let output = check(tmp.path());
    (output.status.code().unwrap_or(-1), stdout(&output))
}

#[test]
fn check_exit_one_reports_broken_links_and_duplicates() {
    let (code, text) = check_files(&[
        ("Home.md", "[[Missing]]"),
        ("a/Index.md", "a"),
        ("b/Index.md", "b"),
    ]);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("broken links: 1"), "{text}");
    assert!(text.contains("Home.md -> Missing"), "{text}");
    assert!(text.contains("duplicates: 1"), "{text}");
    assert!(text.contains("Index (2 notes)"), "{text}");
}

#[test]
fn check_exit_zero_for_a_clean_vault() {
    let (code, text) = check_files(&[("Alpha.md", "[[Beta]]"), ("Beta.md", "back to [[Alpha]]")]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("broken links: 0"));
}

#[test]
fn check_exit_two_for_a_missing_vault_and_creates_nothing() {
    let tmp = TempVault::new(&[("Alpha.md", "one note")]);
    let missing = tmp.path().join("not-here");
    let output = check(&missing);
    assert_eq!(output.status.code(), Some(2), "{}", stdout(&output));
    assert!(!missing.exists(), "--check must not create a vault");
    assert!(stdout(&output).contains("not a readable vault folder"));
}

#[test]
fn list_links_prints_sorted_referrer_target_lines() {
    let tmp = TempVault::new(&[
        ("Home.md", "[[Beta]] and [[Missing]]"),
        ("Beta.md", "[[Home]]"),
    ]);
    let path = tmp.path().to_str().expect("utf8 path");
    let output = run(&["--list-links", path]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        "Beta.md -> Home.md\nHome.md -> Beta.md\nHome.md -> missing:Missing\n"
    );
}

#[test]
fn help_names_the_new_commands() {
    let output = run(&["--help"]);
    let text = stdout(&output);
    assert_eq!(output.status.code(), Some(0));
    assert!(text.contains("--check"), "{text}");
    assert!(text.contains("--list-links"), "{text}");
    assert!(text.contains("--trace"), "{text}");
}

#[test]
fn a_missing_check_path_prints_our_usage() {
    let output = run(&["--check"]);
    let text = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code(), Some(2));
    assert!(text.contains("--check"), "{text}");
    assert!(text.contains("--list-links"), "{text}");
    assert!(text.contains("--trace"), "{text}");
}
