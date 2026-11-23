//! Phase-2 final review fixes, end to end through the API:
//!
//! * the oplog is data: a restored backup starts a fresh history (a restore barrier), and
//!   undo/redo only run inverses whose method is replayable (never job starters / file
//!   writers such as `io.export.start`), whatever the oplog says;
//! * a trash purge (explicit or at startup) writes a history barrier: undo stops there with
//!   `CONFLICT_HISTORY_BARRIER` instead of `NOT_FOUND_*`, older history stays listed;
//! * `media.add` records `{hash, name, mime, size}` in the oplog, not the file;
//! * with the `attributes` module off, notes and imports work without classifications, and
//!   nested calls respect module switches.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::{TempSqlite, TestApp, TickClock};
use crate::{App, AppDeps};
use base64::Engine as _;
use fc_api::v1::attributes::{DefineAttribute, NewAttributeOption};
use fc_api::v1::browse::PurgeTrash;
use fc_api::v1::cardtypes::{AddMedia, MediaRef};
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::io::*;
use fc_api::v1::jobs::{JobDto, JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::{CreateNote, NoteDto, NoteIds};
use fc_api::v1::oplog::{Actor, ListOplog, OplogEntryDto};
use fc_api::{ApiMethod, ErrorCode, Visibility};
use fc_core::attribute::{AttributeKind, EntityRef};
use fc_core::time::MILLIS_PER_DAY;
use fc_core::{BatchId, IdGen, OplogId, V7IdGen};
use fc_storage::repo::{OplogRecord, OplogRecordKind};
use fc_storage::{SqliteStore, Store};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// `TestApp`'s clock start (2026-01-01T00:00Z).
const TODAY: i64 = 1_767_225_600_000;

fn open(db: &Path) -> TestApp {
    TestApp::new(Arc::new(SqliteStore::open(db).unwrap()))
}

/// An app over `store` whose clock starts at `start` (ms), `load_state` done (runs the
/// startup trash purge).
fn app_at(store: Arc<dyn Store>, start: i64) -> App {
    let clock: Arc<dyn fc_core::Clock> = Arc::new(TickClock::new(start));
    let app = App::new(AppDeps {
        store,
        ids: Box::new(V7IdGen::new(
            clock.clone(),
            fc_core::rng::SplitMix64::new(start.unsigned_abs()),
        )),
        clock,
        events: Arc::new(crate::events::InMemoryEventBus::new()),
    })
    .unwrap();
    app.load_state().unwrap();
    app
}

fn finish(t: &TestApp, job: &JobDto) -> JobDto {
    t.app.run_pending_jobs().unwrap();
    t.ok::<JobsGet>(JobRef { id: job.id })
}

fn deck(t: &TestApp, path: &str) -> DeckDto {
    t.ok::<DecksCreate>(CreateDeck {
        path: path.into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    })
}

fn note(t: &TestApp, deck: &DeckDto, front: &str) -> NoteDto {
    t.ok::<NotesCreate>(new_note(deck, front))
}

fn new_note(deck: &DeckDto, front: &str) -> CreateNote {
    CreateNote {
        notetype_id: fc_core::notetype::builtin::CLASSIC_ID,
        deck_id: deck.id,
        fields: BTreeMap::from([
            ("front".to_owned(), front.to_owned()),
            ("back".to_owned(), format!("{front}!")),
        ]),
        tags: Vec::new(),
        attributes: Vec::new(),
        allow_duplicate: None,
    }
}

fn oplog(t: &TestApp) -> Vec<OplogEntryDto> {
    t.ok::<OplogList>(ListOplog {
        page: fc_core::paging::PageRequest {
            cursor: None,
            limit: Some(500),
        },
    })
    .items
}

/// Appends a hand-written `do` batch whose inverse is `inverse` (what a tampered oplog —
/// e.g. from a crafted backup — could hold).
fn append_crafted_batch(store: &dyn Store, inverse: Value) {
    let mut ids = V7IdGen::new(
        Arc::new(fc_core::time::FixedClock::new(fc_core::Timestamp(
            TODAY + 5,
        ))),
        fc_core::rng::SplitMix64::new(7),
    );
    let mut tx = store.write().unwrap();
    tx.append_oplog(&OplogRecord {
        id: OplogId::generate(&mut ids as &mut dyn IdGen),
        batch_id: BatchId::generate(&mut ids as &mut dyn IdGen),
        seq: 0,
        command: "v1.decks.create".into(),
        payload: json!({}),
        inverse,
        actor: json!({"kind": "user"}),
        kind: OplogRecordKind::Do,
        reverts_batch_id: None,
        summary: json!({"key": "oplog.summary.decks.create", "params": {"path": "x"}}),
        created_at: fc_core::Timestamp(TODAY + 5),
    })
    .unwrap();
    tx.commit().unwrap();
}

fn export_call(path: &Path) -> Value {
    json!([{
        "method": "v1.io.export.start",
        "params": {"format": "json", "path": path.to_string_lossy()}
    }])
}

// ------------------------------------------------------------------ #1 replay allow-list

#[test]
fn replayable_methods_are_undoable_commands_without_side_channels() {
    let t = TestApp::mem();
    let registry = t.app.registry();
    for d in registry.methods() {
        if d.replayable {
            assert!(
                d.undoable && d.kind == fc_api::MethodKind::Command,
                "{} is replayable but not an undoable command",
                d.name
            );
        }
    }
    // Job starters, file writers and staged-data consumers never run as stored inverses.
    for name in [
        IoExportStart::NAME,
        IoImportStart::NAME,
        IoBackupCreate::NAME,
        IoBackupRestore::NAME,
        IoImportApply::NAME,
        IoImportDiscard::NAME,
        IoBackupLoad::NAME,
        MediaAdd::NAME,
        BatchExecute::NAME,
        OplogUndo::NAME,
        OplogRedo::NAME,
        BrowsePurgeTrash::NAME,
        JobsCancel::NAME,
    ] {
        assert!(!registry.descriptor(name).unwrap().replayable, "{name}");
    }
    // Every internal inverse method a handler can emit is replayable (else its undo breaks).
    // `ai.usage.record` is a ledger row a job writes, never an inverse; `automation.apply` runs
    // a rule (its nested actions are the undo step), never an inverse either (v0.9).
    for d in registry.methods() {
        if d.visibility == Visibility::Internal
            && ![
                IoImportApply::NAME,
                IoImportDiscard::NAME,
                IoBackupLoad::NAME,
                AiUsageRecord::NAME,
                AutomationApply::NAME,
                // v0.9: HTTP server bookkeeping with empty inverses, never in the oplog.
                LocalApiPair::NAME,
                LocalApiTouch::NAME,
                LocalApiExpirePending::NAME,
                // v1.2: what the HTTP server does for the Subtitle extension: a capture is
                // a do step (its inverses are `subtitle.captures.forget` and the nested
                // ones), uploads and key notes never enter the oplog.
                SubtitleCapturesCreate::NAME,
                SubtitleMediaUpload::NAME,
                SubtitleKeysTouch::NAME,
                // v1.4: capture diagnostics, never in the oplog.
                SubtitleCapturesLog::NAME,
                // v1.4: an automatic meaning suggestion a job writes (no inverse), never an
                // inverse itself.
                AiMeaningStoreSuggestion::NAME,
                // v1.4 W4: media origin (no inverse, never in the oplog) and the rollback
                // of an unfinished audio fill (like `io.import.discard`).
                MediaOriginsRecord::NAME,
                MediaAudioFillRollback::NAME,
            ]
            .contains(&d.name)
        {
            assert!(
                d.replayable,
                "internal inverse {} must be replayable",
                d.name
            );
        }
    }
}

#[test]
fn undo_refuses_stored_inverses_that_are_not_replayable() {
    let dir = tempfile::tempdir().unwrap();
    let evil = dir.path().join("evil.json");
    let t = TestApp::mem();
    let kept = deck(&t, "Kept");
    // A safe step and a malicious one in the same batch: nothing runs.
    let mut inverse = vec![json!({
        "method": "v1.decks.delete",
        "params": {"id": kept.id}
    })];
    inverse.extend(export_call(&evil).as_array().unwrap().iter().cloned());
    append_crafted_batch(&*t.store, Value::Array(inverse));

    let e = t.err::<OplogUndo>(Empty {});
    assert_eq!(e.code, ErrorCode::PermissionDenied, "{e:?}");
    assert_eq!(e.details["reason"], "not_replayable");
    assert_eq!(e.details["method"], "v1.io.export.start");
    t.app.run_pending_jobs().unwrap();
    assert!(!evil.exists(), "no export ran");
    assert!(t.ok::<JobsList>(Empty {}).jobs.is_empty(), "no job queued");
    assert!(
        t.store
            .read()
            .unwrap()
            .get_deck(kept.id)
            .unwrap()
            .unwrap()
            .deleted_at
            .is_none(),
        "the safe step of the refused batch did not run either"
    );
}

#[test]
fn restore_starts_a_fresh_history_behind_a_barrier() {
    let dir = tempfile::tempdir().unwrap();
    let evil = dir.path().join("evil.json");
    let db = dir.path().join("a").join("collection.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let t = open(&db);
    t.app.set_data_dir(dir.path().join("app-data"));
    let d = deck(&t, "Lang");
    note(&t, &d, "ev");
    // The crafted backup: its newest undo step would start an export to `evil`.
    append_crafted_batch(&*t.store, export_call(&evil));
    let backup = dir.path().join("crafted.fcbackup");
    let job = t.ok::<IoBackupCreate>(CreateBackup {
        path: backup.to_string_lossy().into_owned(),
    });
    assert_eq!(finish(&t, &job).status, JobStatus::Succeeded);
    drop(t);

    let db2 = dir.path().join("b").join("collection.db");
    std::fs::create_dir_all(db2.parent().unwrap()).unwrap();
    let fresh = open(&db2);
    fresh.app.set_data_dir(dir.path().join("app-data"));
    let job = fresh.ok::<IoBackupRestore>(RestoreBackup {
        path: backup.to_string_lossy().into_owned(),
        safety_backup_path: Some(
            dir.path()
                .join("safety.fcbackup")
                .to_string_lossy()
                .into_owned(),
        ),
    });
    assert_eq!(finish(&fresh, &job).status, JobStatus::Succeeded);
    assert_eq!(
        fresh
            .ok::<NotesList>(fc_api::v1::notes::ListNotes {
                deck_id: None,
                include_subdecks: None,
                page: Default::default(),
            })
            .items
            .len(),
        1
    );

    // The backup's oplog is gone; only the restore barrier is there.
    let log = oplog(&fresh);
    assert_eq!(log.len(), 1, "{log:?}");
    assert_eq!(log[0].command, "v1.oplog.barrier");
    assert_eq!(log[0].payload["reason"], "restore");
    assert_eq!(log[0].summary.key, "oplog.summary.oplog.barrier.restore");
    let e = fresh.err::<OplogUndo>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictHistoryBarrier);
    assert_eq!(e.message_key, "errors.conflict.history_barrier");
    assert_eq!(e.details["reason"], "restore");
    assert!(!fresh.ok::<OplogStatus>(Empty {}).can_undo);
    fresh.app.run_pending_jobs().unwrap();
    assert!(!evil.exists(), "the crafted inverse never ran");

    // Work after the restore undoes normally, up to the barrier.
    let d2 = deck(&fresh, "After");
    fresh.ok::<OplogUndo>(Empty {});
    assert!(fresh
        .store
        .read()
        .unwrap()
        .get_deck(d2.id)
        .unwrap()
        .unwrap()
        .deleted_at
        .is_some());
    assert_eq!(
        fresh.err::<OplogUndo>(Empty {}).code,
        ErrorCode::ConflictHistoryBarrier
    );
    fresh.ok::<OplogRedo>(Empty {});
}

// ------------------------------------------------------------------ #2 purge barrier

#[test]
fn explicit_purge_barrier_keeps_history_listed_but_not_undoable() {
    for t in [TestApp::mem(), TestApp::new(Arc::new(TempSqlite::new()))] {
        let d = deck(&t, "Lang");
        let a = note(&t, &d, "a");
        note(&t, &d, "b");
        t.ok::<NotesDelete>(NoteIds { ids: vec![a.id] });
        let r = t.ok::<BrowsePurgeTrash>(PurgeTrash {
            older_than_days: Some(0),
        });
        assert_eq!(r.notes, 1);
        let e = t.err::<OplogUndo>(Empty {});
        assert_eq!(
            e.code,
            ErrorCode::ConflictHistoryBarrier,
            "not NOT_FOUND_NOTE: {e:?}"
        );
        assert_eq!(e.details["reason"], "purge");
        let log = oplog(&t);
        assert_eq!(log[0].command, "v1.oplog.barrier");
        assert_eq!(log[0].summary.params["count"], "1");
        assert!(log.iter().any(|e| e.command == "v1.notes.delete"));
        assert_eq!(
            log.iter()
                .filter(|e| e.command == "v1.notes.create")
                .count(),
            2
        );
    }
}

#[test]
fn startup_purge_writes_a_barrier_too() {
    let file = TempSqlite::new();
    let path = file.path();
    {
        let t = open(&path);
        let d = deck(&t, "Lang");
        let a = note(&t, &d, "a");
        t.ok::<NotesDelete>(NoteIds { ids: vec![a.id] });
    }
    // 31 days later the startup purge deletes the note for good.
    let app = app_at(
        Arc::new(SqliteStore::open(&path).unwrap()),
        TODAY + 31 * MILLIS_PER_DAY,
    );
    let e = app.call::<OplogUndo>(&Empty {}, &Actor::User).unwrap_err();
    assert_eq!(e.code, ErrorCode::ConflictHistoryBarrier, "{e:?}");
    assert_eq!(e.details["reason"], "purge");
    let log = app
        .call::<OplogList>(&ListOplog::default(), &Actor::User)
        .unwrap()
        .items;
    assert_eq!(log[0].command, "v1.oplog.barrier");
    assert!(log.iter().any(|e| e.command == "v1.notes.delete"));
    drop(file);
}

// ------------------------------------------------------------------ #3 media payload

#[test]
fn media_add_oplog_row_stays_small_and_undo_redo_use_the_store() {
    let t = TestApp::new(Arc::new(TempSqlite::new()));
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.resize(2 * 1024 * 1024, 7);
    let m = t.ok::<MediaAdd>(AddMedia {
        name: Some("big.png".into()),
        data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
    });
    assert!(m.created);
    let log = oplog(&t);
    let entry = log.iter().find(|e| e.command == "v1.media.add").unwrap();
    let row = serde_json::to_string(entry).unwrap();
    assert!(row.len() < 2048, "oplog row is {} bytes", row.len());
    assert_eq!(
        entry.payload,
        json!({"hash": m.hash, "name": "big.png", "mime": "image/png", "size": bytes.len()})
    );
    // And the raw stored payload too.
    let stored: i64 = {
        let r = t.store.read().unwrap();
        let rows = r.list_oplog(&Default::default()).unwrap().items;
        rows.iter()
            .map(|r| r.payload.to_string().len() as i64 + r.inverse.to_string().len() as i64)
            .max()
            .unwrap()
    };
    assert!(stored < 2048, "{stored}");

    let get = || {
        t.app.call::<MediaGet>(
            &MediaRef {
                hash: m.hash.clone(),
            },
            &Actor::User,
        )
    };
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(get().unwrap_err().code, ErrorCode::NotFoundMedia);
    t.ok::<OplogRedo>(Empty {});
    let back = get().unwrap();
    assert_eq!(
        (back.size, back.mime.as_str()),
        (bytes.len() as u64, "image/png")
    );
    let content = t.ok::<MediaRead>(MediaRef {
        hash: m.hash.clone(),
    });
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(content.data_base64)
            .unwrap(),
        bytes
    );
}

