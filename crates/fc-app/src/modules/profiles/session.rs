//! The running app's session: which profile it opened and whether it is unlocked
//! (docs/02 §28–§29). One per desktop process, installed into the [`App`] by the shell
//! (`App::install_session`); the CLI and the MCP server have none.
//!
//! The lock secret is never stored: only its Argon2id hash (PHC string, random salt), in the
//! OS keychain (`fc_ai::secrets`, account `lock.profile.<id>`) when the keychain keeps secrets
//! across restarts, else in the profile registry. Wrong attempts back off
//! ([`fc_core::ux::lock::retry_delay_ms`]).

use super::store::{io_error, LockEntry, ProfileEntry, ProfileStore, RegistryFile};
use crate::error::{AppError, AppResult};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use fc_ai::secrets::{Secret, SecretStore};
use fc_api::v1::session::{LockState, ProfileDto};
use fc_api::{ApiError, ErrorCode};
use fc_core::ux::lock::{
    idle_expired, retry_delay_ms, valid_idle_minutes, validate_secret, LockKind,
    DEFAULT_IDLE_MINUTES, IDLE_CHOICES,
};
use fc_core::ux::profile::{normalize_name, same_name, DEFAULT_PROFILE_ID, MAX_PROFILES};
use fc_core::{Clock, Timestamp};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// Keychain account of a profile's lock hash.
pub fn secret_account(profile_id: &str) -> String {
    format!("lock.profile.{profile_id}")
}

#[derive(Debug, Default)]
struct Runtime {
    locked: bool,
    /// Idle minutes of the active lock (`None` = no lock): cached so the per-call gate does
    /// no file or keychain I/O.
    idle: Option<u32>,
    last_activity: i64,
    failures: u32,
    retry_at: i64,
}

/// See the module docs.
pub struct Session {
    store: ProfileStore,
    profile_id: String,
    secrets: Arc<dyn SecretStore>,
    clock: Arc<dyn Clock>,
    runtime: Mutex<Runtime>,
    /// Another profile was chosen (`profiles.switch`): this session's collection is on its
    /// way out (the shell restarts on the new one). Background work bound to it stops.
    retired: AtomicBool,
    /// Serialises registry read-modify-write.
    io: Mutex<()>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("root", &self.store.root())
            .field("profile_id", &self.profile_id)
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn storage_error(e: impl std::fmt::Display) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ProviderLockStorage, e.to_string())
            .with_detail("reason", e.to_string()),
    )
}

fn not_found(id: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::NotFoundProfile, format!("profile {id} not found"))
            .with_detail("id", id),
    )
}

fn lock_state(reason: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ConflictLockState, format!("app lock: {reason}"))
            .with_detail("reason", reason),
    )
}

