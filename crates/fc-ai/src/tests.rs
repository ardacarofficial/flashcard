//! Every provider against the local mock server (docs/04 v0.8: "providers are tested against
//! a mock server; real API tests are optional"). Real-API smoke tests run only when their
//! environment variable is set (`FC_AI_LIVE_ANTHROPIC_API_KEY`, `FC_AI_LIVE_OPENAI_API_KEY`,
//! `FC_AI_LIVE_OLLAMA_URL`; models via `FC_AI_LIVE_*_MODEL`).

use super::*;
use crate::mock::{MockResponse, MockServer};
use crate::secrets::{MemorySecrets, NoSecrets, SecretStore};
use fc_providers::ai::{CompletionRequest, ContentPart, Message, Role};
use fc_providers::ProviderErrorKind;
use serde_json::json;
use std::time::Duration;

const KEY: &str = "sk-test-SECRET-0123456789abcdef";

fn config(kind: ProviderKind, url: &str, key: Option<&str>) -> ProviderConfig {
    ProviderConfig {
        kind,
        base_url: url.to_owned(),
        api_key: key.map(Secret::new),
        timeout: Duration::from_secs(10),
        media: None,
    }
}

fn request(schema: Option<serde_json::Value>) -> CompletionRequest {
    CompletionRequest {
        model: Some("claude-sonnet-5-5".into()),
        system: Some("You write flashcards.".into()),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentPart::Text {
                text: "Make one card about photosynthesis.".into(),
            }],
        }],
        max_tokens: 500,
        temperature: None,
        json_schema: schema,
        feature: "ai.generate_cards".into(),
    }
}

fn schema() -> serde_json::Value {
    json!({"type": "object", "properties": {"notes": {"type": "array"}}, "required": ["notes"]})
}

struct OneImage;

impl MediaSource for OneImage {
    fn media_bytes(&self, hash: &str) -> Option<Vec<u8>> {
        (hash == "abc").then(|| vec![1, 2, 3])
    }
}

// ------------------------------------------------------------------ Anthropic

#[test]
fn anthropic_text_completion_wire_format() {
    let server = MockServer::start(|_| {
        MockResponse::json(&json!({
            "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-sonnet-5-5",
            "content": [{"type": "text", "text": "Plants "}, {"type": "text", "text": "make sugar."}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 42, "output_tokens": 7}
        }))
    });
    let p = build(config(ProviderKind::Anthropic, &server.url(), Some(KEY)));
    let r = block_on(p.complete(request(None))).unwrap();
    assert_eq!(r.text, "Plants make sugar.");
    assert_eq!((r.usage.input_tokens, r.usage.output_tokens), (42, 7));
    assert!(!r.truncated);

    let req = &server.requests()[0];
    assert_eq!(
        (req.method.as_str(), req.path.as_str()),
        ("POST", "/v1/messages")
    );
    assert_eq!(req.header("x-api-key"), Some(KEY));
    assert_eq!(req.header("anthropic-version"), Some("2023-06-01"));
    let body = req.json();
    assert_eq!(body["model"], "claude-sonnet-5-5");
    assert_eq!(body["max_tokens"], 500);
    assert_eq!(body["system"], "You write flashcards.");
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"][0]["type"], "text");
    assert!(body.get("tools").is_none() && body.get("temperature").is_none());
    // The key travels only in its header.
    assert!(!req.body.contains(KEY) && !req.path.contains(KEY));
}

#[test]
fn anthropic_structured_output_is_a_forced_tool_call() {
    let server = MockServer::start(|_| {
        MockResponse::json(&json!({
            "model": "claude-sonnet-5-5",
            "content": [{"type": "tool_use", "id": "t1", "name": "emit_result",
                         "input": {"notes": [{"front": "a", "back": "b"}]}}],
            "stop_reason": "max_tokens",
            "usage": {"input_tokens": 1, "output_tokens": 2}
        }))
    });
    let p = build(config(ProviderKind::Anthropic, &server.url(), Some(KEY)));
    let r = block_on(p.complete(request(Some(schema())))).unwrap();
    let out: serde_json::Value = serde_json::from_str(&r.text).unwrap();
    assert_eq!(out["notes"][0]["front"], "a");
    assert!(r.truncated);
    let body = server.requests()[0].json();
    assert_eq!(body["tools"][0]["name"], "emit_result");
    assert_eq!(body["tools"][0]["input_schema"], schema());
    assert_eq!(
        body["tool_choice"],
        json!({"type": "tool", "name": "emit_result"})
    );
}

