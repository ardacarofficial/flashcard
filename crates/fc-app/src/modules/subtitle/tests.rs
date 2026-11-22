//! Service tests of the `subtitle` module (three stores): access keys, captures as notes
//! (deck choice, duplicates, media), batches, known words, undo/redo, the module switch and
//! backups revoking keys. The HTTP transport itself is tested in `fc-http`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::decks::DeckRef;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::NoteRef;
use fc_api::v1::oplog::ListOplog;
use fc_core::card::{CardState, SchedulingState};
use fc_core::time::FixedClock;
use fc_storage::{SqliteStore, Store};
use serde_json::json;
use std::sync::Arc;

type Factory = fn() -> Arc<dyn Store>;

macro_rules! suite {
    ($($name:ident),* $(,)?) => {
        mod mem {
            $( #[test] fn $name() { super::$name(|| std::sync::Arc::new(super::MemStore::new())); } )*
        }
        mod sqlite {
            $( #[test] fn $name() {
                super::$name(|| std::sync::Arc::new(super::SqliteStore::open_in_memory().unwrap()));
            } )*
        }
        mod sqlite_file {
            $( #[test] fn $name() { super::$name(|| std::sync::Arc::new(super::TempSqlite::new())); } )*
        }
    };
}

suite!(
    keys_are_shown_once_stored_hashed_and_revocable,
    nothing_works_while_the_module_is_off,
    a_capture_becomes_a_note_in_the_subtitle_deck,
    captures_are_idempotent_by_id_and_by_sentence,
    captures_go_to_their_deck_or_the_default_deck,
    invalid_captures_are_refused,
    uploads_attach_to_the_note_and_retries_get_the_same_id,
    uploads_are_checked,
    stale_uploads_are_forgotten,
    batches_judge_each_capture_on_its_own,
    known_words_follow_the_cards_intervals,
    a_capture_is_one_undo_step,
    a_word_whose_card_was_deleted_comes_back,
    restoring_a_backup_revokes_every_key,
    restoring_a_backup_brings_no_capture_diagnostics,
);

/// A JPEG and a WAV header (what the media sniffer recognises).
const JPEG: &[u8] = b"\xff\xd8\xff\xe0\x00\x10JFIF\x00jpeg-bytes";
const WAV: &[u8] = b"RIFF\x24\x00\x00\x00WAVEfmt wav-bytes";

fn enable(t: &TestApp) {
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: ID.into(),
        enabled: true,
    });
}

fn new_key(t: &TestApp, name: &str) -> (SubtitleKeyDto, String) {
    let c = t.ok::<SubtitleKeysCreate>(CreateSubtitleKey { name: name.into() });
    let key = authenticate(&t.app, &key_hash(&c.secret)).unwrap();
    (key, c.secret)
}

fn share_known_words(t: &TestApp, on: bool) {
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: SHARE_KNOWN_WORDS.into(),
        value: json!(on),
    });
}

fn setup(f: Factory) -> (TestApp, SubtitleKeyDto) {
    let t = TestApp::new(f());
    enable(&t);
    let (key, _) = new_key(&t, "Chrome");
    (t, key)
}

/// The contract's example capture with another id (`n` = last digits) and line.
fn example(n: u32, lemma: &str, sentence: &str) -> Value {
    let mut v = json!({
      "schema": 1,
      "capture_id": format!("0192812a-6f3c-7a41-9c2e-{n:012x}"),
      "kind": "word",
      "lang": { "target": "en", "native": "tr" },
      "term": { "surface": "keys", "lemma": lemma, "pos": "noun" },
      "definition": { "text": "anahtar", "source": "dict" },
      "context": {
        "sentence": sentence,
        "translation": "Neredeyse. Sadece anahtarlarımı almam lazım.",
        "term_offset": [34, 38]
      },
      "source": {
        "type": "video",
        "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=83s",
        "title": "Never Gonna Give You Up",
        "site": "youtube",
        "video_id": "dQw4w9WgXcQ",
        "t_start_ms": 83450,
        "t_end_ms": 86120
      },
      "media": {},
      "deck_id": null,
      "tags": ["subtitle"],
      "created_at": "2026-10-12T10:00:00.000Z",
      "client": { "name": "subtitle-extension", "ext_version": "3.1.3" }
    });
    if lemma != "key" {
        v["context"]["term_offset"] = Value::Null;
        v["term"]["surface"] = json!(lemma);
    }
    v
}

