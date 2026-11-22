//! v1.4 meaning in context and accepted answers, every provider call against the local mock
//! server (no real key is available here). Acceptance of the brief:
//! * good JSON → `the_meaning_in_this_sentence_is_asked_previewed_and_applied_on_request`
//!   (Claude) and `a_local_model_is_repaired_once_after_bad_json` (Ollama, bad JSON then good);
//! * refusal → `a_refusal_writes_nothing`; timeout → `a_timeout_writes_nothing`;
//! * provider off → `with_the_module_or_provider_off_nothing_is_sent_or_written`;
//! * budget exceeded → `automatic_requests_stop_at_the_budget`;
//! * accepted answers pass the offline check → `accepted_answers_pass_the_written_check_offline`.

use super::tests::{app, claude, claude_text, claude_tool, deck, result, run, KEY};
use super::*;
use crate::testing::TestApp;
use fc_ai::mock::{MockResponse, MockServer};
use fc_api::v1::accepted_answers::{AcceptedAnswersRef, EditAcceptedAnswers};
use fc_api::v1::ai::*;
use fc_api::v1::common::Empty;
use fc_api::v1::jobs::{JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::{CreateNote, NoteDto, NoteRef};
use fc_api::v1::oplog::Actor;
use fc_api::v1::settings::SetSetting;
use fc_api::v1::study::{AnswerGrade, CheckAnswer, SessionDto};
use fc_api::ErrorCode;
use fc_core::content::builtin;
use fc_core::settings::SettingScope;
use fc_core::{CardId, DeckId};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const SENTENCE: &str = "Can you pick up the kids at five?";

fn good() -> Value {
    json!({
        "meaning": "almak (okuldan)",
        "phrase": "pick up the kids",
        "expression": true,
        "also_correct": ["okuldan almak", "alıp getirmek"],
        "not_this": ["kaldırmak", "öğrenmek"],
        "confidence": 0.9
    })
}

/// A Subtitle note: "pick up" whose source meaning is "kaldırmak".
fn pick_up(t: &TestApp, deck: DeckId) -> NoteDto {
    subtitle(t, deck, "pick up", "kaldırmak", SENTENCE)
}

fn subtitle(t: &TestApp, deck: DeckId, word: &str, meaning: &str, sentence: &str) -> NoteDto {
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: builtin::find(builtin::SUBTITLE).unwrap().id,
        deck_id: deck,
        fields: [
            ("word", word),
            ("surface", word),
            ("meaning", meaning),
            ("sentence", sentence),
            ("translation", ""),
            ("source", "Film · 1:23"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect(),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    })
}

fn task(note: &NoteDto) -> AiTask {
    AiTask::MeaningInContext {
        note_id: Some(note.id),
        sentence: None,
        lemma: None,
        surface: None,
        position: None,
        target_language: None,
        candidates: vec!["almak".into()],
    }
}

fn meaning_state(t: &TestApp, note: &NoteDto) -> AiNoteMeaningState {
    t.ok::<AiMeaningGet>(AiMeaningRef { note_id: note.id })
}

fn field(t: &TestApp, note: &NoteDto, key: &str) -> String {
    t.ok::<NotesGet>(NoteRef { id: note.id }).fields[key].clone()
}

fn set(t: &TestApp, key: &str, value: Value) {
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: key.into(),
        value,
    });
}

/// Nothing about the note's meaning was written.
fn untouched(t: &TestApp, note: &NoteDto) {
    let s = meaning_state(t, note);
    assert!(s.meaning.is_none() && s.accepted.is_empty(), "{s:?}");
    assert_eq!(field(t, note, "meaning"), "kaldırmak");
}

fn ollama(t: &TestApp, server: &MockServer) -> AiProviderDto {
    t.ok::<AiProvidersCreate>(CreateAiProvider {
        kind: AiProviderKind::Ollama,
        name: None,
        base_url: Some(server.url()),
        default_model: Some("llama3.2".into()),
        is_default: None,
    })
}