/// Argon2id hash (PHC string) of `secret` with a fresh random salt.
pub(crate) fn hash_secret(secret: &str) -> AppResult<String> {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(storage_error)?;
    let salt = SaltString::encode_b64(&salt).map_err(storage_error)?;
    Argon2::default()
        .hash_password(secret.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(storage_error)
}

/// Whether `secret` matches the PHC `hash` (constant-time comparison inside argon2).
pub(crate) fn verify_secret(secret: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .and_then(|h| Argon2::default().verify_password(secret.as_bytes(), &h))
        .is_ok()
}

impl Session {
    /// Opens the session of the active profile in app data folder `root`: the app starts
    /// **locked** when that profile has a lock.
    pub fn open(
        store: ProfileStore,
        secrets: Arc<dyn SecretStore>,
        clock: Arc<dyn Clock>,
    ) -> AppResult<Self> {
        let file = store.load()?;
        let s = Self {
            store,
            profile_id: file.active.clone(),
            secrets,
            clock,
            runtime: Mutex::new(Runtime::default()),
            retired: AtomicBool::new(false),
            io: Mutex::new(()),
        };
        let idle = s.effective_lock(&file)?.map(|(l, _)| l.idle_minutes);
        let now = s.now();
        {
            let mut rt = lock(&s.runtime);
            rt.locked = idle.is_some();
            rt.idle = idle;
            rt.last_activity = now;
        }
        Ok(s)
    }

    fn now(&self) -> i64 {
        self.clock.now().as_millis()
    }

    /// The profile this process opened.
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    /// Its database file.
    pub fn collection_path(&self) -> std::path::PathBuf {
        self.store.collection_path(&self.profile_id)
    }

    /// The registry.
    pub fn store(&self) -> &ProfileStore {
        &self.store
    }

    /// Whether the app is locked now.
    pub fn is_locked(&self) -> bool {
        lock(&self.runtime).locked
    }

    /// Whether another profile was chosen since this session opened (see `retired`).
    pub fn is_retired(&self) -> bool {
        self.retired.load(Ordering::SeqCst)
    }

    /// Whether work nobody at the keyboard asked for (the local HTTP API, scheduled
    /// automation rules, plugins) must stop: the app is locked, or its profile is being
    /// switched away from.
    pub fn blocks_background(&self) -> bool {
        self.is_locked() || self.is_retired()
    }

    /// The lock of `entry` with its hash, if it has a usable one. A lock whose hash is gone
    /// (keychain entry removed by hand) does not lock: there is nothing to check against, and
    /// whoever can remove keychain entries is the OS user anyway (docs/08 K4).
    fn lock_with_hash(&self, entry: &ProfileEntry) -> AppResult<Option<(LockEntry, String)>> {
        let Some(l) = &entry.lock else {
            return Ok(None);
        };
        if let Some(h) = &l.hash {
            return Ok(Some((l.clone(), h.clone())));
        }
        let stored = self
            .secrets
            .get(&secret_account(&entry.id))
            .map_err(storage_error)?;
        Ok(stored.map(|s| (l.clone(), s.expose().to_owned())))
    }

    fn effective_lock(&self, file: &RegistryFile) -> AppResult<Option<(LockEntry, String)>> {
        match file.get(&self.profile_id) {
            Some(p) => self.lock_with_hash(p),
            None => Ok(None),
        }
    }

    fn current_entry(file: &RegistryFile, id: &str) -> AppResult<ProfileEntry> {
        file.get(id).cloned().ok_or_else(|| not_found(id))
    }

    // ------------------------------------------------------------------ profiles

    pub(crate) fn dto(&self, p: &ProfileEntry) -> ProfileDto {
        ProfileDto {
            id: p.id.clone(),
            name: p.name.clone(),
            active: p.id == self.profile_id,
            locked: p.lock.is_some(),
            created_at: Timestamp(p.created_at),
        }
    }

    /// Profiles and the active one.
    pub(crate) fn list(&self) -> AppResult<(Vec<ProfileDto>, String)> {
        let file = self.store.load()?;
        Ok((file.profiles.iter().map(|p| self.dto(p)).collect(), self.profile_id.clone()))
    }

    fn check_name(file: &RegistryFile, raw: &str, except: Option<&str>) -> AppResult<String> {
        let name = normalize_name(raw).map_err(|e| {
            AppError::Api(
                ApiError::new(ErrorCode::ValidationProfileName, format!("bad profile name: {}", e.code()))
                    .with_detail("reason", e.code()),
            )
        })?;
        let clash = file.profiles.iter().any(|p| {
            Some(p.id.as_str()) != except && p.name.as_deref().is_some_and(|n| same_name(n, &name))
        });
        if clash {
            return Err(AppError::Api(
                ApiError::new(ErrorCode::ConflictProfileName, format!("profile {name} exists"))
                    .with_detail("name", name),
            ));
        }
        Ok(name)
    }

    /// Creates a profile and its (empty) folder; the database is created when it opens.
    pub(crate) fn create(&self, raw_name: &str) -> AppResult<ProfileDto> {
        let _io = lock(&self.io);
        let mut file = self.store.load()?;
        let name = Self::check_name(&file, raw_name, None)?;
        if file.profiles.len() >= MAX_PROFILES {
            return Err(AppError::Api(
                ApiError::new(ErrorCode::ConflictProfileLimit, "too many profiles")
                    .with_detail("max", MAX_PROFILES),
            ));
        }
        let mut raw = [0u8; 16];
        getrandom::fill(&mut raw).map_err(|e| io_error(self.store.root(), e))?;
        let id = uuid::Uuid::from_bytes(raw).hyphenated().to_string();
        let dir = self.store.profile_dir(&id);
        std::fs::create_dir_all(&dir).map_err(|e| io_error(&dir, e))?;
        let entry = ProfileEntry {
            id,
            name: Some(name),
            created_at: self.now(),
            lock: None,
        };
        file.profiles.push(entry.clone());
        self.store.save(&file)?;
        Ok(self.dto(&entry))
    }

    pub(crate) fn rename(&self, id: &str, raw_name: &str) -> AppResult<ProfileDto> {
        let _io = lock(&self.io);
        let mut file = self.store.load()?;
        let name = Self::check_name(&file, raw_name, Some(id))?;
        let entry = file.get_mut(id).ok_or_else(|| not_found(id))?;
        entry.name = Some(name);
        let entry = entry.clone();
        self.store.save(&file)?;
        Ok(self.dto(&entry))
    }

    /// Deletes a profile with its folder. Not the active one, not the default one (its
    /// collection shares the app data folder). A locked profile needs its secret.
    pub(crate) fn delete(&self, id: &str, secret: Option<&str>) -> AppResult<()> {
        let _io = lock(&self.io);
        let mut file = self.store.load()?;
        let entry = Self::current_entry(&file, id)?;
        if id == self.profile_id || id == DEFAULT_PROFILE_ID {
            let reason = if id == self.profile_id { "active" } else { "default" };
            return Err(AppError::Api(
                ApiError::new(ErrorCode::ConflictProfileActive, format!("cannot delete {id}"))
                    .with_detail("id", id)
                    .with_detail("reason", reason),
            ));
        }
        if let Some((_, hash)) = self.lock_with_hash(&entry)? {
            self.attempt(secret.unwrap_or_default(), &hash)?;
        }
        file.profiles.retain(|p| p.id != id);
        self.store.save(&file)?;
        let _ = self.secrets.delete(&secret_account(id));
        let dir = self.store.profile_dir(id);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_error(&dir, e)),
        }
    }

    /// Makes `id` the profile to open; `true` when that needs a restart.
    pub(crate) fn switch(&self, id: &str) -> AppResult<(ProfileDto, bool)> {
        let _io = lock(&self.io);
        let mut file = self.store.load()?;
        let entry = Self::current_entry(&file, id)?;
        file.active = id.to_owned();
        self.store.save(&file)?;
        let away = id != self.profile_id;
        if away {
            self.retired.store(true, Ordering::SeqCst);
        } else {
            // Switching back before the restart: this collection stays.
            self.retired.store(false, Ordering::SeqCst);
        }
        Ok((self.dto(&entry), away))
    }

    // ------------------------------------------------------------------ lock

    /// The lock state of the active profile.
    pub(crate) fn status(&self) -> AppResult<LockState> {
        let file = self.store.load()?;
        let entry = Self::current_entry(&file, &self.profile_id)?;
        let effective = self.lock_with_hash(&entry)?;
        let now = self.now();
        let rt = lock(&self.runtime);
        Ok(LockState {
            available: true,
            enabled: effective.is_some(),
            kind: effective.as_ref().map(|(l, _)| l.kind),
            locked: rt.locked,
            idle_minutes: effective
                .as_ref()
                .map_or(DEFAULT_IDLE_MINUTES, |(l, _)| l.idle_minutes),
            idle_choices: IDLE_CHOICES.to_vec(),
            failures: rt.failures,
            retry_after_ms: (rt.retry_at - now).max(0),
            secure_storage: self.secrets.persistent(),
            profile_id: Some(entry.id.clone()),
            profile_name: entry.name.clone(),
        })
    }

    /// One unlock attempt against `hash`, with back-off.
    fn attempt(&self, secret: &str, hash: &str) -> AppResult<()> {
        let now = self.now();
        {
            let rt = lock(&self.runtime);
            if now < rt.retry_at {
                return Err(AppError::Api(
                    ApiError::new(ErrorCode::ConflictLockRetry, "too many wrong attempts")
                        .with_detail("retry_after_ms", rt.retry_at - now)
                        .retryable(true),
                ));
            }
        }
        // Slow on purpose (Argon2id); not under the runtime lock.
        let ok = verify_secret(secret, hash);
        let mut rt = lock(&self.runtime);
        if ok {
            rt.failures = 0;
            rt.retry_at = 0;
            return Ok(());
        }
        rt.failures = rt.failures.saturating_add(1);
        let delay = retry_delay_ms(rt.failures);
        rt.retry_at = now + delay;
        Err(AppError::Api(
            ApiError::new(ErrorCode::PermissionLockSecret, "wrong PIN or password")
                .with_detail("failures", rt.failures)
                .with_detail("retry_after_ms", delay),
        ))
    }

    fn store_hash(&self, file: &mut RegistryFile, kind: LockKind, hash: String, idle: u32) -> AppResult<()> {
        let persistent = self.secrets.persistent();
        if persistent {
            self.secrets
                .set(&secret_account(&self.profile_id), &Secret::new(hash.clone()))
                .map_err(storage_error)?;
        }
        let entry = file
            .get_mut(&self.profile_id)
            .ok_or_else(|| not_found(&self.profile_id))?;
        entry.lock = Some(LockEntry {
            kind,
            idle_minutes: idle,
            hash: (!persistent).then_some(hash),
        });
        Ok(())
    }

    fn check_idle_choice(minutes: u32) -> AppResult<()> {
        if valid_idle_minutes(minutes) {
            Ok(())
        } else {
            Err(AppError::Api(
                ApiError::new(ErrorCode::ValidationLockIdle, format!("idle {minutes}"))
                    .with_detail("choices", IDLE_CHOICES.to_vec()),
            ))
        }
    }

    /// Sets or changes the secret (changing needs the current one).
    pub(crate) fn set(
        &self,
        kind: LockKind,
        secret: &str,
        current: Option<&str>,
        idle: Option<u32>,
    ) -> AppResult<LockState> {
        validate_secret(kind, secret).map_err(|e| {
            AppError::Api(
                ApiError::new(ErrorCode::ValidationLockSecret, format!("bad secret: {}", e.code()))
                    .with_detail("reason", e.code()),
            )
        })?;
        {
            let _io = lock(&self.io);
            let mut file = self.store.load()?;
            let entry = Self::current_entry(&file, &self.profile_id)?;
            let existing = self.lock_with_hash(&entry)?;
            if let Some((_, hash)) = &existing {
                let current = current.ok_or_else(|| lock_state("current_required"))?;
                self.attempt(current, hash)?;
            }
            let idle = idle
                .or(existing.as_ref().map(|(l, _)| l.idle_minutes))
                .unwrap_or(DEFAULT_IDLE_MINUTES);
            Self::check_idle_choice(idle)?;
            let hash = hash_secret(secret)?;
            self.store_hash(&mut file, kind, hash, idle)?;
            self.store.save(&file)?;
            let mut rt = lock(&self.runtime);
            rt.locked = false;
            rt.idle = Some(idle);
            rt.last_activity = self.now();
        }
        self.status()
    }

    /// Changes the idle timeout of the lock.
    pub(crate) fn configure(&self, idle: u32) -> AppResult<LockState> {
        Self::check_idle_choice(idle)?;
        {
            let _io = lock(&self.io);
            let mut file = self.store.load()?;
            let entry = file
                .get_mut(&self.profile_id)
                .ok_or_else(|| not_found(&self.profile_id))?;
            let l = entry.lock.as_mut().ok_or_else(|| lock_state("not_set"))?;
            l.idle_minutes = idle;
            self.store.save(&file)?;
            lock(&self.runtime).idle = Some(idle);
        }
        self.touch();
        self.status()
    }

    /// Removes the lock (needs the current secret).
    pub(crate) fn clear(&self, current: &str) -> AppResult<LockState> {
        {
            let _io = lock(&self.io);
            let mut file = self.store.load()?;
            let entry = Self::current_entry(&file, &self.profile_id)?;
            let (_, hash) = self
                .lock_with_hash(&entry)?
                .ok_or_else(|| lock_state("not_set"))?;
            self.attempt(current, &hash)?;
            if let Some(e) = file.get_mut(&self.profile_id) {
                e.lock = None;
            }
            self.store.save(&file)?;
            self.secrets
                .delete(&secret_account(&self.profile_id))
                .map_err(storage_error)?;
            let mut rt = lock(&self.runtime);
            rt.locked = false;
            rt.idle = None;
        }
        self.status()
    }

    /// Unlocks with the secret. `Ok(true)` when it was locked.
    pub(crate) fn unlock(&self, secret: &str) -> AppResult<bool> {
        if !self.is_locked() {
            return Ok(false);
        }
        let file = self.store.load()?;
        let entry = Self::current_entry(&file, &self.profile_id)?;
        let Some((_, hash)) = self.lock_with_hash(&entry)? else {
            // The lock was removed meanwhile (or its hash is gone): nothing to check.
            let mut rt = lock(&self.runtime);
            rt.locked = false;
            rt.idle = None;
            return Ok(true);
        };
        self.attempt(secret, &hash)?;
        let now = self.now();
        let mut rt = lock(&self.runtime);
        let was = rt.locked;
        rt.locked = false;
        rt.last_activity = now;
        Ok(was)
    }

    /// Locks now. `Ok(true)` when it was unlocked.
    pub(crate) fn lock_now(&self) -> AppResult<bool> {
        let file = self.store.load()?;
        let entry = Self::current_entry(&file, &self.profile_id)?;
        if self.lock_with_hash(&entry)?.is_none() {
            return Err(lock_state("not_set"));
        }
        let mut rt = lock(&self.runtime);
        let was = !rt.locked;
        rt.locked = true;
        Ok(was)
    }

    /// The user did something: restart the idle timer (no effect while locked).
    pub fn touch(&self) {
        let now = self.now();
        let mut rt = lock(&self.runtime);
        if !rt.locked {
            rt.last_activity = now;
        }
    }

    /// Locks when idle longer than the timeout. `true` when it locked just now. No I/O.
    pub fn check_idle(&self) -> bool {
        let now = self.now();
        let mut rt = lock(&self.runtime);
        match rt.idle {
            Some(idle) if !rt.locked && idle_expired(rt.last_activity, now, idle) => {
                rt.locked = true;
                true
            }
            _ => false,
        }
    }
}
