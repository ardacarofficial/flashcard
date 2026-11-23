//! AI service tests, every provider call against the local mock HTTP server (`fc_ai::mock`).
//!
//! The v0.8 acceptance criteria (docs/04):
//! * providers are tested against a mock server → `generate_cards_end_to_end_is_level_aware`
//!   (and the feature/provider tests below; wire formats in `fc-ai`'s own tests);
//! * the API key never shows up in the database, the oplog, errors, events or jobs →
//!   `api_key_never_reaches_db_oplog_errors_events_or_jobs`;
//! * content sent to the AI is previewed first → `nothing_is_sent_that_was_not_previewed`.

use super::*;
use crate::testing::{MemStore, TempSqlite, TestApp};
use crate::tests::internal_methods_of;
use fc_ai::mock::{MockRequest, MockResponse, MockServer};
use fc_ai::secrets::MemorySecrets;
use fc_api::v1::ai::*;
use fc_api::v1::attributes::{AttributeAssignment, DefineAttribute, NewAttributeOption};
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::jobs::{JobDto, JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::{CreateNote, NoteDto, NoteRef};
use fc_api::v1::oplog::ListOplog;
use fc_api::v1::settings::SetSetting;
use fc_api::{ApiError, ErrorCode};
use fc_core::attribute::{AttributeKind, AttributeValue};
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::settings::SettingScope;
use fc_core::{AttributeDefId, AttributeOptionId, NoteTypeId};
use fc_storage::Store;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub(super) const KEY: &str = "sk-ant-TEST-9f8e7d6c5b4a39281706";

pub(super) fn app_over(store: Arc<dyn Store>) -> TestApp {
    let t = TestApp::new(store);
    t.app.set_secret_store(Arc::new(MemorySecrets::new()));
    t.app.ai_services().unwrap().set_retry_base_ms(0);
    t
}

pub(super) fn app() -> TestApp {
    app_over(Arc::new(MemStore::new()))
}

/// Claude answering with a forced tool call carrying `input`.
pub(super) fn claude_tool(input: Value) -> MockResponse {
    MockResponse::json(&json!({
        "model": "claude-sonnet-5-5",
        "content": [{"type": "tool_use", "id": "t", "name": "emit_result", "input": input}],
        "stop_reason": "tool_use",
        "usage": {"input_tokens": 1000, "output_tokens": 200}
    }))
}

pub(super) fn claude_text(text: &str) -> MockResponse {
    MockResponse::json(&json!({
        "model": "claude-sonnet-5-5",
        "content": [{"type": "text", "text": text}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 100, "output_tokens": 20}
    }))
}

pub(super) fn claude(t: &TestApp, server: &MockServer, key: Option<&str>) -> AiProviderDto {
    let p = t.ok::<AiProvidersCreate>(CreateAiProvider {
        kind: AiProviderKind::Anthropic,
        name: None,
        base_url: Some(server.url()),
        default_model: None,
        is_default: None,
    });
    match key {
        Some(k) => t.ok::<AiProvidersSetKey>(SetAiProviderKey {
            provider_id: p.id,
            key: Some(k.into()),
        }),
        None => p,
    }
}

pub(super) fn deck(t: &TestApp, path: &str) -> DeckDto {
    t.ok::<DecksCreate>(CreateDeck {
        path: path.into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    })
}

/// "Dil::İngilizce" under a "Dil" deck with a Seviye (A1–C2) classification.
pub(super) struct Levels {
    pub(super) deck: DeckDto,
    pub(super) seviye: AttributeDefId,
    pub(super) b1: AttributeOptionId,
}

pub(super) fn levels(t: &TestApp) -> Levels {
    let parent = deck(t, "Dil");
    let deck = deck(t, "Dil::İngilizce");
    let def = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(parent.id),
        key: None,
        name: "Seviye".into(),
        kind: AttributeKind::OrderedEnum,
        target: None,
        required: None,
        options: ["A1", "A2", "B1", "B2", "C1", "C2"]
            .iter()
            .map(|l| NewAttributeOption {
                label: (*l).into(),
                color: None,
            })
            .collect(),
    });
    let b1 = def.options.iter().find(|o| o.label == "B1").unwrap().id;
    Levels {
        deck,
        seviye: def.id,
        b1,
    }
}

pub(super) fn generate(levels: &Levels, source: &str) -> AiTask {
    AiTask::GenerateCards {
        deck_id: levels.deck.id,
        notetype_id: CLASSIC_ID,
        source: source.into(),
        count: 2,
        instructions: None,
        images: vec![],
        attributes: vec![AttributeAssignment {
            attr_id: levels.seviye,
            value: Some(AttributeValue::Choice(levels.b1)),
        }],
    }
}

