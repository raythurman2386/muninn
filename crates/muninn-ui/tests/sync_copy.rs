//! Sync status and error copy extraction guard (#158).
//!
//! Settings › Sync builds its status line in code, not at draw time. This
//! guard pins the two halves of the extraction: the catalog resolves to the
//! original English byte for byte, and the state transitions really read it
//! (`nothing connected`, `connected as`, `disconnected`, the Tailscale role
//! hints). Parameterised sentences are typed helpers, so their exact English
//! is asserted directly.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_i18n::{
    Key, browser_failed, connected_as, device_code, kanban_could_not_save_copy,
    kanban_quit_refused, last_sync, shared_key_copied, sign_in_failed, sync_authentication_needed,
    sync_conflict_copies, sync_conflicts, sync_credentials_unavailable, sync_dirty_buffer,
    sync_excluded, sync_file_error, sync_key_rotation_failed, sync_limit_exceeded,
    sync_listing_incomplete, sync_local_changed, sync_not_configured, sync_nothing_saved,
    sync_now_after_error, sync_oauth_scope_rejected, sync_offline, sync_outcome_unknown,
    sync_permission_denied, sync_previous_credential, sync_provider_error, sync_rate_limited_after,
    sync_remote_changed, sync_remote_scope_not_found, sync_unsupported, t, tailnet_hosting,
    tailnet_hosting_ready,
};
use muninn_ui::{Settings, State};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-sync-copy-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn state(tag: &str, settings: Settings) -> (State, PathBuf) {
    let root = unique_dir(tag);
    let state_home = unique_dir(&format!("{tag}-state"));
    let config_home = unique_dir(&format!("{tag}-config"));
    let state = State::with_settings(root.clone(), state_home, config_home, settings);
    (state, root)
}

