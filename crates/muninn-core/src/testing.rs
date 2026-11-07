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
