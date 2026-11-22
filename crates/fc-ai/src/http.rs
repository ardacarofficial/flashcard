//! Shared HTTP plumbing: one blocking JSON call with status → [`ProviderError`] mapping and
//! secret scrubbing.

use crate::secrets::Secret;
use crate::ProviderConfig;
use base64::Engine;
use fc_providers::ai::ContentPart;
use fc_providers::{ProviderError, ProviderErrorKind, ProviderResult};
use serde_json::Value;
use std::io::{BufRead, BufReader};

/// Longest wait a provider's `Retry-After` is honoured up to (10 minutes).
const MAX_RETRY_AFTER_MS: u64 = 600_000;

/// Longest response body read (a generous cap: card batches are a few hundred KB at most).
const MAX_BODY: u64 = 16 * 1024 * 1024;

/// Longest provider error message kept.
const MAX_ERROR_CHARS: usize = 300;

/// Replaces `secret` — and every run of 8+ of its characters, as providers that echo a
/// partially masked key do — in `text` with `***`.
pub fn redact(text: &str, secret: Option<&Secret>) -> String {
    let Some(secret) = secret else {
        return text.to_owned();
    };
    let key: Vec<char> = secret.expose().chars().collect();
    let mut out = text.to_owned();
    if key.is_empty() {
        return out;
    }
    out = out.replace(secret.expose(), "***");
    const MIN: usize = 8;
    if key.len() >= MIN {
        // Longest windows first so a longer match is not split by a shorter one.
        for len in (MIN..key.len()).rev() {
            for start in 0..=key.len() - len {
                let window: String = key[start..start + len].iter().collect();
                if out.contains(&window) {
                    out = out.replace(&window, "***");
                }
            }
        }
    }
    out
}

/// A JSON-over-HTTP client for one provider configuration.
pub(crate) struct Http {
    agent: ureq::Agent,
    base: String,
    key: Option<Secret>,
    provider: &'static str,
}

impl Http {
    pub(crate) fn new(config: &ProviderConfig) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(config.timeout))
            .http_status_as_error(false)
            .user_agent("flashcard")
            .build()
            .into();
        Self {
            agent,
            base: config.base_url.trim_end_matches('/').to_owned(),
            key: config.api_key.clone(),
            provider: config.kind.provider_id(),
        }
    }

    pub(crate) fn key(&self) -> Option<&Secret> {
        self.key.as_ref()
    }

    /// The key, or `Unauthorized` if none is set.
    pub(crate) fn require_key(&self) -> ProviderResult<&str> {
        self.key.as_ref().map(Secret::expose).ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::Unauthorized,
                format!("{}: no API key set", self.provider),
            )
        })
    }

    /// `POST <base><path>` with a JSON body; returns the JSON response of a 2xx.
    pub(crate) fn post(
        &self,
        path: &str,
        headers: &[(&str, &str)],
        body: &Value,
    ) -> ProviderResult<Value> {
        let url = format!("{}{path}", self.base);
        let mut req = self
            .agent
            .post(&url)
            .header("content-type", "application/json");
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let body = serde_json::to_string(body).map_err(|e| self.failed(e.to_string()))?;
        let response = req.send(body.as_str()).map_err(|e| self.transport(e))?;
        self.finish(response)
    }

    /// `POST <base><path>` of a streaming request: a 2xx body is read line by line and each
    /// line goes to `on_line` as it arrives (SSE `data:` lines, NDJSON objects…). `on_line`
    /// returns `false` to stop reading. A server that ignores `stream` and answers with one
    /// `application/json` document returns it as `Some` (the caller parses it as a plain
    /// response).
    pub(crate) fn post_stream(
        &self,
        path: &str,
        headers: &[(&str, &str)],
        body: &Value,
        on_line: &mut dyn FnMut(&str) -> ProviderResult<bool>,
    ) -> ProviderResult<Option<Value>> {
        let url = format!("{}{path}", self.base);
        let mut req = self
            .agent
            .post(&url)
            .header("content-type", "application/json");
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let body = serde_json::to_string(body).map_err(|e| self.failed(e.to_string()))?;
        let response = req.send(body.as_str()).map_err(|e| self.transport(e))?;
        let json = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.to_ascii_lowercase().starts_with("application/json"));
        if !response.status().is_success() || json {
            return self.finish(response).map(Some);
        }
        let reader = response
            .into_body()
            .into_with_config()
            .limit(MAX_BODY)
            .reader();
        let mut lines = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            let n = lines
                .read_line(&mut line)
                .map_err(|e| self.transport(ureq::Error::Io(e)))?;
            if n == 0 {
                return Ok(None);
            }
            let l = line.trim_end_matches(['\r', '\n']);
            if !on_line(l)? {
                return Ok(None);
            }
        }
    }

    /// `GET <base><path>`; returns the JSON response of a 2xx.
    pub(crate) fn get(&self, path: &str, headers: &[(&str, &str)]) -> ProviderResult<Value> {
        let url = format!("{}{path}", self.base);
        let mut req = self.agent.get(&url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let response = req.call().map_err(|e| self.transport(e))?;
        self.finish(response)
    }

    /// One request whose outcome is returned raw — status and body text, or the transport
    /// failure — for the connection test, which explains failures instead of mapping them to
    /// a [`ProviderError`]. Nothing here is retried.
    pub(crate) fn exchange(
        &self,
        post: bool,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<(u16, String), ureq::Error> {
        let url = format!("{}{path}", self.base);
        let mut response = if post {
            let mut req = self
                .agent
                .post(&url)
                .header("content-type", "application/json");
            for (k, v) in headers {
                req = req.header(*k, *v);
            }
            let text = body
                .map(Value::to_string)
                .unwrap_or_else(|| "{}".to_owned());
            req.send(text.as_str())?
        } else {
            let mut req = self.agent.get(&url);
            for (k, v) in headers {
                req = req.header(*k, *v);
            }
            req.call()?
        };
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_string()?;
        Ok((status, text))
    }

    fn finish(&self, mut response: ureq::http::Response<ureq::Body>) -> ProviderResult<Value> {
        let status = response.status().as_u16();
        let retry_after = retry_after_ms(response.headers());
        let text = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_string()
            .map_err(|e| self.transport(e))?;
        if (200..300).contains(&status) {
            return serde_json::from_str(&text)
                .map_err(|e| self.failed(format!("invalid JSON response: {e}")));
        }
        Err(self
            .status_error(status, &text)
            .with_retry_after_ms(retry_after))
    }

    fn status_error(&self, status: u16, body: &str) -> ProviderError {
        let (kind, retryable) = match status {
            // The body of an auth failure is never kept: some providers echo (part of) the key.
            401 | 403 => {
                return ProviderError::new(
                    ProviderErrorKind::Unauthorized,
                    format!("{}: HTTP {status}: the API key was rejected", self.provider),
                )
            }
            404 => (ProviderErrorKind::Unavailable, false),
            408 | 429 => (ProviderErrorKind::RateLimited, true),
            400 | 413 | 422 => (ProviderErrorKind::InvalidRequest, false),
            500..=599 => (ProviderErrorKind::Unavailable, true),
            _ => (ProviderErrorKind::Failed, false),
        };
        let detail = error_message(body);
        let message = redact(
            &format!("{}: HTTP {status}: {detail}", self.provider),
            self.key.as_ref(),
        );
        ProviderError {
            kind,
            message,
            retryable,
            retry_after_ms: None,
        }
    }

    fn transport(&self, e: ureq::Error) -> ProviderError {
        let message = redact(&format!("{}: {e}", self.provider), self.key.as_ref());
        match e {
            ureq::Error::Timeout(_)
            | ureq::Error::Io(_)
            | ureq::Error::HostNotFound
            | ureq::Error::ConnectionFailed => {
                ProviderError::new(ProviderErrorKind::Unavailable, message)
            }
            _ => ProviderError::new(ProviderErrorKind::Failed, message),
        }
    }

    pub(crate) fn failed(&self, message: impl Into<String>) -> ProviderError {
        ProviderError::new(
            ProviderErrorKind::Failed,
            redact(&message.into(), self.key.as_ref()),
        )
    }
}

/// The wait a response asks for: `retry-after-ms` (OpenAI) or `retry-after` in seconds
/// (Anthropic, most servers; the HTTP-date form is ignored), capped at 10 minutes.
pub(crate) fn retry_after_ms(headers: &ureq::http::HeaderMap) -> Option<u64> {
    let number = |name: &str| -> Option<f64> {
        headers
            .get(name)?
            .to_str()
            .ok()?
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.0)
    };
    let ms = number("retry-after-ms").or_else(|| number("retry-after").map(|s| s * 1000.0))?;
    Some((ms.ceil() as u64).min(MAX_RETRY_AFTER_MS))
}

