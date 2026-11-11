//! Temporary vaults for tests in this workspace.
//!
//! Enabled by the `test-support` feature so `muninn-graph` and `muninn-ui` can use the same
//! fixture helper without duplicating filesystem code.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::vault::Vault;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A uniquely named temp folder that removes itself when dropped.
pub struct TempVault {
    root: PathBuf,
}

impl TempVault {
    /// Create a temp folder with `files`, where each name may contain subdirectories.
    #[must_use]
    pub fn new(files: &[(&str, &str)]) -> TempVault {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("muninn-{}-{n}", std::process::id()));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("temp dir");
        for (name, body) in files {
            let path = root.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("fixture folder");
            }
            fs::write(path, body).expect("fixture");
        }
        TempVault { root }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Scan the folder into a [`Vault`].
    #[must_use]
    pub fn load(&self) -> Vault {
        Vault::load(&self.root).expect("vault")
    }
}

impl Drop for TempVault {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.root));
    }
}

thread_local! {
    static SCRATCH: std::cell::RefCell<Vec<Scratch>> = const { std::cell::RefCell::new(Vec::new()) };
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

/// Remove `dir` when the current thread ends: a test's config and state
/// homes, which outlive the `Runtime` they are handed to (a relaunch reuses
/// them), go when the test does instead of piling up in the temp folder.
pub fn remove_on_thread_exit(dir: &Path) {
    SCRATCH.with(|s| s.borrow_mut().push(Scratch(dir.to_path_buf())));
}

/// A deterministic synthetic vault of `n` notes: 25-note star groups plus a
/// chain, so every note links to its group hub and the previous note. Used by
/// graph and UI performance/settle tests so the fixture lives in one place.
#[must_use]
pub fn big_vault(n: usize) -> TempVault {
    let files: Vec<(String, String)> = (0..n)
        .map(|i| {
            let mut body = format!("# Note {i:04}\n");
            if i > 0 {
                body.push_str(&format!("[[Note {:04}]]\n", i - 1));
            }
            let hub = i - i % 25;
            if hub != i {
                body.push_str(&format!("[[Note {hub:04}]]\n"));
            }
            (format!("Note {i:04}.md"), body)
        })
        .collect();
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, body)| (name.as_str(), body.as_str()))
        .collect();
    TempVault::new(&refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scratch_dir_is_gone_once_its_thread_ends() {
        let dir = std::env::temp_dir().join(format!("muninn-scratch-{}", std::process::id()));
        let inner = dir.clone();
        std::thread::spawn(move || {
            fs::create_dir_all(inner.join("state")).expect("dir");
            remove_on_thread_exit(&inner);
            assert!(inner.is_dir(), "kept while the thread runs");
        })
        .join()
        .expect("thread");
        assert!(!dir.exists(), "{} was left behind", dir.display());
    }
}
