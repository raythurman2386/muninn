//! Settings-owned sync session.
//!
//! Sync Now is manual. Network work runs on a worker thread. `applying` stays
//! set for that whole run so autosave and the metadata watcher do not write or
//! reload across the batch. Disconnect removes the binding and its credential
//! only.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use muninn_sync::{
    BindingId, CancelToken, CredentialStore, MemoryCredentialStore, NativeCredentialStore,
    OperationId, ProviderKind, RunRequest, SyncReport, VaultIdentity,
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
    pub applying: bool,
    pub running: bool,
    /// Headless scenes show only the sync section so the controls fit.
    pub preview: bool,
    credentials: Arc<dyn CredentialStore>,
    cancel: CancelToken,
    inbox: Option<Receiver<SyncMessage>>,
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
            applying: false,
            running: false,
            preview: false,
            credentials,
            cancel: CancelToken::new(),
            inbox: None,
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
        self.sync_ui.provider_index = usize::from(index == 1);
        self.sync_ui.status = if self.sync_ui.provider_index == 0 {
            "GitHub selected. Use a dedicated repository and branch, not the Muninn source repository."
                .to_owned()
        } else {
            "Google Drive selected. Sync stores an app-managed commit history in the chosen folder, not editable Markdown files."
                .to_owned()
        };
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
        let provider = if self.sync_ui.provider_index == 0 {
            "github"
        } else {
            "google_drive"
        };
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

    pub fn disconnect_sync(&mut self) {
        self.sync_ui.cancel.cancel();
        let Some(binding) = self.current_binding().cloned() else {
            self.sync_ui.status = "Nothing is connected. Local notes were not changed.".to_owned();
            return;
        };
        if let Ok(id) = BindingId::parse(&binding.binding_id) {
            let provider = if binding.provider == "google_drive" {
                ProviderKind::GoogleDrive
            } else {
                ProviderKind::GitHub
            };
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
        self.sync_ui.running = false;
        self.sync_ui.applying = false;
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
        if self.sync_ui.running {
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
        self.sync_ui.running = true;
        self.sync_ui.applying = true;
        self.sync_ui.cancel = CancelToken::new();
        let cancel = self.sync_ui.cancel.clone();
        let credentials = Arc::clone(&self.sync_ui.credentials);
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
            self.sync_ui.running = false;
            self.sync_ui.applying = false;
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

fn run_request(
    binding: &VaultSyncBinding,
    root: std::path::PathBuf,
    state_dir: std::path::PathBuf,
) -> Result<RunRequest, String> {
    let binding_id = BindingId::parse(&binding.binding_id).map_err(|error| error.to_string())?;
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
    Ok(RunRequest {
        root,
        state_dir,
        binding: binding_id,
        provider,
        github,
        drive,
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
}