/// Every static sentence moved out of `sync_ui.rs`, `kanban_sync.rs` and
/// `settings.rs`, pinned to the exact English it replaced.
#[test]
fn static_sync_copy_is_byte_identical() {
    for (key, english) in [
        (Key::SyncTailnetKeyLabel, "Shared key"),
        (Key::SyncTailnetKeyCopy, "Copy key"),
        (Key::SyncTailnetKeyShow, "Show key"),
        (Key::SyncTailnetKeyHide, "Hide key"),
        (Key::SyncTailnetKeyRotate, "Rotate key"),
        (
            Key::SyncStatusNotConnected,
            "Not connected. Choose a provider and an explicit remote, then Connect.",
        ),
        (
            Key::SyncStatusTailnetHost,
            "This device hosts. Muninn serves the commit store on 127.0.0.1 while it is open; tailscale serve publishes it to your tailnet.",
        ),
        (
            Key::SyncStatusTailnetJoin,
            "Join a host. Enter the host's full MagicDNS name and paste the key from Copy key on the host.",
        ),
        (
            Key::SyncStatusConnectedSharedKey,
            "Connected. The shared key is in the OS credential store. Sync Now when ready.",
        ),
        (
            Key::SyncStatusConnectedToken,
            "Connected. The token is in the OS credential store. Sync Now when ready.",
        ),
        (
            Key::SyncStatusAskingGithubCode,
            "Asking GitHub for a sign-in code…",
        ),
        (
            Key::SyncStatusOpeningGoogle,
            "Opening Google sign-in in your browser…",
        ),
        (
            Key::SyncStatusWaitingGoogle,
            "Finish signing in to Google in your browser. Muninn is waiting on 127.0.0.1 for the answer.",
        ),
        (
            Key::SyncStatusSignedInAfterDisconnect,
            "Sign-in finished after Disconnect; its token was removed.",
        ),
        (
            Key::SyncStatusTailnetJoined,
            "Joined. The shared key is in the OS credential store. Sync Now reaches the host over your tailnet.",
        ),
        (
            Key::SyncStatusSyncStarted,
            "Sync started. The window stays responsive.",
        ),
        (
            Key::SyncStatusCancelling,
            "Cancelling. A change the provider already accepted is kept and reconciled.",
        ),
        (
            Key::SyncStatusNoteChangedWhileSyncing,
            "The open note changed while sync ran; its text was not overwritten.",
        ),
        (
            Key::SyncStatusNothingConnected,
            "Nothing is connected. Local notes were not changed.",
        ),
        (
            Key::SyncStatusDisconnected,
            "Disconnected. Credentials for this binding were removed. Local and remote notes were not deleted.",
        ),
        (
            Key::SyncStatusSaveBeforeSync,
            "Save or resolve the open note before Sync Now. Unsaved text is not uploaded or overwritten.",
        ),
        (Key::SyncStatusAlreadyRunning, "A sync is already running."),
        (
            Key::SyncStatusFinishSignIn,
            "Finish signing in first, or Disconnect to cancel.",
        ),
        (
            Key::SyncStatusConnectProvider,
            "Connect a provider for this vault before Sync Now.",
        ),
        (
            Key::SyncStatusWaitForRun,
            "Wait for the running sync or sign-in to finish, or Disconnect to cancel it.",
        ),
        (
            Key::SyncStatusWaitToRotateKey,
            "Wait for the running sync to finish before rotating the key.",
        ),
        (
            Key::SyncStatusKeyRotated,
            "Key rotated; the old key no longer works. On each joined device, Disconnect and join again with the new key (Copy key).",
        ),
        (
            Key::SyncStatusCurrentConnectionKept,
            "The current connection was not changed.",
        ),
        (
            Key::SyncErrorVaultNotCanonical,
            "This vault folder cannot be canonicalized, so it cannot be bound.",
        ),
        (
            Key::SyncErrorGithubScope,
            "Enter the GitHub owner and repository before connecting.",
        ),
        (
            Key::SyncErrorLoopbackPort,
            "Enter a loopback port between 1024 and 65535 for the host.",
        ),
        (
            Key::SyncErrorPasteSharedKey,
            "Paste the shared key shown on the host. It is stored in the OS credential store, not in settings.",
        ),
        (
            Key::SyncErrorNotTailnetHost,
            "This vault does not host a Tailscale store.",
        ),
        (
            Key::SyncErrorSharedKeyMissing,
            "The shared key is missing from the credential store.",
        ),
        (
            Key::SyncErrorTailnetKeyMissing,
            "The Tailscale shared key is missing from the credential store. Disconnect and connect again.",
        ),
        (Key::SyncErrorCancelled, "Sync cancelled"),
        (
            Key::SyncErrorTimedOutAfterSend,
            "Timed out after a change was sent; reconcile before retrying",
        ),
        (
            Key::SyncErrorTimedOutBeforeResponse,
            "Timed out before the provider responded",
        ),
        (Key::SyncErrorRateLimited, "Rate limited"),
        (
            Key::SyncErrorLeaseBusy,
            "Another Muninn instance is writing this vault",
        ),
        (
            Key::SyncErrorSettingsNotObject,
            "Sync settings were not an object, so sync is off.",
        ),
        (
            Key::SyncErrorSettingsVersion,
            "Sync settings version is not supported, so sync is off.",
        ),
        (
            Key::SyncErrorSettingsVaultList,
            "Sync vault list was missing, so sync is off.",
        ),
        (
            Key::SyncErrorSettingsBindingInvalid,
            "A sync binding was invalid and was not loaded. Sync is off until it is replaced.",
        ),
        (
            Key::KanbanUiNotInsideVault,
            "the kanban is not inside the open vault",
        ),
        (
            Key::KanbanUiFieldRefused,
            "Not quitting: the kanban could not be saved. Fix what stopped the save, or undo the edits, then quit again.",
        ),
        (
            Key::ErrorSaveKanbanBeforeRestore,
            "Save or discard the current kanban before restoring another",
        ),
    ] {
        assert_eq!(t(key), english, "{key:?}");
    }
}

