//! API keys: kept only in the OS keychain (CLAUDE.md rule 10, docs/06 §1.2).
//!
//! A [`Secret`] cannot be printed (`Display` is not implemented, `Debug` shows `Secret(***)`)
//! or serialized, so it cannot slip into the database, the oplog, an event, an error or a log
//! line by accident; reading it takes an explicit [`Secret::expose`].

use std::collections::HashMap;
use std::sync::Mutex;

/// An API key.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wraps a key.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The key itself — only for the request header that needs it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// Keychain failure. Messages never contain the secret.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    /// No keychain on this system (or none configured for this app instance).
    #[error("keychain unavailable: {0}")]
    Unavailable(String),
    /// The keychain refused or failed.
    #[error("keychain error: {0}")]
    Failed(String),
}

/// Stores API keys by account name (`ai.provider.<id>`).
pub trait SecretStore: Send + Sync {
    /// The key of `account`, if set.
    fn get(&self, account: &str) -> Result<Option<Secret>, SecretError>;
    /// Sets the key of `account`.
    fn set(&self, account: &str, secret: &Secret) -> Result<(), SecretError>;
    /// Removes the key of `account` (no error if absent).
    fn delete(&self, account: &str) -> Result<(), SecretError>;
}

/// Process memory only: tests, and the CLI/desktop fallback when there is no keychain.
#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<String, Secret>>);

impl MemorySecrets {
    /// Empty.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, Secret>>, SecretError> {
        self.0
            .lock()
            .map_err(|_| SecretError::Failed("poisoned".into()))
    }
}

impl SecretStore for MemorySecrets {
    fn get(&self, account: &str) -> Result<Option<Secret>, SecretError> {
        Ok(self.lock()?.get(account).cloned())
    }

    fn set(&self, account: &str, secret: &Secret) -> Result<(), SecretError> {
        self.lock()?.insert(account.to_owned(), secret.clone());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        self.lock()?.remove(account);
        Ok(())
    }
}

/// No keychain: nothing is stored, setting a key fails (the app's default until a transport
/// installs a real store).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSecrets;

impl SecretStore for NoSecrets {
    fn get(&self, _account: &str) -> Result<Option<Secret>, SecretError> {
        Ok(None)
    }

    fn set(&self, _account: &str, _secret: &Secret) -> Result<(), SecretError> {
        Err(SecretError::Unavailable("no keychain configured".into()))
    }

    fn delete(&self, _account: &str) -> Result<(), SecretError> {
        Ok(())
    }
}

/// The operating system's keychain: Windows Credential Manager, macOS Keychain, Linux kernel
/// keyring (`keyutils`; session-scoped — a persistent Secret Service backend can be enabled
/// later without changing callers).
#[cfg(feature = "keychain")]
#[derive(Debug, Clone)]
pub struct OsKeychain {
    service: String,
}

#[cfg(feature = "keychain")]
impl OsKeychain {
    /// Entries are stored under `service` (e.g. the app identifier).
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, account).map_err(keychain_error)
    }
}

#[cfg(feature = "keychain")]
fn keychain_error(e: keyring::Error) -> SecretError {
    // keyring errors describe the platform failure, never the password.
    match e {
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
            SecretError::Unavailable(e.to_string())
        }
        other => SecretError::Failed(other.to_string()),
    }
}

#[cfg(feature = "keychain")]
impl SecretStore for OsKeychain {
    fn get(&self, account: &str) -> Result<Option<Secret>, SecretError> {
        match self.entry(account)?.get_password() {
            Ok(p) => Ok(Some(Secret::new(p))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keychain_error(e)),
        }
    }

    fn set(&self, account: &str, secret: &Secret) -> Result<(), SecretError> {
        self.entry(account)?
            .set_password(secret.expose())
            .map_err(keychain_error)
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keychain_error(e)),
        }
    }
}
