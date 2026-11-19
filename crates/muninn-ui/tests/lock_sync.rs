//! Per-note encryption (#134): sync uploads the envelope, never the body.
//!
//! The engine is not driven here — the inventory is the seam that decides
//! exactly which bytes a provider would receive, so hashing it proves the
//! upload is ciphertext.

use std::fs;

use muninn_core::testing::TempVault;
use muninn_sync::{ContentHash, RelativePath, inventory_default};

const PW: &str = "correct horse battery staple";
const BODY: &str = "# Secret\n\nTOP SECRET BODY\n";
const MARKER: &str = "TOP SECRET BODY";

#[test]
fn the_sync_inventory_hashes_the_envelope_and_not_the_body() {
    let envelope = muninn_core::lock(BODY, PW, "Secret").expect("lock");
    assert!(!envelope.contains(MARKER));
    let vault = TempVault::new(&[("Secret.md", envelope.as_str())]);

    let inventory = inventory_default(vault.path()).expect("inventory");
    let path = RelativePath::parse("Secret.md").expect("relative path");
    let record = inventory
        .snapshot
        .get(&path)
        .expect("Secret.md is in the snapshot");
    assert_eq!(
        record.hash,
        ContentHash::of(envelope.as_bytes()),
        "the snapshot points at the envelope bytes"
    );
    assert_eq!(record.size, envelope.len() as u64);
    assert_eq!(
        inventory.total_bytes,
        envelope.len() as u64,
        "the whole upload budget is the envelope"
    );

    let bytes = fs::read(vault.path().join("Secret.md")).expect("read the file");
    assert!(
        !bytes
            .windows(MARKER.len())
            .any(|window| window == MARKER.as_bytes()),
        "no plaintext marker in the uploaded bytes"
    );
    assert_eq!(
        muninn_core::unlock(
            std::str::from_utf8(&bytes).expect("the envelope is UTF-8"),
            PW,
            "Secret"
        )
        .expect("the uploaded bytes open with the password"),
        BODY
    );
}
