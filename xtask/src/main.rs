//! Workspace tasks for Muninn.
//!
//! Keeping fmt/clippy/test/layering/copy checks in one place means CI and contributors
//! run the same commands, and crate-boundary rules live next to the crates they describe.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod i18n_lint;

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("help");
    match dispatch(command, &args[1..]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(command: &str, args: &[String]) -> Result<()> {
    match command {
        "fmt" => run_cargo(&["fmt", "--all"]),
        "fmt-check" => run_cargo(&["fmt", "--all", "--", "--check"]),
        "clippy" => run_cargo(&[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ]),
        "test" => run_cargo(&["test", "--workspace", "--all-targets"]),
        "build" => run_cargo(&["build", "--workspace"]),
        "check" | "all" => {
            run_cargo(&["fmt", "--all", "--", "--check"])?;
            run_cargo(&[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ])?;
            run_cargo(&["test", "--workspace", "--all-targets"])?;
            check_layers()?;
            i18n_lint::run()?;
            println!("all checks passed");
            Ok(())
        }
        "layers" | "check-layers" => check_layers(),
        "i18n-lint" => i18n_lint::run(),
        "wasm-check" => wasm_check(),
        "secret-canary" => secret_canary(),
        "bundle" => bundle(),
        "snapshots" => snapshots(args),
        "run" => {
            let mut run = Command::new("cargo");
            run.args(["run", "-p", "muninn-app", "--"]);
            run.args(args);
            status(run)
        }
        "help" | "--help" | "-h" => {
            println!(
                "xtask: fmt | fmt-check | clippy | test | build | check | layers | i18n-lint | wasm-check | secret-canary | bundle | snapshots | run [args]"
            );
            Ok(())
        }
        other => Err(format!("unknown task {other}").into()),
    }
}

fn run_cargo(args: &[&str]) -> Result<()> {
    let mut command = Command::new("cargo");
    command.args(args);
    status(command)
}

fn status(mut command: Command) -> Result<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("command failed with {status}").into())
    }
}

pub(crate) fn workspace_root() -> Result<PathBuf> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("workspace root not found")?
        .to_path_buf())
}

