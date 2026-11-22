//! Profiles (separate collections) and the app lock (v1.0, docs/02 §28–§29).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::secrets::{scoped_account, ProfileSecrets};
use super::store::{PROFILES_DIR, REGISTRY_FILE};
use super::*;
use crate::testing::{MemStore, TestApp};
use fc_ai::secrets::{MemorySecrets, Secret, SecretError, SecretStore};
use fc_api::v1::automation::{CreateRule, RuleAction};
use fc_api::v1::modules::SetModuleEnabled;
use fc_core::automation::RuleTrigger;
use fc_core::time::FixedClock;
use fc_core::ux::lock::{LockKind, FREE_ATTEMPTS};
use fc_core::ux::profile::DEFAULT_PROFILE_ID;
use fc_core::Timestamp;
use serde_json::json;

/// A keychain that keeps secrets across restarts (like Windows Credential Manager).
#[derive(Default)]
struct Keychain(MemorySecrets);

impl SecretStore for Keychain {
    fn get(&self, account: &str) -> Result<Option<Secret>, SecretError> {
        self.0.get(account)
    }
    fn set(&self, account: &str, secret: &Secret) -> Result<(), SecretError> {
        self.0.set(account, secret)
    }
    fn delete(&self, account: &str) -> Result<(), SecretError> {
        self.0.delete(account)
    }
}

struct Fx {
    root: tempfile::TempDir,
    clock: Arc<FixedClock>,
    secrets: Arc<dyn SecretStore>,
    t: TestApp,
}

impl Fx {
    fn new() -> Self {
        Self::with_secrets(Arc::new(MemorySecrets::new()))
    }

    fn with_secrets(secrets: Arc<dyn SecretStore>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let clock = Arc::new(FixedClock::new(Timestamp(1_790_000_000_000)));
        let t = TestApp::with_clock(Arc::new(MemStore::new()), clock.clone());
        let fx = Self {
            root,
            clock,
            secrets,
            t,
        };
        fx.t.app.install_session(Arc::new(fx.open_session()));
        fx
    }

    fn store(&self) -> ProfileStore {
        ProfileStore::new(self.root.path())
    }

    /// What the next process start would open.
    fn open_session(&self) -> Session {
        Session::open(self.store(), self.secrets.clone(), self.clock.clone()).unwrap()
    }

    fn session(&self) -> Arc<Session> {
        self.t.app.session().unwrap()
    }

    fn set_pin(&self, pin: &str, idle: Option<u32>) -> LockState {
        self.t.ok::<LockSet>(SetLock {
            kind: LockKind::Pin,
            secret: pin.into(),
            current: None,
            idle_minutes: idle,
        })
    }

    fn lock_events(&self) -> Vec<bool> {
        self.t
            .take_events()
            .into_iter()
            .filter_map(|e| match e.event {
                DomainEvent::LockChanged { locked } => Some(locked),
                _ => None,
            })
            .collect()
    }

    fn registry_text(&self) -> String {
        std::fs::read_to_string(self.root.path().join(REGISTRY_FILE)).unwrap()
    }
}

// ------------------------------------------------------------------ profiles

