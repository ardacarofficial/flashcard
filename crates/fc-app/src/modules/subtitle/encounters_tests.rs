//! v1.4 W1 acceptance: a capture is an encounter, not a card (three stores). Every check
//! reads the PERSISTED result back (`subtitle.encounters.list`, `notes.get`, the store).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::encounters::{ALWAYS_NEW_CARD, SAME_WORD};
use super::*;
use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::common::Empty;
use fc_api::v1::notes::NoteRef;
use fc_core::card::{CardState, SchedulingState};
use fc_core::card::Rating;
use fc_core::ReviewLogId;
use fc_storage::repo::ReviewLogRecord;
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
    a_resend_does_not_add_a_card,
    two_scenes_of_one_meaning_are_one_card_with_two_examples,
    a_different_meaning_stays_separate_until_the_user_decides,
    switching_the_primary_example_leaves_the_schedule_identical,
    moving_and_merging_keep_the_note_with_more_history,
    always_new_card_is_the_v1_3_behaviour,
);

const LINE2: &str = "Where are my keys?";

fn by_note(t: &TestApp, note: NoteId) -> Vec<SubtitleCaptureRow> {
    t.ok::<SubtitleEncountersList>(ListSubtitleEncounters {
        note_id: Some(note),
        ..Default::default()
    })
    .encounters
}

fn live_cards(t: &TestApp) -> usize {
    t.store
        .read()
        .unwrap()
        .list_cards(&fc_storage::repo::CardFilter::default())
        .unwrap()
        .len()
}

fn with_meaning(mut v: Value, meaning: &str) -> Value {
    v["definition"]["text"] = json!(meaning);
    v
}

fn a_resend_does_not_add_a_card(f: Factory) {
    let (t, key) = setup(f);
    let first = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    // The extension lost the answer and sends again, alone and in a batch.
    let again = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    assert_eq!(again.status, SubtitleCaptureStatus::Duplicate);
    let b = batch(&t.app, &key, &json!({ "captures": [example(1, "key", LINE)] })).unwrap();
    assert_eq!(b.results[0].status, SubtitleBatchStatus::Duplicate);
    assert_eq!(b.results[0].card_id, Some(first.card_id.to_string()));
    assert_eq!(live_cards(&t), 1);
    assert_eq!(by_note(&t, first.note_id).len(), 1);
}

fn two_scenes_of_one_meaning_are_one_card_with_two_examples(f: Factory) {
    let (t, key) = setup(f);
    let first = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    let mut second_scene = example(2, "key", LINE2);
    second_scene["source"]["t_start_ms"] = json!(120_000);
    second_scene["source"]["t_end_ms"] = json!(121_500);
    second_scene["definition"]["text"] = json!("Anahtar.");
    let o = capture(&t.app, &key, &second_scene).unwrap();
    // Stored (201 on the wire), no new card.
    assert_eq!(o.status, SubtitleCaptureStatus::Created);
    assert!(o.attached && !o.needs_review);
    assert_eq!((o.note_id, o.card_id), (first.note_id, first.card_id));
    assert_eq!(live_cards(&t), 1);
    let rows = by_note(&t, first.note_id);
    assert_eq!(rows.len(), 2);
    assert!(rows[0].is_primary && !rows[1].is_primary);
    let f2 = &rows[1].facts;
    assert_eq!(f2.sentence.as_deref(), Some(LINE2));
    assert_eq!(f2.video_id.as_deref(), Some("dQw4w9WgXcQ"));
    assert_eq!((f2.t_start_ms, f2.t_end_ms), (Some(120_000), Some(121_500)));
    assert_eq!(f2.kind, Some(SubtitleCaptureKind::Word));
    assert_eq!(f2.definition_source, Some(SubtitleDefinitionSource::Dict));
    assert_eq!(f2.term_offset, Some((34, 38)));
    // The card shows the first scene; the second is listed in `examples`.
    let n = note(&t, first.note_id);
    assert!(n.fields["sentence"].contains("pick up my <b>keys</b>"));
    assert!(n.fields["examples"].contains("Where are my <b>keys</b>?"));
    assert!(n.fields["examples"].contains("2:00"));
    // By video and by word too.
    let by_video = t.ok::<SubtitleEncountersList>(ListSubtitleEncounters {
        video_id: Some("dQw4w9WgXcQ".into()),
        ..Default::default()
    });
    assert_eq!(by_video.encounters.len(), 2);
    let by_word = t.ok::<SubtitleEncountersList>(ListSubtitleEncounters {
        lang: Some("EN".into()),
        lemma: Some("Key".into()),
        ..Default::default()
    });
    assert_eq!(by_word.encounters.len(), 2);
    assert_eq!(
        t.err::<SubtitleEncountersList>(ListSubtitleEncounters::default()).code,
        ErrorCode::ValidationSubtitleEncounter
    );
    // Diagnostics say what happened.
    let recent = t.ok::<SubtitleCapturesRecent>(RecentSubtitleCaptures { limit: None });
    assert_eq!(recent.attempts[0].result, SubtitleAttemptResult::Attached);
    // One undo step takes the example back; the card stays.
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(by_note(&t, first.note_id).len(), 1);
    assert!(note(&t, first.note_id).fields.get("examples").is_none_or(String::is_empty));
    assert_eq!(live_cards(&t), 1);
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(by_note(&t, first.note_id).len(), 2);
}

