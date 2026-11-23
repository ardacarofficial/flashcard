//! v1.4 W3 acceptance: the most teachable scene (`examples` module) and the leech repair loop
//! (`stats.leech_repairs.*`). Every check reads the PERSISTED result back
//! (`subtitle.encounters.list`, `notes.get`, `accepted_answers.list`, the review log).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::accepted_answers::AcceptedAnswersRef;
use fc_api::v1::common::Empty;
use fc_api::v1::examples::*;
use fc_api::v1::settings::SetSetting;
use fc_api::v1::stats::*;
use fc_core::card::{CardState, Rating, SchedulingState};
use fc_core::encounter_score::SceneReasonCode;
use fc_core::{ReviewLogId, Timestamp};
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
    encounters_are_ranked_with_reasons_and_nothing_switches_by_itself,
    auto_best_example_switches_on_capture_and_undo_restores,
    a_leech_gets_evidence_repairs_and_an_honest_report,
    a_scene_of_another_meaning_is_proposed_to_move_out,
);

/// A cut line with a dangling dash: the worst scene.
const CUT: &str = "...and then the dumpsters, the keys —";
/// A whole sentence.
const WHOLE: &str = "Almost. I just need to pick up my keys.";
/// An overlapping subtitle line of [`WHOLE`].
const OVERLAP: &str = "I just need to pick up my keys";

fn scene(n: u32, sentence: &str, meaning: &str) -> Value {
    let mut v = example(n, "key", sentence);
    v["definition"]["text"] = json!(meaning);
    v["source"]["t_start_ms"] = json!(1000 * n);
    v
}

fn encounters(t: &TestApp, note: NoteId) -> Vec<SubtitleCaptureRow> {
    t.ok::<SubtitleEncountersList>(ListSubtitleEncounters {
        note_id: Some(note),
        ..Default::default()
    })
    .encounters
}

fn primary(t: &TestApp, note: NoteId) -> String {
    encounters(t, note)
        .into_iter()
        .find(|r| r.is_primary)
        .unwrap()
        .capture_id
}

fn schedule(t: &TestApp, card: CardId) -> SchedulingState {
    t.store.read().unwrap().get_card(card).unwrap().unwrap().scheduling
}