/// "Profiles = separate collections": each profile has its own database file (and media
/// folder next to it); the default one stays where every earlier version kept it.
#[test]
fn profiles_are_separate_collections() {
    let fx = Fx::new();
    let list = fx.t.ok::<ProfilesList>(Empty {});
    assert!(list.available);
    assert_eq!(list.active.as_deref(), Some(DEFAULT_PROFILE_ID));
    assert_eq!(
        fx.session().collection_path(),
        fx.root.path().join("collection.db"),
        "upgrading keeps the data in place"
    );

    let work = fx.t.ok::<ProfilesCreate>(CreateProfile {
        name: "  İş  ".into(),
    });
    assert_eq!(work.name.as_deref(), Some("İş"));
    assert!(!work.active);
    let work_db = fx.store().collection_path(&work.id);
    assert_eq!(
        work_db,
        fx.root
            .path()
            .join(PROFILES_DIR)
            .join(&work.id)
            .join("collection.db")
    );
    assert!(work_db.parent().unwrap().is_dir());

    // Names are unique (case-insensitive) and checked.
    let dup = fx.t.err::<ProfilesCreate>(CreateProfile { name: "İŞ".into() });
    assert_eq!(dup.code, ErrorCode::ConflictProfileName);
    let blank = fx.t.err::<ProfilesCreate>(CreateProfile { name: " ".into() });
    assert_eq!(blank.code, ErrorCode::ValidationProfileName);
    let renamed = fx.t.ok::<ProfilesRename>(RenameProfile {
        id: work.id.clone(),
        name: "Okul".into(),
    });
    assert_eq!(renamed.name.as_deref(), Some("Okul"));

    // The active (here also the default) profile cannot be deleted.
    let e = fx.t.err::<ProfilesDelete>(DeleteProfile {
        id: DEFAULT_PROFILE_ID.into(),
        secret: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictProfileActive);
    let other = fx.t.ok::<ProfilesCreate>(CreateProfile {
        name: "Geçici".into(),
    });
    let other_dir = fx.store().profile_dir(&other.id);
    std::fs::write(other_dir.join("collection.db"), b"x").unwrap();
    fx.t.ok::<ProfilesDelete>(DeleteProfile {
        id: other.id.clone(),
        secret: None,
    });
    assert!(!other_dir.exists(), "its folder goes with it");
    assert_eq!(fx.t.ok::<ProfilesList>(Empty {}).profiles.len(), 2);
}

/// Switching retires the running session (background work stops at once), announces the
/// switch for the shell's restart, and the next start opens the other profile.
#[test]
fn switching_retires_the_session_and_the_next_start_opens_the_new_profile() {
    let fx = Fx::new();
    let work = fx.t.ok::<ProfilesCreate>(CreateProfile { name: "İş".into() });
    fx.t.take_events();
    assert!(!fx.t.app.background_blocked());
    let r = fx.t.ok::<ProfilesSwitch>(SwitchProfile {
        id: work.id.clone(),
    });
    assert!(r.restart_required);
    assert!(fx.session().is_retired());
    assert!(fx.t.app.background_blocked());
    assert!(fx.t.take_events().iter().any(|e| matches!(
        &e.event,
        DomainEvent::ProfileSwitched { profile_id } if *profile_id == work.id
    )));
    // The person at the keyboard still gets in (the restart is a moment away).
    assert!(gate(&fx.t.app, "v1.decks.list").is_ok());

    let next = fx.open_session();
    assert_eq!(next.profile_id(), work.id);
    assert_eq!(next.collection_path(), fx.store().collection_path(&work.id));
    assert!(!next.is_retired());

    // Switching back before the restart keeps this collection.
    let back = fx.t.ok::<ProfilesSwitch>(SwitchProfile {
        id: DEFAULT_PROFILE_ID.into(),
    });
    assert!(!back.restart_required);
    assert!(!fx.t.app.background_blocked());
}

/// v1.0 round 2: switching profiles drops the in-memory study queue cache (the session
/// queue, stats caches and theme are stored in each profile's own collection).
#[test]
fn switching_profiles_drops_the_in_memory_queue_cache() {
    use fc_api::v1::decks::CreateDeck;
    use fc_api::v1::notes::CreateNote;
    use fc_api::v1::study::{AnswerCard, StartSession, StudyKind};
    use fc_core::card::Rating;
    use std::collections::BTreeMap;

    let fx = Fx::new();
    let deck = fx.t.ok::<DecksCreate>(CreateDeck {
        path: "Deste".into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    });
    for (front, back) in [("a", "1"), ("b", "2"), ("c", "3")] {
        fx.t.ok::<NotesCreate>(CreateNote {
            notetype_id: fc_core::notetype::builtin::CLASSIC_ID,
            deck_id: deck.id,
            fields: BTreeMap::from([
                ("front".to_owned(), front.to_owned()),
                ("back".to_owned(), back.to_owned()),
            ]),
            tags: vec![],
            attributes: vec![],
            allow_duplicate: None,
        });
    }
    let s = fx.t.ok::<StudyStart>(StartSession {
        kind: StudyKind::Normal,
        mode: None,
        deck_id: Some(deck.id),
        include_subdecks: None,
        filter: None,
        from_session: None,
        new_limit: None,
        order: None,
        tz_offset_mins: Some(0),
        modes: None,
        scope: None,
    });
    let card = s.current.as_ref().expect("a card to study").card_id;
    fx.t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: card,
        rating: Rating::Good,
        duration_ms: 1_000,
        reveal_ms: None,
    });
    assert!(
        !crate::modules::study::queue_cache_is_empty(fx.t.app.registry()),
        "answering caches the saved queue"
    );
    let work = fx.t.ok::<ProfilesCreate>(CreateProfile { name: "İş".into() });
    fx.t.ok::<ProfilesSwitch>(SwitchProfile { id: work.id });
    assert!(crate::modules::study::queue_cache_is_empty(
        fx.t.app.registry()
    ));
    // The session itself is still stored in this collection: going back resumes it.
    let again = fx.t.ok::<StudySession>(fc_api::v1::study::SessionRef { session_id: s.id });
    assert_eq!(again.id, s.id);
}

