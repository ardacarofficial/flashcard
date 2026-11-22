//! v1.3 provider setup: presets, key rules per address, the typed model list and the
//! connection test (every provider call against the local mock server).

use super::tests::{app, claude, claude_text, KEY};
use super::*;
use fc_ai::mock::{MockResponse, MockServer};
use fc_api::v1::ai::*;
use fc_api::v1::common::Empty;
use fc_api::v1::jobs::{JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::ErrorCode;
use serde_json::json;

fn create(t: &crate::testing::TestApp, kind: AiProviderKind, url: &str, model: Option<&str>) -> AiProviderDto {
    t.ok::<AiProvidersCreate>(CreateAiProvider {
        kind,
        name: None,
        base_url: Some(url.into()),
        default_model: model.map(str::to_owned),
        is_default: None,
    })
}

fn set_key(t: &crate::testing::TestApp, id: fc_core::AiProviderId, key: Option<&str>) -> AiProviderDto {
    t.ok::<AiProvidersSetKey>(SetAiProviderKey {
        provider_id: id,
        key: key.map(str::to_owned),
    })
}

fn provider(t: &crate::testing::TestApp, id: fc_core::AiProviderId) -> AiProviderDto {
    t.ok::<AiProvidersList>(Empty {})
        .providers
        .into_iter()
        .find(|p| p.id == id)
        .unwrap()
}

/// Runs `ai.providers.test` to the end; returns the typed job result.
fn run_test(t: &crate::testing::TestApp, id: fc_core::AiProviderId) -> AiConnectionTest {
    let job = t.ok::<AiProvidersTest>(AiProviderRef { id });
    t.app.run_pending_jobs().unwrap();
    let job = t.ok::<JobsGet>(JobRef { id: job.id });
    assert_eq!(job.status, JobStatus::Succeeded, "{job:?}");
    serde_json::from_value(job.result.unwrap()).unwrap()
}

#[test]
fn the_gallery_lists_every_preset_with_its_key_rule() {
    let t = app();
    let list = t.ok::<AiProvidersList>(Empty {});
    let ids: Vec<&str> = list.presets.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        ids,
        ["anthropic", "openai", "openrouter", "gemini", "groq", "mistral", "deepseek", "ollama", "lmstudio", "custom"]
    );
    let or = list.presets.iter().find(|p| p.id == "openrouter").unwrap();
    assert_eq!(or.kind, AiProviderKind::OpenaiCompatible);
    assert!(or.needs_key && or.key_url.is_some() && or.free && !or.local);
    let ollama = list.presets.iter().find(|p| p.id == "ollama").unwrap();
    assert!(!ollama.needs_key && ollama.local && ollama.setup_url.is_some());
}

#[test]
fn an_openrouter_provider_needs_a_key_and_says_so() {
    let t = app();
    // Created the way the gallery does it: the preset's kind and address.
    let p = create(&t, AiProviderKind::OpenaiCompatible, "https://openrouter.ai/api/v1", None);
    assert_eq!((p.name.as_str(), p.preset.as_str()), ("OpenRouter", "openrouter"));
    assert_eq!(p.default_model, "openrouter/auto");
    assert!(p.models.contains(&"openrouter/auto".to_owned()));
    // The v1.2 bug: `needs_key` was false here, so the UI hid the key field.
    assert!(p.needs_key && !p.has_key && !p.local);
    assert_eq!(p.status, AiProviderStatus::NeedsKey);
    let p = set_key(&t, p.id, Some(KEY));
    assert_eq!(p.status, AiProviderStatus::Ready);

    // A local OpenAI-compatible server does not need one (but accepts one).
    let lm = create(&t, AiProviderKind::OpenaiCompatible, "http://localhost:1234/v1", None);
    assert_eq!((lm.name.as_str(), lm.preset.as_str()), ("LM Studio", "lmstudio"));
    assert!(!lm.needs_key && lm.local);
    assert_eq!(lm.status, AiProviderStatus::NeedsModel);
    // An unknown hosted address is a custom endpoint that needs a key.
    let custom = create(&t, AiProviderKind::OpenaiCompatible, "https://llm.example.com/v1", Some("m"));
    assert_eq!((custom.preset.as_str(), custom.needs_key), ("custom", true));
    assert_eq!(custom.name, "OpenAI-compatible");
}

