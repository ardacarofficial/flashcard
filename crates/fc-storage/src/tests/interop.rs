//! Import/export storage: migrations 0400–0402 (media blobs moved to files), review log rows,
//! dump/load, unfinished undo groups (import crash markers).

use super::*;
use crate::migrate;
use crate::repo::{MediaRecord, ReviewLogRecord, SqlValue};
use fc_core::card::{CardState, Rating};
use fc_core::content::media::media_hash;
use fc_core::{ReviewLogId, Timestamp};

fn media(bytes: &[u8]) -> MediaRecord {
    MediaRecord {
        hash: media_hash(bytes),
        mime: "image/png".into(),
        size: bytes.len() as u64,
        original_name: Some("a.png".into()),
        ref_count: 0,
        created_at: Timestamp(7),
    }
}

fn review(n: u128, card: CardId, at: i64) -> ReviewLogRecord {
    ReviewLogRecord {
        id: ReviewLogId(u(0x9000 + n)),
        card_id: card,
        session_id: None,
        rating: Rating::Good,
        state_before: CardState::Learning,
        state_after: CardState::Review,
        scheduled_days: 3,
        elapsed_days: 1,
        duration_ms: 4200,
        answer_duration_ms: Some(900),
        hints_used: 0,
        mode: "normal".into(),
        stability_after: Some(0.1 + 0.2),
        difficulty_after: None,
        created_at: Timestamp(at),
    }
}

#[test]
fn migration_0402_moves_media_blobs_to_files_and_drops_the_table() {
    use crate::migrate::MIGRATIONS;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    // A pre-release v0.5 database (up to 0401) with media bytes in `media_blobs`.
    let upto: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version <= 401)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &upto).unwrap());
    let (a, b) = (&b"first"[..], &b"second"[..]);
    {
        let conn = Connection::open(&path).unwrap();
        for bytes in [a, b] {
            let m = media(bytes);
            conn.execute(
                "INSERT INTO media (hash, mime, size, created_at) VALUES (?1, 'image/png', ?2, 7)",
                rusqlite::params![m.hash, m.size as i64],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO media_blobs VALUES (?1, ?2)",
                rusqlite::params![m.hash, bytes],
            )
            .unwrap();
        }
    }
    // One of them is already on disk (e.g. a re-run): kept as is.
    let pa = crate::media::media_file_path(&path, &media_hash(a)).unwrap();
    std::fs::create_dir_all(pa.parent().unwrap()).unwrap();
    std::fs::write(&pa, a).unwrap();

    let store = SqliteStore::open(&path).unwrap();
    let conn = Connection::open(&path).unwrap();
    assert!(!migrate::schema_object_exists(&conn, "media_blobs").unwrap());
    assert!(migrate::current_version(&conn).unwrap() >= 402);
    let r = store.read().unwrap();
    for bytes in [a, b] {
        let h = media_hash(bytes);
        assert_eq!(r.read_media_file(&h).unwrap().as_deref(), Some(bytes));
        assert!(r.get_media(&h).unwrap().is_some(), "metadata row stays");
    }
    // Idempotent once the table is gone.
    assert_eq!(crate::media::export_legacy_blobs(&conn, &path).unwrap(), 0);
}