/// A hand-edited registry can never point a profile outside the profiles folder.
#[test]
fn registry_ids_cannot_escape_and_unknown_active_falls_back() {
    let fx = Fx::new();
    std::fs::write(
        fx.root.path().join(REGISTRY_FILE),
        json!({
            "version": 1,
            "active": "../../etc",
            "profiles": [{ "id": "../../etc", "name": "x" }]
        })
        .to_string(),
    )
    .unwrap();
    let (id, path) = fx.store().active_collection().unwrap();
    assert_eq!(id, DEFAULT_PROFILE_ID);
    assert_eq!(path, fx.root.path().join("collection.db"));
}

/// Only the person at the computer manages profiles and the lock: plugins, automation rules
/// and API clients are refused.
#[test]
fn only_the_user_manages_profiles_and_the_lock() {
    let fx = Fx::new();
    for actor in [
        Actor::Plugin("p".into()),
        Actor::Mcp("claude".into()),
        Actor::LocalApi("ext".into()),
    ] {
        let e = fx
            .t
            .app
            .call::<ProfilesList>(&Empty {}, &actor)
            .unwrap_err();
        assert!(
            matches!(e.code, ErrorCode::PermissionDenied),
            "{actor:?}: {e:?}"
        );
    }
}

/// Without a session (CLI, MCP server) profiles are "unavailable" and nothing is locked.
#[test]
fn without_a_session_profiles_are_unavailable() {
    let t = TestApp::mem();
    assert!(!t.ok::<ProfilesList>(Empty {}).available);
    assert!(!t.ok::<LockStatus>(Empty {}).available);
    let e = t.err::<ProfilesCreate>(CreateProfile { name: "x".into() });
    assert_eq!(e.code, ErrorCode::ConflictProfilesUnavailable);
    assert!(gate(&t.app, "v1.decks.list").is_ok());
    assert!(!t.app.background_blocked());
}

// ------------------------------------------------------------------ lock

