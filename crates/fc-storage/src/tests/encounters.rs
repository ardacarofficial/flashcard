//! v1.4 W1 migration `1490`: Subtitle captures become encounters (source columns, backfill,
//! one primary per note), `attached` capture attempts, and the Subtitle note type's
//! `examples` field.

use super::{did, nid, put_note, HOME};
use crate::migrate::MIGRATIONS;
use crate::repo::{CaptureLogRecord, EncounterFacts, SubtitleCaptureRecord};
use crate::store::{SqliteStore, Store};
use fc_core::content::builtin;
use fc_core::i18n::Locale;
use fc_core::note::{note_checksum, sort_field_text, FieldValues, Note};
use fc_core::notetype::{FieldKey, NoteType};
use fc_core::{CardId, Timestamp};
use rusqlite::Connection;

const HASH: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

/// The Subtitle note type as v1.3 shipped it: today's minus the `examples` field and part.
pub(crate) fn subtitle_v1_3(locale: Locale) -> NoteType {
    let mut nt = builtin::subtitle(Timestamp(1), locale);
    nt.fields.retain(|f| f.key.0 != "examples");
    let part =
        "{{#examples}}<div class=\"fc-part fc-part-examples\">{{examples}}</div>{{/examples}}";
    assert!(nt.templates[0].back.contains(part));
    nt.templates[0].back = nt.templates[0].back.replace(part, "");
    nt
}

fn subtitle_note(nt: &NoteType, n: u128, fields: &[(&str, &str)]) -> Note {
    let fields: FieldValues = fields
        .iter()
        .map(|(k, v)| (FieldKey::new(*k), (*v).to_owned()))
        .collect();
    Note {
        id: nid(n),
        notetype_id: nt.id,
        sort_field: sort_field_text(nt, &fields),
        checksum: note_checksum(nt, &fields),
        fields,
        tags: vec![],
        created_at: Timestamp(5),
        updated_at: Timestamp(5),
        deleted_at: None,
        home_deck_id: did(HOME),
    }
}

