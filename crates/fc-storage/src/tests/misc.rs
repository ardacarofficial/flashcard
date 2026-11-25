//! Oplog, settings, module switches, jobs.

use super::{did, Fx};
use crate::error::StorageError;
use crate::repo::*;
use crate::store::Store;
use fc_core::paging::PageRequest;
use fc_core::settings::SettingScope;
use fc_core::{BatchId, JobId, OplogId, Timestamp};
use serde_json::json;

fn op(n: u128, batch: u128, seq: u32, kind: OplogRecordKind, reverts: Option<u128>) -> OplogRecord {
    OplogRecord {
        id: OplogId(super::u(0x5000 + n)),
        batch_id: BatchId(super::u(0x6000 + batch)),
        seq,
        command: format!("v1.decks.create#{n}"),
        payload: json!({"path": format!("D{n}")}),
        inverse: json!([{"method": "v1.decks.delete", "params": {"id": n}}]),
        actor: json!({"kind": "user"}),
        kind,
        reverts_batch_id: reverts.map(|r| BatchId(super::u(0x6000 + r))),
        summary: json!({"key": "oplog.decks.create", "args": {"n": n}}),
        created_at: Timestamp(i64::try_from(n).unwrap()),
    }
}

fn bid(n: u128) -> BatchId {
    BatchId(super::u(0x6000 + n))
}

#[test]
fn oplog_append_batches_and_entries() {
    let fx = Fx::new();
    use OplogRecordKind::*;
    let records = [
        op(1, 1, 0, Do, None),
        op(2, 2, 0, Do, None),
        op(3, 2, 1, Do, None),
        op(4, 3, 0, Undo, Some(2)),
        op(5, 3, 1, Undo, Some(2)),
        op(6, 4, 0, Redo, Some(3)),
    ];
    // One command = one write transaction = one batch.
    for batch in [
        &records[0..1],
        &records[1..3],
        &records[3..5],
        &records[5..6],
    ] {
        fx.write(|tx| {
            for r in batch {
                tx.append_oplog(r).unwrap();
            }
        });
    }
    fx.read(|r| {
        assert_eq!(r.oplog_batch_entries(bid(2)).unwrap(), records[1..3]);
        assert!(r.oplog_batch_entries(bid(99)).unwrap().is_empty());

        let batches = r.list_oplog_batches(None, 10).unwrap();
        let ids: Vec<_> = batches.iter().map(|b| b.batch_id).collect();
        assert_eq!(ids, [bid(4), bid(3), bid(2), bid(1)]);
        assert_eq!(batches[0].kind, Redo);
        assert_eq!(batches[0].reverts_batch_id, Some(bid(3)));
        assert_eq!(batches[1].kind, Undo);
        assert_eq!(
            batches[1].summary, records[3].summary,
            "summary of first entry"
        );
        assert_eq!(batches[2].created_at, Timestamp(2));

        let two = r.list_oplog_batches(None, 2).unwrap();
        assert_eq!(
            two.iter().map(|b| b.batch_id).collect::<Vec<_>>(),
            [bid(4), bid(3)]
        );
        let older = r.list_oplog_batches(Some(bid(3)), 5).unwrap();
        assert_eq!(
            older.iter().map(|b| b.batch_id).collect::<Vec<_>>(),
            [bid(2), bid(1)]
        );
        assert!(r.list_oplog_batches(Some(bid(1)), 5).unwrap().is_empty());
        assert!(r.list_oplog_batches(None, 0).unwrap().is_empty());
        assert!(matches!(
            r.list_oplog_batches(Some(bid(77)), 5),
            Err(StorageError::NotFound { .. })
        ));
    });
}

#[test]
fn oplog_listing_pages_newest_first() {
    let fx = Fx::new();
    fx.write(|tx| {
        for n in 1..=5 {
            tx.append_oplog(&op(n, n, 0, OplogRecordKind::Do, None))
                .unwrap();
        }
    });
    let mut page = PageRequest {
        cursor: None,
        limit: Some(2),
    };
    let mut seen = Vec::new();
    loop {
        let p = fx.read(|r| r.list_oplog(&page).unwrap());
        seen.extend(p.items.iter().map(|e| e.id));
        match p.next_cursor {
            Some(c) => page.cursor = Some(c),
            None => break,
        }
    }
    let expected: Vec<_> = (1..=5)
        .rev()
        .map(|n| OplogId(super::u(0x5000 + n)))
        .collect();
    assert_eq!(seen, expected);
    fx.read(|r| {
        let full = r.list_oplog(&PageRequest::default()).unwrap();
        assert_eq!(full.items[0], op(5, 5, 0, OplogRecordKind::Do, None));
        assert!(matches!(
            r.list_oplog(&PageRequest {
                cursor: Some("x".into()),
                limit: None
            }),
            Err(StorageError::InvalidCursor(_))
        ));
    });
}

