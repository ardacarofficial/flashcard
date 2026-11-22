//! "Test the connection" (v1.3): one minimal request to a configured provider, explained in
//! plain categories — works, wrong key, no credit, model not found, wrong address, cannot
//! reach, TLS problem, … — with the provider's own message.
//!
//! The probe is a tiny completion (`"Reply with OK."`, at most [`PROBE_MAX_TOKENS`] output
//! tokens) to the chosen model, built by the same adapter code a real request uses; without a
//! model it is the model-list request (free). Failures are classified from the raw HTTP status
//! and body ([`classify`]), not from the generic [`fc_providers::ProviderError`] mapping, so
//! "your key is wrong" and "your balance is empty" are told apart.

use crate::http::{error_message, Http};
use crate::{anthropic, ollama, openai, presets, redact, ProviderConfig, ProviderKind};
use fc_providers::ai::{CompletionRequest, ContentPart, Message, Role};
use serde_json::Value;
use std::time::Instant;

/// Output cap of the probe completion (a few tokens: the test costs next to nothing).
pub const PROBE_MAX_TOKENS: u32 = 16;

/// Longest provider message kept in a [`Diagnosis`].
const MAX_MESSAGE_CHARS: usize = 300;

/// What a connection test found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// The provider answered.
    Ok,
    /// The provider needs a key and none is stored (nothing was sent).
    NoKey,
    /// The key was rejected.
    WrongKey,
    /// The account has no credit / quota left (payment needed).
    NoCredit,
    /// The model does not exist or this key may not use it.
    ModelNotFound,
    /// Something answered, but not this API: the base URL is probably wrong.
    WrongAddress,
    /// Too many requests right now.
    RateLimited,
    /// The provider had an error of its own (5xx, overloaded).
    ServiceDown,
    /// The server could not be reached (offline, wrong host, nothing listening, timeout).
    CannotReach,
    /// The secure connection failed (certificate, TLS interception by a proxy/antivirus).
    Tls,
    /// Anything else (the provider's message says what).
    Failed,
}

/// Which request the test sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    /// A tiny completion to the model.
    Chat,
    /// The model list (no model chosen yet).
    Models,
}

/// Result of [`test_connection`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnosis {
    /// Category.
    pub outcome: Outcome,
    /// Request sent (`None` when nothing was sent: no key).
    pub probe: Option<ProbeKind>,
    /// HTTP status of the answer (`None`: no answer).
    pub status: Option<u16>,
    /// The provider's (or the network's) own message, key scrubbed; never kept for rejected
    /// keys (some providers echo the key back).
    pub message: Option<String>,
    /// Model that answered (chat probe), as the provider reports it.
    pub model: Option<String>,
    /// Round trip.
    pub latency_ms: u64,
}

/// One request: method, path, headers and body.
struct Probe {
    kind: ProbeKind,
    post: bool,
    path: String,
    headers: Vec<(&'static str, String)>,
    body: Option<Value>,
}

fn probe_request(model: &str) -> CompletionRequest {
    CompletionRequest {
        model: Some(model.to_owned()),
        system: None,
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentPart::Text {
                text: "Reply with OK.".into(),
            }],
        }],
        max_tokens: PROBE_MAX_TOKENS,
        temperature: None,
        json_schema: None,
        feature: "ai.test_connection".into(),
    }
}

