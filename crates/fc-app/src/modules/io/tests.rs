//! v0.5 acceptance (docs/04): text import with auto-detection, lossless Anki import and a
//! re-importable `.apkg` export, backup → delete → restore, cancellable imports that undo in
//! one step, and classification handling.

use super::import::Staged;
use super::rows::{AttrRef, RowSpec, RowsImport};
use crate::jobs::CancelToken;
use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::attributes::{DefineAttribute, NewAttributeOption};
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto, DeckRef};
use fc_api::v1::io::*;
use fc_api::v1::jobs::{JobDto, JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::notes::CreateNote;
use fc_api::ErrorCode;
use fc_core::attribute::{AttributeKind, AttributeValue, EntityRef};
use fc_core::card::{CardState, Rating};
use fc_core::note::Note;
use fc_core::notetype::{builtin::CLASSIC_ID, FieldKey, NoteTypeKind};
use fc_core::{DeckId, JobId};
use fc_interop::anki::fixtures as anki_fx;
use fc_interop::media::{media_url, sha256_hex};
use fc_storage::repo::{CardFilter, NoteFilter};
use fc_storage::Store;
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use std::sync::Arc;

fn sqlite() -> TestApp {
    TestApp::new(Arc::new(TempSqlite::new()))
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

fn text_import(text: &str, deck: DeckId) -> StartImport {
    StartImport {
        source: ImportSource::Text { text: text.into() },
        format: None,
        text_options: None,
        deck_id: Some(deck),
        notetype_id: None,
        columns: None,
        skip_first_row: None,
        duplicates: None,
        create_missing_options: None,
        include_history: None,
    }
}

fn file_import(path: &std::path::Path, parent: Option<DeckId>) -> StartImport {
    StartImport {
        source: ImportSource::File {
            path: path.to_string_lossy().into_owned(),
        },
        deck_id: parent,
        ..text_import("", DeckId::from_uuid(uuid::Uuid::nil()))
    }
}

/// Runs queued jobs and returns the job.
fn finish(t: &TestApp, job: &JobDto) -> JobDto {
    t.app.run_pending_jobs().unwrap();
    t.ok::<JobsGet>(JobRef { id: job.id })
}

fn result<T: DeserializeOwned>(job: &JobDto) -> T {
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    serde_json::from_value(job.result.clone().unwrap()).unwrap()
}

fn import(t: &TestApp, req: StartImport) -> ImportReport {
    let job = t.ok::<IoImportStart>(req);
    assert_eq!(job.status, JobStatus::Queued);
    result(&finish(t, &job))
}

fn live_notes(store: &Arc<dyn Store>) -> Vec<Note> {
    let tx = store.read().unwrap();
    let mut notes: Vec<Note> = tx
        .list_notes(
            &NoteFilter::default(),
            &fc_core::paging::PageRequest {
                cursor: None,
                limit: Some(500),
            },
        )
        .unwrap()
        .items
        .into_iter()
        .map(|r| r.note)
        .collect();
    notes.sort_by(|a, b| a.sort_field.cmp(&b.sort_field));
    notes
}

fn fields(n: &Note) -> (String, String) {
    let get = |k: &str| n.fields.get(&FieldKey::new(k)).cloned().unwrap_or_default();
    (get("front"), get("back"))
}

// ------------------------------------------------------------------ text

#[test]
fn text_import_parses_acceptance_examples_with_auto_detection() {
    let t = TestApp::mem();
    let d = deck(&t, "Words");
    for (text, expected) in [
        ("Arda, porsuk", ("Arda", "porsuk")),
        ("Ali: kedi", ("Ali", "kedi")),
        ("Veli >> köpek\nCan >> kuş", ("Veli", "köpek")),
        ("\"Ayşe, Fatma\": ikizler", ("Ayşe, Fatma", "ikizler")),
    ] {
        let r = import(&t, text_import(text, d.id));
        assert!(r.added_notes >= 1, "{text}: {r:?}");
        assert!(live_notes(&t.store)
            .iter()
            .any(|n| fields(n) == (expected.0.into(), expected.1.into())));
    }
    // Custom separator given explicitly.
    let mut req = text_import("kalem ~ pen\nsilgi ~ eraser", d.id);
    req.text_options = Some(TextOptionsDto {
        field_separator: "~".into(),
        record_separator: "\n".into(),
        quotes: true,
        max_fields: None,
    });
    assert_eq!(import(&t, req).added_notes, 2);
    let notes = live_notes(&t.store);
    assert!(notes.iter().any(|n| fields(n) == ("silgi".into(), "eraser".into())));
    // Each note got its card in the deck.
    let tx = t.store.read().unwrap();
    let cards = tx
        .list_cards(&CardFilter {
            deck_ids: Some(vec![d.id]),
            ..CardFilter::default()
        })
        .unwrap();
    assert_eq!(cards.len(), notes.len());
}

#[test]
fn preview_detects_and_suggests_header_mapping() {
    let t = TestApp::mem();
    let d = deck(&t, "Lang");
    let level = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(d.id),
        key: Some("level".into()),
        name: "Seviye".into(),
        kind: AttributeKind::OrderedEnum,
        target: None,
        required: None,
        options: vec![NewAttributeOption {
            label: "A1".into(),
            color: None,
        }],
    });
    let p = t.ok::<IoImportPreview>(PreviewImport {
        source: ImportSource::Text {
            text: "Front;Back;Seviye;tags\nkedi;cat;A1;animal\nköpek;dog;A2;animal".into(),
        },
        format: None,
        text_options: None,
        notetype_id: None,
        deck_id: Some(d.id),
        limit: Some(1),
    });
    assert_eq!(p.format, ImportFormat::Text);
    assert_eq!(p.text_options.unwrap().field_separator, ";");
    assert_eq!((p.columns, p.total_rows, p.rows.len()), (4, 3, 1));
    assert_eq!(
        p.suggested_columns,
        vec![
            ColumnTarget::Field { key: "front".into() },
            ColumnTarget::Field { key: "back".into() },
            ColumnTarget::Attribute { attr_id: level.id },
            ColumnTarget::Tags,
        ]
    );
}