// ------------------------------------------------------------------ #7 attributes off

fn required_level(t: &TestApp, d: &DeckDto) -> fc_api::v1::attributes::AttributeDefDto {
    t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(d.id),
        key: Some("level".into()),
        name: "Level".into(),
        kind: AttributeKind::Enum,
        target: None,
        required: Some(true),
        options: vec![NewAttributeOption {
            label: "A1".into(),
            color: None,
        }],
    })
}

fn set_attributes_module(t: &TestApp, on: bool) {
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "attributes".into(),
        enabled: on,
    });
}

#[test]
fn notes_work_without_classifications_when_the_module_is_off() {
    let t = TestApp::mem();
    let d = deck(&t, "Lang");
    let level = required_level(&t, &d);
    assert_eq!(
        t.err::<NotesCreate>(new_note(&d, "x")).code,
        ErrorCode::ValidationAttributeRequired
    );
    set_attributes_module(&t, false);
    // Required classification is hidden: not asked for, given values ignored.
    let n = note(&t, &d, "x");
    let mut with_value = new_note(&d, "y");
    with_value.attributes = vec![fc_api::v1::attributes::AttributeAssignment {
        attr_id: level.id,
        value: Some(fc_core::attribute::AttributeValue::Choice(
            level.options[0].id,
        )),
    }];
    let y = t.ok::<NotesCreate>(with_value);
    let reads = t.store.read().unwrap();
    assert!(reads
        .get_attribute_values(EntityRef::Note(n.id))
        .unwrap()
        .is_empty());
    assert!(reads
        .get_attribute_values(EntityRef::Note(y.id))
        .unwrap()
        .is_empty());
    drop(reads);
    // Undo/redo of the notes still work.
    t.ok::<OplogUndo>(Empty {});
    t.ok::<OplogRedo>(Empty {});
}

