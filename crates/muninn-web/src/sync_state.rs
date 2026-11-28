//! Where the worker's sync state lives, and the keys it uses (#240 slice 8a).
//!
//! The directory name is shared by the wasm worker and the native test, so it
//! lives outside the wasm-only `sync` module. The keys themselves are not
//! re-declared here: the engine and the worker both use
//! `muninn_sync::state_home`'s constants and helpers. The test below pins
//! those keys to their relative spellings and to `NativeStateHome`'s native
//! paths, which is the key-for-key equality the browser twin must preserve.

/// The OPFS directory the sync engine's state lives in, sibling to
/// `muninn-vault` and `muninn-state`.
///
/// Nothing else may read or write it: it is the per-binding base, journal,
/// prepared list, sha cache, staged bytes and checkpoints, not user state.
pub const SYNC_STATE_DIR: &str = "muninn-sync-state";

/// The cross-tab lock a worker takes for the duration of a run.
///
/// It is a Web Lock, not a lease file: a worker that is killed releases it,
/// and there is no timeout to steal. The page's boot lock already serializes
/// writing tabs for the vault; this one is for the run entry to serialize runs
/// across tabs.
pub const SYNC_LOCK_NAME: &str = "muninn-sync-run";

#[cfg(test)]
mod tests {
    use super::SYNC_STATE_DIR;
    use muninn_sync::state_home::{self, NativeStateHome};

    /// The worker's OPFS home stores each key as a relative path below
    /// [`SYNC_STATE_DIR`]. This is exactly the native binding layout, so a
    /// key's spelling is the same on both hosts:
    ///
    /// | key | relative file |
    /// | --- | --- |
    /// | `base.json` | `base.json` |
    /// | `journal.json` | `journal.json` |
    /// | `prepared.json` | `prepared.json` |
    /// | `github-blob-cache.json` | `github-blob-cache.json` |
    /// | `staged/<path>` | `staged/<path>` |
    /// | `checkpoints/<path>` | `checkpoints/<path>` |
    ///
    /// The expected strings are written out rather than read from the same
    /// constants the code uses, so a rename fails here instead of silently
    /// matching itself.
    #[test]
    fn the_worker_keys_equal_the_native_state_home_paths() {
        let root = std::path::PathBuf::from("/tmp/muninn-sync-state");
        let home = NativeStateHome::new(root.clone());
        let fixed = [
            (state_home::BASE, "base.json"),
            (state_home::JOURNAL, "journal.json"),
            (state_home::PREPARED, "prepared.json"),
            (state_home::SHA_CACHE, "github-blob-cache.json"),
        ];
        for (key, relative) in fixed {
            assert_eq!(key, relative, "the key is its own relative path");
            assert_eq!(home.path_for(key), root.join(relative), "{key}");
        }
        for (key, relative) in [
            (state_home::staged_key("notes/a.md"), "staged/notes/a.md"),
            (
                state_home::checkpoint_key("notes/a.md"),
                "checkpoints/notes/a.md",
            ),
        ] {
            assert_eq!(key, relative);
            assert_eq!(home.path_for(&key), root.join(relative), "{key}");
        }
    }

    #[test]
    fn the_sync_state_directory_is_not_a_vault_or_app_state_directory() {
        // The three OPFS siblings are distinct by name. The vault and app
        // state names are literals in `opfs.rs`; pin the sync one here so a
        // future rename cannot collide with them.
        assert_eq!(SYNC_STATE_DIR, "muninn-sync-state");
        assert_ne!(SYNC_STATE_DIR, "muninn-vault");
        assert_ne!(SYNC_STATE_DIR, "muninn-state");
        assert!(!SYNC_STATE_DIR.contains('/'), "one directory, not a path");
    }
}
