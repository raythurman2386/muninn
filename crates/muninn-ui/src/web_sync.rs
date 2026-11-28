//! The browser's GitHub and Google Drive sync surface (#240 slices 8b, 9).
//!
//! The desktop's Sync tab (`sync_ui`) signs in through the OS keychain and runs
//! the engine on a worker thread. A browser has neither, so this is the web
//! twin of that surface:
//!
//! - **Two providers, one binding per vault.** GitHub authenticates with a
//!   personal access token; device flow is impossible in page JavaScript
//!   (GitHub's OAuth endpoints are not CORS-open). Google Drive authenticates
//!   through Google Identity Services' own popup with a public Web-application
//!   client id — there is no redirect page and no hosted service of ours.
//!   Tailscale stays a desktop-only door: a browser cannot reach a tailnet
//!   commit store without a relay (#240 slice 10). Like the desktop, a vault
//!   has one saved binding: connecting a provider replaces it and drops the
//!   replaced provider's stored credential.
//! - **The form and its config** are the same shapes as the desktop: owner,
//!   repository, branch, path prefix (GitHub) or a store name (Drive), one
//!   status line, and Connect / Sync now / Cancel / Disconnect. The public
//!   config is a `VaultSyncBinding` in `settings.json`, so the browser and the
//!   desktop write the same JSON.
//! - **The run** goes through `muninn_web::page_sync::spawn`, which owns the
//!   worker, the ready handshake, the flush gate and the post-run OPFS
//!   re-read. This module builds the config, relays the run's events into the
//!   status line, and re-indexes the vault UI when the run is over.
//!
//! # Where credentials live at rest
//!
//! A browser has no OS keychain, so credentials are stored in the page's state
//! home (`muninn-state/` via `StateHome`): the GitHub PAT under [`PAT_KEY`],
//! and Drive's tokens under [`DRIVE_KEY`] as the same JSON `TokenSet` shape the
//! desktop's credential store uses. Neither is ever in `settings.json` or a log
//! line. That is a deliberate departure from the desktop's keychain
//! discipline, recorded on #240: any script running on this origin, or anyone
//! with this browser profile, can read origin-private storage. The mitigations
//! are the honest ones this platform allows:
//!
//! - the PAT field is masked ([`rune::TextEdit::secure`]) and is cleared as
//!   soon as it is saved, so it is never re-displayed; Drive has no token field
//!   at all — it shows connected/not and never any token;
//! - the copies that travel to a run are zeroized after use ([`zeroize`]); the
//!   serialized worker message is dropped by the spawn session;
//! - Disconnect removes the binding and its key together;
//! - a read-only tab (another tab holds the vault's write lock) cannot write
//!   the state home, so it cannot store a credential.
//!
//! # The Drive key's name
//!
//! The plan sketches `muninn/sync/drive-<account>`. GIS's token flow discloses
//! no account label unless an identity scope is requested, and this slice
//! deliberately requests `drive.appdata` only; a browser vault also holds one
//! Drive binding. The key is therefore [`DRIVE_KEY`], and [`drive_key`] keeps
//! the `<account>` spelling ready for a later slice that has a label. Storing
//! an empty one now would be pretending to know which Google account signed
//! in.
//!
//! # One run at a time
//!
//! A second Sync now while a run is in flight is **refused** with
//! [`muninn_i18n::Key::SyncStatusAlreadyRunning`] (the desktop's policy), and
//! Disconnect refuses too: Cancel stops the run, then Disconnect clears the
//! credential. The page's run handle owns the worker, so there is never a
//! queue. A Drive sign-in is the same: Connect refuses while one popup is
//! open, and the form stays put until the popup reports back or is closed.

use std::sync::Arc;

use muninn_core::StateHome;
use muninn_i18n::{
    Key as TextKey, MsgKey, connected_as, last_sync, phrase_current, sync_conflict_copies,
    sync_conflicts, sync_credentials_unavailable, sync_dirty_buffer, sync_excluded,
    sync_file_error, sync_limit_exceeded, sync_listing_incomplete, sync_local_changed,
    sync_not_configured, sync_oauth_scope_rejected, sync_offline, sync_outcome_unknown,
    sync_permission_denied, sync_remote_changed, sync_remote_scope_not_found, t,
};
use rune::TextEdit;
use zeroize::Zeroizing;

use crate::State;
use crate::settings::VaultSyncBinding;

/// The state-home key the GitHub PAT is stored under.
///
/// The spelling follows the state home's `muninn/…` keyspace (the same one
/// `muninn/settings.json` and `muninn/recovery/…` use). It is not the worker's
/// `muninn-sync-state` directory: the worker's journals are engine state, this
/// is the page's credential. See the module docs for the at-rest tradeoff.
pub const PAT_KEY: &str = "muninn/sync/github-pat";

/// The state-home key the Drive token set is stored under.
///
/// See the module docs for why the account suffix is not used yet, and
/// [`drive_key`] for the forward shape.
pub const DRIVE_KEY: &str = "muninn/sync/drive";

/// The state-home key for one Drive account label. Empty (the slice-9 case)
/// is [`DRIVE_KEY`]; anything else is suffixed with a sanitized label, so the
/// key can never contain a `/` that would escape the `muninn/sync/` prefix.
#[must_use]
pub fn drive_key(account: &str) -> String {
    let label: String = account
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '@'))
        .take(64)
        .collect();
    if label.is_empty() {
        DRIVE_KEY.to_owned()
    } else {
        format!("{DRIVE_KEY}-{label}")
    }
}

/// The default branch a fresh GitHub form shows. The config keeps whatever is
/// saved.
pub const DEFAULT_BRANCH: &str = "main";

/// The desktop's default Drive store name (`sync_ui::DEFAULT_DRIVE_STORE`).
/// Devices that should share a vault use the same name.
pub const DEFAULT_DRIVE_STORE: &str = "Muninn";

/// Which provider the form is configuring.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WebSyncProvider {
    /// GitHub, authenticated with a personal access token.
    #[default]
    Github,
    /// Google Drive, authenticated through the GIS popup.
    Drive,
}

impl WebSyncProvider {
    /// The segmented control's index.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            WebSyncProvider::Github => 0,
            WebSyncProvider::Drive => 1,
        }
    }

    /// The index → provider mapping; an out-of-range index stays GitHub.
    #[must_use]
    pub fn from_index(index: usize) -> WebSyncProvider {
        match index {
            1 => WebSyncProvider::Drive,
            _ => WebSyncProvider::Github,
        }
    }

    /// The provider string stored in `VaultSyncBinding.provider`, the same
    /// spelling the desktop writes (`ProviderKind::as_str`).
    #[must_use]
    pub fn settings_name(self) -> &'static str {
        match self {
            WebSyncProvider::Github => "github",
            WebSyncProvider::Drive => "google_drive",
        }
    }

    /// Parse a binding's provider string. `None` for Tailscale and unknowns:
    /// the browser has no third door.
    #[must_use]
    pub fn from_settings(name: &str) -> Option<WebSyncProvider> {
        match name {
            "github" => Some(WebSyncProvider::Github),
            "google_drive" => Some(WebSyncProvider::Drive),
            _ => None,
        }
    }
}

/// Whether the Drive store name is legal, mirroring
/// `muninn_sync::DriveScope::validate`: 1–64 ASCII letters, digits, spaces,
/// `.`, `-` or `_`, with no leading or trailing whitespace.
///
/// It lives here because the browser UI does not link `muninn-sync`; the
/// worker validates the same string again against the engine's own rule, and
/// the tests below pin the two to the same accepted set.
#[must_use]
pub fn drive_store_is_valid(store: &str) -> bool {
    !store.is_empty()
        && store.len() <= 64
        && store
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ' ' | '.'))
        && store.trim() == store
}

/// Whether a sync run is in flight, or a Drive popup is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WebSyncPhase {
    /// No run and no sign-in; the form is editable.
    #[default]
    Idle,
    /// A run owns the page's vault until its terminal event.
    Running,
    /// A Drive sign-in popup is open; the run cannot start until it answers.
    Connecting,
}

impl WebSyncPhase {
    #[must_use]
    pub fn is_running(self) -> bool {
        matches!(self, WebSyncPhase::Running)
    }

    #[must_use]
    pub fn is_connecting(self) -> bool {
        matches!(self, WebSyncPhase::Connecting)
    }

    /// A run or a sign-in owns the form.
    #[must_use]
    pub fn is_busy(self) -> bool {
        self.is_running() || self.is_connecting()
    }
}

/// The Drive token set as the page received it from GIS.
///
/// The page builds this from `muninn_web::google_auth::GisToken`; the copies
/// are [`Zeroizing`], so dropping an unused set wipes the bytes. There is no
/// `Debug`: an accidental log must not be able to render a token.
pub struct WebDriveToken {
    pub access: Zeroizing<String>,
    pub refresh: Option<Zeroizing<String>>,
    pub expires_unix_ms: Option<u64>,
}

/// The JSON shape a stored Drive credential has.
///
/// It is the desktop `TokenSet`'s shape (`access_token`, `refresh_token`,
/// `expires_unix_ms`, `account`) so the two doors' state reads the same, even
/// though the browser writes it into OPFS rather than the OS keychain.
pub(crate) struct StoredDriveToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_unix_ms: Option<u64>,
    pub account: String,
}