fn build_probe(config: &ProviderConfig, model: Option<&str>) -> Result<Probe, String> {
    let key = config.api_key.as_ref().map(|k| k.expose().to_owned());
    let bearer = key
        .as_ref()
        .map(|k| ("authorization", format!("Bearer {k}")));
    let model = model.map(str::trim).filter(|m| !m.is_empty());
    let probe = match (config.kind, model) {
        (ProviderKind::Anthropic, model) => {
            let mut headers = vec![("anthropic-version", anthropic::VERSION.to_owned())];
            if let Some(k) = key {
                headers.push(("x-api-key", k));
            }
            match model {
                Some(m) => Probe {
                    kind: ProbeKind::Chat,
                    post: true,
                    path: "/v1/messages".into(),
                    headers,
                    body: Some(
                        anthropic::Anthropic::new(config.clone())
                            .body(&probe_request(m))
                            .map_err(|e| e.message)?,
                    ),
                },
                None => Probe {
                    kind: ProbeKind::Models,
                    post: false,
                    path: "/v1/models?limit=1".into(),
                    headers,
                    body: None,
                },
            }
        }
        (ProviderKind::Openai | ProviderKind::OpenaiCompatible, model) => match model {
            Some(m) => Probe {
                kind: ProbeKind::Chat,
                post: true,
                path: "/chat/completions".into(),
                headers: bearer.into_iter().collect(),
                body: Some(
                    openai::OpenAi::new(config.clone())
                        .body(&probe_request(m))
                        .map_err(|e| e.message)?,
                ),
            },
            None => Probe {
                kind: ProbeKind::Models,
                post: false,
                path: "/models".into(),
                headers: bearer.into_iter().collect(),
                body: None,
            },
        },
        (ProviderKind::Ollama, model) => match model {
            Some(m) => Probe {
                kind: ProbeKind::Chat,
                post: true,
                path: "/api/chat".into(),
                headers: bearer.into_iter().collect(),
                body: Some(
                    ollama::Ollama::new(config.clone())
                        .body(&probe_request(m))
                        .map_err(|e| e.message)?,
                ),
            },
            None => Probe {
                kind: ProbeKind::Models,
                post: false,
                path: "/api/tags".into(),
                headers: bearer.into_iter().collect(),
                body: None,
            },
        },
    };
    Ok(probe)
}

/// Sends one minimal request to the provider of `config` (a tiny completion to `model`, or the
/// model list without one) and explains the answer. A provider that needs a key and has none
/// is reported as [`Outcome::NoKey`] without sending anything. Blocking.
pub fn test_connection(config: &ProviderConfig, model: Option<&str>) -> Diagnosis {
    let key = config.api_key.as_ref();
    let nothing_sent = |outcome, message: Option<String>| Diagnosis {
        outcome,
        probe: None,
        status: None,
        message,
        model: None,
        latency_ms: 0,
    };
    if key.is_none() && presets::needs_key(config.kind, &config.base_url) {
        return nothing_sent(Outcome::NoKey, None);
    }
    let probe = match build_probe(config, model) {
        Ok(p) => p,
        Err(message) => return nothing_sent(Outcome::Failed, Some(redact(&message, key))),
    };
    let http = Http::new(config);
    let headers: Vec<(&str, &str)> = probe
        .headers
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    let started = Instant::now();
    let answer = http.exchange(probe.post, &probe.path, &headers, probe.body.as_ref());
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut d = match answer {
        Ok((status, body)) => {
            let outcome = classify(config.kind, probe.kind, status, &body);
            let keep_body = !matches!(outcome, Outcome::WrongKey) && outcome != Outcome::Ok;
            Diagnosis {
                outcome,
                probe: Some(probe.kind),
                status: Some(status),
                message: keep_body.then(|| short(&error_message(&body))),
                model: (outcome == Outcome::Ok && probe.kind == ProbeKind::Chat)
                    .then(|| answered_model(&body))
                    .flatten(),
                latency_ms,
            }
        }
        Err(e) => Diagnosis {
            outcome: transport_outcome(&e),
            probe: Some(probe.kind),
            status: None,
            message: Some(short(&e.to_string())),
            model: None,
            latency_ms,
        },
    };
    d.message = d
        .message
        .map(|m| redact(&m, key))
        .filter(|m| !m.trim().is_empty());
    d
}

fn short(text: &str) -> String {
    text.trim().chars().take(MAX_MESSAGE_CHARS).collect()
}

/// `model` of a successful chat answer.
fn answered_model(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    v.get("model").and_then(Value::as_str).map(str::to_owned)
}

/// A transport failure (no HTTP answer) → outcome.
fn transport_outcome(e: &ureq::Error) -> Outcome {
    match e {
        ureq::Error::Tls(_)
        | ureq::Error::Rustls(_)
        | ureq::Error::Pem(_)
        | ureq::Error::TlsRequired => Outcome::Tls,
        ureq::Error::Io(io) => {
            let text = io.to_string().to_ascii_lowercase();
            // ureq hands rustls failures over as I/O errors: recognise them by their text
            // ("invalid peer certificate: UnknownIssuer", "received corrupt message …").
            let tls_words = [
                "certificate",
                "tls",
                "handshake",
                "ssl",
                "corrupt message",
                "alert",
            ];
            if tls_words.iter().any(|w| text.contains(w)) {
                Outcome::Tls
            } else {
                Outcome::CannotReach
            }
        }
        ureq::Error::Timeout(_)
        | ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed
        | ureq::Error::ConnectProxyFailed(_) => Outcome::CannotReach,
        ureq::Error::BadUri(_) => Outcome::WrongAddress,
        _ => Outcome::Failed,
    }
}