#[test]
fn text_columns_map_to_classifications_and_create_missing_options() {
    let t = TestApp::mem();
    let d = deck(&t, "Lang");
    let level = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(d.id),
        key: Some("level".into()),
        name: "Seviye".into(),
        kind: AttributeKind::OrderedEnum,
        target: None,
        required: None,
        options: vec![NewAttributeOption {
            label: "A1".into(),
            color: None,
        }],
    });
    let columns = Some(vec![
        ColumnTarget::Field { key: "front".into() },
        ColumnTarget::Field { key: "back".into() },
        ColumnTarget::Attribute { attr_id: level.id },
        ColumnTarget::Tags,
    ]);
    let text = "front,back,level,tags\nkedi,cat,a1,animal\nhayal,dream,B2,abstract";
    // Without create_missing_options the unknown label is reported, the note still imported.
    let mut req = text_import(text, d.id);
    req.columns = columns.clone();
    req.skip_first_row = Some(true);
    let r = import(&t, req.clone());
    assert_eq!(r.added_notes, 2);
    assert_eq!(r.issues.len(), 1);
    assert_eq!(r.issues[0].row, 3);
    assert_eq!(r.issues[0].message.key, "io.issue.attribute_unknown_option");
    // With it, B2 is created; duplicates are updated instead of skipped.
    req.create_missing_options = Some(true);
    req.duplicates = Some(DuplicatePolicy::Update);
    let r = import(&t, req);
    assert_eq!((r.added_notes, r.updated_notes, r.added_options), (0, 2, 1));
    let def = t.ok::<AttributesGet>(fc_api::v1::attributes::AttributeRef { id: level.id });
    let b2 = def.options.iter().find(|o| o.label == "B2").unwrap().id;
    let hayal = live_notes(&t.store)
        .into_iter()
        .find(|n| fields(n).0 == "hayal")
        .unwrap();
    let tx = t.store.read().unwrap();
    let values = tx.get_attribute_values(EntityRef::Note(hayal.id)).unwrap();
    assert_eq!(values, vec![(level.id, AttributeValue::Choice(b2))]);
    assert_eq!(hayal.tags[0].as_str(), "abstract");
}

#[test]
fn json_import_creates_decks_and_resolves_names() {
    let t = TestApp::mem();
    let json = r#"{"format":"fc-json","version":1,"notes":[
        {"deck":"Dil::İngilizce","fields":{"Front":"dog","back":"köpek"},"tags":["a"]},
        {"deck":"Dil::Almanca","notetype":"Classic","fields":{"front":"Hund","back":"köpek"}},
        {"fields":{"front":"orphan"}}
    ]}"#;
    let mut req = text_import(json, DeckId::from_uuid(uuid::Uuid::nil()));
    req.deck_id = None;
    req.format = Some(ImportFormat::Json);
    let r = import(&t, req);
    assert_eq!(r.added_notes, 2);
    assert_eq!(r.added_decks, 3);
    assert_eq!(r.issues[0].message.key, "io.issue.no_deck");
    let tree = t.ok::<DecksList>(Empty {});
    let paths: Vec<_> = tree.decks.iter().map(|d| d.path.clone()).collect();
    assert!(paths.contains(&"Dil::İngilizce".to_string()));
}

// ------------------------------------------------------------------ cancel + undo

#[test]
fn import_is_one_undo_step_and_redoable() {
    // One chunk, and one transaction per row: the undo step is the same.
    for chunk in [super::apply::CHUNK_ROWS, 1] {
        import_is_one_undo_step_and_redoable_with(TestApp::mem(), chunk);
        import_is_one_undo_step_and_redoable_with(sqlite(), chunk);
    }
}

/// Runs an import job with `chunk` units per transaction and an optional hook after each
/// committed chunk.
fn import_chunked(
    t: &TestApp,
    req: StartImport,
    chunk: usize,
    after_chunk: impl Fn(usize, &CancelToken) + Send + Sync + 'static,
) -> JobDto {
    let job = t.ok::<IoImportStart>(req);
    super::import::knobs::set(
        job.id,
        super::import::knobs::Knobs {
            chunk_rows: chunk,
            after_chunk: Box::new(after_chunk),
        },
    );
    finish(t, &job)
}

