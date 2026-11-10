# Privacy Policy

Effective date: November 9, 2026

Muninn is a local-first notes app made by Raymond Thurman, doing business as Ravenwood Software. This policy describes what happens when you use optional sync and when you visit the Muninn website.

## Notes stay on your device

Muninn stores your notes as Markdown files in a folder you choose on your computer. We do not host your notes on a Muninn server.

## Optional sync

You can choose to sync a vault with Google Drive, GitHub, or another device on your Tailscale network. Sync is off until you connect it.

### Google Drive

If you connect Google Drive, Muninn asks only for access to this app's hidden app folder in Drive (`drive.appdata`). Muninn cannot see, read, or change the rest of your Drive files. The sync store is not visible in the Drive website. It counts against your Google storage. If you revoke Muninn's access in your Google Account, Google deletes that hidden store; the notes on your devices are not deleted.

### GitHub

If you connect GitHub, you install the Muninn Sync GitHub App on one repository you choose and sign in with GitHub. Muninn can only read and write files in that repository. Use a private repository. A public repository would make your notes public.

### Tailscale

If you use Tailscale sync, traffic stays on your private tailnet. A shared key is stored in your device's credential store, not in Muninn settings. Turning on Tailscale HTTPS can list a device's name in public Certificate Transparency logs, which is how Tailscale HTTPS certificates work.

## Credentials

Sign-in tokens and the Tailscale key are stored in your operating system's credential store (such as Keychain, Credential Manager, or Secret Service). They are not saved in settings files or notes.

## What we do not collect

Muninn does not create a Muninn account, and the app does not send your notes or usage data to us. Google, GitHub, and Tailscale may keep their own records of sign-in and API use under their policies.

## Website

The Muninn website at https://muninn.ravenwoodsoftware.dev uses Umami, an analytics tool we host ourselves at https://umami-app.ravenwoodsoftware.dev, to count visits to the site. This is only on the website. It is not part of the desktop app, and the app still does not send your notes or usage data to us.

## Contact

Questions or requests: open an issue at https://github.com/raythurman2386/muninn or email raymondthurman5@gmail.com.
