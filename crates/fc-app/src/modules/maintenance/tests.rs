//! Service tests of the `maintenance` and `updates` modules (three stores): crash recovery
//! after an unclean shutdown (session + drafts + crash report + integrity verdict), drafts,
//! log export redaction (including the exact API keys the app holds), the update flow with a
//! fake updater (signature refusal included) and that no automated actor reaches either area.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::crash::write_report;
use super::logging::{FileLog, Level};
use super::run::RunGuard;
use super::smoke::zip_contains;
use crate::modules::updates::{UpdateError, Updater};
use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_ai::secrets::MemorySecrets;
use fc_api::v1::maintenance::UpdateInfoDto;
use fc_api::v1::oplog::Actor;
use fc_api::ErrorCode;
use fc_core::maintenance::crash::CrashReport;
use fc_core::maintenance::redact::Redactor;
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::Timestamp;
use fc_storage::{SqliteStore, Store};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

type Factory = fn() -> Arc<dyn Store>;

macro_rules! suite {
    ($($name:ident),* $(,)?) => {
        mod mem {
            $( #[test] fn $name() { super::$name(|| std::sync::Arc::new(super::MemStore::new())); } )*
        }
        mod sqlite {
            $( #[test] fn $name() {
                super::$name(|| std::sync::Arc::new(super::SqliteStore::open_in_memory().unwrap()));
            } )*
        }
        mod sqlite_file {
            $( #[test] fn $name() { super::$name(|| std::sync::Arc::new(super::TempSqlite::new())); } )*
        }
    };
}

suite!(
    after_an_unclean_shutdown_the_app_offers_session_drafts_crash_and_integrity,
    a_clean_previous_run_offers_nothing,
    drafts_are_scratch_state_validated_and_bounded,
    exported_logs_never_contain_keys_or_tokens,
    updates_are_checked_downloaded_verified_and_applied_only_on_request,
    a_badly_signed_update_is_refused,
    no_automated_actor_reaches_maintenance_or_updates,
);

fn call(t: &TestApp, method: &str, params: Value) -> Value {
    t.app
        .invoke(&format!("v1.{method}"), params, &Actor::User)
        .unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

fn call_err(t: &TestApp, method: &str, params: Value) -> fc_api::ApiError {
    t.app
        .invoke(&format!("v1.{method}"), params, &Actor::User)
        .expect_err(method)
}

fn crash_report(at: Timestamp, message: &str) -> CrashReport {
    CrashReport {
        version: super::APP_VERSION.into(),
        at,
        os: super::crash::os_label(),
        thread: "main".into(),
        location: Some("crates/fc-app/src/x.rs:1:1".into()),
        message: message.into(),
        backtrace: String::new(),
    }
}

/// A study session left open with a card on screen.
fn open_session(t: &TestApp) -> Value {
    let deck = call(t, "decks.create", json!({"path": "Kurtarma"}));
    call(
        t,
        "notes.create",
        json!({"notetype_id": CLASSIC_ID, "deck_id": deck["id"], "fields": {"front": "kılıç", "back": "sword"}, "tags": [], "attributes": []}),
    );
    call(t, "study.start", json!({"kind": "normal", "deck_id": deck["id"]}))
}

fn after_an_unclean_shutdown_the_app_offers_session_drafts_crash_and_integrity(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    let session = open_session(&t);
    call(
        &t,
        "maintenance.drafts.save",
        json!({"key": "notes.add", "payload": {"fields": {"front": "yarım"}}}),
    );
    // Run 1 starts and "dies" (no finish), after writing a crash report.
    let (_g, _) = RunGuard::begin(dir.path(), "1.0.0", Timestamp(1_000)).unwrap();
    write_report(
        &dir.path().join(super::CRASH_DIR),
        &crash_report(Timestamp(2_000), "boom with sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAA"),
        &Redactor::new(),
    )
    .unwrap();
    // Run 2.
    let (_g2, start) = RunGuard::begin(dir.path(), "1.0.0", Timestamp(3_000)).unwrap();
    assert!(start.previous_unclean());
    let log = FileLog::open(&super::log_dir(dir.path()), Arc::new(crate::system::SystemClock)).unwrap();
    t.app.start_maintenance(dir.path(), Some(log), start);

    let offer = call(&t, "maintenance.recovery.get", json!({}));
    assert_eq!(offer["reason"], "crash");
    assert_eq!(offer["previous_run"]["clean"], false);
    assert_eq!(offer["session"]["id"], session["id"], "the open session can be resumed");
    assert_eq!(offer["drafts"][0]["key"], "notes.add");
    assert_eq!(offer["drafts"][0]["payload"]["fields"]["front"], "yarım");
    assert_eq!(offer["integrity"]["ok"], true, "the quick check ran at start");
    assert_eq!(offer["integrity"]["quick"], true);
    let crash = &offer["crash"];
    assert!(crash["message"].as_str().unwrap().starts_with("boom with"));
    assert!(!crash.to_string().contains("sk-ant-api03"), "the report was redacted: {crash}");
    // The session really resumes (the card is still on screen).
    let resumed = call(&t, "study.session", json!({"session_id": session["id"]}));
    assert_eq!(resumed["current"]["card_id"], session["current"]["card_id"]);
    let status = call(&t, "maintenance.status", json!({}));
    assert_eq!(status["crash_reports"].as_array().unwrap().len(), 1);
    assert_eq!(status["previous_run"]["clean"], false);

    // Dismissed: nothing more this run; not an undo step.
    let before = call(&t, "oplog.list", json!({})).to_string();
    let after = call(&t, "maintenance.recovery.dismiss", json!({}));
    assert_eq!(after["reason"], Value::Null);
    assert_eq!(call(&t, "maintenance.recovery.get", json!({}))["reason"], Value::Null);
    assert_eq!(call(&t, "oplog.list", json!({})).to_string(), before);
    assert_eq!(call(&t, "maintenance.crash_reports.clear", json!({}))["removed"], 1);
    assert!(call(&t, "maintenance.status", json!({}))["crash_reports"]
        .as_array()
        .unwrap()
        .is_empty());
    // A full integrity check on demand.
    let full = call(&t, "maintenance.integrity.check", json!({}));
    assert_eq!((full["ok"].clone(), full["quick"].clone()), (json!(true), json!(false)));
}

fn a_clean_previous_run_offers_nothing(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    open_session(&t);
    let (g, _) = RunGuard::begin(dir.path(), "1.0.0", Timestamp(1_000)).unwrap();
    g.finish().unwrap();
    let (_g, start) = RunGuard::begin(dir.path(), "1.0.0", Timestamp(2_000)).unwrap();
    t.app.start_maintenance(dir.path(), None, start);
    let offer = call(&t, "maintenance.recovery.get", json!({}));
    assert_eq!(offer["reason"], Value::Null);
    assert_eq!(offer["previous_run"]["clean"], true);
    assert_eq!(offer["integrity"], Value::Null, "no check after a clean run");
    // Without the desktop start (CLI, tests) the methods still answer.
    let bare = TestApp::new(f());
    assert_eq!(call(&bare, "maintenance.recovery.get", json!({}))["reason"], Value::Null);
    assert_eq!(call(&bare, "maintenance.status", json!({}))["log_dir"], Value::Null);
}

fn drafts_are_scratch_state_validated_and_bounded(f: Factory) {
    let t = TestApp::new(f());
    let before = call(&t, "oplog.list", json!({})).to_string();
    let saved = call(&t, "maintenance.drafts.save", json!({"key": "notes.add", "payload": {"a": 1}}));
    let again = call(&t, "maintenance.drafts.save", json!({"key": "notes.add", "payload": {"a": 2}}));
    assert_eq!(saved["created_at"], again["created_at"]);
    assert_eq!(call(&t, "maintenance.drafts.get", json!({"key": "notes.add"}))["draft"]["payload"], json!({"a": 2}));
    assert_eq!(call(&t, "oplog.list", json!({})).to_string(), before, "drafts are not undo steps");

    for bad in ["", "Notes.Add", "a b", &"x".repeat(101)] {
        let e = call_err(&t, "maintenance.drafts.save", json!({"key": bad, "payload": {}}));
        assert_eq!(e.code, ErrorCode::ValidationDraftKey, "{bad:?}");
    }
    let big = "x".repeat(super::drafts::MAX_PAYLOAD_BYTES + 1);
    let e = call_err(&t, "maintenance.drafts.save", json!({"key": "big", "payload": big}));
    assert_eq!(e.code, ErrorCode::ValidationDraftTooLarge);

    for i in 0..(super::drafts::MAX_DRAFTS + 5) {
        call(&t, "maintenance.drafts.save", json!({"key": format!("form.{i}"), "payload": i}));
    }
    let list = call(&t, "maintenance.drafts.list", json!({}));
    assert_eq!(list["items"].as_array().unwrap().len(), super::drafts::MAX_DRAFTS);
    let gone = call(&t, "maintenance.drafts.discard", json!({"key": format!("form.{}", super::drafts::MAX_DRAFTS + 4)}));
    assert!(gone["draft"].is_object());
    assert_eq!(call(&t, "maintenance.drafts.discard", json!({"key": "nope"}))["draft"], Value::Null);
}

fn exported_logs_never_contain_keys_or_tokens(f: Factory) {
    // A provider key without any recognisable shape: only the exact-secret pass catches it.
    const PROVIDER_KEY: &str = "plainproviderkeyvalue";
    const TOKENS: [&str; 6] = [
        "sk-ant-api03-ZZZZZZZZZZZZZZZZZZZZZZZZZZZZ",
        "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.c2lnbmF0dXJlc2lnbmF0dXJl",
        "0123456789abcdef0123456789abcdef01234567",
        "hunter2hunter2",
        "fclocal-Qx7Lk2Pz9Mw4Rt8Vb3Nc6Hy1Jd5Fg0Se",
    ];
    let t = TestApp::new(f());
    t.app.set_secret_store(Arc::new(MemorySecrets::new()));
    let p = call(&t, "ai.providers.create", json!({"kind": "anthropic"}));
    call(&t, "ai.providers.set_key", json!({"provider_id": p["id"], "key": PROVIDER_KEY}));

    let dir = tempfile::tempdir().unwrap();
    let (_g, start) = RunGuard::begin(dir.path(), "1.0.0", Timestamp(1_000)).unwrap();
    let log = FileLog::open(&super::log_dir(dir.path()), Arc::new(crate::system::SystemClock)).unwrap();
    // Written before the app knew the provider key: lands on disk unmasked by shape rules…
    log.write_line(Level::Error, "fc_ai", &format!("provider said: bad key {PROVIDER_KEY}"));
    log.write_line(Level::Warn, "fc_http", &format!("Authorization: Bearer {}", TOKENS[5]));
    log.write_line(Level::Warn, "fc_app", &format!("api_key={} token={}", TOKENS[0], TOKENS[1]));
    log.write_line(Level::Info, "fc_app", &format!("jwt {} hash {} password: {}", TOKENS[2], TOKENS[3], TOKENS[4]));
    log.flush();
    // …and a crash report with a token in its message.
    write_report(
        &dir.path().join(super::CRASH_DIR),
        &crash_report(Timestamp(1_500), &format!("panicked holding {}", TOKENS[0])),
        &Redactor::new(),
    )
    .unwrap();
    t.app.start_maintenance(dir.path(), Some(log.clone()), start);
    // From now on the app knows the key: new lines are masked when written.
    log.write_line(Level::Error, "fc_ai", &format!("again {PROVIDER_KEY}"));
    log.flush();
    let current = std::fs::read_to_string(log.dir().join(super::logging::LOG_FILE)).unwrap();
    assert!(!current.contains(&format!("again {PROVIDER_KEY}")), "{current}");

    let target = dir.path().join("bug-report.zip");
    let out = call(&t, "maintenance.logs.export", json!({"path": target.display().to_string()}));
    assert!(out["files"].as_u64().unwrap() >= 3, "{out}");
    assert!(out["redactions"].as_u64().unwrap() >= 1, "the early provider key was masked: {out}");
    for secret in std::iter::once(PROVIDER_KEY).chain(TOKENS) {
        assert!(!zip_contains(&target, secret).unwrap(), "{secret} leaked into the bundle");
    }
    assert!(zip_contains(&target, "Flashcard log bundle").unwrap());
    assert!(zip_contains(&target, "provider said: bad key").unwrap(), "the log stays useful");

    // Path checks.
    for bad in ["relative.zip", "/definitely/not/here/x.zip"] {
        let e = call_err(&t, "maintenance.logs.export", json!({"path": bad}));
        assert_eq!(e.code, ErrorCode::ValidationInvalidParams, "{bad}");
    }
    let e = call_err(&t, "maintenance.logs.export", json!({"path": dir.path().join("x.txt").display().to_string()}));
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
}

/// Fake platform updater: offers `version`; the download "verifies" unless `bad_signature`.
#[derive(Default)]
struct FakeUpdater {
    version: Option<String>,
    bad_signature: bool,
    checks: AtomicUsize,
    downloaded: AtomicBool,
    applied: AtomicBool,
}

impl Updater for FakeUpdater {
    fn check(&self) -> Result<Option<UpdateInfoDto>, UpdateError> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        Ok(self.version.clone().map(|version| UpdateInfoDto {
            version,
            notes: Some("Fixes".into()),
            date: None,
        }))
    }

    fn download(
        &self,
        progress: &dyn Fn(u64, Option<u64>),
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<(), UpdateError> {
        progress(50, Some(100));
        if self.bad_signature {
            return Err(UpdateError::Signature("signature mismatch".into()));
        }
        progress(100, Some(100));
        self.downloaded.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn apply(&self) -> Result<(), UpdateError> {
        if !self.downloaded.load(Ordering::SeqCst) {
            return Err(UpdateError::NotReady);
        }
        self.applied.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn updates_are_checked_downloaded_verified_and_applied_only_on_request(f: Factory) {
    let t = TestApp::new(f());
    // No updater (CLI, dev build): unsupported, nothing checked.
    let s = call(&t, "updates.status", json!({}));
    assert_eq!((s["supported"].clone(), s["check_due"].clone()), (json!(false), json!(false)));
    assert_eq!(call_err(&t, "updates.check", json!({})).code, ErrorCode::ProviderUpdateUnsupported);

    let fake = Arc::new(FakeUpdater {
        version: Some("9.9.9".into()),
        ..FakeUpdater::default()
    });
    t.app.set_updater(fake.clone());
    let s = call(&t, "updates.status", json!({}));
    assert_eq!(s["supported"], true);
    assert_eq!(s["check_due"], false, "never checks unasked by default");
    assert_eq!(fake.checks.load(Ordering::SeqCst), 0);
    call(&t, "settings.set", json!({"key": "updates.check_on_start", "scope": {"kind": "app"}, "value": true}));
    assert_eq!(call(&t, "updates.status", json!({}))["check_due"], true);

    assert_eq!(call_err(&t, "updates.apply", json!({})).code, ErrorCode::ConflictUpdateNotReady);
    let checked = call(&t, "updates.check", json!({}));
    assert_eq!(checked["available"]["version"], "9.9.9");
    assert_eq!(checked["check_due"], false);
    let job = call(&t, "updates.install", json!({}));
    assert_eq!(job["kind"], "updates.install");
    assert_eq!(call(&t, "updates.status", json!({}))["phase"], "downloading");
    t.app.run_pending_jobs().unwrap();
    assert_eq!(call(&t, "updates.status", json!({}))["phase"], "ready");
    assert!(!fake.applied.load(Ordering::SeqCst), "nothing installed without the click");
    call(&t, "updates.apply", json!({}));
    assert!(fake.applied.load(Ordering::SeqCst));

    // Up to date: nothing to install.
    let t2 = TestApp::new(f());
    t2.app.set_updater(Arc::new(FakeUpdater::default()));
    assert_eq!(call(&t2, "updates.check", json!({}))["available"], Value::Null);
    assert_eq!(call_err(&t2, "updates.install", json!({})).code, ErrorCode::ConflictUpdateNotAvailable);
}

fn a_badly_signed_update_is_refused(f: Factory) {
    let t = TestApp::new(f());
    let fake = Arc::new(FakeUpdater {
        version: Some("9.9.9".into()),
        bad_signature: true,
        ..FakeUpdater::default()
    });
    t.app.set_updater(fake.clone());
    call(&t, "updates.check", json!({}));
    call(&t, "updates.install", json!({}));
    t.app.run_pending_jobs().unwrap();
    let s = call(&t, "updates.status", json!({}));
    assert_eq!(s["phase"], "idle");
    assert_eq!(s["last_error"]["code"], "PROVIDER_UPDATE_SIGNATURE");
    assert_eq!(call_err(&t, "updates.apply", json!({})).code, ErrorCode::ConflictUpdateNotReady);
    assert!(!fake.applied.load(Ordering::SeqCst));
}

fn no_automated_actor_reaches_maintenance_or_updates(f: Factory) {
    let t = TestApp::new(f());
    let actors = [
        Actor::Plugin("p".into()),
        Actor::Automation("r".into()),
        Actor::LocalApi("c".into()),
        Actor::Mcp("m".into()),
    ];
    for actor in &actors {
        for (method, params) in [
            ("maintenance.logs.export", json!({"path": "/tmp/x.zip"})),
            ("maintenance.status", json!({})),
            ("maintenance.drafts.list", json!({})),
            ("updates.check", json!({})),
            ("updates.apply", json!({})),
        ] {
            let e = t
                .app
                .invoke(&format!("v1.{method}"), params, actor)
                .expect_err(method);
            assert_eq!(e.code, ErrorCode::PermissionDenied, "{actor:?} {method}");
        }
    }
}