/// Store `token` under `account`'s key and report success.
///
/// The free function [`drive_authorized`] calls this with the page's registered
/// state home; the tests call it with a `MemoryStateHome` so the JSON shape and
/// the key are pinned without a browser.
pub(crate) fn store_drive_token(
    home: &dyn StateHome,
    token: &WebDriveToken,
    account: &str,
) -> Result<(), ()> {
    // The desktop `TokenSet`'s JSON shape, so both doors' stored state reads
    // the same. `serde_json` is already a dependency; this crate does not link
    // `muninn-sync` on the browser side.
    let json = serde_json::json!({
        "access_token": token.access.as_str(),
        "refresh_token": token
            .refresh
            .as_ref()
            .map(|refresh| refresh.as_str())
            .unwrap_or(""),
        "expires_unix_ms": token.expires_unix_ms,
        "account": account,
    })
    .to_string();
    home.put(&drive_key(account), json.as_bytes())
        .map_err(|_| ())
}

/// Parse a stored Drive credential. A corrupt value is `None` (the token is
/// simply missing), never a panic.
pub(crate) fn stored_drive_token(bytes: &[u8]) -> Option<StoredDriveToken> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    Some(StoredDriveToken {
        access_token: value.get("access_token")?.as_str()?.to_owned(),
        refresh_token: value
            .get("refresh_token")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_owned(),
        expires_unix_ms: value
            .get("expires_unix_ms")
            .and_then(|value| value.as_u64()),
        account: value
            .get("account")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_owned(),
    })
}

/// The form, its status and the run phase.
pub struct WebSyncUi {
    pub provider_index: usize,
    pub owner: TextEdit,
    pub repo: TextEdit,
    pub branch: TextEdit,
    pub prefix: TextEdit,
    /// Drive's store name inside the app's hidden `appDataFolder`.
    pub drive_store: TextEdit,
    /// Masked; cleared once saved. Never loaded back into the form. Drive has
    /// no corresponding field: its sign-in is the popup.
    pub token: TextEdit,
    pub status: String,
    pub phase: WebSyncPhase,
    /// A binding is saved for this vault. Derived at reflect time from
    /// settings + the stored credential; kept for the tests and the view.
    pub connected: bool,
    /// The state home credentials are read from and written to. On the desktop
    /// this is `None` (the browser surface is never drawn or driven there).
    pub(crate) home: Option<Arc<dyn StateHome>>,
    /// The Drive binding a Connect is signing in for. It replaces the vault's
    /// current binding only once the popup succeeds, exactly as the desktop's
    /// sign-in keeps the old binding until its token is stored.
    pub(crate) pending_drive: Option<VaultSyncBinding>,
}

impl WebSyncUi {
    /// A fresh form. The page boot registers the state home before the `State`
    /// is built, so the browser captures it here; a native build captures
    /// nothing and the browser methods are inert.
    #[must_use]
    pub fn new() -> WebSyncUi {
        WebSyncUi {
            provider_index: 0,
            owner: TextEdit::new(""),
            repo: TextEdit::new(""),
            branch: TextEdit::new(DEFAULT_BRANCH),
            prefix: TextEdit::new(""),
            drive_store: TextEdit::new(DEFAULT_DRIVE_STORE),
            token: TextEdit::new("").secure(),
            status: t(TextKey::SyncWebStatusNotConnected).to_owned(),
            phase: WebSyncPhase::Idle,
            connected: false,
            home: session_home(),
            pending_drive: None,
        }
    }

    /// Install the state home a test drives directly. The browser gets its
    /// home from the boot, so this is test-only.
    #[cfg(test)]
    pub(crate) fn use_state_home(&mut self, home: Option<Arc<dyn StateHome>>) {
        self.home = home;
    }

    /// The selected provider.
    #[must_use]
    pub fn provider(&self) -> WebSyncProvider {
        WebSyncProvider::from_index(self.provider_index)
    }

    /// The stored PAT, or `None` when no token is saved.
    fn load_pat(&self) -> Option<Zeroizing<String>> {
        let bytes = self.home.as_ref()?.get(PAT_KEY)?;
        let text = String::from_utf8(bytes).ok()?;
        (!text.trim().is_empty()).then(|| Zeroizing::new(text))
    }

    /// Store `token`. The failure message never includes the token.
    fn store_pat(&self, token: &str) -> Result<(), String> {
        let unavailable = || t(TextKey::SyncWebErrorStateUnavailable).to_owned();
        let home = self.home.as_ref().ok_or_else(unavailable)?;
        home.put(PAT_KEY, token.as_bytes())
            .map_err(|_| unavailable())
    }

    fn clear_pat(&self) {
        if let Some(home) = self.home.as_ref() {
            let _ = home.remove(PAT_KEY);
        }
    }

    /// The stored Drive credential, parsed, or `None` when absent or corrupt.
    fn load_drive(&self) -> Option<StoredDriveToken> {
        let bytes = self.home.as_ref()?.get(DRIVE_KEY)?;
        stored_drive_token(&bytes)
    }

    fn clear_drive(&self) {
        if let Some(home) = self.home.as_ref() {
            let _ = home.remove(DRIVE_KEY);
        }
    }
}

impl Default for WebSyncUi {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything one browser run needs from the UI. The access token (and Drive's
/// optional refresh token) is the only secret.
///
/// No `Debug` fields can render a token: the tokens are [`Zeroizing`], so
/// dropping the config wipes them.
pub struct WebSyncConfig {
    pub provider: WebSyncProvider,
    pub binding_id: String,
    /// GitHub: owner, repo, branch, prefix. Empty for Drive.
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub prefix: String,
    /// Drive: the store name. Empty for GitHub.
    pub store: String,
    /// The GitHub PAT, or Drive's access token.
    pub access: Zeroizing<String>,
    /// Drive only: the refresh token GIS returned, when it returned one.
    pub refresh: Option<Zeroizing<String>>,
    /// Drive only: absolute Unix milliseconds when `access` expires.
    pub expires_unix_ms: Option<u64>,
}

impl std::fmt::Debug for WebSyncConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSyncConfig")
            .field("provider", &self.provider)
            .field("binding_id", &self.binding_id)
            .field("owner", &self.owner)
            .field("repo", &self.repo)
            .field("branch", &self.branch)
            .field("prefix", &self.prefix)
            .field("store", &self.store)
            .field("access", &"Secret([redacted])")
            .field(
                "refresh",
                &self.refresh.as_ref().map(|_| "Secret([redacted])"),
            )
            .field("expires_unix_ms", &self.expires_unix_ms)
            .finish()
    }
}

/// One message, as the page relays it from the worker (or from GIS).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebSyncEvent {
    /// The worker is about to make its first provider request.
    Started,
    /// The run converged. `head` is the published commit.
    Completed {
        head: String,
        exclusions: Vec<String>,
        conflict_copies: Vec<String>,
    },
    /// The run stopped on conflicts. With keep-both this should not happen;
    /// it is surfaced honestly if it ever does.
    Conflicts { paths: Vec<String> },
    /// The run stopped. `code` is `muninn_sync::SyncError::code` (or one of
    /// the page's own `worker`/`busy`/`cancelled`), `detail` the diagnostic.
    Failed { code: String, detail: String },
    /// GIS finished and the page stored the tokens. No token is in the event.
    DriveSignedIn { account: String },
    /// The popup was closed, Google refused, or the response was unusable.
    DriveSignInFailed,
    /// The popup succeeded but the tokens could not be saved (a read-only tab
    /// or unavailable storage).
    DriveStorageFailed,
}

impl WebSyncEvent {
    /// Exactly one of `Completed`, `Conflicts` and `Failed` ends a run. The
    /// Drive sign-in events are about the form, not a run.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            WebSyncEvent::Completed { .. }
                | WebSyncEvent::Conflicts { .. }
                | WebSyncEvent::Failed { .. }
        )
    }
}

/// The browser callbacks the surface needs. `muninn-web` registers them at
/// boot; a native test leaves them absent, so [`available`] is false and Sync
/// now reports the honest "no worker in this build" status.
#[derive(Clone, Copy, Debug)]
pub struct WebSyncHooks {
    /// Spawn one run. The page relays its events through [`enqueue`].
    pub start: fn(WebSyncConfig),
    /// Stop the run in flight.
    pub cancel: fn(),
    /// Open Google Identity Services' popup for a Drive sign-in. The page
    /// reports the outcome through [`drive_authorized`] and [`WebSyncEvent`].
    pub drive_connect: fn(),
    /// Whether this build embeds a public Web client id. The page answers from
    /// `muninn_web::google_auth`.
    pub drive_configured: fn() -> bool,
}

static HOOKS: std::sync::OnceLock<WebSyncHooks> = std::sync::OnceLock::new();