fn import_is_one_undo_step_and_redoable_with(t: TestApp, chunk: usize) {
    let d = deck(&t, "Base");
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: CLASSIC_ID,
        deck_id: d.id,
        fields: [("front".into(), "keep".into()), ("back".into(), "me".into())].into(),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    });
    let level = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: None,
        key: Some("level".into()),
        name: "Level".into(),
        kind: AttributeKind::Enum,
        target: None,
        required: None,
        options: vec![NewAttributeOption {
            label: "Z".into(),
            color: None,
        }],
    });
    let before = live_notes(&t.store);
    let json = r#"[
        {"deck":"New::Sub","fields":{"front":"a","back":"1"},"attributes":{"level":"X"}},
        {"deck":"New::Sub","fields":{"front":"b","back":"2"},"attributes":{"Level":"Y"}},
        {"deck":"Base","fields":{"front":"keep","back":"updated"}}
    ]"#;
    let mut req = text_import(json, d.id);
    req.format = Some(ImportFormat::Json);
    req.create_missing_options = Some(true);
    req.duplicates = Some(DuplicatePolicy::Update);
    let batches = |t: &TestApp| -> std::collections::HashSet<_> {
        t.ok::<OplogList>(Default::default())
            .items
            .iter()
            .map(|e| e.batch_id)
            .collect()
    };
    let batches_before = batches(&t);
    let r: ImportReport = result(&import_chunked(&t, req, chunk, |_, _| {}));
    assert_eq!((r.added_notes, r.updated_notes, r.added_options, r.added_decks), (2, 1, 2, 2));
    assert_eq!(
        batches(&t).difference(&batches_before).count(),
        1,
        "one undo step whatever the chunking"
    );
    let status = t.ok::<OplogStatus>(Empty {});
    assert_eq!(
        status.undo_summary.unwrap().params.get("count"),
        Some(&"2".to_string()),
        "the step is summarized with the import's total"
    );
    let after = live_notes(&t.store);
    assert_eq!(after.len(), 3);

    // One undo removes everything the import did.
    t.ok::<OplogUndo>(Empty {});
    let content = |notes: &[Note]| {
        notes
            .iter()
            .map(|n| (n.id, n.fields.clone(), n.tags.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(content(&live_notes(&t.store)), content(&before));
    let paths: Vec<_> = t.ok::<DecksList>(Empty {}).decks.into_iter().map(|d| d.path).collect();
    assert_eq!(paths, vec!["Base".to_string()]);
    let def = t.ok::<AttributesGet>(fc_api::v1::attributes::AttributeRef { id: level.id });
    assert_eq!(def.options.len(), 1);

    // Redo brings it back exactly.
    t.ok::<OplogRedo>(Empty {});
    let redone = live_notes(&t.store);
    assert_eq!(
        redone.iter().map(fields).collect::<Vec<_>>(),
        after.iter().map(fields).collect::<Vec<_>>()
    );
    let def = t.ok::<AttributesGet>(fc_api::v1::attributes::AttributeRef { id: level.id });
    assert_eq!(def.options.len(), 3);
    let a = redone.iter().find(|n| fields(n).0 == "a").unwrap();
    let tx = t.store.read().unwrap();
    assert_eq!(tx.get_attribute_values(EntityRef::Note(a.id)).unwrap().len(), 1);
}

fn staged_rows(deck: DeckId, n: usize) -> Staged {
    Staged::Rows(RowsImport {
        rows: (0..n)
            .map(|i| RowSpec {
                row: i as u32 + 1,
                deck: Some(format!("Cancelled::Deck{}", i % 3)),
                notetype: None,
                fields: vec![("front".into(), format!("q{i}")), ("back".into(), "a".into())],
                tags: vec!["t".into()],
                attrs: vec![(AttrRef::Name("level".into()), format!("L{i}"))],
            })
            .collect(),
        deck_id: Some(deck),
        notetype_id: CLASSIC_ID,
        duplicates: DuplicatePolicy::Skip,
        create_missing_options: true,
    })
}

#[test]
fn cancelling_in_the_middle_of_a_chunk_rolls_the_chunk_back() {
    let t = TestApp::mem();
    let d = deck(&t, "Base");
    t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: None,
        key: Some("level".into()),
        name: "Level".into(),
        kind: AttributeKind::Enum,
        target: None,
        required: None,
        options: vec![NewAttributeOption {
            label: "Z".into(),
            color: None,
        }],
    });
    let staging = super::Staging::default();
    let job = JobId::from_uuid(fc_core::id::uuid_v7_from_parts(fc_core::Timestamp(1), &[7; 10]));
    // The third cancel check happens after 64 rows (decks, notes, options) were written.
    staging.put_import(
        job,
        super::apply::ApplyState::new(staged_rows(d.id, 200), super::apply::CHUNK_ROWS),
        CancelToken::cancelling_at_check(3),
    );
    let staging = Arc::new(staging);
    let mut reg = crate::Registry::new();
    crate::modules::register_all(&mut reg);
    let handler = super::apply::ApplyHandler(staging);
    let oplog_before = t.ok::<OplogList>(Default::default()).items.len();
    let mut tx = t.store.write().unwrap();
    let mut ids = fc_core::V7IdGen::new(
        Arc::new(fc_core::time::FixedClock::new(fc_core::Timestamp(5))),
        crate::system::OsRng::new(),
    );
    let mut ctx = crate::CommandCtx {
        tx: &mut *tx,
        now: fc_core::Timestamp(5),
        ids: &mut ids,
        actor: &fc_api::v1::oplog::Actor::System,
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
    let e = crate::CommandHandler::<IoImportApply>::execute(&handler, StagedJob { job_id: job }, &mut ctx)
        .unwrap_err();
    assert_eq!(e.into_api().code, ErrorCode::ConflictJobCancelled);
    // Rows had been written in this transaction before the cancel...
    assert!(ctx.reads().list_decks(false).unwrap().len() > 1);
    drop(ctx);
    drop(tx); // ...and the dispatcher's rollback discards them.
    assert_eq!(live_notes(&t.store).len(), 0);
    assert_eq!(t.ok::<DecksList>(Empty {}).decks.len(), 1);
    assert_eq!(t.ok::<OplogList>(Default::default()).items.len(), oplog_before);
}


/// Everything but the `jobs` table (whose row records the job itself).
/// Every table but `jobs` (the backup/restore jobs themselves) and `oplog` (a restore starts
/// a fresh history: see [`assert_fresh_history`]).
fn collection(t: &TestApp) -> fc_storage::repo::CollectionDump {
    let mut d = t.store.read().unwrap().dump_collection().unwrap();
    d.tables.retain(|t| t.name != "jobs" && t.name != "oplog");
    d
}

/// After a restore the oplog holds exactly one restore barrier (the backup's history is
/// never replayed).
pub(crate) fn assert_fresh_history(t: &TestApp) {
    let rows = t
        .store
        .read()
        .unwrap()
        .list_oplog(&Default::default())
        .unwrap()
        .items;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].command, fc_storage::repo::OPLOG_BARRIER_COMMAND);
    assert_eq!(rows[0].payload["reason"], "restore");
}

fn level_attribute(t: &TestApp) {
    t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: None,
        key: Some("level".into()),
        name: "Level".into(),
        kind: AttributeKind::Enum,
        target: None,
        required: None,
        options: vec![NewAttributeOption {
            label: "Z".into(),
            color: None,
        }],
    });
}