#[test]
fn migration_0401_unfinished_batches_hold_parts_and_stay_out_of_dumps() {
    use crate::repo::UnfinishedBatchRecord;
    use fc_core::{BatchId, JobId};
    let fx = Fx::new();
    {
        let conn = fx.raw();
        assert!(migrate::current_version(&conn).unwrap() >= 401);
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        // A part needs its marker; the state is constrained.
        assert!(conn
            .execute(
                "INSERT INTO unfinished_batch_parts VALUES ('x', 0, '[]')",
                []
            )
            .is_err());
        assert!(conn
            .execute(
                "INSERT INTO unfinished_batches VALUES ('x', NULL, 'done', '{}', 0, 0)",
                []
            )
            .is_err());
    }
    let batch = BatchId(u(0x77));
    let mut marker = UnfinishedBatchRecord {
        batch_id: batch,
        job_id: Some(JobId(u(0x78))),
        state: "started".into(),
        rollback: serde_json::json!({"method": "v1.x", "params": {"since": 3}}),
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
    };
    fx.write(|tx| tx.upsert_unfinished_batch(&marker).unwrap());
    marker.state = "writing".into();
    marker.updated_at = Timestamp(2);
    fx.write(|tx| {
        tx.upsert_unfinished_batch(&marker).unwrap();
        tx.add_unfinished_batch_part(batch, 1, &serde_json::json!([["b"]]))
            .unwrap();
        tx.add_unfinished_batch_part(batch, 0, &serde_json::json!([["a"]]))
            .unwrap();
    });
    fx.read(|r| {
        assert_eq!(r.list_unfinished_batches().unwrap(), vec![marker.clone()]);
        assert_eq!(
            r.unfinished_batch_parts(batch).unwrap(),
            vec![serde_json::json!([["a"]]), serde_json::json!([["b"]])]
        );
        let dump = r.dump_collection().unwrap();
        assert!(!dump
            .tables
            .iter()
            .any(|t| t.name.starts_with("unfinished_batch")));
    });
    assert!(fx.write(|tx| tx.delete_unfinished_batch(batch).unwrap()));
    fx.read(|r| {
        assert!(r.list_unfinished_batches().unwrap().is_empty());
        assert!(r.unfinished_batch_parts(batch).unwrap().is_empty());
    });
    assert!(!fx.write(|tx| tx.delete_unfinished_batch(batch).unwrap()));
}

#[test]
fn review_logs_insert_once_and_read_by_card() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        ensure_home(tx);
        tx.insert_deck(&deck(1, None, "D")).unwrap();
        add_note(tx, &note(1, "a", "b", &[], 1), 1);
        add_note(tx, &note(2, "c", "d", &[], 1), 1);
    });
    let (c1, c2) = (CardId(nid(1).0), CardId(nid(2).0));
    let rows = vec![review(2, c1, 20), review(1, c1, 10), review(3, c2, 5)];
    assert_eq!(fx.write(|tx| tx.insert_review_logs(&rows).unwrap()), 3);
    assert_eq!(fx.write(|tx| tx.insert_review_logs(&rows).unwrap()), 0);
    fx.read(|r| {
        let got = r.review_logs_for_cards(&[c1]).unwrap();
        assert_eq!(got, vec![review(1, c1, 10), review(2, c1, 20)]);
        assert_eq!(r.review_logs_for_cards(&[c1, c2]).unwrap().len(), 3);
    });
}