const LINE: &str = "Almost. I just need to pick up my keys.";

fn note(t: &TestApp, id: NoteId) -> NoteDto {
    t.ok::<NotesGet>(NoteRef { id })
}

fn rows(t: &TestApp) -> u64 {
    t.store.read().unwrap().subtitle_capture_stats().unwrap().0
}

pub(super) fn keys_are_shown_once_stored_hashed_and_revocable(f: Factory) {
    let t = TestApp::new(f());
    enable(&t);
    let before = t.ok::<OplogList>(ListOplog::default()).items.len();
    let made = t.ok::<SubtitleKeysCreate>(CreateSubtitleKey { name: " Chrome ".into() });
    assert!(made.secret.starts_with(KEY_PREFIX));
    assert!(made.secret.len() > 40);
    assert_eq!(made.key.name, "Chrome");
    assert!(made.secret.starts_with(&made.key.prefix));
    assert_eq!(made.key.prefix.len(), 10);
    // Only the hash is stored; the list never shows the key.
    let rec = t
        .store
        .read()
        .unwrap()
        .get_subtitle_key(made.key.id)
        .unwrap()
        .unwrap();
    assert_eq!(rec.token_hash, key_hash(&made.secret));
    assert_ne!(rec.token_hash, made.secret);
    let list = t.ok::<SubtitleKeysList>(Empty {});
    assert_eq!(list.keys, vec![made.key.clone()]);
    assert!(!serde_json::to_string(&list).unwrap().contains(&made.secret));
    // Keys are not undo steps.
    assert_eq!(t.ok::<OplogList>(ListOplog::default()).items.len(), before);

    let key = authenticate(&t.app, &key_hash(&made.secret)).unwrap();
    assert_eq!(key.id, made.key.id);
    let wrong = authenticate(&t.app, &key_hash("sk_fc_wrong")).unwrap_err();
    assert_eq!(wrong.code, ErrorCode::PermissionUnauthenticated);
    assert_eq!(wrong.details["reason"], "invalid");

    touch_key(&t.app, &key).unwrap();
    let touched = t.ok::<SubtitleKeysList>(Empty {}).keys[0].clone();
    assert!(touched.last_used_at.is_some());

    let revoked = t.ok::<SubtitleKeysRevoke>(SubtitleKeyRef { id: key.id });
    assert!(revoked.revoked_at.is_some());
    let e = authenticate(&t.app, &key_hash(&made.secret)).unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionUnauthenticated);
    assert_eq!(e.details["reason"], "revoked");
    assert_eq!(
        t.err::<SubtitleKeysRevoke>(SubtitleKeyRef { id: key.id }).code,
        ErrorCode::ConflictSubtitleKeyState
    );
    assert_eq!(
        t.err::<SubtitleKeysCreate>(CreateSubtitleKey { name: "  ".into() }).code,
        ErrorCode::ValidationSubtitleKey
    );
    assert_eq!(t.ok::<SubtitleStatusGet>(Empty {}).active_keys, 0);
    // The extension's actor holds nothing: public methods refuse it.
    let e = t
        .app
        .invoke("v1.decks.list", json!({}), &actor(&key))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied);
}

pub(super) fn nothing_works_while_the_module_is_off(f: Factory) {
    let t = TestApp::new(f());
    assert!(!t.app.is_module_enabled(ID), "off by default");
    assert_eq!(
        t.err::<SubtitleKeysList>(Empty {}).code,
        ErrorCode::ConflictModuleDisabled
    );
    enable(&t);
    let (key, secret) = new_key(&t, "Chrome");
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: ID.into(),
        enabled: false,
    });
    assert_eq!(
        authenticate(&t.app, &key_hash(&secret)).unwrap_err().code,
        ErrorCode::ConflictModuleDisabled
    );
    assert_eq!(
        capture(&t.app, &key, &example(1, "key", LINE)).unwrap_err().code,
        ErrorCode::ConflictModuleDisabled
    );
    assert_eq!(rows(&t), 0);
}

