//! v1.4 language-learning queries (docs/02 §9.1) through the API, against SQLite (the real
//! indexes and triggers) and the in-memory store:
//!
//! * the owner's example — "the expressions from this episode, that have audio, that I have
//!   seen in two different scenes and am still learning" — is ONE query whose count
//!   (`search.cards` total), list (its rows, `search.ids`) and study session (`study.start`
//!   custom with the query) are the same cards; a note with two clips counts once;
//! * `kelime:went` finds the notes of `go` (inflection), not `goer`-like derivations;
//! * the Subtitle capture and reading mode record the origin the queries read.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::{MemStore, TempSqlite, TestApp};
use base64::Engine as _;
use fc_api::v1::cardtypes::AddMedia;
use fc_api::v1::decks::CreateDeck;
use fc_api::v1::methods::*;
use fc_api::v1::notes::CreateNote;
use fc_api::v1::reading::MarkWords;
use fc_api::v1::search::{QueryText, SearchCards};
use fc_api::v1::settings::SetSetting;
use fc_api::v1::study::{StartSession, StudyFilter, StudyKind, StudyMode};
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::query::NoteOrigin;
use fc_core::reading::WordMark;
use fc_core::settings::SettingScope;
use fc_core::{DeckId, NoteId, Timestamp};
use fc_storage::repo::NoteOriginRecord;
use fc_storage::Store;
use std::collections::BTreeMap;
use std::sync::Arc;

const EPISODE: &str = "The Night Train · S1E2";

fn english(t: &TestApp) -> DeckId {
    let d = t
        .ok::<DecksCreate>(CreateDeck {
            path: "English".into(),
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        })
        .id;
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::Deck(d),
        key: crate::modules::words::LANGUAGE.into(),
        value: serde_json::json!("en"),
    });
    d
}

/// A stored audio file (`subtitle-…` names are clips taken from the video).
fn audio(t: &TestApp, name: &str, salt: u8) -> String {
    let mut bytes = b"ID3\x03\x00\x00\x00\x00\x00\x00".to_vec();
    bytes.push(salt);
    t.ok::<MediaAdd>(AddMedia {
        name: Some(name.into()),
        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
    .reference
}

fn note(t: &TestApp, d: DeckId, term: &str, back: &str) -> NoteId {
    let mut fields = BTreeMap::new();
    fields.insert("front".to_owned(), term.to_owned());
    fields.insert("back".to_owned(), format!("m {back}"));
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: CLASSIC_ID,
        deck_id: d,
        fields,
        tags: vec![],
        attributes: vec![],
        allow_duplicate: Some(true),
    })
    .id
}

/// Records an origin as the Subtitle extension's capture would (the capture path itself is
/// covered by `subtitle_capture_records_its_origin`).
fn origin(t: &TestApp, n: NoteId, o: NoteOrigin, source: &str) {
    let mut tx = t.store.write().unwrap();
    tx.set_note_origin(&NoteOriginRecord {
        note_id: n,
        origin: o,
        source: source.into(),
        site: "youtube".into(),
        video_id: String::new(),
        created_at: Timestamp(0),
    })
    .unwrap();
    tx.commit().unwrap();
}

fn count(t: &TestApp, q: &str) -> u32 {
    t.ok::<SearchCardsMethod>(SearchCards {
        query: q.into(),
        sort: None,
        descending: None,
        notes_only: None,
        offset: None,
        limit: Some(500),
    })
    .total
}

fn notes_of(t: &TestApp, q: &str) -> Vec<NoteId> {
    let mut n = t.ok::<SearchIdsMethod>(QueryText { query: q.into() }).note_ids;
    n.sort();
    n
}

