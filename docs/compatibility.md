# Compatibility contract

This page says which on-disk and interface contracts Muninn promises to keep, what is still
provisional before 1.0, and how a format, key or flag is retired. It is a contract document, not
a tutorial: for install steps, sync setup and the format grammars themselves, follow the linked
guides. Every promise below names the code, test or doc that guards it.

The vault is the source of truth; everything the UI shows is derived from it. See
[ARCHITECTURE.md](../ARCHITECTURE.md) for the data flow and the [README](../README.md) for the
other format guides and day-to-day use.

## At a glance

| Surface | Contract | Guarded by | Stable at 1.0 |
| --- | --- | --- | --- |
| Notes | Plain UTF-8 `.md` files; only `.md` files are notes | `crates/muninn-core/src/vault.rs` walk, `crates/muninn-core/src/lib.rs` | Yes |
| Wikilinks | `[[Note]]`, `[[Note\|alias]]`, `[[Note#heading]]` | `crates/muninn-core/src/wikilink.rs::parse_links` + its tests | Yes |
| Kanban | Markdown frontmatter + lanes + cards + footer | [kanban-format.md](kanban-format.md), `crates/muninn-core/src/kanban.rs` | Yes |
| Whiteboards | `.excalidraw` v2 scene JSON | `crates/muninn-core/src/board.rs` (`BOARD_EXTENSION`, `SCENE_VERSION`) | Yes |
| Frontmatter | Minimal flat, read-only reader; unknown YAML preserved | `crates/muninn-core/src/frontmatter.rs` | Yes (reader scope) |
| Locked notes | `MUNINN-NOTE-LOCK v2` ASCII envelope | `crates/muninn-core/src/note_lock.rs` + tests, [per-note-encryption.md](per-note-encryption.md) | Yes (v2; future versions rejected) |
| Settings | `settings.json`, load merges over defaults | `crates/muninn-ui/src/settings.rs::from_value` | Yes (named keys) |
| Sync state | `SCHEMA_VERSION = 1` in journals/bases/manifests | `crates/muninn-sync/src/lib.rs`, `engine.rs`, `journal.rs`, `drive.rs`, `tailnet.rs` | Yes (version 1) |
| CLI | Flags and exit codes below | `crates/muninn-app/src/main.rs`, `crates/muninn-app/tests/cli.rs` | Yes |
| Install | Asset names, receipt and `--uninstall` | `.github/workflows/release.yml`, `install.sh`, `scripts/tests/install-sh.sh` | Yes |
| Platforms | OS/arch matrix and build runners | `.github/workflows/release.yml`, `install.sh::detect_platform` | Yes (matrix; glibc floor is a build property) |

## 1. Vault and note formats

- **Plain Markdown is the source of truth.** A vault is a folder of UTF-8 `.md` files; there is
  no database or migration step, and links are resolved from the files on every load. Non-UTF-8
  files are skipped and counted, and the walk stops at 12 folder levels — both are scan limits,
  not format changes (`crates/muninn-core/src/vault.rs`, test
  `invalid_utf8_notes_are_skipped_and_counted`; `--check` prints a folder-depth warning at 12).
- **Wikilinks** are parsed by `crates/muninn-core/src/wikilink.rs::parse_links`. The supported
  forms are `[[Note]]`, `[[Note|alias]]`, `[[Note#heading]]`, `[[Note#heading|alias]]` and
  `[[#heading]]` (self-link). A trailing `.md` in the target is stripped; `[[` inside inline code
  or fenced blocks is left alone. Tests: `aliases_headings_and_self_links_are_parsed` and
  `code_spans_and_fences_keep_their_brackets` in that file.
- **Kanban** syntax — frontmatter `kanban-plugin`, lanes, cards, dates, tags, archive and the
  settings footer — is specified in [kanban-format.md](kanban-format.md), including section 10's
  round-trip rules. Detection is `muninn_core::is_kanban`; parsing and editing live in
  `crates/muninn-core/src/kanban.rs` and `crates/muninn-core/src/kanban/`, with fixtures in
  `crates/muninn-core/src/kanban/tests.rs`. Other frontmatter keys, headings that are not lanes,
  and other unmodelled text are **passthrough and kept byte for byte**; a broken settings footer is
  left untouched, not read as an empty board.
- **Whiteboards** are `.excalidraw` files, not notes (`BOARD_EXTENSION`, `crates/muninn-core/src/board.rs`).
  Muninn reads and writes Excalidraw v2 scene JSON (`SCENE_VERSION = 2`). Unmodelled element fields
  are kept in `Shape::extra`, unknown element kinds in `Element::Raw`, and `appState`, `files` and
  unknown top-level keys verbatim; keys are sorted and absent modelled fields get Excalidraw's
  defaults. Those are data-preserving normalisations, not a rewrite of unknown content.