/// The secret is never stored: only an Argon2id hash with a random salt (in the registry
/// when the keychain keeps nothing across restarts).
#[test]
fn only_an_argon2id_hash_is_kept() {
    let fx = Fx::new();
    let st = fx.set_pin("4821", None);
    assert!(st.enabled && !st.locked && !st.secure_storage);
    assert_eq!(st.kind, Some(LockKind::Pin));
    assert_eq!(st.idle_minutes, fc_core::ux::lock::DEFAULT_IDLE_MINUTES);
    let text = fx.registry_text();
    assert!(!text.contains("4821"));
    assert!(text.contains("$argon2id$"), "{text}");
    // Same PIN, new salt: a different hash.
    let first = text.clone();
    fx.t.ok::<LockSet>(SetLock {
        kind: LockKind::Pin,
        secret: "4821".into(),
        current: Some("4821".into()),
        idle_minutes: None,
    });
    assert_ne!(fx.registry_text(), first);
    // Changing it needs the current one.
    let e = fx.t.err::<LockSet>(SetLock {
        kind: LockKind::Password,
        secret: "şifre123".into(),
        current: None,
        idle_minutes: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictLockState);
    let bad = fx.t.err::<LockSet>(SetLock {
        kind: LockKind::Pin,
        secret: "12".into(),
        current: Some("4821".into()),
        idle_minutes: None,
    });
    assert_eq!(bad.code, ErrorCode::ValidationLockSecret);
}

/// With a real (persistent) keychain the hash lives there and the registry has none.
#[test]
fn the_hash_goes_to_the_os_keychain_when_it_persists() {
    let keychain = Arc::new(Keychain::default());
    let fx = Fx::with_secrets(keychain.clone());
    let st = fx.set_pin("4821", None);
    assert!(st.secure_storage);
    assert!(!fx.registry_text().contains("argon2"));
    let stored = keychain
        .get(&session::secret_account(DEFAULT_PROFILE_ID))
        .unwrap()
        .unwrap();
    assert!(stored.expose().starts_with("$argon2id$"));
    // A restart starts locked and unlocks against the keychain hash.
    let next = fx.open_session();
    assert!(next.is_locked());
    assert!(next.unlock("4821").unwrap());
    // Clearing removes it from the keychain.
    fx.t.ok::<LockClear>(ClearLock {
        current: "4821".into(),
    });
    assert!(keychain
        .get(&session::secret_account(DEFAULT_PROFILE_ID))
        .unwrap()
        .is_none());
}

/// Locked: the window may only reach the lock screen's methods; wrong attempts back off.
#[test]
fn locked_app_lets_only_the_lock_screen_through_and_backs_off() {
    let fx = Fx::new();
    fx.set_pin("4821", Some(5));
    fx.t.take_events();
    fx.t.ok::<LockLock>(Empty {});
    assert_eq!(fx.lock_events(), vec![true]);
    let e = gate(&fx.t.app, "v1.decks.list").unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionAppLocked);
    for allowed in ALLOWED_WHILE_LOCKED {
        assert!(gate(&fx.t.app, &format!("v1.{allowed}")).is_ok(), "{allowed}");
    }
    // The lock screen draws in the profile's theme (read only); changing it or reading
    // other settings stays behind the lock.
    assert!(gate(&fx.t.app, "v1.themes.current").is_ok());
    for refused in ["v1.themes.set", "v1.themes.presets", "v1.settings.get"] {
        assert_eq!(
            gate(&fx.t.app, refused).unwrap_err().code,
            ErrorCode::PermissionAppLocked,
            "{refused}"
        );
    }

    // Wrong PINs: free attempts, then a growing wait.
    for n in 1..=FREE_ATTEMPTS {
        let e = fx.t.err::<LockUnlock>(Unlock {
            secret: "0000".into(),
        });
        assert_eq!(e.code, ErrorCode::PermissionLockSecret, "attempt {n}");
    }
    let e = fx.t.err::<LockUnlock>(Unlock {
        secret: "4821".into(),
    });
    assert_eq!(e.code, ErrorCode::ConflictLockRetry, "even the right PIN waits");
    assert!(fx.t.ok::<LockStatus>(Empty {}).retry_after_ms > 0);
    fx.clock.advance(1_000);
    let st = fx.t.ok::<LockUnlock>(Unlock {
        secret: "4821".into(),
    });
    assert!(!st.locked);
    assert_eq!(st.failures, 0);
    assert_eq!(fx.lock_events(), vec![false]);
    assert!(gate(&fx.t.app, "v1.decks.list").is_ok());

    // Removing the lock needs the PIN.
    let e = fx.t.err::<LockClear>(ClearLock {
        current: "1111".into(),
    });
    assert_eq!(e.code, ErrorCode::PermissionLockSecret);
    let st = fx.t.ok::<LockClear>(ClearLock {
        current: "4821".into(),
    });
    assert!(!st.enabled);
    let e = fx.t.err::<LockLock>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictLockState);
}

/// Auto-lock: idle for the chosen minutes → locked (and announced); activity restarts the
/// timer; `0` never locks; a restart of a locked profile starts locked.
#[test]
fn idle_app_locks_itself() {
    let fx = Fx::new();
    fx.set_pin("4821", Some(1));
    fx.t.take_events();
    fx.clock.advance(50_000);
    assert!(gate(&fx.t.app, "v1.decks.list").is_ok());
    fx.t.ok::<LockActivity>(Empty {});
    fx.clock.advance(50_000);
    assert!(!fx.session().check_idle(), "activity restarted the timer");
    fx.clock.advance(10_000);
    let e = gate(&fx.t.app, "v1.decks.list").unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionAppLocked);
    assert_eq!(fx.lock_events(), vec![true]);
    assert!(fx.open_session().is_locked(), "a restart starts locked");

    fx.t.ok::<LockUnlock>(Unlock {
        secret: "4821".into(),
    });
    let st = fx.t.ok::<LockConfigure>(ConfigureLock { idle_minutes: 0 });
    assert_eq!(st.idle_minutes, 0);
    fx.clock.advance(24 * 3_600_000);
    assert!(!fx.session().check_idle(), "0 = never");
    let e = fx.t.err::<LockConfigure>(ConfigureLock { idle_minutes: 7 });
    assert_eq!(e.code, ErrorCode::ValidationLockIdle);
}

