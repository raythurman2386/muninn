# Privacy Policy

Effective date: November 14, 2026

Muninn is a local-first notes app made by Raymond Thurman, doing business as Ravenwood Software. This policy describes what happens when you use optional sync, when you visit the Muninn website, and when you join the Early access list.

## Notes stay on your device

Muninn stores your notes as Markdown files in a folder you choose on your computer. We do not host your notes on a Muninn server.

If you lock a note with its own password, its file is ciphertext on disk, in backups and in the
connected sync store: Muninn uploads the envelope bytes and never the body. Muninn keeps only the
derived key in memory, for 15 idle minutes after the note is last used, and holds no escrow, reset
or recovery key — losing the password loses the note. Some metadata stays visible by design: the
filename (title), folder, file size and modification time.

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

The Muninn website at https://muninn.ravenwoodsoftware.dev is served through Cloudflare, which proxies the site and caches it on a CDN. Cloudflare sees visitor IP addresses and request metadata such as the requested URL and user agent. Cloudflare's email address obfuscation is on, so email addresses in the pages Cloudflare serves are rewritten to make them harder for scrapers to harvest.

The site uses Umami, an analytics tool we host ourselves at https://umami-app.ravenwoodsoftware.dev, to count visits. The script we load from that host does not set cookies. It sends a page-view event to that same host with the page URL, referrer, title, screen size, language, and hostname. This is only on the website. It is not part of the desktop app, and the app still does not send your notes or usage data to us.

## Early access list

The website has an optional Early access form at https://muninn.ravenwoodsoftware.dev/waitlist/. If you sign up:

What we collect: your email address only. The form has no name field. Listmonk, the mailing-list tool we use, fills in a display name from the part of your address before the @.

Why: to email you about Muninn launches and major releases. We do not use it for anything else.

Double opt-in: we send one confirmation email. You are not on the list until you click the link in it.

Where it is stored: in Listmonk, which we host ourselves on our own server, together with your subscription status and the times you signed up and confirmed. We do not record the IP address you confirm from, and emails to this list do not use tracking pixels or tracked links.

Spam protection: the form uses Cloudflare Turnstile to check that a person is signing up. Turnstile runs in your browser, and Cloudflare processes your IP address and browser signals to issue the check, under [Cloudflare's privacy policy](https://www.cloudflare.com/privacypolicy/) and its [Turnstile privacy addendum](https://www.cloudflare.com/turnstile-privacy-policy/). Our signup service keeps rate-limit counters in memory only, keyed by a hash of your IP address (for an IPv6 address, of its /64 network prefix) and by a hash of your email address; they are discarded when idle, when the service restarts, or to make room for newer entries. It does not write your IP address or email address to disk or to its logs.

Server logs: like any web server, our server keeps short-lived technical logs for the website, the signup service, and Listmonk, including when you click the confirmation link. They can include request details such as the time, URL, user agent, and an IP address. They are rotated automatically and are not used to build a profile of you.

Email delivery: the confirmation email and any list emails are sent through our email provider, which processes your address to deliver them.

Retention: signups that are still unconfirmed after 30 days are deleted by a regular cleanup in Listmonk. The cleanup first removes the unconfirmed subscription, then deletes email addresses that are no longer on any list, so the address itself is removed. Confirmed addresses are kept until you unsubscribe or the Early access list ends.

Unsubscribe, export, and delete: every email has an unsubscribe link. To get a copy of your data or have it deleted, use the contact details below.

We do not sell or share the list.

## Contact

Questions or requests: open an issue at https://github.com/raythurman2386/muninn or email raymondthurman5@gmail.com.
