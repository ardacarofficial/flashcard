//! Streaming, `Retry-After`, keychain persistence and card sources (PDF, web pages) — all
//! against the local mock server.

use super::*;
use crate::mock::{sample_pdf, MockRequest, MockResponse, MockServer};
use crate::secrets::{MemorySecrets, NoSecrets, SecretStore};
use crate::sources::{fetch_page, pdf_text, readable_text};
use fc_providers::ai::{CompletionRequest, ContentPart, Message, Role};
use fc_providers::ProviderErrorKind;
use serde_json::json;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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

fn text_request() -> CompletionRequest {
    CompletionRequest {
        model: Some("m-1".into()),
        system: Some("Explain.".into()),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentPart::Text {
                text: "Explain photosynthesis.".into(),
            }],
        }],
        max_tokens: 300,
        temperature: None,
        json_schema: None,
        feature: "ai.explain".into(),
    }
}

/// Runs a streaming completion, recording each piece and when it arrived.
fn stream(
    p: &dyn fc_providers::ai::AiProvider,
) -> (
    fc_providers::ProviderResult<fc_providers::ai::CompletionResponse>,
    Vec<(String, Instant)>,
) {
    let seen: Mutex<Vec<(String, Instant)>> = Mutex::new(Vec::new());
    let on_text = |t: &str| seen.lock().unwrap().push((t.to_owned(), Instant::now()));
    let r = block_on(p.complete_stream(text_request(), &on_text));
    (r, seen.into_inner().unwrap())
}

fn sse(events: &[serde_json::Value]) -> Vec<String> {
    events
        .iter()
        .map(|e| {
            format!(
                "event: {}\ndata: {}\n\n",
                e["type"].as_str().unwrap_or("message"),
                e
            )
        })
        .collect()
}

// ------------------------------------------------------------------ streaming

