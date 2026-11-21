//! `subtitle_keys`, `subtitle_captures`, `subtitle_uploads` (v1.2): migration `1200` and the
//! repository round trips.

use super::{u, Fx};
use crate::migrate::MIGRATIONS;
use crate::repo::{SubtitleCaptureRecord, SubtitleKeyRecord, SubtitleUploadRecord};
use crate::store::{SqliteStore, Store};
use crate::StorageError;
use fc_core::{CardId, NoteId, SubtitleKeyId, Timestamp};

fn key(n: u128, hash: &str, at: i64) -> SubtitleKeyRecord {
    SubtitleKeyRecord {
        id: SubtitleKeyId(u(n)),
        name: format!("Chrome {n}"),
        prefix: "sk_fc_abcd".into(),
        token_hash: hash.into(),
        created_at: Timestamp(at),
        last_used_at: None,
        revoked_at: None,
    }
}

fn capture(id: &str, lang: &str, lemma: &str, sentence: &str, at: i64) -> SubtitleCaptureRecord {
    SubtitleCaptureRecord {
        capture_id: id.into(),
        dedup_key: format!("{lang}|{lemma}|{sentence}"),
        lang: lang.into(),
        lemma: lemma.into(),
        note_id: NoteId(u(at as u128)),
        card_id: CardId(u(1000 + at as u128)),
        key_id: Some(SubtitleKeyId(u(1))),
        created_at: Timestamp(at),
    }
}

fn upload(id: &str, capture: &str, kind: &str, at: i64) -> SubtitleUploadRecord {
    SubtitleUploadRecord {
        id: id.into(),
        capture_id: capture.into(),
        kind: kind.into(),
        media_hash: "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08".into(),
        mime: "image/jpeg".into(),
        created_at: Timestamp(at),
    }
}

/// An existing v1.0 collection gets the three tables; its data is untouched.
#[test]
fn migration_1200_adds_the_subtitle_tables_to_an_existing_collection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 1200)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &before).unwrap());
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        let has = |name: &str| -> bool {
            conn.query_row(
                "SELECT count(*) > 0 FROM sqlite_master WHERE name = ?1",
                [name],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert!(!has("subtitle_keys"));
        conn.execute(
            "INSERT INTO settings (scope, key, value, updated_at) VALUES ('app', 'app.locale', '\"tr\"', 1)",
            [],
        )
        .unwrap();
    }

    let store = SqliteStore::open(&path).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    for name in [
        "subtitle_keys",
        "subtitle_captures",
        "subtitle_uploads",
        "subtitle_keys_token",
        "subtitle_captures_dedup",
        "subtitle_captures_lemma",
        "subtitle_uploads_capture",
        "subtitle_uploads_created",
    ] {
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = ?1",
                [name],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "{name}");
    }
    let locale: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'app.locale'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(locale, "\"tr\"");
    // The kind check holds.
    assert!(conn
        .execute(
            "INSERT INTO subtitle_uploads (id, capture_id, kind, media_hash, mime, created_at)
             VALUES ('up_1', 'c', 'video', 'h', 'video/mp4', 1)",
            [],
        )
        .is_err());
    assert_eq!(
        store.read().unwrap().subtitle_capture_stats().unwrap(),
        (0, None)
    );
}

#[test]
fn keys_round_trip_and_hashes_are_unique() {
    let fx = Fx::new();
    let a = key(1, "aaa", 1);
    let b = key(2, "bbb", 5);
    fx.write(|tx| {
        tx.upsert_subtitle_key(&a).unwrap();
        tx.upsert_subtitle_key(&b).unwrap();
    });
    assert_eq!(
        fx.read(|r| r.list_subtitle_keys().unwrap()),
        vec![b.clone(), a.clone()]
    );
    assert_eq!(
        fx.read(|r| r.subtitle_key_by_token_hash("aaa").unwrap()),
        Some(a.clone())
    );
    let revoked = SubtitleKeyRecord {
        last_used_at: Some(Timestamp(7)),
        revoked_at: Some(Timestamp(8)),
        ..a.clone()
    };
    fx.write(|tx| tx.upsert_subtitle_key(&revoked).unwrap());
    assert_eq!(
        fx.read(|r| r.get_subtitle_key(a.id).unwrap()),
        Some(revoked)
    );
    let mut tx = fx.store.write().unwrap();
    assert!(matches!(
        tx.upsert_subtitle_key(&key(3, "bbb", 9)),
        Err(StorageError::Constraint { constraint }) if constraint == "subtitle_keys_token"
    ));
}