- **Frontmatter** is read by a deliberately minimal, flat, **read-only** parser
  (`crates/muninn-core/src/frontmatter.rs`): `key: value`, quoted values, `- item` and `[a, b]`
  lists, comments and CRLF. Nested YAML is ignored — not rewritten, and no caller writes through
  this reader. Muninn reads `tags`/`tag` (`crates/muninn-core/src/tags.rs`) and the `kanban-plugin`
  key; the latter's value is for display only and is never written back
  (`crates/muninn-core/src/kanban.rs`).
- **Unknown constructs are preserved, not rewritten.** A note's body stays as written; Kanban
  edits splice only the affected lines (`docs/kanban-format.md` §10); board round-trips keep
  unknown JSON (above); and frontmatter has no write path. Rename/move may rewrite *inbound*
  wikilinks as a deliberate, per-file edit (`Vault::reindex` would resolve links without it), and
  a locked note's bytes are replaced by its envelope (next section).

## 2. Per-note encryption

- A locked note keeps its `.md` filename but its body becomes an ASCII, UTF-8-valid
  `MUNINN-NOTE-LOCK v2` envelope: a `kdf=argon2id` line, an `aead=aes-256-gcm` line, a `note=`
  line and a `rev=` line, a blank separator, then wrapped base64url ciphertext plus a 16-byte tag
  (`crates/muninn-core/src/note_lock.rs`, `LOCK_MAGIC`, `LOCK_VERSION = 2`; full field table in
  [per-note-encryption.md](per-note-encryption.md)). Tests include
  `the_envelope_is_a_valid_utf8_markdown_file_with_a_magic`, `round_trip_returns_the_exact_text`,
  `the_vault_walk_still_lists_a_locked_note` and the `crates/muninn-ui/tests/lock_*.rs` integration
  tests.
- **What is authenticated:** all four header lines (including the version, salt, nonce, note id
  and revision) are AES-GCM associated data, and the ciphertext and tag cover the plaintext.
  Changing any header byte or payload byte fails `unlock` with the single
  `AuthenticationFailed` error; a wrong note id is `WrongNote`, detected before key derivation.
  Tests: `tampered_header_fails`, `tampered_payload_fails`, `tampering_with_rev_fails_authentication`,
  `a_different_note_is_wrong_note_not_a_password_failure`.
- **Unknown versions are detected, not decrypted.** `is_locked` recognises any numeric version so
  a future envelope stays visibly locked; `parse` returns `UnsupportedVersion` for anything but
  `v2`, and the file is never shown as plaintext. Test: `a_v1_envelope_is_reported_not_decoded`.
  CRLF line endings are normalised on read (`crlf_rewritten_envelopes_still_open`); that is reader
  tolerance, not a second format.
- **Password loss is unrecoverable and is not a compatibility break.** There is no reset, recovery
  key or escrow: a forgotten password loses that note, and earlier plaintext may survive in
  backups or sync history ([per-note-encryption.md](per-note-encryption.md), module docs in
  `note_lock.rs`). No release can restore it.

## 3. Settings

`settings.json` lives at `$XDG_CONFIG_HOME/muninn/settings.json` (fallback `~/.config/muninn/`),
outside the vault and outside sync. Loading calls `Settings::from_value`, which **starts from
`Settings::default()` and overlays only the keys it reads** (`crates/muninn-ui/src/settings.rs`);
a missing, unreadable or corrupt file yields defaults (`corrupt_settings_fall_back_to_defaults`).
Unrecognised top-level keys are ignored, so an older or newer build can share a file. One
deliberate exception: the `kanban_defaults` sub-object keeps unknown/mistyped keys through a save
(`kanban_defaults_keep_unknown_and_mistyped_keys_through_a_save`).

The keys that carry the user-facing contract are `theme` (`omarchy|system|dark|light`, with old
booleans still read), `ui_language`, `last_vault`, `autosave_delay`, `sync`, `reduce_motion`,
`graph_effects`, `graph_effect_intensity`, `board_autosave`, `kanban_autosave` and
`check_for_updates`. The remaining fields (`collapsed`, `selected_rel`, `mode`, `scope`,
`recent_vaults`, `note_template`, `board`, `kanban`, `kanban_defaults`) are session/UI state with
the same forgiving read. The update check's schedule and response cache (`last_update_check`,
`update_next_check`, `update_failures`, `update_etag`) are bookkeeping, not named contract keys.
Malformed `sync` prefs disable sync and tokens are never written here
(`malformed_sync_disables_sync_and_settings_never_store_tokens`).

## 4. Sync

