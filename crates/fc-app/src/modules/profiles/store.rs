//! The profile registry (docs/02 §28): `profiles.json` in the app data folder, **outside**
//! every collection — it decides which collection opens.
//!
//! * The default profile's collection is `<root>/collection.db` (where every version before
//!   v1.0 kept it: upgrading keeps the data in place); every other profile has its own folder
//!   `<root>/profiles/<id>/` (database + `media/` + backups next to it).
//! * A missing file means "only the default profile". Writes go to a temporary file that is
//!   renamed over the old one, so a crash never leaves half a registry.

use crate::error::{AppError, AppResult};
use fc_api::{ApiError, ErrorCode};
use fc_core::ux::lock::LockKind;
use fc_core::ux::profile::{valid_id, DEFAULT_PROFILE_ID};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// File name of the registry inside the app data folder.
pub const REGISTRY_FILE: &str = "profiles.json";
/// Folder (inside the app data folder) holding the non-default profiles.
pub const PROFILES_DIR: &str = "profiles";
/// File name of a collection inside its folder (the desktop's `COLLECTION_FILE`).
pub const COLLECTION_FILE: &str = "collection.db";

/// The registry file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RegistryFile {
    /// Format version.
    pub version: u32,
    /// The profile to open.
    pub active: String,
    /// Profiles, default first, then by creation.
    pub profiles: Vec<ProfileEntry>,
}

/// One profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProfileEntry {
    pub id: String,
    /// `None` = the default profile's translated name.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub lock: Option<LockEntry>,
}

/// A profile's app lock settings (never the secret).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LockEntry {
    pub kind: LockKind,
    pub idle_minutes: u32,
    /// The Argon2id hash, **only** when the OS keychain does not keep secrets across
    /// restarts (then the PHC string is stored here; it is a slow salted hash, not the
    /// secret). Otherwise `None` and the hash is in the keychain.
    #[serde(default)]
    pub hash: Option<String>,
}

impl RegistryFile {
    fn fresh() -> Self {
        Self {
            version: 1,
            active: DEFAULT_PROFILE_ID.to_owned(),
            profiles: vec![ProfileEntry {
                id: DEFAULT_PROFILE_ID.to_owned(),
                name: None,
                created_at: 0,
                lock: None,
            }],
        }
    }

    pub fn get(&self, id: &str) -> Option<&ProfileEntry> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut ProfileEntry> {
        self.profiles.iter_mut().find(|p| p.id == id)
    }
}

/// Reads and writes the registry under one app data folder.
#[derive(Debug, Clone)]
pub struct ProfileStore {
    root: PathBuf,
}

pub(crate) fn io_error(path: &Path, e: impl std::fmt::Display) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::IoProfile, format!("{}: {e}", path.display()))
            .with_detail("path", path.display().to_string()),
    )
}

impl ProfileStore {
    /// Registry of the app data folder `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The app data folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn file(&self) -> PathBuf {
        self.root.join(REGISTRY_FILE)
    }

    /// Folder of a profile's collection.
    pub fn profile_dir(&self, id: &str) -> PathBuf {
        if id == DEFAULT_PROFILE_ID {
            self.root.clone()
        } else {
            self.root.join(PROFILES_DIR).join(id)
        }
    }

    /// Database file of a profile.
    pub fn collection_path(&self, id: &str) -> PathBuf {
        self.profile_dir(id).join(COLLECTION_FILE)
    }

    /// Loads the registry (missing → only the default profile). Entries with an id that could
    /// leave the profiles folder are dropped; an unknown active profile falls back to the
    /// default one.
    pub(crate) fn load(&self) -> AppResult<RegistryFile> {
        let path = self.file();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(RegistryFile::fresh()),
            Err(e) => return Err(io_error(&path, e)),
        };
        let mut file: RegistryFile =
            serde_json::from_str(&text).map_err(|e| io_error(&path, e))?;
        file.profiles.retain(|p| valid_id(&p.id));
        if file.get(DEFAULT_PROFILE_ID).is_none() {
            file.profiles.insert(0, RegistryFile::fresh().profiles.remove(0));
        }
        if file.get(&file.active).is_none() {
            file.active = DEFAULT_PROFILE_ID.to_owned();
        }
        Ok(file)
    }

    /// Writes the registry atomically (temporary file + rename).
    pub(crate) fn save(&self, file: &RegistryFile) -> AppResult<()> {
        let path = self.file();
        std::fs::create_dir_all(&self.root).map_err(|e| io_error(&self.root, e))?;
        let text = serde_json::to_string_pretty(file).map_err(|e| io_error(&path, e))?;
        let tmp = self.root.join(format!("{REGISTRY_FILE}.tmp"));
        std::fs::write(&tmp, text).map_err(|e| io_error(&tmp, e))?;
        std::fs::rename(&tmp, &path).map_err(|e| io_error(&path, e))
    }

    /// The active profile and its database file.
    pub fn active_collection(&self) -> AppResult<(String, PathBuf)> {
        let file = self.load()?;
        let path = self.collection_path(&file.active);
        Ok((file.active, path))
    }
}