pub(super) fn preview(t: &TestApp, task: AiTask) -> AiPreview {
    t.ok::<AiPreviewTask>(PreviewAi {
        task,
        provider_id: None,
        model: None,
    })
}

/// Previews, confirms and runs `task`; returns the finished job.
pub(super) fn run(t: &TestApp, task: AiTask) -> JobDto {
    let p = preview(t, task.clone());
    let job = t.ok::<AiRun>(RunAi {
        task,
        provider_id: None,
        model: None,
        preview_hash: p.preview_hash,
    });
    t.app.run_pending_jobs().unwrap();
    t.ok::<JobsGet>(JobRef { id: job.id })
}

pub(super) fn result(job: &JobDto) -> AiJobResult {
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    serde_json::from_value(job.result.clone().unwrap()).unwrap()
}

pub(super) fn classic(t: &TestApp, deck: &DeckDto, front: &str, back: &str) -> NoteDto {
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: CLASSIC_ID,
        deck_id: deck.id,
        fields: BTreeMap::from([("front".into(), front.into()), ("back".into(), back.into())]),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    })
}

// ------------------------------------------------------------------ acceptance

#[test]
fn generate_cards_end_to_end_is_level_aware() {
    let server = MockServer::start(|_| {
        claude_tool(json!({"notes": [
            {"front": "photosynthesis", "back": "how plants make food from light"},
            {"front": "chlorophyll", "back": "the green pigment of plants"},
        ]}))
    });
    let t = app();
    let provider = claude(&t, &server, Some(KEY));
    assert!(provider.is_default && provider.has_key && provider.needs_key);
    assert_eq!(provider.default_model, "claude-sonnet-5-5");
    let lv = levels(&t);

    let p = preview(&t, generate(&lv, "Plants turn light into sugar."));
    assert_eq!((p.model.as_str(), p.missing_key), ("claude-sonnet-5-5", false));
    let text = &p.messages[0].text;
    assert!(text.contains("Plants turn light into sugar."), "{text}");
    assert!(text.contains("- Seviye: B1"), "level-aware prompt: {text}");
    assert!(text.contains("front (") && text.contains("back ("), "{text}");
    assert!(p.structured && p.estimated_input_tokens > 0);
    // Sonnet 5.5: $3 / $15 per Mtok → the upper bound includes max_tokens of output.
    let bound = p.estimated_cost_usd.unwrap();
    assert!(bound >= f64::from(p.max_tokens) * 15.0 / 1e6, "{bound}");

    let job = run(&t, generate(&lv, "Plants turn light into sugar."));
    let r = result(&job);
    assert_eq!((r.attempts, r.input_tokens, r.output_tokens), (1, 1000, 200));
    assert!((r.cost_usd.unwrap() - (1000.0 * 3.0 + 200.0 * 15.0) / 1e6).abs() < 1e-12);
    let AiOutput::GenerateCards {
        deck_id,
        notetype_id,
        notes,
        attributes,
    } = r.output
    else {
        panic!("{:?}", r.output)
    };
    assert_eq!((deck_id, notetype_id, notes.len()), (lv.deck.id, CLASSIC_ID, 2));
    assert_eq!(attributes[0].attr_id, lv.seviye);

    // The wire request: Claude Messages API with a forced tool for the schema.
    let req = &server.requests()[0];
    assert_eq!(req.path, "/v1/messages");
    assert_eq!(req.json()["model"], "claude-sonnet-5-5");
    assert_eq!(req.json()["tool_choice"]["name"], "emit_result");
    assert_eq!(
        req.json()["tools"][0]["input_schema"]["properties"]["notes"]["items"]["required"],
        // v1.4 G1: the field the card asks for (`back`) is required too.
        json!(["front", "back"])
    );

    // Usage was recorded with its cost.
    let usage = t.ok::<AiUsageGet>(Empty {});
    assert_eq!((usage.month.requests, usage.week.requests), (1, 1));
    assert_eq!(usage.by_feature[0].key, "generate_cards");
    assert_eq!(usage.by_model[0].key, "claude-sonnet-5-5");

    // Adding the drafts: one undo step; the notes carry the level.
    let before = crate::testing::suite::snapshot(&t);
    let added = t.ok::<AiNotesAdd>(AddAiNotes {
        deck_id,
        notetype_id,
        notes: notes.clone(),
        attributes,
        tags: vec![],
    });
    assert_eq!((added.note_ids.len(), added.skipped), (2, 0));
    let note = t.ok::<NotesGet>(NoteRef { id: added.note_ids[0] });
    assert_eq!(
        note.attributes,
        vec![AttributeAssignment {
            attr_id: lv.seviye,
            value: Some(AttributeValue::Choice(lv.b1))
        }]
    );
    let after = crate::testing::suite::snapshot(&t);
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(crate::testing::suite::snapshot(&t), before);
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(crate::testing::suite::snapshot(&t), after);
    // Adding the same drafts again skips them as duplicates.
    let again = t.ok::<AiNotesAdd>(AddAiNotes {
        deck_id,
        notetype_id,
        notes,
        attributes: vec![],
        tags: vec![],
    });
    assert_eq!((again.note_ids.len(), again.skipped), (0, 2));
}

