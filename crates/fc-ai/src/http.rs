//! Shared HTTP plumbing: one blocking JSON call with status → [`ProviderError`] mapping and
//! secret scrubbing.

use crate::secrets::Secret;
use crate::ProviderConfig;
use base64::Engine;
use fc_providers::ai::ContentPart;
use fc_providers::{ProviderError, ProviderErrorKind, ProviderResult};
use serde_json::Value;

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

    fn finish(&self, mut response: ureq::http::Response<ureq::Body>) -> ProviderResult<Value> {
        let status = response.status().as_u16();
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
        Err(self.status_error(status, &text))
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

/// The human-readable part of a provider error body: `error.message` (Anthropic, OpenAI),
/// `error` (Ollama), else the body itself — truncated.
fn error_message(body: &str) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
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
