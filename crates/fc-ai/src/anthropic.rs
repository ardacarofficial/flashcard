//! Anthropic Claude — Messages API (`POST /v1/messages`, `GET /v1/models`).
//!
//! Structured output uses a forced tool call: the JSON schema becomes the input schema of a
//! single `emit_result` tool and `tool_choice` names it, so the reply is the tool input — the
//! long-standing, model-independent way to get schema-shaped JSON from Claude.

use crate::http::{image_base64, model_of, u32_at, Http};
use crate::ProviderConfig;
use async_trait::async_trait;
use fc_providers::ai::{AiProvider, CompletionRequest, CompletionResponse, ContentPart, Usage};
use fc_providers::{Capability, ProviderError, ProviderErrorKind, ProviderInfo, ProviderResult};
use serde_json::{json, Value};

/// API version header value.
const VERSION: &str = "2023-06-01";
/// Name of the tool that carries structured output.
const TOOL: &str = "emit_result";

pub(crate) struct Anthropic {
    http: Http,
    config: ProviderConfig,
}

impl Anthropic {
    pub(crate) fn new(config: ProviderConfig) -> Self {
        Self {
            http: Http::new(&config),
            config,
        }
    }

    fn body(&self, request: &CompletionRequest) -> ProviderResult<Value> {
        let mut messages = Vec::new();
        for m in &request.messages {
            let mut content = Vec::new();
            for part in &m.content {
                match part {
                    ContentPart::Text { text } => {
                        content.push(json!({"type": "text", "text": text}))
                    }
                    image => {
                        if let Some((mime, data)) =
                            image_base64(self.config.media.as_deref(), image)?
                        {
                            content.push(json!({
                                "type": "image",
                                "source": {"type": "base64", "media_type": mime, "data": data},
                            }));
                        }
                    }
                }
            }
            messages.push(json!({"role": m.role, "content": content}));
        }
        let mut body = json!({
            "model": model_of(request)?,
            "max_tokens": request.max_tokens,
            "messages": messages,
        });
        if let Some(system) = request.system.as_deref().filter(|s| !s.is_empty()) {
            body["system"] = json!(system);
        }
        if let Some(t) = request.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(schema) = &request.json_schema {
            body["tools"] = json!([{
                "name": TOOL,
                "description": "Return the result. Call this exactly once with the complete answer.",
                "input_schema": schema,
            }]);
            body["tool_choice"] = json!({"type": "tool", "name": TOOL});
        }
        Ok(body)
    }

    fn headers(key: &str) -> [(&str, &str); 2] {
        [("x-api-key", key), ("anthropic-version", VERSION)]
    }
}

/// Folds one Anthropic SSE `data:` payload into `out`; text deltas go to `on_text`.
fn sse_event(
    v: &Value,
    out: &mut CompletionResponse,
    on_text: &(dyn for<'t> Fn(&'t str) + Send + Sync),
) -> ProviderResult<()> {
    match v.get("type").and_then(Value::as_str) {
        Some("message_start") => {
            if let Some(m) = v.pointer("/message/model").and_then(Value::as_str) {
                out.model = m.to_owned();
            }
            out.usage.input_tokens = u32_at(v, "/message/usage/input_tokens");
            out.usage.output_tokens = u32_at(v, "/message/usage/output_tokens");
        }
        Some("content_block_delta") => {
            if let Some(t) = v.pointer("/delta/text").and_then(Value::as_str) {
                out.text.push_str(t);
                on_text(t);
            }
        }
        Some("message_delta") => {
            if let Some(n) = v.pointer("/usage/output_tokens").and_then(Value::as_u64) {
                out.usage.output_tokens = u32::try_from(n).unwrap_or(u32::MAX);
            }
            if v.pointer("/delta/stop_reason").and_then(Value::as_str) == Some("max_tokens") {
                out.truncated = true;
            }
        }
        Some("error") => {
            let kind = match v.pointer("/error/type").and_then(Value::as_str) {
                Some("overloaded_error") | Some("api_error") => ProviderErrorKind::Unavailable,
                Some("rate_limit_error") => ProviderErrorKind::RateLimited,
                Some("invalid_request_error") => ProviderErrorKind::InvalidRequest,
                _ => ProviderErrorKind::Failed,
            };
            let message = v
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("stream error");
            return Err(ProviderError::new(
                kind,
                format!(
                    "builtin.ai.anthropic: {}",
                    message.chars().take(300).collect::<String>()
                ),
            ));
        }
        _ => {}
    }
    Ok(())
}