#[test]
fn nothing_is_sent_that_was_not_previewed() {
    let server = MockServer::start(|_| claude_text("Because light is energy."));
    let t = app();
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "Bio");
    let note = classic(&t, &d, "Why do plants need light?", "To make sugar");
    let card = note.cards[0].id;
    let task = AiTask::Explain {
        card_id: card,
        question: None,
    };
    let p = preview(&t, task.clone());
    assert!(p.messages[0].text.contains("Why do plants need light?"));
    assert!(p.messages[0].text.contains("To make sugar"));
    assert_eq!(p.items, 1);

    // A made-up hash is refused before anything is queued or sent.
    let e = t.err::<AiRun>(RunAi {
        task: task.clone(),
        provider_id: None,
        model: None,
        preview_hash: "0000000000000000".into(),
    });
    assert_eq!(e.code, ErrorCode::ConflictAiPreviewStale);
    // The card changed after the preview: the old preview no longer matches.
    t.ok::<NotesUpdate>(fc_api::v1::notes::UpdateNote {
        id: note.id,
        deck_id: None,
        fields: BTreeMap::from([
            ("front".into(), "Why do plants need light?".into()),
            ("back".into(), "Photosynthesis".into()),
        ]),
        tags: vec![],
        attributes: vec![],
    });
    let e = t.err::<AiRun>(RunAi {
        task: task.clone(),
        provider_id: None,
        model: None,
        preview_hash: p.preview_hash.clone(),
    });
    assert_eq!(e.code, ErrorCode::ConflictAiPreviewStale);
    assert_eq!(t.app.run_pending_jobs().unwrap(), 0);
    assert!(server.requests().is_empty(), "nothing left the app");

    // Previewed again → sent exactly as shown.
    let p = preview(&t, task.clone());
    let job = t.ok::<AiRun>(RunAi {
        task,
        provider_id: None,
        model: None,
        preview_hash: p.preview_hash.clone(),
    });
    t.app.run_pending_jobs().unwrap();
    let r = result(&t.ok::<JobsGet>(JobRef { id: job.id }));
    assert_eq!(r.output, AiOutput::Explain { text: "Because light is energy.".into() });
    let sent = server.requests()[0].json();
    assert_eq!(sent["system"], json!(p.system));
    assert_eq!(sent["messages"][0]["content"][0]["text"], json!(p.messages[0].text));
    assert_eq!(sent["model"], json!(p.model));
    assert_eq!(sent["max_tokens"], json!(p.max_tokens));
    assert_eq!(server.requests().len(), 1);
}