fn metadata() -> Result<serde_json::Value> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(workspace_root()?)
        .output()?;
    if !output.status.success() {
        return Err("cargo metadata failed".into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn check_layers() -> Result<()> {
    let metadata = metadata()?;
    let allowed = allowed_edges();
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or("metadata packages missing")?;
    let mut problems = Vec::new();
    for package in packages {
        let Some(name) = package.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(allowed_for_crate) = allowed.get(name) else {
            continue;
        };
        let Some(dependencies) = package
            .get("dependencies")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        for dependency in dependencies {
            let Some(dep_name) = dependency.get("name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if !dep_name.starts_with("muninn-") {
                continue;
            }
            if !allowed_for_crate.contains(dep_name) {
                problems.push(format!(
                    "{name} must not depend on {dep_name} (layering violation)"
                ));
            }
        }
    }
    if problems.is_empty() {
        println!("layering ok ({} workspace crates)", packages.len());
        Ok(())
    } else {
        for problem in &problems {
            eprintln!("{problem}");
        }
        Err(format!("{} layering violation(s)", problems.len()).into())
    }
}

fn allowed_edges() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    let mut allowed: BTreeMap<&'static str, BTreeSet<&'static str>> = BTreeMap::new();
    // The catalog is leaf data: English copy, typed helpers and date names. It
    // has no dependencies, so core, ui and app can all point at it (docs/i18n.md
    // §5.4). Sync, graph and xtask stay off it.
    allowed.insert("muninn-i18n", BTreeSet::new());
    allowed.insert("muninn-core", BTreeSet::from(["muninn-i18n"]));
    allowed.insert("muninn-graph", BTreeSet::from(["muninn-core"]));
    // Import conversion is pure: anydoc in, Markdown and images out. It never
    // reads the vault or the UI, so it is a leaf like i18n and sync.
    allowed.insert("muninn-import", BTreeSet::new());
    // Sync is UI-free. It does not depend on the vault index: the file bytes
    // are the source of truth, and the UI coordinates editor/watcher state.
    // Slice 8a adds exactly one edge, to core's `StateHome`: sync's small host
    // state (base, journal, prepared, sha cache, staged bytes) goes through
    // that seam so a browser build can supply OPFS. Core stays leafward of
    // sync; no vault/UI dependency is introduced.
    allowed.insert("muninn-sync", BTreeSet::from(["muninn-core"]));
    allowed.insert(
        "muninn-ui",
        BTreeSet::from([
            "muninn-core",
            "muninn-graph",
            "muninn-import",
            "muninn-sync",
            "muninn-i18n",
        ]),
    );
    allowed.insert(
        "muninn-app",
        BTreeSet::from([
            "muninn-core",
            "muninn-graph",
            "muninn-ui",
            "muninn-sync",
            "muninn-i18n",
        ]),
    );
    // The browser demo: the shared UI (no native features) over the sample
    // vault's bytes. Like the app it sits at the top; it adds no edge any
    // other front end could not have.
    allowed.insert(
        "muninn-web",
        BTreeSet::from(["muninn-core", "muninn-i18n", "muninn-ui"]),
    );
    allowed.insert("xtask", BTreeSet::new());
    allowed
}

/// The one target the portable crates are held to.
const WASM_TARGET: &str = "wasm32-unknown-unknown";

/// `xtask wasm-check`: keep the portable crates, the gated UI and the web
/// crate building for the browser.
///
/// `muninn-core`, `muninn-graph`, `muninn-i18n` and a no-native `muninn-ui`
/// all compile for `wasm32-unknown-unknown`; `muninn-sync` is checked with its
/// `native` feature off, the configuration a web build uses. The UI's
/// `native` feature is the desktop-only boundary (filesystem, clock, threads,
/// tray, sync transports), so `--no-default-features` is the browser crate.
/// `muninn-web` is the demo entry point: all of it is wasm-only, so its
/// `check`/`clippy` need no feature juggling. This is a compile-only guard;
/// it builds no bundle and runs nothing in a browser.
fn wasm_check() -> Result<()> {
    check_wasm_target()?;
    run_cargo(&[
        "check",
        "--locked",
        "-p",
        "muninn-core",
        "-p",
        "muninn-graph",
        "-p",
        "muninn-i18n",
        "--target",
        WASM_TARGET,
    ])?;
    run_cargo(&[
        "clippy",
        "--locked",
        "-p",
        "muninn-core",
        "-p",
        "muninn-graph",
        "-p",
        "muninn-i18n",
        "--target",
        WASM_TARGET,
        "--",
        "-D",
        "warnings",
    ])?;
    run_cargo(&[
        "check",
        "--locked",
        "-p",
        "muninn-sync",
        "--no-default-features",
        "--target",
        WASM_TARGET,
    ])?;
    run_cargo(&[
        "clippy",
        "--locked",
        "-p",
        "muninn-sync",
        "--no-default-features",
        "--target",
        WASM_TARGET,
        "--",
        "-D",
        "warnings",
    ])?;
    run_cargo(&[
        "check",
        "--locked",
        "-p",
        "muninn-ui",
        "--no-default-features",
        "--target",
        WASM_TARGET,
    ])?;
    run_cargo(&[
        "clippy",
        "--locked",
        "-p",
        "muninn-ui",
        "--no-default-features",
        "--target",
        WASM_TARGET,
        "--",
        "-D",
        "warnings",
    ])?;
    // The demo crate is the browser binary: muninn-ui with no native features,
    // the sample vault and the wasm entry. Nothing in it is native-only.
    run_cargo(&[
        "check",
        "--locked",
        "-p",
        "muninn-web",
        "--target",
        WASM_TARGET,
    ])?;
    run_cargo(&[
        "clippy",
        "--locked",
        "-p",
        "muninn-web",
        "--target",
        WASM_TARGET,
        "--",
        "-D",
        "warnings",
    ])
}

/// Name the missing `rustup` step instead of letting cargo report a missing
/// `core` crate halfway through a dependency build.
fn check_wasm_target() -> Result<()> {
    let output = Command::new("rustc")
        .args(["--print", "target-libdir", "--target", WASM_TARGET])
        .output()?;
    let libdir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let installed = output.status.success()
        && !libdir.is_empty()
        && std::fs::read_dir(&libdir).is_ok_and(|entries| {
            entries
                .filter_map(std::result::Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().starts_with("libstd-"))
        });
    if installed {
        return Ok(());
    }
    let hint =
        format!("the {WASM_TARGET} target is not installed; run `rustup target add {WASM_TARGET}`");
    Err(hint.into())
}

/// The build-time env var and the sentinel the canary plants in it.
const SECRET_ENV: &str = "MUNINN_GOOGLE_CLIENT_SECRET";
const CANARY: &str = "wasm-canary";

/// `xtask secret-canary`: prove the Google Desktop secret cannot reach a wasm
/// artifact, and that native builds still bake it in.
///
/// The wasm half builds the browser crates with `MUNINN_GOOGLE_CLIENT_SECRET`
/// set to a sentinel, then scans every file cargo produced under the wasm
/// target directory. A hit names the offending file and fails. The native half
/// builds with the same sentinel and asserts it survives into the crate's
/// `rlib`, so a change that drops the secret from native builds fails here too
/// rather than only at runtime.
fn secret_canary() -> Result<()> {
    check_wasm_target()?;
    let target_dir = target_dir()?;

    run_cargo_env(
        &[
            "build",
            "--locked",
            "--target",
            WASM_TARGET,
            "-p",
            "muninn-core",
            "-p",
            "muninn-graph",
            "-p",
            "muninn-sync",
            "--no-default-features",
        ],
        SECRET_ENV,
        CANARY,
    )?;

    let wasm_dir = target_dir.join(WASM_TARGET);
    let scanned = scan_for_bytes(&wasm_dir, CANARY.as_bytes())?;
    if !scanned.hits.is_empty() {
        for hit in &scanned.hits {
            eprintln!("secret-canary: `{CANARY}` found in {}", hit.display());
        }
        return Err(format!(
            "{} of {} wasm artifact file(s) contain the sentinel",
            scanned.hits.len(),
            scanned.files
        )
        .into());
    }
    println!(
        "secret-canary: wasm ok ({} file(s) under {} contain no `{CANARY}`)",
        scanned.files,
        wasm_dir.display()
    );

    run_cargo_env(
        &["build", "--locked", "-p", "muninn-sync"],
        SECRET_ENV,
        CANARY,
    )?;

    let deps = target_dir.join("debug").join("deps");
    let mut rlibs = Vec::new();
    for entry in std::fs::read_dir(&deps)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.starts_with("libmuninn_sync") && name.ends_with(".rlib") {
            rlibs.push(path);
        }
    }
    if rlibs.is_empty() {
        return Err(format!(
            "no libmuninn_sync*.rlib under {}; the native build produced no artifact to check",
            deps.display()
        )
        .into());
    }
    let mut bundled = 0;
    for rlib in &rlibs {
        if contains_bytes(rlib, CANARY.as_bytes())? {
            bundled += 1;
        }
    }
    if bundled == 0 {
        return Err(format!(
            "none of the {} libmuninn_sync*.rlib file(s) under {} contain `{CANARY}`; the native build no longer bundles the secret",
            rlibs.len(),
            deps.display()
        )
        .into());
    }
    println!(
        "secret-canary: native ok ({bundled} of {} libmuninn_sync*.rlib file(s) contain `{CANARY}`)",
        rlibs.len()
    );
    Ok(())
}

