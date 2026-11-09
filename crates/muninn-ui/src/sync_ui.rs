//! Settings-owned sync session.
//!
//! Sync Now is manual. Network work runs on a worker thread. While a run is
//! in flight (`phase == Running`), autosave and the metadata watcher do not
//! write or reload across the batch. Disconnect removes the binding and its
//! credential only.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use muninn_sync::{
    BindingId, CancelToken, CredentialStore, MemoryCredentialStore, NativeCredentialStore,
    OperationId, ProviderKind, RunRequest, Secret, SyncReport, TAILNET_DEFAULT_PORT, TailnetHost,
    TailnetHostConfig, TailnetRole, TailnetScope, TokenSet, VaultIdentity,
};
use rune::TextEdit;

use crate::State;
use crate::settings::VaultSyncBinding;

pub struct SyncUi {
    pub provider_index: usize,
    pub status: String,
    pub github_owner: TextEdit,
    pub github_repo: TextEdit,
    pub github_branch: TextEdit,
    pub github_prefix: TextEdit,
    pub drive_folder_id: TextEdit,
    pub drive_folder_name: TextEdit,
    pub github_client_id: TextEdit,
    pub google_client_id: TextEdit,
    /// Tailscale: 0 = this device hosts, 1 = join a host.
    pub tailnet_role_index: usize,
    pub tailnet_host: TextEdit,
    /// Peer only. Cleared as soon as the key is in the OS credential store.
    pub tailnet_key: TextEdit,
    pub tailnet_port: TextEdit,
    pub tailnet_allowed: TextEdit,
    /// Sheet-sync machinery state. While a run is in flight `phase` is
    /// `Running`, and nothing else (autosave, watcher, a second Sync Now) is
    /// allowed to move. `Idle` means the last result has been consumed.
    pub phase: SyncPhase,
    /// Headless scenes show only the sync section so the controls fit.
    pub preview: bool,
    credentials: Arc<dyn CredentialStore>,
    cancel: CancelToken,
    inbox: Option<Receiver<SyncMessage>>,
    /// Running commit-store server while this device hosts a Tailscale binding.
    host: Option<TailnetHost>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SyncPhase {
    /// No run in flight.
    #[default]
    Idle,
    /// A worker thread owns the run until `poll_sync` consumes its result.
    Running,
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
            status: "Not connected. Choose a provider and an explicit remote, then Connect."
                .to_owned(),
            github_owner: TextEdit::new(""),
            github_repo: TextEdit::new(""),
            github_branch: TextEdit::new("muninn-sync"),
            github_prefix: TextEdit::new("vault"),
            drive_folder_id: TextEdit::new(""),
            drive_folder_name: TextEdit::new(""),
            github_client_id: TextEdit::new(""),
            google_client_id: TextEdit::new(""),
            tailnet_role_index: 0,
            tailnet_host: TextEdit::new(""),
            tailnet_key: TextEdit::new(""),
            tailnet_port: TextEdit::new(TAILNET_DEFAULT_PORT.to_string()),
            tailnet_allowed: TextEdit::new(""),
            phase: SyncPhase::Idle,
            preview: false,
            credentials,
            cancel: CancelToken::new(),
            inbox: None,
            host: None,
        }
    }

    pub fn blur(&mut self) {
        for field in [
            &mut self.github_owner,
            &mut self.github_repo,
            &mut self.github_branch,
            &mut self.github_prefix,
            &mut self.drive_folder_id,
            &mut self.drive_folder_name,
            &mut self.github_client_id,
            &mut self.google_client_id,
            &mut self.tailnet_host,
            &mut self.tailnet_key,
            &mut self.tailnet_port,
            &mut self.tailnet_allowed,
        ] {
            field.focused = false;
        }
    }
}

impl State {
    pub fn sync_status(&self) -> &str {
        &self.sync_ui.status
    }

