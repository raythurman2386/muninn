//! Workspace tasks for Muninn.
//!
//! Keeping fmt/clippy/test/layering in one place means CI and contributors run the same
//! commands, and crate-boundary rules live next to the crates they describe.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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
            println!("all checks passed");
            Ok(())
        }
        "layers" | "check-layers" => check_layers(),
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
                "xtask: fmt | fmt-check | clippy | test | build | check | layers | bundle | snapshots | run [args]"
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

fn workspace_root() -> Result<PathBuf> {
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
    allowed.insert("muninn-core", BTreeSet::new());
    allowed.insert("muninn-graph", BTreeSet::from(["muninn-core"]));
    // Sync is UI-free. It does not depend on the vault index: the file bytes
    // are the source of truth, and the UI coordinates editor/watcher state.
    allowed.insert("muninn-sync", BTreeSet::new());
    allowed.insert(
        "muninn-ui",
        BTreeSet::from(["muninn-core", "muninn-graph", "muninn-sync"]),
    );
    allowed.insert(
        "muninn-app",
        BTreeSet::from(["muninn-core", "muninn-graph", "muninn-ui", "muninn-sync"]),
    );
    allowed.insert("xtask", BTreeSet::new());
    allowed
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
