# Sync setup

No Google project and no GitHub App were registered for this work. Nothing in
the tests logs into a real account. The steps below are what a person has to do
before Connect can succeed. Muninn does not do them.

## Google Drive

1. Create a Google Cloud project.
2. Enable the Google Drive API. Enable the Google Picker API if a person will
   choose the folder with Google's picker instead of pasting an id.
3. Configure the OAuth consent screen. External testing needs test users until
   the app is published and, if Google requires it, verified.
4. Create a Desktop OAuth client. Desktop clients do not have a client secret
   Muninn can safely embed. The client id is public configuration.
5. Muninn requests only `https://www.googleapis.com/auth/drive.file`, with PKCE
   S256, a random `state`, and a loopback redirect on `127.0.0.1`. It does not
   request full Drive, email, or profile scopes.
6. The chosen folder must be one this app created or that the user granted to
   the app. A pasted folder id does not by itself grant access. Children the
   app cannot see are reported as inaccessible, not deleted.
7. Drive sync writes `muninn-sync/blobs` and `muninn-sync/commits` inside that
   folder. Those objects are not a normal tree of Markdown files. Do not expect
   to edit notes by opening them in Drive.

## GitHub

1. Create a GitHub App. Enable device flow. Do not embed a client secret or a
   private key in Muninn.
2. Repository permission: Contents read and write. Metadata read comes with
   that. Do not grant Actions, Administration, or Workflows.
3. The user installs the app on one repository. The app's permissions are
   intersected with the user's. A private repository is appropriate because a
   public repository makes the notes public.
4. Choose the account, repository, branch, and path prefix in Settings. The
   permission is per repository, not per branch, so Muninn also enforces the
   prefix locally. Do not point this at the Muninn source repository.
5. A fine-grained personal access token limited to one repository's Contents
   read and write can be stored in the OS credential store as a manual
   alternative. It must not be pasted into a note or into `settings.json`.

## Linux credentials

The credential store is Secret Service over the session D-Bus. The collection
has to be unlocked. If it is missing or locked, sync reports that credentials
are unavailable. It does not write the token to a file.

macOS uses Keychain Services. Windows uses Credential Manager. Those prompts
and signing requirements were not exercised in this environment.
