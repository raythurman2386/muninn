# Contributing to Muninn

Thanks for helping. This project is small enough that a focused change and a short explanation
are more valuable than a large refactor.

## Setup

1. Install Rust 1.99.1. `rustup` reads `rust-toolchain.toml` automatically.
2. On Linux, install the windowing headers listed in the README.
3. Build and run the sample vault:

   ```sh
   cargo run -p muninn-app -- --vault vault
   ```

4. Run the full check before opening a pull request:

   ```sh
   cargo run -p xtask -- check
   ```

## WASM (core and graph only)

`muninn-core` and `muninn-graph` are held to `wasm32-unknown-unknown` so a browser front end
can reuse them later. The UI, app and sync crates are native-only for now (filesystem, clock,
threads, blocking HTTP), so this guard covers only the portable two:

```sh
rustup target add wasm32-unknown-unknown --toolchain 1.99.1
cargo run -p xtask -- wasm-check
```

That runs `cargo check` and `cargo clippy -- -D warnings` for both crates on the wasm target.
CI runs the same command in the `wasm32 check (core, graph)` job.

## Working on Rune itself

Muninn depends on an unpublished Rune revision. To use a local checkout:

```sh
./scripts/use-local-rune.sh
# or
RUNE_DIR=/path/to/rune ./scripts/use-local-rune.sh
```

This writes a gitignored `.cargo/config.toml`. Remove it (or run the script again with the
desired path) to switch back to the pinned revision.

## Where changes go

- Vault parsing, indexing, search, preview rewriting → `crates/muninn-core`
- Layout or camera math → `crates/muninn-graph`
- State, shortcuts, widgets, screens → `crates/muninn-ui`
- CLI arguments, scenes, `rune::run` wiring → `crates/muninn-app`
- Build, CI and packaging helpers → `xtask`

If you add a workspace crate or dependency edge, update the allowed graph in
`xtask/src/main.rs`. `cargo run -p xtask -- layers` will fail until you do.

## Tests

- Pure logic gets a unit test next to it.
- UI changes should have a headless runtime test in `crates/muninn-ui/tests/ui.rs` when the
  behaviour can be driven by keys or pointer events.
- New graph behaviour should keep the deterministic layout tests passing.
- `scripts/ui-smoke.sh` launches the real app headless (Rune offscreen tooling, then a windowed
  run on a private Xvfb display), switches modes, screenshots each step and checks vault files
  afterwards. Usage and requirements are in its header; output lands in `target/ui-smoke`.
- `scripts/ci/apt-install.sh PACKAGE...` installs Linux build dependencies with a bounded retry
  (three attempts and a short backoff by default) around both `apt-get update` and
  `apt-get install --fix-missing`, so one flaky mirror request cannot fail a build. It never calls
  `sudo`; the release workflow wraps it. `scripts/tests/apt-retry.sh` checks the retry and failure
  paths offline with a fake `apt-get`. The release build matrix runs with `fail-fast: false`, and
  the release job waits for all four builds. Re-running a tag is safe: if `gh release view` finds
  the release, the workflow uploads the same full asset set with `--clobber` instead of trying to
  create it again; a persistent failure exits non-zero after the bounded retries.
- Snapshot images are proof for a PR, not committed fixtures. Use
  `cargo run -p xtask -- snapshots` and attach what changed.

## Pull requests

- One topic per PR; link the issue when there is one.
- Add a test that fails without the change.
- Update `CHANGELOG.md` under `## [Unreleased]` for user-visible changes.
- Update the README when a shortcut, flag or file rule changes.
- Keep secrets out of code, logs and committed files.

## Style

- `rustfmt` and `clippy -D warnings` are the baseline.
- Prefer small modules and plain data over trait hierarchies.
- Keep `muninn-core` and `muninn-graph` free of Rune types.
- Comments should explain why something is done, especially layout constants and parsing edge
  cases.

## Reporting bugs

Use the issue templates. A useful report includes the Muninn version or commit, the operating
system, the smallest vault that reproduces the problem, and the exact steps. Never paste private
note contents; reduce the fixture first.
