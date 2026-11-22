//! Presets (recognition, key rules) and the connection test against the local mock server.

use super::*;
use crate::diagnose::{classify, test_connection, Outcome, ProbeKind, PROBE_MAX_TOKENS};
use crate::mock::{MockResponse, MockServer};
use crate::presets::{detect, is_loopback, is_private, needs_key, preset, CUSTOM, PRESETS};
use serde_json::json;
use std::time::Duration;

const KEY: &str = "sk-or-test-SECRET-0123456789abcdef";

fn config(kind: ProviderKind, url: &str, key: Option<&str>) -> ProviderConfig {
    ProviderConfig {
        kind,
        base_url: url.to_owned(),
        api_key: key.map(Secret::new),
        timeout: Duration::from_secs(10),
        media: None,
    }
}

// ------------------------------------------------------------------ presets

#[test]
fn every_preset_is_well_formed_and_unique() {
    let ids: Vec<&str> = PRESETS.iter().map(|p| p.id).collect();
    for (i, id) in ids.iter().enumerate() {
        assert!(!ids[..i].contains(id), "duplicate preset {id}");
    }
    assert_eq!(ids.first(), Some(&"anthropic"), "Claude leads the gallery");
    assert_eq!(ids.last(), Some(&CUSTOM), "the custom endpoint comes last");
    for p in PRESETS {
        if p.id == CUSTOM {
            assert!(p.base_url.is_empty() && p.name.is_empty());
            continue;
        }
        assert!(
            p.base_url.starts_with("https://") || p.local,
            "{}: hosted presets use https",
            p.id
        );
        assert!(!p.name.is_empty());
        // Hosted ⇒ key needed and a page to get it; local ⇒ no key, a download page, free.
        assert_eq!(p.needs_key, !p.local, "{}", p.id);
        assert_eq!(p.key_url.is_some(), p.needs_key, "{}", p.id);
        if p.local {
            assert!(
                p.free && p.setup_url.is_some() && is_loopback(p.base_url),
                "{}",
                p.id
            );
        }
        if !p.default_model.is_empty() {
            assert!(p.suggested_models.contains(&p.default_model), "{}", p.id);
        }
        // A provider created from the preset is recognised as that preset again.
        assert_eq!(detect(p.kind, p.base_url).id, p.id);
        assert_eq!(needs_key(p.kind, p.base_url), p.needs_key, "{}", p.id);
    }
}

#[test]
fn hosted_compatible_endpoints_need_a_key_and_own_servers_do_not() {
    let c = ProviderKind::OpenaiCompatible;
    // The v1.2 bug: a compatible provider never asked for a key, so OpenRouter could not work.
    assert!(needs_key(c, "https://openrouter.ai/api/v1"));
    assert_eq!(detect(c, "https://openrouter.ai/api/v1/").id, "openrouter");
    assert_eq!(detect(c, "https://OPENROUTER.ai/api/v1").id, "openrouter");
    assert_eq!(detect(c, "https://api.groq.com/openai/v1").id, "groq");
    assert_eq!(
        detect(c, "https://generativelanguage.googleapis.com/v1beta/openai").id,
        "gemini"
    );
    // Unknown hosted address: custom, key needed.
    assert_eq!(detect(c, "https://llm.example.com/v1").id, CUSTOM);
    assert!(needs_key(c, "https://llm.example.com/v1"));
    // This computer / the local network: key optional.
    assert_eq!(detect(c, "http://localhost:1234/v1").id, "lmstudio");
    assert_eq!(detect(c, "http://127.0.0.1:1234/v1").id, "lmstudio");
    assert_eq!(detect(c, "http://localhost:8000/v1").id, CUSTOM);
    for url in [
        "http://localhost:8000/v1",
        "http://[::1]:8080/v1",
        "http://192.168.1.20:8000/v1",
        "http://10.0.0.5/v1",
        "http://gpu-box.local:8000/v1",
        "http://gpubox:8000/v1",
    ] {
        assert!(!needs_key(c, url), "{url}");
        assert!(is_private(url), "{url}");
    }
    assert!(!is_loopback("http://192.168.1.20:8000"));
    assert!(is_loopback("http://[::1]:8080/v1"));
    assert!(!is_private("https://8.8.8.8/v1"));
    // Native kinds keep their rule wherever they point.
    assert!(needs_key(ProviderKind::Anthropic, "http://localhost:9999"));
    assert!(!needs_key(ProviderKind::Ollama, "http://gpu-box:11434"));
    assert_eq!(preset("openrouter").unwrap().kind, c);
    assert!(preset("nope").is_none());
}