/// JSON rows: decks, new tags, unknown classification labels, one update of `keep`.
fn json_rows(n: usize) -> String {
    let mut rows: Vec<String> = (0..n)
        .map(|i| {
            format!(
                r#"{{"deck":"Big::D{}","fields":{{"front":"q{i}","back":"a"}},"tags":["t{}"],"attributes":{{"level":"L{}"}}}}"#,
                i % 4,
                i % 5,
                i % 7
            )
        })
        .collect();
    rows.insert(n / 2, r#"{"deck":"Base","fields":{"front":"keep","back":"changed"}}"#.into());
    format!("[{}]", rows.join(","))
}

fn json_import(text: String, d: DeckId) -> StartImport {
    let mut req = text_import(&text, d);
    req.format = Some(ImportFormat::Json);
    req.create_missing_options = Some(true);
    req.duplicates = Some(DuplicatePolicy::Update);
    req
}

#[test]
fn cancel_after_committed_chunks_leaves_no_trace() {
    for t in [sqlite(), TestApp::mem()] {
        let d = deck(&t, "Base");
        level_attribute(&t);
        t.ok::<NotesCreate>(CreateNote {
            notetype_id: CLASSIC_ID,
            deck_id: d.id,
            fields: [("front".into(), "keep".into()), ("back".into(), "me".into())].into(),
            tags: vec!["t1".into()],
            attributes: vec![],
            allow_duplicate: None,
        });
        let sql = t.store.read().unwrap().dump_collection().is_ok();
        let before = sql.then(|| collection(&t));
        let notes_before = live_notes(&t.store);
        let undo_before = t.ok::<OplogStatus>(Empty {});
        // 120 rows, 10 per chunk; cancelled once 4 chunks (with the update of `keep`, new
        // decks, tags and options) are committed.
        let job = import_chunked(&t, json_import(json_rows(120), d.id), 10, |n, cancel| {
            if n == 7 {
                cancel.cancel();
            }
        });
        assert_eq!(job.status, JobStatus::Cancelled);
        assert_eq!(job.error.unwrap().code, ErrorCode::ConflictJobCancelled);
        // Exactly as before: no notes, decks, tags or options left, nothing in the trash, no
        // undo step. (Rows the import had touched — `keep`, the classification it added
        // options to — carry a later `updated_at`, as after an undo; their content is equal.)
        if let Some(before) = before {
            assert_eq!(without_updated_at(collection(&t)), without_updated_at(before));
        }
        let content = |notes: &[Note]| {
            notes.iter().map(|n| (n.id, n.fields.clone(), n.tags.clone())).collect::<Vec<_>>()
        };
        assert_eq!(content(&live_notes(&t.store)), content(&notes_before));
        let tx = t.store.read().unwrap();
        assert_eq!(tx.list_decks(true).unwrap().len(), 1);
        assert_eq!(tx.list_tags().unwrap().len(), 1);
        drop(tx);
        let after = t.ok::<OplogStatus>(Empty {});
        assert_eq!(after.undo_summary, undo_before.undo_summary);
    }
}

/// The app stops (crash, power loss) between two committed chunks: the next start rolls the
/// committed chunks back through the durable marker, so nothing is left without an undo step.
#[test]
fn crash_between_chunks_is_rolled_back_at_next_start() {
    let db = Arc::new(TempSqlite::new());
    let path = db.path();
    let stores: [(Arc<dyn Store>, bool); 2] = [(db, true), (Arc::new(MemStore::new()), false)];
    for (store, is_sqlite) in stores {
        let t = TestApp::new(store.clone());
        let d = deck(&t, "Base");
        level_attribute(&t);
        t.ok::<NotesCreate>(CreateNote {
            notetype_id: CLASSIC_ID,
            deck_id: d.id,
            fields: [("front".into(), "keep".into()), ("back".into(), "me".into())].into(),
            tags: vec!["t1".into()],
            attributes: vec![],
            allow_duplicate: None,
        });
        let before = is_sqlite.then(|| collection(&t));
        let notes_before = live_notes(&t.store);
        let undo_before = t.ok::<OplogStatus>(Empty {});
        // 120 rows, 10 per chunk; the process "dies" after the 5th committed chunk: no
        // cancel, no discard, no commit of the undo group runs.
        let job = t.ok::<IoImportStart>(json_import(json_rows(120), d.id));
        super::import::knobs::set(
            job.id,
            super::import::knobs::Knobs {
                chunk_rows: 10,
                after_chunk: Box::new(|n, _| {
                    if n == 5 {
                        panic!("simulated crash between chunks");
                    }
                }),
            },
        );
        let crashed =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| t.app.run_pending_jobs()));
        assert!(crashed.is_err());
        // The crash left committed rows and a marker describing them.
        assert!(live_notes(&store).len() > notes_before.len());
        {
            let tx = store.read().unwrap();
            let markers = tx.list_unfinished_batches().unwrap();
            assert_eq!(markers.len(), 1);
            assert_eq!(markers[0].state, "writing");
            assert_eq!(markers[0].job_id, Some(job.id));
            assert!(!tx.unfinished_batch_parts(markers[0].batch_id).unwrap().is_empty());
        }
        drop(t);
        // Next start (a fresh connection to the same file for SQLite).
        let reopened: Arc<dyn Store> = if is_sqlite {
            Arc::new(fc_storage::SqliteStore::open(&path).unwrap())
        } else {
            store.clone()
        };
        let t = TestApp::new(reopened);
        assert!(t.store.read().unwrap().list_unfinished_batches().unwrap().is_empty());
        if let Some(before) = before {
            assert_eq!(without_updated_at(collection(&t)), without_updated_at(before));
        }
        let content = |notes: &[Note]| {
            notes.iter().map(|n| (n.id, n.fields.clone(), n.tags.clone())).collect::<Vec<_>>()
        };
        assert_eq!(content(&live_notes(&t.store)), content(&notes_before));
        let tx = t.store.read().unwrap();
        assert_eq!(tx.list_decks(true).unwrap().len(), 1);
        assert_eq!(tx.list_tags().unwrap().len(), 1);
        drop(tx);
        assert_eq!(t.ok::<OplogStatus>(Empty {}).undo_summary, undo_before.undo_summary);
        let job = t.ok::<JobsGet>(JobRef { id: job.id });
        assert_eq!(job.status, JobStatus::Interrupted);
    }
}

#[test]
fn finished_and_cancelled_imports_leave_no_marker() {
    let t = sqlite();
    let d = deck(&t, "Base");
    import(&t, text_import("a, b\nc, d", d.id));
    let job = import_chunked(&t, json_import(json_rows(30), d.id), 10, |n, cancel| {
        if n == 2 {
            cancel.cancel();
        }
    });
    assert_eq!(job.status, JobStatus::Cancelled);
    assert!(t.store.read().unwrap().list_unfinished_batches().unwrap().is_empty());
}

#[test]
fn commands_run_while_a_large_import_writes_and_undo_is_still_exact() {
    let t = sqlite();
    let d = deck(&t, "Base");
    level_attribute(&t);
    let before = collection(&t);
    let job = t.ok::<IoImportStart>(json_import(json_rows(600), d.id));
    let chunks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (tx, rx) = std::sync::mpsc::channel::<usize>();
    let tx = std::sync::Mutex::new(tx);
    let counter = chunks.clone();
    super::import::knobs::set(
        job.id,
        super::import::knobs::Knobs {
            chunk_rows: 20,
            after_chunk: Box::new(move |n, _| {
                counter.store(n, std::sync::atomic::Ordering::SeqCst);
                if let Ok(tx) = tx.lock() {
                    let _ = tx.send(n);
                }
            }),
        },
    );
    let app = Arc::new(t.app);
    let worker = {
        let app = app.clone();
        std::thread::spawn(move || app.run_pending_jobs().unwrap())
    };
    // Once the import is under way, ordinary commands go through between its chunks.
    rx.recv().unwrap();
    let user = fc_api::v1::oplog::Actor::User;
    let mut seen = Vec::new();
    for i in 0..3 {
        let started = std::time::Instant::now();
        app.call::<NotesCreate>(
            &CreateNote {
                notetype_id: CLASSIC_ID,
                deck_id: d.id,
                fields: [("front".into(), format!("mine{i}")), ("back".into(), "x".into())]
                    .into(),
                tags: vec![],
                attributes: vec![],
                allow_duplicate: None,
            },
            &user,
        )
        .unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
        seen.push(chunks.load(std::sync::atomic::Ordering::SeqCst));
    }
    assert!(
        *seen.last().unwrap() < 30,
        "the commands ran while the import was still writing (chunks seen: {seen:?})"
    );
    assert_eq!(worker.join().unwrap(), 1);
    let done = app
        .call::<JobsGet>(&JobRef { id: job.id }, &user)
        .unwrap();
    let r: ImportReport = result(&done);
    assert_eq!(r.added_notes, 601);
    assert!(chunks.load(std::sync::atomic::Ordering::SeqCst) >= 30);
    assert_eq!(count_live(&t.store), 601 + 3);

    // One undo removes the import — and only the import: the three notes made meanwhile
    // stay.
    app.call::<OplogUndo>(&Empty {}, &user).unwrap();
    let mine: Vec<_> = live_notes(&t.store)
        .into_iter()
        .map(|n| fields(&n).0)
        .collect();
    assert_eq!(mine, vec!["mine0", "mine1", "mine2"]);
    // Undoing the three notes too gives back the collection as it was, apart from rows the
    // import's undo trashes (soft delete) — compare the live view.
    for _ in 0..3 {
        app.call::<OplogUndo>(&Empty {}, &user).unwrap();
    }
    let live = |d: &fc_storage::repo::CollectionDump| {
        d.tables
            .iter()
            .filter(|t| t.name == "attribute_defs")
            .map(|t| t.rows.len())
            .collect::<Vec<_>>()
    };
    assert_eq!(live(&collection_of(&t.store)), live(&before));
    assert!(live_notes(&t.store).is_empty());
    let decks = app.call::<DecksList>(&Empty {}, &user).unwrap().decks;
    assert_eq!(decks.into_iter().map(|d| d.path).collect::<Vec<_>>(), vec!["Base"]);
}