/// Phase-4 review: while locked (or switching profile) the local HTTP API's clients,
/// automation rules and plugins are refused; scheduled rules do not run.
#[test]
fn background_callers_are_refused_while_locked() {
    let fx = Fx::new();
    fx.t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "automation".into(),
        enabled: true,
    });
    fx.t.ok::<fc_api::v1::methods::AutomationRulesCreate>(CreateRule {
        name: "Nightly".into(),
        enabled: None,
        trigger: RuleTrigger::Schedule { every_mins: 60 },
        condition: "tag:none".into(),
        actions: vec![RuleAction {
            method: "bulk.add_tags".into(),
            params: json!({ "tags": ["x"] }),
        }],
    });
    fx.set_pin("4821", Some(5));
    fx.t.ok::<LockLock>(Empty {});
    assert!(fx.t.app.background_blocked());
    for actor in [
        Actor::LocalApi("ext".into()),
        Actor::Automation("rule".into()),
        Actor::Plugin("p".into()),
    ] {
        let e = fx
            .t
            .app
            .call::<DecksList>(&Empty {}, &actor)
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::PermissionAppLocked, "{actor:?}");
    }
    assert!(
        crate::modules::automation::run_due(&fx.t.app)
            .unwrap()
            .is_empty(),
        "the scheduler skips while locked"
    );

    fx.t.ok::<LockUnlock>(Unlock {
        secret: "4821".into(),
    });
    assert!(!fx.t.app.background_blocked());
    let e = fx
        .t
        .app
        .call::<DecksList>(&Empty {}, &Actor::LocalApi("ext".into()))
        .map(|_| ())
        .err();
    assert!(
        e.is_none_or(|e| e.code != ErrorCode::PermissionAppLocked),
        "unlocked: the ordinary rules apply"
    );
    assert_eq!(
        crate::modules::automation::run_due(&fx.t.app).unwrap().len(),
        1,
        "the due rule runs once unlocked"
    );
}

/// A locked profile is deleted only with its secret.
#[test]
fn deleting_a_locked_profile_needs_its_secret() {
    let fx = Fx::new();
    let kid = fx.t.ok::<ProfilesCreate>(CreateProfile { name: "Ece".into() });
    // Lock it from its own session (as if it were open).
    let mut file = fx.store().load().unwrap();
    file.active = kid.id.clone();
    fx.store().save(&file).unwrap();
    let kid_session = fx.open_session();
    kid_session
        .set(LockKind::Pin, "2468", None, None)
        .unwrap();
    let mut file = fx.store().load().unwrap();
    file.active = DEFAULT_PROFILE_ID.into();
    fx.store().save(&file).unwrap();

    assert!(fx.t.ok::<ProfilesList>(Empty {}).profiles[1].locked);
    let e = fx.t.err::<ProfilesDelete>(DeleteProfile {
        id: kid.id.clone(),
        secret: Some("0000".into()),
    });
    assert_eq!(e.code, ErrorCode::PermissionLockSecret);
    fx.t.ok::<ProfilesDelete>(DeleteProfile {
        id: kid.id,
        secret: Some("2468".into()),
    });
}

// ------------------------------------------------------------------ security review (v1.0)

fn wrong(fx: &Fx, pin: &str) -> ErrorCode {
    fx.t.err::<LockUnlock>(Unlock { secret: pin.into() }).code
}