thread_local! {
    /// Events the page has relayed and the UI has not drawn yet. A thread-local
    /// is right: the UI state is `!Send` and the browser is single-threaded.
    static PENDING: std::cell::RefCell<Vec<WebSyncEvent>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Register this session's browser callbacks. The first registration wins; a
/// host boots once.
pub fn install(hooks: WebSyncHooks) {
    let _ = HOOKS.set(hooks);
}

/// Whether a browser host can start a run in this session.
#[must_use]
pub fn available() -> bool {
    HOOKS.get().is_some()
}

/// Whether this build can open a Google sign-in, as the page reports it.
#[must_use]
pub fn drive_configured() -> bool {
    HOOKS.get().is_some_and(|hooks| (hooks.drive_configured)())
}

/// Queue one relayed event for the next [`poll`].
pub fn enqueue(event: WebSyncEvent) {
    PENDING.with(|slot| slot.borrow_mut().push(event));
}

/// Store the tokens GIS returned, then queue the outcome for the form.
///
/// Called by the page from the GIS callback (or its error callback, via
/// [`drive_sign_in_failed`]). The token never crosses into an event.
pub fn drive_authorized(token: WebDriveToken, account: &str) {
    let stored = session_home()
        .as_deref()
        .is_some_and(|home| store_drive_token(home, &token, account).is_ok());
    enqueue(if stored {
        WebSyncEvent::DriveSignedIn {
            account: account.to_owned(),
        }
    } else {
        WebSyncEvent::DriveStorageFailed
    });
}

/// Report that the popup did not produce a usable token.
pub fn drive_sign_in_failed() {
    enqueue(WebSyncEvent::DriveSignInFailed);
}

/// Drain relayed events into `state`. Returns whether anything changed.
pub fn poll(state: &mut State) -> bool {
    let events: Vec<WebSyncEvent> = PENDING.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
    let mut changed = false;
    for event in events {
        changed |= state.apply_web_sync(event);
    }
    changed
}

/// A stable binding id for a browser vault and one provider's identity fields.
///
/// The engine's journals, base and sha cache are keyed by binding id, so the
/// id must be **the same on every run**. A browser vault has no filesystem
/// path, so its label (the store's root) stands in for one, and the provider
/// plus its identity fields are folded in so switching provider or remote
/// starts a distinct binding rather than reading the old one's journal against
/// a different remote.
///
/// Two independent FNV-1a passes give 32 hex characters, inside
/// `muninn_sync::BindingId`'s 8..=80 hex-or-dash contract. This is an
/// identifier, not a secret: it names a state-home directory, and it is not
/// used for authentication.
#[must_use]
pub fn binding_id_for(label: &str, provider: &str, identity: &[&str]) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    const SEP: u8 = 0x1f;
    let pass = |seed: u64, parts: &[&str]| {
        let mut hash = seed;
        for part in parts {
            for byte in part.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(PRIME);
            }
            hash ^= u64::from(SEP);
            hash = hash.wrapping_mul(PRIME);
        }
        hash
    };
    let mut parts = Vec::with_capacity(identity.len() + 2);
    parts.push(label);
    parts.push(provider);
    parts.extend_from_slice(identity);
    let first = pass(OFFSET, &parts);
    // A different seed over the reversed parts keeps both halves independent
    // of each other, so two 64-bit collisions would have to coincide.
    parts.reverse();
    let second = pass(seed(), &parts);
    format!("{first:016x}{second:016x}")
}

/// The GitHub binding id: owner and repository are ASCII-lowercased because
/// GitHub names are case-insensitive.
#[must_use]
pub fn github_binding_id_for(label: &str, owner: &str, repo: &str) -> String {
    let owner = owner.to_ascii_lowercase();
    let repo = repo.to_ascii_lowercase();
    binding_id_for(label, "github", &[owner.as_str(), repo.as_str()])
}

/// The Drive binding id: the store name is case-sensitive, because Drive's
/// folder names are.
#[must_use]
pub fn drive_binding_id_for(label: &str, store: &str) -> String {
    binding_id_for(label, "google_drive", &[store])
}

/// The second pass's seed. Kept in a function so the constant reads as a seed,
/// not as a second offset basis that looks accidentally duplicated.
const fn seed() -> u64 {
    0x9e37_79b9_7f4a_7c15
}

/// The status line for a run failure, in the UI's words.
///
/// `muninn_sync::SyncError::code` is the stable machine string the worker
/// sends; the sentences are the catalog's. Each keeps the provider's `detail`,
/// path or status as written. Codes this build does not know, and the
/// developer-facing ones, keep the engine's rendering inside a translated
/// frame rather than showing raw English as the whole line.
fn failure_text(code: &str, detail: &str) -> String {
    match code {
        "offline" => sync_offline(detail),
        "timeout_mutation" => t(TextKey::SyncErrorTimedOutAfterSend).to_owned(),
        "timeout" => t(TextKey::SyncErrorTimedOutBeforeResponse).to_owned(),
        "cancelled" => t(TextKey::SyncErrorCancelled).to_owned(),
        "rate_limited" => t(TextKey::SyncErrorRateLimited).to_owned(),
        "auth" => muninn_i18n::sync_authentication_needed(detail),
        "permission" => sync_permission_denied(detail),
        "scope" => sync_oauth_scope_rejected(detail),
        "not_found" => sync_remote_scope_not_found(detail),
        "remote_changed" => sync_remote_changed(detail),
        "outcome_unknown" => sync_outcome_unknown(detail),
        "listing_incomplete" => sync_listing_incomplete(detail),
        "limit" => sync_limit_exceeded(detail),
        "credentials" => sync_credentials_unavailable(detail),
        "lease_busy" => t(TextKey::SyncErrorLeaseBusy).to_owned(),
        "dirty_buffer" => sync_dirty_buffer(detail),
        "local_changed" => sync_local_changed(detail),
        "config" => sync_not_configured(detail),
        "io" => sync_file_error(detail),
        "worker" => t(TextKey::SyncWebErrorWorker).to_owned(),
        "busy" => t(TextKey::SyncWebErrorBusy).to_owned(),
        // `server`, `unsupported`, `path`, `inventory`, `journal`, `protocol`,
        // `injected_fault` and any future code: the worker's rendering is a
        // diagnostic, so keep it as data inside a translated frame.
        _ => phrase_current(MsgKey::SyncWebFailed, &[detail]),
    }
}

/// Every failure code the page can draw, kept for the headless test that pins
/// each one to copy. The engine's list mirrors `muninn_sync::SyncError::code`;
/// the last three are the page's and the worker's own.
pub const FAILURE_CODES: &[&str] = &[
    "offline",
    "timeout_mutation",
    "timeout",
    "cancelled",
    "rate_limited",
    "server",
    "auth",
    "permission",
    "scope",
    "not_found",
    "remote_changed",
    "outcome_unknown",
    "listing_incomplete",
    "unsupported",
    "path",
    "inventory",
    "limit",
    "credentials",
    "lease_busy",
    "dirty_buffer",
    "local_changed",
    "journal",
    "protocol",
    "injected_fault",
    "config",
    "io",
    "worker",
    "busy",
];

impl State {
    /// The browser Sync tab's status line.
    #[must_use]
    pub fn web_sync_status(&self) -> &str {
        &self.web_sync.status
    }

    /// The vault label this browser vault uses as its identity: the store's
    /// root. It is the same on every run, which is what makes the derived
    /// binding id stable.
    fn web_sync_identity(&self) -> String {
        self.vault.root.to_string_lossy().into_owned()
    }

    /// This vault's saved binding, whatever provider it names. The desktop
    /// keeps one binding per vault, and the browser writes the same shape:
    /// connecting the other provider replaces it.
    fn web_sync_binding(&self) -> Option<&VaultSyncBinding> {
        let identity = self.web_sync_identity();
        self.settings
            .sync
            .vaults
            .iter()
            .find(|binding| binding.identity == identity)
    }

    /// The saved binding's provider, when this host has a door for it.
    fn web_sync_binding_provider(&self) -> Option<WebSyncProvider> {
        self.web_sync_binding()
            .and_then(|binding| WebSyncProvider::from_settings(&binding.provider))
    }

    /// The status a freshly selected provider shows when nothing is connected.
    fn provider_idle_status(&self) -> String {
        match self.web_sync.provider() {
            WebSyncProvider::Github => t(TextKey::SyncWebStatusNotConnected).to_owned(),
            WebSyncProvider::Drive => {
                if drive_configured() {
                    t(TextKey::SyncWebDriveNotConnected).to_owned()
                } else {
                    t(TextKey::SyncWebDriveUnconfigured).to_owned()
                }
            }
        }
    }

    /// Rebuild the form from this vault's saved binding and its stored
    /// credential.
    ///
    /// Startup calls this after the settings and the state home are in hand. A
    /// saved remote fills the fields; a saved token is reported as connected,
    /// never loaded into the form. A run or sign-in in flight owns the form and
    /// is left alone.
    pub fn reflect_web_sync(&mut self) {
        if self.web_sync.phase.is_busy() {
            return;
        }
        let Some(binding) = self.web_sync_binding().cloned() else {
            self.web_sync.connected = false;
            self.web_sync.status = self.idle_status_for_selected();
            return;
        };
        let Some(provider) = WebSyncProvider::from_settings(&binding.provider) else {
            // A Tailscale binding from the desktop is not this host's door.
            self.web_sync.connected = false;
            self.web_sync.status = self.idle_status_for_selected();
            return;
        };
        self.web_sync.provider_index = provider.index();
        match provider {
            WebSyncProvider::Github => {
                self.web_sync.owner.replace_text(&binding.github_owner);
                self.web_sync.repo.replace_text(&binding.github_repo);
                self.web_sync
                    .branch
                    .replace_text(if binding.github_branch.is_empty() {
                        DEFAULT_BRANCH
                    } else {
                        &binding.github_branch
                    });
                self.web_sync.prefix.replace_text(&binding.github_prefix);
                let saved = self.web_sync.load_pat().is_some();
                self.web_sync.connected = saved;
                self.web_sync.status = if saved {
                    t(TextKey::SyncWebStatusConnected)
                } else {
                    t(TextKey::SyncWebErrorTokenMissing)
                }
                .to_owned();
            }
            WebSyncProvider::Drive => {
                self.web_sync
                    .drive_store
                    .replace_text(if binding.drive_store.is_empty() {
                        DEFAULT_DRIVE_STORE
                    } else {
                        &binding.drive_store
                    });
                let saved = self.web_sync.load_drive();
                self.web_sync.connected = saved.is_some();
                self.web_sync.status = match &saved {
                    // A later slice may store an account label; when one is
                    // there, say who, as the desktop does.
                    Some(stored) if !stored.account.is_empty() => connected_as(&stored.account),
                    Some(_) => t(TextKey::SyncWebDriveConnected).to_owned(),
                    None => t(TextKey::SyncWebDriveTokenMissing).to_owned(),
                };
            }
        }
    }