fn ollama_text(text: &str) -> MockResponse {
    MockResponse::json(&json!({
        "model": "llama3.2",
        "message": {"role": "assistant", "content": text},
        "done": true,
        "done_reason": "stop",
        "prompt_eval_count": 300,
        "eval_count": 60
    }))
}

#[test]
fn the_meaning_in_this_sentence_is_asked_previewed_and_applied_on_request() {
    let server = MockServer::start(|_| claude_tool(good()));
    let t = app();
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "English");
    let note = pick_up(&t, d.id);

    // The preview shows the sentence, the word, the offered meanings (the note's own too) and
    // the answer language; the answer must follow a schema.
    let p = t.ok::<AiPreviewTask>(PreviewAi {
        task: task(&note),
        provider_id: None,
        model: None,
    });
    let text = &p.messages[0].text;
    for want in [SENTENCE, "Word: pick up", "almak; kaldırmak", "Answer in Turkish"] {
        assert!(text.contains(want), "{want}: {text}");
    }
    assert!(p.structured && p.feature == AiFeature::MeaningInContext);
    assert!(!text.contains("Film"), "the source is not sent: {text}");

    let r = result(&run(&t, task(&note)));
    let AiOutput::MeaningInContext {
        note_id,
        field: f,
        meaning,
        phrase,
        expression,
        also_correct,
        not_this,
        confidence,
        original,
    } = r.output.clone()
    else {
        panic!("{:?}", r.output)
    };
    assert_eq!((note_id, f.as_deref()), (Some(note.id), Some("meaning")));
    assert_eq!((meaning.as_str(), phrase.as_str(), expression), ("almak (okuldan)", "pick up the kids", true));
    assert_eq!(also_correct, ["okuldan almak", "alıp getirmek"]);
    assert_eq!(not_this, ["kaldırmak", "öğrenmek"]);
    assert!((confidence - 0.9).abs() < 1e-6);
    assert_eq!(original, "kaldırmak");
    let sent = server.requests()[0].json();
    assert_eq!(sent["tool_choice"]["name"], "emit_result");
    assert_eq!(
        sent["tools"][0]["input_schema"]["required"],
        json!(["meaning", "phrase", "expression", "also_correct", "not_this", "confidence"])
    );
    // Asking wrote nothing: the source's meaning stands.
    untouched(&t, &note);

    // Applying without replacing: stored apart, the field keeps the source's wording, the
    // applied meaning and the kept alternative become accepted answers. One undo step.
    let s = t.ok::<AiMeaningApply>(ApplyAiMeaning {
        note_id: note.id,
        meaning,
        phrase,
        expression,
        also_correct: also_correct.clone(),
        not_this,
        confidence: Some(confidence),
        model: Some(r.model.clone()),
        edited: false,
        accepted: vec!["okuldan almak".into()],
        replace_field: false,
    });
    let m = s.meaning.clone().unwrap();
    assert_eq!((m.status, m.original.as_str(), m.field.as_str()), (AiMeaningStatus::Applied, "kaldırmak", "meaning"));
    assert_eq!(m.model, "claude-sonnet-5-5");
    assert_eq!(s.accepted[0].answers, ["almak (okuldan)", "okuldan almak"]);
    assert_eq!(field(&t, &note, "meaning"), "kaldırmak");
    t.ok::<OplogUndo>(Empty {});
    untouched(&t, &note);
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(meaning_state(&t, &note), s);

    // Applying an edited meaning INTO the field: the source's definition stays `original`.
    let s2 = t.ok::<AiMeaningApply>(ApplyAiMeaning {
        note_id: note.id,
        meaning: "almak".into(),
        phrase: "pick up the kids".into(),
        expression: true,
        also_correct: vec![],
        not_this: vec![],
        confidence: None,
        model: None,
        edited: true,
        accepted: vec!["alıp getirmek".into()],
        replace_field: true,
    });
    assert_eq!(field(&t, &note, "meaning"), "almak");
    let m2 = s2.meaning.unwrap();
    assert_eq!((m2.original.as_str(), m2.edited, m2.created_at), ("kaldırmak", true, m.created_at));
    assert_eq!(s2.accepted[0].answers, ["almak (okuldan)", "okuldan almak", "alıp getirmek"]);
    // The whole apply (row, accepted answers, field) is one undo step.
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(meaning_state(&t, &note), s);
    assert_eq!(field(&t, &note, "meaning"), "kaldırmak");

    // The persisted result reads back the same from a SQLite collection too (see the
    // storage tests); here: a bad apply is refused and changes nothing.
    let e = t.err::<AiMeaningApply>(ApplyAiMeaning {
        note_id: note.id,
        meaning: "  ".into(),
        phrase: String::new(),
        expression: false,
        also_correct: vec![],
        not_this: vec![],
        confidence: Some(2.0),
        model: None,
        edited: false,
        accepted: vec![],
        replace_field: true,
    });
    assert_eq!(e.code, ErrorCode::ValidationAiTask);
    assert_eq!(meaning_state(&t, &note), s);
}