/// Every place an API key could leak to: the database file (and its WAL), every table of a
/// dump, the oplog, events, jobs (payload, result, error), API errors and debug output.
#[test]
fn api_key_never_reaches_db_oplog_errors_events_or_jobs() {
    // The provider echoes the key back in its error (as some real ones do, partly masked).
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let server = MockServer::start(move |r: &MockRequest| {
        assert_eq!(r.header("x-api-key"), Some(KEY));
        match c.fetch_add(1, Ordering::SeqCst) {
            0 => MockResponse::status(401, &json!({"error": {"message": format!("invalid x-api-key {KEY}")}})),
            1 => MockResponse::status(400, &json!({"error": {"message": format!("bad request for key {}", &KEY[..14])}})),
            _ => claude_text("fine"),
        }
    });
    let sqlite = Arc::new(TempSqlite::new());
    let path = sqlite.path();
    let t = app_over(sqlite);
    let p = claude(&t, &server, Some(KEY));
    // A rejected key (with a space) is not echoed either.
    let bad = format!("{KEY} x");
    let e = t.err::<AiProvidersSetKey>(SetAiProviderKey {
        provider_id: p.id,
        key: Some(bad.clone()),
    });
    assert_eq!(e.code, ErrorCode::ValidationAiKey);
    let mut errors: Vec<ApiError> = vec![e];
    let d = deck(&t, "Keys");
    let note = classic(&t, &d, "q", "a");
    let task = AiTask::Hint {
        card_id: note.cards[0].id,
    };
    let mut jobs = Vec::new();
    for _ in 0..3 {
        jobs.push(run(&t, task.clone()));
    }
    assert_eq!(jobs[0].status, JobStatus::Failed);
    assert_eq!(jobs[0].error.as_ref().unwrap().code, ErrorCode::ProviderAiUnauthorized);
    assert_eq!(jobs[1].error.as_ref().unwrap().code, ErrorCode::ProviderAiInvalidRequest);
    assert_eq!(jobs[2].status, JobStatus::Succeeded);
    errors.extend(jobs.iter().filter_map(|j| j.error.clone()));
    t.ok::<AiProvidersUpdate>(UpdateAiProvider {
        id: p.id,
        name: "Claude (work)".into(),
        base_url: server.url(),
        default_model: "claude-haiku-5-5".into(),
        models: vec![],
        is_default: true,
        keep_key: None,
    });

    let leaks = |what: &str, text: &str| {
        assert!(!text.contains(KEY), "{what} contains the key");
        assert!(!text.contains(&KEY[..14]), "{what} contains part of the key");
    };
    for e in &errors {
        leaks("error", &format!("{e:?} {}", serde_json::to_string(e).unwrap()));
    }
    let set_key = SetAiProviderKey {
        provider_id: p.id,
        key: Some(KEY.into()),
    };
    leaks("debug of set_key", &format!("{set_key:?}"));
    for env in t.take_events() {
        leaks("event", &serde_json::to_string(&env).unwrap());
    }
    let oplog = t.ok::<OplogList>(ListOplog::default());
    leaks("oplog", &serde_json::to_string(&oplog).unwrap());
    assert!(
        !serde_json::to_string(&oplog).unwrap().contains("set_key"),
        "setting a key is not in the history"
    );
    let jobs = t.ok::<JobsList>(Empty {});
    leaks("jobs", &serde_json::to_string(&jobs).unwrap());
    let dump = t.store.read().unwrap().dump_collection().unwrap();
    leaks("dump", &format!("{dump:?}"));
    for suffix in ["", "-wal"] {
        let file = path.with_file_name(format!(
            "{}{suffix}",
            path.file_name().unwrap().to_string_lossy()
        ));
        match std::fs::read(&file) {
            Ok(bytes) => leaks("database file", &String::from_utf8_lossy(&bytes)),
            Err(e) => assert!(!suffix.is_empty(), "collection file unreadable: {e}"),
        }
    }
    // The key is only in the keychain (and only travelled in its header).
    let stored = t.app.ai_services().unwrap().secrets().get(&key_account(p.id)).unwrap();
    assert_eq!(stored.unwrap().expose(), KEY);
    for r in server.requests() {
        leaks("request body", &r.body);
        leaks("request path", &r.path);
    }
    // Removing the provider removes its key.
    t.ok::<AiProvidersDelete>(AiProviderRef { id: p.id });
    assert!(t.app.ai_services().unwrap().secrets().get(&key_account(p.id)).unwrap().is_none());
}

// ------------------------------------------------------------------ features

