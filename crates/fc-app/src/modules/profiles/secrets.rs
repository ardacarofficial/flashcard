//! Per-profile keychain accounts (v1.0). Every profile is a separate collection with its own
//! AI providers, so their API keys must not be shared: a profile's secrets live under its own
//! accounts in the one OS keychain service.
//!
//! **Migration.** The default profile keeps the account names every version before v1.0 used
//! (`ai.provider.<id>`): keys set before profiles existed keep working, and the CLI (which
//! opens the default collection) still finds them. Every other profile prefixes its accounts
//! with `profile.<profile id>.`. The lock hash has its own per-profile account
//! (`lock.profile.<id>`, see `session`) and does not go through this store.

use fc_ai::secrets::{Secret, SecretError, SecretStore};
use fc_core::ux::profile::DEFAULT_PROFILE_ID;
use std::sync::Arc;

/// The keychain account `account` (e.g. `ai.provider.<id>`) of profile `profile_id`.
pub fn scoped_account(profile_id: &str, account: &str) -> String {
    if profile_id == DEFAULT_PROFILE_ID {
        account.to_owned()
    } else {
        format!("profile.{profile_id}.{account}")
    }
}

/// A [`SecretStore`] that maps every account into one profile's namespace.
pub struct ProfileSecrets {
    inner: Arc<dyn SecretStore>,
    profile_id: String,
}

impl ProfileSecrets {
    /// The accounts of `profile_id` in `inner`.
    pub fn new(inner: Arc<dyn SecretStore>, profile_id: &str) -> Self {
        Self {
            inner,
            profile_id: profile_id.to_owned(),
        }
    }

    fn account(&self, account: &str) -> String {
        scoped_account(&self.profile_id, account)
    }
}

impl SecretStore for ProfileSecrets {
    fn get(&self, account: &str) -> Result<Option<Secret>, SecretError> {
        self.inner.get(&self.account(account))
    }

    fn set(&self, account: &str, secret: &Secret) -> Result<(), SecretError> {
        self.inner.set(&self.account(account), secret)
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        self.inner.delete(&self.account(account))
    }

    fn persistent(&self) -> bool {
        self.inner.persistent()
    }
}