#[test]
fn dump_then_load_is_bit_exact() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        ensure_home(tx);
        tx.insert_deck(&deck(1, None, "Deck")).unwrap();
        add_note(tx, &note(1, "front <b>x</b>", "back", &["t1"], 1), 1);
        let mut c = tx.get_card(CardId(nid(1).0)).unwrap().unwrap();
        c.scheduling.stability = Some(0.1 + 0.2);
        tx.update_card(&c).unwrap();
        tx.insert_review_logs(&[review(1, c.id, 10)]).unwrap();
        tx.insert_media(&media(&[0, 255, 7])).unwrap();
        tx.write_media_file(&media_hash(&[0, 255, 7]), &[0, 255, 7])
            .unwrap();
    });
    let before = fx.read(|r| r.dump_collection().unwrap());
    // v1.4 Z1: counting in place agrees with the dump (filtered tables included).
    let counted = fx.read(|r| r.dump_row_counts().unwrap());
    let dumped: std::collections::BTreeMap<String, u64> = before
        .tables
        .iter()
        .map(|t| (t.name.clone(), t.rows.len() as u64))
        .collect();
    assert_eq!(counted, dumped);
    assert!(counted["review_log"] == 1 && counted["notes"] == 1);
    assert!(before.schema_versions.contains(&1) && before.schema_versions.contains(&402));
    let names: Vec<&str> = before.tables.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"notes") && names.contains(&"notes_fts") && names.contains(&"oplog"));
    assert!(!names.iter().any(|n| n.starts_with("notes_fts_")));
    assert!(names.contains(&"media"));
    let cards = before.tables.iter().find(|t| t.name == "cards").unwrap();
    assert_eq!(cards.columns[0], "rowid");
    let stab = cards.columns.iter().position(|c| c == "stability").unwrap();
    assert_eq!(
        cards.rows[0][stab],
        SqlValue::Real("0.30000000000000004".into())
    );

    // JSON round trip, then wipe and load.
    let json = serde_json::to_string(&before).unwrap();
    let dump = serde_json::from_str(&json).unwrap();
    fx.write(|tx| {
        tx.insert_deck(&deck(2, None, "Other")).unwrap();
    });
    fx.write(|tx| tx.load_collection(&dump).unwrap());
    let after = fx.read(|r| r.dump_collection().unwrap());
    assert_eq!(after, before);
    // Media files are not part of the dump; the content-addressed store keeps them.
    fx.read(|r| {
        let h = media_hash(&[0, 255, 7]);
        assert_eq!(
            r.read_media_file(&h).unwrap().as_deref(),
            Some(&[0u8, 255, 7][..])
        );
    });
    // FTS still finds the note.
    let hits: i64 = fx
        .raw()
        .query_row(
            "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'front'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
}

#[test]
fn load_rejects_unknown_tables_and_columns_without_touching_data() {
    let fx = Fx::seeded();
    let mut dump = fx.read(|r| r.dump_collection().unwrap());
    let good = dump.clone();
    dump.tables[0].columns.push("nope".into());
    let mut tx = fx.store.write().unwrap();
    assert!(tx.load_collection(&dump).is_err());
    drop(tx);
    let mut bad_table = good.clone();
    bad_table.tables.push(crate::repo::TableDump {
        name: "nope".into(),
        columns: vec![],
        rows: vec![],
    });
    let mut tx = fx.store.write().unwrap();
    assert!(tx.load_collection(&bad_table).is_err());
    drop(tx);
    assert_eq!(fx.read(|r| r.dump_collection().unwrap()), good);
}

#[test]
fn purge_rows_removes_an_import_without_a_trace() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        ensure_home(tx);
        tx.insert_deck(&deck(1, None, "Keep")).unwrap();
        add_note(tx, &note(1, "keep", "me", &["old"], 1), 1);
    });
    let before = fx.read(|r| r.dump_collection().unwrap());
    // An "import": a deck, two notes (one reusing tag `old`, one with a new tag), review
    // history, a classification with a value.
    fx.write(|tx| {
        tx.insert_deck(&deck(2, None, "Imported")).unwrap();
        add_note(tx, &note(2, "new one", "x", &["old", "fresh"], 50), 2);
        add_note(tx, &note(3, "new two", "y", &[], 50), 2);
        tx.insert_review_logs(&[review(1, CardId(nid(2).0), 60)])
            .unwrap();
    });
    let after_import = fx.read(|r| r.dump_collection().unwrap());
    assert_ne!(after_import, before);
    fx.write(|tx| {
        tx.purge_rows(&crate::repo::PurgeRows {
            decks: vec![did(2)],
            notetypes: vec![],
            notes: vec![nid(2), nid(3)],
            cards: vec![CardId(nid(2).0), CardId(nid(3).0)],
            attribute_defs: vec![],
            tags_created_since: Some(Timestamp(50)),
        })
        .unwrap()
    });
    let after = fx.read(|r| r.dump_collection().unwrap());
    assert_eq!(after, before);
    let hits: i64 = fx
        .raw()
        .query_row(
            "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'new'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 0);
}

