//! `profiles` module (v1.0, docs/02 §28–§29): profiles (separate collections) and the app
//! lock. Both sit **above** the collection — the registry decides which collection opens and
//! the lock whether the window may use it — so nothing here is a collection write: no oplog,
//! no undo (a lock change must never be undoable), and only the user may call them.
//!
//! * The desktop shell opens a [`Session`] (`ProfileStore::active_collection` → the database
//!   to open), installs it with [`App::install_session`] and passes every IPC call through
//!   [`gate`]: while locked only [`ALLOWED_WHILE_LOCKED`] get through (`PERMISSION_APP_LOCKED`).
//! * [`spawn_idle_watch`] locks an idle app and publishes `lock_changed`.
//! * The module cannot be switched off: it is the session backbone (like `core`), and a lock
//!   must not be removable by flipping a switch. With one profile and no PIN it does nothing.

pub mod secrets;
pub mod session;
pub mod store;
#[cfg(test)]
mod tests;

pub use session::Session;
pub use store::ProfileStore;

use crate::error::{AppError, AppResult};
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::App;
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::oplog::Actor;
use fc_api::v1::session::*;
use fc_api::{ApiError, ErrorCode};
use std::sync::{Arc, RwLock, Weak};

/// Module id.
pub const ID: &str = "profiles";

/// Methods the desktop bridge lets through while the app is locked.
pub const ALLOWED_WHILE_LOCKED: &[&str] = &[
    "lock.status",
    "lock.unlock",
    "lock.recover",
    "profiles.list",
    "profiles.switch",
];

/// How often [`spawn_idle_watch`] looks at the idle timer.
pub const IDLE_TICK: std::time::Duration = std::time::Duration::from_secs(5);

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.profiles.name",
        description_key: "modules.profiles.description",
        enabled_by_default: true,
        can_disable: false,
        settings: vec![],
        ui: vec![main_menu("profiles.screen", "nav.profiles", "users", "/profiles", 96)],
    }
}

/// The installed session (none in the CLI/MCP server/tests that do not install one).
#[derive(Default)]
pub struct SessionSlot(RwLock<Option<Arc<Session>>>);

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.provide(Arc::new(SessionSlot::default()));
    reg.action::<ProfilesList>(list);
    reg.action::<ProfilesCreate>(create);
    reg.action::<ProfilesRename>(rename);
    reg.action::<ProfilesDelete>(delete);
    reg.action::<ProfilesSwitch>(switch);
    reg.action::<LockStatus>(status);
    reg.action::<LockSet>(set);
    reg.action::<LockConfigure>(configure);
    reg.action::<LockClear>(clear);
    reg.action::<LockUnlock>(unlock);
    reg.action::<LockLock>(lock_now);
    reg.action::<LockRecover>(recover);
    reg.action::<LockActivity>(activity);
}

impl App {
    /// Installs the desktop session (profile + app lock). Until then `profiles.*` report
    /// "unavailable" and the lock never engages.
    pub fn install_session(&self, session: Arc<Session>) {
        if let Some(slot) = self.registry().extension::<SessionSlot>() {
            if let Ok(mut s) = slot.0.write() {
                *s = Some(session);
            }
        }
    }

    /// The installed session.
    pub fn session(&self) -> Option<Arc<Session>> {
        self.registry()
            .extension::<SessionSlot>()
            .and_then(|slot| slot.0.read().ok().and_then(|s| s.clone()))
    }

    /// Whether background work bound to this collection must stop or refuse: the app is
    /// locked, or another profile was chosen ([`Session::blocks_background`]). Always
    /// `false` without a session (CLI, MCP server, tests).
    pub fn background_blocked(&self) -> bool {
        self.session().is_some_and(|s| s.blocks_background())
    }
}

/// `PERMISSION_APP_LOCKED` for `method`.
pub fn locked_error(method: &str) -> ApiError {
    ApiError::new(ErrorCode::PermissionAppLocked, "the app is locked")
        .with_detail("method", method)
}

/// Callers that act without the person at the keyboard: refused while
/// [`App::background_blocked`].
pub fn is_background_actor(actor: &Actor) -> bool {
    matches!(
        actor,
        Actor::LocalApi(_) | Actor::Automation(_) | Actor::Plugin(_)
    )
}

/// The desktop bridge's check before every call: idle → lock (publishing `lock_changed`);
/// locked → only [`ALLOWED_WHILE_LOCKED`]. `method` is the qualified wire name.
pub fn gate(app: &App, method: &str) -> Result<(), ApiError> {
    let Some(s) = app.session() else {
        return Ok(());
    };
    if s.check_idle() {
        app.publish(vec![DomainEvent::LockChanged { locked: true }], None);
    }
    if !s.is_locked() {
        return Ok(());
    }
    let name = method
        .strip_prefix(fc_api::API_VERSION)
        .and_then(|n| n.strip_prefix('.'))
        .unwrap_or(method);
    if ALLOWED_WHILE_LOCKED.contains(&name) {
        Ok(())
    } else {
        Err(locked_error(method))
    }
}

