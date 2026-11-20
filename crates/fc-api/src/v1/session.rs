//! Profiles and app lock (v1.0, docs/02 §28–§29).
//!
//! A **profile** is a separate collection (own database, media, settings, plugin
//! permissions). The registry of profiles lives outside every collection, next to them in the
//! app data folder; switching restarts the app on the other collection.
//!
//! The **app lock** is an access lock for the desktop window (not database encryption, docs/08
//! K4): a PIN or password per profile, hashed with Argon2id, the hash kept in the OS keychain
//! where it persists. While locked, the desktop bridge refuses every method except
//! `lock.status`, `lock.unlock`, `profiles.list` and `profiles.switch`
//! (`PERMISSION_APP_LOCKED`). Idle for the chosen time → locked.
//!
//! None of these are collection writes: no oplog entry, no undo (a lock change must not be
//! undoable). Only the user (desktop/CLI) may call them.

use fc_core::ux::lock::LockKind;
use fc_core::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProfileDto {
    /// Id (`default` for the first profile).
    pub id: String,
    /// Name (`null` = the default profile's translated name).
    pub name: Option<String>,
    /// The collection open in this app.
    pub active: bool,
    /// Protected by an app lock.
    pub locked: bool,
    /// When it was created (`0` for a default profile from before v1.0).
    pub created_at: Timestamp,
}

/// Response of `profiles.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProfileList {
    /// Profiles are available in this process (the desktop app; not the CLI).
    pub available: bool,
    /// Profiles by creation (the default first).
    pub profiles: Vec<ProfileDto>,
    /// The profile in use.
    pub active: Option<String>,
}

/// `profiles.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreateProfile {
    /// Name (unique, case-insensitive).
    pub name: String,
}

/// `profiles.rename`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RenameProfile {
    /// Profile.
    pub id: String,
    /// New name.
    pub name: String,
}

/// `profiles.delete`: removes a profile **and its collection folder** (not undoable; the UI
/// confirms). Not the active one. A locked profile needs its PIN/password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct DeleteProfile {
    /// Profile.
    pub id: String,
    /// The profile's PIN/password, when it has a lock.
    #[serde(default)]
    pub secret: Option<String>,
}

/// `profiles.switch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SwitchProfile {
    /// Profile to open.
    pub id: String,
}

/// Response of `profiles.switch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProfileSwitched {
    /// The profile that opens next.
    pub profile: ProfileDto,
    /// The app restarts to open it (`false` when it was already open).
    pub restart_required: bool,
}

/// Response of every `lock.*` method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LockState {
    /// The app lock exists in this process (the desktop app).
    pub available: bool,
    /// A PIN/password is set for the active profile.
    pub enabled: bool,
    /// Its kind.
    pub kind: Option<LockKind>,
    /// The app is locked now.
    pub locked: bool,
    /// Idle minutes before locking (`0` = never).
    pub idle_minutes: u32,
    /// Choices for `idle_minutes`.
    pub idle_choices: Vec<u32>,
    /// Wrong attempts in a row.
    pub failures: u32,
    /// Milliseconds to wait before the next attempt (`0` = now).
    pub retry_after_ms: i64,
    /// The hash is in the OS keychain (`false`: the keychain keeps nothing across restarts
    /// here, so it is in the profile registry file instead).
    pub secure_storage: bool,
    /// Active profile id.
    pub profile_id: Option<String>,
    /// Active profile name (`null` = default profile).
    pub profile_name: Option<String>,
}

/// `lock.set`: sets or changes the PIN/password. Changing an existing one needs `current`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct SetLock {
    /// PIN or password.
    pub kind: LockKind,
    /// The new secret.
    pub secret: String,
    /// The current secret (required when a lock is set).
    #[serde(default)]
    pub current: Option<String>,
    /// Idle minutes (`null` = keep / default).
    #[serde(default)]
    pub idle_minutes: Option<u32>,
}

/// `lock.configure`: changes the idle timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ConfigureLock {
    /// Idle minutes (one of `idle_choices`).
    pub idle_minutes: u32,
}

/// `lock.clear`: removes the lock (needs the current secret).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClearLock {
    /// The current secret.
    pub current: String,
}

/// `lock.unlock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Unlock {
    /// PIN or password.
    pub secret: String,
}