// ------------------------------------------------------------------ classification

#[test]
fn known_vendor_errors_are_classified() {
    use ProbeKind::{Chat, Models};
    use ProviderKind::{Anthropic, Ollama, Openai, OpenaiCompatible as Compat};
    let cases: &[(ProviderKind, ProbeKind, u16, &str, Outcome)] = &[
        (
            Anthropic,
            Chat,
            200,
            r#"{"content":[{"type":"text","text":"OK"}],"model":"m"}"#,
            Outcome::Ok,
        ),
        (
            Compat,
            Chat,
            200,
            r#"{"choices":[{"message":{"content":"OK"}}]}"#,
            Outcome::Ok,
        ),
        (
            Ollama,
            Chat,
            200,
            r#"{"message":{"role":"assistant","content":"OK"},"done":true}"#,
            Outcome::Ok,
        ),
        (Compat, Models, 200, r#"{"data":[{"id":"a"}]}"#, Outcome::Ok),
        (Ollama, Models, 200, r#"{"models":[]}"#, Outcome::Ok),
        // A web page (or another API) at the address.
        (
            Compat,
            Chat,
            200,
            "<html>Welcome</html>",
            Outcome::WrongAddress,
        ),
        (
            Compat,
            Models,
            200,
            r#"{"status":"ok"}"#,
            Outcome::WrongAddress,
        ),
        // Keys.
        (
            Anthropic,
            Chat,
            401,
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
            Outcome::WrongKey,
        ),
        (
            Compat,
            Chat,
            401,
            r#"{"error":{"message":"No auth credentials found","code":401}}"#,
            Outcome::WrongKey,
        ),
        (
            Compat,
            Chat,
            400,
            r#"[{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT"}}]"#,
            Outcome::WrongKey,
        ),
        // Credit.
        (
            Compat,
            Chat,
            402,
            r#"{"error":{"message":"Insufficient credits","code":402}}"#,
            Outcome::NoCredit,
        ),
        (
            Compat,
            Chat,
            402,
            r#"{"error":{"message":"Insufficient Balance"}}"#,
            Outcome::NoCredit,
        ),
        (
            Anthropic,
            Chat,
            400,
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"Your credit balance is too low to access the Anthropic API."}}"#,
            Outcome::NoCredit,
        ),
        (
            Openai,
            Chat,
            429,
            r#"{"error":{"message":"You exceeded your current quota, please check your plan and billing details.","type":"insufficient_quota","code":"insufficient_quota"}}"#,
            Outcome::NoCredit,
        ),
        // Models.
        (
            Openai,
            Chat,
            404,
            r#"{"error":{"message":"The model `gpt-x` does not exist or you do not have access to it.","code":"model_not_found"}}"#,
            Outcome::ModelNotFound,
        ),
        (
            Anthropic,
            Chat,
            404,
            r#"{"type":"error","error":{"type":"not_found_error","message":"model: claude-x"}}"#,
            Outcome::ModelNotFound,
        ),
        (
            Ollama,
            Chat,
            404,
            r#"{"error":"model \"llama9\" not found, try pulling it first"}"#,
            Outcome::ModelNotFound,
        ),
        (
            Compat,
            Chat,
            400,
            r#"{"error":{"message":"foo/bar is not a valid model ID","code":400}}"#,
            Outcome::ModelNotFound,
        ),
        (
            Compat,
            Chat,
            400,
            r#"{"object":"error","message":"Invalid model: mistral-x","type":"invalid_model"}"#,
            Outcome::ModelNotFound,
        ),
        (
            Compat,
            Chat,
            400,
            r#"{"error":{"message":"Model Not Exist"}}"#,
            Outcome::ModelNotFound,
        ),
        // Addresses and the rest.
        (
            Compat,
            Models,
            404,
            "404 page not found",
            Outcome::WrongAddress,
        ),
        (
            Compat,
            Chat,
            404,
            "<html>Not Found</html>",
            Outcome::WrongAddress,
        ),
        (
            Compat,
            Chat,
            429,
            r#"{"error":{"message":"Rate limit reached"}}"#,
            Outcome::RateLimited,
        ),
        (
            Anthropic,
            Chat,
            529,
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
            Outcome::ServiceDown,
        ),
        (
            Compat,
            Chat,
            400,
            r#"{"error":{"message":"temperature must be <= 2"}}"#,
            Outcome::Failed,
        ),
        (
            Anthropic,
            Chat,
            402,
            r#"{"type":"error","error":{"type":"billing_error","message":"Payment required"}}"#,
            Outcome::NoCredit,
        ),
        (
            Openai,
            Chat,
            429,
            r#"{"error":{"message":"Out of credit","type":"insufficient_quota","code":"credit_balance_exhausted"}}"#,
            Outcome::NoCredit,
        ),
        // OpenRouter: an error inside a 200 answer.
        (
            Compat,
            Chat,
            200,
            r#"{"error":{"code":402,"message":"Insufficient credits"}}"#,
            Outcome::NoCredit,
        ),
        (
            Compat,
            Models,
            200,
            r#"[{"id":"mistral-small-latest"}]"#,
            Outcome::Ok,
        ),
    ];
    for (kind, probe, status, body, want) in cases {
        assert_eq!(
            classify(*kind, *probe, *status, body),
            *want,
            "{kind:?} {status} {body}"
        );
    }
}