#[test]
fn card_and_note_features_parse_and_apply() {
    let server = MockServer::start(|r: &MockRequest| {
        let text = r.json()["messages"][0]["content"][0]["text"].as_str().unwrap_or_default().to_owned();
        if text.starts_with("Decide whether") {
            claude_tool(json!({"verdict": "correct", "feedback": "Same meaning."}))
        } else if text.starts_with("Rewrite this flashcard") {
            claude_tool(json!({"fields": {"front": "What is the capital of France?", "back": "Paris"}, "reason": "Clearer."}))
        } else if text.starts_with("The learner keeps forgetting") {
            claude_tool(json!({"fields": {"front": "Paris is the capital of …?"}, "mnemonic": "PARIS: Pretty And Romantic…"}))
        } else if text.starts_with("Write 3 plausible") {
            claude_tool(json!({"distractors": ["Lyon", "Paris", "Nice", "Lille", "Lyon"]}))
        } else if text.starts_with("Fill in the empty") {
            claude_tool(json!({"fields": {"back": "the capital of France"}}))
        } else {
            claude_text("Think of the Eiffel Tower.")
        }
    });
    let t = app();
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "Geo");
    let note = classic(&t, &d, "capital France", "Paris");
    let card = note.cards[0].id;

    let r = result(&run(&t, AiTask::CheckAnswer { card_id: card, given: "paris city".into() }));
    assert_eq!(
        r.output,
        AiOutput::CheckAnswer {
            verdict: AiVerdict::Correct,
            feedback: "Same meaning.".into()
        }
    );
    let r = result(&run(&t, AiTask::Hint { card_id: card }));
    assert_eq!(r.output, AiOutput::Hint { text: "Think of the Eiffel Tower.".into() });

    // Improve → only changed fields; applying is one undo step.
    let r = result(&run(&t, AiTask::Improve { note_id: note.id }));
    let AiOutput::Improve { changes, reason, .. } = r.output else { panic!() };
    assert_eq!(reason, "Clearer.");
    assert_eq!(changes.len(), 1);
    assert_eq!((changes[0].key.as_str(), changes[0].before.as_str()), ("front", "capital France"));
    let fields: BTreeMap<String, String> = changes.into_iter().map(|c| (c.key, c.after)).collect();
    let updated = t.ok::<AiNotesUpdate>(UpdateAiNote { note_id: note.id, fields });
    assert_eq!(updated.fields["front"], "What is the capital of France?");
    assert_eq!(updated.fields["back"], "Paris");
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(t.ok::<NotesGet>(NoteRef { id: note.id }).fields["front"], "capital France");
    let e = t.err::<AiNotesUpdate>(UpdateAiNote {
        note_id: note.id,
        fields: BTreeMap::from([("nope".into(), "x".into())]),
    });
    assert_eq!(e.code, ErrorCode::ValidationAiTask);

    let r = result(&run(&t, AiTask::RewriteLeech { note_id: note.id }));
    let AiOutput::RewriteLeech { changes, mnemonic, .. } = r.output else { panic!() };
    assert!(mnemonic.starts_with("PARIS"));
    assert_eq!(changes.len(), 1);

    // Distractors: the answer and repeats are dropped; capped at the count.
    let r = result(&run(&t, AiTask::Distractors { note_id: note.id, count: 3 }));
    assert_eq!(
        r.output,
        AiOutput::Distractors {
            note_id: note.id,
            items: vec!["Lyon".into(), "Nice".into(), "Lille".into()],
            field: None
        }
    );

    // Fill fields: only the empty ones.
    let r = result(&run(
        &t,
        AiTask::FillFields {
            deck_id: d.id,
            notetype_id: CLASSIC_ID,
            fields: BTreeMap::from([("front".into(), "Paris".into())]),
            attributes: vec![],
        },
    ));
    assert_eq!(
        r.output,
        AiOutput::FillFields { fields: BTreeMap::from([("back".into(), "the capital of France".into())]) }
    );
    let e = t.err::<AiPreviewTask>(PreviewAi {
        task: AiTask::FillFields {
            deck_id: d.id,
            notetype_id: CLASSIC_ID,
            fields: BTreeMap::new(),
            attributes: vec![],
        },
        provider_id: None,
        model: None,
    });
    assert_eq!((e.code, e.details["reason"].as_str()), (ErrorCode::ValidationAiTask, Some("no_fields")));
}

#[test]
fn distractors_target_the_multiple_choice_field() {
    let server = MockServer::start(|_| claude_tool(json!({"distractors": ["Lyon", "Nice"]})));
    let t = app();
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "MC");
    let choice: NoteTypeId = fc_core::content::builtin::find("multiple_choice").unwrap().id;
    let note = t.ok::<NotesCreate>(CreateNote {
        notetype_id: choice,
        deck_id: d.id,
        fields: BTreeMap::from([
            ("question".into(), "Capital of France?".into()),
            ("answer".into(), "Paris".into()),
        ]),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    });
    let r = result(&run(&t, AiTask::Distractors { note_id: note.id, count: 2 }));
    let AiOutput::Distractors { items, field, .. } = r.output else { panic!() };
    assert_eq!(field.as_deref(), Some("distractors"));
    let sent = server.requests()[0].json()["messages"][0]["content"][0]["text"].clone();
    assert!(sent.as_str().unwrap().contains("Correct answer: Paris"), "{sent}");
    let updated = t.ok::<AiNotesUpdate>(UpdateAiNote {
        note_id: note.id,
        fields: BTreeMap::from([("distractors".into(), items.join("\n"))]),
    });
    assert_eq!(updated.fields["distractors"], "Lyon\nNice");
}

#[test]
fn retries_outages_and_repairs_invalid_output_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let server = MockServer::start(move |_| match c.fetch_add(1, Ordering::SeqCst) {
        0 => MockResponse::status(529, &json!({"error": {"message": "overloaded"}})),
        1 => claude_tool(json!({"wrong": true})),
        _ => claude_tool(json!({"notes": [{"front": "a", "back": "b"}]})),
    });
    let t = app();
    claude(&t, &server, Some(KEY));
    let lv = levels(&t);
    let r = result(&run(&t, generate(&lv, "x")));
    assert_eq!(r.attempts, 3, "outage retried, then one repair");
    assert_eq!((r.input_tokens, r.output_tokens), (2000, 400));
    let reqs = server.requests();
    let repair = reqs[2].json();
    assert_eq!(repair["messages"].as_array().unwrap().len(), 3);
    assert_eq!(repair["messages"][1]["role"], "assistant");
    assert!(repair["messages"][2]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("`notes` must be an array"));
    // Only answered requests are billed.
    assert_eq!(t.ok::<AiUsageGet>(Empty {}).month.requests, 2);

    // Still invalid after the repair → a clear error.
    let server = MockServer::start(|_| claude_tool(json!({"notes": "nope"})));
    let t = app();
    claude(&t, &server, Some(KEY));
    let lv = levels(&t);
    let job = run(&t, generate(&lv, "x"));
    assert_eq!(job.error.unwrap().code, ErrorCode::ProviderAiInvalidOutput);
    assert_eq!(server.requests().len(), 2);

    // Rate limits beyond the retry budget fail as rate limited (retryable).
    let server = MockServer::start(|_| MockResponse::status(429, &json!({})));
    let t = app();
    claude(&t, &server, Some(KEY));
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: RETRIES.into(),
        value: json!(1),
    });
    let lv = levels(&t);
    let e = run(&t, generate(&lv, "x")).error.unwrap();
    assert_eq!((e.code, e.retryable), (ErrorCode::ProviderAiRateLimited, true));
    assert_eq!(server.requests().len(), 2);
}

