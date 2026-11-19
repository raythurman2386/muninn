//! Settings-owned sync session.
//!
//! Sync Now is manual. Network work runs on a worker thread. While a run is
//! in flight (`phase == Running`), autosave and the metadata watcher do not
//! write or reload across the batch. Disconnect removes the binding and its
//! credential only.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use muninn_i18n::{
    Key as TextKey, browser_failed, connected_as, device_code, last_sync, shared_key_copied,
    sign_in_failed, sync_authentication_needed, sync_conflict_copies, sync_conflicts,
    sync_credentials_unavailable, sync_dirty_buffer, sync_excluded, sync_file_error,
    sync_key_rotation_failed, sync_limit_exceeded, sync_listing_incomplete, sync_local_changed,
    sync_not_configured, sync_nothing_saved, sync_now_after_error, sync_oauth_scope_rejected,
    sync_offline, sync_outcome_unknown, sync_permission_denied, sync_previous_credential,
    sync_provider_error, sync_rate_limited_after, sync_remote_changed, sync_remote_scope_not_found,
    sync_unsupported, t, tailnet_hosting, tailnet_hosting_ready,
};
use muninn_sync::{
    BindingId, CancelToken, CredentialStore, MemoryCredentialStore, NativeCredentialStore,
    OperationId, ProviderKind, Resolution, RunRequest, Secret, SyncReport, TAILNET_DEFAULT_PORT,
    TailnetHost, TailnetHostConfig, TailnetRole, TailnetScope, TokenSet, VaultIdentity,
};
use rune::TextEdit;

use crate::State;
use crate::settings::VaultSyncBinding;

/// How long a copied Tailscale key stays on the clipboard.
pub const TAILNET_KEY_CLIPBOARD: Duration = Duration::from_secs(60);

/// Default Drive store name. Two vaults on one Google account need two names.
pub const DEFAULT_DRIVE_STORE: &str = "Muninn";

pub struct SyncUi {
    pub provider_index: usize,
    pub status: String,
    pub github_owner: TextEdit,
    pub github_repo: TextEdit,
    pub github_branch: TextEdit,
    pub github_prefix: TextEdit,
    pub drive_store: TextEdit,
    pub github_client_id: TextEdit,
    pub google_client_id: TextEdit,
    /// Tailscale: 0 = this device hosts, 1 = join a host.
    pub tailnet_role_index: usize,
    pub tailnet_host: TextEdit,
    /// Peer only. Masked; cleared as soon as the key is in the OS credential
    /// store.
    pub tailnet_key: TextEdit,
    /// Host only: the shared key while "Show key" is on. Read from the
    /// credential store on demand and dropped on Hide, blur or Disconnect.
    pub(crate) revealed_key: Option<Secret>,
    pub tailnet_port: TextEdit,
    pub tailnet_allowed: TextEdit,
    /// Sheet-sync machinery state. While a run is in flight `phase` is
    /// `Running`, and nothing else (autosave, watcher, a second Sync Now) is
    /// allowed to move. `Idle` means the last result has been consumed.
    pub phase: SyncPhase,
    credentials: Arc<dyn CredentialStore>,
    cancel: CancelToken,
    inbox: Option<Receiver<SyncMessage>>,
    /// Running commit-store server while this device hosts a Tailscale binding.
    host: Option<TailnetHost>,
    /// Where sign-in sends people and requests. Production endpoints and the
    /// system browser; tests swap in loopback fixtures and a fake browser.
    pub(crate) sign_in: SignInEnv,
    /// The binding a Connect is signing in for. It replaces the vault's
    /// current binding only once sign-in succeeds; until then the old one
    /// keeps working and stays in settings.
    pending: Option<VaultSyncBinding>,
    /// Why the last Connect did not finish, while no binding replaced it.
    /// Sync Now keeps this in the status instead of a generic hint.
    sign_in_error: Option<String>,
}

/// Endpoints and browser for interactive sign-in.
#[derive(Clone)]
pub(crate) struct SignInEnv {
    pub github_api: String,
    pub github_oauth: String,
    pub google_token: String,
    pub google_auth: String,
    pub browser: Arc<dyn muninn_sync::BrowserOpener>,
    pub sleeper: Arc<dyn muninn_sync::Sleeper>,
    /// Tests only: allow `http://127.0.0.1` endpoints.
    pub loopback: bool,
    /// How long to wait for the Google consent redirect.
    pub google_timeout: Duration,
    /// The Google client secret compiled into this build. Tests use a fake
    /// one. Never shown and never saved in settings.
    pub google_secret: Option<Secret>,
}