/// Cards a custom study session over `q` holds (count, then every card answered).
fn study(t: &TestApp, q: &str) -> Vec<fc_core::CardId> {
    let s = t.ok::<StudyStart>(StartSession {
        kind: StudyKind::Custom,
        mode: Some(StudyMode::Trial),
        deck_id: None,
        include_subdecks: None,
        filter: Some(StudyFilter {
            attributes: vec![],
            states: None,
            due_only: None,
            limit: None,
            query: Some(q.into()),
            saved_query_id: None,
        }),
        from_session: None,
        new_limit: Some(1000),
        order: None,
        tz_offset_mins: Some(0),
        modes: None,
        scope: None,
    });
    let total = s.counts.new + s.counts.learning + s.counts.review;
    let mut ids = Vec::new();
    let mut c = s;
    while let Some(cur) = c.current.clone() {
        ids.push(cur.card_id);
        c = t.ok::<StudyAnswer>(fc_api::v1::study::AnswerCard {
            session_id: c.id,
            card_id: cur.card_id,
            rating: fc_core::card::Rating::Easy,
            duration_ms: 1000,
            reveal_ms: None,
        });
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len() as u32, total, "{q}: session counts vs cards shown");
    ids
}

fn owners_example(store: Arc<dyn Store>, with_media: bool) {
    let t = TestApp::new(store);
    let en = english(&t);
    let (clip1, clip2, clip3, voice) = if with_media {
        (
            audio(&t, "subtitle-audio-1.mp3", 1),
            audio(&t, "subtitle-audio-2.mp3", 2),
            audio(&t, "subtitle-audio-3.mp3", 3),
            audio(&t, "voice.mp3", 4),
        )
    } else {
        Default::default()
    };
    // Four scenes of "go" (went / go / goes / going), one of "run", an expression.
    let went = note(&t, en, "went", &format!("{clip1} {clip2}")); // two clips: one hit
    let go = note(&t, en, "go", &clip3);
    let goes = note(&t, en, "goes", "");
    let going = note(&t, en, "going", &voice);
    let run = note(&t, en, "run", &clip3);
    let pick = note(&t, en, "picked up", "");
    for n in [went, go, goes, run] {
        origin(&t, n, NoteOrigin::Subtitle, EPISODE);
    }
    origin(&t, going, NoteOrigin::Subtitle, "The Night Train · S1E3");
    origin(&t, pick, NoteOrigin::Reading, "A book");

    let mut go_notes = vec![went, go, goes, going];
    go_notes.sort();
    assert_eq!(notes_of(&t, "kelime:went"), go_notes, "inflection joins the word");
    assert_eq!(notes_of(&t, "word:going"), go_notes);
    assert_eq!(notes_of(&t, "kelime:\"pick up\""), vec![pick]);
    assert_eq!(notes_of(&t, "nereden:okuma"), vec![pick]);
    assert_eq!(notes_of(&t, "dil:en").len(), 6);
    assert_eq!(count(&t, "karşılaşma>=2"), 4);
    assert_eq!(count(&t, "durum:öğreniyorum"), 6);

    let q = if with_media {
        "kaynak:\"The Night Train · S1E2\" ses:video karşılaşma>=2 durum:öğreniyorum"
    } else {
        "kaynak:\"The Night Train · S1E2\" karşılaşma>=2 durum:öğreniyorum"
    };
    let want: Vec<NoteId> = if with_media {
        let mut v = vec![went, go];
        v.sort();
        v
    } else {
        let mut v = vec![went, go, goes];
        v.sort();
        v
    };
    // One query: count = list = study.
    assert_eq!(notes_of(&t, q), want);
    assert_eq!(count(&t, q) as usize, want.len());
    let ids = t.ok::<SearchIdsMethod>(QueryText { query: q.into() }).card_ids;
    let mut listed = ids.clone();
    listed.sort();
    assert_eq!(study(&t, q), listed, "study the query = its list");

    // A manual mark wins: "go" known → no longer learning.
    t.ok::<ReadingWordsMark>(MarkWords {
        language: "en".into(),
        lemmas: vec!["go".into()],
        mark: Some(WordMark::Known),
    });
    assert_eq!(notes_of(&t, "durum:biliyorum"), go_notes);
    assert_eq!(count(&t, q), 0);
    assert_eq!(count(&t, "durum:learning"), 2);
}

