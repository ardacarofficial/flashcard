//! v1.4 W4 migration `1420_media_origins`: the film's own media is backfilled as `original`,
//! nothing else is guessed; the TTS cache finds a stored file by its key only.

use crate::migrate::MIGRATIONS;
use crate::repo::{MediaOriginRecord, TtsCacheKey};
use crate::store::{SqliteStore, Store};
use fc_core::content::media::MediaOrigin;
use fc_core::Timestamp;
use rusqlite::Connection;

fn h(c: char) -> String {
    std::iter::repeat_n(c, 64).collect()
}

#[test]
fn migration_1420_backfills_the_film_media_as_original() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let upto = MIGRATIONS.iter().position(|m| m.version >= 1420).unwrap();
    {
        let old = SqliteStore::open_with_migrations(&path, &MIGRATIONS[..upto]).unwrap();
        drop(old);
        let conn = Connection::open(&path).unwrap();
        for (hash, mime, name) in [
            (h('a'), "audio/wav", Some("Subtitle-audio-cap1.wav")),
            (h('b'), "image/jpeg", None),
            (h('c'), "audio/mpeg", Some("my-recording.mp3")),
        ] {
            conn.execute(
                "INSERT INTO media (hash, mime, size, original_name, ref_count, created_at)
                 VALUES (?1, ?2, 1, ?3, 0, 5)",
                rusqlite::params![hash, mime, name],
            )
            .unwrap();
        }
        // `b` came in as an upload (re-stored later without its name).
        conn.execute(
            "INSERT INTO subtitle_uploads (id, capture_id, kind, media_hash, mime, created_at)
             VALUES ('up_1', 'cap2', 'screenshot', ?1, 'image/jpeg', 5)",
            [h('b')],
        )
        .unwrap();
    }
    let store = SqliteStore::open(&path).unwrap();
    let r = store.read().unwrap();
    let rows = r.media_origins_of(&[h('a'), h('b'), h('c')]).unwrap();
    let got: Vec<(String, MediaOrigin)> = rows.iter().map(|r| (r.hash.clone(), r.origin)).collect();
    assert_eq!(
        got,
        vec![
            (h('a'), MediaOrigin::Original),
            (h('b'), MediaOrigin::Original)
        ],
        "the user's own file is not guessed"
    );
    assert_eq!(rows[0].created_at, Timestamp(5));
}

#[test]
fn the_tts_cache_needs_the_whole_key_and_the_file() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mut w = store.write().unwrap();
    let rec = MediaOriginRecord {
        hash: h('d'),
        origin: MediaOrigin::Tts,
        provider: "p".into(),
        voice: String::new(),
        language: "en".into(),
        rate_milli: 1000,
        text_hash: h('e'),
        source: String::new(),
        created_at: Timestamp(1),
    };
    w.put_media_origin(&rec).unwrap();
    let key = TtsCacheKey {
        provider: "p".into(),
        language: "en".into(),
        voice: String::new(),
        rate_milli: 1000,
        text_hash: h('e'),
    };
    // No media row yet (the file was forgotten): no hit.
    assert_eq!(w.find_tts_media(&key).unwrap(), None);
    w.insert_media(&crate::repo::MediaRecord {
        hash: h('d'),
        mime: "audio/wav".into(),
        size: 1,
        original_name: None,
        ref_count: 0,
        created_at: Timestamp(1),
    })
    .unwrap();
    assert_eq!(w.find_tts_media(&key).unwrap(), Some(h('d')));
    for k in [
        TtsCacheKey {
            rate_milli: 850,
            ..key.clone()
        },
        TtsCacheKey {
            voice: "v".into(),
            ..key.clone()
        },
        TtsCacheKey {
            language: "de".into(),
            ..key.clone()
        },
        TtsCacheKey {
            provider: "q".into(),
            ..key.clone()
        },
    ] {
        assert_eq!(w.find_tts_media(&k).unwrap(), None, "{k:?}");
    }
    // A clip of the same file is not a TTS entry.
    w.put_media_origin(&MediaOriginRecord {
        origin: MediaOrigin::Clip,
        ..rec
    })
    .unwrap();
    assert_eq!(w.find_tts_media(&key).unwrap(), None);
}

/// A backup (dump) carries the origins and a restore (load, as the verify-by-restore does)
/// gives them back unchanged.
#[test]
fn a_backup_restore_keeps_origins() {
    let src = SqliteStore::open_in_memory().unwrap();
    let rec = MediaOriginRecord {
        hash: h('f'),
        origin: MediaOrigin::Tts,
        provider: "p".into(),
        voice: "v".into(),
        language: "en".into(),
        rate_milli: 850,
        text_hash: h('e'),
        source: String::new(),
        created_at: Timestamp(3),
    };
    {
        let mut w = src.write().unwrap();
        w.put_media_origin(&rec).unwrap();
        w.put_media_origin(&MediaOriginRecord {
            hash: h('g'),
            origin: MediaOrigin::Clip,
            source: "show.mkv".into(),
            ..rec.clone()
        })
        .unwrap();
        w.commit().unwrap();
    }
    let dump = src.read().unwrap().dump_collection().unwrap();
    let dst = SqliteStore::open_in_memory().unwrap();
    let mut w = dst.write().unwrap();
    w.load_collection(&dump).unwrap();
    let back = w.media_origins_of(&[h('f'), h('g')]).unwrap();
    assert_eq!(back.len(), 2);
    assert_eq!(back[0], rec);
    assert_eq!(
        (back[1].origin, back[1].source.as_str()),
        (MediaOrigin::Clip, "show.mkv")
    );
}