/// The parameterised sentences, with a representative argument each.
#[test]
fn sync_helpers_are_byte_identical() {
    assert_eq!(
        connected_as("octo"),
        "Connected as octo. The token is in the OS credential store. Sync Now when ready."
    );
    assert_eq!(last_sync("a1b2c3d"), "Last successful sync a1b2c3d.");
    assert_eq!(
        device_code("ABCD-1234", "https://github.com/login/device"),
        "Enter code ABCD-1234 at https://github.com/login/device. Your browser is opening it; Muninn finishes when GitHub approves."
    );
    assert_eq!(
        browser_failed("https://example.test"),
        "Could not open a browser. Open this address yourself to continue: https://example.test"
    );
    assert_eq!(
        sign_in_failed("access denied"),
        "Sign-in did not finish: access denied No token was stored."
    );
    assert_eq!(sync_excluded(2), "2 path(s) excluded by the sync contract.");
    assert_eq!(
        sync_conflict_copies("Plan (remote).md"),
        "A note changed on two devices; both versions were kept: Plan (remote).md."
    );
    assert_eq!(
        sync_conflicts(1, "Plan.md"),
        "1 conflict(s): Plan.md. Nothing was overwritten."
    );
    assert_eq!(
        tailnet_hosting(8080, "tailscale serve --bg 8080"),
        "Hosting on 127.0.0.1:8080. On this device run: tailscale serve --bg 8080"
    );
    assert_eq!(
        tailnet_hosting_ready("tailscale serve --bg 8080"),
        "Hosting. Run once on this device: tailscale serve --bg 8080. Use Copy key to give the shared key to your other devices."
    );
    assert_eq!(
        shared_key_copied(60),
        "Shared key copied. The clipboard clears in 60 seconds; paste it on the joining device."
    );
    assert_eq!(
        sync_previous_credential("permission denied"),
        "The previous connection's credential could not be removed: permission denied"
    );
    assert_eq!(
        sync_nothing_saved("the keyring is locked"),
        "the keyring is locked Nothing was saved."
    );
    assert_eq!(
        sync_key_rotation_failed("the keyring is locked"),
        "the keyring is locked The old key still works."
    );
    assert_eq!(
        sync_now_after_error("access denied"),
        "access denied Connect again before Sync Now."
    );
    assert_eq!(
        kanban_quit_refused("disk full"),
        "Kanban not saved and no recovery copy could be written: disk full. Use Save copy or fix the folder, then quit again."
    );
    assert_eq!(
        kanban_could_not_save_copy("disk full"),
        "could not save a copy: disk full"
    );
}

/// The run-failure sentences keep the provider's `detail` as written and only
/// the surrounding copy is catalogued.
#[test]
fn sync_error_helpers_are_byte_identical() {
    assert_eq!(
        sync_offline("connection refused"),
        "Offline: connection refused"
    );
    assert_eq!(
        sync_rate_limited_after(1500),
        "Rate limited; retry after 1500 ms"
    );
    assert_eq!(
        sync_provider_error(500, "internal error"),
        "Provider error 500: internal error"
    );
    assert_eq!(
        sync_authentication_needed("token expired"),
        "Authentication needed: token expired"
    );
    assert_eq!(
        sync_permission_denied("no access to the repository"),
        "Permission denied: no access to the repository"
    );
    assert_eq!(
        sync_oauth_scope_rejected("drive.file required"),
        "OAuth scope rejected: drive.file required"
    );
    assert_eq!(
        sync_remote_scope_not_found("branch main"),
        "Remote scope not found: branch main"
    );
    assert_eq!(
        sync_remote_changed("commit moved"),
        "Remote changed: commit moved"
    );
    assert_eq!(
        sync_outcome_unknown("connection dropped"),
        "Outcome unknown, reconcile before retrying: connection dropped"
    );
    assert_eq!(
        sync_listing_incomplete("page 3 of 5"),
        "Remote listing incomplete; refusing to treat that as deletion: page 3 of 5"
    );
    assert_eq!(
        sync_unsupported("Data.xlsx", "Workspace document"),
        "Unsupported Data.xlsx: Workspace document"
    );
    assert_eq!(
        sync_limit_exceeded("file larger than 100 MB"),
        "Sync limit exceeded: file larger than 100 MB"
    );
    assert_eq!(
        sync_credentials_unavailable("the keyring is locked"),
        "Credentials unavailable: the keyring is locked"
    );
    assert_eq!(
        sync_dirty_buffer("Plan.md"),
        "Save or discard unsaved edits in Plan.md before sync can change it"
    );
    assert_eq!(
        sync_local_changed("Plan.md"),
        "Plan.md changed during sync; run Sync Now again to replan"
    );
    assert_eq!(
        sync_not_configured("missing repository"),
        "Sync is not configured: missing repository"
    );
    assert_eq!(
        sync_file_error("read-only folder"),
        "File error: read-only folder"
    );
}