/// Security review: the wrong-attempt back-off is kept in the registry per profile, so a
/// restart does not hand out fresh attempts.
#[test]
fn wrong_attempt_back_off_survives_a_restart() {
    let fx = Fx::new();
    fx.set_pin("4821", Some(5));
    fx.t.ok::<LockLock>(Empty {});
    for _ in 0..FREE_ATTEMPTS {
        assert_eq!(wrong(&fx, "0000"), ErrorCode::PermissionLockSecret);
    }
    assert!(fx.registry_text().contains("\"failures\": 5"), "{}", fx.registry_text());

    // "Relaunch": a new session of the same profile is still waiting, with the same count.
    let next = Arc::new(fx.open_session());
    assert!(next.is_locked());
    let st = next.status().unwrap();
    assert_eq!(st.failures, FREE_ATTEMPTS);
    assert!(st.retry_after_ms > 0);
    let e = next.unlock("4821").unwrap_err().into_api();
    assert_eq!(e.code, ErrorCode::ConflictLockRetry, "no free tries after a restart");

    // Exponential: the next wrong one doubles the wait, also across a restart.
    fx.clock.advance(1_000);
    let e = next.unlock("0000").unwrap_err().into_api();
    assert_eq!(e.details["retry_after_ms"], 2_000);
    let again = fx.open_session();
    assert_eq!(again.status().unwrap().retry_after_ms, 2_000);
    fx.clock.advance(2_000);
    assert!(again.unlock("4821").unwrap());
    let after = fx.open_session().status().unwrap();
    assert_eq!((after.failures, after.retry_after_ms), (0, 0), "reset on success");
}

/// Security review: check and record of an attempt are one step, so parallel wrong attempts
/// cannot all slip into the free ones.
#[test]
fn parallel_attempts_share_no_free_slot() {
    let fx = Fx::new();
    fx.set_pin("4821", Some(5));
    fx.t.ok::<LockLock>(Empty {});
    let session = fx.session();
    let codes: Vec<ErrorCode> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..FREE_ATTEMPTS * 2)
            .map(|_| {
                let session = session.clone();
                s.spawn(move || session.unlock("0000").unwrap_err().into_api().code)
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let wrong = codes
        .iter()
        .filter(|c| **c == ErrorCode::PermissionLockSecret)
        .count();
    assert_eq!(wrong, FREE_ATTEMPTS as usize, "{codes:?}");
    assert_eq!(session.status().unwrap().failures, FREE_ATTEMPTS);
}

/// Security review: a profile marked as locked whose hash is gone (keychain entry removed)
/// fails closed — it starts locked, the PIN is not even checked, and only the explicit
/// recovery flow removes the lock.
#[test]
fn a_lock_without_its_hash_fails_closed_until_recovered() {
    let keychain = Arc::new(Keychain::default());
    let fx = Fx::with_secrets(keychain.clone());
    fx.set_pin("4821", Some(5));
    // Recovery is refused while the hash is there.
    let e = fx.t.err::<LockRecover>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictLockState);
    assert_eq!(e.details["reason"], "not_recoverable");
    fx.t.ok::<LockLock>(Empty {});
    keychain
        .delete(&session::secret_account(DEFAULT_PROFILE_ID))
        .unwrap();

    let next = fx.open_session();
    assert!(next.is_locked(), "a restart starts locked");
    let st = fx.t.ok::<LockStatus>(Empty {});
    assert!(st.locked && st.enabled && st.recovery_required);
    let e = fx.t.err::<LockUnlock>(Unlock {
        secret: "4821".into(),
    });
    assert_eq!(e.code, ErrorCode::ConflictLockState);
    assert_eq!(e.details["reason"], "recovery_required");
    assert!(fx.session().is_locked(), "not silently unlocked");
    assert!(gate(&fx.t.app, "v1.decks.list").is_err());
    let e = fx.t.err::<LockSet>(SetLock {
        kind: LockKind::Pin,
        secret: "1357".into(),
        current: None,
        idle_minutes: None,
    });
    assert_eq!(e.details["reason"], "recovery_required");
    assert!(gate(&fx.t.app, "v1.lock.recover").is_ok(), "reachable from the lock screen");

    fx.t.take_events();
    let st = fx.t.ok::<LockRecover>(Empty {});
    assert!(!st.locked && !st.enabled && !st.recovery_required);
    assert_eq!(fx.lock_events(), vec![false]);
    assert!(gate(&fx.t.app, "v1.decks.list").is_ok());
    assert!(!fx.open_session().is_locked());
    // A new lock can be set right away.
    assert!(fx.set_pin("1357", None).enabled);
}