impl Anthropic {
    /// A Messages API response → [`CompletionResponse`].
    fn response(
        &self,
        v: &Value,
        request: &CompletionRequest,
    ) -> ProviderResult<CompletionResponse> {
        let blocks = v
            .get("content")
            .and_then(Value::as_array)
            .ok_or_else(|| self.http.failed("response without content"))?;
        let text = if request.json_schema.is_some() {
            let input = blocks
                .iter()
                .find(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
                .and_then(|b| b.get("input"))
                .ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorKind::Failed,
                        "the model did not return structured output",
                    )
                })?;
            serde_json::to_string(input).map_err(|e| self.http.failed(e.to_string()))?
        } else {
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        };
        Ok(CompletionResponse {
            model: v
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(model_of(request)?)
                .to_owned(),
            text,
            usage: Usage {
                input_tokens: u32_at(v, "/usage/input_tokens"),
                output_tokens: u32_at(v, "/usage/output_tokens"),
            },
            truncated: v.get("stop_reason").and_then(Value::as_str) == Some("max_tokens"),
        })
    }
}

#[async_trait]
impl AiProvider for Anthropic {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.config.kind.provider_id().into(),
            name_key: "ai.kind.anthropic".into(),
            capabilities: vec![
                Capability::Streaming,
                Capability::StructuredOutput,
                Capability::Vision,
                Capability::Tools,
            ],
            settings: vec![],
            secret_keys: vec!["api_key".into()],
            local: false,
        }
    }

    async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse> {
        let key = self.http.require_key()?;
        let body = self.body(&request)?;
        let v = self.http.post("/v1/messages", &Self::headers(key), &body)?;
        self.response(&v, &request)
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
        on_text: &(dyn for<'t> Fn(&'t str) + Send + Sync),
    ) -> ProviderResult<CompletionResponse> {
        if request.json_schema.is_some() {
            // Structured output arrives as one tool input: nothing to show while it streams.
            let response = self.complete(request).await?;
            on_text(&response.text);
            return Ok(response);
        }
        let key = self.http.require_key()?;
        let mut body = self.body(&request)?;
        body["stream"] = json!(true);
        let mut out = CompletionResponse {
            model: model_of(&request)?.to_owned(),
            text: String::new(),
            usage: Usage::default(),
            truncated: false,
        };
        let mut failure = None;
        let whole =
            self.http
                .post_stream("/v1/messages", &Self::headers(key), &body, &mut |line| {
                    let Some(data) = line.strip_prefix("data:") else {
                        return Ok(true);
                    };
                    let Ok(v) = serde_json::from_str::<Value>(data.trim()) else {
                        return Ok(true);
                    };
                    if let Err(e) = sse_event(&v, &mut out, on_text) {
                        failure = Some(e);
                        return Ok(false);
                    }
                    Ok(v.get("type").and_then(Value::as_str) != Some("message_stop"))
                })?;
        if let Some(v) = whole {
            let response = self.response(&v, &request)?;
            on_text(&response.text);
            return Ok(response);
        }
        match failure {
            Some(e) => Err(ProviderError {
                message: crate::redact(&e.message, self.http.key()),
                ..e
            }),
            None => Ok(out),
        }
    }

    async fn list_models(&self) -> ProviderResult<Vec<String>> {
        let key = self.http.require_key()?;
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        // A handful of pages is plenty; stop rather than loop on a misbehaving server.
        for _ in 0..10 {
            let path = match &after {
                Some(id) => format!("/v1/models?limit=100&after_id={id}"),
                None => "/v1/models?limit=100".to_owned(),
            };
            let v = self
                .http
                .get(&path, &[("x-api-key", key), ("anthropic-version", VERSION)])?;
            let page: Vec<String> = v
                .get("data")
                .and_then(Value::as_array)
                .map(|d| {
                    d.iter()
                        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            after = page.last().cloned();
            out.extend(page);
            if v.get("has_more").and_then(Value::as_bool) != Some(true) || after.is_none() {
                break;
            }
        }
        Ok(out)
    }
}
