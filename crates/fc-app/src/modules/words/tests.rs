//! Word index tests (v1.4 A3), against SQLite (the triggers) and the in-memory store.
//!
//! * Incremental = rebuilt: after every step of a seeded random history (notes made, edited,
//!   deleted, undone and redone; marks; reviews written behind the commands' back; deck
//!   languages switched) the index equals a full rebuild — `incremental_equals_rebuild_*`.
//! * Inflection joins a word, derivation does not — `inflection_is_one_word_derivation_two`.
//! * One truth: reading mode and the Subtitle endpoint answer the same —
//!   `reading_and_subtitle_read_the_same_index`.
//! * Performance: 50 000 notes, a 500-form lookup — `lookup_500_forms_among_50k_notes`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::common::Empty;
use fc_api::v1::decks::CreateDeck;
use fc_api::v1::methods::*;
use fc_api::v1::notes::{CreateNote, CreateNotes, NoteFieldsDraft, NoteIds, NotesTarget, UpdateNote};
use fc_api::v1::reading::{AnalyzeText, MarkWords};
use fc_api::v1::settings::{ResetSetting, SetSetting};
use fc_api::v1::subtitle::SubtitleWordStatus;
use fc_api::v1::words::*;
use fc_core::card::CardState;
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::reading::{WordMark, WordStatus};
use fc_core::settings::SettingScope;
use fc_core::time::MILLIS_PER_DAY;
use fc_core::{DeckId, NoteId, Timestamp};
use fc_storage::Store;
use std::collections::BTreeMap;
use std::sync::Arc;

fn deck(t: &TestApp, path: &str) -> DeckId {
    t.ok::<DecksCreate>(CreateDeck {
        path: path.into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    })
    .id
}

fn set(t: &TestApp, scope: SettingScope, key: &str, value: serde_json::Value) {
    t.ok::<SettingsSet>(SetSetting {
        scope,
        key: key.into(),
        value,
    });
}

fn deck_language(t: &TestApp, d: DeckId, lang: &str) {
    set(t, SettingScope::Deck(d), super::LANGUAGE, serde_json::json!(lang));
}