    /// Whether the selected provider has both a saved binding and its
    /// credential.
    fn selected_provider_connected(&self) -> bool {
        if self.web_sync_binding_provider() != Some(self.web_sync.provider()) {
            return false;
        }
        match self.web_sync.provider() {
            WebSyncProvider::Github => self.web_sync.load_pat().is_some(),
            WebSyncProvider::Drive => self.web_sync.load_drive().is_some(),
        }
    }

    /// The selected provider's idle status.
    fn idle_status_for_selected(&self) -> String {
        if self.selected_provider_connected() {
            return match self.web_sync.provider() {
                WebSyncProvider::Github => t(TextKey::SyncWebStatusConnected).to_owned(),
                WebSyncProvider::Drive => t(TextKey::SyncWebDriveConnected).to_owned(),
            };
        }
        self.provider_idle_status()
    }

    /// Switch the segmented control. The saved binding is not touched: it is
    /// replaced only by a successful Connect, exactly as on the desktop.
    pub fn set_web_sync_provider(&mut self, index: usize) {
        if self.web_sync.phase.is_busy() {
            self.web_sync.status = t(TextKey::SyncStatusWaitForRun).to_owned();
            return;
        }
        self.web_sync.provider_index = WebSyncProvider::from_index(index).index();
        self.web_sync.connected = self.selected_provider_connected();
        self.web_sync.status = self.idle_status_for_selected();
    }

    /// Validate the GitHub form, save the public config in settings and the PAT
    /// in the state home. Nothing reaches the provider: the browser's PAT has
    /// no sign-in step, and the first run reports an `auth` failure if the
    /// token is wrong.
    pub fn connect_web_sync(&mut self) {
        if self.web_sync.phase.is_busy() {
            self.web_sync.status = t(TextKey::SyncStatusWaitForRun).to_owned();
            return;
        }
        match self.web_sync.provider() {
            WebSyncProvider::Github => self.connect_web_sync_github(),
            WebSyncProvider::Drive => self.connect_web_sync_drive(),
        }
    }

    fn connect_web_sync_github(&mut self) {
        let owner = self.web_sync.owner.text.trim().to_owned();
        let repo = self.web_sync.repo.text.trim().to_owned();
        let branch = self.web_sync.branch.text.trim().to_owned();
        let prefix = self
            .web_sync
            .prefix
            .text
            .trim()
            .trim_matches('/')
            .to_owned();
        if owner.is_empty() || repo.is_empty() || owner.contains('/') || repo.contains('/') {
            self.web_sync.status = t(TextKey::SyncErrorGithubScope).to_owned();
            return;
        }
        if branch.is_empty() {
            self.web_sync.status = t(TextKey::SyncWebErrorBranch).to_owned();
            return;
        }
        let typed = self.web_sync.token.text.trim().to_owned();
        let token = if typed.is_empty() {
            let Some(saved) = self.web_sync.load_pat() else {
                self.web_sync.status = t(TextKey::SyncWebErrorTokenMissing).to_owned();
                return;
            };
            saved
        } else {
            if typed.split_whitespace().count() != 1 {
                // The field keeps the pasted value (masked) so it can be fixed;
                // a malformed token is never saved.
                self.web_sync.status = t(TextKey::SyncWebErrorTokenShape).to_owned();
                return;
            }
            let typed = Zeroizing::new(typed);
            if let Err(error) = self.web_sync.store_pat(&typed) {
                self.web_sync.status = error;
                return;
            }
            typed
        };
        // The field is cleared through `replace_text`, which zeroizes the old
        // buffer and drops the edit history: a saved PAT is never shown again,
        // and the secure field leaves nothing for a later frame to reveal.
        self.web_sync.token.replace_text("");
        let identity = self.web_sync_identity();
        let binding_id = github_binding_id_for(&identity, &owner, &repo);
        let previous = self.web_sync_binding().cloned();
        let (last_success_unix_ms, last_head) = match previous {
            Some(previous) if previous.binding_id == binding_id => {
                (previous.last_success_unix_ms, previous.last_head)
            }
            _ => (None, String::new()),
        };
        self.replace_web_binding(VaultSyncBinding {
            identity,
            provider: "github".to_owned(),
            binding_id,
            account: String::new(),
            github_owner: owner,
            github_repo: repo,
            github_branch: branch,
            github_prefix: prefix,
            drive_store: String::new(),
            tailnet_role: String::new(),
            tailnet_host: String::new(),
            tailnet_port: 0,
            tailnet_allowed: String::new(),
            last_success_unix_ms,
            last_head,
        });
        drop(token);
        self.web_sync.connected = true;
        self.web_sync.status = t(TextKey::SyncWebStatusConnected).to_owned();
    }

    /// Prepare the Drive binding and open GIS. The binding replaces the vault's
    /// current one only once the popup reports back, so a closed popup leaves
    /// the old binding and its credential working, exactly like the desktop's
    /// sign-in.
    fn connect_web_sync_drive(&mut self) {
        if !drive_configured() {
            // The honest unconfigured state: say what is needed and do nothing
            // else. No popup, no settings write, no request.
            self.web_sync.status = t(TextKey::SyncWebDriveUnconfigured).to_owned();
            return;
        }
        let store = self.web_sync.drive_store.text.trim().to_owned();
        if !drive_store_is_valid(&store) {
            self.web_sync.status = t(TextKey::SyncWebDriveStoreShape).to_owned();
            return;
        }
        let Some(hooks) = HOOKS.get().copied() else {
            self.web_sync.status = t(TextKey::SyncWebErrorUnavailable).to_owned();
            return;
        };
        let identity = self.web_sync_identity();
        let binding_id = drive_binding_id_for(&identity, &store);
        let previous = self.web_sync_binding().cloned();
        let (last_success_unix_ms, last_head) = match previous {
            Some(previous) if previous.binding_id == binding_id => {
                (previous.last_success_unix_ms, previous.last_head)
            }
            _ => (None, String::new()),
        };
        self.web_sync.pending_drive = Some(VaultSyncBinding {
            identity,
            provider: "google_drive".to_owned(),
            binding_id,
            account: String::new(),
            github_owner: String::new(),
            github_repo: String::new(),
            github_branch: String::new(),
            github_prefix: String::new(),
            drive_store: store,
            tailnet_role: String::new(),
            tailnet_host: String::new(),
            tailnet_port: 0,
            tailnet_allowed: String::new(),
            last_success_unix_ms,
            last_head,
        });
        self.web_sync.phase = WebSyncPhase::Connecting;
        self.web_sync.status = t(TextKey::SyncWebDriveSigningIn).to_owned();
        (hooks.drive_connect)();
    }

    /// Replace this vault's binding, clearing the credential of a provider it
    /// replaced. The desktop's `swap_binding` does the same for its keychain.
    fn replace_web_binding(&mut self, binding: VaultSyncBinding) {
        let identity = binding.identity.clone();
        let replaced = self
            .settings
            .sync
            .vaults
            .iter()
            .find(|known| known.identity == identity)
            .map(|known| known.provider.clone());
        self.settings
            .sync
            .vaults
            .retain(|known| known.identity != identity);
        let provider = WebSyncProvider::from_settings(&binding.provider);
        self.settings.sync.vaults.push(binding);
        self.save_settings();
        // Clearing the replaced provider's credential is the desktop swap's
        // rule. A same-provider reconnect must not clear the credential it
        // just stored (the browser keys one credential per provider, not per
        // binding id).
        match replaced.as_deref().and_then(WebSyncProvider::from_settings) {
            Some(WebSyncProvider::Github) if provider != Some(WebSyncProvider::Github) => {
                self.web_sync.clear_pat();
            }
            Some(WebSyncProvider::Drive) if provider != Some(WebSyncProvider::Drive) => {
                self.web_sync.clear_drive();
            }
            _ => {}
        }
    }

    /// Snapshot the saved config + credential into one run's [`WebSyncConfig`].
    fn web_sync_config(&self) -> Result<WebSyncConfig, String> {
        let binding = self
            .web_sync_binding()
            .ok_or_else(|| t(TextKey::SyncStatusConnectProvider).to_owned())?;
        match WebSyncProvider::from_settings(&binding.provider) {
            Some(WebSyncProvider::Github) => {
                let token = self
                    .web_sync
                    .load_pat()
                    .ok_or_else(|| t(TextKey::SyncWebErrorTokenMissing).to_owned())?;
                Ok(WebSyncConfig {
                    provider: WebSyncProvider::Github,
                    binding_id: binding.binding_id.clone(),
                    owner: binding.github_owner.clone(),
                    repo: binding.github_repo.clone(),
                    branch: binding.github_branch.clone(),
                    prefix: binding.github_prefix.clone(),
                    store: String::new(),
                    access: token,
                    refresh: None,
                    expires_unix_ms: None,
                })
            }
            Some(WebSyncProvider::Drive) => {
                if binding.drive_store.trim().is_empty() {
                    return Err(t(TextKey::SyncWebDriveStoreMissing).to_owned());
                }
                let stored = self
                    .web_sync
                    .load_drive()
                    .ok_or_else(|| t(TextKey::SyncWebDriveTokenMissing).to_owned())?;
                if stored.access_token.trim().is_empty() {
                    return Err(t(TextKey::SyncWebDriveTokenMissing).to_owned());
                }
                Ok(WebSyncConfig {
                    provider: WebSyncProvider::Drive,
                    binding_id: binding.binding_id.clone(),
                    owner: String::new(),
                    repo: String::new(),
                    branch: String::new(),
                    prefix: String::new(),
                    store: binding.drive_store.clone(),
                    access: Zeroizing::new(stored.access_token),
                    refresh: (!stored.refresh_token.is_empty())
                        .then(|| Zeroizing::new(stored.refresh_token)),
                    expires_unix_ms: stored.expires_unix_ms,
                })
            }
            None => Err(t(TextKey::SyncStatusConnectProvider).to_owned()),
        }
    }

