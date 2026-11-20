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
}

#[async_trait]
impl AiProvider for Anthropic {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.config.kind.provider_id().into(),
            name_key: "ai.kind.anthropic".into(),
            capabilities: vec![
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
        let v = self.http.post(
            "/v1/messages",
            &[("x-api-key", key), ("anthropic-version", VERSION)],
            &body,
        )?;
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
                .unwrap_or(model_of(&request)?)
                .to_owned(),
            text,
            usage: Usage {
                input_tokens: u32_at(&v, "/usage/input_tokens"),
                output_tokens: u32_at(&v, "/usage/output_tokens"),
            },
            truncated: v.get("stop_reason").and_then(Value::as_str) == Some("max_tokens"),
        })
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