impl SignInEnv {
    fn production() -> Self {
        Self {
            github_api: "https://api.github.com".to_owned(),
            github_oauth: "https://github.com".to_owned(),
            google_token: "https://oauth2.googleapis.com".to_owned(),
            google_auth: muninn_sync::GOOGLE_AUTH_BASE.to_owned(),
            browser: Arc::new(muninn_sync::SystemBrowser),
            sleeper: Arc::new(muninn_sync::RealSleeper),
            loopback: false,
            google_timeout: Duration::from_secs(300),
            google_secret: muninn_sync::bundled_google_client_secret(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SyncPhase {
    /// No run in flight.
    #[default]
    Idle,
    /// A worker thread owns the run until `poll_sync` consumes its result.
    Running,
    /// A sign-in worker is waiting on the browser. Notes stay editable and
    /// autosave keeps running; only Sync Now and a second Connect wait.
    Connecting,
}

impl SyncPhase {
    #[must_use]
    pub fn is_running(self) -> bool {
        matches!(self, SyncPhase::Running)
    }

    #[must_use]
    pub fn is_idle(self) -> bool {
        matches!(self, SyncPhase::Idle)
    }
}

enum SyncMessage {
    Finished(Result<SyncReport, String>),
    SignIn(SignInUpdate),
}

enum SignInUpdate {
    DeviceCode {
        user_code: String,
        uri: String,
    },
    Waiting,
    BrowserFailed {
        url: String,
    },
    /// The token is already in the credential store when this arrives.
    Connected {
        binding_id: String,
        result: Result<String, String>,
    },
}

struct ChannelEvents(Sender<SyncMessage>);

impl muninn_sync::SignInEvents for ChannelEvents {
    fn device_code(&mut self, user_code: &str, verification_uri: &str) {
        let _ = self.0.send(SyncMessage::SignIn(SignInUpdate::DeviceCode {
            user_code: user_code.to_owned(),
            uri: verification_uri.to_owned(),
        }));
    }
    fn waiting_for_browser(&mut self, _url: &str) {
        let _ = self.0.send(SyncMessage::SignIn(SignInUpdate::Waiting));
    }
    fn browser_failed(&mut self, url: &str, _error: &muninn_sync::SyncError) {
        let _ = self
            .0
            .send(SyncMessage::SignIn(SignInUpdate::BrowserFailed {
                url: url.to_owned(),
            }));
    }
}

impl SyncUi {
    pub fn memory() -> Self {
        Self::with_store(Arc::new(MemoryCredentialStore::default()))
    }

    pub fn native() -> Self {
        Self::with_store(Arc::new(NativeCredentialStore))
    }

    fn with_store(credentials: Arc<dyn CredentialStore>) -> Self {
        Self {
            provider_index: 0,
            status: t(TextKey::SyncStatusNotConnected).to_owned(),
            github_owner: TextEdit::new(""),
            github_repo: TextEdit::new(""),
            github_branch: TextEdit::new("muninn-sync"),
            github_prefix: TextEdit::new("vault"),
            drive_store: TextEdit::new(DEFAULT_DRIVE_STORE),
            github_client_id: TextEdit::new(""),
            google_client_id: TextEdit::new(""),
            tailnet_role_index: 0,
            tailnet_host: TextEdit::new(""),
            tailnet_key: TextEdit::new("").secure(),
            revealed_key: None,
            tailnet_port: TextEdit::new(TAILNET_DEFAULT_PORT.to_string()),
            tailnet_allowed: TextEdit::new(""),
            phase: SyncPhase::Idle,
            credentials,
            cancel: CancelToken::new(),
            inbox: None,
            host: None,
            sign_in: SignInEnv::production(),
            pending: None,
            sign_in_error: None,
        }
    }

    pub fn blur(&mut self) {
        for field in [
            &mut self.github_owner,
            &mut self.github_repo,
            &mut self.github_branch,
            &mut self.github_prefix,
            &mut self.drive_store,
            &mut self.github_client_id,
            &mut self.google_client_id,
            &mut self.tailnet_host,
            &mut self.tailnet_key,
            &mut self.tailnet_port,
            &mut self.tailnet_allowed,
        ] {
            field.focused = false;
        }
        self.revealed_key = None;
    }
}

impl State {
    pub fn sync_status(&self) -> &str {
        &self.sync_ui.status
    }

    /// Whether Sync Now can start a run for this vault right now: a provider
    /// is connected for it, sync is not disabled, and no run or sign-in is in
    /// progress. The tray greys its Sync now item otherwise. A dirty note is
    /// left to [`State::sync_now`], which refuses and says why.
    #[must_use]
    pub fn sync_ready(&self) -> bool {
        self.sync_ui.phase.is_idle()
            && self.settings.sync.disabled_reason.is_none()
            && self.current_binding().is_some()
    }

    /// Whether a sync run or a sign-in is in flight.
    #[must_use]
    pub fn sync_busy(&self) -> bool {
        !self.sync_ui.phase.is_idle()
    }

    pub fn set_sync_provider(&mut self, index: usize) {
        self.sync_ui.provider_index = index.min(2);
        self.sync_ui.status = t(match self.sync_ui.provider_index {
            0 => TextKey::SyncProviderGithubDescription,
            1 => TextKey::SyncProviderDriveDescription,
            _ => TextKey::SyncProviderTailscaleDescription,
        })
        .to_owned();
    }

    pub fn set_tailnet_role(&mut self, index: usize) {
        self.sync_ui.tailnet_role_index = usize::from(index == 1);
        self.sync_ui.status = t(if self.sync_ui.tailnet_role_index == 0 {
            TextKey::SyncStatusTailnetHost
        } else {
            TextKey::SyncStatusTailnetJoin
        })
        .to_owned();
    }

    /// Starts the Tailscale commit-store server if this vault's binding hosts
    /// one. Called at startup and after Connect; a no-op otherwise.
    pub fn resume_tailnet_host(&mut self) {
        let Some(binding) = self.current_binding().cloned() else {
            return;
        };
        if binding.provider != "tailscale" || binding.tailnet_role != "host" {
            return;
        }
        if self.sync_ui.host.is_some() {
            return;
        }
        match self.start_tailnet_host(&binding) {
            Ok(host) => {
                self.sync_ui.status = tailnet_hosting(host.addr().port(), &host.serve_command());
                self.sync_ui.host = Some(host);
            }
            Err(error) => self.sync_ui.status = error,
        }
    }

    /// Rebuild the Settings › Sync form from this vault's saved binding.
    ///
    /// Startup and a vault switch both hand the app a fresh `SyncUi` with
    /// GitHub selected and "Not connected"; without this the tab and the tray
    /// forget a working Drive, GitHub or Tailscale connection. A run or a
    /// sign-in in flight owns the form and is left alone.
    pub fn reflect_sync_binding(&mut self) {
        if !self.sync_ui.phase.is_idle() {
            return;
        }
        let Some(binding) = self.current_binding().cloned() else {
            // Keep a failed Connect's reason visible: it is the only place it
            // is shown, and Sync Now repeats it.
            if self.sync_ui.sign_in_error.is_none() {
                self.sync_ui.status = t(TextKey::SyncStatusNotConnected).to_owned();
            }
            return;
        };
        match binding.provider.as_str() {
            "github" => {
                self.sync_ui.provider_index = 0;
                self.sync_ui
                    .github_owner
                    .replace_text(&binding.github_owner);
                self.sync_ui.github_repo.replace_text(&binding.github_repo);
                self.sync_ui
                    .github_branch
                    .replace_text(&binding.github_branch);
                self.sync_ui
                    .github_prefix
                    .replace_text(&binding.github_prefix);
            }
            "google_drive" => {
                self.sync_ui.provider_index = 1;
                self.sync_ui.drive_store.replace_text(&binding.drive_store);
            }
            "tailscale" => {
                self.sync_ui.provider_index = 2;
                self.sync_ui.tailnet_role_index = usize::from(binding.tailnet_role == "peer");
                self.sync_ui
                    .tailnet_host
                    .replace_text(&binding.tailnet_host);
                if binding.tailnet_port != 0 {
                    self.sync_ui
                        .tailnet_port
                        .replace_text(binding.tailnet_port.to_string());
                }
                self.sync_ui
                    .tailnet_allowed
                    .replace_text(&binding.tailnet_allowed);
            }
            // Settings refuse unknown providers, so this cannot happen in a
            // state built from disk; leave the form as it was.
            _ => return,
        }
        // The hosting callers (`State::new`, a vault switch) start the server
        // through `resume_tailnet_host`; they own the status line while it is
        // up, so do not replace their "Hosting on …" line here.
        if binding.provider == "tailscale"
            && binding.tailnet_role == "host"
            && self.sync_ui.host.is_some()
        {
            return;
        }
        self.sync_ui.status = if binding.provider == "tailscale" {
            // The Tailscale secret is a shared key, not an OAuth token.
            t(TextKey::SyncStatusConnectedSharedKey).to_owned()
        } else if binding.account.is_empty() {
            t(TextKey::SyncStatusConnectedToken).to_owned()
        } else {
            connected_as(&binding.account)
        };
    }

    fn start_tailnet_host(&self, binding: &VaultSyncBinding) -> Result<TailnetHost, String> {
        let id = BindingId::parse(&binding.binding_id).map_err(|error| sync_error_text(&error))?;
        let key = muninn_sync::CredentialKey {
            provider: ProviderKind::Tailscale,
            binding: id.clone(),
        };
        let token = self
            .sync_ui
            .credentials
            .get(&key)
            .map_err(|error| sync_error_text(&error))?
            .ok_or_else(|| t(TextKey::SyncErrorTailnetKeyMissing).to_owned())?;
        TailnetHost::start(TailnetHostConfig {
            port: binding.tailnet_port,
            store_dir: muninn_sync::tailnet_store_dir(&self.state_home.join("sync"), &id),
            key: token.access,
            allowed_logins: binding
                .tailnet_allowed
                .split(',')
                .map(str::trim)
                .filter(|login| !login.is_empty())
                .map(str::to_owned)
                .collect(),
        })
        .map_err(|error| sync_error_text(&error))
    }

    pub fn connect_sync(&mut self) {
        if !self.sync_ui.phase.is_idle() {
            self.sync_ui.status = t(TextKey::SyncStatusWaitForRun).to_owned();
            return;
        }
        if let Some(reason) = &self.settings.sync.disabled_reason {
            self.sync_ui.status = reason.clone();
            return;
        }
        let Ok(identity) = VaultIdentity::of_canonical(&self.vault.root) else {
            self.sync_ui.status = t(TextKey::SyncErrorVaultNotCanonical).to_owned();
            return;
        };
        let provider = match self.sync_ui.provider_index {
            0 => "github",
            1 => "google_drive",
            _ => "tailscale",
        };
        if provider == "tailscale" {
            self.connect_tailnet(identity.as_str().to_owned());
            return;
        }
        if provider == "github"
            && (self.sync_ui.github_owner.text.trim().is_empty()
                || self.sync_ui.github_repo.text.trim().is_empty())
        {
            self.sync_ui.status = t(TextKey::SyncErrorGithubScope).to_owned();
            return;
        }
        if provider == "google_drive" {
            let scope = muninn_sync::DriveScope {
                store: self.sync_ui.drive_store.text.trim().to_owned(),
            };
            if let Err(error) = scope.validate() {
                self.sync_ui.status = sync_error_text(&error);
                return;
            }
        }
        // The person's own client id when they entered one, else the id
        // bundled into this build; with neither, fail closed.
        let client = if provider == "github" {
            muninn_sync::client_id_for(ProviderKind::GitHub, &self.sync_ui.github_client_id.text)
                .map(SignInClient::GitHub)
        } else {
            muninn_sync::google_client_for(
                &self.sync_ui.google_client_id.text,
                self.sync_ui.sign_in.google_secret.clone(),
            )
            .map(SignInClient::Google)
        };
        let client = match client {
            Ok(client) => client,
            Err(error) => {
                self.sync_ui.status = sync_error_text(&error);
                return;
            }
        };
        let binding = VaultSyncBinding {
            identity: identity.as_str().to_owned(),
            provider: provider.to_owned(),
            binding_id: BindingId::fresh().as_str().to_owned(),
            account: String::new(),
            github_owner: self.sync_ui.github_owner.text.trim().to_owned(),
            github_repo: self.sync_ui.github_repo.text.trim().to_owned(),
            github_branch: {
                let branch = self.sync_ui.github_branch.text.trim();
                if branch.is_empty() {
                    "muninn-sync".to_owned()
                } else {
                    branch.to_owned()
                }
            },
            github_prefix: self.sync_ui.github_prefix.text.trim().to_owned(),
            drive_store: if provider == "google_drive" {
                self.sync_ui.drive_store.text.trim().to_owned()
            } else {
                String::new()
            },
            tailnet_role: String::new(),
            tailnet_host: String::new(),
            tailnet_port: 0,
            tailnet_allowed: String::new(),
            last_success_unix_ms: None,
            last_head: String::new(),
        };
        // Settings are not touched until sign-in succeeds: a cancelled or
        // failed Connect leaves the current binding and its token working.
        let binding_id = binding.binding_id.clone();
        self.sync_ui.pending = Some(binding);
        self.start_sign_in(binding_id, client);
    }

    /// The binding this vault is connected with now, in settings.
    fn binding_for(&self, identity: &str) -> Option<VaultSyncBinding> {
        self.settings
            .sync
            .vaults
            .iter()
            .find(|known| known.identity == identity)
            .cloned()
    }

    /// Make `binding` this vault's binding and save, then delete the
    /// credential of the binding it replaced (and stop its host). Called
    /// only once the new credential is stored.
    fn swap_binding(&mut self, binding: VaultSyncBinding) -> Result<(), String> {
        let old = self.binding_for(&binding.identity);
        self.settings
            .sync
            .vaults
            .retain(|known| known.identity != binding.identity);
        self.settings.sync.vaults.push(binding.clone());
        self.save_settings();
        let leftover = match old.filter(|old| old.binding_id != binding.binding_id) {
            None => Ok(()),
            Some(old) => {
                if old.provider == "tailscale" && old.tailnet_role == "host" {
                    self.sync_ui.host = None;
                    self.sync_ui.revealed_key = None;
                }
                match BindingId::parse(&old.binding_id) {
                    Err(_) => Ok(()),
                    Ok(id) => {
                        let provider =
                            ProviderKind::parse(&old.provider).unwrap_or(ProviderKind::GitHub);
                        self.sync_ui
                            .credentials
                            .delete(&muninn_sync::CredentialKey {
                                provider,
                                binding: id,
                            })
                            .map_err(|error| format!(" {}", sync_previous_credential(error)))
                    }
                }
            }
        };
        // Show the binding that just replaced the old one, so the form does
        // not keep the previous provider's values.
        self.reflect_sync_binding();
        leftover
    }

    /// Sign in on a worker thread: GitHub device flow or Google OAuth with
    /// PKCE and a loopback redirect, both in the system browser. The token
    /// goes straight to the OS credential store; settings never see it.
    fn start_sign_in(&mut self, binding_id: String, client: SignInClient) {
        let Ok(id) = BindingId::parse(&binding_id) else {
            return;
        };
        let provider = match client {
            SignInClient::GitHub(_) => ProviderKind::GitHub,
            SignInClient::Google(_) => ProviderKind::GoogleDrive,
        };
        self.sync_ui.sign_in_error = None;
        let (tx, rx) = mpsc::channel();
        self.sync_ui.inbox = Some(rx);
        self.sync_ui.phase = SyncPhase::Connecting;
        self.sync_ui.cancel = CancelToken::new();
        self.sync_ui.status = if provider == ProviderKind::GitHub {
            t(TextKey::SyncStatusAskingGithubCode).to_owned()
        } else {
            t(TextKey::SyncStatusOpeningGoogle).to_owned()
        };
        let cancel = self.sync_ui.cancel.clone();
        let credentials = Arc::clone(&self.sync_ui.credentials);
        let env = self.sync_ui.sign_in.clone();
        thread::spawn(move || {
            let mut events = ChannelEvents(tx.clone());
            let key = muninn_sync::CredentialKey {
                provider,
                binding: id,
            };
            // The token goes to the credential store inside the sign-in, so
            // Google's browser tab says "signed in" only once it is stored.
            let mut store = |token: &TokenSet| credentials.put(&key, token);
            let result =
                sign_in(&client, &env, cancel, &mut events, &mut store).map(|token| token.account);
            let _ = tx.send(SyncMessage::SignIn(SignInUpdate::Connected {
                binding_id,
                result: result.map_err(|error| error.to_string()),
            }));
        });
    }

    fn forget_credential(&mut self, binding_id: &str) {
        let Ok(binding) = BindingId::parse(binding_id) else {
            return;
        };
        for provider in [ProviderKind::GitHub, ProviderKind::GoogleDrive] {
            let _ = self
                .sync_ui
                .credentials
                .delete(&muninn_sync::CredentialKey {
                    provider,
                    binding: binding.clone(),
                });
        }
    }

    fn sign_in_update(&mut self, update: SignInUpdate) {
        match update {
            SignInUpdate::DeviceCode { user_code, uri } => {
                self.sync_ui.status = device_code(&user_code, &uri);
            }
            SignInUpdate::Waiting => {
                self.sync_ui.status = t(TextKey::SyncStatusWaitingGoogle).to_owned();
            }
            SignInUpdate::BrowserFailed { url } => {
                self.sync_ui.status = browser_failed(&url);
            }
            SignInUpdate::Connected { binding_id, result } => {
                self.sync_ui.phase = SyncPhase::Idle;
                match result {
                    Ok(account) => {
                        let pending = self
                            .sync_ui
                            .pending
                            .take()
                            .filter(|pending| pending.binding_id == binding_id);
                        let Some(mut binding) = pending else {
                            // Disconnected while the browser was open: do not
                            // keep a token for a binding that no longer exists.
                            self.forget_credential(&binding_id);
                            self.sync_ui.status =
                                t(TextKey::SyncStatusSignedInAfterDisconnect).to_owned();
                            return;
                        };
                        self.sync_ui.sign_in_error = None;
                        binding.account.clone_from(&account);
                        if binding.provider == "github" {
                            self.settings.sync.github_client_id =
                                self.sync_ui.github_client_id.text.trim().to_owned();
                        } else {
                            self.settings.sync.google_client_id =
                                self.sync_ui.google_client_id.text.trim().to_owned();
                        }
                        let leftover = self.swap_binding(binding).err().unwrap_or_default();
                        let mut status = if account.is_empty() {
                            t(TextKey::SyncStatusConnectedToken).to_owned()
                        } else {
                            connected_as(&account)
                        };
                        status.push_str(&leftover);
                        self.sync_ui.status = status;
                    }
                    Err(error) => {
                        self.sync_ui.pending = None;
                        let kept = if self.current_binding().is_some() {
                            t(TextKey::SyncStatusCurrentConnectionKept)
                        } else {
                            ""
                        };
                        let mut status = sign_in_failed(&error);
                        if !kept.is_empty() {
                            status.push(' ');
                            status.push_str(kept);
                        }
                        self.sync_ui.status = status;
                        self.sync_ui.sign_in_error = Some(self.sync_ui.status.clone());
                    }
                }
            }
        }
    }

    fn connect_tailnet(&mut self, identity: String) {
        let hosting = self.sync_ui.tailnet_role_index == 0;
        let host_name = self.sync_ui.tailnet_host.text.trim().to_ascii_lowercase();
        let port = if hosting {
            match self.sync_ui.tailnet_port.text.trim().parse::<u16>() {
                Ok(port) if port >= 1024 => port,
                _ => {
                    self.sync_ui.status = t(TextKey::SyncErrorLoopbackPort).to_owned();
                    return;
                }
            }
        } else {
            0
        };
        if !hosting {
            if let Err(error) = muninn_sync::validate_tailnet_host(&host_name) {
                self.sync_ui.status = sync_error_text(&error);
                return;
            }
            if self.sync_ui.tailnet_key.text.trim().len() < 32 {
                self.sync_ui.status = t(TextKey::SyncErrorPasteSharedKey).to_owned();
                return;
            }
        }
        let binding_id = BindingId::fresh();
        let key = if hosting {
            muninn_sync::new_shared_key()
        } else {
            Secret::new(self.sync_ui.tailnet_key.text.trim())
        };
        let credential = muninn_sync::CredentialKey {
            provider: ProviderKind::Tailscale,
            binding: binding_id.clone(),
        };
        let token = TokenSet {
            access: key.clone(),
            refresh: None,
            expires_unix_ms: None,
            account: String::new(),
        };
        if let Err(error) = self.sync_ui.credentials.put(&credential, &token) {
            self.sync_ui.status = sync_nothing_saved(error);
            return;
        }
        self.sync_ui.tailnet_key.replace_text("");
        let binding = VaultSyncBinding {
            identity,
            provider: "tailscale".to_owned(),
            binding_id: binding_id.as_str().to_owned(),
            account: String::new(),
            github_owner: String::new(),
            github_repo: String::new(),
            github_branch: String::new(),
            github_prefix: String::new(),
            drive_store: String::new(),
            tailnet_role: if hosting { "host" } else { "peer" }.to_owned(),
            tailnet_host: if hosting { String::new() } else { host_name },
            tailnet_port: port,
            tailnet_allowed: if hosting {
                self.sync_ui.tailnet_allowed.text.trim().to_owned()
            } else {
                String::new()
            },
            last_success_unix_ms: None,
            last_head: String::new(),
        };
        // The new key is stored; only now replace the old binding, then
        // delete the old key and stop the old host.
        let leftover = self.swap_binding(binding).err().unwrap_or_default();
        self.sync_ui.host = None;
        if hosting {
            self.resume_tailnet_host();
            if self.sync_ui.host.is_some() {
                let mut status = tailnet_hosting_ready(&muninn_sync::serve_command(port));
                status.push_str(&leftover);
                self.sync_ui.status = status;
            }
        } else {
            let mut status = t(TextKey::SyncStatusTailnetJoined).to_owned();
            status.push_str(&leftover);
            self.sync_ui.status = status;
        }
    }

    /// True when this vault's binding hosts a Tailscale commit store.
    #[must_use]
    pub fn hosting_tailnet(&self) -> bool {
        self.current_binding().is_some_and(|binding| {
            binding.provider == "tailscale" && binding.tailnet_role == "host"
        })
    }

    fn host_key_credential(&self) -> Result<muninn_sync::CredentialKey, String> {
        let binding = self
            .current_binding()
            .filter(|binding| binding.provider == "tailscale" && binding.tailnet_role == "host")
            .ok_or_else(|| t(TextKey::SyncErrorNotTailnetHost).to_owned())?;
        let id = BindingId::parse(&binding.binding_id).map_err(|error| sync_error_text(&error))?;
        Ok(muninn_sync::CredentialKey {
            provider: ProviderKind::Tailscale,
            binding: id,
        })
    }

    fn read_host_key(&self) -> Result<Secret, String> {
        let key = self.host_key_credential()?;
        self.sync_ui
            .credentials
            .get(&key)
            .map_err(|error| sync_error_text(&error))?
            .map(|token| token.access)
            .ok_or_else(|| t(TextKey::SyncErrorSharedKeyMissing).to_owned())
    }

    /// The host's shared key for a secret clipboard copy, or `None` with the
    /// reason in the status line.
    pub fn tailnet_key_for_copy(&mut self) -> Option<Secret> {
        match self.read_host_key() {
            Ok(key) => {
                self.sync_ui.status = shared_key_copied(TAILNET_KEY_CLIPBOARD.as_secs());
                Some(key)
            }
            Err(error) => {
                self.sync_ui.status = error;
                None
            }
        }
    }

    /// Show or hide the host's shared key in Settings.
    pub fn toggle_tailnet_key(&mut self) {
        if self.sync_ui.revealed_key.take().is_some() {
            return;
        }
        match self.read_host_key() {
            Ok(key) => self.sync_ui.revealed_key = Some(key),
            Err(error) => self.sync_ui.status = error,
        }
    }

    /// Replace the host's shared key and restart its server with the new
    /// one. Joined devices keep working only after they join again with it.
    pub fn rotate_tailnet_key(&mut self) {
        if !self.sync_ui.phase.is_idle() {
            self.sync_ui.status = t(TextKey::SyncStatusWaitToRotateKey).to_owned();
            return;
        }
        let credential = match self.host_key_credential() {
            Ok(credential) => credential,
            Err(error) => {
                self.sync_ui.status = error;
                return;
            }
        };
        let token = TokenSet {
            access: muninn_sync::new_shared_key(),
            refresh: None,
            expires_unix_ms: None,
            account: String::new(),
        };
        if let Err(error) = self.sync_ui.credentials.put(&credential, &token) {
            self.sync_ui.status = sync_key_rotation_failed(error);
            return;
        }
        self.sync_ui.revealed_key = None;
        // Dropping the server stops it; resume starts it with the new key.
        self.sync_ui.host = None;
        self.resume_tailnet_host();
        if self.sync_ui.host.is_some() {
            self.sync_ui.status = t(TextKey::SyncStatusKeyRotated).to_owned();
        }
    }

    pub fn disconnect_sync(&mut self) {
        self.sync_ui.cancel.cancel();
        // A sign-in still in the browser is abandoned; its token is deleted
        // when it arrives.
        self.sync_ui.pending = None;
        self.sync_ui.sign_in_error = None;
        self.sync_ui.host = None;
        self.sync_ui.revealed_key = None;
        let Some(binding) = self.current_binding().cloned() else {
            self.sync_ui.status = t(TextKey::SyncStatusNothingConnected).to_owned();
            return;
        };
        if let Ok(id) = BindingId::parse(&binding.binding_id) {
            let provider = ProviderKind::parse(&binding.provider).unwrap_or(ProviderKind::GitHub);
            let key = muninn_sync::CredentialKey {
                provider,
                binding: id,
            };
            if let Err(error) = self.sync_ui.credentials.delete(&key) {
                self.sync_ui.status = sync_error_text(&error);
                return;
            }
        }
        let identity = binding.identity.clone();
        self.settings
            .sync
            .vaults
            .retain(|known| known.identity != identity);
        self.save_settings();
        self.sync_ui.phase = SyncPhase::Idle;
        // The vault now has no binding; reset the form before saying why, so a
        // stale provider hint never outlives the connection.
        self.reflect_sync_binding();
        self.sync_ui.status = t(TextKey::SyncStatusDisconnected).to_owned();
    }

    pub fn sync_now(&mut self) {
        if self.dirty() || self.conflict.is_some() {
            self.sync_ui.status = t(TextKey::SyncStatusSaveBeforeSync).to_owned();
            return;
        }
        if self.sync_ui.phase.is_running() {
            self.sync_ui.status = t(TextKey::SyncStatusAlreadyRunning).to_owned();
            return;
        }
        if self.sync_ui.phase == SyncPhase::Connecting {
            self.sync_ui.status = t(TextKey::SyncStatusFinishSignIn).to_owned();
            return;
        }
        let Some(binding) = self.current_binding().cloned() else {
            // Keep why the last Connect failed; a generic hint would hide it.
            self.sync_ui.status = match &self.sync_ui.sign_in_error {
                Some(error) => sync_now_after_error(error),
                None => t(TextKey::SyncStatusConnectProvider).to_owned(),
            };
            return;
        };
        if let Some(reason) = &self.settings.sync.disabled_reason {
            self.sync_ui.status = reason.clone();
            return;
        }
        let request = match run_request(
            &binding,
            self.vault.root.clone(),
            self.state_home.join("sync"),
        ) {
            Ok(request) => request,
            Err(error) => {
                self.sync_ui.status = error;
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        self.sync_ui.inbox = Some(rx);
        self.sync_ui.phase = SyncPhase::Running;
        self.sync_ui.cancel = CancelToken::new();
        let cancel = self.sync_ui.cancel.clone();
        let credentials = Arc::clone(&self.sync_ui.credentials);
        // Refresh with the id that signed in: the saved override, else the
        // bundled id. An empty id makes refresh fail as an auth error.
        let github_client_id =
            muninn_sync::client_id_for(ProviderKind::GitHub, &self.settings.sync.github_client_id)
                .unwrap_or_default();
        // Google needs the bundled client and its secret; with a custom id or
        // no secret, a refresh fails closed without sending anything.
        let google_client = muninn_sync::google_client_for(
            &self.settings.sync.google_client_id,
            self.sync_ui.sign_in.google_secret.clone(),
        )
        .ok();
        thread::spawn(move || {
            let engine = match muninn_sync::ReqwestTransport::production() {
                Ok(transport) => muninn_sync::EngineParts {
                    transport,
                    credentials,
                    cancel,
                    sleeper: muninn_sync::production_sleeper(),
                    github_api: "https://api.github.com".to_owned(),
                    github_oauth: "https://github.com".to_owned(),
                    drive_api: "https://www.googleapis.com".to_owned(),
                    drive_upload: "https://www.googleapis.com/upload".to_owned(),
                    drive_oauth: "https://oauth2.googleapis.com".to_owned(),
                    github_client_id,
                    google_client,
                    tailnet_api: None,
                },
                Err(error) => {
                    let _ = tx.send(SyncMessage::Finished(Err(sync_error_text(&error))));
                    return;
                }
            };
            let result =
                muninn_sync::run_sync(&engine, &request).map_err(|error| sync_error_text(&error));
            let _ = tx.send(SyncMessage::Finished(result));
        });
        self.sync_ui.status = t(TextKey::SyncStatusSyncStarted).to_owned();
    }

    pub fn cancel_sync(&mut self) {
        self.sync_ui.cancel.cancel();
        self.sync_ui.status = t(TextKey::SyncStatusCancelling).to_owned();
    }

    pub fn poll_sync(&mut self) -> bool {
        let mut pending = Vec::new();
        if let Some(inbox) = self.sync_ui.inbox.as_ref() {
            while let Ok(message) = inbox.try_recv() {
                pending.push(message);
            }
        } else {
            return false;
        }
        if pending.is_empty() {
            return false;
        }
        for message in pending {
            match message {
                SyncMessage::Finished(result) => {
                    self.sync_ui.phase = SyncPhase::Idle;
                    self.finish_sync(result);
                }
                SyncMessage::SignIn(update) => self.sign_in_update(update),
            }
        }
        true
    }

    fn finish_sync(&mut self, result: Result<SyncReport, String>) {
        match result {
            Ok(SyncReport::Succeeded {
                head,
                exclusions,
                conflict_copies,
            }) => {
                if let Some(binding) = self.current_binding_mut() {
                    binding.last_head = head.clone();
                    binding.last_success_unix_ms = Some(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|duration| duration.as_millis() as u64)
                            .unwrap_or(0),
                    );
                }
                self.save_settings();
                let extra = if exclusions.is_empty() {
                    String::new()
                } else {
                    format!(" {}", sync_excluded(exclusions.len()))
                };
                let mut extra = extra;
                if !conflict_copies.is_empty() {
                    let names: Vec<&str> =
                        conflict_copies.iter().map(|path| path.as_str()).collect();
                    extra.push_str(&format!(" {}", sync_conflict_copies(&names.join(", "))));
                }
                self.sync_ui.status = last_sync(&head);
                self.sync_ui.status.push_str(&extra);
                if self.dirty() {
                    self.sync_ui.status.push(' ');
                    self.sync_ui
                        .status
                        .push_str(t(TextKey::SyncStatusNoteChangedWhileSyncing));
                } else {
                    let select = self.selected_path.clone();
                    self.reload(select);
                }
                self.refresh_watch();
            }
            Ok(SyncReport::Conflicts { conflicts }) => {
                // Not reached from the app (it keeps both by default), but
                // name the files if a run ever stops on conflicts.
                let names: Vec<&str> = conflicts
                    .iter()
                    .map(|conflict| conflict.path.as_str())
                    .collect();
                self.sync_ui.status = sync_conflicts(conflicts.len(), &names.join(", "));
            }
            Err(error) => self.sync_ui.status = error,
        }
    }

    fn current_binding(&self) -> Option<&VaultSyncBinding> {
        let identity = VaultIdentity::of_canonical(&self.vault.root).ok()?;
        self.settings
            .sync
            .vaults
            .iter()
            .find(|binding| binding.identity == identity.as_str())
    }

    fn current_binding_mut(&mut self) -> Option<&mut VaultSyncBinding> {
        let identity = VaultIdentity::of_canonical(&self.vault.root).ok()?;
        let key = identity.as_str().to_owned();
        self.settings
            .sync
            .vaults
            .iter_mut()
            .find(|binding| binding.identity == key)
    }
}

/// `sync_home` is `<state_home>/sync`. Each binding runs in its own state dir
/// under it; a Tailscale host's store stays at its existing path.
/// Which client a Connect signs in with.
enum SignInClient {
    GitHub(String),
    Google(muninn_sync::GoogleOAuthClient),
}

fn sign_in(
    client: &SignInClient,
    env: &SignInEnv,
    cancel: CancelToken,
    events: &mut ChannelEvents,
    store: &mut dyn FnMut(&TokenSet) -> Result<(), muninn_sync::SyncError>,
) -> Result<TokenSet, muninn_sync::SyncError> {
    let transport = if env.loopback {
        muninn_sync::ReqwestTransport::for_tests(&["127.0.0.1"])?
    } else {
        muninn_sync::ReqwestTransport::production()?
    };
    let oauth = match client {
        SignInClient::GitHub(client_id) => {
            let client = muninn_sync::GitHubClient {
                api_base: env.github_api.clone(),
                oauth_base: env.github_oauth.clone(),
                transport,
                cancel,
                sleeper: env.sleeper.as_ref(),
                request_timeout: Duration::from_secs(20),
            };
            let token = muninn_sync::github_device_sign_in(
                &client,
                client_id,
                env.browser.as_ref(),
                events,
            )?;
            store(&token)?;
            return Ok(token);
        }
        SignInClient::Google(oauth) => oauth,
    };
    let client = muninn_sync::DriveClient {
        api_base: String::new(),
        upload_base: String::new(),
        oauth_base: env.google_token.clone(),
        auth_base: env.google_auth.clone(),
        transport,
        cancel,
        sleeper: env.sleeper.as_ref(),
        request_timeout: Duration::from_secs(20),
    };
    muninn_sync::google_sign_in(
        &client,
        oauth,
        env.browser.as_ref(),
        events,
        env.google_timeout,
        store,
    )
}

fn run_request(
    binding: &VaultSyncBinding,
    root: std::path::PathBuf,
    sync_home: std::path::PathBuf,
) -> Result<RunRequest, String> {
    let binding_id =
        BindingId::parse(&binding.binding_id).map_err(|error| sync_error_text(&error))?;
    let state_dir = muninn_sync::binding_state_dir(&sync_home, &binding_id)
        .map_err(|error| sync_error_text(&error))?;
    let provider =
        ProviderKind::parse(&binding.provider).map_err(|error| sync_error_text(&error))?;
    let github = (provider == ProviderKind::GitHub).then(|| muninn_sync::GitHubScope {
        owner: binding.github_owner.clone(),
        repo: binding.github_repo.clone(),
        branch: binding.github_branch.clone(),
        prefix: binding.github_prefix.clone(),
    });
    let drive = (provider == ProviderKind::GoogleDrive).then(|| muninn_sync::DriveScope {
        store: binding.drive_store.clone(),
    });
    let tailnet = (provider == ProviderKind::Tailscale).then(|| TailnetScope {
        role: if binding.tailnet_role == "host" {
            TailnetRole::Host {
                store_dir: muninn_sync::tailnet_store_dir(&sync_home, &binding_id),
            }
        } else {
            TailnetRole::Peer {
                host: binding.tailnet_host.clone(),
            }
        },
    });
    Ok(RunRequest {
        root,
        state_dir,
        binding: binding_id,
        provider,
        github,
        drive,
        tailnet,
        operation: OperationId::fresh(),
        protected: Default::default(),
        // There are no per-file choices in the app yet. Keep both versions
        // of a note edited on two devices so sync never stops on it: the
        // local one keeps the path, the remote one lands beside it as
        // `<name> (remote).md`, and the status names the copies.
        resolutions: Vec::new(),
        default_resolution: Some(Resolution::KeepBoth),
    })
}

/// The status line for a sync failure, in the UI's words.
///
/// [`muninn_sync::SyncError`]'s `Display` is the diagnostic rendering used by
/// logs and by the developer-facing variants below. The sentences a person
/// reads are the catalog's; each keeps the provider's `detail`, path or
/// status as written.
fn sync_error_text(error: &muninn_sync::SyncError) -> String {
    use muninn_sync::SyncError;
    match error {
        SyncError::Offline { detail } => sync_offline(detail),
        SyncError::Timeout { mutation: true } => t(TextKey::SyncErrorTimedOutAfterSend).to_owned(),
        SyncError::Timeout { mutation: false } => {
            t(TextKey::SyncErrorTimedOutBeforeResponse).to_owned()
        }
        SyncError::Cancelled => t(TextKey::SyncErrorCancelled).to_owned(),
        SyncError::RateLimited { retry_after_ms } => match retry_after_ms {
            Some(ms) => sync_rate_limited_after(*ms),
            None => t(TextKey::SyncErrorRateLimited).to_owned(),
        },
        SyncError::Server { status, detail } => sync_provider_error(*status, detail),
        SyncError::Auth { detail } => sync_authentication_needed(detail),
        SyncError::Permission { detail } => sync_permission_denied(detail),
        SyncError::Scope { detail } => sync_oauth_scope_rejected(detail),
        SyncError::NotFound { detail } => sync_remote_scope_not_found(detail),
        SyncError::ConflictRemote { detail } => sync_remote_changed(detail),
        SyncError::OutcomeUnknown { detail } => sync_outcome_unknown(detail),
        SyncError::ListingIncomplete { detail } => sync_listing_incomplete(detail),
        SyncError::Unsupported { path, detail } => sync_unsupported(path, detail),
        SyncError::Limit { detail } => sync_limit_exceeded(detail),
        SyncError::Credentials { detail } => sync_credentials_unavailable(detail),
        SyncError::LeaseBusy => t(TextKey::SyncErrorLeaseBusy).to_owned(),
        SyncError::DirtyBuffer { path } => sync_dirty_buffer(path),
        SyncError::LocalChanged { path } => sync_local_changed(path),
        SyncError::Config { detail } => sync_not_configured(detail),
        SyncError::Io { detail } => sync_file_error(detail),
        // Developer diagnostics: they name an internal invariant rather than
        // a person's next step, so they keep the diagnostic rendering.
        SyncError::Path { .. }
        | SyncError::Inventory { .. }
        | SyncError::Journal { .. }
        | SyncError::Protocol { .. }
        | SyncError::InjectedFault { .. } => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::State;
    use muninn_i18n::{
        Key as TextKey, connected_as, last_sync, sign_in_failed, t, tailnet_hosting,
    };
    use std::fs;

    /// Leaving the Sync tab drops typing focus from every sync field and
    /// hides a revealed host key, as leaving the Vaults tab does for the
    /// path field: a hidden field must not keep focus or show a secret.
    #[test]
    fn leaving_the_sync_tab_drops_field_focus_and_hides_the_key() {
        let root = std::env::temp_dir().join(format!(
            "muninn-sync-tab-blur-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let state_home = root.with_extension("state");
        let config_home = root.with_extension("config");
        fs::create_dir_all(&state_home).unwrap();
        fs::create_dir_all(&config_home).unwrap();
        let mut state = State::with_settings(
            root.clone(),
            state_home.clone(),
            config_home.clone(),
            crate::settings::Settings::default(),
        );
        state.set_settings_tab(crate::SettingsTab::Sync);
        state.sync_ui.github_owner.focused = true;
        state.sync_ui.tailnet_host.focused = true;
        state.sync_ui.tailnet_key.focused = true;
        state.sync_ui.revealed_key = Some(muninn_sync::Secret::new("shown-key"));

        // Re-selecting the Sync tab is not leaving it.
        state.set_settings_tab(crate::SettingsTab::Sync);
        assert!(state.sync_ui.github_owner.focused);
        assert!(state.sync_ui.revealed_key.is_some());

        state.set_settings_tab(crate::SettingsTab::Editor);
        assert_eq!(state.settings_tab, crate::SettingsTab::Editor);
        assert!(
            !state.sync_ui.github_owner.focused
                && !state.sync_ui.tailnet_host.focused
                && !state.sync_ui.tailnet_key.focused,
            "a hidden sync field must not keep typing focus"
        );
        assert!(
            state.sync_ui.revealed_key.is_none(),
            "a hidden host key must not stay revealed"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&state_home);
        let _ = fs::remove_dir_all(&config_home);
    }

    /// A stand-in for the bundled Google client secret. Never the real one.
    const TEST_GOOGLE_SECRET: &str = "test-only-google-secret";

    /// Counts opens and refuses to answer: Connect must fail before it.
    #[derive(Default)]
    struct UnusedBrowser(std::sync::atomic::AtomicUsize);

    impl muninn_sync::BrowserOpener for UnusedBrowser {
        fn open(&self, _url: &str) -> Result<(), muninn_sync::SyncError> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    /// A credential store whose writes fail, like a locked keyring.
    struct LockedStore;

    impl muninn_sync::CredentialStore for LockedStore {
        fn get(
            &self,
            _key: &muninn_sync::CredentialKey,
        ) -> Result<Option<muninn_sync::TokenSet>, muninn_sync::SyncError> {
            Ok(None)
        }
        fn put(
            &self,
            _key: &muninn_sync::CredentialKey,
            _token: &muninn_sync::TokenSet,
        ) -> Result<(), muninn_sync::SyncError> {
            Err(muninn_sync::SyncError::Credentials {
                detail: "the keyring is locked".to_owned(),
            })
        }
        fn delete(&self, _key: &muninn_sync::CredentialKey) -> Result<(), muninn_sync::SyncError> {
            Ok(())
        }
    }

    #[test]
    fn drive_connect_without_a_built_in_secret_fails_closed() {
        let (mut state, root) = temp_state("no-secret");
        let browser = std::sync::Arc::new(UnusedBrowser::default());
        state.sync_ui.sign_in = super::SignInEnv {
            browser: browser.clone(),
            loopback: true,
            google_secret: None,
            ..super::SignInEnv::production()
        };
        state.set_sync_provider(1);
        state.connect_sync();
        assert_eq!(
            state.sync_ui.phase,
            super::SyncPhase::Idle,
            "no sign-in starts"
        );
        assert!(
            state.sync_ui.status.contains("MUNINN_GOOGLE_CLIENT_SECRET")
                && state.sync_ui.status.contains("Nothing was sent"),
            "{}",
            state.sync_ui.status
        );
        assert_eq!(browser.0.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(state.settings.sync.vaults.is_empty(), "no binding saved");
        remove_temp(&root);
    }

    #[test]
    fn drive_connect_with_a_custom_client_id_never_sends_the_official_secret() {
        let (mut state, root) = temp_state("custom-id");
        let browser = std::sync::Arc::new(UnusedBrowser::default());
        state.sync_ui.sign_in = super::SignInEnv {
            browser: browser.clone(),
            loopback: true,
            google_secret: Some(muninn_sync::Secret::new(TEST_GOOGLE_SECRET)),
            ..super::SignInEnv::production()
        };
        state.set_sync_provider(1);
        state
            .sync_ui
            .google_client_id
            .replace_text("mine.apps.googleusercontent.com");
        state.connect_sync();
        assert_eq!(
            state.sync_ui.phase,
            super::SyncPhase::Idle,
            "no sign-in starts"
        );
        assert!(
            state.sync_ui.status.contains("custom Google client id"),
            "{}",
            state.sync_ui.status
        );
        assert!(!state.sync_ui.status.contains(TEST_GOOGLE_SECRET));
        assert_eq!(browser.0.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(state.settings.sync.vaults.is_empty(), "no binding saved");
        remove_temp(&root);
    }

    #[test]
    fn drive_connect_is_not_connected_when_the_token_cannot_be_stored() {
        let (mut state, root) = temp_state("locked");
        state.sync_ui.credentials = std::sync::Arc::new(LockedStore);
        let token = token_server();
        state.sync_ui.sign_in = super::SignInEnv {
            google_token: format!("http://{token}"),
            browser: std::sync::Arc::new(ConsentingBrowser(true)),
            loopback: true,
            google_timeout: std::time::Duration::from_secs(10),
            google_secret: Some(muninn_sync::Secret::new(TEST_GOOGLE_SECRET)),
            ..super::SignInEnv::production()
        };
        state.set_sync_provider(1);
        state.connect_sync();
        wait_idle(&mut state);
        assert_eq!(
            state.sync_ui.status,
            sign_in_failed("Credentials unavailable: the keyring is locked")
        );
        assert!(state.settings.sync.vaults.is_empty(), "no binding saved");
        remove_temp(&root);
    }

    #[test]
    fn sync_now_keeps_the_sign_in_error_in_the_status() {
        let (mut state, root) = temp_state("keep-error");
        state.sync_ui.sign_in = super::SignInEnv {
            browser: std::sync::Arc::new(ConsentingBrowser(false)),
            loopback: true,
            google_timeout: std::time::Duration::from_secs(10),
            google_secret: Some(muninn_sync::Secret::new(TEST_GOOGLE_SECRET)),
            ..super::SignInEnv::production()
        };
        state.set_sync_provider(1);
        state.connect_sync();
        wait_idle(&mut state);
        let error = state.sync_ui.status.clone();
        assert!(error.starts_with("Sign-in did not finish"), "{error}");
        state.sync_now();
        assert!(
            state.sync_ui.status.starts_with(&error),
            "Sync Now replaced the sign-in error with: {}",
            state.sync_ui.status
        );
        assert!(
            !state
                .sync_ui
                .status
                .starts_with(t(TextKey::SyncStatusConnectProvider)),
            "{}",
            state.sync_ui.status
        );
        remove_temp(&root);
    }

    /// Answers Google's consent page on the loopback redirect: with a code
    /// when `true`, as a person who declined when `false`.
    struct ConsentingBrowser(bool);

    impl muninn_sync::BrowserOpener for ConsentingBrowser {
        fn open(&self, url: &str) -> Result<(), muninn_sync::SyncError> {
            // Tests avoid a URL crate: the query values here are plain or
            // percent-encoded ASCII.
            let (host_and_path, query) = url.split_once('?').unwrap();
            assert!(host_and_path.starts_with("https://accounts.google.com/"));
            let value = |name: &str| {
                query
                    .split('&')
                    .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
                    .map(percent_decode)
                    .unwrap()
            };
            let redirect = value("redirect_uri");
            let rest = redirect.strip_prefix("http://127.0.0.1:").unwrap();
            let (port, path) = rest.split_once('/').unwrap();
            let port: u16 = port.parse().unwrap();
            let target = if self.0 {
                format!(
                    "/{path}?state={}&code=consented&scope={}",
                    value("state"),
                    muninn_sync::DRIVE_SCOPE
                )
            } else {
                format!("/{path}?state={}&error=access_denied", value("state"))
            };
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
                write!(stream, "GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
                let mut sink = String::new();
                let _ = stream.read_to_string(&mut sink);
            });
            Ok(())
        }
    }

    /// Status of an authenticated state read against the local host server.
    fn host_status(port: u16, key: &str) -> u16 {
        use std::io::{Read, Write};
        let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) else {
            return 0;
        };
        if write!(
            stream,
            "GET {}state HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {key}\r\nConnection: close\r\n\r\n",
            muninn_sync::TAILNET_API_PREFIX
        )
        .is_err()
        {
            return 0;
        }
        let mut reply = String::new();
        let _ = stream.read_to_string(&mut reply);
        reply
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0)
    }

    /// Wait for a host to answer with `expected`. Binds and rotates are
    /// synchronous, but a loaded runner can still lose the first connection;
    /// a retry keeps the assertion about the key, not about scheduling.
    fn wait_for_host(port: u16, key: &str, expected: u16) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if host_status(port, key) == expected {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the host never answered {expected} on {port}"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    fn percent_decode(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 3 <= bytes.len() {
                out.push(u8::from_str_radix(&text[i + 1..i + 3], 16).unwrap());
                i += 3;
            } else {
                out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    /// One-route token endpoint on loopback for the code exchange.
    fn token_server() -> std::net::SocketAddr {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(1) {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0usize;
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() > 2 {
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                    line.clear();
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let body = String::from_utf8(body).unwrap();
                assert!(body.contains("grant_type=authorization_code"));
                assert!(body.contains("code=consented"));
                assert!(body.contains("code_verifier="));
                let reply = if body.contains(&format!("client_secret={TEST_GOOGLE_SECRET}")) {
                    format!(
                        "{{\"access_token\":\"ya29.ui\",\"refresh_token\":\"1//ui\",\"expires_in\":3599,\"scope\":\"{}\"}}",
                        muninn_sync::DRIVE_SCOPE
                    )
                } else {
                    "{\"error\":\"invalid_request\",\"error_description\":\"client_secret is missing.\"}".to_owned()
                };
                let status = if reply.contains("access_token") {
                    "200 OK"
                } else {
                    "400 Bad Request"
                };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                )
                .unwrap();
            }
        });
        addr
    }

    #[test]
    fn connect_drive_signs_in_through_the_browser_and_stores_the_token() {
        let root = std::env::temp_dir().join(format!(
            "muninn-sync-ui-signin-{}-{}",
            std::process::id(),
            muninn_sync::BindingId::fresh().as_str()
        ));
        fs::create_dir_all(&root).unwrap();
        let state_home = root.with_extension("state");
        let config_home = root.with_extension("config");
        fs::create_dir_all(&state_home).unwrap();
        fs::create_dir_all(&config_home).unwrap();
        let mut state = State::with_settings(
            root.clone(),
            state_home.clone(),
            config_home.clone(),
            crate::settings::Settings::default(),
        );
        let token = token_server();
        state.sync_ui.sign_in = super::SignInEnv {
            google_token: format!("http://{token}"),
            browser: std::sync::Arc::new(ConsentingBrowser(true)),
            loopback: true,
            google_timeout: std::time::Duration::from_secs(10),
            google_secret: Some(muninn_sync::Secret::new(TEST_GOOGLE_SECRET)),
            ..super::SignInEnv::production()
        };
        state.set_sync_provider(1);
        state.connect_sync();
        assert_eq!(state.sync_ui.phase, super::SyncPhase::Connecting);
        // Autosave is not held while waiting on the browser.
        assert!(!state.sync_ui.phase.is_running());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while state.sync_ui.phase != super::SyncPhase::Idle {
            assert!(
                std::time::Instant::now() < deadline,
                "{}",
                state.sync_ui.status
            );
            state.poll_sync();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(
            state.sync_ui.status,
            t(TextKey::SyncStatusConnectedToken),
            "{}",
            state.sync_ui.status
        );
        let binding = state.settings.sync.vaults[0].clone();
        assert_eq!(binding.drive_store, "Muninn");
        let stored = state
            .sync_ui
            .credentials
            .get(&muninn_sync::CredentialKey {
                provider: muninn_sync::ProviderKind::GoogleDrive,
                binding: muninn_sync::BindingId::parse(&binding.binding_id).unwrap(),
            })
            .unwrap()
            .unwrap();
        assert_eq!(stored.access.expose(), "ya29.ui");
        let saved = fs::read_to_string(config_home.join("muninn/settings.json")).unwrap();
        assert!(!saved.contains("ya29.ui") && !saved.contains("1//ui"));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&state_home);
        let _ = fs::remove_dir_all(&config_home);
    }

    fn temp_state(tag: &str) -> (State, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "muninn-sync-ui-{tag}-{}-{}",
            std::process::id(),
            muninn_sync::BindingId::fresh().as_str()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Note.md"), "keep me").unwrap();
        for ext in ["state", "config"] {
            fs::create_dir_all(root.with_extension(ext)).unwrap();
        }
        let state = State::with_settings(
            root.clone(),
            root.with_extension("state"),
            root.with_extension("config"),
            crate::settings::Settings::default(),
        );
        (state, root)
    }

    fn remove_temp(root: &std::path::Path) {
        for path in [
            root.to_path_buf(),
            root.with_extension("state"),
            root.with_extension("config"),
        ] {
            let _ = fs::remove_dir_all(path);
        }
    }

    fn wait_idle(state: &mut State) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while state.sync_ui.phase != super::SyncPhase::Idle {
            assert!(
                std::time::Instant::now() < deadline,
                "{}",
                state.sync_ui.status
            );
            state.poll_sync();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    fn stored(state: &State, binding: &crate::settings::VaultSyncBinding) -> Option<String> {
        state
            .sync_ui
            .credentials
            .get(&muninn_sync::CredentialKey {
                provider: muninn_sync::ProviderKind::parse(&binding.provider).unwrap(),
                binding: muninn_sync::BindingId::parse(&binding.binding_id).unwrap(),
            })
            .unwrap()
            .map(|token| token.access.expose().to_owned())
    }

    #[test]
    fn connect_replaces_the_old_binding_only_after_sign_in_succeeds() {
        let (mut state, root) = temp_state("swap");
        // The vault is already joined to a Tailscale host.
        state.set_sync_provider(2);
        state.set_tailnet_role(1);
        state
            .sync_ui
            .tailnet_host
            .replace_text("minis.tail1234.ts.net");
        state.sync_ui.tailnet_key.replace_text("k".repeat(40));
        state.connect_sync();
        let old = state.settings.sync.vaults[0].clone();
        assert_eq!(stored(&state, &old), Some("k".repeat(40)));

        // Switching to Drive, but the person declines in the browser.
        state.sync_ui.sign_in = super::SignInEnv {
            browser: std::sync::Arc::new(ConsentingBrowser(false)),
            loopback: true,
            google_timeout: std::time::Duration::from_secs(10),
            google_secret: Some(muninn_sync::Secret::new(TEST_GOOGLE_SECRET)),
            ..super::SignInEnv::production()
        };
        state.set_sync_provider(1);
        state.connect_sync();
        assert_eq!(
            state.settings.sync.vaults,
            vec![old.clone()],
            "nothing is replaced while signing in"
        );
        wait_idle(&mut state);
        assert!(
            state
                .sync_ui
                .status
                .contains(t(TextKey::SyncStatusCurrentConnectionKept)),
            "{}",
            state.sync_ui.status
        );
        assert_eq!(state.settings.sync.vaults, vec![old.clone()]);
        let reloaded = crate::settings::Settings::load_from(&root.with_extension("config"));
        assert_eq!(reloaded.sync.vaults, vec![old.clone()]);
        assert_eq!(stored(&state, &old), Some("k".repeat(40)));

        // This time sign-in succeeds: the Drive binding replaces the old one
        // and the old key is deleted.
        let token = token_server();
        state.sync_ui.sign_in = super::SignInEnv {
            google_token: format!("http://{token}"),
            browser: std::sync::Arc::new(ConsentingBrowser(true)),
            loopback: true,
            google_timeout: std::time::Duration::from_secs(10),
            google_secret: Some(muninn_sync::Secret::new(TEST_GOOGLE_SECRET)),
            ..super::SignInEnv::production()
        };
        state.connect_sync();
        wait_idle(&mut state);
        assert_eq!(
            state.sync_ui.status,
            t(TextKey::SyncStatusConnectedToken),
            "{}",
            state.sync_ui.status
        );
        assert_eq!(state.settings.sync.vaults.len(), 1);
        let new = state.settings.sync.vaults[0].clone();
        assert_eq!(new.provider, "google_drive");
        assert_ne!(new.binding_id, old.binding_id);
        assert_eq!(stored(&state, &new).as_deref(), Some("ya29.ui"));
        assert_eq!(stored(&state, &old), None, "the old key is deleted");
        let reloaded = crate::settings::Settings::load_from(&root.with_extension("config"));
        assert_eq!(reloaded.sync.vaults, vec![new]);
        remove_temp(&root);
    }

    #[test]
    fn tailscale_reconnect_deletes_the_old_key_and_stops_the_old_host() {
        let (mut state, root) = temp_state("ts-swap");
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);
        state.set_sync_provider(2);
        state.set_tailnet_role(0);
        state.sync_ui.tailnet_port.replace_text(port.to_string());
        state.connect_sync();
        let old = state.settings.sync.vaults[0].clone();
        let key = stored(&state, &old).unwrap();
        wait_for_host(port, &key, 200);

        state.set_tailnet_role(1);
        state
            .sync_ui
            .tailnet_host
            .replace_text("minis.tail1234.ts.net");
        state.sync_ui.tailnet_key.replace_text("p".repeat(40));
        state.connect_sync();
        assert_eq!(state.settings.sync.vaults.len(), 1);
        let new = state.settings.sync.vaults[0].clone();
        assert_eq!(new.tailnet_role, "peer");
        assert_eq!(stored(&state, &new), Some("p".repeat(40)));
        assert_eq!(stored(&state, &old), None, "the old host key is deleted");
        assert!(!state.hosting_tailnet());
        // The host stops asynchronously, so a single probe can race the
        // close and find the port still accepting (the #170 CI flake). Poll
        // until it refuses, with a bound so a host that really stays up fails.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            assert!(std::time::Instant::now() < deadline, "the old host stopped");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        state.disconnect_sync();
        remove_temp(&root);
    }

    fn current_identity(state: &State) -> String {
        super::VaultIdentity::of_canonical(&state.vault.root)
            .unwrap()
            .as_str()
            .to_owned()
    }

    /// A binding for `provider` on the vault `state` has open.
    fn binding_for(state: &State, provider: &str) -> crate::settings::VaultSyncBinding {
        crate::settings::VaultSyncBinding {
            identity: current_identity(state),
            provider: provider.to_owned(),
            binding_id: muninn_sync::BindingId::fresh().as_str().to_owned(),
            ..Default::default()
        }
    }

    /// After a restart the Sync tab and the tray must show the saved Drive
    /// binding instead of the fresh form's GitHub/"Not connected" defaults.
    #[test]
    fn sync_ui_rehydrates_a_drive_binding_after_a_restart() {
        let (state, root) = temp_state("rehydrate-drive");
        let mut settings = state.settings.clone();
        settings
            .sync
            .vaults
            .push(crate::settings::VaultSyncBinding {
                account: "person@example.com".to_owned(),
                drive_store: "Work".to_owned(),
                ..binding_for(&state, "google_drive")
            });
        drop(state);

        // A fresh State stands in for restarting the app: build only has the
        // persisted settings and must rehydrate the form from the binding.
        let mut restarted = State::with_settings(
            root.clone(),
            root.with_extension("state"),
            root.with_extension("config"),
            settings,
        );
        assert_eq!(restarted.sync_ui.provider_index, 1);
        assert_eq!(restarted.sync_ui.drive_store.text, "Work");
        assert!(
            !restarted
                .sync_status()
                .starts_with(t(TextKey::SyncStatusNotConnected)),
            "{}",
            restarted.sync_status()
        );
        assert_eq!(
            restarted.sync_status(),
            connected_as("person@example.com"),
            "{}",
            restarted.sync_status()
        );
        assert!(restarted.sync_ready());
        assert_eq!(
            crate::tray::tray_status_label(restarted.sync_status()),
            "Sync: Connected as person@example.com."
        );

        // Disconnect still works against a rehydrated form.
        restarted.disconnect_sync();
        assert!(restarted.settings.sync.vaults.is_empty());
        assert!(!restarted.sync_ready());
        assert_eq!(
            restarted.sync_status(),
            t(TextKey::SyncStatusDisconnected),
            "{}",
            restarted.sync_status()
        );
        drop(restarted);
        remove_temp(&root);
    }

    /// GitHub and Tailscale bindings map onto their tab indexes and fields.
    #[test]
    fn sync_ui_rehydrates_github_and_tailscale_mappings() {
        let (mut state, root) = temp_state("rehydrate-map");
        state
            .settings
            .sync
            .vaults
            .push(crate::settings::VaultSyncBinding {
                account: "octo".to_owned(),
                github_owner: "muninn".to_owned(),
                github_repo: "notes".to_owned(),
                github_branch: "sync".to_owned(),
                github_prefix: "vault".to_owned(),
                ..binding_for(&state, "github")
            });
        state.reflect_sync_binding();
        assert_eq!(state.sync_ui.provider_index, 0);
        assert_eq!(state.sync_ui.github_owner.text, "muninn");
        assert_eq!(state.sync_ui.github_repo.text, "notes");
        assert_eq!(state.sync_ui.github_branch.text, "sync");
        assert_eq!(state.sync_ui.github_prefix.text, "vault");
        assert_eq!(
            state.sync_status(),
            connected_as("octo"),
            "{}",
            state.sync_status()
        );
        assert!(state.sync_ready());

        // The same vault bound to a Tailscale host: index 2, host role and
        // the port/allow list come back from settings.
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);
        let host = crate::settings::VaultSyncBinding {
            tailnet_role: "host".to_owned(),
            tailnet_port: port,
            tailnet_allowed: "alice@example.com".to_owned(),
            ..binding_for(&state, "tailscale")
        };
        state
            .sync_ui
            .credentials
            .put(
                &muninn_sync::CredentialKey {
                    provider: muninn_sync::ProviderKind::Tailscale,
                    binding: muninn_sync::BindingId::parse(&host.binding_id).unwrap(),
                },
                &muninn_sync::TokenSet {
                    access: muninn_sync::new_shared_key(),
                    refresh: None,
                    expires_unix_ms: None,
                    account: String::new(),
                },
            )
            .unwrap();
        state.settings.sync.vaults.clear();
        state.settings.sync.vaults.push(host);
        state.reflect_sync_binding();
        state.resume_tailnet_host();
        assert_eq!(state.sync_ui.provider_index, 2);
        assert_eq!(state.sync_ui.tailnet_role_index, 0);
        assert_eq!(state.sync_ui.tailnet_port.text, port.to_string());
        assert_eq!(state.sync_ui.tailnet_allowed.text, "alice@example.com");
        assert!(
            state.sync_status().starts_with(&tailnet_hosting(port, "")),
            "{}",
            state.sync_status()
        );
        assert!(state.sync_ready());
        state.disconnect_sync();

        // A peer keeps its host name and the join role.
        let peer = crate::settings::VaultSyncBinding {
            tailnet_role: "peer".to_owned(),
            tailnet_host: "minis.tail1234.ts.net".to_owned(),
            ..binding_for(&state, "tailscale")
        };
        state.settings.sync.vaults.push(peer);
        state.reflect_sync_binding();
        assert_eq!(state.sync_ui.provider_index, 2);
        assert_eq!(state.sync_ui.tailnet_role_index, 1);
        assert_eq!(state.sync_ui.tailnet_host.text, "minis.tail1234.ts.net");
        assert_eq!(
            state.sync_status(),
            t(TextKey::SyncStatusConnectedSharedKey),
            "{}",
            state.sync_status()
        );
        assert!(state.sync_ready());
        state.disconnect_sync();
        remove_temp(&root);
    }

    /// Switching vaults changes the binding the form shows.
    #[test]
    fn switching_vaults_rehydrates_that_vaults_binding() {
        let (mut state, root) = temp_state("rehydrate-switch");
        let other = root.join("other-vault");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("Note.md"), "other").unwrap();
        state
            .settings
            .sync
            .vaults
            .push(crate::settings::VaultSyncBinding {
                account: "octo".to_owned(),
                github_owner: "muninn".to_owned(),
                github_repo: "notes".to_owned(),
                ..binding_for(&state, "github")
            });
        state
            .settings
            .sync
            .vaults
            .push(crate::settings::VaultSyncBinding {
                identity: super::VaultIdentity::of_canonical(&other)
                    .unwrap()
                    .as_str()
                    .to_owned(),
                provider: "google_drive".to_owned(),
                binding_id: muninn_sync::BindingId::fresh().as_str().to_owned(),
                account: "other@example.com".to_owned(),
                drive_store: "OtherVault".to_owned(),
                ..Default::default()
            });
        state.reflect_sync_binding();
        assert_eq!(state.sync_ui.provider_index, 0);

        assert!(state.switch_vault(other.clone()));
        assert_eq!(state.sync_ui.provider_index, 1);
        assert_eq!(state.sync_ui.drive_store.text, "OtherVault");
        assert_eq!(
            state.sync_status(),
            connected_as("other@example.com"),
            "{}",
            state.sync_status()
        );

        assert!(state.switch_vault(root.clone()));
        assert_eq!(state.sync_ui.provider_index, 0);
        assert_eq!(state.sync_ui.github_owner.text, "muninn");
        assert_eq!(
            state.sync_status(),
            connected_as("octo"),
            "{}",
            state.sync_status()
        );
        remove_temp(&root);
    }

    /// A run or sign-in in flight owns the form; rehydration must not move it.
    #[test]
    fn rehydration_leaves_a_busy_sync_form_alone() {
        let (mut state, root) = temp_state("rehydrate-busy");
        state
            .settings
            .sync
            .vaults
            .push(binding_for(&state, "google_drive"));
        state.sync_ui.provider_index = 2;
        state.sync_ui.status = t(TextKey::SyncStatusSyncStarted).to_owned();
        state.sync_ui.phase = super::SyncPhase::Running;
        state.reflect_sync_binding();
        assert_eq!(state.sync_ui.provider_index, 2);
        assert_eq!(state.sync_ui.status, t(TextKey::SyncStatusSyncStarted));

        state.sync_ui.phase = super::SyncPhase::Connecting;
        state.sync_ui.status = "Waiting for the browser.".to_owned();
        state.reflect_sync_binding();
        assert_eq!(state.sync_ui.provider_index, 2);
        assert_eq!(state.sync_ui.status, "Waiting for the browser.");
        state.sync_ui.phase = super::SyncPhase::Idle;
        remove_temp(&root);
    }

    /// The tray's Sync now is enabled only when Sync Now could start a run.
    #[test]
    fn sync_ready_needs_a_binding_and_an_idle_unblocked_sync() {
        let (mut state, root) = temp_state("ready");
        assert!(!state.sync_ready(), "nothing connected");
        assert!(!state.sync_busy());
        let identity = super::VaultIdentity::of_canonical(&state.vault.root).unwrap();
        state
            .settings
            .sync
            .vaults
            .push(crate::settings::VaultSyncBinding {
                identity: identity.as_str().to_owned(),
                provider: "github".to_owned(),
                binding_id: muninn_sync::BindingId::fresh().as_str().to_owned(),
                ..Default::default()
            });
        assert!(state.sync_ready(), "connected and idle");
        state.sync_ui.phase = super::SyncPhase::Running;
        assert!(
            !state.sync_ready() && state.sync_busy(),
            "a run is in flight"
        );
        state.sync_ui.phase = super::SyncPhase::Connecting;
        assert!(!state.sync_ready() && state.sync_busy(), "signing in");
        state.sync_ui.phase = super::SyncPhase::Idle;
        state.settings.sync.disabled_reason = Some("Sync settings are unreadable.".to_owned());
        assert!(!state.sync_ready(), "sync is disabled");
        drop(state);
        remove_temp(&root);
    }

    /// A window closed to the tray polls only on timers, so a running sync
    /// keeps `poll` awake until its result lands.
    #[test]
    fn poll_keeps_waking_while_a_sync_is_in_flight() {
        let (state, root) = temp_state("wake");
        let mut rt = rune::Runtime::new(
            state,
            crate::view,
            (1180.0, 780.0),
            rune::Env::new(true, 2.0),
        );
        rt.poll = Some(crate::poll);
        for _ in 0..4 {
            rt.frame(1.0 / 60.0);
        }
        let idle = rt.wake_deadline();
        rt.state.sync_ui.phase = super::SyncPhase::Running;
        rt.frame(1.0 / 60.0);
        let busy = rt
            .wake_deadline()
            .expect("a wake is armed while a sync runs");
        assert!(
            idle.is_none_or(|idle| busy <= idle),
            "the sync wake is the soonest: {busy} vs {idle:?}"
        );
        rt.state.sync_ui.phase = super::SyncPhase::Idle;
        drop(rt);
        remove_temp(&root);
    }

    #[test]
    fn each_binding_runs_in_its_own_state_dir() {
        let sync_home = std::env::temp_dir().join(format!(
            "muninn-sync-ui-dirs-{}-{}",
            std::process::id(),
            muninn_sync::BindingId::fresh().as_str()
        ));
        let binding = |id: muninn_sync::BindingId, role: &str| crate::settings::VaultSyncBinding {
            provider: "tailscale".to_owned(),
            binding_id: id.as_str().to_owned(),
            tailnet_role: role.to_owned(),
            tailnet_host: "minis.tail1234.ts.net".to_owned(),
            ..Default::default()
        };
        let host_id = muninn_sync::BindingId::fresh();
        let host = super::run_request(
            &binding(host_id.clone(), "host"),
            sync_home.join("vault-a"),
            sync_home.clone(),
        )
        .unwrap();
        let peer = super::run_request(
            &binding(muninn_sync::BindingId::fresh(), "peer"),
            sync_home.join("vault-b"),
            sync_home.clone(),
        )
        .unwrap();
        assert_ne!(host.state_dir, peer.state_dir);
        assert!(host.state_dir.starts_with(sync_home.join("bindings")));
        // The host's commit store stays where earlier builds put it.
        let Some(muninn_sync::TailnetScope {
            role: muninn_sync::TailnetRole::Host { store_dir },
        }) = host.tailnet
        else {
            panic!("expected a host scope");
        };
        assert_eq!(
            store_dir,
            muninn_sync::tailnet_store_dir(&sync_home, &host_id)
        );
        let _ = fs::remove_dir_all(&sync_home);
    }

    /// C1: the app has no per-file conflict choices, so every run it starts
    /// must carry a default that keeps both versions; an empty choice list
    /// alone would stop sync for the vault on the first same-note edit.
    #[test]
    fn every_provider_run_keeps_both_versions_of_a_conflict_by_default() {
        let sync_home = std::env::temp_dir().join(format!(
            "muninn-sync-ui-c1-{}-{}",
            std::process::id(),
            muninn_sync::BindingId::fresh().as_str()
        ));
        for provider in [
            muninn_sync::ProviderKind::GitHub,
            muninn_sync::ProviderKind::GoogleDrive,
            muninn_sync::ProviderKind::Tailscale,
        ] {
            let binding = crate::settings::VaultSyncBinding {
                provider: provider.as_str().to_owned(),
                binding_id: muninn_sync::BindingId::fresh().as_str().to_owned(),
                tailnet_role: "peer".to_owned(),
                tailnet_host: "minis.tail1234.ts.net".to_owned(),
                ..Default::default()
            };
            let request =
                super::run_request(&binding, sync_home.join("vault"), sync_home.clone()).unwrap();
            assert_eq!(request.provider, provider);
            assert!(request.resolutions.is_empty());
            assert_eq!(
                request.default_resolution,
                Some(muninn_sync::Resolution::KeepBoth),
                "{provider:?}"
            );
        }
        let _ = fs::remove_dir_all(&sync_home);
    }

    #[test]
    fn tailscale_host_and_peer_keep_the_key_out_of_settings() {
        let root = std::env::temp_dir().join(format!(
            "muninn-sync-ui-ts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Note.md"), "keep me").unwrap();
        let state_home = root.with_extension("state");
        let config_home = root.with_extension("config");
        fs::create_dir_all(&state_home).unwrap();
        fs::create_dir_all(&config_home).unwrap();
        let mut state = State::with_settings(
            root.clone(),
            state_home.clone(),
            config_home.clone(),
            crate::settings::Settings::default(),
        );
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);
        state.set_sync_provider(2);
        state.set_tailnet_role(0);
        state.sync_ui.tailnet_port.replace_text(port.to_string());
        state.connect_sync();
        assert!(
            state
                .sync_status()
                .contains("tailscale serve --bg --https=443"),
            "{}",
            state.sync_status()
        );
        let saved = fs::read_to_string(config_home.join("muninn/settings.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(json["sync"]["vaults"][0]["tailnet_role"], "host");
        let key = state.read_host_key().unwrap().expose().to_owned();
        assert!(key.len() >= 32);
        assert!(
            !saved.contains(&key),
            "the shared key must not reach settings"
        );
        assert!(
            !state.sync_status().contains(&key),
            "the shared key must not be in the status line"
        );
        assert!(state.hosting_tailnet());

        // Show reads the key on demand; Hide drops it.
        assert!(state.sync_ui.revealed_key.is_none());
        state.toggle_tailnet_key();
        assert_eq!(state.sync_ui.revealed_key.as_ref().unwrap().expose(), key);
        state.toggle_tailnet_key();
        assert!(state.sync_ui.revealed_key.is_none());
        assert_eq!(state.tailnet_key_for_copy().unwrap().expose(), key);
        assert!(!state.sync_status().contains(&key));
        wait_for_host(port, &key, 200);

        // Rotate: a new key, the server restarted with it, the old one dead.
        state.toggle_tailnet_key();
        state.rotate_tailnet_key();
        assert!(state.sync_ui.revealed_key.is_none());
        let rotated = state.read_host_key().unwrap().expose().to_owned();
        assert_ne!(rotated, key);
        assert!(!state.sync_status().contains(&rotated));
        // Wait for the restarted host before asking about either key; the
        // drop/rebind window can answer with an empty reply on a loaded runner.
        wait_for_host(port, &rotated, 200);
        assert_eq!(host_status(port, &key), 401);
        state.disconnect_sync();
        assert!(state.sync_ui.revealed_key.is_none());
        assert!(state.settings.sync.vaults.is_empty());
        assert_eq!(fs::read_to_string(root.join("Note.md")).unwrap(), "keep me");

        state.set_sync_provider(2);
        state.set_tailnet_role(1);
        assert!(
            state.sync_ui.tailnet_key.is_secure(),
            "the join field is masked"
        );
        state.sync_ui.tailnet_host.replace_text("minis");
        state.sync_ui.tailnet_key.replace_text("k".repeat(40));
        state.connect_sync();
        assert!(
            state.settings.sync.vaults.is_empty(),
            "a short name is refused"
        );
        state
            .sync_ui
            .tailnet_host
            .replace_text("minis.tail1234.ts.net");
        state.connect_sync();
        assert_eq!(state.settings.sync.vaults.len(), 1);
        assert!(state.sync_ui.tailnet_key.text.is_empty());
        assert!(state.sync_ui.tailnet_key.is_secure());
        let saved = fs::read_to_string(config_home.join("muninn/settings.json")).unwrap();
        assert!(saved.contains("minis.tail1234.ts.net"));
        assert!(!saved.contains(&"k".repeat(40)));
        let reloaded = crate::settings::Settings::load_from(&config_home);
        assert_eq!(reloaded.sync.vaults, state.settings.sync.vaults);
        state.disconnect_sync();
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&state_home);
        let _ = fs::remove_dir_all(&config_home);
    }

    /// The already-running refusal and a finished run's line come from the
    /// catalog, not from literals at the transition.
    #[test]
    fn running_and_last_sync_statuses_read_the_catalog() {
        let (mut state, root) = temp_state("catalog-status");
        state.sync_ui.phase = super::SyncPhase::Running;
        state.sync_now();
        assert_eq!(state.sync_ui.status, t(TextKey::SyncStatusAlreadyRunning));

        state.sync_ui.phase = super::SyncPhase::Idle;
        state.finish_sync(Ok(muninn_sync::SyncReport::Succeeded {
            head: "abc123".to_owned(),
            exclusions: Vec::new(),
            conflict_copies: Vec::new(),
        }));
        assert_eq!(state.sync_ui.status, last_sync("abc123"));
        assert_eq!(state.sync_ui.status, "Last successful sync abc123.");
        remove_temp(&root);
    }

    /// Every catalogued run failure renders exactly as the diagnostic
    /// `Display` did before the extraction; only the five developer variants
    /// still use that rendering.
    #[test]
    fn sync_error_status_matches_the_diagnostic_rendering() {
        use muninn_sync::SyncError;
        let errors = [
            SyncError::Offline {
                detail: "connection refused".to_owned(),
            },
            SyncError::Timeout { mutation: true },
            SyncError::Timeout { mutation: false },
            SyncError::Cancelled,
            SyncError::RateLimited {
                retry_after_ms: Some(1500),
            },
            SyncError::RateLimited {
                retry_after_ms: None,
            },
            SyncError::Server {
                status: 500,
                detail: "internal error".to_owned(),
            },
            SyncError::Auth {
                detail: "token expired".to_owned(),
            },
            SyncError::Permission {
                detail: "no access".to_owned(),
            },
            SyncError::Scope {
                detail: "drive.file required".to_owned(),
            },
            SyncError::NotFound {
                detail: "branch main".to_owned(),
            },
            SyncError::ConflictRemote {
                detail: "commit moved".to_owned(),
            },
            SyncError::OutcomeUnknown {
                detail: "connection dropped".to_owned(),
            },
            SyncError::ListingIncomplete {
                detail: "page 3 of 5".to_owned(),
            },
            SyncError::Unsupported {
                path: "Data.xlsx".to_owned(),
                detail: "Workspace document".to_owned(),
            },
            SyncError::Limit {
                detail: "file larger than 100 MB".to_owned(),
            },
            SyncError::Credentials {
                detail: "the keyring is locked".to_owned(),
            },
            SyncError::LeaseBusy,
            SyncError::DirtyBuffer {
                path: "Plan.md".to_owned(),
            },
            SyncError::LocalChanged {
                path: "Plan.md".to_owned(),
            },
            SyncError::Config {
                detail: "missing repository".to_owned(),
            },
            SyncError::Io {
                detail: "read-only folder".to_owned(),
            },
        ];
        for error in errors {
            assert_eq!(
                super::sync_error_text(&error),
                error.to_string(),
                "{error:?}"
            );
        }
    }
}