    pub fn set_sync_provider(&mut self, index: usize) {
        self.sync_ui.provider_index = index.min(2);
        self.sync_ui.status = match self.sync_ui.provider_index {
            0 => "GitHub selected. Use a dedicated repository and branch, not the Muninn source repository."
                .to_owned(),
            1 => "Google Drive selected. Sync stores an app-managed commit history in the chosen folder, not editable Markdown files."
                .to_owned(),
            _ => "Tailscale selected. One device hosts a commit store; the others join it by MagicDNS name over your tailnet."
                .to_owned(),
        };
    }

    pub fn set_tailnet_role(&mut self, index: usize) {
        self.sync_ui.tailnet_role_index = usize::from(index == 1);
        self.sync_ui.status = if self.sync_ui.tailnet_role_index == 0 {
            "This device hosts. Muninn serves the commit store on 127.0.0.1 while it is open; tailscale serve publishes it to your tailnet."
                .to_owned()
        } else {
            "Join a host. Enter the host's full MagicDNS name and the shared key shown on the host."
                .to_owned()
        };
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
                self.sync_ui.status = format!(
                    "Hosting on 127.0.0.1:{}. On this device run: {}",
                    host.addr().port(),
                    host.serve_command()
                );
                self.sync_ui.host = Some(host);
            }
            Err(error) => self.sync_ui.status = error,
        }
    }

    fn start_tailnet_host(&self, binding: &VaultSyncBinding) -> Result<TailnetHost, String> {
        let id = BindingId::parse(&binding.binding_id).map_err(|error| error.to_string())?;
        let key = muninn_sync::CredentialKey {
            provider: ProviderKind::Tailscale,
            binding: id.clone(),
        };
        let token = self
            .sync_ui
            .credentials
            .get(&key)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                "The Tailscale shared key is missing from the credential store. Disconnect and connect again."
                    .to_owned()
            })?;
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
        .map_err(|error| error.to_string())
    }

    pub fn connect_sync(&mut self) {
        if let Some(reason) = &self.settings.sync.disabled_reason {
            self.sync_ui.status = reason.clone();
            return;
        }
        let Ok(identity) = VaultIdentity::of_canonical(&self.vault.root) else {
            self.sync_ui.status =
                "This vault folder cannot be canonicalized, so it cannot be bound.".to_owned();
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
            self.sync_ui.status =
                "Enter the GitHub owner and repository before connecting.".to_owned();
            return;
        }
        if provider == "google_drive" && self.sync_ui.drive_folder_id.text.trim().is_empty() {
            self.sync_ui.status =
                "Enter the Drive folder id granted to this app. Pasting an id does not grant access."
                    .to_owned();
            return;
        }
        let client_missing = if provider == "github" {
            self.sync_ui.github_client_id.text.trim().is_empty()
        } else {
            self.sync_ui.google_client_id.text.trim().is_empty()
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
            drive_folder_id: self.sync_ui.drive_folder_id.text.trim().to_owned(),
            drive_folder_name: self.sync_ui.drive_folder_name.text.trim().to_owned(),
            tailnet_role: String::new(),
            tailnet_host: String::new(),
            tailnet_port: 0,
            tailnet_allowed: String::new(),
            last_success_unix_ms: None,
            last_head: String::new(),
        };
        self.settings.sync.github_client_id = self.sync_ui.github_client_id.text.trim().to_owned();
        self.settings.sync.google_client_id = self.sync_ui.google_client_id.text.trim().to_owned();
        self.settings
            .sync
            .vaults
            .retain(|known| known.identity != binding.identity);
        self.settings.sync.vaults.push(binding);
        self.save_settings();
        self.sync_ui.status = if client_missing {
            "Scope saved. Add the public OAuth client id before Connect can reach the provider. No request was sent."
                .to_owned()
        } else if provider == "github" {
            "Scope saved. GitHub device flow runs when a token is requested. No browser was opened and no token was stored."
                .to_owned()
        } else {
            "Scope saved. Drive sync uses an app-managed commit folder. No browser was opened and no token was stored."
                .to_owned()
        };
    }

    fn connect_tailnet(&mut self, identity: String) {
        let hosting = self.sync_ui.tailnet_role_index == 0;
        let host_name = self.sync_ui.tailnet_host.text.trim().to_ascii_lowercase();
        let port = if hosting {
            match self.sync_ui.tailnet_port.text.trim().parse::<u16>() {
                Ok(port) if port >= 1024 => port,
                _ => {
                    self.sync_ui.status =
                        "Enter a loopback port between 1024 and 65535 for the host.".to_owned();
                    return;
                }
            }
        } else {
            0
        };
        if !hosting {
            if let Err(error) = muninn_sync::validate_tailnet_host(&host_name) {
                self.sync_ui.status = error.to_string();
                return;
            }
            if self.sync_ui.tailnet_key.text.trim().len() < 32 {
                self.sync_ui.status =
                    "Paste the shared key shown on the host. It is stored in the OS credential store, not in settings."
                        .to_owned();
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
            self.sync_ui.status = format!("{error} Nothing was saved.");
            return;
        }
        self.sync_ui.tailnet_key.replace_text("");
        self.sync_ui.host = None;
        let binding = VaultSyncBinding {
            identity,
            provider: "tailscale".to_owned(),
            binding_id: binding_id.as_str().to_owned(),
            account: String::new(),
            github_owner: String::new(),
            github_repo: String::new(),
            github_branch: String::new(),
            github_prefix: String::new(),
            drive_folder_id: String::new(),
            drive_folder_name: String::new(),
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
        self.settings
            .sync
            .vaults
            .retain(|known| known.identity != binding.identity);
        self.settings.sync.vaults.push(binding);
        self.save_settings();
        if hosting {
            self.resume_tailnet_host();
            if self.sync_ui.host.is_some() {
                self.sync_ui.status = format!(
                    "Hosting. Run once on this device: {}. Shared key for your other devices (shown once): {}",
                    muninn_sync::serve_command(port),
                    key.expose()
                );
            }
        } else {
            self.sync_ui.status =
                "Joined. The shared key is in the OS credential store. Sync Now reaches the host over your tailnet."
                    .to_owned();
        }
    }

    pub fn disconnect_sync(&mut self) {
        self.sync_ui.cancel.cancel();
        self.sync_ui.host = None;
        let Some(binding) = self.current_binding().cloned() else {
            self.sync_ui.status = "Nothing is connected. Local notes were not changed.".to_owned();
            return;
        };
        if let Ok(id) = BindingId::parse(&binding.binding_id) {
            let provider = ProviderKind::parse(&binding.provider).unwrap_or(ProviderKind::GitHub);
            let key = muninn_sync::CredentialKey {
                provider,
                binding: id,
            };
            if let Err(error) = self.sync_ui.credentials.delete(&key) {
                self.sync_ui.status = error.to_string();
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
        self.sync_ui.status =
            "Disconnected. Credentials for this binding were removed. Local and remote notes were not deleted."
                .to_owned();
    }

    pub fn sync_now(&mut self) {
        if self.dirty() || self.conflict.is_some() {
            self.sync_ui.status =
                "Save or resolve the open note before Sync Now. Unsaved text is not uploaded or overwritten."
                    .to_owned();
            return;
        }
        if self.sync_ui.phase.is_running() {
            self.sync_ui.status = "A sync is already running.".to_owned();
            return;
        }
        let Some(binding) = self.current_binding().cloned() else {
            self.sync_ui.status = "Connect a provider for this vault before Sync Now.".to_owned();
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
        let github_client_id = self.settings.sync.github_client_id.clone();
        let google_client_id = self.settings.sync.google_client_id.clone();
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
                    google_client_id,
                    tailnet_api: None,
                },
                Err(error) => {
                    let _ = tx.send(SyncMessage::Finished(Err(error.to_string())));
                    return;
                }
            };
            let result =
                muninn_sync::run_sync(&engine, &request).map_err(|error| error.to_string());
            let _ = tx.send(SyncMessage::Finished(result));
        });
        self.sync_ui.status = "Sync started. The window stays responsive.".to_owned();
    }

    pub fn cancel_sync(&mut self) {
        self.sync_ui.cancel.cancel();
        self.sync_ui.status =
            "Cancelling. A change the provider already accepted is kept and reconciled.".to_owned();
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
            let SyncMessage::Finished(result) = message;
            self.sync_ui.phase = SyncPhase::Idle;
            self.finish_sync(result);
        }
        true
    }

    fn finish_sync(&mut self, result: Result<SyncReport, String>) {
        match result {
            Ok(SyncReport::Succeeded { head, exclusions }) => {
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
                    format!(
                        " {} path(s) excluded by the sync contract.",
                        exclusions.len()
                    )
                };
                self.sync_ui.status = format!("Last successful sync {head}.{extra}");
                if self.dirty() {
                    self.sync_ui.status.push_str(
                        " The open note changed while sync ran; its text was not overwritten.",
                    );
                } else {
                    let select = self.selected_path.clone();
                    self.reload(select);
                }
                self.refresh_watch();
            }
            Ok(SyncReport::Conflicts { conflicts }) => {
                self.sync_ui.status = format!(
                    "{} conflict(s). Nothing was overwritten. Resolve them, then Sync Now again.",
                    conflicts.len()
                );
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
fn run_request(
    binding: &VaultSyncBinding,
    root: std::path::PathBuf,
    sync_home: std::path::PathBuf,
) -> Result<RunRequest, String> {
    let binding_id = BindingId::parse(&binding.binding_id).map_err(|error| error.to_string())?;
    let state_dir = muninn_sync::binding_state_dir(&sync_home, &binding_id)
        .map_err(|error| error.to_string())?;
    let provider = ProviderKind::parse(&binding.provider).map_err(|error| error.to_string())?;
    let github = (provider == ProviderKind::GitHub).then(|| muninn_sync::GitHubScope {
        owner: binding.github_owner.clone(),
        repo: binding.github_repo.clone(),
        branch: binding.github_branch.clone(),
        prefix: binding.github_prefix.clone(),
    });
    let drive = (provider == ProviderKind::GoogleDrive).then(|| muninn_sync::DriveScope {
        folder_id: binding.drive_folder_id.clone(),
        folder_name: binding.drive_folder_name.clone(),
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
        resolutions: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use crate::State;
    use std::fs;

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

    #[test]
    fn connect_persists_scope_without_tokens_and_disconnect_keeps_the_note() {
        let root = std::env::temp_dir().join(format!(
            "muninn-sync-ui-{}-{}",
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
            state_home,
            config_home.clone(),
            crate::settings::Settings::default(),
        );
        state.sync_ui.github_owner.replace_text("octocat");
        state.sync_ui.github_repo.replace_text("notes");
        state.connect_sync();
        let saved = fs::read_to_string(config_home.join("muninn/settings.json")).unwrap();
        assert!(saved.contains("octocat"));
        assert!(!saved.contains("access_token"));
        assert!(!saved.contains("refresh_token"));
        state.disconnect_sync();
        assert_eq!(fs::read_to_string(root.join("Note.md")).unwrap(), "keep me");
        assert!(state.settings.sync.vaults.is_empty());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(root.with_extension("state"));
        let _ = fs::remove_dir_all(&config_home);
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
        assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_ok());
        let saved = fs::read_to_string(config_home.join("muninn/settings.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(json["sync"]["vaults"][0]["tailnet_role"], "host");
        let shown = state.sync_status().rsplit(' ').next().unwrap().to_owned();
        assert!(shown.len() >= 32);
        assert!(
            !saved.contains(&shown),
            "the shared key must not reach settings"
        );
        state.disconnect_sync();
        assert!(state.settings.sync.vaults.is_empty());
        assert_eq!(fs::read_to_string(root.join("Note.md")).unwrap(), "keep me");

        state.set_sync_provider(2);
        state.set_tailnet_role(1);
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
}