#[test]
fn upgrade_dump_runs_later_data_migrations_on_restored_rows() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        ensure_home(tx);
        tx.insert_deck(&deck(1, None, "Deck")).unwrap();
        add_note(tx, &note(1, "Işık", "back", &[], 1), 1);
    });
    let old = fx.read(|r| r.dump_collection().unwrap());
    // Same schema: unchanged.
    assert_eq!(crate::upgrade_dump(old.clone()).unwrap(), old);

    // A later build with a data migration (like v0.4's 0300 re-folding search text; the
    // index is written folded since v0.4, so this one just marks the restored rows).
    let fts_col = |d: &crate::repo::CollectionDump| {
        let fts = d.tables.iter().find(|t| t.name == "notes_fts").unwrap();
        let col = fts.columns.iter().position(|c| c == "sort_field").unwrap();
        fts.rows[0][col].clone()
    };
    let SqlValue::Text(folded) = fts_col(&old) else {
        panic!("sort_field is text")
    };
    assert_eq!(folded, "isik", "the search index is written folded");
    let mut newer: Vec<migrate::Migration> = migrate::MIGRATIONS.to_vec();
    // One past the newest real migration (later tracks keep adding theirs).
    let later = newer.iter().map(|m| m.version).max().unwrap() + 1;
    newer.push(migrate::Migration {
        version: later,
        name: "test_fold",
        sql: "UPDATE notes_fts SET sort_field = sort_field || '!';
              CREATE TABLE fold_marker (n INTEGER NOT NULL);
              INSERT INTO fold_marker SELECT count(*) FROM notes;",
    });
    let up = crate::upgrade_dump_with(old.clone(), &newer).unwrap();
    assert_eq!(*up.schema_versions.last().unwrap(), later);
    assert_eq!(fts_col(&up), SqlValue::Text("isik!".into()));
    let marker = up.tables.iter().find(|t| t.name == "fold_marker").unwrap();
    assert_eq!(marker.rows[0].last(), Some(&SqlValue::Integer(1)));
    // Unknown versions are refused.
    let mut alien = old;
    alien.schema_versions.push(9999);
    assert!(crate::upgrade_dump(alien).is_err());
}

/// `fit_dump_to_schema` cuts a current dump to an older schema's tables and columns
/// generically (no per-track list), and the result restores forward to the same rows.
#[test]
fn fit_dump_to_schema_drops_later_tables_and_columns() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        ensure_home(tx);
        tx.insert_deck(&deck(1, None, "D")).unwrap();
        add_note(tx, &note(1, "a", "b", &["t"], 1), 1);
        tx.insert_review_logs(&[review(1, CardId(nid(1).0), 60)])
            .unwrap();
    });
    let dump = fx.read(|r| r.dump_collection().unwrap());
    assert!(dump.tables.iter().any(|t| t.name == "exam_dates"));
    let v05: Vec<u32> = dump
        .schema_versions
        .iter()
        .copied()
        .filter(|v| *v < 500)
        .collect();
    let old = crate::fit_dump_to_schema(dump.clone(), &v05).unwrap();
    assert_eq!(old.schema_versions, v05);
    assert!(!old.tables.iter().any(|t| t.name == "exam_dates"));
    for t in &old.tables {
        assert!(
            t.rows.iter().all(|r| r.len() == t.columns.len()),
            "{}",
            t.name
        );
    }
    // Every table the older schema has is kept (same rows).
    let names = |d: &crate::repo::CollectionDump| -> Vec<String> {
        d.tables.iter().map(|t| t.name.clone()).collect()
    };
    assert!(names(&old).iter().all(|n| names(&dump).contains(n)));
    // Exactly the tables that migrations from 0500 on create are dropped (v0.6 `exam_dates`,
    // and whatever later tracks add, e.g. v0.8 `mcp_servers`).
    let later: Vec<String> = names(&dump)
        .into_iter()
        .filter(|n| {
            migrate::MIGRATIONS
                .iter()
                .any(|m| m.version >= 500 && m.sql.contains(&format!("CREATE TABLE {n} (")))
        })
        .collect();
    assert!(later.contains(&"exam_dates".to_owned()), "{later:?}");
    let dropped: Vec<String> = names(&dump)
        .into_iter()
        .filter(|n| !names(&old).contains(n))
        .collect();
    assert_eq!(dropped, later);
    // Restoring it forward gives the current dump back.
    assert_eq!(crate::upgrade_dump(old).unwrap(), dump);

    // Columns too: a table a later migration widened loses the new columns and their values.
    let mut widened = dump.clone();
    let notes = widened
        .tables
        .iter_mut()
        .find(|t| t.name == "notes")
        .unwrap();
    notes.columns.push("later_column".into());
    for r in &mut notes.rows {
        r.push(SqlValue::Integer(1));
    }
    let cut = crate::fit_dump_to_schema(widened, &dump.schema_versions).unwrap();
    assert_eq!(cut, dump);
    assert!(crate::fit_dump_to_schema(dump, &[1, 9_999]).is_err());
}

