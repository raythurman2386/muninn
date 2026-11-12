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
    assert!(text.contains("--new-instance"), "{text}");
}

/// #42 end to end, before any window: a launch that finds the lock taken
/// hands its request over and exits 0, or exits 1 when nobody answers.
#[cfg(unix)]
mod single_instance {
    use std::fs::{self, File};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use std::process::Command;

    /// A fresh runtime folder in the temp folder (`TMPDIR`). The name is
    /// short so the socket stays beside the lock under macOS's long
    /// per-user temp folder rather than moving to the short fallback.
    fn runtime(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mc-{tag}-{}", std::process::id()));
        let socket = dir.join("muninn/instance.sock");
        assert!(
            socket.as_os_str().len() <= 100,
            "TMPDIR is too long for the socket to stay beside the lock: {}",
            socket.display()
        );
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("muninn")).unwrap();
        dir
    }

    fn launch(runtime: &PathBuf, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_muninn"));
        cmd.args(args)
            .env("XDG_RUNTIME_DIR", runtime)
            .env("XDG_ACTIVATION_TOKEN", "token-42")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("WAYLAND_SOCKET");
        cmd
    }

    #[test]
    fn a_second_launch_hands_over_its_vault_and_token_and_exits_0() {
        let dir = runtime("forward");
        let lock = File::create(dir.join("muninn/instance.lock")).unwrap();
        lock.try_lock().unwrap();
        let listener = UnixListener::bind(dir.join("muninn/instance.sock")).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            stream.write_all(b"ok\n").unwrap();
            line
        });
        let output = launch(&dir, &["--vault", "/tmp/other-vault"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let sent: serde_json::Value = serde_json::from_str(&server.join().unwrap()).unwrap();
        assert_eq!(sent["cmd"], "show");
        assert_eq!(sent["token"], "token-42");
        assert_eq!(sent["vault"], "/tmp/other-vault");
        drop(lock);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_running_app_that_never_answers_makes_the_launch_exit_1() {
        let dir = runtime("silent");
        let lock = File::create(dir.join("muninn/instance.lock")).unwrap();
        lock.try_lock().unwrap();
        fs::write(dir.join("muninn/instance.info"), "777\n").unwrap();
        let output = launch(&dir, &[]).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "stderr {stderr}");
        assert!(stderr.contains("pid 777"), "{stderr}");
        assert!(stderr.contains("--new-instance"), "{stderr}");
        drop(lock);
        let _ = fs::remove_dir_all(&dir);
    }
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

/// GK MF-C (#43): `--version` / `-V` print `muninn <version>` and exit 0
/// without opening a window. Before, both fell through to the GUI. The child
/// is killed after 20 s, so a launch fails the test instead of hanging it.
#[test]
fn version_prints_the_crate_version_and_exits_without_a_window() {
    for flag in ["--version", "-V"] {
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_muninn"))
            .arg(flag)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("WAYLAND_SOCKET")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let start = std::time::Instant::now();
        while child.try_wait().unwrap().is_none() {
            if start.elapsed() > std::time::Duration::from_secs(20) {
                let _ = child.kill();
                panic!("{flag}: still running after 20 s (it launched the app)");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            stdout(&output),
            format!("muninn {}\n", env!("CARGO_PKG_VERSION")),
            "{flag}: stderr {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(0), "{flag}");
        assert!(output.stderr.is_empty(), "{flag}: nothing on stderr");
    }
}