#[test]
fn running_an_openrouter_request_without_a_key_is_refused_before_sending() {
    let server = MockServer::start(|_| MockResponse::json(&json!({})));
    let t = app();
    // OpenRouter at a test address is a custom hosted endpoint: same rule.
    let url = server.url().replace("127.0.0.1", "openrouter.test");
    let p = create(&t, AiProviderKind::OpenaiCompatible, &url, Some("openrouter/auto"));
    assert!(p.needs_key);
    let d = super::tests::deck(&t, "D");
    let card = super::tests::classic(&t, &d, "q", "a").cards[0].id;
    let pv = super::tests::preview(&t, AiTask::Hint { card_id: card });
    assert!(pv.missing_key);
    let e = t.err::<AiRun>(RunAi {
        task: AiTask::Hint { card_id: card },
        provider_id: None,
        model: None,
        preview_hash: pv.preview_hash,
    });
    assert_eq!(e.code, ErrorCode::ProviderAiUnauthorized);
    assert!(server.requests().is_empty());
}

#[test]
fn the_model_list_is_typed_and_cached_on_the_provider() {
    let server = MockServer::start(|r| match r.path.as_str() {
        "/models" => MockResponse::json(&json!({"data": [
            {"id": "z-model:free"}, {"id": "a-model"}, {"id": "a-model"}
        ]})),
        _ => MockResponse::text(404, "text/plain", "no"),
    });
    let t = app();
    let p = create(&t, AiProviderKind::OpenaiCompatible, &server.url(), None);
    let p = set_key(&t, p.id, Some(KEY));
    let job = t.ok::<AiProvidersRefreshModels>(AiProviderRef { id: p.id });
    t.app.run_pending_jobs().unwrap();
    let job = t.ok::<JobsGet>(JobRef { id: job.id });
    let list: AiModelList = serde_json::from_value(job.result.unwrap()).unwrap();
    assert_eq!(list.provider_id, p.id);
    assert_eq!(list.models, ["a-model", "z-model:free"]);
    assert_eq!(provider(&t, p.id).models, list.models);
    // The key went along (an OpenRouter-style endpoint lists models per key).
    assert_eq!(
        server.requests()[0].header("authorization"),
        Some(format!("Bearer {KEY}").as_str())
    );
}

#[test]
fn a_connection_test_reports_ok_and_marks_the_provider_ready() {
    let server = MockServer::start(|_| claude_text("OK"));
    let t = app();
    let p = claude(&t, &server, Some(KEY));
    assert_eq!(p.status, AiProviderStatus::Ready);
    assert_eq!(p.last_test, None);
    let r = run_test(&t, p.id);
    assert_eq!(r.outcome, AiTestOutcome::Ok);
    assert_eq!(r.model.as_deref(), Some("claude-sonnet-5-5"));
    assert_eq!(r.http_status, Some(200));
    assert_eq!(r.message, None);
    let req = &server.requests()[0];
    assert_eq!(req.path, "/v1/messages");
    assert_eq!(req.json()["max_tokens"], fc_ai::diagnose::PROBE_MAX_TOKENS);
    let p = provider(&t, p.id);
    assert_eq!(p.last_test.as_ref().map(|x| x.outcome), Some(AiTestOutcome::Ok));
    assert_eq!(p.status, AiProviderStatus::Ready);
    // A test is not a feature request: no usage row, nothing in the undo history.
    assert_eq!(t.ok::<AiUsageGet>(Empty {}).month.requests, 0);
}