fn count_live(store: &Arc<dyn Store>) -> usize {
    let tx = store.read().unwrap();
    let mut n = 0;
    let mut cursor = None;
    loop {
        let page = tx
            .list_notes(
                &NoteFilter::default(),
                &fc_core::paging::PageRequest {
                    cursor,
                    limit: Some(1000),
                },
            )
            .unwrap();
        n += page.items.len();
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => return n,
        }
    }
}

/// A dump with every `updated_at` column blanked.
fn without_updated_at(mut d: fc_storage::repo::CollectionDump) -> fc_storage::repo::CollectionDump {
    for t in &mut d.tables {
        if let Some(i) = t.columns.iter().position(|c| c == "updated_at") {
            for r in &mut t.rows {
                r[i] = fc_storage::repo::SqlValue::Null;
            }
        }
    }
    d
}

fn collection_of(store: &Arc<dyn Store>) -> fc_storage::repo::CollectionDump {
    store.read().unwrap().dump_collection().unwrap()
}

#[test]
fn jobs_cancel_stops_queued_and_running_imports() {
    let t = TestApp::mem();
    let d = deck(&t, "Base");
    // Queued: never runs.
    let job = t.ok::<IoImportStart>(text_import("a,b\nc,d", d.id));
    let cancelled = t.ok::<JobsCancel>(JobRef { id: job.id });
    assert_eq!(cancelled.status, JobStatus::Cancelled);
    assert_eq!(t.app.run_pending_jobs().unwrap(), 0);
    assert!(live_notes(&t.store).is_empty());

    // Running: `jobs.cancel` reaches the job's token (signalled before the command waits for
    // the writer); the job stops at its next check and nothing is written.
    let job = t.ok::<IoImportStart>(text_import("a,b\nc,d", d.id));
    t.app.job_runtime().request_cancel(job.id);
    let done = finish(&t, &job);
    assert_eq!(done.status, JobStatus::Cancelled);
    assert_eq!(done.error.unwrap().code, ErrorCode::ConflictJobCancelled);
    assert!(live_notes(&t.store).is_empty());
}