// ------------------------------------------------------------ v1.4 Z2: streamed dump/load

use crate::repo::{CollectionDump, DumpStream, DumpStreamError, DumpSummary, TableDump, CHUNK};

/// A collection with more than two chunks of review history and a media row.
fn streamed_fixture() -> Fx {
    let fx = Fx::seeded();
    fx.write(|tx| {
        ensure_home(tx);
        tx.insert_deck(&deck(1, None, "Deck")).unwrap();
        add_note(tx, &note(1, "front <b>x</b>", "back", &["t1"], 1), 1);
        let c = CardId(nid(1).0);
        let rows: Vec<_> = (0..(CHUNK as u128 * 2 + 7))
            .map(|i| review(i, c, i as i64))
            .collect();
        tx.insert_review_logs(&rows).unwrap();
        tx.insert_media(&media(&[0, 255, 7])).unwrap();
    });
    fx
}

fn streamed(r: Result<DumpStream, DumpStreamError>) -> DumpSummary {
    match r {
        Ok(DumpStream::Streamed(s)) => s,
        Ok(DumpStream::Whole(_)) => panic!("read whole"),
        Err(e) => panic!("{e}"),
    }
}

/// The streamed dump writes exactly the bytes `serde_json` makes of the in-memory dump, and
/// loading them back streamed is `load_collection`.
#[test]
fn streamed_dump_is_byte_identical_and_loads_like_load_collection() {
    let fx = streamed_fixture();
    let before = fx.read(|r| r.dump_collection().unwrap());
    let mut json = Vec::new();
    let summary = fx.read(|r| r.dump_collection_json(&mut json).unwrap());
    assert_eq!(json, serde_json::to_vec(&before).unwrap());
    assert_eq!(summary, DumpSummary::of(&before));
    assert_eq!(summary.counts, fx.read(|r| r.dump_row_counts().unwrap()));
    assert_eq!(summary.counts["review_log"], CHUNK as u64 * 2 + 7);

    // The reference: the in-memory load of the same JSON into another collection.
    let reference = Fx::seeded();
    reference.write(|tx| {
        tx.load_collection(&serde_json::from_slice(&json).unwrap())
            .unwrap()
    });
    let reference = reference.read(|r| r.dump_collection().unwrap());
    assert_eq!(reference, before);

    fx.write(|tx| tx.insert_deck(&deck(2, None, "Other")).unwrap());
    let mut chunks = 0;
    let loaded = fx.write(|tx| {
        streamed(tx.load_collection_json(&mut &json[..], &mut |t| {
            assert!(!t.rows.is_empty() && t.rows.len() <= CHUNK);
            chunks += 1;
            Ok(())
        }))
    });
    assert_eq!(loaded, summary);
    assert!(chunks >= 3, "the history came in chunks");
    assert_eq!(fx.read(|r| r.dump_collection().unwrap()), before);
    // The search index was rebuilt from the loaded notes.
    let hits: i64 = fx
        .raw()
        .query_row(
            "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'front'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
    // A scan reads the same counts without keeping anything.
    let scanned = streamed(crate::repo::scan_dump_json(&mut &json[..], &mut |_| Ok(())));
    assert_eq!(scanned, summary);
}

/// The hook sees every chunk and may change or drop rows; counts are what was kept.
#[test]
fn streamed_load_applies_the_row_hook() {
    let fx = streamed_fixture();
    let mut json = Vec::new();
    fx.read(|r| r.dump_collection_json(&mut json).unwrap());
    let mut keep_one = |t: &mut TableDump| {
        if t.name == "review_log" {
            t.rows.truncate(1);
        }
        if t.name == "decks" {
            let i = t.columns.iter().position(|c| c == "name").unwrap();
            for r in &mut t.rows {
                if let SqlValue::Text(name) = &mut r[i] {
                    name.push_str(" (restored)");
                }
            }
        }
        Ok(())
    };
    let scanned = streamed(crate::repo::scan_dump_json(&mut &json[..], &mut keep_one));
    let loaded = fx.write(|tx| streamed(tx.load_collection_json(&mut &json[..], &mut keep_one)));
    assert_eq!(loaded, scanned);
    assert_eq!(loaded.counts["review_log"], 3, "one per chunk");
    let after = fx.read(|r| r.dump_collection().unwrap());
    assert_eq!(after.row_counts(), loaded.counts);
    let decks = after.tables.iter().find(|t| t.name == "decks").unwrap();
    let i = decks.columns.iter().position(|c| c == "name").unwrap();
    assert!(decks
        .rows
        .iter()
        .all(|r| matches!(&r[i], SqlValue::Text(n) if n.ends_with(" (restored)"))));
    // A hook error stops the load.
    let mut tx = fx.store.write().unwrap();
    let r = tx.load_collection_json(&mut &json[..], &mut |_| Err(crate::StorageError::Busy));
    assert!(matches!(
        r,
        Err(DumpStreamError::Storage(crate::StorageError::Busy))
    ));
}

/// Field order does not matter (a hand-made file may list `tables` before `schema_versions`,
/// or `rows` before `columns`): the same rows load. An older schema is handed back whole,
/// nothing loaded.
#[test]
fn streamed_load_takes_any_field_order_and_hands_older_schemas_back_whole() {
    let fx = streamed_fixture();
    let before = fx.read(|r| r.dump_collection().unwrap());
    let reordered = serde_json::json!({
        "tables": before.tables.iter().map(|t| serde_json::json!({
            "rows": t.rows, "columns": t.columns, "extra": 1, "name": t.name,
        })).collect::<Vec<_>>(),
        "note": "ignored",
        "schema_versions": before.schema_versions,
    });
    let json = serde_json::to_vec(&reordered).unwrap();
    fx.write(|tx| tx.insert_deck(&deck(2, None, "Other")).unwrap());
    let loaded = fx.write(|tx| streamed(tx.load_collection_json(&mut &json[..], &mut |_| Ok(()))));
    assert_eq!(loaded, DumpSummary::of(&before));
    assert_eq!(fx.read(|r| r.dump_collection().unwrap()), before);
    // Array form (serde accepts structs as sequences).
    let tuple = serde_json::json!([
        before.schema_versions,
        before
            .tables
            .iter()
            .map(|t| serde_json::json!([t.name, t.columns, t.rows]))
            .collect::<Vec<_>>(),
    ]);
    let json = serde_json::to_vec(&tuple).unwrap();
    fx.write(|tx| tx.insert_deck(&deck(2, None, "Other")).unwrap());
    fx.write(|tx| streamed(tx.load_collection_json(&mut &json[..], &mut |_| Ok(()))));
    assert_eq!(fx.read(|r| r.dump_collection().unwrap()), before);

    let v05: Vec<u32> = before
        .schema_versions
        .iter()
        .copied()
        .filter(|v| *v < 500)
        .collect();
    let old = crate::fit_dump_to_schema(before.clone(), &v05).unwrap();
    let json = serde_json::to_vec(&old).unwrap();
    fx.write(|tx| tx.insert_deck(&deck(2, None, "Other")).unwrap());
    let changed = fx.read(|r| r.dump_collection().unwrap());
    let got = fx.write(|tx| {
        tx.load_collection_json(&mut &json[..], &mut |_| Ok(()))
            .unwrap()
    });
    let DumpStream::Whole(whole) = got else {
        panic!("an older schema is not streamed")
    };
    assert_eq!(whole, old);
    assert_eq!(
        fx.read(|r| r.dump_collection().unwrap()),
        changed,
        "nothing loaded"
    );
    // …and the scan says the same.
    let scanned = crate::repo::scan_dump_json(&mut &json[..], &mut |_| Ok(())).unwrap();
    assert!(matches!(scanned, DumpStream::Whole(d) if d == old));
}

/// Damage is refused as `load_collection` refuses it; dropping the transaction leaves the
/// collection as it was.
#[test]
fn streamed_load_refuses_damage_and_rolls_back() {
    let fx = streamed_fixture();
    let good = fx.read(|r| r.dump_collection().unwrap());
    let refuse = |dump: serde_json::Value| {
        let json = serde_json::to_vec(&dump).unwrap();
        let mut tx = fx.store.write().unwrap();
        let r = tx.load_collection_json(&mut &json[..], &mut |_| Ok(()));
        drop(tx);
        assert_eq!(fx.read(|r| r.dump_collection().unwrap()), good);
        r.expect_err("refused")
    };
    let as_json = |d: &CollectionDump| serde_json::to_value(d).unwrap();
    let mut d = good.clone();
    d.tables.push(TableDump {
        name: "nope".into(),
        columns: vec![],
        rows: vec![],
    });
    let e = refuse(as_json(&d));
    assert!(e.to_string().contains("unknown table nope"), "{e}");
    let mut d = good.clone();
    let reviews = d
        .tables
        .iter_mut()
        .find(|t| t.name == "review_log")
        .unwrap();
    reviews.columns.push("nope".into());
    let e = refuse(as_json(&d));
    assert!(
        e.to_string().contains("unknown column review_log.nope"),
        "{e}"
    );
    let mut d = good.clone();
    let reviews = d
        .tables
        .iter_mut()
        .find(|t| t.name == "review_log")
        .unwrap();
    reviews.rows[CHUNK + 3].pop();
    let e = refuse(as_json(&d));
    assert!(matches!(e, DumpStreamError::Storage(_)));
    assert!(e.to_string().contains("ragged rows in review_log"), "{e}");
    // Not a dump: a JSON error.
    let mut v = as_json(&good);
    v.as_object_mut().unwrap().remove("tables");
    assert!(matches!(refuse(v), DumpStreamError::Json(e) if e.to_string().contains("tables")));
    let json = serde_json::to_vec(&good).unwrap();
    let mut tx = fx.store.write().unwrap();
    let r = tx.load_collection_json(&mut &json[..json.len() / 2], &mut |_| Ok(()));
    assert!(matches!(r, Err(DumpStreamError::Json(_))));
    drop(tx);
    let mut trailing = json.clone();
    trailing.extend_from_slice(b" x");
    let mut tx = fx.store.write().unwrap();
    let r = tx.load_collection_json(&mut &trailing[..], &mut |_| Ok(()));
    assert!(matches!(r, Err(DumpStreamError::Json(_))));
    drop(tx);
    assert_eq!(fx.read(|r| r.dump_collection().unwrap()), good);
}