`SCHEMA_VERSION: u32 = 1` in `crates/muninn-sync/src/lib.rs` marks local journals
(`journal.json`), bases (`base.json`), Drive commit manifests and Tailnet host state. On a
**mismatch the reader refuses the state and sync stops — it never guesses, migrates in place or
reports success**: `Journal::load`/`store` return `SyncError::Journal`, and `load_base`,
`parse_commit` and `HttpStore::state` return an error for an unsupported schema
(`crates/muninn-sync/src/{journal,engine,drive,tailnet}.rs`; see
[sync-architecture.md](sync-architecture.md)). This state is bookkeeping outside the vault; the
note files remain the source of truth.

## 5. CLI

All flags below are parsed in `crates/muninn-app/src/main.rs` and guarded by
`crates/muninn-app/tests/cli.rs`; flag strings and exit codes are **stable**.

| Invocation | Contract |
| --- | --- |
| `--version`, `-V` | Print `muninn <version>` and exit 0 before a window or GPU context is created. |
| `--help`, `-h` | Print the app's usage and exit 0 (answered from raw args, before Rune consumes `--help`). |
| `--check <vault>` | Exit **0** clean, **1** broken links or duplicate titles, **2** unreadable/missing vault; never creates the folder. |
| `--list-links <vault>` | Exit **0** with one sorted `referrer -> target` line per wikilink, unresolved targets prefixed `missing:`; exit **2** on an unreadable vault. |
| `--vault PATH` | Open this vault (wins over `$MUNINN_VAULT`, the last vault and `~/Muninn`). |
| `--theme omarchy\|system\|dark\|light` | Session-only theme override; does not change the persisted preference. |
| `--trace` | Add autosave ticks to the session log. |
| `--new-instance` | Start a second app even when one is already running. |
| Bad/missing value for `--theme`, `--check` or `--list-links` | Print help on stderr and exit **2** (`Cli::Error` in `main.rs`; test `a_missing_check_path_prints_our_usage`). |

**Machine-readable output:** `--list-links` writes no header — one line per link, sorted and
deduplicated, with resolved targets as vault-relative paths and unresolved ones as
`missing:<target>` (`list_links`, test `list_links_prints_sorted_referrer_target_lines`).
`--check` writes fixed `notes:`, `broken links:`, `duplicates:` and `skipped:` count lines, then
`  referrer -> target` lines for broken links, with warnings on separate `warning:` lines
(`check_vault`, tests `check_exit_zero_for_a_clean_vault`,
`check_exit_one_reports_broken_links_and_duplicates`, `check_exit_two_for_a_missing_vault_and_creates_nothing`).
Headless results — including `error:` lines for an unreadable vault — go to stdout; CLI parse
errors go to stderr. Normal GUI launches have their own single-instance exit codes (0 forwarded,
1 no answer) — see the `single_instance` tests.

## 6. Install and update surface

Release artifacts are named by `.github/workflows/release.yml` as
`muninn_<TAG>_<os>_<arch>` (with `<TAG>` the `v`-prefixed git tag), so the stable names are
`muninn_<TAG>_linux_x86_64.tar.gz`, `muninn_<TAG>_linux_aarch64.tar.gz`,
`muninn_<TAG>_macos_aarch64.tar.gz`, `muninn_<TAG>_windows_x86_64.zip`, plus Linux `.deb` packages
(`muninn_<VERSION>-1_amd64.deb` / `_arm64.deb`). `scripts/build-appimage.sh` normalises its output
to `muninn_v<VERSION>_linux_<arch>.AppImage` with a single `v` prefix.

Published release tags are immutable (a `v*` tag is never deleted or moved, so `--version TAG`
always names the same version), while release assets may be refreshed in place by a rerun: the
asset set and names above are the contract, not the publish timestamp.

