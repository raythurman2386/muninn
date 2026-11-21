# Fuzz harness

`cargo-fuzz` targets for the parsers that read bytes the app did not write:

| Target      | Parser                                                        |
| ----------- | ------------------------------------------------------------- |
| `wikilink`  | `muninn_core::{parse_links, headings, block_anchors, unclosed_link}` |
| `note_lock` | the note-lock envelope frame (`MUNINN-NOTE-LOCK v2`)          |
| `sync_json` | `journal.json` and the Drive commit manifest                   |

This crate is excluded from the root workspace (`exclude = ["fuzz"]`): it needs
nightly and its own lockfile. It is not part of the app build or the PR matrix.

## Requirements

- a Rust **nightly** toolchain (`cargo +nightly`);
- `cargo-fuzz` (`cargo install cargo-fuzz`, or `taiki-e/install-action`);
- a C toolchain with `clang` and `llvm` (libFuzzer).

The repository pins a stable toolchain in `rust-toolchain.toml`; always pass
`+nightly` so the fuzz build does not fall back to it. On Linux hosts where
cargo-fuzz selects the static musl target, pass
`--target x86_64-unknown-linux-gnu`: ASAN cannot link against a statically
linked libc. The scheduled workflow pins that target explicitly.

## Run

From the repository root:

```sh
cargo +nightly fuzz run wikilink -- -runs=10000
cargo +nightly fuzz run note_lock -- -runs=10000
cargo +nightly fuzz run sync_json -- -runs=10000
```

Interactive/default libFuzzer run (until stopped):

```sh
cargo +nightly fuzz run note_lock -- -rss_limit_mb=4096
```

Build all targets without running them:

```sh
cargo +nightly fuzz build
```

## Feature-gated seams

The product APIs take a password or a state directory, so each target needs a
narrow seam. Both seams are behind an off-by-default `fuzzing` feature:

- `muninn-core`: `features = ["fuzzing"]` exposes
  `note_lock::parse_for_fuzzing`, the private structure check behind `unlock`
  with no Argon2id key derivation. `unlock`/`lock` themselves are not fuzzed:
  one Argon2id run per input is too slow.
- `muninn-sync`: `features = ["fuzzing"]` exposes
  `parse_journal_for_fuzzing` (the parser `Journal::load` now calls) and
  `parse_commit_for_fuzzing` (the private Drive commit parser).

Neither feature is enabled by the app or by any default feature set, and the
seams only forward to the existing parsers, so product behaviour is unchanged.

## Corpus

`fuzz/corpus/<target>/` holds a small offline seed corpus: wikilink/Markdown
samples, a valid and a truncated v2 envelope, a valid journal and a valid
commit manifest. libFuzzer also writes new interesting inputs there; keep the
small, distinct ones and commit them (they are plain files in this directory).
`fuzz/artifacts/<target>/` keeps crashing inputs and is git-ignored.

## Crashes

A crash prints a reproducer and writes the input to
`fuzz/artifacts/<target>/crash-<hash>`. Reproduce with:

```sh
cargo +nightly fuzz run <target> fuzz/artifacts/<target>/crash-<hash>
```

and minimize with `cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/crash-<hash>`.
The scheduled workflow (`.github/workflows/fuzz.yml`, Mondays 04:00 UTC, plus
manual dispatch) runs each target for 120 s and uploads `fuzz/artifacts` on
failure.