#[test]
fn background_worker_runs_queued_jobs() {
    let t = TestApp::new(Arc::new(MemStore::new()));
    let d = deck(&t, "Base");
    let app = Arc::new(t.app);
    let worker = crate::jobs::spawn_worker(app.clone()).unwrap();
    let job = app
        .call::<IoImportStart>(&text_import("x,y\nz,w", d.id), &fc_api::v1::oplog::Actor::User)
        .unwrap();
    let mut status = JobStatus::Queued;
    for _ in 0..500 {
        status = app
            .call::<JobsGet>(&JobRef { id: job.id }, &fc_api::v1::oplog::Actor::User)
            .unwrap()
            .status;
        if status == JobStatus::Succeeded {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    drop(worker);
    assert_eq!(status, JobStatus::Succeeded);
    assert_eq!(live_notes(&t.store).len(), 2);
}

// ------------------------------------------------------------------ Anki

fn write_tmp(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.path().join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

fn assert_sample_imported(t: &TestApp, r: &ImportReport) {
    assert_eq!((r.added_notes, r.added_cards), (4, 5), "{r:?}");
    // Anki's stock Basic and Cloze land on the v0.3 built-ins: no new note types.
    assert_eq!((r.added_media, r.added_reviews, r.added_notetypes), (2, 4, 0));
    let tx = t.store.read().unwrap();
    let builtin = |key: &str| {
        fc_core::content::builtin::ALL
            .iter()
            .find(|b| b.key == key)
            .unwrap()
            .id
    };
    let cloze = tx.get_notetype(builtin("cloze")).unwrap().unwrap();
    assert_eq!(cloze.kind, NoteTypeKind::Cloze);
    let notes = live_notes(&t.store);
    assert!(notes
        .iter()
        .all(|n| n.notetype_id == cloze.id || n.notetype_id == CLASSIC_ID));
    let geo = notes.iter().find(|n| n.notetype_id == cloze.id).unwrap();
    assert_eq!(
        geo.fields[&FieldKey::new("text")],
        "{{c1::Ankara}} is the capital of {{c2::Türkiye}}"
    );
    let geo_cards = tx
        .list_cards(&CardFilter {
            note_ids: Some(vec![geo.id]),
            ..CardFilter::default()
        })
        .unwrap();
    let mut keys: Vec<_> = geo_cards.iter().map(|c| c.template_key.clone()).collect();
    keys.sort();
    assert_eq!(keys, vec!["c1", "c2"]);
    // Image and sound: references rewritten, bytes stored by hash.
    let cat = notes.iter().find(|n| fields(n).1 == "kedi").unwrap();
    assert_eq!(
        fields(cat).0,
        format!("<img src=\"{}\">", media_url(&sha256_hex(anki_fx::CAT)))
    );
    assert_eq!(
        tx.read_media_file(&sha256_hex(anki_fx::CAT)).unwrap().as_deref(),
        Some(anki_fx::CAT)
    );
    let hello = notes.iter().find(|n| fields(n).1 == "merhaba").unwrap();
    assert_eq!(
        fields(hello).0,
        format!(
            "hello <audio controls src=\"{}\"></audio>",
            media_url(&sha256_hex(anki_fx::HELLO))
        )
    );
    assert_eq!(
        tx.read_media_file(&sha256_hex(anki_fx::HELLO)).unwrap().as_deref(),
        Some(anki_fx::HELLO)
    );
    // Scheduling and history.
    let dog = notes.iter().find(|n| fields(n).0 == "dog").unwrap();
    let dog_card = &tx
        .list_cards(&CardFilter {
            note_ids: Some(vec![dog.id]),
            ..CardFilter::default()
        })
        .unwrap()[0];
    assert_eq!(dog_card.scheduling.state, CardState::Review);
    assert_eq!((dog_card.scheduling.reps, dog_card.scheduling.lapses), (5, 1));
    // Anki flag 7 (purple) → `card_flag_defs` flag 7.
    assert_eq!(dog_card.flags, 1 << 6);
    let logs = tx.review_logs_for_cards(&[dog_card.id]).unwrap();
    let ratings: Vec<_> = logs.iter().map(|l| l.rating).collect();
    assert_eq!(ratings, vec![Rating::Good, Rating::Good, Rating::Again]);
    // Brought-in history never counts toward today's limits.
    assert!(logs
        .iter()
        .all(|l| l.mode == fc_storage::repo::REVIEW_MODE_IMPORTED));
    let tags: Vec<_> = dog.tags.iter().map(|t| t.as_str()).collect();
    assert_eq!(tags, vec!["animals", "lang::tr"]);
}

#[test]
fn anki_packages_import_losslessly_in_every_layout() {
    let dir = tempfile::tempdir().unwrap();
    let col = anki_fx::sample();
    for (name, bytes) in [
        ("legacy.apkg", fc_interop::anki::write_package(&col, anki_fx::NOW).unwrap()),
        ("anki21.apkg", anki_fx::legacy2_package(&col)),
        ("latest.colpkg", anki_fx::latest_package(&col)),
    ] {
        let t = sqlite();
        let path = write_tmp(&dir, name, &bytes);
        let preview = t.ok::<IoImportPreview>(PreviewImport {
            source: ImportSource::File {
                path: path.to_string_lossy().into_owned(),
            },
            format: None,
            text_options: None,
            notetype_id: None,
            deck_id: None,
            limit: None,
        });
        assert_eq!(preview.format, ImportFormat::Anki);
        let pkg = preview.package.unwrap();
        assert_eq!((pkg.notes, pkg.cards, pkg.media), (4, 5, 2));
        assert_eq!(pkg.decks, vec!["Lang", "Lang::Words"]);
        let r = import(&t, file_import(&path, None));
        assert_sample_imported(&t, &r);
        // Importing again finds the same notes.
        let again = import(&t, file_import(&path, None));
        assert_eq!((again.added_notes, again.skipped_duplicates), (0, 4), "{name}");
    }
}

#[test]
fn exported_apkg_reimports_with_the_same_content() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_tmp(
        &dir,
        "in.apkg",
        &fc_interop::anki::write_package(&anki_fx::sample(), anki_fx::NOW).unwrap(),
    );
    let a = sqlite();
    import(&a, file_import(&src, None));
    let out = dir.path().join("out.apkg");
    let job = a.ok::<IoExportStart>(StartExport {
        format: ExportFormat::Apkg,
        path: out.to_string_lossy().into_owned(),
        deck_id: None,
        include_history: Some(true),
        text_options: None,
    });
    let report: ExportReport = result(&finish(&a, &job));
    assert_eq!((report.notes, report.cards, report.media), (4, 5, 2));
    // The file is a valid Anki package (structure checked in fc-interop) and re-imports.
    let b = sqlite();
    let r = import(&b, file_import(&out, None));
    assert_sample_imported(&b, &r);
    let norm = |t: &TestApp| {
        live_notes(&t.store)
            .into_iter()
            .map(|n| (n.id, n.fields, n.tags))
            .collect::<Vec<_>>()
    };
    assert_eq!(norm(&a), norm(&b));
}

// ------------------------------------------------------------------ .fcpack

#[test]
fn fcpack_preserves_classifications_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let a = sqlite();
    let lang = deck(&a, "Lang");
    let words = a.ok::<DecksCreate>(CreateDeck {
        path: "Words".into(),
        parent_id: Some(lang.id),
        description: Some("desc".into()),
        icon: None,
        color: Some("#ff0000".into()),
    });
    let level = a.ok::<AttributesDefineFromTemplate>(
        fc_api::v1::attributes::DefineAttributeFromTemplate {
            template_id: "cefr".into(),
            scope_deck_id: Some(lang.id),
            locale: None,
        },
    );
    let b1 = level.options.iter().find(|o| o.label == "B1").unwrap().id;
    let note = a.ok::<NotesCreate>(CreateNote {
        notetype_id: CLASSIC_ID,
        deck_id: words.id,
        fields: [("front".into(), "hayal".into()), ("back".into(), "dream".into())].into(),
        tags: vec!["x".into()],
        attributes: vec![fc_api::v1::attributes::AttributeAssignment {
            attr_id: level.id,
            value: Some(AttributeValue::Choice(b1)),
        }],
        allow_duplicate: None,
    });
    let path = dir.path().join("lang.fcpack");
    let job = a.ok::<IoExportStart>(StartExport {
        format: ExportFormat::Fcpack,
        path: path.to_string_lossy().into_owned(),
        deck_id: Some(lang.id),
        include_history: Some(true),
        text_options: None,
    });
    result::<ExportReport>(&finish(&a, &job));

    let b = sqlite();
    let r = import(&b, file_import(&path, None));
    assert_eq!((r.added_notes, r.added_attributes, r.added_decks), (1, 1, 2));
    let def = b.ok::<AttributesGet>(fc_api::v1::attributes::AttributeRef { id: level.id });
    assert_eq!(def.options, level.options);
    assert_eq!((def.key.as_str(), def.kind), (level.key.as_str(), level.kind));
    let tx = b.store.read().unwrap();
    let imported = tx.get_note(note.id).unwrap().unwrap();
    assert_eq!(
        tx.get_attribute_values(EntityRef::Note(imported.id)).unwrap(),
        vec![(level.id, AttributeValue::Choice(b1))]
    );
    let d = tx.get_deck(words.id).unwrap().unwrap();
    assert_eq!((d.description.as_str(), d.color.as_deref()), ("desc", Some("#ff0000")));
    assert_eq!(def.scope_deck_id, Some(lang.id));
}

/// v0.10 review fix: imported history counts as `imported` (never against today's limits),
/// except practice rows — `trial`, every `trial_<mode>`, `test` and `match` — which keep their mode
/// (so statistics and FSRS optimization keep skipping them).
#[test]
fn imported_history_keeps_practice_modes() {
    use fc_core::{ReviewLogId, Timestamp};
    use fc_storage::repo::ReviewLogRecord;
    let dir = tempfile::tempdir().unwrap();
    let a = sqlite();
    let d = deck(&a, "Modlar");
    let note = a.ok::<NotesCreate>(CreateNote {
        notetype_id: CLASSIC_ID,
        deck_id: d.id,
        fields: [("front".into(), "kip".into()), ("back".into(), "mode".into())].into(),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    });
    let card = note.cards[0].id;
    let modes = [
        "normal", "learn", "timed", "confidence", "trial", "trial_learn", "trial_timed",
        "trial_confidence", "test", "match",
    ];
    {
        let mut tx = a.store.write().unwrap();
        for (i, mode) in modes.iter().enumerate() {
            tx.insert_review_log(&ReviewLogRecord {
                id: ReviewLogId(uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0001 + i as u128)),
                card_id: card,
                session_id: None,
                rating: Rating::Good,
                state_before: CardState::New,
                state_after: CardState::Learning,
                scheduled_days: 0,
                elapsed_days: 0,
                duration_ms: 1_000,
                answer_duration_ms: None,
                hints_used: 0,
                mode: (*mode).to_owned(),
                stability_after: None,
                difficulty_after: None,
                created_at: Timestamp(1_772_445_600_000 + i as i64),
            })
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let path = dir.path().join("modes.fcpack");
    let job = a.ok::<IoExportStart>(StartExport {
        format: ExportFormat::Fcpack,
        path: path.to_string_lossy().into_owned(),
        deck_id: Some(d.id),
        include_history: Some(true),
        text_options: None,
    });
    result::<ExportReport>(&finish(&a, &job));

    let b = sqlite();
    let mut req = file_import(&path, None);
    req.include_history = Some(true);
    let r = import(&b, req);
    assert_eq!(r.added_reviews, modes.len() as u32);
    let tx = b.store.read().unwrap();
    let mut got: Vec<String> = tx
        .review_logs_for_cards(&[card])
        .unwrap()
        .into_iter()
        .map(|l| l.mode)
        .collect();
    got.sort();
    let imported = fc_storage::repo::REVIEW_MODE_IMPORTED.to_owned();
    let mut want = vec![imported.clone(), imported.clone(), imported.clone(), imported];
    want.extend(
        ["match", "test", "trial", "trial_confidence", "trial_learn", "trial_timed"]
            .map(str::to_owned),
    );
    want.sort();
    assert_eq!(got, want);
}

// ------------------------------------------------------------------ backup

#[test]
fn backup_delete_restore_preserves_everything() {
    let dir = tempfile::tempdir().unwrap();
    let t = sqlite();
    let app_data = dir.path().join("app-data");
    t.app.set_data_dir(app_data.clone());
    // Rich state: Anki import (media, history, cloze), classifications, settings, oplog.
    let src = write_tmp(
        &dir,
        "in.apkg",
        &fc_interop::anki::write_package(&anki_fx::sample(), anki_fx::NOW).unwrap(),
    );
    import(&t, file_import(&src, None));
    let lang = t.ok::<DecksList>(Empty {}).decks.into_iter().find(|d| d.path == "Lang").unwrap();
    t.ok::<AttributesDefineFromTemplate>(fc_api::v1::attributes::DefineAttributeFromTemplate {
        template_id: "cefr".into(),
        scope_deck_id: Some(lang.id),
        locale: None,
    });
    let dump = collection;
    let before = dump(&t);
    let media_before = t.store.read().unwrap().list_media().unwrap();

    let backup = dir.path().join("b.fcbackup");
    let job = t.ok::<IoBackupCreate>(CreateBackup {
        path: backup.to_string_lossy().into_owned(),
    });
    let rep: ExportReport = result(&finish(&t, &job));
    assert_eq!(rep.notes, 4);
    let info = t.ok::<IoBackupInspect>(InspectBackup {
        path: backup.to_string_lossy().into_owned(),
    });
    assert!(info.compatible);
    assert_eq!(info.media, 2);

    // Delete.
    t.ok::<DecksDelete>(DeckRef { id: lang.id });
    assert!(live_notes(&t.store).is_empty());
    let after_delete = dump(&t);
    assert_ne!(after_delete, before);

    // Restore.
    t.take_events();
    let job = t.ok::<IoBackupRestore>(RestoreBackup {
        path: backup.to_string_lossy().into_owned(),
        safety_backup_path: None,
    });
    let rr: RestoreReport = result(&finish(&t, &job));
    assert_eq!(dump(&t), before, "every table is restored exactly");
    assert_fresh_history(&t);
    let tx = t.store.read().unwrap();
    assert_eq!(tx.list_media().unwrap(), media_before);
    for m in &media_before {
        assert_eq!(sha256_hex(&tx.read_media_file(&m.hash).unwrap().unwrap()), m.hash);
    }
    drop(tx);
    assert!(t
        .take_events()
        .iter()
        .any(|e| matches!(e.event, fc_api::v1::events::DomainEvent::CollectionReplaced)));

    // The automatic safety backup holds the deleted state; it goes to the app data folder
    // (the backup's own folder may be read-only).
    assert!(std::path::Path::new(&rr.safety_backup_path).exists());
    assert!(std::path::Path::new(&rr.safety_backup_path).starts_with(app_data.join("backups")));
    let job = t.ok::<IoBackupRestore>(RestoreBackup {
        path: rr.safety_backup_path.clone(),
        safety_backup_path: Some(dir.path().join("s2.fcbackup").to_string_lossy().into_owned()),
    });
    result::<RestoreReport>(&finish(&t, &job));
    assert_eq!(dump(&t), after_delete);
    assert_fresh_history(&t);
}

#[test]
fn restore_refuses_damaged_or_foreign_files() {
    let dir = tempfile::tempdir().unwrap();
    let t = sqlite();
    let bad = write_tmp(&dir, "x.fcbackup", b"not a zip");
    let job = t.ok::<IoBackupRestore>(RestoreBackup {
        path: bad.to_string_lossy().into_owned(),
        safety_backup_path: None,
    });
    let done = finish(&t, &job);
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error.unwrap().code, ErrorCode::ValidationImportFormat);
    let missing = t.err::<IoBackupInspect>(InspectBackup {
        path: dir.path().join("nope").to_string_lossy().into_owned(),
    });
    assert_eq!(missing.code, ErrorCode::IoFile);
}

// ------------------------------------------------------------------ export, print

#[test]
fn text_and_json_exports_reimport_and_print_renders() {
    let dir = tempfile::tempdir().unwrap();
    let a = TestApp::mem();
    let d = deck(&a, "Words");
    import(&a, text_import("dog\tköpek\ncat\tkedi, pisi", d.id));
    for (format, name) in [(ExportFormat::Text, "w.txt"), (ExportFormat::Json, "w.json")] {
        let path = dir.path().join(name);
        let job = a.ok::<IoExportStart>(StartExport {
            format,
            path: path.to_string_lossy().into_owned(),
            deck_id: Some(d.id),
            include_history: None,
            text_options: None,
        });
        assert_eq!(result::<ExportReport>(&finish(&a, &job)).notes, 2);
        let b = TestApp::mem();
        let bd = deck(&b, "Words");
        let mut req = file_import(&path, Some(bd.id));
        if format == ExportFormat::Text {
            req.columns = Some(vec![
                ColumnTarget::Field { key: "front".into() },
                ColumnTarget::Field { key: "back".into() },
                ColumnTarget::Tags,
            ]);
        }
        assert_eq!(import(&b, req).added_notes, 2, "{name}");
        let got: Vec<_> = live_notes(&b.store).iter().map(fields).collect();
        assert!(got.contains(&("cat".into(), "kedi, pisi".into())));
    }
    let doc = a.ok::<IoPrintRender>(RenderPrint {
        deck_id: d.id,
        layout: PrintLayoutDto::Cards { columns: 2, rows: 4 },
    });
    assert_eq!(doc.cards, 2);
    assert!(doc.html.contains("köpek"));
}

#[test]
fn start_validates_requests() {
    let t = TestApp::mem();
    let d = deck(&t, "D");
    let mut req = text_import("a,b", d.id);
    req.text_options = Some(TextOptionsDto {
        field_separator: ",".into(),
        record_separator: ",".into(),
        quotes: true,
        max_fields: None,
    });
    assert_eq!(t.err::<IoImportStart>(req).code, ErrorCode::ValidationTextOptions);
    let mut req = text_import("a,b", d.id);
    req.columns = Some(vec![ColumnTarget::Field { key: "nope".into() }]);
    assert_eq!(t.err::<IoImportStart>(req).code, ErrorCode::ValidationColumnMapping);
    // Internal methods stay internal.
    let e = t
        .app
        .invoke(
            "v1.io.import.apply",
            serde_json::json!({"job_id": uuid::Uuid::nil()}),
            &fc_api::v1::oplog::Actor::User,
        )
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied);
    // The module can be switched off.
    t.ok::<ModulesSetEnabled>(fc_api::v1::modules::SetModuleEnabled {
        id: "io".into(),
        enabled: false,
    });
    assert_eq!(
        t.err::<IoImportStart>(text_import("a,b", d.id)).code,
        ErrorCode::ConflictModuleDisabled
    );
}