#[test]
fn anthropic_streams_sse_text_deltas_progressively() {
    let chunks = sse(&[
        json!({"type": "message_start", "message": {"model": "claude-x", "usage": {"input_tokens": 21, "output_tokens": 1}}}),
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
        json!({"type": "ping"}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Plants "}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "make "}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "sugar."}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 9}}),
        json!({"type": "message_stop"}),
    ]);
    let server =
        MockServer::start(move |_| MockResponse::stream("text/event-stream", chunks.clone(), 40));
    let p = build(config(ProviderKind::Anthropic, &server.url(), Some(KEY)));
    let (r, seen) = stream(&*p);
    let r = r.unwrap();
    assert_eq!(r.text, "Plants make sugar.");
    assert_eq!(r.model, "claude-x");
    assert_eq!((r.usage.input_tokens, r.usage.output_tokens), (21, 9));
    assert!(!r.truncated);
    let pieces: Vec<&str> = seen.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(pieces, ["Plants ", "make ", "sugar."]);
    // Progressive: pieces arrived as the server sent them, not all at the end.
    assert!(seen[2].1.duration_since(seen[0].1) >= Duration::from_millis(60));
    let req = &server.requests()[0];
    assert_eq!(req.path, "/v1/messages");
    assert_eq!(req.json()["stream"], json!(true));
    assert_eq!(req.header("x-api-key"), Some(KEY));
}

#[test]
fn anthropic_stream_error_event_is_an_error_without_the_key() {
    let chunks = sse(&[
        json!({"type": "message_start", "message": {"model": "claude-x", "usage": {"input_tokens": 1}}}),
        json!({"type": "error", "error": {"type": "overloaded_error", "message": format!("Overloaded {KEY}")}}),
    ]);
    let server =
        MockServer::start(move |_| MockResponse::stream("text/event-stream", chunks.clone(), 0));
    let p = build(config(ProviderKind::Anthropic, &server.url(), Some(KEY)));
    let (r, _) = stream(&*p);
    let e = r.unwrap_err();
    assert_eq!(e.kind, ProviderErrorKind::Unavailable);
    assert!(e.retryable);
    assert!(!e.message.contains(KEY), "{}", e.message);
}

#[test]
fn openai_streams_sse_chunks_with_usage_and_done() {
    let chunk = |content: &str| {
        format!(
            "data: {}\n\n",
            json!({"model": "gpt-x", "choices": [{"index": 0, "delta": {"content": content}, "finish_reason": null}]})
        )
    };
    let chunks = vec![
        chunk("Light "),
        chunk("becomes "),
        chunk("energy."),
        format!(
            "data: {}\n\n",
            json!({"model": "gpt-x", "choices": [{"index": 0, "delta": {}, "finish_reason": "length"}]})
        ),
        format!(
            "data: {}\n\n",
            json!({"model": "gpt-x", "choices": [], "usage": {"prompt_tokens": 12, "completion_tokens": 3}})
        ),
        "data: [DONE]\n\n".to_owned(),
    ];
    let server =
        MockServer::start(move |_| MockResponse::stream("text/event-stream", chunks.clone(), 5));
    let p = build(config(ProviderKind::Openai, &server.url(), Some(KEY)));
    let (r, seen) = stream(&*p);
    let r = r.unwrap();
    assert_eq!(r.text, "Light becomes energy.");
    assert_eq!((r.usage.input_tokens, r.usage.output_tokens), (12, 3));
    assert!(r.truncated);
    assert_eq!(seen.len(), 3);
    let body = server.requests()[0].json();
    assert_eq!(body["stream"], json!(true));
    assert_eq!(body["stream_options"]["include_usage"], json!(true));
    // A compatible server is not sent OpenAI-only stream options.
    let server2 = MockServer::start(|_| {
        MockResponse::stream("text/event-stream", vec!["data: [DONE]\n\n".into()], 0)
    });
    let p = build(config(ProviderKind::OpenaiCompatible, &server2.url(), None));
    let (r, _) = stream(&*p);
    assert_eq!(r.unwrap().text, "");
    assert!(server2.requests()[0].json().get("stream_options").is_none());
}

#[test]
fn ollama_streams_ndjson() {
    let line = |v: serde_json::Value| format!("{v}\n");
    let chunks = vec![
        line(
            json!({"model": "llama3", "message": {"role": "assistant", "content": "Chloro"}, "done": false}),
        ),
        line(
            json!({"model": "llama3", "message": {"role": "assistant", "content": "phyll"}, "done": false}),
        ),
        line(
            json!({"model": "llama3", "message": {"role": "assistant", "content": ""}, "done": true,
            "done_reason": "stop", "prompt_eval_count": 30, "eval_count": 4}),
        ),
    ];
    let server =
        MockServer::start(move |_| MockResponse::stream("application/x-ndjson", chunks.clone(), 5));
    let p = build(config(ProviderKind::Ollama, &server.url(), None));
    let (r, seen) = stream(&*p);
    let r = r.unwrap();
    assert_eq!(r.text, "Chlorophyll");
    assert_eq!((r.usage.input_tokens, r.usage.output_tokens), (30, 4));
    assert_eq!(seen.len(), 2);
    let body = server.requests()[0].json();
    assert_eq!(
        (body["stream"].clone(), server.requests()[0].path.as_str()),
        (json!(true), "/api/chat")
    );

    let server = MockServer::start(|_| {
        MockResponse::stream(
            "application/x-ndjson",
            vec!["{\"error\":\"model not found\"}\n".into()],
            0,
        )
    });
    let p = build(config(ProviderKind::Ollama, &server.url(), None));
    let (r, _) = stream(&*p);
    assert!(r.unwrap_err().message.contains("model not found"));
}

#[test]
fn streaming_http_errors_map_like_plain_requests() {
    let server = MockServer::start(|_| {
        MockResponse::status(429, &json!({"error": {"message": "slow down"}}))
            .header("retry-after", "7")
    });
    for kind in [
        ProviderKind::Anthropic,
        ProviderKind::Openai,
        ProviderKind::Ollama,
    ] {
        let p = build(config(kind, &server.url(), Some(KEY)));
        let (r, seen) = stream(&*p);
        let e = r.unwrap_err();
        assert_eq!(
            (e.kind, e.retry_after_ms),
            (ProviderErrorKind::RateLimited, Some(7000))
        );
        assert!(seen.is_empty());
    }
}

#[test]
fn structured_requests_do_not_stream_but_hand_over_the_whole_text() {
    let server = MockServer::start(|_| {
        MockResponse::json(&json!({
            "model": "m", "message": {"role": "assistant", "content": "{\"a\":1}"}, "done": true
        }))
    });
    let p = build(config(ProviderKind::Ollama, &server.url(), None));
    let seen = Mutex::new(Vec::new());
    let on_text = |t: &str| seen.lock().unwrap().push(t.to_owned());
    let mut req = text_request();
    req.json_schema = Some(json!({"type": "object"}));
    let r = block_on(p.complete_stream(req, &on_text)).unwrap();
    assert_eq!(r.text, "{\"a\":1}");
    assert_eq!(seen.into_inner().unwrap(), ["{\"a\":1}"]);
    assert_eq!(server.requests()[0].json()["stream"], json!(false));
}

#[test]
fn providers_report_streaming() {
    for kind in ProviderKind::ALL {
        let p = build(config(kind, "http://127.0.0.1:9", None));
        assert!(
            p.info()
                .capabilities
                .contains(&fc_providers::Capability::Streaming),
            "{kind:?}"
        );
    }
}

// ------------------------------------------------------------------ Retry-After

#[test]
fn retry_after_headers_are_parsed_and_capped() {
    type Headers = Vec<(&'static str, &'static str)>;
    let cases: Vec<(Headers, Option<u64>)> = vec![
        (vec![("retry-after", "3")], Some(3000)),
        (vec![("retry-after", "1.5")], Some(1500)),
        (
            vec![("retry-after-ms", "250"), ("retry-after", "9")],
            Some(250),
        ),
        (vec![("retry-after", "Wed, 21 Oct 2015 07:28:00 GMT")], None),
        (vec![("retry-after", "-4")], None),
        (vec![("retry-after", "99999")], Some(600_000)),
        (vec![], None),
    ];
    for (headers, want) in cases {
        let h = headers.clone();
        let server = MockServer::start(move |_| {
            let mut r = MockResponse::status(529, &json!({"error": {"message": "overloaded"}}));
            for (k, v) in &h {
                r = r.header(k, v);
            }
            r
        });
        let p = build(config(ProviderKind::Anthropic, &server.url(), Some(KEY)));
        let e = block_on(p.complete(text_request())).unwrap_err();
        assert_eq!(e.retry_after_ms, want, "{headers:?}");
        assert!(e.retryable);
    }
}

// ------------------------------------------------------------------ keychain

#[test]
fn session_only_stores_say_so() {
    assert!(!MemorySecrets::new().persistent());
    assert!(!NoSecrets.persistent());
    #[cfg(feature = "keychain")]
    assert_eq!(
        crate::secrets::OsKeychain::new("t").persistent(),
        !cfg!(target_os = "linux")
    );
}

// ------------------------------------------------------------------ PDF

#[test]
fn pdf_text_of_a_page_range() {
    let pdf = sample_pdf(
        "Biology notes",
        &[
            "Cells divide by mitosis.",
            "Leaves make sugar.",
            "Roots take water.",
        ],
    );
    let all = pdf_text(&pdf, None, None).unwrap();
    assert_eq!(all.page_count, Some(3));
    assert_eq!((all.from_page, all.to_page), (Some(1), Some(3)));
    assert_eq!(all.title.as_deref(), Some("Biology notes"));
    for t in ["mitosis", "sugar", "water"] {
        assert!(all.text.contains(t), "{t}: {}", all.text);
    }
    let some = pdf_text(&pdf, Some(2), Some(3)).unwrap();
    assert!(
        !some.text.contains("mitosis")
            && some.text.contains("sugar")
            && some.text.contains("water")
    );
    let one = pdf_text(&pdf, Some(2), Some(2)).unwrap();
    assert!(one.text.contains("sugar") && !one.text.contains("water"));
    // `to` past the end is clamped; empty or inverted ranges are refused.
    assert_eq!(pdf_text(&pdf, Some(3), Some(99)).unwrap().to_page, Some(3));
    assert_eq!(pdf_text(&pdf, Some(0), None).unwrap_err().reason, "pages");
    assert_eq!(
        pdf_text(&pdf, Some(3), Some(2)).unwrap_err().reason,
        "pages"
    );
    assert_eq!(pdf_text(&pdf, Some(4), None).unwrap_err().reason, "pages");
}

#[test]
fn pdf_errors() {
    assert_eq!(
        pdf_text(b"hello", None, None).unwrap_err().reason,
        "not_pdf"
    );
    assert_eq!(
        pdf_text(b"%PDF-1.5 garbage", None, None)
            .unwrap_err()
            .reason,
        "not_pdf"
    );
    let blank = sample_pdf("", &[""]);
    assert_eq!(pdf_text(&blank, None, None).unwrap_err().reason, "no_text");
    let dir = std::env::temp_dir().join(format!("fc-ai-pdf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.pdf");
    std::fs::write(&path, sample_pdf("T", &["File text."])).unwrap();
    assert!(sources::pdf_file(&path, None, None)
        .unwrap()
        .text
        .contains("File text."));
    assert_eq!(
        sources::pdf_file(&dir.join("missing.pdf"), None, None)
            .unwrap_err()
            .reason,
        "io"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------------------ web pages

const PAGE: &str = r#"<!DOCTYPE html>
<html><head><title>Photosynthesis &amp; you</title>
<style>p { color: red }</style>
<script>var x = "<p>not text</p>"; document.write('<b>')</script></head>
<body>
<nav><a href="/">Home</a> <a href="/x">Menu entry</a></nav>
<header><h1>Site name</h1></header>
<!-- a comment <p>hidden</p> -->
<article>
  <h1>How plants eat</h1>
  <p>Plants turn <b>light</b>, water and CO&#8322; into sugar &mdash; this is called photosynthesis.
  It happens in the chloroplasts, which contain the green pigment chlorophyll.</p>
  <ul><li>Light reactions</li><li data-x="a>b">Calvin cycle</li></ul>
  <form><input value="Search"><button>Go</button></form>
  <p>Oxygen is released as a by-product, which is what animals and people breathe.</p>
</article>
<aside>Related: buy seeds!</aside>
<footer>© 2026 Example</footer>
</body></html>"#;

#[test]
fn readable_text_keeps_the_article_and_drops_the_chrome() {
    let r = readable_text(PAGE);
    assert_eq!(r.title.as_deref(), Some("Photosynthesis & you"));
    let t = &r.text;
    assert!(t.starts_with("How plants eat"), "{t}");
    assert!(
        t.contains("Plants turn light, water and CO₂ into sugar — this is called photosynthesis."),
        "{t}"
    );
    assert!(t.contains("- Light reactions\n- Calvin cycle"), "{t}");
    assert!(t.contains("Oxygen is released"));
    for gone in [
        "Home",
        "Menu entry",
        "Site name",
        "not text",
        "hidden",
        "color",
        "Search",
        "Go",
        "buy seeds",
        "2026",
    ] {
        assert!(!t.contains(gone), "{gone} leaked: {t}");
    }
    // No article/main: the body without chrome.
    let r = readable_text(
        "<html><body><nav>x</nav><div>Hello <i>world</i></div><p>Second</p></body></html>",
    );
    assert_eq!(r.text, "Hello world\n\nSecond");
    // Fragments without <body> still give their text.
    assert_eq!(readable_text("just <b>text</b> &lt;3").text, "just text <3");
}

#[test]
fn fetch_page_reads_html_without_running_scripts() {
    let server = MockServer::start(|r: &MockRequest| match r.path.as_str() {
        "/article" => MockResponse::text(200, "text/html; charset=utf-8", PAGE),
        "/plain" => MockResponse::text(200, "text/plain", "line one\n\n\n\nline   two"),
        "/latin" => MockResponse::text(200, "text/html; charset=iso-8859-1", "<p>caf\u{e9}</p>"),
        "/moved" => MockResponse::text(302, "text/html", "").header("location", "/article"),
        "/image" => MockResponse::text(200, "image/png", "PNG"),
        "/missing" => MockResponse::text(404, "text/html", "<p>nope</p>"),
        "/empty" => MockResponse::text(200, "text/html", "<html><script>x()</script></html>"),
        "/huge" => MockResponse::text(200, "text/html", "a".repeat(5 * 1024 * 1024)),
        _ => MockResponse::text(500, "text/plain", ""),
    });
    let url = server.url();
    let page = fetch_page(&format!("{url}/article")).unwrap();
    assert!(page.text.contains("photosynthesis"));
    assert!(!page.text.contains("not text"));
    assert_eq!(
        server.requests()[0]
            .header("accept")
            .map(|a| a.contains("text/html")),
        Some(true)
    );
    assert_eq!(
        fetch_page(&format!("{url}/plain")).unwrap().text,
        "line one\n\nline two"
    );
    assert!(fetch_page(&format!("{url}/moved"))
        .unwrap()
        .text
        .contains("Calvin cycle"));
    // The mock writes the String's UTF-8 bytes; a Latin-1 page is decoded byte by byte.
    assert!(fetch_page(&format!("{url}/latin"))
        .unwrap()
        .text
        .starts_with("caf"));
    let reason = |path: &str| fetch_page(&format!("{url}{path}")).unwrap_err().reason;
    assert_eq!(reason("/image"), "content_type");
    assert_eq!(reason("/missing"), "http_status");
    assert_eq!(reason("/empty"), "no_text");
    assert_eq!(reason("/huge"), "too_large");
    for bad in [
        "file:///etc/passwd",
        "ftp://x",
        "javascript:alert(1)",
        "http://",
        "http://a b",
    ] {
        assert_eq!(fetch_page(bad).unwrap_err().reason, "url", "{bad}");
    }
    let gone = {
        let s = MockServer::start(|_| MockResponse::json(&json!({})));
        s.url()
    };
    assert_eq!(fetch_page(&gone).unwrap_err().reason, "unreachable");
}

#[test]
fn a_server_that_ignores_stream_still_answers() {
    let server = MockServer::start(|r| match r.path.as_str() {
        "/v1/messages" => MockResponse::json(&json!({
            "model": "claude-x", "content": [{"type": "text", "text": "Whole answer."}],
            "stop_reason": "end_turn", "usage": {"input_tokens": 3, "output_tokens": 2}
        })),
        "/chat/completions" => MockResponse::json(&json!({
            "model": "gpt-x", "choices": [{"message": {"content": "Whole answer."}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 3, "completion_tokens": 2}
        })),
        _ => MockResponse::json(&json!({
            "model": "llama", "message": {"content": "Whole answer."}, "done": true,
            "prompt_eval_count": 3, "eval_count": 2
        })),
    });
    for kind in [
        ProviderKind::Anthropic,
        ProviderKind::Openai,
        ProviderKind::Ollama,
    ] {
        let p = build(config(kind, &server.url(), Some(KEY)));
        let (r, seen) = stream(&*p);
        let r = r.unwrap();
        assert_eq!(
            (r.text.as_str(), r.usage.input_tokens, r.usage.output_tokens),
            ("Whole answer.", 3, 2)
        );
        assert_eq!(seen.len(), 1, "{kind:?}");
    }
}