#[test]
fn nested_calls_respect_module_switches() {
    let t = TestApp::mem();
    let d = deck(&t, "Lang");
    let level = required_level(&t, &d);
    t.ok::<AttributesOptionAdd>(fc_api::v1::attributes::AddAttributeOption {
        attr_id: level.id,
        label: "B1".into(),
        color: None,
        after: None,
        id: None,
    });
    // Switch the module off without an undo step: the option add stays the undo target,
    // and its inverse is an `attributes.*` call.
    {
        let mut tx = t.store.write().unwrap();
        tx.set_module_enabled("attributes", false, fc_core::Timestamp(TODAY))
            .unwrap();
        tx.commit().unwrap();
    }
    t.app.reload_cached_state().unwrap();
    let e = t.err::<OplogUndo>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled, "{e:?}");
    assert_eq!(e.details["module"], "attributes");
}

#[test]
fn imports_skip_classifications_when_the_module_is_off() {
    let t = TestApp::mem();
    let d = deck(&t, "Lang");
    let level = required_level(&t, &d);
    set_attributes_module(&t, false);
    let source = ImportSource::Text {
        text: "front;back;level\nev;house;A1\nrüya;dream;B2".into(),
    };
    let preview = t.ok::<IoImportPreview>(PreviewImport {
        source: source.clone(),
        format: None,
        text_options: None,
        notetype_id: None,
        deck_id: Some(d.id),
        limit: None,
    });
    let suggested = &preview.suggested_columns;
    assert!(
        !suggested
            .iter()
            .any(|c| matches!(c, ColumnTarget::Attribute { .. })),
        "{suggested:?}"
    );
    let job = t.ok::<IoImportStart>(StartImport {
        source,
        format: None,
        text_options: None,
        deck_id: Some(d.id),
        notetype_id: None,
        columns: Some(vec![
            ColumnTarget::Field {
                key: "front".into(),
            },
            ColumnTarget::Field { key: "back".into() },
            ColumnTarget::Attribute { attr_id: level.id },
        ]),
        skip_first_row: Some(true),
        duplicates: None,
        create_missing_options: Some(true),
        include_history: None,
    });
    let job = finish(&t, &job);
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let report: ImportReport = serde_json::from_value(job.result.unwrap()).unwrap();
    assert_eq!(
        (report.added_notes, report.added_options),
        (2, 0),
        "{report:?}"
    );
    assert!(report.issues.is_empty(), "{:?}", report.issues);
}

#[test]
fn palette_off_keeps_shortcut_bindings() {
    let t = TestApp::mem();
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "palette".into(),
        enabled: false,
    });
    assert_eq!(
        t.err::<PaletteListMethod>(Empty {}).code,
        ErrorCode::ConflictModuleDisabled
    );
    let list = t.ok::<ShortcutsList>(Empty {});
    assert!(
        list.items.iter().any(|s| s.action == "ui.palette"),
        "defaults still served"
    );
}
