//! v1.4 G1: what the first run against a real provider (OpenRouter, 2026-11-23) found in the
//! app layer, replayed offline against the local mock server with recorded answer shapes.

use super::tests::{app, deck, result, run};
use super::*;
use crate::testing::TestApp;
use fc_ai::mock::{MockResponse, MockServer};
use fc_api::v1::ai::*;
use fc_api::v1::common::Empty;
use fc_api::v1::methods::*;
use fc_api::v1::notes::CreateNote;
use fc_core::notetype::builtin::CLASSIC_ID;
use serde_json::{json, Value};

/// An OpenRouter chat completion as recorded (gemini-2.5-flash-lite), with `content` and the
/// reported `usage.cost` filled in.
fn completion(content: &str, cost: Option<f64>) -> Value {
    let mut usage = json!({"prompt_tokens": 120, "completion_tokens": 40, "total_tokens": 160, "is_byok": false});
    if let Some(c) = cost {
        usage["cost"] = json!(c);
    }
    json!({
        "id": "gen-1", "object": "chat.completion", "model": "google/gemini-2.5-flash-lite",
        "provider": "Google AI Studio",
        "choices": [{"index": 0, "finish_reason": "stop", "native_finish_reason": "STOP",
                     "message": {"role": "assistant", "content": content, "refusal": null, "reasoning": null}}],
        "usage": usage
    })
}

/// OpenRouter at the mock server (an OpenAI-compatible provider whose preset needs a key).
fn openrouter(t: &TestApp, server: &MockServer) -> AiProviderDto {
    let p = t.ok::<AiProvidersCreate>(CreateAiProvider {
        kind: AiProviderKind::OpenaiCompatible,
        name: Some("OpenRouter".into()),
        base_url: Some(server.url()),
        default_model: Some("google/gemini-2.5-flash-lite".into()),
        is_default: Some(true),
    });
    t.ok::<AiProvidersSetKey>(SetAiProviderKey {
        provider_id: p.id,
        key: Some(format!("sk-or-v1-{}", "0".repeat(64))),
    })
}

#[test]
fn the_cost_openrouter_reports_is_what_the_usage_panel_and_budget_count() {
    // No price row for the model: before v1.4 every OpenRouter request counted $0, so the
    // usage panel said $0 and a monthly budget never stopped anything.
    let server = MockServer::start(|_| MockResponse::json(&completion("A cliffhanger is a suspenseful ending.", Some(0.000_028))));
    let t = app();
    openrouter(&t, &server);
    let d = deck(&t, "Live");
    let note = t.ok::<NotesCreate>(CreateNote {
        notetype_id: CLASSIC_ID,
        deck_id: d.id,
        fields: [("front", "cliffhanger"), ("back", "merakta bırakan son")]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    });
    let r = result(&run(&t, AiTask::Explain { card_id: note.cards[0].id, question: None }));
    assert_eq!(r.cost_usd, Some(0.000_028));
    let u = t.ok::<AiUsageGet>(Empty {});
    assert_eq!(u.month.requests, 1);
    assert!((u.month.cost_usd - 0.000_028).abs() < 1e-12, "{:?}", u.month);
    assert!((u.budget.month_spent_usd - 0.000_028).abs() < 1e-12);

    // A provider that reports nothing still uses the price table (here: none → $0, unknown).
    let quiet = MockServer::start(|_| MockResponse::json(&completion("Plain.", None)));
    let t = app();
    openrouter(&t, &quiet);
    let d = deck(&t, "Live");
    let note = t.ok::<NotesCreate>(CreateNote {
        notetype_id: CLASSIC_ID,
        deck_id: d.id,
        fields: [("front", "a"), ("back", "b")]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    });
    let r = result(&run(&t, AiTask::Explain { card_id: note.cards[0].id, question: None }));
    assert_eq!(r.cost_usd, None);
    assert_eq!(t.ok::<AiUsageGet>(Empty {}).month.cost_usd, 0.0);
}

