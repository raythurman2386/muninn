# Sync safety

- One provider binding and one active sync writer per vault. There is no
  background sync and no timer.
- The first bind cannot delete. Later deletes require a complete base snapshot
  and a complete remote listing.
- Before a local file is replaced, its previous bytes are copied to the sync
  checkpoint directory outside the vault. The note trash budget does not prune
  those copies.
- Each file is written to a temporary sibling, fsynced, and renamed. On Unix the
  rename replaces the destination. On Windows the previous file is moved aside
  first; the journal can finish that sequence after a crash.
- A dirty editor buffer blocks Sync Now. Autosave and the metadata watcher do
  not run while a sync is applying, so they do not observe half a batch.
- Paths are opened without following symlinks. A symlink inside the synced
  tree fails the run.
- Remote concurrency: GitHub ref updates send `force: false`. Drive commits are
  immutable, so a concurrent writer becomes a second head instead of an
  overwrite.
- A timeout after a mutation is an unknown outcome. The next attempt looks up
  the operation id (GitHub commit message, or Drive commit manifest) before
  creating another one.
- Retries are bounded and apply to rate limits and transport failures. Auth,
  permission, and protected-branch errors stop the run.
- Cancel stops between requests. It does not roll back a commit the provider
  already accepted.
- Disconnect changes the binding and credentials only.

## Unverified at runtime

These builds and fixture tests do not prove:

- a real OS keychain prompt, unlock, or access-control dialog
- a real Google Cloud project, OAuth consent screen, or Drive grant
- a real GitHub App installation or device-flow login
- that Google or GitHub will keep the HTTP behavior these fixtures model

The native credential backends are compiled. Tests use an in-memory store.