#[test]
fn owners_example_is_one_query_sqlite() {
    owners_example(Arc::new(TempSqlite::new()), true);
}

/// The in-memory store (reference evaluator) agrees on everything it models (no media).
#[test]
fn owners_example_is_one_query_mem() {
    owners_example(Arc::new(MemStore::new()), false);
    owners_example(Arc::new(TempSqlite::new()), false);
}

/// v1.4 W1: `karşılaşma`/`seen` counts ENCOUNTERS. "key" captured in three scenes of one
/// meaning is ONE note (one card) with three encounters; "door" has one. Count = list = study.
fn encounters_are_counted_per_scene(store: Arc<dyn Store>) {
    use crate::modules::subtitle as st;
    use fc_api::v1::modules::SetModuleEnabled;
    use fc_api::v1::subtitle::CreateSubtitleKey;
    let t = TestApp::new(store);
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: st::ID.into(),
        enabled: true,
    });
    let secret = t
        .ok::<SubtitleKeysCreate>(CreateSubtitleKey { name: "x".into() })
        .secret;
    let key = st::authenticate(&t.app, &st::key_hash(&secret)).unwrap();
    let cap = |n: u32, lemma: &str, sentence: &str| {
        let at = sentence.find(lemma).unwrap() as i64;
        serde_json::json!({
          "schema": 1, "capture_id": format!("0192812a-6f3c-7a41-9c2e-{n:012x}"), "kind": "word",
          "lang": { "target": "en", "native": "tr" },
          "term": { "surface": lemma, "lemma": lemma, "pos": "noun" },
          "definition": { "text": format!("m {lemma}"), "source": "dict" },
          "context": { "sentence": sentence, "translation": null, "term_offset": [at, at + lemma.len() as i64] },
          "source": { "type": "video", "url": null, "title": EPISODE, "site": "youtube",
                      "video_id": "v1", "t_start_ms": n * 1000, "t_end_ms": n * 1000 + 900 },
          "media": {}, "deck_id": null, "tags": [], "created_at": "2026-10-12T10:00:00.000Z",
          "client": { "name": "subtitle-extension", "ext_version": "3.1.3" }
        })
    };
    let k1 = st::capture(&t.app, &key, &cap(1, "key", "Give me the key.")).unwrap();
    for (n, s) in [(2, "The key is here."), (3, "Where is the key?")] {
        let o = st::capture(&t.app, &key, &cap(n, "key", s)).unwrap();
        assert!(o.attached && o.note_id == k1.note_id);
    }
    let door = st::capture(&t.app, &key, &cap(4, "door", "Open the door.")).unwrap();
    assert_eq!(count(&t, "dil:en"), 2, "two cards");
    for (q, want) in [
        ("karşılaşma>=3", vec![k1.note_id]),
        ("seen>=2", vec![k1.note_id]),
        ("karşılaşma<2", vec![door.note_id]),
        ("seen>3", vec![]),
    ] {
        assert_eq!(notes_of(&t, q), want, "{q}");
        assert_eq!(count(&t, q) as usize, want.len(), "{q}");
        let mut listed = t.ok::<SearchIdsMethod>(QueryText { query: q.into() }).card_ids;
        listed.sort();
        if !listed.is_empty() {
            assert_eq!(study(&t, q), listed, "{q}: study = list");
        }
    }
}

#[test]
fn encounters_are_counted_per_scene_sqlite() {
    encounters_are_counted_per_scene(Arc::new(TempSqlite::new()));
}

#[test]
fn encounters_are_counted_per_scene_mem() {
    encounters_are_counted_per_scene(Arc::new(MemStore::new()));
}