/// Background thread locking the app once it has been idle for the timeout; ends with the
/// app.
pub fn spawn_idle_watch(app: &Arc<App>) -> std::io::Result<std::thread::JoinHandle<()>> {
    let weak: Weak<App> = Arc::downgrade(app);
    std::thread::Builder::new()
        .name("fc-lock".into())
        .spawn(move || loop {
            std::thread::sleep(IDLE_TICK);
            let Some(app) = weak.upgrade() else { break };
            if app.session().is_some_and(|s| s.check_idle()) {
                app.publish(vec![DomainEvent::LockChanged { locked: true }], None);
            }
        })
}

/// Only the person at the computer: the desktop user, the CLI, the app itself.
fn ensure_user(actor: &Actor) -> AppResult<()> {
    if matches!(actor, Actor::User | Actor::Cli | Actor::System) {
        Ok(())
    } else {
        Err(AppError::Api(
            ApiError::new(ErrorCode::PermissionDenied, "profiles and the app lock are the user's")
                .with_detail("reason", "actor"),
        ))
    }
}

fn session_of(app: &App) -> AppResult<Arc<Session>> {
    app.session().ok_or_else(|| {
        AppError::Api(ApiError::new(
            ErrorCode::ConflictProfilesUnavailable,
            "no profile registry in this process",
        ))
    })
}

fn unavailable_lock() -> LockState {
    LockState {
        available: false,
        enabled: false,
        kind: None,
        locked: false,
        recovery_required: false,
        idle_minutes: 0,
        idle_choices: fc_core::ux::lock::IDLE_CHOICES.to_vec(),
        failures: 0,
        retry_after_ms: 0,
        secure_storage: false,
        profile_id: None,
        profile_name: None,
    }
}

// ------------------------------------------------------------------ profiles

fn list(_: Empty, app: &App, actor: &Actor) -> AppResult<ProfileList> {
    ensure_user(actor)?;
    let Some(s) = app.session() else {
        return Ok(ProfileList {
            available: false,
            profiles: vec![],
            active: None,
        });
    };
    let (profiles, active) = s.list()?;
    Ok(ProfileList {
        available: true,
        profiles,
        active: Some(active),
    })
}

fn create(req: CreateProfile, app: &App, actor: &Actor) -> AppResult<ProfileDto> {
    ensure_user(actor)?;
    session_of(app)?.create(&req.name)
}

fn rename(req: RenameProfile, app: &App, actor: &Actor) -> AppResult<ProfileDto> {
    ensure_user(actor)?;
    session_of(app)?.rename(&req.id, &req.name)
}

fn delete(req: DeleteProfile, app: &App, actor: &Actor) -> AppResult<Empty> {
    ensure_user(actor)?;
    session_of(app)?.delete(&req.id, req.secret.as_deref())?;
    Ok(Empty {})
}

fn switch(req: SwitchProfile, app: &App, actor: &Actor) -> AppResult<ProfileSwitched> {
    ensure_user(actor)?;
    let (profile, restart_required) = session_of(app)?.switch(&req.id)?;
    if restart_required {
        // The shell restarts on the new profile; until then nothing in the background may
        // touch this collection (`background_blocked`).
        app.publish(
            vec![DomainEvent::ProfileSwitched {
                profile_id: profile.id.clone(),
            }],
            None,
        );
    }
    Ok(ProfileSwitched {
        profile,
        restart_required,
    })
}

// ------------------------------------------------------------------ lock

fn status(_: Empty, app: &App, actor: &Actor) -> AppResult<LockState> {
    ensure_user(actor)?;
    match app.session() {
        Some(s) => s.status(),
        None => Ok(unavailable_lock()),
    }
}

fn set(req: SetLock, app: &App, actor: &Actor) -> AppResult<LockState> {
    ensure_user(actor)?;
    session_of(app)?.set(req.kind, &req.secret, req.current.as_deref(), req.idle_minutes)
}

fn configure(req: ConfigureLock, app: &App, actor: &Actor) -> AppResult<LockState> {
    ensure_user(actor)?;
    session_of(app)?.configure(req.idle_minutes)
}

fn clear(req: ClearLock, app: &App, actor: &Actor) -> AppResult<LockState> {
    ensure_user(actor)?;
    session_of(app)?.clear(&req.current)
}

fn unlock(req: Unlock, app: &App, actor: &Actor) -> AppResult<LockState> {
    ensure_user(actor)?;
    let s = session_of(app)?;
    if s.unlock(&req.secret)? {
        app.publish(vec![DomainEvent::LockChanged { locked: false }], None);
    }
    s.status()
}

fn recover(_: Empty, app: &App, actor: &Actor) -> AppResult<LockState> {
    ensure_user(actor)?;
    let s = session_of(app)?;
    if s.recover()? {
        app.publish(vec![DomainEvent::LockChanged { locked: false }], None);
    }
    s.status()
}

fn lock_now(_: Empty, app: &App, actor: &Actor) -> AppResult<LockState> {
    ensure_user(actor)?;
    let s = session_of(app)?;
    if s.lock_now()? {
        app.publish(vec![DomainEvent::LockChanged { locked: true }], None);
    }
    s.status()
}

fn activity(_: Empty, app: &App, actor: &Actor) -> AppResult<Empty> {
    ensure_user(actor)?;
    if let Some(s) = app.session() {
        s.touch();
    }
    Ok(Empty {})
}