pub(super) fn a_capture_becomes_a_note_in_the_subtitle_deck(f: Factory) {
    let (t, key) = setup(f);
    let o = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    assert_eq!(o.status, SubtitleCaptureStatus::Created);
    let n = note(&t, o.note_id);
    assert_eq!(n.notetype_id, builtin::find(builtin::SUBTITLE).unwrap().id);
    assert_eq!(n.cards.len(), 1);
    assert_eq!(n.cards[0].id, o.card_id);
    assert_eq!(n.fields["word"], "key");
    assert_eq!(n.fields["surface"], "keys");
    assert_eq!(n.fields["pos"], "noun");
    assert_eq!(n.fields["meaning"], "anahtar");
    assert_eq!(n.fields["sentence"], "Almost. I just need to pick up my <b>keys</b>.");
    assert!(n.fields["translation"].starts_with("Neredeyse."));
    assert!(n.fields["source"].contains("Never Gonna Give You Up · 1:23"));
    assert!(n.fields["source"].contains("href=\"https://www.youtube.com/watch?v=dQw4w9WgXcQ&amp;t=83s\""));
    assert_eq!(n.tags.iter().map(|t| t.as_str()).collect::<Vec<_>>(), ["subtitle"]);
    // Straight into a deck named "Subtitle", made on first use (not the inbox).
    let deck = t.ok::<DecksGet>(DeckRef { id: n.home_deck_id });
    assert_eq!(deck.path, FALLBACK_DECK);
    // The oplog names the key as the actor.
    let log = t.ok::<OplogList>(ListOplog::default()).items;
    let entry = log
        .iter()
        .find(|e| e.command == "v1.subtitle.captures.create")
        .unwrap();
    assert_eq!(entry.actor, Actor::Subtitle(key.id.to_string()));
    assert_eq!(entry.summary.key, "oplog.summary.subtitle.captures.create");
    let st = t.ok::<SubtitleStatusGet>(Empty {});
    assert_eq!(st.captures, 1);
    assert!(st.last_capture_at.is_some());
    assert_eq!(st.address, "http://127.0.0.1:28735/subtitle");
    assert!(t
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e.event, DomainEvent::SubtitleChanged)));
    // A second capture reuses the deck.
    let o2 = capture(&t.app, &key, &example(2, "pick up", LINE)).unwrap();
    assert_eq!(note(&t, o2.note_id).home_deck_id, n.home_deck_id);
    assert_eq!(note(&t, o2.note_id).fields["sentence"], "Almost. I just need to <b>pick up</b> my keys.");
}

pub(super) fn captures_are_idempotent_by_id_and_by_sentence(f: Factory) {
    let (t, key) = setup(f);
    let first = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    // The same capture again (a retry): the same card.
    let again = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    assert_eq!(again.status, SubtitleCaptureStatus::Duplicate);
    assert_eq!(again.card_id, first.card_id);
    // Another id, the same word from the same line (spacing and case aside).
    let same_line = capture(
        &t.app,
        &key,
        &example(2, "Key", "almost.  I just need to pick up my KEYS."),
    )
    .unwrap();
    assert_eq!(same_line.status, SubtitleCaptureStatus::Duplicate);
    assert_eq!(same_line.card_id, first.card_id);
    // v1.4 (deliberate change): the same word and meaning from another line is stored as
    // another example of the same card (`created` on the wire); see `encounters_tests`.
    let other = capture(&t.app, &key, &example(3, "key", "Where are my keys?")).unwrap();
    assert_eq!(other.status, SubtitleCaptureStatus::Created);
    assert!(other.attached);
    assert_eq!(other.note_id, first.note_id);
    assert_eq!(rows(&t), 2);
}