    /// Start one run through the registered browser host.
    ///
    /// Refused while the open note holds unsaved text or a conflict (nothing
    /// unpublished would be uploaded, and the guard keeps the run's re-index
    /// from overwriting a buffer), while a run or sign-in is in flight, and
    /// when no browser host is registered.
    pub fn web_sync_now(&mut self) {
        if self.dirty() || self.conflict.is_some() {
            self.web_sync.status = t(TextKey::SyncStatusSaveBeforeSync).to_owned();
            return;
        }
        if self.web_sync.phase.is_busy() {
            self.web_sync.status = t(TextKey::SyncStatusAlreadyRunning).to_owned();
            return;
        }
        let config = match self.web_sync_config() {
            Ok(config) => config,
            Err(error) => {
                self.web_sync.status = error;
                return;
            }
        };
        let Some(hooks) = HOOKS.get().copied() else {
            self.web_sync.status = t(TextKey::SyncWebErrorUnavailable).to_owned();
            return;
        };
        (hooks.start)(config);
        self.web_sync.phase = WebSyncPhase::Running;
        self.web_sync.status = t(TextKey::SyncStatusSyncStarted).to_owned();
    }

    /// Stop the run in flight. The page drops its handle; see
    /// `muninn_web::page_sync` for why termination is the immediate and safe
    /// cancel here.
    pub fn cancel_web_sync(&mut self) {
        if !self.web_sync.phase.is_running() {
            return;
        }
        if let Some(hooks) = HOOKS.get().copied() {
            (hooks.cancel)();
        }
        self.web_sync.status = t(TextKey::SyncStatusCancelling).to_owned();
    }

    /// Remove the binding and its saved credential. Local notes are untouched.
    ///
    /// Refused while a run or sign-in is in flight: Cancel stops the run first,
    /// so the credential a run may still be using is never yanked mid-flight.
    pub fn disconnect_web_sync(&mut self) {
        if self.web_sync.phase.is_busy() {
            self.web_sync.status = t(TextKey::SyncStatusWaitForRun).to_owned();
            return;
        }
        self.web_sync.pending_drive = None;
        let identity = self.web_sync_identity();
        let removed = self
            .settings
            .sync
            .vaults
            .iter()
            .find(|binding| binding.identity == identity)
            .and_then(|binding| WebSyncProvider::from_settings(&binding.provider));
        self.settings
            .sync
            .vaults
            .retain(|binding| binding.identity != identity);
        self.save_settings();
        // Clear the binding's provider credential; with no binding, the
        // selected provider's, so a half-finished form cannot leave one.
        match removed.unwrap_or_else(|| self.web_sync.provider()) {
            WebSyncProvider::Github => self.web_sync.clear_pat(),
            WebSyncProvider::Drive => self.web_sync.clear_drive(),
        }
        self.web_sync.owner.replace_text("");
        self.web_sync.repo.replace_text("");
        self.web_sync.branch.replace_text(DEFAULT_BRANCH);
        self.web_sync.prefix.replace_text("");
        self.web_sync.drive_store.replace_text(DEFAULT_DRIVE_STORE);
        self.web_sync.token.replace_text("");
        self.web_sync.connected = false;
        self.web_sync.status = t(TextKey::SyncWebStatusDisconnected).to_owned();
    }

    /// Apply one relayed event. Returns whether anything changed.
    ///
    /// Every terminal run event re-indexes the vault: the worker may have
    /// pulled, replaced or deleted files (or a failure may have applied part
    /// of a plan), and the page's OPFS re-read has already made the store
    /// mirror current when the page hands the event over. A clean buffer is
    /// reloaded through the store, so the note list and the open note show the
    /// pulled bytes. A dirty buffer keeps its text and only the index is
    /// refreshed, with the honest "changed while sync ran" note.
    pub fn apply_web_sync(&mut self, event: WebSyncEvent) -> bool {
        match event {
            WebSyncEvent::Started => {
                self.web_sync.phase = WebSyncPhase::Running;
                self.web_sync.status = t(TextKey::SyncStatusSyncStarted).to_owned();
                true
            }
            WebSyncEvent::Completed {
                head,
                exclusions,
                conflict_copies,
            } => {
                self.web_sync.phase = WebSyncPhase::Idle;
                let identity = self.web_sync_identity();
                if let Some(binding) = self
                    .settings
                    .sync
                    .vaults
                    .iter_mut()
                    .find(|binding| binding.identity == identity)
                {
                    binding.last_head.clone_from(&head);
                    binding.last_success_unix_ms = Some(now_unix_ms(self.web_sync.home.as_deref()));
                }
                self.save_settings();
                let mut status = last_sync(&head);
                if !exclusions.is_empty() {
                    status.push(' ');
                    status.push_str(&sync_excluded(exclusions.len()));
                }
                if !conflict_copies.is_empty() {
                    let names: Vec<&str> = conflict_copies.iter().map(String::as_str).collect();
                    status.push(' ');
                    status.push_str(&sync_conflict_copies(&names.join(", ")));
                }
                self.web_sync.status = status;
                self.reindex_after_web_sync();
                true
            }
            WebSyncEvent::Conflicts { paths } => {
                self.web_sync.phase = WebSyncPhase::Idle;
                // Keep-both is the request's default, so this is the honest
                // report of a run that stopped instead of resolving.
                let names: Vec<&str> = paths.iter().map(String::as_str).collect();
                self.web_sync.status = sync_conflicts(paths.len(), &names.join(", "));
                self.reindex_after_web_sync();
                true
            }
            WebSyncEvent::Failed { code, detail } => {
                self.web_sync.phase = WebSyncPhase::Idle;
                self.web_sync.status = failure_text(&code, &detail);
                self.reindex_after_web_sync();
                true
            }
            WebSyncEvent::DriveSignedIn { account } => {
                self.web_sync.phase = WebSyncPhase::Idle;
                let pending = self.web_sync.pending_drive.take();
                match pending {
                    Some(mut binding) => {
                        binding.account = account;
                        self.replace_web_binding(binding);
                        self.web_sync.connected = true;
                        self.web_sync.status = t(TextKey::SyncWebDriveConnected).to_owned();
                    }
                    // No Connect in flight (a stray event): report the
                    // connected copy only if the saved binding really is Drive.
                    None => {
                        self.web_sync.connected =
                            self.web_sync_binding_provider() == Some(WebSyncProvider::Drive);
                        self.web_sync.status = t(TextKey::SyncWebDriveConnected).to_owned();
                    }
                }
                true
            }
            WebSyncEvent::DriveSignInFailed => {
                // A closed popup leaves the old binding and credential alone;
                // only the pending replacement is dropped.
                self.web_sync.phase = WebSyncPhase::Idle;
                self.web_sync.pending_drive = None;
                self.web_sync.connected = self.selected_provider_connected();
                self.web_sync.status = t(TextKey::SyncWebDriveSignInFailed).to_owned();
                true
            }
            WebSyncEvent::DriveStorageFailed => {
                self.web_sync.phase = WebSyncPhase::Idle;
                self.web_sync.pending_drive = None;
                self.web_sync.connected = self.selected_provider_connected();
                self.web_sync.status = t(TextKey::SyncWebErrorStateUnavailable).to_owned();
                true
            }
        }
    }

    /// Re-read the vault through its store after a run.
    fn reindex_after_web_sync(&mut self) {
        if self.dirty() || self.conflict.is_some() {
            self.refresh_index_only();
            self.web_sync.status.push(' ');
            self.web_sync
                .status
                .push_str(t(TextKey::SyncStatusNoteChangedWhileSyncing));
        } else {
            let select = self.selected_path.clone();
            self.reload(select);
        }
    }
}

/// Unix milliseconds for a binding's last-success stamp.
///
/// `std`'s clock is not implemented on `wasm32-unknown-unknown`; the browser's
/// state home answers from `Date.now()` instead. A native test (and the
/// desktop's inert copy of this form) falls back to `SystemTime`.
fn now_unix_ms(home: Option<&dyn StateHome>) -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        home.map_or(0, |home| home.now_secs()) * 1000
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = home;
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// The page's registered state home, when this build has one. The desktop's
/// `State` carries an inert form with no home.
#[cfg(not(feature = "native"))]
fn session_home() -> Option<Arc<dyn StateHome>> {
    crate::state_home::registered()
}

