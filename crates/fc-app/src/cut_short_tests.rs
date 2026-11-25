//! v1.4.2 K1: a command whose statement is cut short, at the application layer.
//!
//! Review finding 5 was fixed and tested in the storage layer: when SQLite ends a transaction
//! by itself (full disk, deadline, I/O error) the handle takes no more writes and `commit`
//! answers `RolledBack`. Here the same from the user's side, through [`App::invoke`] on a
//! real SQLite collection: the disk "fills up" (SQLite's own `max_page_count`, so the
//! failure is SQLite's real "database or disk is full") at every point of a command in turn.
//! Each time the command must fail **as a whole** — no row, no oplog entry, no file — with
//! the disk-full error, and the app must go on working.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::app::AppDeps;
use crate::App;
use base64::Engine as _;
use fc_api::v1::oplog::Actor;
use fc_api::{ApiError, ErrorCode};
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_storage::crash_points;
use fc_storage::repo::CollectionDump;
use fc_storage::SqliteStore;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

fn open(dir: &Path) -> (Arc<SqliteStore>, App) {
    let db = dir.join("collection.db");
    let store = Arc::new(SqliteStore::open(&db).unwrap());
    let app = App::new(AppDeps::with_store(store.clone())).unwrap();
    app.set_data_dir(dir.to_path_buf());
    app.load_state().unwrap();
    (store, app)
}

fn try_call(app: &App, method: &str, params: Value) -> Result<Value, ApiError> {
    app.invoke(&format!("v1.{method}"), params, &Actor::User)
}