/// Security review: AI keys are per profile. The default profile keeps the pre-v1.0
/// account names (existing keys and the CLI keep working); other profiles get their own.
#[test]
fn ai_keys_are_namespaced_per_profile() {
    use crate::modules::ai::key_account;
    let raw = Arc::new(MemorySecrets::new());
    let fx = Fx::with_secrets(raw.clone());
    let work = fx.t.ok::<ProfilesCreate>(CreateProfile { name: "İş".into() });
    let provider = fc_core::AiProviderId::from_uuid(uuid::Uuid::nil());
    let account = key_account(provider);

    // A key set before profiles existed is the default profile's.
    raw.set(&account, &Secret::new("sk-old")).unwrap();
    let default = fx.session().profile_secrets();
    assert_eq!(default.get(&account).unwrap().unwrap().expose(), "sk-old");

    // The other profile does not see it, and its own key has its own account.
    let theirs = ProfileSecrets::new(raw.clone(), &work.id);
    assert!(theirs.get(&account).unwrap().is_none());
    theirs.set(&account, &Secret::new("sk-work")).unwrap();
    assert_eq!(
        raw.get(&format!("profile.{}.{account}", work.id))
            .unwrap()
            .unwrap()
            .expose(),
        "sk-work"
    );
    assert_eq!(default.get(&account).unwrap().unwrap().expose(), "sk-old");
    theirs.delete(&account).unwrap();
    assert!(raw.get(&account).unwrap().is_some(), "only its own account");
    assert_eq!(scoped_account(DEFAULT_PROFILE_ID, &account), account);
}

/// Deleting a profile removes its keychain accounts (integration round 1): the AI keys its
/// collection's providers held and its lock hash. Another profile's keys stay. The CLI picks
/// the accounts of the profile whose collection `--db` names.
#[test]
fn deleting_a_profile_removes_its_keychain_accounts() {
    use crate::modules::ai::key_account;
    use fc_api::v1::ai::{AiProviderKind, CreateAiProvider, SetAiProviderKey};
    use fc_api::v1::methods::{AiProvidersCreate, AiProvidersSetKey};
    let raw = Arc::new(Keychain::default());
    let fx = Fx::with_secrets(raw.clone());
    let work = fx.t.ok::<ProfilesCreate>(CreateProfile { name: "İş".into() });
    let keep = fx.t.ok::<ProfilesCreate>(CreateProfile { name: "Ev".into() });

    // The work profile's collection, opened as the CLI would (`fc --db …`): its key lands in
    // the work profile's accounts.
    let work_db = fx.store().collection_path(&work.id);
    let app = crate::App::open_sqlite(&work_db).unwrap();
    assert_eq!(super::secrets::profile_of_collection(&work_db), work.id);
    assert_eq!(
        super::secrets::profile_of_collection(&fx.root.path().join("collection.db")),
        DEFAULT_PROFILE_ID
    );
    app.set_secret_store(super::secrets::collection_secrets(raw.clone(), &work_db));
    let provider = app
        .call::<AiProvidersCreate>(
            &CreateAiProvider {
                kind: AiProviderKind::Anthropic,
                name: None,
                base_url: None,
                default_model: None,
                is_default: None,
            },
            &Actor::User,
        )
        .unwrap();
    app.call::<AiProvidersSetKey>(
        &SetAiProviderKey { provider_id: provider.id, key: Some("sk-ant-work-0001".into()) },
        &Actor::User,
    )
    .unwrap();
    drop(app);
    let account = key_account(provider.id);
    let work_account = scoped_account(&work.id, &account);
    assert!(raw.get(&work_account).unwrap().is_some());
    assert!(raw.get(&account).unwrap().is_none(), "not the default profile's account");

    // Its lock hash, and a key of the profile that stays.
    raw.set(&session::secret_account(&work.id), &Secret::new("$argon2id$v=19$x")).unwrap();
    let keep_account = scoped_account(&keep.id, &account);
    raw.set(&keep_account, &Secret::new("sk-keep")).unwrap();

    fx.t.ok::<ProfilesDelete>(DeleteProfile { id: work.id.clone(), secret: None });
    assert!(raw.get(&work_account).unwrap().is_none(), "AI key removed");
    assert!(raw.get(&session::secret_account(&work.id)).unwrap().is_none(), "lock hash removed");
    assert!(!work_db.exists());
    assert_eq!(raw.get(&keep_account).unwrap().unwrap().expose(), "sk-keep");
}
