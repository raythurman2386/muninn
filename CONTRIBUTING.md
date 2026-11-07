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