#[test]
fn a_local_model_is_repaired_once_after_bad_json() {
    let calls = Arc::new(AtomicUsize::new(0));
    let c = Arc::clone(&calls);
    let server = MockServer::start(move |_| match c.fetch_add(1, Ordering::SeqCst) {
        0 => ollama_text("Sure! In this sentence it means almak."),
        _ => ollama_text(&format!("```json\n{}\n```", good())),
    });
    let t = app();
    ollama(&t, &server);
    let d = deck(&t, "English");
    let note = pick_up(&t, d.id);
    let r = result(&run(&t, task(&note)));
    assert_eq!((r.attempts, r.input_tokens, r.cost_usd), (2, 600, None));
    assert!(matches!(r.output, AiOutput::MeaningInContext { ref meaning, .. } if meaning == "almak (okuldan)"));
    let reqs = server.requests();
    assert_eq!(reqs[0].path, "/api/chat");
    // The repair carries the bad answer and what was wrong with it.
    let repair = reqs[1].json()["messages"].to_string();
    assert!(repair.contains("not valid JSON"), "{repair}");
    // A phrase that is not in the sentence is refused the same way (then fails: nothing kept).
    let calls = Arc::new(AtomicUsize::new(0));
    let c = Arc::clone(&calls);
    let server2 = MockServer::start(move |_| {
        c.fetch_add(1, Ordering::SeqCst);
        let mut v = good();
        v["phrase"] = json!("pick up a language");
        ollama_text(&v.to_string())
    });
    let t2 = app();
    ollama(&t2, &server2);
    let d2 = deck(&t2, "English");
    let note2 = pick_up(&t2, d2.id);
    let job = run(&t2, task(&note2));
    assert_eq!(job.status, JobStatus::Failed);
    let err = job.error.unwrap();
    assert_eq!(err.code, ErrorCode::ProviderAiInvalidOutput);
    assert!(format!("{:?}", err.details).contains("phrase"), "{err:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 2, "one repair, no more");
    untouched(&t2, &note2);
}

#[test]
fn a_refusal_writes_nothing() {
    let server = MockServer::start(|_| {
        MockResponse::json(&json!({
            "model": "claude-sonnet-5-5",
            "content": [{"type": "text", "text": "I can't help with that."}],
            "stop_reason": "refusal",
            "usage": {"input_tokens": 50, "output_tokens": 8}
        }))
    });
    let t = app();
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "English");
    let note = pick_up(&t, d.id);
    let job = run(&t, task(&note));
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, ErrorCode::ProviderAiInvalidOutput);
    assert_eq!(server.requests().len(), 2, "asked once more, then gave up");
    // The refused requests still cost tokens: they are on the ledger.
    assert_eq!(t.ok::<AiUsageGet>(Empty {}).month.requests, 2);
    untouched(&t, &note);
}