#[test]
fn a_generated_card_without_its_answer_is_repaired_not_accepted() {
    // Seen live (gemini-2.5-flash-lite via OpenRouter): with only `front` required the model
    // wrote the whole card into it and left `back` out; the app accepted three back-less notes.
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let n = calls.clone();
    let server = MockServer::start(move |_| {
        let first = n.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
        let content = if first {
            json!({"notes": [{"front": "What is a 'spoiler'? (Arka: A spoiler reveals an important plot point before you see it.)"}]})
        } else {
            json!({"notes": [{"front": "What is a 'spoiler'?", "back": "Something that reveals an important plot point too early."}]})
        };
        MockResponse::json(&completion(&content.to_string(), Some(0.000_01)))
    });
    let t = app();
    openrouter(&t, &server);
    let d = deck(&t, "Live");
    let task = AiTask::GenerateCards {
        deck_id: d.id,
        notetype_id: CLASSIC_ID,
        source: "A spoiler reveals an important plot point before you see it.".into(),
        count: 1,
        instructions: None,
        images: vec![],
        attributes: vec![],
    };
    // The schema asks for the answer field.
    let sent = super::tests::preview(&t, task.clone());
    assert!(sent.messages[0].text.contains("- back (Back, required)") || sent.messages[0].text.contains("- back (Arka, required)"), "{}", sent.messages[0].text);
    let r = result(&run(&t, task));
    assert_eq!(r.attempts, 2, "the back-less answer was sent back for repair");
    let AiOutput::GenerateCards { notes, .. } = r.output else { panic!() };
    assert_eq!(notes[0].fields["back"], "Something that reveals an important plot point too early.");
    let first = &server.requests()[0].json();
    assert_eq!(first["response_format"]["json_schema"]["schema"]["properties"]["notes"]["items"]["required"], json!(["front", "back"]));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
}

fn explain_task(t: &TestApp) -> AiTask {
    let d = deck(t, "Live");
    let note = t.ok::<NotesCreate>(CreateNote {
        notetype_id: CLASSIC_ID,
        deck_id: d.id,
        fields: [("front", "spoiler"), ("back", "sürprizi bozan bilgi")]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    });
    AiTask::Explain { card_id: note.cards[0].id, question: None }
}

#[test]
fn a_wrong_model_name_says_so_in_plain_words() {
    // Recorded: OpenRouter's answer to an unknown model (user id removed).
    let server = MockServer::start(|_| {
        MockResponse::status(400, &json!({"error": {"message": "google/no-such-model-g1 is not a valid model ID", "code": 400}}))
    });
    let t = app();
    openrouter(&t, &server);
    let task = explain_task(&t);
    let job = run(&t, task);
    let e = job.error.unwrap();
    assert_eq!(e.code, fc_api::ErrorCode::ProviderAiModelNotFound);
    assert_eq!(e.message_key, "errors.provider.ai_model_not_found");
    // The reason is the provider's words, without the adapter's internal id.
    assert_eq!(e.details["reason"], "HTTP 400: google/no-such-model-g1 is not a valid model ID");
    assert!(!e.retryable);
    // Nothing was recorded as spent.
    assert_eq!(t.ok::<AiUsageGet>(Empty {}).month.requests, 0);
}

#[test]
fn a_timeout_is_called_a_timeout() {
    let server = MockServer::start(|_| {
        std::thread::sleep(std::time::Duration::from_millis(600));
        MockResponse::json(&completion("late", Some(0.000_01)))
    });
    let t = app();
    t.app.ai_services().unwrap().set_timeout_ms(150);
    openrouter(&t, &server);
    t.ok::<SettingsSet>(fc_api::v1::settings::SetSetting {
        scope: fc_core::settings::SettingScope::App,
        key: RETRIES.into(),
        value: json!(0),
    });
    let task = explain_task(&t);
    let job = run(&t, task);
    let e = job.error.unwrap();
    assert_eq!(e.code, fc_api::ErrorCode::ProviderAiTimeout, "{e:?}");
    assert_eq!(e.message_key, "errors.provider.ai_timeout");
    assert!(e.retryable);
}

#[test]
fn cancelling_frees_the_ai_lane_at_once() {
    // Seen live: a cancelled 30-card request kept the AI lane busy until the provider answered
    // (11 s), so the next request waited. The job now ends when cancelled.
    let server = MockServer::start(|_| {
        std::thread::sleep(std::time::Duration::from_millis(3_000));
        MockResponse::json(&completion("{\"notes\": []}", None))
    });
    let t = std::sync::Arc::new(app());
    openrouter(&t, &server);
    let task = explain_task(&t);
    let p = super::tests::preview(&t, task.clone());
    let job = t.ok::<AiRun>(RunAi { task, provider_id: None, model: None, preview_hash: p.preview_hash });
    let canceller = {
        let t = std::sync::Arc::clone(&t);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            t.ok::<JobsCancel>(fc_api::v1::jobs::JobRef { id: job.id });
        })
    };
    let started = std::time::Instant::now();
    t.app.run_pending_jobs().unwrap();
    let took = started.elapsed();
    canceller.join().unwrap();
    assert!(took < std::time::Duration::from_millis(2_000), "the job waited for the provider: {took:?}");
    let done = t.ok::<JobsGet>(fc_api::v1::jobs::JobRef { id: job.id });
    assert_eq!(done.status, fc_api::v1::jobs::JobStatus::Cancelled);
}
