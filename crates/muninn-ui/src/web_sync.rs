//! The browser's GitHub sync surface (#240 slice 8b).
//!
//! The desktop's Sync tab (`sync_ui`) signs in through the OS
//! keychain and runs the engine on a worker thread. A browser has neither, so
//! this is the web twin of that surface:
//!
//! - **GitHub only.** Device flow is impossible in page JavaScript (GitHub's
//!   OAuth endpoints are not CORS-open), so the auth decision recorded on #240
//!   is **personal access token entry with no hosted service**. Drive arrives
//!   with slice 9; Tailscale stays a desktop-only door (a browser cannot reach
//!   a tailnet commit store without a relay).
//! - **The form and its config** are the same shapes as the desktop: owner,
//!   repository, branch, path prefix, one status line, and Connect / Sync now /
//!   Cancel / Disconnect. The public config is a `VaultSyncBinding` in
//!   `settings.json`, so the browser and the desktop write the same JSON.
//! - **The run** goes through `muninn_web::page_sync::spawn`, which owns the
//!   worker, the ready handshake, the flush gate and the post-run OPFS
//!   re-read. This module builds the config, relays the run's events into the
//!   status line, and re-indexes the vault UI when the run is over.
//!
//! # Where the token lives at rest
//!
//! A browser has no OS keychain. The PAT is therefore stored in the page's
//! state home (`muninn-state/` via `StateHome`) under [`PAT_KEY`], never in
//! `settings.json` and never in a log line. That is a deliberate departure
//! from the desktop's keychain discipline, recorded on #240: any script running
//! on this origin, or anyone with this browser profile, can read origin-private
//! storage. The mitigations are the honest ones this platform allows:
//!
//! - the field is masked ([`rune::TextEdit::secure`]) and is cleared as soon as
//!   it is saved, so it is never re-displayed;
//! - the copy that travels to the run is zeroized after use ([`zeroize`]);
//!   the serialized worker message is dropped by the spawn session;
//! - Disconnect removes the key and the config together;
//! - a read-only tab (another tab holds the vault's write lock) cannot write
//!   the state home, so it cannot store a token.
//!
//! The token is one dedicated key rather than one per binding: a browser vault
//! has one GitHub binding, and Disconnect clears both. Changing the repository
//! keeps the token and derives a new binding id; the old binding's worker-side
//! journal directory is simply no longer read.
//!
//! # One run at a time
//!
//! A second Sync now while a run is in flight is **refused** with
//! [`muninn_i18n::Key::SyncStatusAlreadyRunning`] (the desktop's policy), and
//! Disconnect refuses too: Cancel stops the run, then Disconnect clears the
//! token. The page's run handle owns the worker, so there is never a queue.

use std::sync::Arc;

use muninn_core::StateHome;
use muninn_i18n::{
    Key as TextKey, MsgKey, last_sync, phrase_current, sync_conflict_copies, sync_conflicts,
    sync_credentials_unavailable, sync_dirty_buffer, sync_excluded, sync_file_error,
    sync_limit_exceeded, sync_listing_incomplete, sync_local_changed, sync_not_configured,
    sync_oauth_scope_rejected, sync_offline, sync_outcome_unknown, sync_permission_denied,
    sync_remote_changed, sync_remote_scope_not_found, t,
};
use rune::TextEdit;
use zeroize::Zeroizing;

use crate::State;
use crate::settings::VaultSyncBinding;

/// The state-home key the PAT is stored under.
///
/// The spelling follows the state home's `muninn/…` keyspace (the same one
/// `muninn/settings.json` and `muninn/recovery/…` use). It is not the worker's
/// `muninn-sync-state` directory: the worker's journals are engine state, this
/// is the page's credential. See the module docs for the at-rest tradeoff.
pub const PAT_KEY: &str = "muninn/sync/github-pat";

/// The default branch a fresh form shows. The config keeps whatever is saved.
pub const DEFAULT_BRANCH: &str = "main";