/// The human-readable part of a provider error body: `error.message` (Anthropic, OpenAI),
/// `error` (Ollama), else the body itself — truncated.
pub(crate) fn error_message(body: &str) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    // Google's endpoints wrap the error object in a one-element array.
    let parsed = match parsed {
        Some(Value::Array(mut items)) if !items.is_empty() => Some(items.swap_remove(0)),
        other => other,
    };
    let text = parsed
        .as_ref()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("error").filter(|e| e.is_string()))
                .or_else(|| v.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.trim().to_owned());
    text.chars().take(MAX_ERROR_CHARS).collect()
}

/// An image part's bytes, base64-encoded, with its MIME type.
pub(crate) fn image_base64(
    config_media: Option<&dyn crate::MediaSource>,
    part: &ContentPart,
) -> ProviderResult<Option<(String, String)>> {
    let ContentPart::Image { media_hash, mime } = part else {
        return Ok(None);
    };
    let media = config_media.ok_or_else(|| {
        ProviderError::new(
            ProviderErrorKind::InvalidRequest,
            "images are not supported here",
        )
    })?;
    let bytes = media.media_bytes(media_hash).ok_or_else(|| {
        ProviderError::new(
            ProviderErrorKind::InvalidRequest,
            format!("image {media_hash} not found"),
        )
    })?;
    Ok(Some((
        mime.clone(),
        base64::engine::general_purpose::STANDARD.encode(bytes),
    )))
}

/// Model of a request, or an `InvalidRequest` error.
pub(crate) fn model_of(request: &fc_providers::ai::CompletionRequest) -> ProviderResult<&str> {
    request
        .model
        .as_deref()
        .filter(|m| !m.trim().is_empty())
        .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidRequest, "no model selected"))
}

/// A `u32` field of a JSON object (missing/invalid = 0).
pub(crate) fn u32_at(v: &Value, pointer: &str) -> u32 {
    v.pointer(pointer)
        .and_then(Value::as_u64)
        .map(|n| u32::try_from(n).unwrap_or(u32::MAX))
        .unwrap_or(0)
}