pub(super) fn captures_go_to_their_deck_or_the_default_deck(f: Factory) {
    let (t, key) = setup(f);
    let mk = |path: &str| {
        t.ok::<DecksCreate>(CreateDeck {
            path: path.into(),
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        })
    };
    let series = mk("Diziler");
    let english = mk("İngilizce");
    let mut v = example(1, "key", LINE);
    v["deck_id"] = json!(series.id.to_string());
    let o = capture(&t.app, &key, &v).unwrap();
    assert_eq!(note(&t, o.note_id).home_deck_id, series.id);

    let st = t.ok::<SubtitleDefaultDeckSet>(SetSubtitleDefaultDeck {
        deck_id: Some(english.id),
    });
    assert_eq!(st.default_deck_id, Some(english.id));
    assert_eq!(st.default_deck_path.as_deref(), Some("İngilizce"));
    let o = capture(&t.app, &key, &example(2, "pick up", LINE)).unwrap();
    assert_eq!(note(&t, o.note_id).home_deck_id, english.id);
    // An unknown deck id falls back to the default deck (the capture is never lost).
    let mut v = example(3, "need", LINE);
    v["deck_id"] = json!("deck_gone");
    let o = capture(&t.app, &key, &v).unwrap();
    assert_eq!(note(&t, o.note_id).home_deck_id, english.id);
    // The setting is an undo step; clearing it brings back the Subtitle deck.
    let st = t.ok::<SubtitleDefaultDeckSet>(SetSubtitleDefaultDeck { deck_id: None });
    assert_eq!(st.default_deck_id, None);
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(
        t.ok::<SubtitleStatusGet>(Empty {}).default_deck_id,
        Some(english.id)
    );
    assert_eq!(
        t.err::<SubtitleDefaultDeckSet>(SetSubtitleDefaultDeck {
            deck_id: Some(DeckId(uuid::Uuid::from_u128(7)))
        })
        .code,
        ErrorCode::NotFoundDeck
    );
}

pub(super) fn invalid_captures_are_refused(f: Factory) {
    let (t, key) = setup(f);
    let mut v = example(1, "key", LINE);
    v["term"].as_object_mut().unwrap().remove("lemma");
    let e = capture(&t.app, &key, &v).unwrap_err();
    assert_eq!(e.code, ErrorCode::ValidationSubtitleCapture);
    assert_eq!(e.message, "term.lemma is required");
    assert_eq!(error_code(&e), "invalid_capture");
    assert_eq!(rows(&t), 0);
}

fn upload_of(t: &TestApp, key: &SubtitleKeyDto, n: u32, kind: &str, mime: &str, bytes: &[u8]) -> Result<String, ApiError> {
    let id = format!("0192812a-6f3c-7a41-9c2e-{n:012x}");
    upload(&t.app, key, &id, kind, mime, bytes).map(|r| r.upload_id)
}

pub(super) fn uploads_attach_to_the_note_and_retries_get_the_same_id(f: Factory) {
    let (t, key) = setup(f);
    let shot = upload_of(&t, &key, 1, "screenshot", "image/jpeg", JPEG).unwrap();
    let audio = upload_of(&t, &key, 1, "audio", "audio/wav", WAV).unwrap();
    assert!(shot.starts_with("up_"));
    assert_ne!(shot, audio);
    // A retry of the same capture + kind: the same id.
    assert_eq!(upload_of(&t, &key, 1, "screenshot", "image/jpeg", JPEG).unwrap(), shot);
    // Uploads are not undo steps.
    assert!(t
        .ok::<OplogList>(ListOplog::default())
        .items
        .iter()
        .all(|e| !e.command.contains("media")));
    let media = upload_media(&*t.store.read().unwrap(), &shot).unwrap();
    assert_eq!(media.mime, "image/jpeg");

    let mut v = example(1, "key", LINE);
    v["media"] = json!({
        "screenshot": {"upload_id": shot, "mime": "image/jpeg"},
        "audio": {"upload_id": audio, "mime": "audio/wav"},
    });
    let o = capture(&t.app, &key, &v).unwrap();
    let n = note(&t, o.note_id);
    assert_eq!(n.fields["image"], format!("media:{}", media.hash));
    assert!(n.fields["audio"].starts_with("media:"));
    assert_eq!(
        t.store.read().unwrap().get_media(&media.hash).unwrap().unwrap().ref_count,
        1
    );
    // An unknown upload id (or another capture's) only costs the card its media.
    let mut v = example(2, "pick up", LINE);
    v["media"] = json!({"screenshot": {"upload_id": "up_nope", "mime": "image/jpeg"},
                        "audio": {"upload_id": audio, "mime": "audio/wav"}});
    let o = capture(&t.app, &key, &v).unwrap();
    let n = note(&t, o.note_id);
    assert_eq!(n.fields.get("image").map(String::as_str).unwrap_or_default(), "");
    assert_eq!(n.fields.get("audio").map(String::as_str).unwrap_or_default(), "");
}