/// Cargo's real target directory, so `CARGO_TARGET_DIR` and config overrides
/// point the canary at the artifacts that were just written.
fn target_dir() -> Result<PathBuf> {
    let metadata = metadata()?;
    let dir = metadata
        .get("target_directory")
        .and_then(serde_json::Value::as_str)
        .ok_or("metadata target_directory missing")?;
    Ok(PathBuf::from(dir))
}

fn run_cargo_env(args: &[&str], key: &str, value: &str) -> Result<()> {
    let mut command = Command::new("cargo");
    command
        .args(args)
        .current_dir(workspace_root()?)
        .env(key, value);
    status(command)
}

/// Every regular file under `root`, and which of them contain `needle`.
struct Scan {
    files: usize,
    hits: Vec<PathBuf>,
}

fn scan_for_bytes(root: &Path, needle: &[u8]) -> Result<Scan> {
    let mut scan = Scan {
        files: 0,
        hits: Vec::new(),
    };
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() {
                scan.files += 1;
                if contains_bytes(&path, needle)? {
                    scan.hits.push(path);
                }
            }
        }
    }
    Ok(scan)
}

/// Whether a file contains `needle`, read in chunks so a large `.rlib` never
/// has to fit in memory at once.
fn contains_bytes(path: &Path, needle: &[u8]) -> Result<bool> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut chunk = vec![0u8; 64 * 1024];
    let mut carry = Vec::new();
    loop {
        let read = file.read(&mut chunk)?;
        if read == 0 {
            return Ok(false);
        }
        let mut window = Vec::with_capacity(carry.len() + read);
        window.extend_from_slice(&carry);
        window.extend_from_slice(&chunk[..read]);
        if window.windows(needle.len()).any(|slice| slice == needle) {
            return Ok(true);
        }
        let keep = needle.len().saturating_sub(1).min(window.len());
        carry = window[window.len() - keep..].to_vec();
    }
}

fn bundle() -> Result<()> {
    run_cargo(&["build", "--release", "-p", "muninn-app"])?;
    let root = workspace_root()?;
    let binary = root.join("target").join("release").join(binary_name());
    if binary.exists() {
        println!("bundled binary: {}", binary.display());
        Ok(())
    } else {
        Err(format!("expected binary at {}", binary.display()).into())
    }
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "muninn.exe"
    } else {
        "muninn"
    }
}

/// Render the sample vault to `target/snapshots` for visual review.
fn snapshots(args: &[String]) -> Result<()> {
    run_cargo(&["build", "-p", "muninn-app"])?;
    let root = workspace_root()?;
    let binary = root.join("target").join("debug").join(binary_name());
    let out = root.join("target").join("snapshots");
    std::fs::create_dir_all(&out)?;
    let vault = root.join("vault");
    let scenes = if args.is_empty() {
        vec!["notes", "preview", "graph", "local", "light", "empty"]
    } else {
        args.iter().map(String::as_str).collect()
    };
    for scene in scenes {
        let image = out.join(format!("{scene}.png"));
        let mut command = Command::new(&binary);
        command
            .arg("--vault")
            .arg(&vault)
            .arg("--snap")
            .arg(&image)
            .arg("--scene")
            .arg(scene);
        status(command)?;
        println!("wrote {}", image.display());
    }
    Ok(())
}