fn call(app: &App, method: &str, params: Value) -> Value {
    try_call(app, method, params).unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

fn dump(app: &App) -> CollectionDump {
    app.store().read().unwrap().dump_collection().unwrap()
}

fn media_files(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                walk(&e.path(), out);
            } else {
                out.push(e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    let mut out = Vec::new();
    walk(&dir.join("media"), &mut out);
    out.sort();
    out
}

fn note_params(deck: &Value, front: &str) -> Value {
    json!({"notetype_id": CLASSIC_ID, "deck_id": deck,
           "fields": {"front": front, "back": "x".repeat(9_000)},
           "tags": ["dolu", "disk"], "attributes": [], "allow_duplicate": true})
}

/// Runs `method` with the disk filling up after 0, 1, 2, … more pages until it goes through.
/// Returns how often it failed.
fn sweep(store: &SqliteStore, app: &App, dir: &Path, method: &str, params: &Value) -> usize {
    let mut failures = 0;
    for extra in 0..400 {
        let before = dump(app);
        let files = media_files(dir);
        crash_points::allow_pages(store, Some(extra)).unwrap();
        let result = try_call(app, method, params.clone());
        crash_points::allow_pages(store, None).unwrap();
        match result {
            Err(e) => {
                assert_eq!(
                    e.code,
                    ErrorCode::IoDiskFull,
                    "{method} at +{extra} pages: {e:?}"
                );
                assert_eq!(e.message_key, "errors.io.disk_full");
                assert!(
                    dump(app) == before,
                    "{method} at +{extra} pages failed but left something written"
                );
                // A media file written before the failure has no row; the next start
                // removes it. No partial file stays.
                assert!(
                    media_files(dir).iter().all(|f| !f.ends_with(".part")),
                    "{method} at +{extra} pages left a partial media file"
                );
                let _ = files;
                failures += 1;
            }
            Ok(_) => {
                assert!(dump(app) != before, "{method} succeeded and wrote nothing");
                return failures;
            }
        }
    }
    panic!("{method} never went through");
}

#[test]
fn a_full_disk_at_any_point_of_a_command_fails_it_whole_with_the_disk_full_error() {
    let root = tempfile::tempdir().unwrap();
    let (store, app) = open(root.path());
    let deck = call(&app, "decks.create", json!({"path": "Dolu"}))["id"].clone();
    call(&app, "notes.create", note_params(&deck, "first"));

    // One note: rows in notes, cards, tags, the search index, the word index, the oplog.
    let failed = sweep(
        &store,
        &app,
        root.path(),
        "notes.create",
        &note_params(&deck, "second"),
    );
    assert!(
        failed >= 2,
        "the disk filled up at several points ({failed})"
    );

    // Several commands in one transaction: none of the earlier ones may stay.
    let batch = json!({"calls": (0..4).map(|i| json!({
        "method": "v1.notes.create",
        "params": note_params(&deck, &format!("batch {i}")),
    })).collect::<Vec<_>>()});
    let failed = sweep(&store, &app, root.path(), "batch.execute", &batch);
    assert!(
        failed >= 4,
        "the disk filled up inside the batch ({failed})"
    );

    // A media file and its row.
    let png = {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        bytes.extend((0..40_000u32).flat_map(|i| i.wrapping_mul(2_654_435_761).to_le_bytes()));
        base64::engine::general_purpose::STANDARD.encode(bytes)
    };
    sweep(
        &store,
        &app,
        root.path(),
        "media.add",
        &json!({"name": "a.png", "data_base64": png}),
    );

    // An edit of an existing note (old rows replaced, an inverse recorded).
    let id = call(&app, "notes.create", note_params(&deck, "edit me"))["id"].clone();
    let edit = json!({"id": id, "fields": {"front": "edited", "back": "y".repeat(30_000)},
                      "tags": ["yeni"], "attributes": []});
    let failed = sweep(&store, &app, root.path(), "notes.update", &edit);
    assert!(failed >= 1, "the disk filled up inside the edit ({failed})");

    // The app went on working all along; what was acknowledged can be undone and redone.
    let status = call(&app, "oplog.status", json!({}));
    assert_eq!(status["can_undo"], true);
    let before_undo = dump(&app);
    call(&app, "oplog.undo", json!({}));
    call(&app, "oplog.redo", json!({}));
    let _ = before_undo;
    let check = call(&app, "browse.check_database", json!({}));
    assert_eq!(check["healthy"], true, "{check}");
}

/// The capture hook of the AI module runs a nested command and tolerates its refusals
/// ("not set up", "over budget"). A storage failure is not a refusal: swallowed, the capture
/// would go on inside a transaction that may be gone, and report the wrong error.
#[test]
fn the_capture_hook_passes_a_storage_failure_on() {
    let root = tempfile::tempdir().unwrap();
    let (store, app) = open(root.path());
    let deck = call(&app, "decks.create", json!({"path": "Hook"}))["id"].clone();
    let note = call(&app, "notes.create", note_params(&deck, "word"));
    let note_id: fc_core::NoteId = serde_json::from_value(note["id"].clone()).unwrap();

    let run = |expect_refusal: bool| {
        let mut reg = crate::Registry::new();
        crate::modules::register_all(&mut reg);
        let mut tx = app.store().write().unwrap();
        let clock = Arc::new(fc_core::time::FixedClock::new(fc_core::Timestamp(5)));
        let mut ids = fc_core::V7IdGen::new(clock, crate::system::OsRng::new());
        let mut ctx = crate::CommandCtx {
            tx: &mut *tx,
            now: fc_core::Timestamp(5),
            ids: &mut ids,
            actor: &Actor::System,
            locale: fc_core::i18n::Locale::En,
            batch_id: fc_core::BatchId::from_uuid(fc_core::id::uuid_v7_from_parts(
                fc_core::Timestamp(2),
                &[8; 10],
            )),
            batch_kind: fc_api::v1::oplog::OplogKind::Do,
            reverts_batch_id: None,
            registry: &reg,
            oplog: Vec::new(),
            events: Vec::new(),
            check_modules: true,
        };
        let out = crate::modules::ai::suggest_after_capture(&mut ctx, note_id);
        if expect_refusal {
            // Not opted in: nothing queued, and that is not an error.
            assert!(matches!(out, Ok(None)), "{out:?}");
        } else {
            let e = out.expect_err("a storage failure is not swallowed");
            assert!(matches!(e, crate::AppError::Storage(_)), "{e:?}");
        }
    };
    run(true);
    // The table the hook reads first cannot be read any more.
    crash_points::run_sql(
        &store,
        "ALTER TABLE module_state RENAME TO module_state_gone",
    )
    .unwrap();
    run(false);
    crash_points::run_sql(
        &store,
        "ALTER TABLE module_state_gone RENAME TO module_state",
    )
    .unwrap();
    run(true);
}