pub(super) fn uploads_are_checked(f: Factory) {
    let (t, key) = setup(f);
    let code = |r: Result<String, ApiError>| r.unwrap_err().code;
    assert_eq!(
        code(upload_of(&t, &key, 1, "video", "image/jpeg", JPEG)),
        ErrorCode::ValidationSubtitleMedia
    );
    assert_eq!(
        code(upload_of(&t, &key, 1, "screenshot", "image/png", JPEG)),
        ErrorCode::ValidationSubtitleMedia
    );
    // The bytes must be what the headers say.
    assert_eq!(
        code(upload_of(&t, &key, 1, "audio", "audio/wav", JPEG)),
        ErrorCode::ValidationSubtitleMedia
    );
    let mut big = JPEG.to_vec();
    big.resize(MAX_SCREENSHOT_BYTES + 1, 0);
    assert_eq!(
        code(upload_of(&t, &key, 1, "screenshot", "image/jpeg", &big)),
        ErrorCode::ValidationPayloadTooLarge
    );
    assert_eq!(
        upload(&t.app, &key, "not-a-capture", "audio", "audio/wav", WAV)
            .unwrap_err()
            .code,
        ErrorCode::ValidationSubtitleMedia
    );
    // `; codecs=…` parameters are fine.
    assert!(upload_of(&t, &key, 1, "audio", "audio/wav; codecs=1", WAV).is_ok());
}

pub(super) fn stale_uploads_are_forgotten(f: Factory) {
    let clock = Arc::new(FixedClock::new(Timestamp(1_767_225_600_000)));
    let t = TestApp::with_clock(f(), clock.clone());
    enable(&t);
    let (key, _) = new_key(&t, "Chrome");
    let old = upload_of(&t, &key, 1, "screenshot", "image/jpeg", JPEG).unwrap();
    let hash = upload_media(&*t.store.read().unwrap(), &old).unwrap().hash;
    clock.advance(UPLOAD_TTL_MS + 1);
    // The next upload forgets the one whose capture never came, and its unused media.
    upload_of(&t, &key, 2, "audio", "audio/wav", WAV).unwrap();
    let r = t.store.read().unwrap();
    assert!(r.get_subtitle_upload(&old).unwrap().is_none());
    assert!(r.get_media(&hash).unwrap().is_none());
}