fn encounters_are_ranked_with_reasons_and_nothing_switches_by_itself(f: Factory) {
    let (t, key) = setup(f);
    let first = capture(&t.app, &key, &scene(1, CUT, "anahtar")).unwrap();
    let whole = capture(&t.app, &key, &scene(2, WHOLE, "anahtar")).unwrap();
    let overlap = capture(&t.app, &key, &scene(3, OVERLAP, "anahtar")).unwrap();
    assert!(whole.attached && overlap.attached);
    let ids: Vec<String> = encounters(&t, first.note_id)
        .into_iter()
        .map(|r| r.capture_id)
        .collect();
    let r = t.ok::<SubtitleEncountersRank>(RankEncounters {
        note_id: first.note_id,
    });
    assert_eq!(r.items.len(), 3);
    assert_eq!(r.target, "key");
    assert_eq!(r.items[0].id, ids[1], "the whole sentence ranks first: {r:#?}");
    let worst = r.items.iter().find(|i| i.id == ids[0]).unwrap();
    assert_eq!(worst.rank, 3);
    assert!(worst.is_primary);
    let codes: Vec<SceneReasonCode> = worst.reasons.iter().map(|x| x.code).collect();
    assert!(codes.contains(&SceneReasonCode::CutStart), "{codes:?}");
    assert!(codes.contains(&SceneReasonCode::DanglingDash), "{codes:?}");
    assert!(r.items.iter().all(|i| i.target_found));
    assert_eq!(
        worst.score,
        worst.reasons.iter().map(|x| x.points).sum::<i32>()
    );
    // The overlapping subtitle lines are duplicates of each other.
    assert_eq!(r.items[0].duplicates, vec![ids[2].clone()]);
    let o = r.items.iter().find(|i| i.id == ids[2]).unwrap();
    assert_eq!(o.duplicates, vec![ids[1].clone()]);
    // A suggestion, never a switch.
    let s = t.ok::<SubtitleEncountersSuggestPrimary>(RankEncounters {
        note_id: first.note_id,
    });
    assert_eq!(s.current.as_deref(), Some(ids[0].as_str()));
    assert_eq!(s.suggested.as_deref(), Some(ids[1].as_str()));
    assert!(s.gain >= crate::modules::examples::SUGGEST_MARGIN);
    assert_eq!(primary(&t, first.note_id), ids[0], "suggesting changes nothing");
    // Applying it is the existing command: the schedule stays.
    let before = schedule(&t, first.card_id);
    t.ok::<SubtitleEncountersSetPrimary>(SubtitleEncounterRef {
        capture_id: ids[1].clone(),
    });
    assert_eq!(primary(&t, first.note_id), ids[1]);
    assert!(note(&t, first.note_id).fields["sentence"].contains("pick up my <b>keys</b>"));
    assert_eq!(schedule(&t, first.card_id), before);
    let s = t.ok::<SubtitleEncountersSuggestPrimary>(RankEncounters {
        note_id: first.note_id,
    });
    assert_eq!(s.suggested, None, "the best one is on the card");
    // The module switch hides the methods.
    t.ok::<ModulesSetEnabled>(fc_api::v1::modules::SetModuleEnabled {
        id: crate::modules::examples::ID.into(),
        enabled: false,
    });
    let e = t.err::<SubtitleEncountersRank>(RankEncounters {
        note_id: first.note_id,
    });
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
}

fn auto_best_example_switches_on_capture_and_undo_restores(f: Factory) {
    let (t, key) = setup(f);
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: crate::modules::examples::AUTO_BEST.into(),
        value: json!(true),
    });
    let first = capture(&t.app, &key, &scene(1, CUT, "anahtar")).unwrap();
    let before = schedule(&t, first.card_id);
    let first_id = primary(&t, first.note_id);
    capture(&t.app, &key, &scene(2, WHOLE, "anahtar")).unwrap();
    let rows = encounters(&t, first.note_id);
    assert_eq!(rows.len(), 2);
    let best = rows.iter().find(|r| r.is_primary).unwrap();
    assert_eq!(best.facts.sentence.as_deref(), Some(WHOLE));
    assert!(note(&t, first.note_id).fields["sentence"].contains("pick up my <b>keys</b>"));
    assert_eq!(schedule(&t, first.card_id), before, "schedule untouched");
    // One undo step: the capture and the switch go together.
    t.ok::<OplogUndo>(Empty {});
    let rows = encounters(&t, first.note_id);
    assert_eq!(rows.len(), 1);
    assert_eq!(primary(&t, first.note_id), first_id);
    assert!(note(&t, first.note_id).fields["sentence"].contains("dumpsters"));
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(
        encounters(&t, first.note_id)
            .into_iter()
            .find(|r| r.is_primary)
            .unwrap()
            .facts
            .sentence
            .as_deref(),
        Some(WHOLE)
    );
}