#[test]
fn a_timeout_writes_nothing() {
    let server = MockServer::start(|_| {
        MockResponse::stream("application/json", vec![super::tests::claude_text("x").body], 2_000)
    });
    let t = app();
    t.app.ai_services().unwrap().set_timeout_ms(200);
    set(&t, RETRIES, json!(0));
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "English");
    let note = pick_up(&t, d.id);
    let job = run(&t, task(&note));
    assert_eq!(job.status, JobStatus::Failed, "{:?}", job.result);
    let err = job.error.unwrap();
    assert!(
        matches!(err.code, ErrorCode::ProviderUnavailable | ErrorCode::ProviderFailed),
        "{err:?}"
    );
    untouched(&t, &note);
}

#[test]
fn with_the_module_or_provider_off_nothing_is_sent_or_written() {
    // No provider: a clear error before anything is built.
    let t = app();
    let d = deck(&t, "English");
    let note = pick_up(&t, d.id);
    let e = t.err::<AiPreviewTask>(PreviewAi {
        task: task(&note),
        provider_id: None,
        model: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictAiNoProvider);

    // A request queued, then the module turned off: it never leaves, nothing is written.
    let server = MockServer::start(|_| claude_tool(good()));
    claude(&t, &server, Some(KEY));
    let p = t.ok::<AiPreviewTask>(PreviewAi {
        task: task(&note),
        provider_id: None,
        model: None,
    });
    let job = t.ok::<AiRun>(RunAi {
        task: task(&note),
        provider_id: None,
        model: None,
        preview_hash: p.preview_hash,
    });
    t.ok::<ModulesSetEnabled>(SetModuleEnabled { id: ID.into(), enabled: false });
    t.app.run_pending_jobs().unwrap();
    let job = t.ok::<JobsGet>(JobRef { id: job.id });
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, ErrorCode::ConflictModuleDisabled);
    assert!(server.requests().is_empty());
    for e in [
        t.err::<AiMeaningGet>(AiMeaningRef { note_id: note.id }),
        t.err::<AiMeaningSuggest>(AiMeaningRef { note_id: note.id }),
    ] {
        assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
    }
    t.ok::<ModulesSetEnabled>(SetModuleEnabled { id: ID.into(), enabled: true });
    untouched(&t, &note);

    // A note without a sentence: refused with a reason, nothing sent.
    let bare = subtitle(&t, d.id, "give up", "vazgeçmek", "");
    let e = t.err::<AiPreviewTask>(PreviewAi {
        task: task(&bare),
        provider_id: None,
        model: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationAiTask);
    assert!(server.requests().is_empty());
}

#[test]
fn asking_without_a_preview_needs_the_opt_in_and_only_suggests() {
    let server = MockServer::start(|_| claude_tool(good()));
    let t = app();
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "English");
    let note = pick_up(&t, d.id);

    // Off by default: refused, nothing queued or sent.
    let e = t.err::<AiMeaningSuggest>(AiMeaningRef { note_id: note.id });
    assert_eq!(e.code, ErrorCode::ConflictAiAutoOff);
    assert_eq!(t.app.run_pending_jobs().unwrap(), 0);
    assert!(server.requests().is_empty());

    // Opted in: the answer is kept as a suggestion; the note is unchanged.
    set(&t, AUTO_SUGGEST, json!(true));
    let job = t.ok::<AiMeaningSuggest>(AiMeaningRef { note_id: note.id });
    t.app.run_pending_jobs().unwrap();
    assert_eq!(t.ok::<JobsGet>(JobRef { id: job.id }).status, JobStatus::Succeeded);
    let s = meaning_state(&t, &note);
    let m = s.meaning.unwrap();
    assert_eq!((m.status, m.meaning.as_str(), m.original.as_str()), (AiMeaningStatus::Suggested, "almak (okuldan)", "kaldırmak"));
    assert!(s.accepted.is_empty());
    assert_eq!(field(&t, &note, "meaning"), "kaldırmak");
    // Storing a suggestion is no undo step.
    let oplog = serde_json::to_string(&t.ok::<OplogList>(Default::default())).unwrap();
    assert!(!oplog.contains("ai.meaning.store_suggestion"), "{oplog}");

    // Applied once, a later suggestion never replaces the user's choice.
    t.ok::<AiMeaningApply>(ApplyAiMeaning {
        note_id: note.id,
        meaning: "almak".into(),
        phrase: "pick up".into(),
        expression: false,
        also_correct: vec![],
        not_this: vec![],
        confidence: None,
        model: None,
        edited: true,
        accepted: vec![],
        replace_field: false,
    });
    t.ok::<AiMeaningSuggest>(AiMeaningRef { note_id: note.id });
    t.app.run_pending_jobs().unwrap();
    let m = meaning_state(&t, &note).meaning.unwrap();
    assert_eq!((m.status, m.meaning.as_str()), (AiMeaningStatus::Applied, "almak"));
}