/// A fresh form, a saved GitHub binding and a disconnect all draw the
/// catalog's status line.
#[test]
fn sync_status_lines_read_the_catalog() {
    let root = unique_dir("status");
    let config = unique_dir("status-config");
    let state_home = unique_dir("status-state");
    let identity = muninn_sync::VaultIdentity::of_canonical(&root).expect("canonical vault");
    let saved = format!(
        r#"{{"sync":{{"version":1,"vaults":[{{"identity":"{}","provider":"github","binding_id":"{}","account":"octo"}}]}}}}"#,
        identity.as_str(),
        muninn_sync::BindingId::fresh().as_str()
    );
    fs::create_dir_all(config.join("muninn")).expect("config dir");
    fs::write(config.join("muninn/settings.json"), saved).expect("settings file");
    let settings = Settings::load_from(&config);
    let mut state = State::with_settings(root, state_home, config, settings);

    assert_eq!(state.sync_status(), connected_as("octo"));
    assert_eq!(
        state.sync_status(),
        "Connected as octo. The token is in the OS credential store. Sync Now when ready."
    );

    state.set_tailnet_role(0);
    assert_eq!(state.sync_status(), t(Key::SyncStatusTailnetHost));
    state.set_tailnet_role(1);
    assert_eq!(state.sync_status(), t(Key::SyncStatusTailnetJoin));

    state.reflect_sync_binding();
    state.disconnect_sync();
    assert_eq!(state.sync_status(), t(Key::SyncStatusDisconnected));
    assert_eq!(
        state.sync_status(),
        "Disconnected. Credentials for this binding were removed. Local and remote notes were not deleted."
    );

    state.disconnect_sync();
    assert_eq!(state.sync_status(), t(Key::SyncStatusNothingConnected));
}

/// A vault with no binding starts on the catalog's not-connected line.
#[test]
fn a_fresh_vault_shows_the_not_connected_line() {
    let (state, _root) = state("fresh", Settings::default());
    assert_eq!(state.sync_status(), t(Key::SyncStatusNotConnected));
    assert_eq!(
        state.sync_status(),
        "Not connected. Choose a provider and an explicit remote, then Connect."
    );
}

/// A stored sync section that cannot be trusted disables sync with the
/// catalog's reason, which the status line then shows verbatim.
#[test]
fn unusable_sync_settings_use_the_catalog_reason() {
    let config = unique_dir("bad-settings");
    fs::create_dir_all(config.join("muninn")).expect("config dir");
    fs::write(
        config.join("muninn/settings.json"),
        r#"{"sync":{"version":2}}"#,
    )
    .expect("settings file");
    let settings = Settings::load_from(&config);
    assert_eq!(
        settings.sync.disabled_reason.as_deref(),
        Some(t(Key::SyncErrorSettingsVersion))
    );
    assert_eq!(
        settings.sync.disabled_reason.as_deref(),
        Some("Sync settings version is not supported, so sync is off.")
    );
    assert!(settings.sync.vaults.is_empty());
}
