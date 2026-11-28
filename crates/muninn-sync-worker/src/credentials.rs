//! The worker's credential store: one run-scoped PAT, memory only (#240
//! slice 8a).
//!
//! The page receives the personal access token from the person (slice 8b owns
//! that UI) and sends it in the run request. The worker keeps it in this store
//! for the duration of the run; the store is dropped when the run ends, and
//! [`muninn_sync::Secret`]'s `Drop` wipes each token's bytes.
//!
//! This is deliberately *not* `muninn_sync::MemoryCredentialStore`: that store
//! serialises a token to a JSON `String` so it can mirror the OS keyring's
//! shape, and a plain `String` is not zeroized on drop. A run-scoped store can
//! hold the typed [`muninn_sync::TokenSet`] directly instead. No browser
//! storage, OPFS directory or settings file ever sees the token here.
//!
//! Before 8b decides where a *persisted* token lives, this store is the only
//! holder on the worker side. It refuses a token for any other binding, so a
//! second vault cannot read the first run's credential by accident.

use std::sync::Mutex;

use muninn_sync::{
    BindingId, CredentialKey, CredentialStore, ProviderKind, Secret, SyncError, TokenSet,
};

/// A [`CredentialStore`] holding one PAT for one binding.
pub struct RunCredentials {
    key: CredentialKey,
    token: Mutex<Option<TokenSet>>,
}

impl RunCredentials {
    /// A store seeded with `token` for `binding` on GitHub.
    ///
    /// The engine asks for exactly this key before its first request; a
    /// different provider or binding gets `None` and the engine reports the
    /// ordinary "connect the provider first" auth error.
    #[must_use]
    pub fn new(binding: BindingId, token: Secret) -> RunCredentials {
        let key = CredentialKey {
            provider: ProviderKind::GitHub,
            binding,
        };
        let token = TokenSet {
            access: token,
            refresh: None,
            expires_unix_ms: None,
            account: String::new(),
        };
        RunCredentials {
            key,
            token: Mutex::new(Some(token)),
        }
    }

    /// Drop the token now instead of waiting for the store to fall out of
    /// scope. Idempotent.
    pub fn clear(&self) {
        *self
            .token
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

impl CredentialStore for RunCredentials {
    fn get(&self, key: &CredentialKey) -> Result<Option<TokenSet>, SyncError> {
        if key != &self.key {
            return Ok(None);
        }
        Ok(self
            .token
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone())
    }

    fn put(&self, key: &CredentialKey, token: &TokenSet) -> Result<(), SyncError> {
        if key != &self.key {
            return Err(SyncError::Credentials {
                detail: "this sync run holds a credential for a different binding".to_owned(),
            });
        }
        if token.access.is_empty() {
            return Err(SyncError::Credentials {
                detail: "refusing to store an empty access token".to_owned(),
            });
        }
        *self
            .token
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(token.clone());
        Ok(())
    }

    fn delete(&self, key: &CredentialKey) -> Result<(), SyncError> {
        if key == &self.key {
            self.clear();
        }
        Ok(())
    }
}

impl Drop for RunCredentials {
    fn drop(&mut self) {
        // Taking the option drops the `TokenSet`, whose `Secret`s zeroize their
        // bytes. `Mutex::get_mut` cannot fail here: `Drop` has `&mut self`.
        let _ = self
            .token
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }
}