fn any(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| text.contains(w))
}

/// The body names an empty balance / exhausted paid quota.
fn mentions_credit(text: &str) -> bool {
    any(
        text,
        &[
            "insufficient_quota",
            "credit_balance",
            "billing_error",
            "insufficient balance",
            "insufficient_balance",
            "insufficient credits",
            "credit balance",
            "not enough credits",
            "exceeded your current quota",
            "billing",
            "payment required",
            "add credits",
            "top up",
        ],
    )
}

/// The body says the model is unknown or not available to this key.
fn mentions_missing_model(text: &str) -> bool {
    text.contains("model")
        && any(
            text,
            &[
                "not found",
                "not_found",
                "does not exist",
                "not exist",
                "invalid model",
                "not a valid model",
                "unknown model",
                "no such model",
                "model_not_found",
                "not supported",
                "is not available",
                "decommissioned",
                "try pulling it",
            ],
        )
}

/// The body says the key is wrong (Google answers a bad key with `400`).
fn mentions_bad_key(text: &str) -> bool {
    any(
        text,
        &[
            "api key not valid",
            "api_key_invalid",
            "invalid api key",
            "invalid_api_key",
            "incorrect api key",
            "invalid x-api-key",
            "authentication",
        ],
    )
}

/// Whether a 2xx body is the answer this API gives (and not, say, a web page at a wrong
/// address).
fn looks_like_api(kind: ProviderKind, probe: ProbeKind, body: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    match (kind, probe) {
        (ProviderKind::Anthropic, ProbeKind::Chat) => v.get("content").is_some_and(Value::is_array),
        (ProviderKind::Openai | ProviderKind::OpenaiCompatible, ProbeKind::Chat) => {
            v.get("choices").is_some_and(Value::is_array)
        }
        (ProviderKind::Ollama, ProbeKind::Chat) => v.get("message").is_some_and(Value::is_object),
        (ProviderKind::Ollama, ProbeKind::Models) => v.get("models").is_some_and(Value::is_array),
        // Mistral's docs show a bare array as well as `{"data": [...]}`.
        (_, ProbeKind::Models) => v.get("data").is_some_and(Value::is_array) || v.is_array(),
    }
}

/// An HTTP answer to a probe → outcome. Pure (the unit tests cover each vendor's known error
/// shapes).
pub fn classify(kind: ProviderKind, probe: ProbeKind, status: u16, body: &str) -> Outcome {
    let text = body.to_ascii_lowercase();
    // OpenRouter can report a failure inside a 200 answer: `{"error": {"code": 402, …}}`.
    if (200..300).contains(&status) {
        let embedded = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|v| v.pointer("/error/code").and_then(Value::as_u64))
            .and_then(|c| u16::try_from(c).ok())
            .filter(|c| !(200..300).contains(c));
        if let Some(code) = embedded {
            return classify(kind, probe, code, body);
        }
    }
    match status {
        200..=299 if looks_like_api(kind, probe, body) => Outcome::Ok,
        200..=299 => Outcome::WrongAddress,
        402 => Outcome::NoCredit,
        401 => Outcome::WrongKey,
        403 if mentions_credit(&text) => Outcome::NoCredit,
        403 => Outcome::WrongKey,
        404 if mentions_missing_model(&text) => Outcome::ModelNotFound,
        // A JSON 404 from the chat endpoint names a missing resource — the model (Anthropic:
        // `not_found_error` "model: …"); an HTML/empty 404 is a wrong address.
        404 if probe == ProbeKind::Chat && text.contains("model") => Outcome::ModelNotFound,
        404 | 405 => Outcome::WrongAddress,
        400 | 422 if mentions_missing_model(&text) => Outcome::ModelNotFound,
        400 | 422 if mentions_credit(&text) => Outcome::NoCredit,
        400 | 422 if mentions_bad_key(&text) => Outcome::WrongKey,
        429 if mentions_credit(&text) => Outcome::NoCredit,
        408 | 429 => Outcome::RateLimited,
        500..=599 => Outcome::ServiceDown,
        _ => Outcome::Failed,
    }
}