#[test]
fn captures_are_unique_by_id_and_dedup_key_and_page_by_lemma() {
    let fx = Fx::new();
    let rows = [
        capture("c1", "en", "key", "s1", 1),
        capture("c2", "en", "key", "s2", 2),
        capture("c3", "en", "pick up", "s1", 3),
        capture("c4", "de", "haus", "s1", 4),
    ];
    fx.write(|tx| {
        for r in &rows {
            tx.insert_subtitle_capture(r).unwrap();
        }
    });
    assert_eq!(
        fx.read(|r| r.get_subtitle_capture("c2").unwrap()),
        Some(rows[1].clone())
    );
    assert_eq!(
        fx.read(|r| r.subtitle_capture_by_dedup_key("en|pick up|s1").unwrap()),
        Some(rows[2].clone())
    );
    assert_eq!(
        fx.read(|r| r.subtitle_capture_stats().unwrap()),
        (4, Some(Timestamp(4)))
    );
    for dup in [
        capture("c1", "en", "other", "x", 9),
        capture("c9", "en", "key", "s1", 9),
    ] {
        let mut tx = fx.store.write().unwrap();
        assert!(matches!(
            tx.insert_subtitle_capture(&dup),
            Err(StorageError::Constraint { constraint }) if constraint == "subtitle_captures"
        ));
    }

    let pairs = |v: &[(&str, &str)]| -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    };
    assert_eq!(
        fx.read(|r| r.subtitle_lemma_page(None, None, 2).unwrap()),
        pairs(&[("de", "haus"), ("en", "key")])
    );
    assert_eq!(
        fx.read(|r| r
            .subtitle_lemma_page(None, Some(("en", "key")), 10)
            .unwrap()),
        pairs(&[("en", "pick up")])
    );
    assert_eq!(
        fx.read(|r| r.subtitle_lemma_page(Some("en"), None, 10).unwrap()),
        pairs(&[("en", "key"), ("en", "pick up")])
    );
    let of_key = fx.read(|r| r.subtitle_captures_of(&pairs(&[("en", "key")])).unwrap());
    assert_eq!(of_key, vec![rows[0].clone(), rows[1].clone()]);

    assert!(fx.write(|tx| tx.delete_subtitle_capture("c1").unwrap()));
    assert!(!fx.write(|tx| tx.delete_subtitle_capture("c1").unwrap()));
    assert_eq!(fx.read(|r| r.subtitle_capture_stats().unwrap()).0, 3);
}

#[test]
fn uploads_are_one_per_capture_and_kind_and_expire() {
    let fx = Fx::new();
    fx.write(|tx| {
        tx.upsert_subtitle_upload(&upload("up_1", "c1", "screenshot", 10))
            .unwrap();
        tx.upsert_subtitle_upload(&upload("up_2", "c1", "audio", 20))
            .unwrap();
    });
    assert_eq!(
        fx.read(|r| r.subtitle_upload_for("c1", "audio").unwrap())
            .map(|u| u.id),
        Some("up_2".to_owned())
    );
    // The same capture + kind again replaces the row.
    fx.write(|tx| {
        tx.upsert_subtitle_upload(&upload("up_3", "c1", "audio", 30))
            .unwrap()
    });
    assert_eq!(fx.read(|r| r.get_subtitle_upload("up_2").unwrap()), None);
    assert_eq!(
        fx.read(|r| r.get_subtitle_upload("up_3").unwrap()),
        Some(upload("up_3", "c1", "audio", 30))
    );
    let gone = fx.write(|tx| tx.delete_subtitle_uploads_before(Timestamp(25)).unwrap());
    assert_eq!(
        gone.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(),
        ["up_1"]
    );
    assert_eq!(fx.read(|r| r.get_subtitle_upload("up_1").unwrap()), None);
    assert!(fx
        .read(|r| r.get_subtitle_upload("up_3").unwrap())
        .is_some());
}
