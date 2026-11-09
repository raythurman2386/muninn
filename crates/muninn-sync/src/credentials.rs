//! Credential storage. The memory store is the only one tests construct.
//! [`NativeCredentialStore`] calls the OS keychain and has no file fallback.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::CREDENTIAL_SERVICE;
use crate::error::SyncError;
use crate::model::{BindingId, ProviderKind};
use crate::secret::TokenSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialKey {
    pub provider: ProviderKind,
    pub binding: BindingId,
}

impl CredentialKey {
    #[must_use]
    pub fn account(&self) -> String {
        format!("{}:{}", self.provider.as_str(), self.binding.as_str())
    }
}

pub trait CredentialStore: Send + Sync {
    fn get(&self, key: &CredentialKey) -> Result<Option<TokenSet>, SyncError>;
    fn put(&self, key: &CredentialKey, token: &TokenSet) -> Result<(), SyncError>;
    fn delete(&self, key: &CredentialKey) -> Result<(), SyncError>;
}

impl<C: CredentialStore + ?Sized> CredentialStore for std::sync::Arc<C> {
    fn get(&self, key: &CredentialKey) -> Result<Option<crate::secret::TokenSet>, SyncError> {
        self.as_ref().get(key)
    }

    fn put(&self, key: &CredentialKey, token: &crate::secret::TokenSet) -> Result<(), SyncError> {
        self.as_ref().put(key, token)
    }

    fn delete(&self, key: &CredentialKey) -> Result<(), SyncError> {
        self.as_ref().delete(key)
    }
}

impl<C: CredentialStore + ?Sized> CredentialStore for &C {
    fn get(&self, key: &CredentialKey) -> Result<Option<TokenSet>, SyncError> {
        (*self).get(key)
    }

    fn put(&self, key: &CredentialKey, token: &TokenSet) -> Result<(), SyncError> {
        (*self).put(key, token)
    }

    fn delete(&self, key: &CredentialKey) -> Result<(), SyncError> {
        (*self).delete(key)
    }
}

/// Process-local store. Tokens never touch the disk.
#[derive(Default)]
pub struct MemoryCredentialStore {
    entries: Mutex<BTreeMap<String, String>>,
}

impl CredentialStore for MemoryCredentialStore {
    fn get(&self, key: &CredentialKey) -> Result<Option<TokenSet>, SyncError> {
        let entries = self.entries.lock().expect("credential lock");
        entries
            .get(&key.account())
            .map(|json| TokenSet::from_store_json(json))
            .transpose()
    }

    fn put(&self, key: &CredentialKey, token: &TokenSet) -> Result<(), SyncError> {
        if token.access.is_empty() {
            return Err(SyncError::Credentials {
                detail: "refusing to store an empty access token".to_owned(),
            });
        }
        self.entries
            .lock()
            .expect("credential lock")
            .insert(key.account(), token.to_store_json());
        Ok(())
    }

    fn delete(&self, key: &CredentialKey) -> Result<(), SyncError> {
        self.entries
            .lock()
            .expect("credential lock")
            .remove(&key.account());
        Ok(())
    }
}

/// OS keychain via the `keyring` crate's v1 backend. Constructing this value
/// does not talk to the OS. The first `get`/`put`/`delete` does, and any
/// failure is [`SyncError::Credentials`] — callers must not write the token
/// anywhere else.
pub struct NativeCredentialStore;

impl NativeCredentialStore {
    #[must_use]
    pub fn backend_name() -> &'static str {
        "keyring-v1"
    }
}

impl CredentialStore for NativeCredentialStore {
    fn get(&self, key: &CredentialKey) -> Result<Option<TokenSet>, SyncError> {
        let entry = native_entry(key)?;
        match entry.get_password() {
            Ok(json) => Ok(Some(TokenSet::from_store_json(&json)?)),
            Err(error) if is_missing(&error) => Ok(None),
            Err(error) => Err(native_error(error)),
        }
    }

    fn put(&self, key: &CredentialKey, token: &TokenSet) -> Result<(), SyncError> {
        if token.access.is_empty() {
            return Err(SyncError::Credentials {
                detail: "refusing to store an empty access token".to_owned(),
            });
        }
        let entry = native_entry(key)?;
        entry
            .set_password(&token.to_store_json())
            .map_err(native_error)
    }

    fn delete(&self, key: &CredentialKey) -> Result<(), SyncError> {
        let entry = native_entry(key)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(error) if is_missing(&error) => Ok(()),
            Err(error) => Err(native_error(error)),
        }
    }
}

fn native_entry(key: &CredentialKey) -> Result<keyring::Entry, SyncError> {
    keyring::Entry::new(CREDENTIAL_SERVICE, &key.account()).map_err(native_error)
}

fn native_error(error: keyring::Error) -> SyncError {
    SyncError::Credentials {
        detail: format!("os credential store refused the request ({error})"),
    }
}

fn is_missing(error: &keyring::Error) -> bool {
    matches!(error, keyring::Error::NoEntry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::Secret;

    #[test]
    fn memory_store_round_trips_and_debug_does_not_apply() {
        let store = MemoryCredentialStore::default();
        let key = CredentialKey {
            provider: ProviderKind::GitHub,
            binding: BindingId::parse("abcdef0123456789").unwrap(),
        };
        assert!(store.get(&key).unwrap().is_none());
        let token = TokenSet {
            access: Secret::new("access-secret"),
            refresh: Some(Secret::new("refresh-secret")),
            expires_unix_ms: Some(9),
            account: "octocat".to_owned(),
        };
        store.put(&key, &token).unwrap();
        let loaded = store.get(&key).unwrap().unwrap();
        assert_eq!(loaded.access.expose(), "access-secret");
        store.delete(&key).unwrap();
        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn native_backend_is_compiled_and_not_opened() {
        // Touching the type is enough to compile the OS backend. Calling
        // `get` here would initialize the user keychain, which tests must not do.
        assert_eq!(NativeCredentialStore::backend_name(), "keyring-v1");
        let _store = NativeCredentialStore;
    }
}