#[test]
fn automatic_requests_stop_at_the_budget() {
    let server = MockServer::start(|_| claude_tool(good()));
    let t = app();
    claude(&t, &server, Some(KEY));
    set(&t, AUTO_SUGGEST, json!(true));
    let d = deck(&t, "English");
    let note = pick_up(&t, d.id);
    let other = subtitle(&t, d.id, "pick", "toplamak", "We pick up the kids at five.");

    // A previewed request spends $0.006 (1000 in + 200 out at $3/$15).
    result(&run(&t, task(&note)));
    // An automatic one is queued while there is no budget …
    let queued = t.ok::<AiMeaningSuggest>(AiMeaningRef { note_id: other.id });
    // … then the user sets a budget that is already spent: the job sends nothing.
    set(&t, BUDGET, json!(0.005));
    t.app.run_pending_jobs().unwrap();
    let q = t.ok::<JobsGet>(JobRef { id: queued.id });
    assert_eq!(q.status, JobStatus::Failed);
    assert_eq!(q.error.unwrap().code, ErrorCode::ConflictAiBudgetExceeded);
    assert_eq!(server.requests().len(), 1);
    assert!(meaning_state(&t, &other).meaning.is_none());
    assert_eq!(field(&t, &other, "meaning"), "toplamak");
    assert!(t.ok::<AiUsageGet>(Empty {}).budget.exceeded);

    // Over the budget: refused up front, nothing queued or sent.
    let e = t.err::<AiMeaningSuggest>(AiMeaningRef { note_id: note.id });
    assert_eq!(e.code, ErrorCode::ConflictAiBudgetExceeded);
    assert_eq!(t.app.run_pending_jobs().unwrap(), 0);
    assert_eq!(server.requests().len(), 1);
    untouched(&t, &note);
    // A previewed request still shows the budget and leaves the choice to the user.
    let p = t.ok::<AiPreviewTask>(PreviewAi {
        task: task(&note),
        provider_id: None,
        model: None,
    });
    assert!(p.budget.exceeded);
}

/// A practice session showing `card`.
fn session_on(t: &TestApp, deck: DeckId, card: CardId) -> SessionDto {
    let v = t
        .app
        .invoke(
            "v1.study.start",
            json!({ "kind": "normal", "mode": "trial", "deck_id": deck, "tz_offset_mins": 0 }),
            &Actor::User,
        )
        .unwrap();
    let s: SessionDto = serde_json::from_value(v).unwrap();
    assert_eq!(s.current.as_ref().unwrap().card_id, card);
    s
}

fn check(t: &TestApp, s: &SessionDto, card: CardId, given: &str) -> AnswerGrade {
    t.ok::<StudyCheckAnswer>(CheckAnswer {
        session_id: s.id,
        card_id: card,
        given: given.into(),
        choice: None,
    })
    .grade
}