// ------------------------------------------------------------------ providers & settings

#[test]
fn provider_commands_are_exactly_undoable_and_keep_one_default() {
    let t = app();
    let a = t.ok::<AiProvidersCreate>(CreateAiProvider {
        kind: AiProviderKind::Anthropic,
        name: None,
        base_url: None,
        default_model: None,
        is_default: None,
    });
    assert_eq!((a.name.as_str(), a.base_url.as_str()), ("Claude", "https://api.anthropic.com"));
    assert!(a.models.contains(&"claude-opus-5-5".to_owned()) && a.is_default);
    let s0 = t.ok::<AiProvidersList>(Empty {});
    let b = t.ok::<AiProvidersCreate>(CreateAiProvider {
        kind: AiProviderKind::Ollama,
        name: Some("Local".into()),
        base_url: None,
        default_model: Some("llama3.2".into()),
        is_default: Some(true),
    });
    assert!(b.local && !b.needs_key);
    let s1 = t.ok::<AiProvidersList>(Empty {});
    assert_eq!(s1.providers.iter().filter(|p| p.is_default).count(), 1);
    assert!(s1.providers.iter().find(|p| p.id == b.id).unwrap().is_default);
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(t.ok::<AiProvidersList>(Empty {}).providers, s0.providers);
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(t.ok::<AiProvidersList>(Empty {}).providers, s1.providers);

    // Deleting the default hands the flag on; undo restores both rows.
    t.ok::<AiProvidersDelete>(AiProviderRef { id: b.id });
    let s2 = t.ok::<AiProvidersList>(Empty {});
    assert_eq!(s2.providers.len(), 1);
    assert!(s2.providers[0].is_default);
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(t.ok::<AiProvidersList>(Empty {}).providers, s1.providers);

    let e = t.err::<AiProvidersCreate>(CreateAiProvider {
        kind: AiProviderKind::OpenaiCompatible,
        name: None,
        base_url: Some("ftp://x".into()),
        default_model: None,
        is_default: None,
    });
    assert_eq!((e.code, e.details["field"].as_str()), (ErrorCode::ValidationAiProvider, Some("base_url")));
    // Without an installed keychain, keys cannot be stored.
    let bare = TestApp::mem();
    let p = bare.ok::<AiProvidersCreate>(CreateAiProvider {
        kind: AiProviderKind::Anthropic,
        name: None,
        base_url: None,
        default_model: None,
        is_default: None,
    });
    assert!(!bare.ok::<AiProvidersList>(Empty {}).keychain);
    let e = bare.err::<AiProvidersSetKey>(SetAiProviderKey { provider_id: p.id, key: Some(KEY.into()) });
    assert_eq!(e.code, ErrorCode::ProviderAiKeychain);
}