pub(super) fn batches_judge_each_capture_on_its_own(f: Factory) {
    let (t, key) = setup(f);
    let first = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    let mut invalid = example(3, "x", LINE);
    invalid["kind"] = json!("idiom");
    let r = batch(
        &t.app,
        &key,
        &json!({"captures": [example(2, "pick up", LINE), example(1, "key", LINE), invalid, 7]}),
    )
    .unwrap();
    let s: Vec<_> = r.results.iter().map(|i| i.status).collect();
    assert_eq!(
        s,
        [
            SubtitleBatchStatus::Created,
            SubtitleBatchStatus::Duplicate,
            SubtitleBatchStatus::Error,
            SubtitleBatchStatus::Error
        ]
    );
    assert_eq!(r.results[1].card_id, Some(first.card_id.to_string()));
    assert_eq!(r.results[2].capture_id, "0192812a-6f3c-7a41-9c2e-000000000003");
    let err = r.results[2].error.as_ref().unwrap();
    assert_eq!(err.code, "invalid_capture");
    assert_eq!(err.retryable, Some(false));
    assert_eq!(r.results[3].capture_id, "");
    let many: Vec<Value> = (0..=MAX_BATCH as u32).map(|n| example(100 + n, "x", LINE)).collect();
    assert_eq!(
        batch(&t.app, &key, &json!({ "captures": many })).unwrap_err().code,
        ErrorCode::ValidationSubtitleCapture
    );
    assert_eq!(
        batch(&t.app, &key, &json!({})).unwrap_err().code,
        ErrorCode::ValidationSubtitleCapture
    );
}

pub(super) fn known_words_follow_the_cards_intervals(f: Factory) {
    let (t, key) = setup(f);
    let key_card = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    capture(&t.app, &key, &example(2, "pick up", LINE)).unwrap();
    let mut de = example(3, "haus", "Das Haus ist groß.");
    de["lang"]["target"] = json!("DE");
    capture(&t.app, &key, &de).unwrap();

    // v1.4: not shared by default — an empty last page that says so, and `/me` agrees.
    assert!(!t.ok::<SubtitleStatusGet>(Empty {}).share_known_words);
    let off = known_words(&t.app, Some("en"), None).unwrap();
    assert_eq!(
        (off.words.len(), off.next_cursor.clone(), off.shared),
        (0, None, false)
    );
    assert!(!me(&t.app, &key).unwrap().features.known_words);
    share_known_words(&t, true);
    assert!(t.ok::<SubtitleStatusGet>(Empty {}).share_known_words);
    assert!(me(&t.app, &key).unwrap().features.known_words);

    let all = known_words(&t.app, Some("en"), None).unwrap();
    assert!(all.shared);
    let lemmas: Vec<_> = all.words.iter().map(|w| w.lemma.as_str()).collect();
    assert_eq!(lemmas, ["key", "pick up"]);
    assert!(all.words.iter().all(|w| w.status == SubtitleWordStatus::Learning));
    assert!(all.words.iter().all(|w| w.interval_days == 0 && w.lang == "en"));
    assert!(all.words[0].updated_at.ends_with('Z'));
    assert_eq!(all.next_cursor, None);

    // A review card with a 30-day interval: known.
    let now = t.app.now();
    let mut tx = t.store.write().unwrap();
    let mut card = tx.get_card(key_card.card_id).unwrap().unwrap();
    card.scheduling = SchedulingState {
        state: CardState::Review,
        due: Some(now.plus_days(30)),
        last_review: Some(now),
        reps: 3,
        ..SchedulingState::NEW
    };
    tx.update_card(&card).unwrap();
    // A raw write outside the dispatcher: bring the word index along, as every command does.
    crate::modules::words::index::drain(&mut *tx).unwrap();
    tx.commit().unwrap();
    let words = known_words(&t.app, Some("en"), None).unwrap().words;
    assert_eq!(words[0].status, SubtitleWordStatus::Known);
    assert_eq!(words[0].interval_days, 30);
    assert_eq!(words[0].updated_at, fc_core::maintenance::iso_datetime(now));

    // Pages: by language, then lemma.
    let p1 = known_words_page(&t.app, None, None, 2).unwrap();
    assert_eq!(
        p1.words.iter().map(|w| (w.lang.as_str(), w.lemma.as_str())).collect::<Vec<_>>(),
        [("de", "haus"), ("en", "key")]
    );
    let p2 = known_words_page(&t.app, None, p1.next_cursor.as_deref(), 2).unwrap();
    assert_eq!(p2.words.len(), 1);
    assert_eq!(p2.words[0].lemma, "pick up");
    assert_eq!(p2.next_cursor, None);
    assert_eq!(
        known_words(&t.app, None, Some("%%%")).unwrap_err().code,
        ErrorCode::ValidationInvalidParams
    );
    // A word whose note is gone is left out.
    t.ok::<NotesDelete>(fc_api::v1::notes::NoteIds {
        ids: vec![key_card.note_id],
    });
    let words = known_words(&t.app, Some("en"), None).unwrap().words;
    assert_eq!(words.iter().map(|w| w.lemma.as_str()).collect::<Vec<_>>(), ["pick up"]);

    // Switched off again: nothing more is shared.
    share_known_words(&t, false);
    assert!(known_words(&t.app, Some("en"), None).unwrap().words.is_empty());
}

