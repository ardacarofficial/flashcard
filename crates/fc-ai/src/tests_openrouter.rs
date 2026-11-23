//! v1.4 G1: what the first run against a real provider (OpenRouter) found, replayed offline.
//! The bodies below are recorded OpenRouter answers (ids shortened), so CI needs no network.

use super::*;
use crate::mock::{MockResponse, MockServer};
use fc_providers::ai::{CompletionRequest, ContentPart, Message, Role};
use serde_json::json;
use std::time::Duration;

// Not a key: a placeholder in the provider's format, split so secret scanners do not flag it.
const KEY: &str = concat!("sk-or-", "v1-", "0000000000000000");

fn openrouter(url: &str) -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::OpenaiCompatible,
        base_url: url.to_owned(),
        api_key: Some(Secret::new(KEY)),
        timeout: Duration::from_secs(10),
        media: None,
    }
}

fn request(model: &str) -> CompletionRequest {
    CompletionRequest {
        model: Some(model.into()),
        system: None,
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentPart::Text {
                text: "Say OK".into(),
            }],
        }],
        max_tokens: 5,
        temperature: None,
        json_schema: None,
        feature: "ai.explain".into(),
    }
}

/// `POST /chat/completions` on openrouter.ai, 2026-11-23 (google/gemini-2.5-flash-lite).
fn recorded_completion() -> serde_json::Value {
    json!({
        "id": "gen-1795429430-JpDr", "object": "chat.completion", "created": 1795429430u64,
        "model": "google/gemini-2.5-flash-lite", "provider": "Google AI Studio",
        "system_fingerprint": null, "service_tier": "default",
        "choices": [{"index": 0, "logprobs": null, "finish_reason": "stop", "native_finish_reason": "STOP",
                     "message": {"role": "assistant", "content": "OK", "refusal": null, "reasoning": null}}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 1, "total_tokens": 4, "cost": 7e-7, "is_byok": false,
                  "prompt_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0, "audio_tokens": 0, "video_tokens": 0},
                  "cost_details": {"upstream_inference_cost": 7e-7, "upstream_inference_prompt_cost": 3e-7,
                                   "upstream_inference_completions_cost": 4e-7},
                  "completion_tokens_details": {"reasoning_tokens": 0, "image_tokens": 0, "audio_tokens": 0}}
    })
}

#[test]
fn openrouter_reported_cost_is_read() {
    let server = MockServer::start(|_| MockResponse::json(&recorded_completion()));
    let p = build(openrouter(&server.url()));
    let r = block_on(p.complete(request("google/gemini-2.5-flash-lite"))).unwrap();
    assert_eq!(r.text, "OK");
    assert_eq!((r.usage.input_tokens, r.usage.output_tokens), (3, 1));
    assert_eq!(r.usage.cost_usd, Some(7e-7));
    // Providers that report no cost leave it to the app's price table.
    let mut plain = recorded_completion();
    plain["usage"].as_object_mut().unwrap().remove("cost");
    let server = MockServer::start(move |_| MockResponse::json(&plain));
    let r = block_on(build(openrouter(&server.url())).complete(request("m"))).unwrap();
    assert_eq!(r.usage.cost_usd, None);
    // A nonsense figure is ignored, never stored.
    let mut bad = recorded_completion();
    bad["usage"]["cost"] = json!(-1.0);
    let server = MockServer::start(move |_| MockResponse::json(&bad));
    let r = block_on(build(openrouter(&server.url())).complete(request("m"))).unwrap();
    assert_eq!(r.usage.cost_usd, None);
}

#[test]
fn certificates_are_checked_against_the_operating_system_store() {
    // A TLS-inspecting antivirus or company proxy installs its root in the OS store: the
    // compiled-in list (ureq's default) rejected every provider there.
    assert!(matches!(
        crate::http::tls().root_certs(),
        ureq::tls::RootCerts::PlatformVerifier
    ));
}

/// HTTP 429 from openrouter.ai, 2026-11-23 (a `:free` model, 25 requests at once; no
/// `Retry-After` header was sent).
fn recorded_rate_limit() -> serde_json::Value {
    json!({"error": {"message": "Provider returned error", "code": 429, "metadata": {
        "raw": "google/gemma-4-31b-it:free is temporarily rate-limited upstream. Please retry shortly, or add your own provider key to use your own limits: https://openrouter.ai/workspaces/default/byok",
        "provider_name": "Google AI Studio", "is_byok": false, "provider_error_code": "429",
        "limit_source": "upstream_provider_shared_pool",
        "remedy_hint": "Retry shortly, add your own provider key to use your own limits (https://openrouter.ai/workspaces/default/byok), or route to another provider with provider routing: https://openrouter.ai/docs/features/provider-routing."}}})
}

#[test]
fn an_upstream_rate_limit_says_why() {
    let server = MockServer::start(|_| MockResponse::status(429, &recorded_rate_limit()));
    let e =
        block_on(build(openrouter(&server.url())).complete(request("google/gemma-4-31b-it:free")))
            .unwrap_err();
    assert_eq!(e.kind, fc_providers::ProviderErrorKind::RateLimited);
    assert!(e.retryable);
    // Not just "Provider returned error": the upstream reason is kept.
    assert!(e.message.contains("Provider returned error: google/gemma-4-31b-it:free is temporarily rate-limited upstream"), "{}", e.message);
    assert!(!e.message.contains(KEY));
    // The connection test explains it the same way.
    let d = crate::diagnose::test_connection(
        &openrouter(&server.url()),
        Some("google/gemma-4-31b-it:free"),
    );
    assert_eq!(d.outcome, crate::diagnose::Outcome::RateLimited);
    assert!(d
        .message
        .unwrap_or_default()
        .contains("rate-limited upstream"));
}