#[test]
fn accepted_answers_pass_the_written_check_offline() {
    // No provider at all: the check never asks a model.
    let t = TestApp::mem();
    let d = deck(&t, "English");
    let note = subtitle(&t, d.id, "give up", "vazgeçmek", "Never give up on your dreams.");
    let card = note.cards[0].id;
    let s = session_on(&t, d.id, card);
    assert_eq!(check(&t, &s, card, "vazgeçmek"), AnswerGrade::Correct);
    assert_eq!(check(&t, &s, card, "bırakmak"), AnswerGrade::Wrong);

    let added = t.ok::<AcceptedAnswersAdd>(EditAcceptedAnswers {
        note_id: note.id,
        field: None,
        answers: vec!["bırakmak".into(), " Bırakmak ".into(), "".into(), "pes etmek".into()],
    });
    assert_eq!(added.fields[0].field, "meaning");
    assert_eq!(added.fields[0].answers, ["bırakmak", "pes etmek"]);
    assert_eq!(t.ok::<AcceptedAnswersList>(AcceptedAnswersRef { note_id: note.id }), added);
    assert_eq!(check(&t, &s, card, "bırakmak"), AnswerGrade::Correct);
    assert_eq!(check(&t, &s, card, "pes etmek"), AnswerGrade::Correct);
    assert_eq!(check(&t, &s, card, "almak"), AnswerGrade::Wrong);

    // Removing is one undo step too.
    t.ok::<AcceptedAnswersRemove>(EditAcceptedAnswers {
        note_id: note.id,
        field: Some("meaning".into()),
        answers: vec!["BIRAKMAK".into()],
    });
    assert_eq!(check(&t, &s, card, "bırakmak"), AnswerGrade::Wrong);
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(check(&t, &s, card, "bırakmak"), AnswerGrade::Correct);

    // The module off: the lists stay but the check ignores them.
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: crate::modules::accepted_answers::ID.into(),
        enabled: false,
    });
    assert_eq!(check(&t, &s, card, "bırakmak"), AnswerGrade::Wrong);
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: crate::modules::accepted_answers::ID.into(),
        enabled: true,
    });
    assert_eq!(check(&t, &s, card, "bırakmak"), AnswerGrade::Correct);

    // Bad input: an unknown field.
    let e = t.err::<AcceptedAnswersAdd>(EditAcceptedAnswers {
        note_id: note.id,
        field: Some("nope".into()),
        answers: vec!["x".into()],
    });
    assert_eq!(e.code, ErrorCode::ValidationAcceptedAnswer);
}

#[test]
fn accepted_answers_persist_in_sqlite() {
    let db = crate::testing::TempSqlite::new();
    let store: Arc<dyn fc_storage::Store> = Arc::new(fc_storage::SqliteStore::open(&db.path()).unwrap());
    let t = super::tests::app_over(store);
    let d = deck(&t, "English");
    let note = pick_up(&t, d.id);
    t.ok::<AiMeaningApply>(ApplyAiMeaning {
        note_id: note.id,
        meaning: "almak (okuldan)".into(),
        phrase: "pick up the kids".into(),
        expression: true,
        also_correct: vec!["okuldan almak".into()],
        not_this: vec!["kaldırmak".into()],
        confidence: Some(0.8),
        model: Some("m".into()),
        edited: false,
        accepted: vec!["okuldan almak".into()],
        replace_field: false,
    });
    drop(t);
    // Reopened: the persisted result reads back through the API.
    let store: Arc<dyn fc_storage::Store> = Arc::new(fc_storage::SqliteStore::open(&db.path()).unwrap());
    let t = super::tests::app_over(store);
    let s = meaning_state(&t, &note);
    let m = s.meaning.unwrap();
    assert_eq!((m.meaning.as_str(), m.original.as_str(), m.not_this.len()), ("almak (okuldan)", "kaldırmak", 1));
    assert!((m.confidence.unwrap() - 0.8).abs() < 1e-6);
    assert_eq!(s.accepted[0].answers, ["almak (okuldan)", "okuldan almak"]);
    let _ = claude_text;
}
