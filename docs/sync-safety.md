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
- Remote concurrency: every publish names exactly the remote state its plan
  read. GitHub ref updates send `force: false` with that commit as parent; a
  422 is replanned from a fresh read, never re-parented. Drive commits are
  immutable and name the heads the plan merged; a commit that appears after
  the read makes the run replan, and one that lands later becomes a second
  head instead of an overwrite; two Drive heads that changed the same path are
  merged with a conflict copy beside it, never a silent pick, a lost edit, or
  a stuck store. A note edited here and on another device since the last sync
  is kept twice the same way: this device's text at the path, the remote's as
  `<stem> (remote)<ext>`, named in the status. The Tailscale host is one
  serialized writer: it accepts a commit only when its parents are exactly the
  current heads, so a peer that read an older head is refused and replans. Blobs and commits are create-only; the
  host has no overwrite or delete endpoint.
- A timeout after a mutation is an unknown outcome. The next attempt looks up
  the operation id (GitHub commit message, or Drive commit manifest) before
  creating another one. Tailscale PUTs are idempotent (same bytes, same answer),
  so they are retried a bounded number of times.
- Retries are bounded and apply to rate limits and transport failures. Auth,
  permission, and protected-branch errors stop the run.
- Cancel stops between requests. It does not roll back a commit the provider
  already accepted.
- Disconnect changes the binding and credentials only.
- Connect does not touch settings until the new credential is stored; only
  then is the old binding replaced and its credential deleted.
- The Google sign-in listener on `127.0.0.1` answers a request without this
  attempt's `state` with 400 and keeps waiting, so another local process
  cannot end the sign-in.

## Unverified at runtime

These builds and fixture tests do not prove:

- a real OS keychain prompt, unlock, or access-control dialog
- a real Google Cloud project, OAuth consent screen, or Drive grant
- a real GitHub App installation or device-flow login (the Muninn Sync app
  exists, but no login with it was run here)
- Drive sign-in with the bundled Google client (no login with it was run
  here)
- a real tailnet: `tailscale serve`, MagicDNS, ts.net certificates, and the
  `Tailscale-User-Login` header are modeled by loopback fixtures only
- that the system browser opens (`xdg-open`, `open`, `rundll32`); tests use a
  fake opener that follows the redirect
- Google's real `appDataFolder` behavior and `drive.appdata` consent
- that Google or GitHub will keep the HTTP behavior these fixtures model

The native credential backends are compiled. Tests use an in-memory store.