fn a_different_meaning_stays_separate_until_the_user_decides(f: Factory) {
    let (t, key) = setup(f);
    let door = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    let piano = capture(&t.app, &key, &with_meaning(example(2, "key", LINE2), "tuş")).unwrap();
    assert!(!piano.attached && piano.needs_review);
    assert_ne!(piano.note_id, door.note_id);
    assert_eq!(live_cards(&t), 2);
    let flagged = t.ok::<SubtitleEncountersList>(ListSubtitleEncounters {
        needs_review: Some(true),
        ..Default::default()
    });
    assert_eq!(flagged.encounters.len(), 1);
    assert_eq!(flagged.encounters[0].review_note_id, Some(door.note_id));
    // "farklı anlam": stays separate, the flag goes.
    let id = flagged.encounters[0].capture_id.clone();
    t.ok::<SubtitleEncountersResolve>(ResolveSubtitleEncounter {
        capture_id: id.clone(),
        same_meaning: false,
    });
    assert_eq!(by_note(&t, piano.note_id)[0].review_note_id, None);
    assert_eq!(live_cards(&t), 2);
    // Undo brings the question back; "aynı anlam" merges (the older note stays).
    t.ok::<OplogUndo>(Empty {});
    let merged = t.ok::<SubtitleEncountersResolve>(ResolveSubtitleEncounter {
        capture_id: id,
        same_meaning: true,
    });
    assert_eq!(merged.encounters.len(), 2);
    assert!(merged.encounters.iter().all(|e| e.note_id == door.note_id));
    assert!(merged.encounters.iter().all(|e| e.review_note_id.is_none()));
    assert_eq!(live_cards(&t), 1);
    assert!(t
        .store
        .read()
        .unwrap()
        .get_note(piano.note_id)
        .unwrap()
        .unwrap()
        .deleted_at
        .is_some());
}

fn schedule(t: &TestApp, card: CardId) -> SchedulingState {
    t.store.read().unwrap().get_card(card).unwrap().unwrap().scheduling
}

fn review(t: &TestApp, card: CardId, n: u128) {
    let now = t.app.now();
    let mut tx = t.store.write().unwrap();
    let mut c = tx.get_card(card).unwrap().unwrap();
    c.scheduling = SchedulingState {
        state: CardState::Review,
        due: Some(now.plus_days(9)),
        stability: Some(8.25),
        difficulty: Some(5.5),
        last_review: Some(now),
        reps: 2,
        ..SchedulingState::NEW
    };
    tx.update_card(&c).unwrap();
    tx.insert_review_log(&ReviewLogRecord {
        id: ReviewLogId(uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0000 + n)),
        card_id: card,
        session_id: None,
        rating: Rating::Good,
        state_before: CardState::New,
        state_after: CardState::Review,
        scheduled_days: 9,
        elapsed_days: 0,
        duration_ms: 1000,
        answer_duration_ms: None,
        hints_used: 0,
        mode: "normal".into(),
        stability_after: Some(8.25),
        difficulty_after: Some(5.5),
        created_at: now,
    })
    .unwrap();
    crate::modules::words::index::drain(&mut *tx).unwrap();
    tx.commit().unwrap();
}