#[cfg(feature = "native")]
fn session_home() -> Option<Arc<dyn StateHome>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Settings;
    use muninn_core::testing::TempVault;
    use muninn_core::{MemoryStateHome, MemoryStore};

    /// A state over an in-memory store: the browser's shape without OPFS. The
    /// fixture folder removes itself on drop, so no test touches a real
    /// settings or state directory.
    fn state_with_store() -> (State, Arc<MemoryStore>, TempVault) {
        let vault = TempVault::new(&[("A.md", "# A\n")]);
        let root = vault.path().to_path_buf();
        let mut state = State::with_settings(
            root.clone(),
            root.with_extension("state"),
            root.with_extension("config"),
            Settings::default(),
        );
        let store = Arc::new(MemoryStore::new(root));
        store.put_file("A.md", b"# A\n");
        state.install_store(store.clone()).unwrap();
        (state, store, vault)
    }

    fn drive_token(access: &str, refresh: Option<&str>, expires: Option<u64>) -> WebDriveToken {
        WebDriveToken {
            access: Zeroizing::new(access.to_owned()),
            refresh: refresh.map(|token| Zeroizing::new(token.to_owned())),
            expires_unix_ms: expires,
        }
    }

    #[test]
    fn the_form_follows_the_tailnet_masked_field() {
        let ui = WebSyncUi::new();
        assert!(ui.token.is_secure(), "the PAT is masked while typed");
        assert_eq!(ui.branch.text, DEFAULT_BRANCH);
        assert_eq!(ui.drive_store.text, DEFAULT_DRIVE_STORE);
        assert_eq!(ui.status, t(TextKey::SyncWebStatusNotConnected));
        assert_eq!(ui.provider(), WebSyncProvider::Github);
    }

    #[test]
    fn the_provider_control_has_two_options_and_keeps_github_by_default() {
        assert_eq!(WebSyncProvider::from_index(0), WebSyncProvider::Github);
        assert_eq!(WebSyncProvider::from_index(1), WebSyncProvider::Drive);
        assert_eq!(
            WebSyncProvider::from_index(9),
            WebSyncProvider::Github,
            "the browser has no third provider"
        );
        assert_eq!(WebSyncProvider::Github.index(), 0);
        assert_eq!(WebSyncProvider::Drive.index(), 1);
        assert_eq!(WebSyncProvider::Github.settings_name(), "github");
        assert_eq!(WebSyncProvider::Drive.settings_name(), "google_drive");
        assert_eq!(
            WebSyncProvider::from_settings("tailscale"),
            None,
            "Tailscale stays a desktop door"
        );
        assert_eq!(
            WebSyncProvider::from_settings("google_drive"),
            Some(WebSyncProvider::Drive)
        );
    }

    #[test]
    fn the_binding_id_is_stable_and_provider_specific() {
        let label = "/Muninn sample";
        let github = github_binding_id_for(label, "octocat", "notes");
        assert_eq!(
            github_binding_id_for(label, "OctoCat", "NOTES"),
            github,
            "GitHub names are case-insensitive, so the id must be too"
        );
        assert_eq!(
            github_binding_id_for(label, "octocat", "notes"),
            github,
            "the same vault and repository derive the same id on every run"
        );
        assert_ne!(github_binding_id_for(label, "octocat", "other"), github);
        assert_ne!(github_binding_id_for(label, "other", "notes"), github);
        assert_ne!(
            github_binding_id_for("/another vault", "octocat", "notes"),
            github
        );

        let drive = drive_binding_id_for(label, "Muninn");
        assert_eq!(
            drive_binding_id_for(label, "Muninn"),
            drive,
            "stable per vault and store on every run"
        );
        assert_ne!(
            drive_binding_id_for(label, "muninn"),
            drive,
            "Drive store names are case-sensitive, so the id is too"
        );
        assert_ne!(drive_binding_id_for(label, "Another"), drive);
        assert_ne!(
            github, drive,
            "the provider is part of the identity, so the journals cannot collide"
        );
        for id in [&github, &drive] {
            assert_eq!(id.len(), 32);
            assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
            // `BindingId::parse`'s contract: 8..=80 hex characters.
            assert!((8..=80).contains(&id.len()));
        }
    }

    #[test]
    fn the_drive_store_rule_mirrors_the_engines() {
        for valid in ["Muninn", "a", "a b", "a.b-c_d", &"a".repeat(64)] {
            assert!(drive_store_is_valid(valid), "{valid:?} is a legal store");
        }
        for invalid in [
            "",
            " ",
            " Muninn",
            "Muninn ",
            "a/b",
            "a\u{00e9}",
            &"a".repeat(65),
        ] {
            assert!(!drive_store_is_valid(invalid), "{invalid:?} is illegal");
        }
        // The engine's own validator is the authority; these are the same
        // strings it accepts and refuses.
        for (store, ok) in [
            ("Muninn", true),
            ("a b", true),
            ("a.b-c_d", true),
            // The store name becomes one Drive folder *name*, never a path,
            // so the engine accepts dots on their own; only the path separator
            // and the length/whitespace rules are refusals.
            ("..", true),
            ("a/b", false),
            (" Muninn", false),
        ] {
            let engine = muninn_sync::DriveScope {
                store: store.to_owned(),
            }
            .validate()
            .is_ok();
            assert_eq!(engine, ok, "engine and UI disagree about {store:?}");
            assert_eq!(
                drive_store_is_valid(store),
                ok,
                "UI and engine disagree about {store:?}"
            );
        }
    }

    /// Install `home` and fill the GitHub form the way a person would, without
    /// connecting; each test decides whether to Connect.
    fn fill_form(state: &mut State, home: &Arc<MemoryStateHome>, token: &str) {
        state
            .web_sync
            .use_state_home(Some(Arc::clone(home) as Arc<dyn StateHome>));
        state.web_sync.owner.replace_text("octocat");
        state.web_sync.repo.replace_text("notes");
        state.web_sync.token.replace_text(token);
    }

    #[test]
    fn connect_saves_the_public_config_and_the_token_in_separate_places() {
        let (mut state, _store, _vault) = state_with_store();
        let home = Arc::new(MemoryStateHome::new());
        fill_form(&mut state, &home, "ghp_example_token");
        state.web_sync.prefix.replace_text("vault");

        state.connect_web_sync();

        assert!(state.web_sync.connected);
        assert_eq!(state.web_sync.status, t(TextKey::SyncWebStatusConnected));
        assert_eq!(state.web_sync.token.text, "", "the field is cleared");
        let binding = state.web_sync_binding().expect("the config is in settings");
        assert_eq!(binding.github_owner, "octocat");
        assert_eq!(binding.github_repo, "notes");
        assert_eq!(binding.github_branch, "main");
        assert_eq!(binding.github_prefix, "vault");
        assert_eq!(
            binding.binding_id,
            github_binding_id_for(&state.web_sync_identity(), "octocat", "notes")
        );
        assert_eq!(
            home.get(PAT_KEY),
            Some(b"ghp_example_token".to_vec()),
            "the token lives in the state home, not settings"
        );
        // `VaultSyncBinding` has no token field at all: the public config's
        // shape cannot carry the secret.
        assert_eq!(state.settings.sync.vaults.len(), 1);
    }

    #[test]
    fn a_saved_token_is_enough_to_connect_again_and_disconnect_clears_both() {
        let (mut state, _store, _vault) = state_with_store();
        let home = Arc::new(MemoryStateHome::new());
        fill_form(&mut state, &home, "ghp_example_token");
        state.connect_web_sync();
        assert!(state.web_sync.connected);

        // Reflecting the saved config keeps the fields and reports the token
        // as saved without ever re-displaying it.
        state.web_sync.token.replace_text("");
        state.reflect_web_sync();
        assert_eq!(state.web_sync.owner.text, "octocat");
        assert_eq!(state.web_sync.repo.text, "notes");
        assert_eq!(state.web_sync.token.text, "");
        assert_eq!(state.web_sync.status, t(TextKey::SyncWebStatusConnected));

        state.disconnect_web_sync();
        assert!(!state.web_sync.connected);
        assert!(state.web_sync_binding().is_none());
        assert_eq!(home.get(PAT_KEY), None, "Disconnect clears the token");
        assert_eq!(state.web_sync.owner.text, "");
        assert_eq!(state.web_sync.branch.text, DEFAULT_BRANCH);
        assert_eq!(state.web_sync.status, t(TextKey::SyncWebStatusDisconnected));
    }

    #[test]
    fn a_token_with_whitespace_is_refused_and_nothing_is_saved() {
        let (mut state, _store, _vault) = state_with_store();
        let home = Arc::new(MemoryStateHome::new());
        fill_form(&mut state, &home, "ghp two words");
        state.connect_web_sync();
        assert_eq!(state.web_sync.status, t(TextKey::SyncWebErrorTokenShape));
        assert!(!state.web_sync.connected);
        assert!(state.web_sync_binding().is_none());
        assert_eq!(home.get(PAT_KEY), None);
    }

    #[test]
    fn sync_now_refuses_without_a_host_or_a_connection() {
        let (mut state, _store, _vault) = state_with_store();
        let home = Arc::new(MemoryStateHome::new());
        state
            .web_sync
            .use_state_home(Some(Arc::clone(&home) as Arc<dyn StateHome>));
        state.web_sync_now();
        assert_eq!(state.web_sync.status, t(TextKey::SyncStatusConnectProvider));
        fill_form(&mut state, &home, "ghp_example_token");
        state.connect_web_sync();
        state.web_sync_now();
        assert_eq!(state.web_sync.status, t(TextKey::SyncWebErrorUnavailable));
        assert_eq!(state.web_sync.phase, WebSyncPhase::Idle);
    }

    /// Regression: the status line must not double the copy's prefix.
    ///
    /// The live gate saw `"Authentication needed: Authentication needed: Bad
    /// credentials"` because the worker sent `SyncError::Auth`'s `Display`
    /// rendering and this mapping added the catalogue sentence on top. The
    /// worker's `report::page_detail` now sends the raw inner detail, and this
    /// pins the page half of that contract — for a Drive-shaped failure too.
    #[test]
    fn a_failure_renders_exactly_one_prefix_for_both_providers() {
        let status = failure_text("auth", "Bad credentials");
        assert_eq!(
            status,
            muninn_i18n::sync_authentication_needed("Bad credentials")
        );
        assert_eq!(
            status.matches("Bad credentials").count(),
            1,
            "the detail is interpolated once: {status}"
        );
        // The shape a `Display` payload would produce is a different, doubled
        // line; the page must not render it.
        let doubled =
            muninn_i18n::sync_authentication_needed("Authentication needed: Bad credentials");
        assert_ne!(status, doubled);

        // A Drive failure the worker passes through the same raw-detail
        // contract: the inner field is rendered once, inside the catalogued
        // frame. `Config` is the unconfigured-client/refresh class.
        let config = failure_text("config", "this web build has no configured Web client id");
        assert_eq!(
            config,
            sync_not_configured("this web build has no configured Web client id")
        );
        assert_eq!(
            config
                .matches("this web build has no configured Web client id")
                .count(),
            1,
            "the Drive detail is interpolated once: {config}"
        );
        let auth = failure_text(
            "auth",
            "Google refused the refresh token (Token has been expired or revoked.); connect again",
        );
        assert_eq!(
            auth.matches("Google refused the refresh token").count(),
            1,
            "a Drive auth detail is not doubled: {auth}"
        );
    }

    #[test]
    fn every_failure_code_has_copy() {
        for code in FAILURE_CODES {
            let text = failure_text(code, "the provider said no");
            assert!(!text.is_empty(), "{code} has no status line");
            assert!(
                !text.contains(muninn_i18n::PSEUDO_OPEN),
                "{code} rendered a pseudo marker: {text}"
            );
        }
        // A known class keeps the detail as data inside catalogued framing.
        let auth = failure_text("auth", "Bad credentials");
        assert_eq!(
            auth,
            muninn_i18n::sync_authentication_needed("Bad credentials")
        );
        // An unknown code is translated framing around the diagnostic, never a
        // raw English sentence as the whole line.
        let unknown = failure_text("something-new", "the provider said no");
        assert_eq!(
            unknown,
            phrase_current(MsgKey::SyncWebFailed, &["the provider said no"])
        );
    }

    #[test]
    fn started_running_and_a_terminal_event_move_the_status_and_phase() {
        let (mut state, _store, _vault) = state_with_store();
        assert_eq!(state.web_sync.phase, WebSyncPhase::Idle);

        assert!(state.apply_web_sync(WebSyncEvent::Started));
        assert_eq!(state.web_sync.status, t(TextKey::SyncStatusSyncStarted));

        assert!(state.apply_web_sync(WebSyncEvent::Completed {
            head: "abc1234".to_owned(),
            exclusions: vec![".git".to_owned()],
            conflict_copies: Vec::new(),
        }));
        assert_eq!(state.web_sync.phase, WebSyncPhase::Idle);
        assert!(
            state
                .web_sync
                .status
                .starts_with(&muninn_i18n::last_sync("abc1234"))
        );
        assert!(state.web_sync.status.contains(&sync_excluded(1)));

        assert!(state.apply_web_sync(WebSyncEvent::Failed {
            code: "auth".to_owned(),
            detail: "Bad credentials".to_owned(),
        }));
        assert_eq!(state.web_sync.phase, WebSyncPhase::Idle);
        assert_eq!(
            state.web_sync.status,
            muninn_i18n::sync_authentication_needed("Bad credentials")
        );
    }

    #[test]
    fn a_conflicts_event_names_the_paths_and_settles_the_phase() {
        let (mut state, _store, _vault) = state_with_store();
        state.apply_web_sync(WebSyncEvent::Started);
        assert!(state.apply_web_sync(WebSyncEvent::Conflicts {
            paths: vec!["Notes/One.md".to_owned(), "Notes/Two.md".to_owned()],
        }));
        assert_eq!(state.web_sync.phase, WebSyncPhase::Idle);
        assert_eq!(
            state.web_sync.status,
            sync_conflicts(2, "Notes/One.md, Notes/Two.md")
        );
    }

    #[test]
    fn a_terminal_event_re_indexes_the_vault_from_the_store() {
        let (mut state, store, _vault) = state_with_store();
        assert!(state.vault.notes.iter().any(|note| note.rel == "A.md"));
        // The worker's pull: the file lands in the store mirror (the page's
        // post-run OPFS re-read) before the terminal event is handed over.
        store.put_file("B.md", b"# B\n");
        assert!(!state.vault.notes.iter().any(|note| note.rel == "B.md"));

        state.apply_web_sync(WebSyncEvent::Completed {
            head: "abc1234".to_owned(),
            exclusions: Vec::new(),
            conflict_copies: Vec::new(),
        });

        assert!(
            state.vault.notes.iter().any(|note| note.rel == "B.md"),
            "the note list reflects the pulled file without a manual reload"
        );
    }

    #[test]
    fn a_dirty_buffer_keeps_its_text_and_still_refreshes_the_index() {
        let (mut state, store, _vault) = state_with_store();
        let id = state.vault.notes[0].id;
        state.load_note(id, false);
        state.editor.replace_text("# A edited\n");
        assert!(state.dirty());
        store.put_file("B.md", b"# B\n");

        state.apply_web_sync(WebSyncEvent::Failed {
            code: "offline".to_owned(),
            detail: "no network".to_owned(),
        });

        assert_eq!(state.editor.text, "# A edited\n", "unsaved text is kept");
        assert!(
            state.vault.notes.iter().any(|note| note.rel == "B.md"),
            "the index still catches up"
        );
        assert!(
            state
                .web_sync
                .status
                .contains(t(TextKey::SyncStatusNoteChangedWhileSyncing)),
            "{}",
            state.web_sync.status
        );
    }

    #[test]
    fn the_config_carries_the_saved_fields_and_the_token_and_redacts_it() {
        let (mut state, _store, _vault) = state_with_store();
        let home = Arc::new(MemoryStateHome::new());
        fill_form(&mut state, &home, "ghp_example_token");
        state.web_sync.prefix.replace_text("vault");
        state.connect_web_sync();

        let config = state.web_sync_config().expect("a saved binding and token");
        assert_eq!(config.provider, WebSyncProvider::Github);
        assert_eq!(config.owner, "octocat");
        assert_eq!(config.repo, "notes");
        assert_eq!(config.branch, "main");
        assert_eq!(config.prefix, "vault");
        assert_eq!(config.access.as_str(), "ghp_example_token");
        assert!(config.refresh.is_none());
        assert_eq!(config.expires_unix_ms, None);
        assert_eq!(
            config.binding_id,
            github_binding_id_for(&state.web_sync_identity(), "octocat", "notes")
        );
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("ghp_example_token"));
        assert!(rendered.contains("redacted"));
    }

    /// Slice 9: the Drive token round-trips through the state home as the
    /// desktop `TokenSet` JSON shape, and the config carries it back.
    #[test]
    fn the_drive_token_round_trips_through_the_state_home() {
        let (mut state, _store, _vault) = state_with_store();
        let home = Arc::new(MemoryStateHome::new());
        state
            .web_sync
            .use_state_home(Some(Arc::clone(&home) as Arc<dyn StateHome>));
        state.set_web_sync_provider(1);
        state.web_sync.drive_store.replace_text("Muninn");

        assert!(
            store_drive_token(
                home.as_ref(),
                &drive_token("ya29.access", Some("1//refresh"), Some(1_700_000_000_000)),
                "",
            )
            .is_ok()
        );
        let bytes = home.get(DRIVE_KEY).expect("the Drive key is written");
        let stored = stored_drive_token(&bytes).expect("the TokenSet shape");
        assert_eq!(stored.access_token, "ya29.access");
        assert_eq!(stored.refresh_token, "1//refresh");
        assert_eq!(stored.expires_unix_ms, Some(1_700_000_000_000));

        // Reflect the saved credential without a binding: the form is not
        // connected until a binding exists, but a Connect with a binding makes
        // the config carry the token back key for key.
        let identity = state.web_sync_identity();
        let binding_id = drive_binding_id_for(&identity, "Muninn");
        state.settings.sync.vaults.push(VaultSyncBinding {
            identity,
            provider: "google_drive".to_owned(),
            binding_id: binding_id.clone(),
            account: String::new(),
            github_owner: String::new(),
            github_repo: String::new(),
            github_branch: String::new(),
            github_prefix: String::new(),
            drive_store: "Muninn".to_owned(),
            tailnet_role: String::new(),
            tailnet_host: String::new(),
            tailnet_port: 0,
            tailnet_allowed: String::new(),
            last_success_unix_ms: None,
            last_head: String::new(),
        });
        state.reflect_web_sync();
        assert!(state.web_sync.connected);
        assert_eq!(state.web_sync.status, t(TextKey::SyncWebDriveConnected));
        assert_eq!(state.web_sync.provider(), WebSyncProvider::Drive);

        let config = state.web_sync_config().expect("a saved Drive credential");
        assert_eq!(config.provider, WebSyncProvider::Drive);
        assert_eq!(config.binding_id, binding_id);
        assert_eq!(config.store, "Muninn");
        assert_eq!(config.access.as_str(), "ya29.access");
        assert_eq!(
            config.refresh.as_ref().map(|token| token.as_str()),
            Some("1//refresh")
        );
        assert_eq!(config.expires_unix_ms, Some(1_700_000_000_000));
        assert!(config.owner.is_empty() && config.repo.is_empty());
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("ya29.access"));
        assert!(!rendered.contains("1//refresh"));
    }

    #[test]
    fn the_drive_config_without_a_token_is_refused_and_disconnect_clears_it() {
        let (mut state, _store, _vault) = state_with_store();
        let home = Arc::new(MemoryStateHome::new());
        state
            .web_sync
            .use_state_home(Some(Arc::clone(&home) as Arc<dyn StateHome>));
        let identity = state.web_sync_identity();
        state.settings.sync.vaults.push(VaultSyncBinding {
            identity,
            provider: "google_drive".to_owned(),
            binding_id: drive_binding_id_for(&state.web_sync_identity(), "Muninn"),
            account: String::new(),
            github_owner: String::new(),
            github_repo: String::new(),
            github_branch: String::new(),
            github_prefix: String::new(),
            drive_store: "Muninn".to_owned(),
            tailnet_role: String::new(),
            tailnet_host: String::new(),
            tailnet_port: 0,
            tailnet_allowed: String::new(),
            last_success_unix_ms: None,
            last_head: String::new(),
        });
        state.reflect_web_sync();
        assert!(!state.web_sync.connected);
        assert_eq!(state.web_sync.status, t(TextKey::SyncWebDriveTokenMissing));
        assert_eq!(
            state.web_sync_config().unwrap_err(),
            t(TextKey::SyncWebDriveTokenMissing)
        );

        store_drive_token(home.as_ref(), &drive_token("ya29.access", None, None), "").unwrap();
        assert!(state.web_sync_config().is_ok());
        state.disconnect_web_sync();
        assert_eq!(home.get(DRIVE_KEY), None, "Disconnect clears the tokens");
        assert!(state.web_sync_binding().is_none());
        assert_eq!(state.web_sync.drive_store.text, DEFAULT_DRIVE_STORE);
    }

    /// The unconfigured state is honest: with no client id Connect says what is
    /// needed and writes nothing, not even the binding.
    #[test]
    fn an_unconfigured_drive_connect_writes_nothing() {
        let (mut state, _store, _vault) = state_with_store();
        let home = Arc::new(MemoryStateHome::new());
        state
            .web_sync
            .use_state_home(Some(Arc::clone(&home) as Arc<dyn StateHome>));
        state.set_web_sync_provider(1);
        assert_eq!(
            state.web_sync.status,
            t(TextKey::SyncWebDriveUnconfigured),
            "selecting Drive says the build needs configuring"
        );
        state.web_sync.drive_store.replace_text("Muninn");
        state.connect_web_sync();
        assert_eq!(state.web_sync.status, t(TextKey::SyncWebDriveUnconfigured));
        assert!(state.web_sync_binding().is_none());
        assert_eq!(state.web_sync.phase, WebSyncPhase::Idle);
        assert_eq!(home.get(DRIVE_KEY), None);
        // No hooks are installed in this test either, so the unavailable
        // status can never outrank the unconfigured one.
        assert_eq!(state.web_sync.status, t(TextKey::SyncWebDriveUnconfigured));
    }

    #[test]
    fn a_drive_sign_in_event_moves_the_phase_and_the_status() {
        let (mut state, _store, _vault) = state_with_store();
        state.set_web_sync_provider(1);
        state.web_sync.phase = WebSyncPhase::Connecting;
        assert!(state.apply_web_sync(WebSyncEvent::DriveSignInFailed));
        assert_eq!(state.web_sync.phase, WebSyncPhase::Idle);
        assert!(!state.web_sync.connected);
        assert_eq!(state.web_sync.status, t(TextKey::SyncWebDriveSignInFailed));

        state.web_sync.phase = WebSyncPhase::Connecting;
        assert!(state.apply_web_sync(WebSyncEvent::DriveStorageFailed));
        assert_eq!(state.web_sync.phase, WebSyncPhase::Idle);
        assert_eq!(
            state.web_sync.status,
            t(TextKey::SyncWebErrorStateUnavailable)
        );
    }

    /// A Drive Connect commits nothing until the popup succeeds. A closed
    /// popup drops the pending binding and leaves the GitHub binding and PAT
    /// (or the previous Drive credential) exactly as they were; a success
    /// replaces them, as the desktop's sign-in does.
    #[test]
    fn a_drive_sign_in_commits_only_on_success_and_failure_keeps_the_old_binding() {
        let (mut state, _store, _vault) = state_with_store();
        let home = Arc::new(MemoryStateHome::new());
        fill_form(&mut state, &home, "ghp_example_token");
        state.connect_web_sync();
        let github = state.web_sync_binding().cloned().expect("GitHub is bound");
        assert_eq!(home.get(PAT_KEY), Some(b"ghp_example_token".to_vec()));

        // What `connect_web_sync_drive` stages before the popup.
        state.set_web_sync_provider(1);
        let identity = state.web_sync_identity();
        state.web_sync.pending_drive = Some(VaultSyncBinding {
            identity: identity.clone(),
            provider: "google_drive".to_owned(),
            binding_id: drive_binding_id_for(&identity, "Muninn"),
            account: String::new(),
            github_owner: String::new(),
            github_repo: String::new(),
            github_branch: String::new(),
            github_prefix: String::new(),
            drive_store: "Muninn".to_owned(),
            tailnet_role: String::new(),
            tailnet_host: String::new(),
            tailnet_port: 0,
            tailnet_allowed: String::new(),
            last_success_unix_ms: None,
            last_head: String::new(),
        });
        state.web_sync.phase = WebSyncPhase::Connecting;

        // A closed popup: nothing is committed, and the GitHub PAT survives.
        assert!(state.apply_web_sync(WebSyncEvent::DriveSignInFailed));
        assert_eq!(state.web_sync.phase, WebSyncPhase::Idle);
        assert!(state.web_sync.pending_drive.is_none());
        assert_eq!(state.web_sync_binding(), Some(&github));
        assert_eq!(
            home.get(PAT_KEY),
            Some(b"ghp_example_token".to_vec()),
            "a cancelled Drive sign-in does not remove the GitHub PAT"
        );
        assert_eq!(state.web_sync.status, t(TextKey::SyncWebDriveSignInFailed));

        // A success replaces the binding with Drive and clears the PAT, as a
        // desktop provider swap does.
        state.web_sync.pending_drive = Some(VaultSyncBinding {
            identity: identity.clone(),
            provider: "google_drive".to_owned(),
            binding_id: drive_binding_id_for(&identity, "Muninn"),
            account: String::new(),
            github_owner: String::new(),
            github_repo: String::new(),
            github_branch: String::new(),
            github_prefix: String::new(),
            drive_store: "Muninn".to_owned(),
            tailnet_role: String::new(),
            tailnet_host: String::new(),
            tailnet_port: 0,
            tailnet_allowed: String::new(),
            last_success_unix_ms: None,
            last_head: String::new(),
        });
        state.web_sync.phase = WebSyncPhase::Connecting;
        // The page stores the new token before it reports success. A
        // same-provider replacement must not clear it as "the old provider".
        store_drive_token(home.as_ref(), &drive_token("ya29.access", None, None), "").unwrap();
        assert!(state.apply_web_sync(WebSyncEvent::DriveSignedIn {
            account: String::new(),
        }));
        assert!(
            home.get(DRIVE_KEY).is_some(),
            "a Drive reconnect keeps the token it just stored"
        );
        assert!(state.web_sync.connected);
        assert_eq!(state.web_sync.phase, WebSyncPhase::Idle);
        assert_eq!(
            state
                .web_sync_binding()
                .map(|binding| binding.provider.as_str()),
            Some("google_drive")
        );
        assert_eq!(
            home.get(PAT_KEY),
            None,
            "the replaced provider's key is gone"
        );
        assert_eq!(state.web_sync.status, t(TextKey::SyncWebDriveConnected));
    }

    #[test]
    fn a_second_run_or_sign_in_is_refused_while_busy() {
        let (mut state, _store, _vault) = state_with_store();
        state.web_sync.phase = WebSyncPhase::Connecting;
        state.set_web_sync_provider(1);
        assert_eq!(
            state.web_sync.provider(),
            WebSyncProvider::Github,
            "the control does not move while a popup is open"
        );
        assert_eq!(state.web_sync.status, t(TextKey::SyncStatusWaitForRun));
        state.disconnect_web_sync();
        assert_eq!(state.web_sync.status, t(TextKey::SyncStatusWaitForRun));
        assert!(WebSyncPhase::Connecting.is_busy());
        assert!(!WebSyncPhase::Connecting.is_running());
    }

    /// The state's settings model and the state home are separate stores: the
    /// config is saved in `settings.json`, the token under its own key.
    #[test]
    fn settings_hold_only_the_public_config() {
        let (mut state, _store, _vault) = state_with_store();
        let home = Arc::new(MemoryStateHome::new());
        fill_form(&mut state, &home, "ghp_example_token");
        state.connect_web_sync();
        let settings = &state.settings.sync.vaults[0];
        for field in [
            &settings.github_owner,
            &settings.github_repo,
            &settings.github_branch,
            &settings.github_prefix,
            &settings.drive_store,
            &settings.account,
            &settings.identity,
        ] {
            assert!(!field.contains("ghp_example_token"));
        }
    }

    #[test]
    fn the_drive_key_keeps_the_account_shape_without_a_slash() {
        assert_eq!(drive_key(""), DRIVE_KEY);
        assert_eq!(drive_key("   "), DRIVE_KEY);
        assert_eq!(
            drive_key("me@example.com"),
            "muninn/sync/drive-me@example.com"
        );
        assert_eq!(drive_key("a/b"), "muninn/sync/drive-ab");
        assert_eq!(
            drive_key("../../etc"),
            "muninn/sync/drive-....etc",
            "the slashes are dropped, so the label cannot add a path separator"
        );
        assert_eq!(
            drive_key("../../etc").matches('/').count(),
            2,
            "only the `muninn/sync/` prefix has separators"
        );
    }
}