#[test]
fn settings_per_scope() {
    let fx = Fx::new();
    let deck = SettingScope::Deck(did(1));
    fx.write(|tx| {
        tx.set_setting(&SettingScope::App, "app.locale", &json!("tr"), Timestamp(1))
            .unwrap();
        tx.set_setting(
            &SettingScope::App,
            "app.theme",
            &json!({"mode": "dark"}),
            Timestamp(1),
        )
        .unwrap();
        tx.set_setting(&deck, "decks.include_subdecks", &json!(false), Timestamp(1))
            .unwrap();
        tx.set_setting(&SettingScope::App, "app.locale", &json!("en"), Timestamp(2))
            .unwrap();
    });
    fx.read(|r| {
        assert_eq!(
            r.get_setting(&SettingScope::App, "app.locale").unwrap(),
            Some(json!("en"))
        );
        assert_eq!(r.get_setting(&deck, "app.locale").unwrap(), None);
        assert_eq!(
            r.list_settings(&SettingScope::App).unwrap(),
            [
                ("app.locale".to_owned(), json!("en")),
                ("app.theme".to_owned(), json!({"mode": "dark"}))
            ]
        );
        assert_eq!(r.list_settings(&deck).unwrap().len(), 1);
        assert!(r.list_settings(&SettingScope::Profile).unwrap().is_empty());
    });
    fx.write(|tx| {
        assert!(tx.delete_setting(&deck, "decks.include_subdecks").unwrap());
        assert!(!tx.delete_setting(&deck, "decks.include_subdecks").unwrap());
    });
    fx.read(|r| assert!(r.list_settings(&deck).unwrap().is_empty()));
}

#[test]
fn module_switches() {
    let fx = Fx::new();
    fx.read(|r| assert!(r.list_module_states().unwrap().is_empty()));
    fx.write(|tx| {
        tx.set_module_enabled("tags", false, Timestamp(1)).unwrap();
        tx.set_module_enabled("attributes", true, Timestamp(1))
            .unwrap();
        tx.set_module_enabled("tags", true, Timestamp(2)).unwrap();
    });
    fx.read(|r| {
        assert_eq!(
            r.list_module_states().unwrap(),
            [("attributes".to_owned(), true), ("tags".to_owned(), true)]
        )
    });
}

fn job(n: u128, status: &str, at: i64) -> JobRecord {
    JobRecord {
        id: JobId(super::u(0x7000 + n)),
        kind: "import.apkg".into(),
        status: status.into(),
        progress: None,
        payload: json!({"file": n}),
        result: None,
        error: None,
        created_at: Timestamp(at),
        updated_at: Timestamp(at),
    }
}

#[test]
fn jobs_enqueue_claim_update_recover() {
    let fx = Fx::new();
    fx.write(|tx| {
        tx.upsert_job(&job(1, "queued", 10)).unwrap();
        tx.upsert_job(&job(2, "queued", 20)).unwrap();
        tx.upsert_job(&job(3, "succeeded", 5)).unwrap();
    });
    let claimed = fx.write(|tx| tx.claim_next_job(Timestamp(30)).unwrap().unwrap());
    assert_eq!(claimed.id, job(1, "", 0).id, "oldest first");
    assert_eq!(
        (claimed.status.as_str(), claimed.updated_at),
        ("running", Timestamp(30))
    );

    let mut done = claimed.clone();
    done.status = "succeeded".into();
    done.progress = Some(1.0);
    done.result = Some(json!({"notes": 12}));
    done.updated_at = Timestamp(40);
    let mut failed = job(2, "failed", 20);
    failed.error = Some(json!({"code": "IO_FILE", "message_key": "errors.io_file"}));
    fx.write(|tx| {
        tx.upsert_job(&done).unwrap();
        tx.upsert_job(&failed).unwrap();
        assert!(tx.claim_next_job(Timestamp(50)).unwrap().is_none());
    });
    fx.read(|r| {
        assert_eq!(r.get_job(done.id).unwrap().unwrap(), done);
        assert_eq!(r.get_job(failed.id).unwrap().unwrap(), failed);
        assert!(r.get_job(JobId(super::u(1))).unwrap().is_none());
        let listed: Vec<_> = r.list_jobs(2).unwrap().into_iter().map(|j| j.id).collect();
        assert_eq!(listed, [failed.id, done.id], "newest first, limited");
    });

    fx.write(|tx| {
        tx.upsert_job(&job(4, "running", 60)).unwrap();
        tx.upsert_job(&job(5, "queued", 61)).unwrap();
        // A job another live process runs is kept (v1.4.2 K1).
        tx.upsert_job(&job(7, "running", 62)).unwrap();
        assert_eq!(
            tx.mark_unfinished_jobs_interrupted(Timestamp(70), &[job(7, "", 0).id])
                .unwrap(),
            2
        );
    });
    fx.read(|r| {
        let j = r.get_job(job(4, "", 0).id).unwrap().unwrap();
        assert_eq!(
            (j.status.as_str(), j.updated_at),
            ("interrupted", Timestamp(70))
        );
        let kept = r.get_job(job(7, "", 0).id).unwrap().unwrap();
        assert_eq!(
            (kept.status.as_str(), kept.updated_at),
            ("running", Timestamp(62))
        );
    });
    fx.write(|tx| {
        assert_eq!(
            tx.mark_unfinished_jobs_interrupted(Timestamp(71), &[])
                .unwrap(),
            1
        );
    });
    // The schema rejects unknown statuses.
    let mut tx = fx.store.write().unwrap();
    assert!(matches!(
        tx.upsert_job(&job(6, "weird", 1)),
        Err(StorageError::Constraint { .. })
    ));
}