#[test]
fn connection_test_failures_are_explained_per_adapter() {
    // Claude: empty balance (400 with a credit message).
    let server = MockServer::start(|_| {
        MockResponse::status(400, &json!({"type": "error", "error": {
            "type": "invalid_request_error",
            "message": "Your credit balance is too low to access the Anthropic API."
        }}))
    });
    let t = app();
    let p = claude(&t, &server, Some(KEY));
    let r = run_test(&t, p.id);
    assert_eq!(r.outcome, AiTestOutcome::NoCredit);
    assert!(r.message.unwrap().contains("credit balance"));
    assert_eq!(provider(&t, p.id).status, AiProviderStatus::TestFailed);

    // OpenAI-compatible (OpenRouter-like): a rejected key — its body is never kept.
    let server = MockServer::start(|_| {
        MockResponse::status(401, &json!({"error": {"message": format!("bad key {KEY}"), "code": 401}}))
    });
    let p = create(&t, AiProviderKind::OpenaiCompatible, &server.url(), Some("openrouter/auto"));
    set_key(&t, p.id, Some(KEY));
    let r = run_test(&t, p.id);
    assert_eq!((r.outcome, r.http_status, r.message), (AiTestOutcome::WrongKey, Some(401), None));

    // Ollama: the model is not pulled.
    let server = MockServer::start(|_| {
        MockResponse::status(404, &json!({"error": "model \"llama9\" not found, try pulling it first"}))
    });
    let p = create(&t, AiProviderKind::Ollama, &server.url(), Some("llama9"));
    let r = run_test(&t, p.id);
    assert_eq!(r.outcome, AiTestOutcome::ModelNotFound);

    // Nothing listening: cannot reach.
    let url = MockServer::start(|_| MockResponse::json(&json!({}))).url();
    let p = create(&t, AiProviderKind::Ollama, &url, Some("qwen3"));
    let r = run_test(&t, p.id);
    assert_eq!((r.outcome, r.http_status), (AiTestOutcome::CannotReach, None));

    // A hosted endpoint without a key: nothing is sent.
    let p = create(&t, AiProviderKind::OpenaiCompatible, "https://openrouter.ai/api/v1", None);
    let r = run_test(&t, p.id);
    assert_eq!(r.outcome, AiTestOutcome::NoKey);
}

#[test]
fn a_test_result_is_forgotten_when_the_setup_changes() {
    let server = MockServer::start(|_| MockResponse::status(401, &json!({})));
    let t = app();
    let p = claude(&t, &server, Some(KEY));
    run_test(&t, p.id);
    assert_eq!(provider(&t, p.id).status, AiProviderStatus::TestFailed);
    // A new key: the old verdict no longer applies.
    set_key(&t, p.id, Some("sk-ant-other-key-123456"));
    let now = provider(&t, p.id);
    assert_eq!((now.status, now.last_test), (AiProviderStatus::Ready, None));
    run_test(&t, p.id);
    // Another model: forgotten as well.
    let p = provider(&t, p.id);
    let p = t.ok::<AiProvidersUpdate>(UpdateAiProvider {
        id: p.id,
        name: p.name.clone(),
        base_url: p.base_url.clone(),
        default_model: "claude-haiku-5-5".into(),
        models: p.models.clone(),
        is_default: p.is_default,
        keep_key: None,
    });
    assert_eq!((p.status, p.last_test), (AiProviderStatus::Ready, None));
}

#[test]
fn testing_needs_the_module() {
    let server = MockServer::start(|_| claude_text("OK"));
    let t = app();
    let p = claude(&t, &server, Some(KEY));
    t.ok::<ModulesSetEnabled>(fc_api::v1::modules::SetModuleEnabled {
        id: ID.into(),
        enabled: false,
    });
    let e = t.err::<AiProvidersTest>(AiProviderRef { id: p.id });
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
    assert!(server.requests().is_empty());
}