pub(super) fn a_capture_is_one_undo_step(f: Factory) {
    let (t, key) = setup(f);
    let o = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    let deck = note(&t, o.note_id).home_deck_id;
    let st = t.ok::<OplogStatus>(Empty {});
    assert_eq!(
        st.undo_summary.map(|s| s.key).as_deref(),
        Some("oplog.summary.subtitle.captures.create")
    );
    // Undo takes back the note, the deck it made and the mapping.
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(rows(&t), 0);
    assert!(t
        .store
        .read()
        .unwrap()
        .get_note(o.note_id)
        .unwrap()
        .is_none_or(|n| n.deleted_at.is_some()));
    assert!(t
        .store
        .read()
        .unwrap()
        .find_child_deck(None, FALLBACK_DECK)
        .unwrap()
        .is_none());
    // Redo puts all of it back: the same capture is a duplicate of the same card again.
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(rows(&t), 1);
    assert_eq!(note(&t, o.note_id).home_deck_id, deck);
    let again = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    assert_eq!(again.status, SubtitleCaptureStatus::Duplicate);
    assert_eq!(again.card_id, o.card_id);
    // Undone for good, a resend makes the card again.
    t.ok::<OplogUndo>(Empty {});
    let resent = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    assert_eq!(resent.status, SubtitleCaptureStatus::Created);
}

/// Review fix: a mapping whose note the user deleted is no duplicate any more; the word
/// comes back as a new card, and undoing that puts the old mapping back.
pub(super) fn a_word_whose_card_was_deleted_comes_back(f: Factory) {
    let (t, key) = setup(f);
    let first = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    t.ok::<NotesDelete>(fc_api::v1::notes::NoteIds {
        ids: vec![first.note_id],
    });
    // Another capture of the same word from the same line (re-added after the delete).
    let again = capture(&t.app, &key, &example(2, "key", LINE)).unwrap();
    assert_eq!(again.status, SubtitleCaptureStatus::Created);
    assert_ne!(again.note_id, first.note_id);
    assert!(note_is_live(&*t.store.read().unwrap(), again.note_id).unwrap());
    assert_eq!(rows(&t), 1, "the stale mapping was replaced");
    let mapped = |id: &str| t.store.read().unwrap().get_subtitle_capture(id).unwrap();
    assert!(mapped(&example_id(1)).is_none());
    // Now it is a duplicate of the new card, by either id.
    for n in [1, 2] {
        let d = capture(&t.app, &key, &example(n, "key", LINE)).unwrap();
        assert_eq!(d.status, SubtitleCaptureStatus::Duplicate, "{n}");
        assert_eq!(d.card_id, again.card_id);
    }
    // Undo: the new card and its mapping go, the old mapping is back as it was.
    t.ok::<OplogUndo>(Empty {});
    assert!(mapped(&example_id(2)).is_none());
    assert_eq!(mapped(&example_id(1)).unwrap().note_id, first.note_id);
    // Redo: the new card is back, mapped, and the stale one is gone again.
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(mapped(&example_id(2)).unwrap().note_id, again.note_id);
    assert!(mapped(&example_id(1)).is_none());
    // A resend of the very first capture (same id) after its card was deleted comes back too.
    t.ok::<NotesDelete>(fc_api::v1::notes::NoteIds {
        ids: vec![again.note_id],
    });
    let resent = capture(&t.app, &key, &example(2, "key", LINE)).unwrap();
    assert_eq!(resent.status, SubtitleCaptureStatus::Created);
    assert_eq!(rows(&t), 1);
}