#[test]
fn restoring_an_older_schema_backup_migrates_it_forward() {
    let dir = tempfile::tempdir().unwrap();
    let t = sqlite();
    let d = deck(&t, "Old");
    import(&t, text_import("bir\tone\niki\ttwo", d.id));
    let before = collection(&t);
    let path = dir.path().join("b.fcbackup");
    let job = t.ok::<IoBackupCreate>(CreateBackup {
        path: path.to_string_lossy().into_owned(),
    });
    result::<ExportReport>(&finish(&t, &job));
    // Rewrite it as a backup from a build without 0302 and the v0.5 migrations (04xx; their
    // tables are not in dumps). Older cuts would need the tables later migrations add removed and
    // derived rows (v0.3 `note_keys`, …) rebuilt — not something a dump can fake. Migrations of
    // later releases (05xx+, e.g. v0.7's plugin tables) stay applied: they do not depend on
    // 0302/04xx, and their tables are in the dump.
    let b = fc_interop::fcbackup::read(&std::fs::read(&path).unwrap()).unwrap();
    let mut dump: fc_storage::repo::CollectionDump = serde_json::from_slice(&b.collection).unwrap();
    let older: Vec<u32> = dump
        .schema_versions
        .iter()
        .copied()
        .filter(|v| *v < 302 || *v >= 500)
        .collect();
    assert!(older.len() < dump.schema_versions.len());
    // …whose search index was written the old way (0302 and the restore rebuild it).
    let fts = dump.tables.iter_mut().find(|t| t.name == "notes_fts").unwrap();
    let col = fts.columns.iter().position(|c| c == "content").unwrap();
    assert!(!fts.rows.is_empty());
    for row in &mut fts.rows {
        row[col] = fc_storage::repo::SqlValue::Text("{{c1::STALE}} <b>raw</b>".into());
    }
    // Tables and columns of later migrations do not exist in that schema (recreated empty by
    // the migrations on restore, as they are here): cut generically, whatever track adds them.
    let dump = fc_storage::fit_dump_to_schema(dump, &older).unwrap();
    let header = fc_interop::fcbackup::BackupHeader {
        created_at: b.manifest.created_at,
        schema_versions: older,
        counts: dump.tables.iter().map(|t| (t.name.clone(), t.rows.len() as u64)).collect(),
    };
    let (bytes, _) =
        fc_interop::fcbackup::write(header, &serde_json::to_vec(&dump).unwrap(), &b.media)
            .unwrap();
    let old = write_tmp(&dir, "old.fcbackup", &bytes);
    t.ok::<DecksDelete>(DeckRef { id: d.id });
    let job = t.ok::<IoBackupRestore>(RestoreBackup {
        path: old.to_string_lossy().into_owned(),
        safety_backup_path: Some(dir.path().join("s.fcbackup").to_string_lossy().into_owned()),
    });
    result::<RestoreReport>(&finish(&t, &job));
    let after = collection(&t);
    assert_eq!(after.schema_versions, before.schema_versions);
    assert_eq!(after, before);
    assert_fresh_history(&t);
}