fn switching_the_primary_example_leaves_the_schedule_identical(f: Factory) {
    let (t, key) = setup(f);
    let first = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    capture(&t.app, &key, &example(2, "key", LINE2)).unwrap();
    review(&t, first.card_id, 1);
    let before = schedule(&t, first.card_id);
    let history = t
        .store
        .read()
        .unwrap()
        .review_logs_for_card(first.card_id)
        .unwrap();
    let second = by_note(&t, first.note_id)[1].capture_id.clone();
    let after = t.ok::<SubtitleEncountersSetPrimary>(SubtitleEncounterRef {
        capture_id: second.clone(),
    });
    assert_eq!(after.encounters[0].capture_id, second);
    assert!(after.encounters[0].is_primary && !after.encounters[1].is_primary);
    // The card shows the other scene now…
    let n = note(&t, first.note_id);
    assert!(n.fields["sentence"].contains("Where are my <b>keys</b>?"));
    assert!(n.fields["examples"].contains("pick up my <b>keys</b>"));
    assert_eq!(n.cards.len(), 1);
    assert_eq!(n.cards[0].id, first.card_id);
    // …with the same card id, due date, stability and history.
    assert_eq!(schedule(&t, first.card_id), before);
    assert_eq!(
        t.store
            .read()
            .unwrap()
            .review_logs_for_card(first.card_id)
            .unwrap(),
        history
    );
    // Undo puts the first scene back on the card.
    t.ok::<OplogUndo>(Empty {});
    assert!(note(&t, first.note_id).fields["sentence"].contains("pick up my <b>keys</b>"));
    assert!(by_note(&t, first.note_id)[0].capture_id != second);
    assert_eq!(schedule(&t, first.card_id), before);
}

fn moving_and_merging_keep_the_note_with_more_history(f: Factory) {
    let (t, key) = setup(f);
    let first = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    capture(&t.app, &key, &example(2, "key", LINE2)).unwrap();
    let second = by_note(&t, first.note_id)[1].capture_id.clone();
    // "This is a different meaning": its own note and card.
    let moved = t.ok::<SubtitleEncountersMove>(MoveSubtitleEncounter {
        capture_id: second.clone(),
        to_note_id: None,
    });
    let new_note = moved.encounters[0].note_id;
    assert_ne!(new_note, first.note_id);
    assert!(moved.encounters[0].is_primary);
    assert_eq!(live_cards(&t), 2);
    assert!(note(&t, new_note).fields["sentence"].contains("Where are my"));
    assert!(note(&t, first.note_id).fields.get("examples").is_none_or(String::is_empty));
    // The only encounter of a note cannot move.
    assert_eq!(
        t.err::<SubtitleEncountersMove>(MoveSubtitleEncounter {
            capture_id: second,
            to_note_id: Some(first.note_id),
        })
        .code,
        ErrorCode::ValidationSubtitleEncounter
    );
    // The newer note has the history: it is kept; the older one is archived.
    review(&t, moved.encounters[0].card_id, 7);
    let kept_schedule = schedule(&t, moved.encounters[0].card_id);
    let m = t.ok::<SubtitleEncountersMergeNotes>(MergeSubtitleNotes {
        a: first.note_id,
        b: new_note,
    });
    assert_eq!((m.kept_note_id, m.archived_note_id), (new_note, first.note_id));
    assert_eq!((m.moved, m.archived_reviews), (1, 0));
    assert_eq!(by_note(&t, new_note).len(), 2);
    assert_eq!(live_cards(&t), 1);
    assert_eq!(schedule(&t, moved.encounters[0].card_id), kept_schedule);
    let archived = t.store.read().unwrap().get_note(first.note_id).unwrap().unwrap();
    assert!(archived.deleted_at.is_some());
    // Undo: both notes again, each with its own encounter.
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(live_cards(&t), 2);
    assert_eq!(by_note(&t, first.note_id).len(), 1);
    assert_eq!(by_note(&t, new_note).len(), 1);
    // Archived reviews stay in the log: the older note reviewed, merged into the newer.
    review(&t, first.card_id, 8);
    review(&t, first.card_id, 9);
    let m = t.ok::<SubtitleEncountersMergeNotes>(MergeSubtitleNotes {
        a: first.note_id,
        b: new_note,
    });
    assert_eq!((m.kept_note_id, m.archived_reviews), (first.note_id, 1));
    assert_eq!(
        t.store
            .read()
            .unwrap()
            .review_logs_for_card(moved.encounters[0].card_id)
            .unwrap()
            .len(),
        1,
        "the archived note's history is kept"
    );
}