/// Whether a sync run is in flight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WebSyncPhase {
    /// No run in flight; the form is editable.
    #[default]
    Idle,
    /// A run owns the page's vault until its terminal event.
    Running,
}

impl WebSyncPhase {
    #[must_use]
    pub fn is_running(self) -> bool {
        matches!(self, WebSyncPhase::Running)
    }
}

/// The form, its status and the run phase.
pub struct WebSyncUi {
    pub owner: TextEdit,
    pub repo: TextEdit,
    pub branch: TextEdit,
    pub prefix: TextEdit,
    /// Masked; cleared once saved. Never loaded back into the form.
    pub token: TextEdit,
    pub status: String,
    pub phase: WebSyncPhase,
    /// A binding is saved for this vault. Derived at reflect time from
    /// settings + the stored token; kept for the tests and the view's shape.
    pub connected: bool,
    /// The state home the token is read from and written to. On the desktop
    /// this is `None` (the browser surface is never drawn or driven there).
    pub(crate) home: Option<Arc<dyn StateHome>>,
}

impl WebSyncUi {
    /// A fresh form. The page boot registers the state home before the `State`
    /// is built, so the browser captures it here; a native build captures
    /// nothing and the browser methods are inert.
    #[must_use]
    pub fn new() -> WebSyncUi {
        WebSyncUi {
            owner: TextEdit::new(""),
            repo: TextEdit::new(""),
            branch: TextEdit::new(DEFAULT_BRANCH),
            prefix: TextEdit::new(""),
            token: TextEdit::new("").secure(),
            status: t(TextKey::SyncWebStatusNotConnected).to_owned(),
            phase: WebSyncPhase::Idle,
            connected: false,
            home: session_home(),
        }
    }

    /// Install the state home a test drives directly. The browser gets its
    /// home from the boot, so this is test-only.
    #[cfg(test)]
    pub(crate) fn use_state_home(&mut self, home: Option<Arc<dyn StateHome>>) {
        self.home = home;
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
}

impl Default for WebSyncUi {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything one browser run needs from the UI. The PAT is the only secret.
///
/// No `Debug`: an accidental log or panic message must not be able to render
/// the token. The token is [`Zeroizing`], so dropping the config wipes it.
pub struct WebSyncConfig {
    pub binding_id: String,
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub prefix: String,
    pub token: Zeroizing<String>,
}

impl std::fmt::Debug for WebSyncConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSyncConfig")
            .field("binding_id", &self.binding_id)
            .field("owner", &self.owner)
            .field("repo", &self.repo)
            .field("branch", &self.branch)
            .field("prefix", &self.prefix)
            .field("token", &"Secret([redacted])")
            .finish()
    }
}

/// One run event, as the page relays it from the worker.
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
}

