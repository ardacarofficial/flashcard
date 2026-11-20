//! Per-profile keychain accounts (v1.0). Every profile is a separate collection with its own
//! AI providers, so their API keys must not be shared: a profile's secrets live under its own
//! accounts in the one OS keychain service.
//!
//! **Migration.** The default profile keeps the account names every version before v1.0 used
//! (`ai.provider.<id>`): keys set before profiles existed keep working. Every other profile
//! prefixes its accounts with `profile.<profile id>.`. The CLI has no session: it takes the
//! accounts of the profile whose collection `--db` names ([`collection_secrets`]). The lock
//! hash has its own per-profile account (`lock.profile.<id>`, see `session`) and does not go
//! through this store. Deleting a profile removes its accounts ([`delete_profile_accounts`]).

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

/// The profile whose collection is the database file `db` (integration round 1: the CLI, which
/// has no session, opens any profile's collection with `--db`). A file at
/// `<root>/profiles/<id>/collection.db` whose `<root>/profiles.json` lists `<id>` belongs to
/// that profile; anything else (the default `<root>/collection.db`, a file elsewhere) to the
/// default profile, whose accounts are the pre-v1.0 names.
pub fn profile_of_collection(db: &std::path::Path) -> String {
    use super::store::{ProfileStore, COLLECTION_FILE, PROFILES_DIR};
    let db = std::fs::canonicalize(db).unwrap_or_else(|_| db.to_path_buf());
    let found = (|| {
        if db.file_name()? != COLLECTION_FILE {
            return None;
        }
        let dir = db.parent()?;
        let id = dir.file_name()?.to_str()?;
        let profiles = dir.parent()?;
        if profiles.file_name()? != PROFILES_DIR {
            return None;
        }
        let store = ProfileStore::new(profiles.parent()?);
        let file = store.load().ok()?;
        file.get(id).map(|p| p.id.clone())
    })();
    found.unwrap_or_else(|| DEFAULT_PROFILE_ID.to_owned())
}

/// The keychain accounts of the profile owning collection `db` ([`profile_of_collection`]).
pub fn collection_secrets(inner: Arc<dyn SecretStore>, db: &std::path::Path) -> Arc<dyn SecretStore> {
    Arc::new(ProfileSecrets::new(inner, &profile_of_collection(db)))
}

/// The AI providers of collection `db` (deleted ones included: their keys may still be in
/// the keychain). The file is about to be deleted with its profile, so an older schema may
/// be migrated to read it.
fn provider_accounts(db: &std::path::Path) -> fc_storage::StorageResult<Vec<String>> {
    use fc_storage::Store;
    let store = match fc_storage::SqliteStore::open_current(db) {
        Err(fc_storage::StorageError::NeedsMigration { .. }) => fc_storage::SqliteStore::open(db)?,
        other => other?,
    };
    let tx = store.read()?;
    Ok(tx
        .list_ai_providers(true)?
        .iter()
        .map(|p| crate::modules::ai::key_account(p.id))
        .collect())
}

/// Removes every keychain account of profile `profile_id` (whose collection is `db`): its AI
/// provider keys and its lock hash. Called when the profile is deleted, before its folder
/// goes. Best effort: an account that is already gone is fine; a collection that cannot be
/// read (newer than this build, damaged) leaves its AI keys, which is logged. Local HTTP API
/// tokens are kept as hashes inside the collection and go with it.
pub(crate) fn delete_profile_accounts(
    secrets: &Arc<dyn SecretStore>,
    profile_id: &str,
    db: &std::path::Path,
) {
    let _ = secrets.delete(&super::session::secret_account(profile_id));
    if !db.is_file() {
        return;
    }
    match provider_accounts(db) {
        Ok(accounts) => {
            let scoped = ProfileSecrets::new(secrets.clone(), profile_id);
            for account in accounts {
                let _ = scoped.delete(&account);
            }
        }
        Err(e) => {
            log::warn!(target: "fc_app::profiles", "profile {profile_id}: AI keys not removed, collection unreadable: {e}");
        }
    }
}