fn example_id(n: u32) -> String {
    example(n, "key", LINE)["capture_id"].as_str().unwrap().to_owned()
}

pub(super) fn restoring_a_backup_revokes_every_key(f: Factory) {
    let (t, _) = setup(f);
    // MemStore has no dump; the SQLite stores have.
    let Ok(mut dump) = t.store.read().unwrap().dump_collection() else {
        return;
    };
    forget_keys(&mut dump);
    let table = dump
        .tables
        .iter()
        .find(|x| x.name == "subtitle_keys")
        .unwrap();
    assert_eq!(table.rows.len(), 1);
    let col = table.columns.iter().position(|c| c == "revoked_at").unwrap();
    assert!(table
        .rows
        .iter()
        .all(|r| matches!(r[col], fc_storage::repo::SqlValue::Integer(_))));
}

/// v1.4 A6: the capture diagnostics (`subtitle_capture_log`) are not user data and never come
/// back with a restore (docs/06 §5.1): the restored profile's `subtitle.captures.recent`
/// starts empty instead of listing attempts made against another collection.
pub(super) fn restoring_a_backup_brings_no_capture_diagnostics(f: Factory) {
    let (t, key) = setup(f);
    capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    let Ok(mut dump) = t.store.read().unwrap().dump_collection() else {
        return;
    };
    let log = |d: &fc_storage::repo::CollectionDump| {
        d.tables
            .iter()
            .find(|x| x.name == "subtitle_capture_log")
            .map(|x| x.rows.len())
    };
    assert_eq!(log(&dump), Some(1), "the attempt is in the collection");
    forget_keys(&mut dump);
    assert_eq!(log(&dump), Some(0));
}

/// v1.4 (B3): a capture records where its note came from, so the language queries find it —
/// the persisted result read back through `search.cards`. Media origin only where stored
/// media exists (SQLite; the in-memory store does not model media files).
fn captures_are_found_by_language_queries(f: Factory, with_media: bool) {
    use fc_api::v1::search::SearchCards;
    let (t, key) = setup(f);
    let mut v = example(1, "key", LINE);
    if with_media {
        let shot = upload_of(&t, &key, 1, "screenshot", "image/jpeg", JPEG).unwrap();
        let audio = upload_of(&t, &key, 1, "audio", "audio/wav", WAV).unwrap();
        v["media"] = json!({
            "screenshot": {"upload_id": shot, "mime": "image/jpeg"},
            "audio": {"upload_id": audio, "mime": "audio/wav"},
        });
    }
    capture(&t.app, &key, &v).unwrap();
    let count = |q: &str| {
        t.ok::<SearchCardsMethod>(SearchCards {
            query: q.into(),
            sort: None,
            descending: None,
            notes_only: None,
            offset: None,
            limit: None,
        })
        .total
    };
    for q in [
        "nereden:altyazı",
        "from:subtitle kaynak:\"Never Gonna Give You Up\"",
        "source:youtube",
        "kaynak:dQw4w9WgXcQ",
        "kelime:keys dil:en durum:öğreniyorum",
    ] {
        assert_eq!(count(q), 1, "{q}");
    }
    assert_eq!(count("nereden:elle"), 0);
    if with_media {
        assert_eq!(count("ses:video görsel:video"), 1);
        assert_eq!(count("ses:dosya"), 0);
    }
}

#[test]
fn captures_are_found_by_language_queries_sqlite() {
    captures_are_found_by_language_queries(|| Arc::new(TempSqlite::new()), true);
}

#[test]
fn captures_are_found_by_language_queries_mem() {
    captures_are_found_by_language_queries(|| Arc::new(MemStore::new()), false);
}

/// v1.4 W1: encounters (uses the helpers above).
#[path = "encounters_tests.rs"]
mod encounters_tests;