`install.sh` accepts `--version TAG`, `--prefix DIR`, `--uninstall`, `--require-signature`,
`--no-verify` and `-h|--help` (its `usage()` is the list). On install it writes a receipt at
`$PREFIX/share/muninn/install-receipt`; `--uninstall` removes **only** receipt-listed files and
then prunes empty directories, and never touches `~/.config/muninn`, sync state or a vault
(`install.sh::do_uninstall`; `scripts/tests/install-sh.sh` tests "install, receipt and binary",
"tampered checksums rejected" and "uninstall removes only receipt-listed files"). The installer
never runs `sudo`. An AppImage is a single self-mounting file: it installs nothing, writes no
receipt and has no uninstall step (`scripts/build-appimage.sh` produces the single file, and
`install.sh` only ever downloads the tarballs). Tarball and AppImage installs can update in place
after the explicit **Update** action, with the checksum verified and a signature check when a
verifier is installed; `.deb` and AUR installs cannot and stay with their package manager, and
the installed asset names are unchanged. Installation instructions and verification
commands remain in the [README](../README.md#install).

## 7. Platform floor

The release workflow builds and publishes exactly these targets
(`.github/workflows/release.yml` matrix; `install.sh::detect_platform` rejects anything else):

| OS | Architecture | Build runner |
| --- | --- | --- |
| Linux | x86_64 | `ubuntu-24.04` |
| Linux | aarch64 | `ubuntu-24.04-arm` |
| macOS | aarch64 only | `macos-latest` |
| Windows | x86_64 | `windows-latest` |

There is no Intel macOS or 32-bit Windows build. For Linux the published glibc floor is **2.39 on
both architectures** (Ubuntu 24.04 LTS ships 2.39), and it is a deliberate policy rather than a
side effect of whichever runner image happens to be current: the release build runners are pinned
to `ubuntu-24.04` / `ubuntu-24.04-arm`, not `ubuntu-latest`. Raising the floor is a breaking
change under §8.

Both Linux release builds, and the CI AppImage job, run
`scripts/ci/check-glibc.sh --binary target/release/muninn --max GLIBC_2.39` after compiling. The
check reads the binary's `GLIBC_*` version needs with `readelf --version-info` and fails when the
maximum exceeds the policy, so a runner-image upgrade that raises the floor fails the release
instead of shipping silently. The CI `appimage` job also runs on `ubuntu-24.04` and then starts the
packaged AppImage (`--version`, `--check`), so a binary that needs a newer glibc than 24.04 provides
fails there as well.

## 8. Versioning and deprecation policy

[CHANGELOG.md](../CHANGELOG.md) states that the project follows Semantic Versioning. For this app
that means:

- **Patch** — no change to anything in the tables above; a bug fix inside a contract.
- **Minor** — additive: a new format feature an old build can ignore, a new optional settings key,
  a new CLI flag, or a new platform. Before 1.0, minors may also change a contract listed above if
  the change ships with migration notes.
- **Breaking / major** — a change that makes a build stop reading a vault file, frontmatter shape,
  Kanban syntax, `.excalidraw` scene, `MUNINN-NOTE-LOCK` envelope, settings key, sync schema or CLI
  flag/exit code that this document promises. At 1.0 and later this requires a major version.
  Dropping a platform is also breaking.

**Deprecation window.** When a format, key or flag is retired, the replacement is documented in
the same change; the old form is still **read for at least one minor release** (and written where
it is still unambiguous); `--check` and `--list-links` keep working on the old vault in that
window; and the removal carries an explicit migration note in the changelog. A reader that cannot
satisfy this — for example an unknown note-lock version — must fail closed and say so, as `v2`
does today, rather than corrupt or silently reinterpret the file.

**Pre-1.0 exceptions.** Current versions are `0.x`. Before 1.0 any of the contracts above may
change in a minor release, and experimental surfaces may change or disappear without that
window; every such change still needs migration notes. This document is the promise that 1.0
makes those contracts durable.

**Proposing a change.** Open an issue naming the contract and the file/test that guards it,
explain the user-visible effect on an existing vault, include migration notes (how an old file or
setting is read, for how long, and what the replacement is), and update this page and the guards
in the same change. A reviewer should be able to classify the change as patch, minor or breaking
from that alone.

## 9. Not stable

Do not build against these before 1.0, and do not treat them as covered by the table:

- **The experimental web/WASM target.** CI only checks `muninn-core` and `muninn-graph` on
  `wasm32-unknown-unknown` (`.github/workflows/ci.yml`); there is no shipped web app.
- **Internal crate APIs and private helpers.** Renameable Rust items, including `pub` items in
  `muninn-*`, are implementation details; only the file/CLI contracts above are stable.
- **The exact Graph effect tuning and visual details.** Presets, intensity values, layout
  constants and screenshot snapshots can change without a format break (see `ARCHITECTURE.md` and
  `crates/muninn-graph`).
- **Sync conflict filenames and internal state layout.** The `<stem> (remote)` / `(conflict …)`
  naming and the on-disk bookkeeping under the XDG state directory are described in
  [sync-architecture.md](sync-architecture.md) but are not part of the vault-file contract.
- **Installer UX details.** Wording, the exact sudo hint and the presence of optional verifiers
  can change; the asset names, flags, receipt behaviour and platform matrix above cannot.

## See also

- [README.md](../README.md) — install, sync setup and day-to-day use.
- [ARCHITECTURE.md](../ARCHITECTURE.md) — data flow and invariants.
- [kanban-format.md](kanban-format.md), [per-note-encryption.md](per-note-encryption.md),
  [sync-architecture.md](sync-architecture.md), [sync-safety.md](sync-safety.md).