fn always_new_card_is_the_v1_3_behaviour(f: Factory) {
    let (t, key) = setup(f);
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: SAME_WORD.into(),
        value: json!(ALWAYS_NEW_CARD),
    });
    let a = capture(&t.app, &key, &example(1, "key", LINE)).unwrap();
    let b = capture(&t.app, &key, &example(2, "key", LINE2)).unwrap();
    assert_ne!(a.note_id, b.note_id);
    assert!(!b.attached && !b.needs_review);
    assert_eq!(live_cards(&t), 2);
    let _ = NoteRef { id: a.note_id };
}

/// Backup → restore into a clean profile: the encounters come back, and the note's audio is
/// the same file, byte for byte.
#[test]
fn restore_into_a_clean_profile_plays_the_same_audio() {
    use fc_api::v1::io::{BackupReport, CreateBackup, RestoreBackup, RestoreReport};
    use fc_api::v1::jobs::{JobRef, JobStatus};
    let profile = || {
        let db = TempSqlite::new();
        let data = db.path().parent().unwrap().to_path_buf();
        let t = TestApp::new(Arc::new(db));
        t.app.set_data_dir(data);
        t
    };
    let finish = |t: &TestApp, id| {
        t.app.run_pending_jobs().unwrap();
        let j = t.ok::<JobsGet>(JobRef { id });
        assert_eq!(j.status, JobStatus::Succeeded, "{:?}", j.error);
        j.result.unwrap()
    };
    let a = profile();
    enable(&a);
    let (key, _) = new_key(&a, "Chrome");
    let up = upload_of(&a, &key, 1, "audio", "audio/wav", WAV).unwrap();
    let mut v = example(1, "key", LINE);
    v["media"]["audio"] = json!({ "upload_id": up, "mime": "audio/wav" });
    let o = capture(&a.app, &key, &v).unwrap();
    capture(&a.app, &key, &example(2, "key", LINE2)).unwrap();
    let audio = note(&a, o.note_id).fields["audio"].clone();
    let hash = audio.strip_prefix("media:").unwrap().to_owned();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("b.fcbackup").to_string_lossy().into_owned();
    let job = a.ok::<IoBackupCreate>(CreateBackup { path: path.clone() });
    let rep: BackupReport = serde_json::from_value(finish(&a, job.id)).unwrap();
    assert!(rep.missing_media.is_empty());

    let b = profile();
    let job = b.ok::<IoBackupRestore>(RestoreBackup {
        path,
        safety_backup_path: None,
    });
    let _: RestoreReport = serde_json::from_value(finish(&b, job.id)).unwrap();
    assert_eq!(note(&b, o.note_id).fields["audio"], audio);
    assert_eq!(
        b.store.read().unwrap().read_media_file(&hash).unwrap().as_deref(),
        Some(WAV)
    );
    let rows = by_note(&b, o.note_id);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].facts.audio_hash.as_deref(), Some(hash.as_str()));
    assert_eq!(rows[1].facts.sentence.as_deref(), Some(LINE2));
}