// ------------------------------------------------------------------ test_connection

#[test]
fn a_working_provider_answers_ok_with_a_tiny_request() {
    let server = MockServer::start(|_| {
        MockResponse::json(&json!({
            "model": "openrouter/auto-picked",
            "choices": [{"message": {"role": "assistant", "content": "OK"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 1}
        }))
    });
    let d = test_connection(
        &config(ProviderKind::OpenaiCompatible, &server.url(), Some(KEY)),
        Some("openrouter/auto"),
    );
    assert_eq!(d.outcome, Outcome::Ok);
    assert_eq!((d.probe, d.status), (Some(ProbeKind::Chat), Some(200)));
    assert_eq!(d.model.as_deref(), Some("openrouter/auto-picked"));
    assert_eq!(d.message, None);
    let req = &server.requests()[0];
    assert_eq!(
        (req.method.as_str(), req.path.as_str()),
        ("POST", "/chat/completions")
    );
    assert_eq!(
        req.header("authorization"),
        Some(format!("Bearer {KEY}").as_str())
    );
    let body = req.json();
    assert_eq!(body["model"], "openrouter/auto");
    assert_eq!(body["max_tokens"], PROBE_MAX_TOKENS);
}

#[test]
fn each_adapter_probes_its_own_endpoint() {
    let server = MockServer::start(|r| match r.path.as_str() {
        "/v1/messages" => MockResponse::json(
            &json!({"content": [{"type": "text", "text": "OK"}], "model": "claude-haiku-5-5"}),
        ),
        "/v1/models?limit=1" => {
            MockResponse::json(&json!({"data": [{"id": "claude-haiku-5-5"}], "has_more": true}))
        }
        "/api/chat" => MockResponse::json(
            &json!({"message": {"role": "assistant", "content": "OK"}, "done": true}),
        ),
        "/api/tags" => MockResponse::json(&json!({"models": [{"name": "qwen3"}]})),
        "/models" => MockResponse::json(&json!({"data": [{"id": "gpt-x"}]})),
        "/chat/completions" => {
            MockResponse::json(&json!({"choices": [{"message": {"content": "OK"}}]}))
        }
        _ => MockResponse::text(404, "text/plain", "404 page not found"),
    });
    let url = server.url();
    let cases = [
        (
            ProviderKind::Anthropic,
            Some("claude-haiku-5-5"),
            "POST",
            "/v1/messages",
        ),
        (ProviderKind::Anthropic, None, "GET", "/v1/models?limit=1"),
        (ProviderKind::Ollama, Some("qwen3"), "POST", "/api/chat"),
        (ProviderKind::Ollama, None, "GET", "/api/tags"),
        (
            ProviderKind::Openai,
            Some("gpt-x"),
            "POST",
            "/chat/completions",
        ),
        (ProviderKind::Openai, None, "GET", "/models"),
    ];
    for (i, (kind, model, method, path)) in cases.iter().enumerate() {
        let d = test_connection(&config(*kind, &url, Some(KEY)), *model);
        assert_eq!(d.outcome, Outcome::Ok, "{kind:?} {model:?}: {d:?}");
        let req = &server.requests()[i];
        assert_eq!((req.method.as_str(), req.path.as_str()), (*method, *path));
    }
    let reqs = server.requests();
    assert_eq!(reqs[0].header("x-api-key"), Some(KEY));
    assert_eq!(reqs[0].json()["max_tokens"], PROBE_MAX_TOKENS);
    assert_eq!(reqs[2].json()["options"]["num_predict"], PROBE_MAX_TOKENS);
    assert_eq!(reqs[4].json()["max_completion_tokens"], PROBE_MAX_TOKENS);
}

#[test]
fn failures_are_explained_and_never_carry_the_key() {
    let server = MockServer::start(|r| {
        let model = r.json()["model"].as_str().unwrap_or_default().to_owned();
        match model.as_str() {
            "broke" => MockResponse::status(
                402,
                &json!({"error": {"message": "Insufficient credits", "code": 402}}),
            ),
            "echo" => MockResponse::status(
                401,
                &json!({"error": {"message": format!("Incorrect API key provided: {KEY}")}}),
            ),
            "ghost" => MockResponse::status(
                404,
                &json!({"error": {"message": format!("The model `ghost` does not exist (key {KEY})"), "code": "model_not_found"}}),
            ),
            _ => MockResponse::status(503, &json!({"error": {"message": "upstream down"}})),
        }
    });
    let cfg = config(ProviderKind::OpenaiCompatible, &server.url(), Some(KEY));
    let d = test_connection(&cfg, Some("broke"));
    assert_eq!((d.outcome, d.status), (Outcome::NoCredit, Some(402)));
    assert_eq!(d.message.as_deref(), Some("Insufficient credits"));
    // A rejected key's body is never kept (it may echo the key).
    let d = test_connection(&cfg, Some("echo"));
    assert_eq!((d.outcome, d.message.as_deref()), (Outcome::WrongKey, None));
    let d = test_connection(&cfg, Some("ghost"));
    assert_eq!(d.outcome, Outcome::ModelNotFound);
    let m = d.message.unwrap();
    assert!(m.contains("does not exist") && !m.contains("SECRET"), "{m}");
    let d = test_connection(&cfg, Some("other"));
    assert_eq!(
        (d.outcome, d.message.as_deref()),
        (Outcome::ServiceDown, Some("upstream down"))
    );
}

#[test]
fn no_key_means_nothing_is_sent() {
    let server = MockServer::start(|_| MockResponse::json(&json!({})));
    // An OpenRouter-like hosted address without a key: refused before any request.
    let mut cfg = config(
        ProviderKind::OpenaiCompatible,
        "https://openrouter.ai/api/v1",
        None,
    );
    let d = test_connection(&cfg, Some("openrouter/auto"));
    assert_eq!((d.outcome, d.probe), (Outcome::NoKey, None));
    // A local server needs none: the request goes out.
    cfg.base_url = server.url();
    let d = test_connection(&cfg, None);
    assert_eq!(d.outcome, Outcome::WrongAddress, "{{}} is not a model list");
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.requests()[0].header("authorization"), None);
}

#[test]
fn unreachable_and_tls_failures_are_told_apart() {
    // Nothing listens on the port any more.
    let url = {
        let s = MockServer::start(|_| MockResponse::json(&json!({})));
        s.url()
    };
    let d = test_connection(&config(ProviderKind::Ollama, &url, None), Some("qwen3"));
    assert_eq!((d.outcome, d.status), (Outcome::CannotReach, None));
    assert!(d.message.is_some());

    // HTTPS to a server that answers in plain HTTP: the TLS handshake fails.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let plain = std::thread::spawn(move || {
        use std::io::Write;
        if let Ok((mut s, _)) = listener.accept() {
            let _ = s.write_all(b"HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n");
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    let https = format!("https://{addr}");
    let d = test_connection(
        &config(ProviderKind::OpenaiCompatible, &https, Some(KEY)),
        None,
    );
    assert_eq!(d.outcome, Outcome::Tls, "{d:?}");
    plain.join().unwrap();
}

#[test]
fn compatible_model_lists_send_the_key_when_one_is_set() {
    let server = MockServer::start(|_| {
        MockResponse::json(
            &json!({"data": [{"id": "openrouter/auto"}, {"id": "meta-llama/llama-3.3-70b-instruct:free"}]}),
        )
    });
    let p = build(config(
        ProviderKind::OpenaiCompatible,
        &server.url(),
        Some(KEY),
    ));
    let models = block_on(p.list_models()).unwrap();
    assert_eq!(
        models,
        ["openrouter/auto", "meta-llama/llama-3.3-70b-instruct:free"]
    );
    assert_eq!(
        server.requests()[0].header("authorization"),
        Some(format!("Bearer {KEY}").as_str())
    );
}

#[test]
fn google_style_error_arrays_keep_their_message() {
    assert_eq!(
        crate::http::error_message(
            r#"[{"error":{"code":429,"message":"Quota exceeded","status":"RESOURCE_EXHAUSTED"}}]"#
        ),
        "Quota exceeded"
    );
}
