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