#[test]
fn routes_pick_provider_and_model_per_feature() {
    let server = MockServer::start(|_| claude_tool(json!({"verdict": "wrong", "feedback": "No."})));
    let local = MockServer::start(|_| {
        MockResponse::json(&json!({"model": "llama3.2", "message": {"content": "{\"verdict\":\"almost\",\"feedback\":\"Close.\"}"},
            "prompt_eval_count": 50, "eval_count": 5}))
    });
    let t = app();
    let main = claude(&t, &server, Some(KEY));
    let ollama = t.ok::<AiProvidersCreate>(CreateAiProvider {
        kind: AiProviderKind::Ollama,
        name: None,
        base_url: Some(local.url()),
        default_model: Some("llama3.2".into()),
        is_default: None,
    });
    let d = deck(&t, "R");
    let card = classic(&t, &d, "q", "a").cards[0].id;
    let task = AiTask::CheckAnswer { card_id: card, given: "b".into() };
    assert_eq!(preview(&t, task.clone()).provider.id, main.id);

    t.ok::<AiRoutesSet>(AiRouteDto {
        feature: AiFeature::CheckAnswer,
        provider_id: Some(ollama.id),
        model: Some("qwen3".into()),
    });
    let p = preview(&t, task.clone());
    assert_eq!((p.provider.id, p.model.as_str()), (ollama.id, "qwen3"));
    // A local model without a price: free, and no key needed.
    assert_eq!((p.estimated_cost_usd, p.missing_key), (None, false));
    let r = result(&run(&t, task.clone()));
    assert_eq!(r.output, AiOutput::CheckAnswer { verdict: AiVerdict::Almost, feedback: "Close.".into() });
    assert_eq!(local.requests()[0].json()["model"], "qwen3");
    assert!(server.requests().is_empty());
    // An explicit model wins over the route.
    let p = t.ok::<AiPreviewTask>(PreviewAi { task: task.clone(), provider_id: Some(main.id), model: None });
    assert_eq!((p.provider.id, p.model.as_str()), (main.id, "claude-sonnet-5-5"));

    let routes = t.ok::<AiRoutesList>(Empty {}).routes;
    assert_eq!(routes.len(), AiFeature::ALL.len());
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(preview(&t, task).provider.id, main.id);

    // No provider at all.
    let bare = app();
    let d = deck(&bare, "X");
    let card = classic(&bare, &d, "q", "a").cards[0].id;
    let e = bare.err::<AiPreviewTask>(PreviewAi { task: AiTask::Hint { card_id: card }, provider_id: None, model: None });
    assert_eq!(e.code, ErrorCode::ConflictAiNoProvider);
}

/// A keychain that fails every read (a headless session without a keyring).
struct BrokenKeychain;

impl fc_ai::secrets::SecretStore for BrokenKeychain {
    fn get(&self, _: &str) -> Result<Option<fc_ai::secrets::Secret>, fc_ai::secrets::SecretError> {
        Err(fc_ai::secrets::SecretError::Unavailable("no session keyring".into()))
    }
    fn set(&self, _: &str, _: &fc_ai::secrets::Secret) -> Result<(), fc_ai::secrets::SecretError> {
        Err(fc_ai::secrets::SecretError::Unavailable("no session keyring".into()))
    }
    fn delete(&self, _: &str) -> Result<(), fc_ai::secrets::SecretError> {
        Ok(())
    }
}

#[test]
fn a_broken_keychain_stops_keyed_providers_but_not_keyless_ones() {
    let local = MockServer::start(|_| {
        MockResponse::json(&json!({"model": "llama3.2", "message": {"content": "{\"verdict\":\"almost\",\"feedback\":\"Close.\"}"},
            "prompt_eval_count": 50, "eval_count": 5}))
    });
    let cloud = MockServer::start(|_| claude_tool(json!({"verdict": "wrong", "feedback": "No."})));
    let t = app();
    t.app.set_secret_store(Arc::new(BrokenKeychain));
    let ollama = t.ok::<AiProvidersCreate>(CreateAiProvider {
        kind: AiProviderKind::Ollama,
        name: None,
        base_url: Some(local.url()),
        default_model: Some("llama3.2".into()),
        is_default: None,
    });
    let claude = claude(&t, &cloud, None);
    let d = deck(&t, "R");
    let card = classic(&t, &d, "q", "a").cards[0].id;
    let task = AiTask::CheckAnswer { card_id: card, given: "b".into() };
    t.ok::<AiRoutesSet>(AiRouteDto {
        feature: AiFeature::CheckAnswer,
        provider_id: Some(ollama.id),
        model: None,
    });
    let r = result(&run(&t, task.clone()));
    assert_eq!(r.output, AiOutput::CheckAnswer { verdict: AiVerdict::Almost, feedback: "Close.".into() });
    // A provider that needs a key still reports the keychain.
    t.ok::<AiRoutesSet>(AiRouteDto {
        feature: AiFeature::CheckAnswer,
        provider_id: Some(claude.id),
        model: None,
    });
    let p = preview(&t, task.clone());
    let e = t.err::<AiRun>(RunAi { task, provider_id: None, model: None, preview_hash: p.preview_hash });
    assert!(
        matches!(e.code, ErrorCode::ProviderAiKeychain | ErrorCode::ProviderAiUnauthorized),
        "{e:?}"
    );
    assert!(cloud.requests().is_empty());
}