#[test]
fn anthropic_sends_images_as_base64_blocks() {
    let server = MockServer::start(|_| {
        MockResponse::json(&json!({"content": [{"type": "text", "text": "ok"}], "usage": {}}))
    });
    let mut cfg = config(ProviderKind::Anthropic, &server.url(), Some(KEY));
    cfg.media = Some(Arc::new(OneImage));
    let mut req = request(None);
    req.messages[0].content.push(ContentPart::Image {
        media_hash: "abc".into(),
        mime: "image/png".into(),
    });
    block_on(build(cfg).complete(req.clone())).unwrap();
    let block = &server.requests()[0].json()["messages"][0]["content"][1];
    assert_eq!(block["type"], "image");
    assert_eq!(block["source"]["media_type"], "image/png");
    assert_eq!(block["source"]["data"], "AQID");

    // Without a media source images are refused before anything is sent.
    let p = build(config(ProviderKind::Anthropic, &server.url(), Some(KEY)));
    let e = block_on(p.complete(req)).unwrap_err();
    assert_eq!(e.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn anthropic_lists_models_across_pages() {
    let server = MockServer::start(|r| {
        if r.path.contains("after_id") {
            MockResponse::json(&json!({"data": [{"id": "claude-haiku-5-5"}], "has_more": false}))
        } else {
            MockResponse::json(&json!({
                "data": [{"id": "claude-opus-5-5"}, {"id": "claude-sonnet-5-5"}],
                "has_more": true
            }))
        }
    });
    let p = build(config(ProviderKind::Anthropic, &server.url(), Some(KEY)));
    let models = block_on(p.list_models()).unwrap();
    assert_eq!(
        models,
        ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"]
    );
    let reqs = server.requests();
    assert_eq!(
        reqs[1].path,
        "/v1/models?limit=100&after_id=claude-sonnet-5-5"
    );
    assert!(reqs.iter().all(|r| r.header("x-api-key") == Some(KEY)));
}

// ------------------------------------------------------------------ OpenAI (+ compatible)

#[test]
fn openai_structured_completion_wire_format() {
    let server = MockServer::start(|_| {
        MockResponse::json(&json!({
            "model": "gpt-x",
            "choices": [{"message": {"role": "assistant", "content": "{\"notes\":[]}"},
                         "finish_reason": "length"}],
            "usage": {"prompt_tokens": 11, "completion_tokens": 3}
        }))
    });
    let mut req = request(Some(schema()));
    req.model = Some("gpt-x".into());
    req.temperature = Some(0.2);
    let p = build(config(ProviderKind::Openai, &server.url(), Some(KEY)));
    let r = block_on(p.complete(req)).unwrap();
    assert_eq!(r.text, "{\"notes\":[]}");
    assert_eq!((r.usage.input_tokens, r.usage.output_tokens), (11, 3));
    assert!(r.truncated);
    let req = &server.requests()[0];
    assert_eq!(req.path, "/chat/completions");
    assert_eq!(
        req.header("authorization"),
        Some(format!("Bearer {KEY}").as_str())
    );
    let body = req.json();
    assert_eq!(
        body["messages"][0],
        json!({"role": "system", "content": "You write flashcards."})
    );
    assert_eq!(
        body["messages"][1]["content"][0]["text"],
        "Make one card about photosynthesis."
    );
    assert_eq!(body["max_completion_tokens"], 500);
    assert_eq!(body["response_format"]["json_schema"]["schema"], schema());
    assert!((body["temperature"].as_f64().unwrap() - 0.2).abs() < 1e-6);
}

#[test]
fn openai_compatible_needs_no_key_and_uses_max_tokens() {
    let server = MockServer::start(|r| {
        if r.path == "/models" {
            MockResponse::json(&json!({"data": [{"id": "local-llama"}]}))
        } else {
            MockResponse::json(&json!({"choices": [{"message": {"content": "hi"}}]}))
        }
    });
    let p = build(config(
        ProviderKind::OpenaiCompatible,
        &format!("{}/", server.url()),
        None,
    ));
    assert_eq!(block_on(p.complete(request(None))).unwrap().text, "hi");
    assert_eq!(block_on(p.list_models()).unwrap(), ["local-llama"]);
    let reqs = server.requests();
    assert!(reqs.iter().all(|r| r.header("authorization").is_none()));
    assert_eq!(reqs[0].json()["max_tokens"], 500);

    // OpenAI proper refuses to send without a key.
    let p = build(config(ProviderKind::Openai, &server.url(), None));
    let e = block_on(p.complete(request(None))).unwrap_err();
    assert_eq!(e.kind, ProviderErrorKind::Unauthorized);
    assert_eq!(server.requests().len(), 2);
}

// ------------------------------------------------------------------ Ollama

#[test]
fn ollama_chat_and_tags() {
    let server = MockServer::start(|r| {
        if r.path == "/api/tags" {
            MockResponse::json(&json!({"models": [{"name": "llama3.2:3b"}, {"name": "qwen3"}]}))
        } else {
            MockResponse::json(&json!({
                "model": "llama3.2:3b",
                "message": {"role": "assistant", "content": "{\"notes\":[]}"},
                "done_reason": "stop", "prompt_eval_count": 30, "eval_count": 5
            }))
        }
    });
    let mut cfg = config(ProviderKind::Ollama, &server.url(), None);
    cfg.media = Some(Arc::new(OneImage));
    let p = build(cfg);
    assert!(p.info().local);
    let mut req = request(Some(schema()));
    req.model = Some("llama3.2:3b".into());
    req.messages[0].content.push(ContentPart::Image {
        media_hash: "abc".into(),
        mime: "image/png".into(),
    });
    let r = block_on(p.complete(req)).unwrap();
    assert_eq!((r.usage.input_tokens, r.usage.output_tokens), (30, 5));
    assert_eq!(block_on(p.list_models()).unwrap(), ["llama3.2:3b", "qwen3"]);
    let body = server.requests()[0].json();
    assert_eq!(server.requests()[0].path, "/api/chat");
    assert_eq!(body["stream"], false);
    assert_eq!(body["format"], schema());
    assert_eq!(body["options"]["num_predict"], 500);
    assert_eq!(body["messages"][1]["images"], json!(["AQID"]));
}

// ------------------------------------------------------------------ errors & secrets

#[test]
fn http_errors_map_to_kinds_and_never_carry_the_key() {
    let echo = |status: u16| {
        move |_: &mock::MockRequest| {
            MockResponse::status(
                status,
                &json!({"type": "error", "error": {"type": "x",
                    "message": format!("bad key {KEY} (prefix {})", &KEY[..12])}}),
            )
        }
    };
    for (status, kind, retryable) in [
        (401, ProviderErrorKind::Unauthorized, false),
        (403, ProviderErrorKind::Unauthorized, false),
        (429, ProviderErrorKind::RateLimited, true),
        (529, ProviderErrorKind::Unavailable, true),
        (500, ProviderErrorKind::Unavailable, true),
        (400, ProviderErrorKind::InvalidRequest, false),
        (404, ProviderErrorKind::Unavailable, false),
    ] {
        let server = MockServer::start(echo(status));
        for kind_of in [ProviderKind::Anthropic, ProviderKind::Openai] {
            let p = build(config(kind_of, &server.url(), Some(KEY)));
            let e = block_on(p.complete(request(None))).unwrap_err();
            assert_eq!((e.kind, e.retryable), (kind, retryable), "{status}");
            let shown = format!("{e} {e:?}");
            assert!(
                !shown.contains(KEY) && !shown.contains(&KEY[..12]),
                "{status}: {shown}"
            );
            assert!(shown.contains(&status.to_string()));
        }
    }
}

#[test]
fn unreachable_server_is_retryable_unavailable() {
    let url = {
        let s = MockServer::start(|_| MockResponse::json(&json!({})));
        s.url()
    };
    let p = build(config(ProviderKind::Ollama, &url, None));
    let e = block_on(p.complete(request(None))).unwrap_err();
    assert_eq!(e.kind, ProviderErrorKind::Unavailable);
    assert!(e.retryable);
}

#[test]
fn missing_model_is_rejected_before_sending() {
    let server = MockServer::start(|_| MockResponse::json(&json!({})));
    let mut req = request(None);
    req.model = None;
    let e =
        block_on(build(config(ProviderKind::Anthropic, &server.url(), Some(KEY))).complete(req))
            .unwrap_err();
    assert_eq!(e.kind, ProviderErrorKind::InvalidRequest);
    assert!(server.requests().is_empty());
}

#[test]
fn redact_removes_whole_and_partial_keys() {
    let k = Secret::new(KEY);
    assert_eq!(redact(&format!("x {KEY} y"), Some(&k)), "x *** y");
    let partial = format!("key {}…{}", &KEY[..10], &KEY[KEY.len() - 9..]);
    let out = redact(&partial, Some(&k));
    assert!(
        !out.contains(&KEY[..10]) && !out.contains(&KEY[KEY.len() - 9..]),
        "{out}"
    );
    assert_eq!(redact("nothing here", Some(&k)), "nothing here");
    assert_eq!(redact("abc", None), "abc");
}

#[test]
fn secrets_never_print_and_stores_round_trip() {
    let s = Secret::new(KEY);
    assert_eq!(format!("{s:?}"), "Secret(***)");
    let cfg = config(ProviderKind::Anthropic, "http://x", Some(KEY));
    assert!(!format!("{cfg:?}").contains(KEY));

    let mem = MemorySecrets::new();
    assert_eq!(mem.get("a").unwrap(), None);
    mem.set("a", &s).unwrap();
    assert_eq!(mem.get("a").unwrap().unwrap().expose(), KEY);
    mem.delete("a").unwrap();
    mem.delete("a").unwrap();
    assert_eq!(mem.get("a").unwrap(), None);

    assert!(NoSecrets.set("a", &s).is_err());
    assert_eq!(NoSecrets.get("a").unwrap(), None);
}

#[test]
fn block_on_waits_for_a_pending_future() {
    struct Twice(u8);
    impl std::future::Future for Twice {
        type Output = u8;
        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<u8> {
            if self.0 == 0 {
                return std::task::Poll::Ready(7);
            }
            self.0 -= 1;
            let waker = cx.waker().clone();
            std::thread::spawn(move || waker.wake());
            std::task::Poll::Pending
        }
    }
    assert_eq!(block_on(Twice(2)), 7);
}

// ------------------------------------------------------------------ live (opt-in)

fn live(kind: ProviderKind, key_var: &str, url_var: Option<&str>, model_var: &str, model: &str) {
    let key = std::env::var(key_var).ok();
    let url = url_var.and_then(|v| std::env::var(v).ok());
    if key.is_none() && url.is_none() {
        return; // not configured: skipped
    }
    let mut cfg = config(
        kind,
        url.as_deref().unwrap_or(kind.default_base_url()),
        key.as_deref(),
    );
    cfg.timeout = Duration::from_secs(60);
    let p = build(cfg);
    let mut req = request(Some(json!({
        "type": "object",
        "properties": {"answer": {"type": "string"}},
        "required": ["answer"]
    })));
    req.model = Some(std::env::var(model_var).unwrap_or_else(|_| model.to_owned()));
    req.max_tokens = 200;
    let r = block_on(p.complete(req)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&r.text).unwrap();
    assert!(v["answer"].is_string(), "{}", r.text);
    assert!(r.usage.input_tokens > 0);
    assert!(!block_on(p.list_models()).unwrap().is_empty());
}

#[test]
fn live_anthropic() {
    live(
        ProviderKind::Anthropic,
        "FC_AI_LIVE_ANTHROPIC_API_KEY",
        None,
        "FC_AI_LIVE_ANTHROPIC_MODEL",
        "claude-haiku-5-5",
    );
}

#[test]
fn live_openai() {
    live(
        ProviderKind::Openai,
        "FC_AI_LIVE_OPENAI_API_KEY",
        None,
        "FC_AI_LIVE_OPENAI_MODEL",
        "gpt-4o-mini",
    );
}

#[test]
fn live_ollama() {
    live(
        ProviderKind::Ollama,
        "FC_AI_LIVE_OLLAMA_KEY_UNUSED",
        Some("FC_AI_LIVE_OLLAMA_URL"),
        "FC_AI_LIVE_OLLAMA_MODEL",
        "llama3.2",
    );
}