impl WebSyncEvent {
    /// Exactly one of `Completed`, `Conflicts` and `Failed` ends a run.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        !matches!(self, WebSyncEvent::Started)
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

/// Queue one relayed event for the next [`poll`].
pub fn enqueue(event: WebSyncEvent) {
    PENDING.with(|slot| slot.borrow_mut().push(event));
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

/// A stable binding id for a browser vault and repository.
///
/// The engine's journals, base and sha cache are keyed by binding id, so the
/// id must be **the same on every run**. A browser vault has no filesystem
/// path, so its label (the store's root) stands in for one; owner and repo are
/// folded in (ASCII-lowercased, because GitHub names are case-insensitive) so
/// switching repositories starts a distinct binding rather than reading the
/// old one's journal against a different remote.
///
/// Two independent FNV-1a passes give 32 hex characters, inside
/// `muninn_sync::BindingId`'s 8..=80 hex-or-dash contract. This is an
/// identifier, not a secret: it names a state-home directory, and it is not
/// used for authentication.
#[must_use]
pub fn binding_id_for(label: &str, owner: &str, repo: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    const SEP: u8 = 0x1f;
    let owner = owner.to_ascii_lowercase();
    let repo = repo.to_ascii_lowercase();
    let pass = |seed: u64, parts: [&str; 3]| {
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
    let first = pass(OFFSET, [label, owner.as_str(), repo.as_str()]);
    // A different seed over the reversed parts keeps both halves independent of
    // each other, so two 64-bit collisions would have to coincide.
    let second = pass(seed(), [repo.as_str(), owner.as_str(), label]);
    format!("{first:016x}{second:016x}")
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

    fn web_sync_binding(&self) -> Option<&VaultSyncBinding> {
        let identity = self.web_sync_identity();
        self.settings
            .sync
            .vaults
            .iter()
            .find(|binding| binding.identity == identity && binding.provider == "github")
    }

    /// Rebuild the form from this vault's saved binding and the stored token.
    ///
    /// Startup calls this after the settings and the state home are in hand. A
    /// saved repository fills the fields; the token itself is never loaded into
    /// the form, only reported as saved. A run in flight owns the form and is
    /// left alone.
    pub fn reflect_web_sync(&mut self) {
        if self.web_sync.phase.is_running() {
            return;
        }
        let Some(binding) = self.web_sync_binding().cloned() else {
            self.web_sync.status = t(TextKey::SyncWebStatusNotConnected).to_owned();
            return;
        };
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

    /// Validate the form, save the public config in settings and the PAT in
    /// the state home. Nothing reaches the provider: the browser's PAT has no
    /// sign-in step, and the first run reports an `auth` failure if the token
    /// is wrong.
    pub fn connect_web_sync(&mut self) {
        if self.web_sync.phase.is_running() {
            self.web_sync.status = t(TextKey::SyncStatusWaitForRun).to_owned();
            return;
        }
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
        // The token is stored (or already was); the public config may now be
        // saved. Losing the config after saving the token is harmless: the next
        // Connect re-saves it.
        let identity = self.web_sync_identity();
        let binding_id = binding_id_for(&identity, &owner, &repo);
        let previous = self.web_sync_binding().cloned();
        let (last_success_unix_ms, last_head) = match previous {
            Some(previous) if previous.binding_id == binding_id => {
                (previous.last_success_unix_ms, previous.last_head)
            }
            _ => (None, String::new()),
        };
        self.settings
            .sync
            .vaults
            .retain(|binding| !(binding.identity == identity && binding.provider == "github"));
        self.settings.sync.vaults.push(VaultSyncBinding {
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
        self.save_settings();
        drop(token);
        self.web_sync.connected = true;
        self.web_sync.status = t(TextKey::SyncWebStatusConnected).to_owned();
    }

    /// Snapshot the saved config + token into one run's [`WebSyncConfig`].
    fn web_sync_config(&self) -> Result<WebSyncConfig, String> {
        let binding = self
            .web_sync_binding()
            .ok_or_else(|| t(TextKey::SyncStatusConnectProvider).to_owned())?;
        let token = self
            .web_sync
            .load_pat()
            .ok_or_else(|| t(TextKey::SyncWebErrorTokenMissing).to_owned())?;
        Ok(WebSyncConfig {
            binding_id: binding.binding_id.clone(),
            owner: binding.github_owner.clone(),
            repo: binding.github_repo.clone(),
            branch: binding.github_branch.clone(),
            prefix: binding.github_prefix.clone(),
            token,
        })
    }

    /// Start one run through the registered browser host.
    ///
    /// Refused while the open note holds unsaved text or a conflict (nothing
    /// unpublished would be uploaded, and the guard keeps the run's re-index
    /// from overwriting a buffer), while a run is in flight, and when no
    /// browser host is registered.
    pub fn web_sync_now(&mut self) {
        if self.dirty() || self.conflict.is_some() {
            self.web_sync.status = t(TextKey::SyncStatusSaveBeforeSync).to_owned();
            return;
        }
        if self.web_sync.phase.is_running() {
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

    /// Remove the binding and the saved token. Local notes are untouched.
    ///
    /// Refused while a run is in flight: Cancel stops it first, so the token
    /// the run may still be using is never yanked mid-flight.
    pub fn disconnect_web_sync(&mut self) {
        if self.web_sync.phase.is_running() {
            self.web_sync.status = t(TextKey::SyncStatusWaitForRun).to_owned();
            return;
        }
        let identity = self.web_sync_identity();
        self.settings
            .sync
            .vaults
            .retain(|binding| !(binding.identity == identity && binding.provider == "github"));
        self.save_settings();
        self.web_sync.clear_pat();
        self.web_sync.owner.replace_text("");
        self.web_sync.repo.replace_text("");
        self.web_sync.branch.replace_text(DEFAULT_BRANCH);
        self.web_sync.prefix.replace_text("");
        self.web_sync.token.replace_text("");
        self.web_sync.connected = false;
        self.web_sync.status = t(TextKey::SyncWebStatusDisconnected).to_owned();
    }

    /// Apply one relayed run event. Returns whether anything changed.
    ///
    /// Every terminal event re-indexes the vault: the worker may have pulled,
    /// replaced or deleted files (or a failure may have applied part of a
    /// plan), and the page's OPFS re-read has already made the store mirror
    /// current when the page hands the event over. A clean buffer is reloaded
    /// through the store, so the note list and the open note show the pulled
    /// bytes. A dirty buffer keeps its text and only the index is refreshed,
    /// with the honest "changed while sync ran" note.
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
                if let Some(binding) =
                    self.settings.sync.vaults.iter_mut().find(|binding| {
                        binding.identity == identity && binding.provider == "github"
                    })
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

    #[test]
    fn the_form_follows_the_tailnet_masked_field() {
        let ui = WebSyncUi::new();
        assert!(ui.token.is_secure(), "the PAT is masked while typed");
        assert_eq!(ui.branch.text, DEFAULT_BRANCH);
        assert_eq!(ui.status, t(TextKey::SyncWebStatusNotConnected));
    }

    #[test]
    fn the_binding_id_is_stable_and_repository_specific() {
        let first = binding_id_for("/Muninn sample", "octocat", "notes");
        assert_eq!(
            binding_id_for("/Muninn sample", "OctoCat", "NOTES"),
            first,
            "GitHub names are case-insensitive, so the id must be too"
        );
        assert_eq!(
            binding_id_for("/Muninn sample", "octocat", "notes"),
            first,
            "the same vault and repository derive the same id on every run"
        );
        assert_ne!(binding_id_for("/Muninn sample", "octocat", "other"), first);
        assert_ne!(binding_id_for("/Muninn sample", "other", "notes"), first);
        assert_ne!(binding_id_for("/another vault", "octocat", "notes"), first);
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        // `BindingId::parse`'s contract: 8..=80 hex characters.
        assert!((8..=80).contains(&first.len()));
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
            binding_id_for(&state.web_sync_identity(), "octocat", "notes")
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
    /// pins the page half of that contract.
    #[test]
    fn an_auth_failure_renders_exactly_one_prefix() {
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
        assert_eq!(config.owner, "octocat");
        assert_eq!(config.repo, "notes");
        assert_eq!(config.branch, "main");
        assert_eq!(config.prefix, "vault");
        assert_eq!(config.token.as_str(), "ghp_example_token");
        assert_eq!(
            config.binding_id,
            binding_id_for(&state.web_sync_identity(), "octocat", "notes")
        );
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("ghp_example_token"));
        assert!(rendered.contains("redacted"));
    }

    /// The state's settings model and the state home are separate stores: the
    /// config is saved in `settings.json`, the token under [`PAT_KEY`].
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
            &settings.account,
            &settings.identity,
        ] {
            assert!(!field.contains("ghp_example_token"));
        }
    }
}