/// A v1.3 collection (schema below 1490) with a Subtitle note made from a capture, opened by
/// this build.
#[test]
fn migration_1490_backfills_encounters_and_keeps_cards_rendering_the_same() {
    for locale in [Locale::En, Locale::Tr] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collection.db");
        let upto = MIGRATIONS.iter().position(|m| m.version >= 1490).unwrap();
        let v13 = subtitle_v1_3(locale);
        let note_fields = [
            ("word", "key"),
            ("surface", "keys"),
            ("meaning", "anahtar"),
            ("sentence", "my <b>keys</b>."),
            ("translation", "anahtarlarım."),
            ("image", &format!("media:{HASH}") as &str),
            ("source", "Show · 0:01"),
        ];
        let note = subtitle_note(&v13, 7, &note_fields);
        {
            let old = SqliteStore::open_with_migrations(&path, &MIGRATIONS[..upto]).unwrap();
            let mut tx = old.write().unwrap();
            tx.upsert_notetype(&v13).unwrap();
            put_note(&mut *tx, &note, "key").unwrap();
            tx.commit().unwrap();
            let conn = Connection::open(&path).unwrap();
            for (id, at) in [("cap-b", 20), ("cap-a", 10)] {
                conn.execute(
                    "INSERT INTO subtitle_captures (capture_id, dedup_key, lang, lemma, note_id,
                         card_id, key_id, created_at)
                     VALUES (?1, ?1, 'en', 'key', ?2, ?3, NULL, ?4)",
                    rusqlite::params![id, note.id.to_string(), CardId(note.id.0).to_string(), at],
                )
                .unwrap();
            }
            conn.execute(
                "INSERT INTO note_origins (note_id, origin, source, site, video_id, created_at)
                 VALUES (?1, 'subtitle', 'Show', '', '', 10)",
                [note.id.to_string()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO subtitle_capture_log (capture_id, at, result, reason, card_id,
                     duration_ms) VALUES ('cap-a', 10, 'created', NULL, NULL, 3)",
                [],
            )
            .unwrap();
        }
        let store = SqliteStore::open(&path).unwrap();
        let r = store.read().unwrap();
        // The note type: `examples` added, the back shows it, and it is today's definition.
        let nt = r.get_notetype(v13.id).unwrap().unwrap();
        let want = builtin::subtitle(Timestamp(1), locale);
        assert_eq!(nt.fields, want.fields);
        assert_eq!(nt.templates, want.templates);
        assert_eq!(nt.version, v13.version + 1);
        // The note is untouched (its card renders as before: `examples` is empty).
        assert_eq!(r.get_note(note.id).unwrap().unwrap().fields, note.fields);
        // Backfill: facts that exist, NULL for the rest; the earliest capture is primary.
        let rows = r.subtitle_encounters_of_note(note.id).unwrap();
        assert_eq!(
            rows.iter()
                .map(|x| (x.capture_id.as_str(), x.is_primary))
                .collect::<Vec<_>>(),
            [("cap-a", true), ("cap-b", false)]
        );
        let f = &rows[0].facts;
        assert_eq!(f.surface.as_deref(), Some("keys"));
        assert_eq!(f.screenshot_hash.as_deref(), Some(HASH));
        assert_eq!(f.audio_hash, None);
        assert_eq!(f.title.as_deref(), Some("Show"));
        assert_eq!(
            (
                &f.sentence,
                &f.definition,
                &f.video_id,
                &f.kind,
                f.t_start_ms,
                f.t_end_ms
            ),
            (&None, &None, &None, &None, None, None),
            "never invented"
        );
        assert_eq!(f.example_fields.len(), 5);
        assert_eq!(f.example_fields["sentence"], "my <b>keys</b>.");
        assert!(!f.example_fields.contains_key("word"));
        // The diagnostics survived the table copy and accept `attached`.
        assert_eq!(r.recent_capture_log(10).unwrap().len(), 1);
        drop(r);
        let mut tx = store.write().unwrap();
        tx.append_capture_log(
            &CaptureLogRecord {
                seq: 0,
                capture_id: "cap-c".into(),
                at: Timestamp(30),
                result: "attached".into(),
                reason: None,
                card_id: Some(CardId(note.id.0)),
                duration_ms: 1,
            },
            500,
        )
        .unwrap();
        tx.commit().unwrap();
        let conn = Connection::open(&path).unwrap();
        let counts: i64 = conn
            .query_row(
                "SELECT encounters FROM subtitle_encounter_counts WHERE note_id = ?1",
                [note.id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(counts, 2);
    }
}

/// An edited back keeps its text (the field is still added); one primary per note holds.
#[test]
fn migration_1490_leaves_an_edited_back_alone_and_enforces_one_primary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let upto = MIGRATIONS.iter().position(|m| m.version >= 1490).unwrap();
    let mut edited = subtitle_v1_3(Locale::Tr);
    edited.templates[0].back.push_str("<p>benim</p>");
    {
        let old = SqliteStore::open_with_migrations(&path, &MIGRATIONS[..upto]).unwrap();
        let mut tx = old.write().unwrap();
        tx.upsert_notetype(&edited).unwrap();
        tx.commit().unwrap();
    }
    let store = SqliteStore::open(&path).unwrap();
    let nt = store
        .read()
        .unwrap()
        .get_notetype(edited.id)
        .unwrap()
        .unwrap();
    assert_eq!(nt.templates, edited.templates);
    let f = nt.fields.iter().find(|f| f.key.0 == "examples").unwrap();
    assert_eq!(f.name, "Örnekler");

    let row = |id: &str, primary: bool| SubtitleCaptureRecord {
        capture_id: id.into(),
        dedup_key: id.into(),
        lang: "en".into(),
        lemma: "go".into(),
        note_id: nid(1),
        card_id: CardId(nid(1).0),
        key_id: None,
        created_at: Timestamp(1),
        facts: EncounterFacts {
            video_id: Some("v1".into()),
            t_start_ms: Some(5),
            kind: Some("word".into()),
            definition_source: Some("dict".into()),
            ..Default::default()
        },
        is_primary: primary,
        review_note_id: None,
    };
    let mut tx = store.write().unwrap();
    tx.insert_subtitle_capture(&row("a", true)).unwrap();
    assert!(tx.insert_subtitle_capture(&row("b", true)).is_err());
    tx.insert_subtitle_capture(&row("b", false)).unwrap();
    // Switching the primary: clear the old one first.
    assert!(tx.update_subtitle_capture(&row("b", true)).is_err());
    assert!(tx.update_subtitle_capture(&row("a", false)).unwrap());
    assert!(tx.update_subtitle_capture(&row("b", true)).unwrap());
    tx.commit().unwrap();
    let r = store.read().unwrap();
    let rows = r.subtitle_encounters_of_note(nid(1)).unwrap();
    assert_eq!(rows[0].capture_id, "b");
    assert_eq!(rows[0], row("b", true));
    assert_eq!(r.subtitle_encounters_of_video("v1").unwrap().len(), 2);
    assert!(r.subtitle_encounters_in_review().unwrap().is_empty());
}