#[test]
fn final_job_status_is_retried() {
    let calls = std::sync::atomic::AtomicU32::new(0);
    let r = crate::jobs::retry(|| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 2 {
            Err(crate::error::AppError::api(ErrorCode::Internal, "busy"))
        } else {
            Ok(7)
        }
    });
    assert_eq!(r.unwrap(), 7);
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
}

#[test]
fn restoring_a_backup_forgets_mcp_server_approvals() {
    use fc_api::v1::mcp::{AddMcpServer, McpTransport};
    let dir = tempfile::tempdir().unwrap();
    let t = sqlite();
    t.app.set_data_dir(dir.path().join("app-data"));
    let s = t.ok::<fc_api::v1::methods::McpServersAdd>(AddMcpServer {
        name: "Files".into(),
        transport: McpTransport::Stdio,
        command: Some("npx".into()),
        args: vec![],
        url: None,
        enabled: None,
    });
    assert!(s.approved);
    let backup = dir.path().join("b.fcbackup");
    let job = t.ok::<IoBackupCreate>(CreateBackup {
        path: backup.to_string_lossy().into_owned(),
    });
    result::<ExportReport>(&finish(&t, &job));
    let job = t.ok::<IoBackupRestore>(RestoreBackup {
        path: backup.to_string_lossy().into_owned(),
        safety_backup_path: None,
    });
    result::<RestoreReport>(&finish(&t, &job));
    let servers = t.ok::<fc_api::v1::methods::McpServersList>(Empty {}).servers;
    assert_eq!(servers.len(), 1, "the registration itself comes back");
    assert!(!servers[0].approved, "…but must be approved again before it can launch");
    // Nor can history bring the approval back: the restored oplog is gone behind a barrier.
    let e = t.err::<OplogUndo>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictHistoryBarrier, "{e:?}");
    let servers = t.ok::<fc_api::v1::methods::McpServersList>(Empty {}).servers;
    assert!(!servers[0].approved);
}

/// v1.4 (B3): imported notes record their origin — `nereden:anki` for an Anki package,
/// `nereden:içe_aktarma` for text — read back through `search.cards`.
#[test]
fn imports_record_the_note_origin() {
    use fc_api::v1::search::SearchCards;
    let count = |t: &TestApp, q: &str| {
        t.ok::<SearchCardsMethod>(SearchCards {
            query: q.into(),
            sort: None,
            descending: None,
            notes_only: Some(true),
            offset: None,
            limit: None,
            cursor: None,
        })
        .total
    };
    let dir = tempfile::tempdir().unwrap();
    let t = sqlite();
    let path = write_tmp(
        &dir,
        "in.apkg",
        &fc_interop::anki::write_package(&anki_fx::sample(), anki_fx::NOW).unwrap(),
    );
    assert_eq!(import(&t, file_import(&path, None)).added_notes, 4);
    let d = deck(&t, "Text");
    assert_eq!(import(&t, text_import("kedi\tcat\nköpek\tdog\n", d.id)).added_notes, 2);
    assert_eq!(count(&t, "nereden:anki"), 4);
    assert_eq!(count(&t, "from:import"), 2);
    assert_eq!(count(&t, "nereden:elle"), 0);
}
