# Sync setup

Muninn ships the public client id of the official **Muninn Sync** GitHub App
(<https://github.com/apps/muninn-sync>). The official Google OAuth client id is
not set yet; until it is, a build has no bundled Google id and Drive Connect
fails closed unless you enter your own. Nothing in the tests logs into a real
account.

Client ids live in `crates/muninn-sync/src/clients.rs` (the insertion point).
They are public configuration, never secrets: Muninn ships no client secret and
no private key for either provider. A build can set them at compile time with
`MUNINN_GITHUB_CLIENT_ID` and `MUNINN_GOOGLE_CLIENT_ID`. The optional client id
fields in Settings → Sync override the bundled id, for a fork's own app.

## Google Drive

1. Create a Google Cloud project.
2. Enable the Google Drive API. No Picker API is needed.
3. Configure the OAuth consent screen and **publish it to production**. An
   External app left in "Testing" needs listed test users and expires refresh
   tokens after 7 days, so every device would have to sign in again weekly.
   `drive.appdata` is a non-sensitive scope, so publishing does not need a
   security assessment.
4. Create a Desktop OAuth client. Use its client id only: never ship or paste
   its client secret. Muninn uses PKCE and sends no secret. Build the official
   id in with `MUNINN_GOOGLE_CLIENT_ID`, or paste your own into Settings → Sync
   → Drive → Google client id (optional).
5. Muninn requests only `https://www.googleapis.com/auth/drive.appdata`, with
   PKCE S256, a random `state`, and a loopback redirect on
   `http://127.0.0.1:<random port>/callback`. It does not request full Drive,
   `drive.file`, email, or profile scopes.
6. Connect opens Google's consent page (`accounts.google.com`) in the system
   browser. After consent, Google redirects to the loopback listener, Muninn
   exchanges the code at `oauth2.googleapis.com`, and the tokens go to the OS
   credential store. The wait times out after five minutes; Disconnect cancels
   it.
7. Drive sync keeps its data in the app's hidden `appDataFolder`, in a root
   folder named `muninn-sync-<store name>` with `blobs` and `commits` inside.
   Use the same store name on every device that shares the vault, and a
   different one for another vault on the same Google account.

### Why `appDataFolder`

`drive.file` only reaches files this app created or that the person opened
with it (for example with the Google Picker), so a pasted folder id never
worked without a Picker. `drive.appdata` reaches only the app's own hidden
folder: no Picker, a non-sensitive scope, and every device signed in with the
same account and OAuth client sees the same store. The costs: the store is not
visible or editable in the Drive UI, it counts against the account's storage,
and revoking Muninn's access in the Google Account deletes it (the notes on
each device are not affected). Those objects are not Markdown files either
way; do not expect to edit notes in Drive.

## GitHub

Muninn signs in through a **GitHub App with device flow**, not a personal
access token. The official app is Muninn Sync
(<https://github.com/apps/muninn-sync>; it is named that because "Muninn" was
taken): device flow on, repository permission Contents read and write only,
expiring user tokens on, no webhook, installable on any account. Its public
client id is bundled; there is no client secret or private key in Muninn, and
device-flow tokens refresh without one.

1. Install Muninn Sync on the one repository that will hold the notes
   (<https://github.com/apps/muninn-sync>, Install). A fork that wants its own
   app creates a GitHub App the same way and enters its client id under
   Settings → Sync → GitHub client id (optional).
2. Repository permission: Contents read and write. Metadata read comes with
   that. Do not grant Actions, Administration, or Workflows.
3. The app is installed on one repository. The app's permissions are
   intersected with the user's. A private repository is appropriate because a
   public repository makes the notes public.
4. Choose the account, repository, branch, and path prefix in Settings. The
   permission is per repository, not per branch, so Muninn also enforces the
   prefix locally. Do not point this at the Muninn source repository.
5. Connect shows a code and opens `https://github.com/login/device` in the
   system browser. Enter the code there; Muninn polls at GitHub's interval and
   stores the token, refresh token and absolute expiry in the OS credential
   store. Expiring user tokens are refreshed before they expire and once after
   a 401; GitHub rotates the refresh token and Muninn stores the new one.

Connect replaces a vault's existing binding only after sign-in succeeds. A
declined, failed or cancelled sign-in leaves the current connection and its
token in place; a successful one deletes the old binding's credential.

## Tailscale

Tailscale has no file sync of its own, so one Muninn device hosts a commit
store and the others join it over the tailnet. Nothing in the tests talks to a
real tailnet or `tailscaled`.

1. Turn on MagicDNS and HTTPS certificates for the tailnet in the Tailscale
   admin console. HTTPS certificates put the device's `*.ts.net` name in public
   Certificate Transparency logs.
2. On the host: Settings → Sync → Tailscale → "This device hosts" → Connect.
   Muninn generates a 256-bit shared key, stores it in the OS credential store,
   and starts serving on `127.0.0.1:<port>` (default 47821) while Muninn is
   open. The key is masked in Settings; use Copy key (a secret clipboard copy
   that is cleared after 60 seconds and kept out of clipboard history) or Show
   key. Rotate key replaces it; every joined device must then join again. Run
   the command Muninn shows once on that device:
   `tailscale serve --bg --https=443 http://127.0.0.1:47821`.
3. On each other device: "Join a host", enter the host's full MagicDNS name
   (`device.tailnet.ts.net`, not the short name), paste the shared key into the
   masked field, then Connect. The key goes to that device's credential store
   and the field is cleared.
4. Optional: list allowed Tailscale logins on the host. Serve sets
   `Tailscale-User-Login` from its own WhoIs lookup. Tagged devices get no
   identity header, so leave the list empty if a tagged device must sync.

The server binds loopback only, so the identity header cannot be forged from
the LAN or tailnet. The host's store is under
`$XDG_STATE_HOME/muninn/sync/tailnet-store/<binding>`. It is not the vault, and
it is not editable Markdown.

## Local state

Each binding keeps its base snapshot, journal, staged bytes and GitHub blob
cache in `$XDG_STATE_HOME/muninn/sync/bindings/<binding id>`, private to the
user (0700) on Unix. A base left in the shared `sync/` directory by an earlier
build moves into the binding it names on the next run.

## Linux credentials

The credential store is Secret Service over the session D-Bus. The collection
has to be unlocked. If it is missing or locked, sync reports that credentials
are unavailable. It does not write the token to a file.

macOS uses Keychain Services. Windows uses Credential Manager. Those prompts
and signing requirements were not exercised in this environment.