/// A hand-made Classic note (`front` has the `term` role).
fn note(t: &TestApp, d: DeckId, front: &str) -> NoteId {
    let mut fields = BTreeMap::new();
    fields.insert("front".to_owned(), front.to_owned());
    fields.insert("back".to_owned(), "m".to_owned());
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

/// Sets every card of `note` to a review card with an interval of `days`, behind the
/// commands' back (as a sync or a hand edit would), then drains like a command does.
fn set_interval(t: &TestApp, note: NoteId, days: i64) {
    let mut tx = t.store.write().unwrap();
    let ids: Vec<_> = tx.word_card_rows(&[note]).unwrap().into_iter().map(|c| c.card_id).collect();
    for id in ids {
        let mut card = tx.get_card(id).unwrap().unwrap();
        let last = Timestamp(1_767_225_600_000);
        card.scheduling.state = CardState::Review;
        card.scheduling.last_review = Some(last);
        card.scheduling.due = Some(Timestamp(last.0 + days * MILLIS_PER_DAY));
        tx.update_card(&card).unwrap();
    }
    super::index::drain(&mut *tx).unwrap();
    tx.commit().unwrap();
}

fn mark(t: &TestApp, lemmas: &[&str], m: Option<WordMark>) {
    t.ok::<ReadingWordsMark>(MarkWords {
        language: "en".into(),
        lemmas: lemmas.iter().map(|s| (*s).to_owned()).collect(),
        mark: m,
    });
}

fn lookup(t: &TestApp, forms: &[&str]) -> Vec<WordFormResult> {
    t.ok::<WordsLookup>(LookupWords {
        language: "en".into(),
        forms: forms.iter().map(|s| (*s).to_owned()).collect(),
    })
    .forms
}

fn status(t: &TestApp, form: &str) -> WordStatus {
    lookup(t, &[form])[0].status
}

/// The persisted index equals a full rebuild of it (made in a transaction that is then
/// rolled back).
fn assert_rebuild_equal(t: &TestApp, step: &str) {
    let before = t.store.read().unwrap().word_index_dump().unwrap();
    let mut tx = t.store.write().unwrap();
    super::index::rebuild(&mut *tx).unwrap();
    let after = tx.word_index_dump().unwrap();
    drop(tx);
    assert_eq!(before, after, "incremental ≠ rebuilt after {step}");
}

// ------------------------------------------------------------------ incremental = rebuild

fn incremental_equals_rebuild(store: Arc<dyn Store>) {
    let t = TestApp::new(store);
    let en = deck(&t, "English");
    let other = deck(&t, "Other");
    deck_language(&t, en, "en-GB");
    let words = [
        "go", "went", "goes", "run", "running", "actor", "act", "pick up", "to pick up",
        "house", "houses", "<b>key</b> (n.)",
    ];
    let lemmas = ["go", "run", "actor", "act", "pick up", "house", "key", "door"];
    let mut notes: Vec<NoteId> = Vec::new();
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    for step in 0..70 {
        let op = next(9);
        let label = format!("step {step} op {op}");
        match op {
            0 | 1 => {
                let d = if next(4) == 0 { other } else { en };
                notes.push(note(&t, d, words[next(words.len())]));
            }
            2 if !notes.is_empty() => {
                let id = notes.swap_remove(next(notes.len()));
                t.ok::<NotesDelete>(NoteIds { ids: vec![id] });
            }
            3 => {
                let _ = t.app.invoke(
                    &format!("{}.oplog.undo", fc_api::API_VERSION),
                    serde_json::json!({}),
                    &fc_api::v1::oplog::Actor::User,
                );
            }
            4 => {
                let _ = t.app.invoke(
                    &format!("{}.oplog.redo", fc_api::API_VERSION),
                    serde_json::json!({}),
                    &fc_api::v1::oplog::Actor::User,
                );
            }
            5 => {
                let m = [Some(WordMark::Known), Some(WordMark::Ignored), None][next(3)];
                mark(&t, &[lemmas[next(lemmas.len())]], m);
            }
            6 if !notes.is_empty() => {
                let id = notes[next(notes.len())];
                if t.store.read().unwrap().get_note(id).unwrap().is_some_and(|n| n.deleted_at.is_none()) {
                    set_interval(&t, id, [3, 21, 40][next(3)]);
                }
            }
            7 if !notes.is_empty() => {
                let id = notes[next(notes.len())];
                let live = t.store.read().unwrap().get_note(id).unwrap().filter(|n| n.deleted_at.is_none());
                if live.is_some() {
                    let mut fields = BTreeMap::new();
                    fields.insert("front".to_owned(), words[next(words.len())].to_owned());
                    fields.insert("back".to_owned(), "m2".to_owned());
                    t.ok::<NotesUpdate>(UpdateNote {
                        id,
                        deck_id: None,
                        fields,
                        tags: vec![],
                        attributes: vec![],
                    });
                }
            }
            8 => match next(3) {
                0 => deck_language(&t, other, "en"),
                1 => {
                    t.ok::<SettingsReset>(ResetSetting {
                        scope: SettingScope::Deck(other),
                        key: super::LANGUAGE.into(),
                    });
                }
                _ => set(&t, SettingScope::App, super::LANGUAGE, serde_json::json!("tr")),
            },
            _ => {}
        }
        assert_rebuild_equal(&t, &label);
    }
    // The history did something.
    let (rows, note_rows) = t.store.read().unwrap().word_index_dump().unwrap();
    assert!(!rows.is_empty() && !note_rows.is_empty());
    // The rebuild job gives the same index as well.
    let before = t.store.read().unwrap().word_index_dump().unwrap();
    t.ok::<WordsIndexRebuild>(Empty {});
    t.app.run_pending_jobs().unwrap();
    assert_eq!(t.store.read().unwrap().word_index_dump().unwrap(), before);
}

#[test]
fn incremental_equals_rebuild_sqlite() {
    incremental_equals_rebuild(Arc::new(TempSqlite::new()));
}

#[test]
fn incremental_equals_rebuild_mem() {
    incremental_equals_rebuild(Arc::new(MemStore::new()));
}

// ------------------------------------------------------------------ rules

#[test]
fn inflection_is_one_word_derivation_two() {
    let t = TestApp::new(Arc::new(TempSqlite::new()));
    let d = deck(&t, "English");
    deck_language(&t, d, "en");
    let go = note(&t, d, "go");
    note(&t, d, "actor");
    let r = lookup(&t, &["went", "goes", "going", "go", "actor", "act", "acting"]);
    let keys: Vec<_> = r.iter().map(|f| f.key.clone().unwrap()).collect();
    assert_eq!(keys, ["go", "go", "go", "go", "actor", "act", "act"]);
    let st: Vec<_> = r.iter().map(|f| f.status).collect();
    use WordStatus::*;
    assert_eq!(st, [Learning, Learning, Learning, Learning, Learning, New, New]);
    // The evidence behind "went": the note and its card.
    let w = r[0].word.as_ref().unwrap();
    assert_eq!(w.note_ids, vec![go]);
    assert_eq!((w.notes, w.cards, w.card_ids.len()), (1, 1, 1));
    // Knowing "actor" does not make "act" known.
    set_interval(&t, go, 30);
    mark(&t, &["actor"], Some(WordMark::Known));
    assert_eq!(status(&t, "went"), Known);
    assert_eq!(status(&t, "actor"), Known);
    assert_eq!(status(&t, "act"), New);
}

#[test]
fn a_manual_mark_wins_and_the_threshold_is_a_setting() {
    let t = TestApp::new(Arc::new(TempSqlite::new()));
    let d = deck(&t, "English");
    deck_language(&t, d, "en");
    let n = note(&t, d, "house");
    set_interval(&t, n, 30);
    assert_eq!(status(&t, "houses"), WordStatus::Known);
    let w = lookup(&t, &["house"])[0].word.clone().unwrap();
    assert!(w.known_by_maturity && w.mark.is_none() && w.max_interval_days == Some(30));
    set(&t, SettingScope::App, super::KNOWN_INTERVAL, serde_json::json!(60));
    assert_eq!(status(&t, "house"), WordStatus::Learning);
    // The mark wins over the card; the evidence stays apart.
    mark(&t, &["house"], Some(WordMark::Ignored));
    let w = lookup(&t, &["house"])[0].word.clone().unwrap();
    assert_eq!(w.status, WordStatus::Ignored);
    assert_eq!((w.cards, w.mark), (1, Some(WordMark::Ignored)));
    // Undo puts the mark back.
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(status(&t, "house"), WordStatus::Learning);
    // Stats and list.
    mark(&t, &["door"], Some(WordMark::Known));
    let s = t.ok::<WordsStats>(Empty {});
    assert_eq!(s.threshold_days, 60);
    assert_eq!(s.languages.len(), 1);
    assert_eq!((s.languages[0].counts.learning, s.languages[0].counts.known), (1, 1));
    let known = t.ok::<WordsList>(ListWords {
        language: Some("en".into()),
        status: Some(WordStatus::Known),
        ..ListWords::default()
    });
    assert_eq!(known.words.iter().map(|w| w.key.as_str()).collect::<Vec<_>>(), ["door"]);
    let p1 = t.ok::<WordsList>(ListWords { limit: Some(1), ..ListWords::default() });
    assert_eq!(p1.words[0].key, "door");
    let p2 = t.ok::<WordsList>(ListWords {
        limit: Some(1),
        cursor: p1.next_cursor,
        ..ListWords::default()
    });
    assert_eq!((p2.words[0].key.as_str(), p2.next_cursor), ("house", None));
}

#[test]
fn notes_without_a_language_are_reported_not_guessed() {
    let t = TestApp::new(Arc::new(TempSqlite::new()));
    let d = deck(&t, "Mixed");
    note(&t, d, "house");
    let s = t.ok::<WordsStats>(Empty {});
    assert_eq!((s.unassigned_notes, s.languages.len()), (1, 0));
    assert_eq!(status(&t, "house"), WordStatus::New);
    // Giving the deck a language indexes its notes (the settings trigger queues them).
    deck_language(&t, d, "en");
    let s = t.ok::<WordsStats>(Empty {});
    assert_eq!((s.unassigned_notes, s.languages[0].counts.learning), (0, 1));
    assert_eq!(status(&t, "houses"), WordStatus::Learning);
}

#[test]
fn reading_and_subtitle_read_the_same_index() {
    let t = TestApp::new(Arc::new(TempSqlite::new()));
    let d = deck(&t, "Anki import");
    deck_language(&t, d, "en");
    let key = note(&t, d, "key");
    note(&t, d, "pick up");
    set_interval(&t, key, 30);
    mark(&t, &["door"], Some(WordMark::Known));
    mark(&t, &["john"], Some(WordMark::Ignored));
    // Reading mode.
    let a = t.ok::<ReadingAnalyze>(AnalyzeText {
        item_id: None,
        text: Some("John picked up the keys at the door.".into()),
        language: Some("en".into()),
        range: None,
        tz_offset_mins: Some(0),
    });
    let of = |lemma: &str| a.tokens.iter().find(|x| x.lemma == lemma).unwrap().status;
    assert_eq!(of("key"), WordStatus::Known);
    assert_eq!(of("door"), WordStatus::Known);
    assert_eq!(of("john"), WordStatus::Ignored);
    // The Subtitle endpoint (same shape as v1.2): hand-made and marked words too.
    let k = crate::modules::subtitle::known_words(&t.app, Some("en"), None).unwrap();
    let got: Vec<_> = k.words.iter().map(|w| (w.lemma.as_str(), w.status)).collect();
    assert_eq!(
        got,
        [
            ("door", SubtitleWordStatus::Known),
            ("key", SubtitleWordStatus::Known),
            ("pick up", SubtitleWordStatus::Learning)
        ]
    );
    assert_eq!(k.words[1].interval_days, 30);
}

// ------------------------------------------------------------------ performance

/// 50 000 notes; a 500-form lookup is indexed reads only (no scan): generous bound, measured
/// time printed.
#[test]
fn lookup_500_forms_among_50k_notes() {
    let t = TestApp::new(Arc::new(TempSqlite::new()));
    let d = deck(&t, "Big");
    deck_language(&t, d, "en");
    let syl = ["ka", "lo", "mi", "ner", "tas", "vu", "pel", "dor", "sin", "bra"];
    let word = |i: usize| {
        format!(
            "{}{}{}{}{}",
            syl[i % 10],
            syl[(i / 10) % 10],
            syl[(i / 100) % 10],
            syl[(i / 1000) % 10],
            syl[(i / 10_000) % 10]
        )
    };
    let build = std::time::Instant::now();
    for chunk in 0..50 {
        t.ok::<NotesCreateMany>(CreateNotes {
            notetype_id: CLASSIC_ID,
            target: NotesTarget::Deck { id: d },
            notes: (0..1000)
                .map(|i| {
                    let mut fields = BTreeMap::new();
                    fields.insert("front".to_owned(), word(chunk * 1000 + i));
                    fields.insert("back".to_owned(), "m".to_owned());
                    NoteFieldsDraft { fields }
                })
                .collect(),
            tags: vec![],
            attributes: vec![],
            allow_duplicate: Some(true),
        });
    }
    println!("50 000 notes created and indexed in {:?}", build.elapsed());
    let s = t.ok::<WordsStats>(Empty {});
    assert_eq!(s.languages[0].counts.learning, 50_000);
    let forms: Vec<String> = (0..500).map(|i| word(i * 97).to_uppercase()).collect();
    let start = std::time::Instant::now();
    let r = t.ok::<WordsLookup>(LookupWords {
        language: "en".into(),
        forms,
    });
    let took = start.elapsed();
    println!("words.lookup of 500 forms among 50 000 notes: {took:?}");
    assert_eq!(r.forms.len(), 500);
    assert!(r.forms.iter().all(|f| f.status == WordStatus::Learning), "every form found");
    assert!(took.as_millis() < 3_000, "lookup took {took:?}");
}