#[test]
fn prices_budget_and_usage_summary() {
    let t = app();
    let seeded = t.ok::<AiPricesList>(Empty {}).prices;
    assert_eq!(seeded.len(), 3);
    t.ok::<AiPricesSet>(AiPriceDto { model: "claude-sonnet-5-5".into(), input_per_mtok: 10.0, output_per_mtok: 20.0 });
    t.ok::<AiPricesSet>(AiPriceDto { model: "local".into(), input_per_mtok: 0.0, output_per_mtok: 0.0 });
    t.ok::<AiPricesDelete>(AiPriceRef { model: "claude-haiku-5-5".into() });
    t.ok::<OplogUndo>(Empty {});
    t.ok::<OplogUndo>(Empty {});
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(t.ok::<AiPricesList>(Empty {}).prices, seeded);
    let e = t.err::<AiPricesSet>(AiPriceDto { model: "x".into(), input_per_mtok: -1.0, output_per_mtok: 0.0 });
    assert_eq!(e.code, ErrorCode::ValidationAiPrice);

    let server = MockServer::start(|_| claude_text("ok"));
    let p = claude(&t, &server, Some(KEY));
    t.ok::<SettingsSet>(SetSetting { scope: SettingScope::App, key: BUDGET.into(), value: json!(0.0005) });
    let d = deck(&t, "B");
    let card = classic(&t, &d, "q", "a").cards[0].id;
    // 100 in + 20 out at $3/$15 = $0.0006 > budget.
    result(&run(&t, AiTask::Hint { card_id: card }));
    let s = t.ok::<AiUsageGet>(Empty {});
    assert!(s.budget.warning && s.budget.exceeded, "{:?}", s.budget);
    assert_eq!(s.budget.limit_usd, Some(0.0005));
    assert!((s.month.cost_usd - 0.0006).abs() < 1e-12);
    assert_eq!((s.by_feature[0].key.as_str(), s.week.input_tokens), ("hint", 100));
    assert!(preview(&t, AiTask::Hint { card_id: card }).budget.exceeded);
    // Usage rows are a ledger: not undo steps.
    let oplog = t.ok::<OplogList>(ListOplog::default());
    assert!(!serde_json::to_string(&oplog).unwrap().contains("ai.usage.record"));
    let _ = p;
}

#[test]
fn model_list_refresh_job() {
    let server = MockServer::start(|_| {
        MockResponse::json(&json!({"data": [{"id": "claude-opus-5-5"}, {"id": "claude-haiku-5-5"}], "has_more": false}))
    });
    let t = app();
    let p = claude(&t, &server, Some(KEY));
    let job = t.ok::<AiProvidersRefreshModels>(AiProviderRef { id: p.id });
    t.app.run_pending_jobs().unwrap();
    assert_eq!(t.ok::<JobsGet>(JobRef { id: job.id }).status, JobStatus::Succeeded);
    let models = &t.ok::<AiProvidersList>(Empty {}).providers[0].models;
    assert_eq!(models, &["claude-haiku-5-5", "claude-opus-5-5"]);
    assert_eq!(server.requests()[0].header("x-api-key"), Some(KEY));
}

#[test]
fn module_switch_internal_methods_and_missing_key() {
    let server = MockServer::start(|_| claude_text("x"));
    let t = app();
    let p = claude(&t, &server, None);
    let d = deck(&t, "M");
    let card = classic(&t, &d, "q", "a").cards[0].id;
    let pv = preview(&t, AiTask::Hint { card_id: card });
    assert!(pv.missing_key);
    let e = t.err::<AiRun>(RunAi {
        task: AiTask::Hint { card_id: card },
        provider_id: None,
        model: None,
        preview_hash: pv.preview_hash,
    });
    assert_eq!(e.code, ErrorCode::ProviderAiUnauthorized);
    assert!(server.requests().is_empty());
    let _ = p;

    assert_eq!(
        internal_methods_of(&t, &["ai"]),
        // v1.4: the meaning-in-context writes (an undo inverse, the suggestion job's write).
        [
            "ai.meaning.restore",
            "ai.meaning.store_suggestion",
            "ai.providers.restore",
            "ai.providers.set_models",
            "ai.usage.record"
        ]
    );
    t.ok::<ModulesSetEnabled>(SetModuleEnabled { id: ID.into(), enabled: false });
    let e = t.err::<AiProvidersList>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
}

/// v1.3 closing pass: the "answer" an AI hint / check / explanation is given for a
/// vocabulary card is the field the card asks for (the meaning), not its whole back with the
/// example, translation and source.
#[test]
fn a_vocabulary_cards_answer_is_its_meaning() {
    let server = MockServer::start(|_| claude_text("ok"));
    let t = app();
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "Subtitle");
    let note = t.ok::<NotesCreate>(fc_api::v1::notes::CreateNote {
        notetype_id: fc_core::content::builtin::find(fc_core::content::builtin::SUBTITLE).unwrap().id,
        deck_id: d.id,
        fields: [
            ("word", "go"),
            ("surface", "went"),
            ("meaning", "gitmek"),
            ("sentence", "I <b>went</b> home."),
            ("translation", "Eve gittim."),
            ("source", "Film · 1:23"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect(),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    });
    let p = preview(&t, AiTask::Hint { card_id: note.cards[0].id });
    let text = &p.messages[0].text;
    assert!(text.contains("gitmek"), "{text}");
    assert!(!text.contains("Film") && !text.contains("Eve gittim"), "{text}");
}