/// Reading-mode candidates with the real English frequency pack: names do not count.
#[test]
fn candidate_sentences_are_ranked_and_names_are_not_unknown() {
    let t = TestApp::with_shipped_word_data(Arc::new(MemStore::new()));
    let r = t.ok::<ExamplesRankSentences>(RankSentences {
        language: "en".into(),
        target: "pick up".into(),
        sentences: vec![
            CandidateSentence {
                text: "Where are you going, Walter?".into(),
                term_offset: None,
                surface: None,
                has_audio: None,
                has_image: None,
            },
            CandidateSentence {
                text: "I'll pick you up at eight, Walter.".into(),
                term_offset: None,
                surface: Some("pick".into()),
                has_audio: Some(true),
                has_image: None,
            },
            CandidateSentence {
                text: "Scrounging through dumpsters, he picked them up —".into(),
                term_offset: None,
                surface: None,
                has_audio: None,
                has_image: None,
            },
        ],
    });
    assert!(r.frequency_pack);
    assert_eq!(r.items[0].id, "1", "{r:#?}");
    assert!(r.items[0].target_found);
    assert!(!r.items[0].unknown_words.iter().any(|w| w == "Walter"));
    let names = r.items[0]
        .reasons
        .iter()
        .find(|x| x.code == SceneReasonCode::NamesIgnored)
        .unwrap();
    assert_eq!(names.words, vec!["Walter"]);
    let missing = r.items.iter().find(|i| i.id == "0").unwrap();
    assert!(!missing.target_found);
    let rare = r.items.iter().find(|i| i.id == "2").unwrap();
    assert!(rare.target_found, "picked … up is the expression");
    assert!(!rare.unknown_words.is_empty(), "{rare:#?}");
    let e = t.err::<ExamplesRankSentences>(RankSentences {
        language: "en".into(),
        target: "pick up".into(),
        sentences: vec![],
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
}

fn log(t: &TestApp, card: CardId, n: u128, rating: Rating, at: Timestamp, mode: &str) {
    let mut tx = t.store.write().unwrap();
    tx.insert_review_log(&ReviewLogRecord {
        id: ReviewLogId(uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0000 + n)),
        card_id: card,
        session_id: None,
        rating,
        state_before: CardState::Review,
        state_after: CardState::Review,
        scheduled_days: 1,
        elapsed_days: 1,
        duration_ms: 4000 + u32::try_from(n).unwrap(),
        answer_duration_ms: Some(3000),
        hints_used: u32::from(rating == Rating::Again),
        mode: mode.into(),
        stability_after: Some(1.0),
        difficulty_after: Some(8.0),
        created_at: at,
    })
    .unwrap();
    tx.commit().unwrap();
}

fn a_leech_gets_evidence_repairs_and_an_honest_report(f: Factory) {
    let (t, key) = setup(f);
    let first = capture(&t.app, &key, &scene(1, CUT, "anahtar; tuş")).unwrap();
    capture(&t.app, &key, &scene(2, WHOLE, "anahtar")).unwrap();
    let ids: Vec<String> = encounters(&t, first.note_id)
        .into_iter()
        .map(|r| r.capture_id)
        .collect();
    assert_eq!(ids.len(), 2);
    // Forgotten eight times; the last scheduled reviews before the repair: 2 of 6 passed.
    let card = first.card_id;
    {
        let mut tx = t.store.write().unwrap();
        let mut c = tx.get_card(card).unwrap().unwrap();
        c.scheduling = SchedulingState {
            state: CardState::Relearning,
            lapses: 8,
            reps: 12,
            ..SchedulingState::NEW
        };
        tx.update_card(&c).unwrap();
        tx.commit().unwrap();
    }
    let t0 = t.app.now().0 - 30 * 86_400_000;
    let day = |d: i64| Timestamp(t0 + d * 86_400_000);
    for (i, r) in [
        Rating::Again,
        Rating::Good,
        Rating::Again,
        Rating::Again,
        Rating::Good,
        Rating::Again,
    ]
    .into_iter()
    .enumerate()
    {
        log(&t, card, i as u128, r, day(i as i64), "normal");
    }
    // A practice review never counts in the comparison.
    log(&t, card, 50, Rating::Good, day(7), "trial");
    let plan = t.ok::<StatsLeechesRepairPlan>(LeechCardRef { card_id: card });
    assert!(plan.is_leech && plan.lapses == 8 && plan.threshold == 8);
    assert_eq!(plan.failed_reviews.len(), 4);
    assert!(plan.failed_reviews[0].at > plan.failed_reviews[1].at, "newest first");
    assert_eq!(plan.failed_reviews[0].hints_used, 1);
    // v1.4 Z1: typed answers are kept by default (`study.keep_typed_answers`); these reviews
    // were not typed.
    assert!(plan.typed_answers_recorded);
    assert!(plan.failed_reviews.iter().all(|r| r.typed_answer.is_none()));
    assert_eq!(plan.answer_field.as_deref(), Some("meaning"));
    assert_eq!(plan.senses, vec!["anahtar", "tuş"]);
    let ex = plan.examples.as_ref().unwrap();
    assert_eq!(ex.items[0].id, ids[1]);
    let switch = plan
        .proposals
        .iter()
        .find(|p| p.why == LeechRepairWhy::BetterExample)
        .expect("a better example is proposed");
    assert_eq!(
        switch.repair,
        LeechRepair::SwitchExample {
            capture_id: ids[1].clone()
        }
    );
    assert_eq!(switch.method, "subtitle.encounters.set_primary");
    assert_eq!(switch.preview[0].after, WHOLE);
    let senses = plan
        .proposals
        .iter()
        .find(|p| p.why == LeechRepairWhy::SensesNotAccepted)
        .expect("each sense alone is proposed as an accepted answer");
    assert_eq!(
        senses.repair,
        LeechRepair::AddAcceptedAnswers {
            field: Some("meaning".into()),
            answers: vec!["anahtar".into(), "tuş".into()],
        }
    );
    // Previewing changed nothing.
    assert_eq!(primary(&t, first.note_id), ids[0]);
    // Apply the better example: recorded, card and schedule kept.
    let before = schedule(&t, card);
    let rec = t.ok::<StatsLeechesRepair>(ApplyLeechRepair {
        card_id: card,
        repair: switch.repair.clone(),
    });
    assert_eq!(rec.kind, LeechRepairKind::SwitchExample);
    assert_eq!(rec.lapses, 8);
    assert_eq!(primary(&t, first.note_id), ids[1]);
    assert_eq!(schedule(&t, card), before);
    let report = t.ok::<StatsLeechesRepairs>(LeechRepairsRequest::default());
    assert_eq!(report.items.len(), 1);
    let o = &report.items[0];
    assert_eq!((o.before.reviews, o.before.passed), (6, 2));
    assert_eq!(o.after.reviews, 0);
    assert_eq!(o.after.rate, None);
    assert!(o.small_sample);
    // Undo: the example and the record go back together; redo brings both, same id.
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(primary(&t, first.note_id), ids[0]);
    assert!(t
        .ok::<StatsLeechesRepairs>(LeechRepairsRequest::default())
        .items
        .is_empty());
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(primary(&t, first.note_id), ids[1]);
    let report = t.ok::<StatsLeechesRepairs>(LeechRepairsRequest {
        card_id: Some(card),
        limit: None,
    });
    assert_eq!(report.items.len(), 1);
    assert_eq!(report.items[0].repair.id, rec.id);
    // The next real reviews after the repair: 3 of 3 passed — still "n is small".
    let applied = report.items[0].repair.applied_at;
    for i in 0..3u128 {
        log(&t, card, 100 + i, Rating::Good, Timestamp(applied.0 + 86_400_000 * (i as i64 + 1)), "normal");
    }
    let o = &t
        .ok::<StatsLeechesRepairs>(LeechRepairsRequest::default())
        .items[0];
    assert_eq!((o.after.reviews, o.after.passed), (3, 3));
    assert_eq!(o.after.rate, Some(1.0));
    assert_eq!(o.next_reviews.len(), 3);
    assert!(o.next_reviews.iter().all(|r| r.passed));
    assert!(o.small_sample, "3 reviews say little yet");
    // Accepted answers: the other repair, read back.
    t.ok::<StatsLeechesRepair>(ApplyLeechRepair {
        card_id: card,
        repair: senses.repair.clone(),
    });
    let lists = t.ok::<AcceptedAnswersList>(AcceptedAnswersRef {
        note_id: first.note_id,
    });
    assert_eq!(lists.fields[0].answers, vec!["anahtar", "tuş"]);
    let plan = t.ok::<StatsLeechesRepairPlan>(LeechCardRef { card_id: card });
    assert_eq!(plan.repairs.len(), 2);
    assert!(plan
        .proposals
        .iter()
        .all(|p| p.why != LeechRepairWhy::SensesNotAccepted && p.why != LeechRepairWhy::BetterExample));
    // The first repair's "after" ends where the second one starts.
    let report = t.ok::<StatsLeechesRepairs>(LeechRepairsRequest::default());
    assert_eq!(report.items.len(), 2);
    assert_eq!(report.items[0].repair.kind, LeechRepairKind::AddAcceptedAnswers);
    // Invalid repairs are refused before anything changes.
    let other = capture(&t.app, &key, &example(9, "door", "Close the door, please.")).unwrap();
    let foreign = encounters(&t, other.note_id)[0].capture_id.clone();
    let e = t.err::<StatsLeechesRepair>(ApplyLeechRepair {
        card_id: card,
        repair: LeechRepair::SwitchExample { capture_id: foreign },
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    let e = t.err::<StatsLeechesRepair>(ApplyLeechRepair {
        card_id: card,
        repair: LeechRepair::AiRewrite {
            fields: Default::default(),
        },
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    assert_eq!(
        t.ok::<StatsLeechesRepairs>(LeechRepairsRequest::default())
            .items
            .len(),
        2
    );
}

fn a_scene_of_another_meaning_is_proposed_to_move_out(f: Factory) {
    let (t, key) = setup(f);
    let first = capture(&t.app, &key, &scene(1, WHOLE, "anahtar")).unwrap();
    // Another meaning: its own note, flagged; the user merges them anyway (a mistake).
    let piano = "She played the keys of the old piano.";
    let other = capture(&t.app, &key, &scene(2, piano, "tuş")).unwrap();
    assert!(other.needs_review && other.note_id != first.note_id);
    let merged = t.ok::<SubtitleEncountersMergeNotes>(MergeSubtitleNotes {
        a: first.note_id,
        b: other.note_id,
        force: None,
    });
    assert_eq!(merged.kept_note_id, first.note_id);
    let moved = encounters(&t, first.note_id)
        .into_iter()
        .find(|r| r.facts.sentence.as_deref() == Some(piano))
        .unwrap()
        .capture_id;
    let plan = t.ok::<StatsLeechesRepairPlan>(LeechCardRef {
        card_id: first.card_id,
    });
    let p = plan
        .proposals
        .iter()
        .find(|p| p.why == LeechRepairWhy::MixedMeanings)
        .expect("the other meaning's scene is proposed to move out");
    assert_eq!(
        p.repair,
        LeechRepair::MoveMeaning {
            capture_id: moved.clone(),
            to_note_id: None
        }
    );
    assert_eq!(p.method, "subtitle.encounters.move");
    t.ok::<StatsLeechesRepair>(ApplyLeechRepair {
        card_id: first.card_id,
        repair: p.repair.clone(),
    });
    let left = encounters(&t, first.note_id);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].facts.sentence.as_deref(), Some(WHOLE));
    let word = t.ok::<SubtitleEncountersList>(ListSubtitleEncounters {
        lang: Some("en".into()),
        lemma: Some("key".into()),
        ..Default::default()
    });
    let new_note = word
        .encounters
        .iter()
        .find(|r| r.capture_id == moved)
        .unwrap()
        .note_id;
    assert_ne!(new_note, first.note_id);
    assert!(note(&t, new_note).fields["meaning"].contains("tuş"));
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(encounters(&t, first.note_id).len(), 2);
    assert!(t
        .ok::<StatsLeechesRepairs>(LeechRepairsRequest::default())
        .items
        .is_empty());
}
