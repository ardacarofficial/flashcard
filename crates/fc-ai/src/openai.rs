//! OpenAI and OpenAI-compatible endpoints — Chat Completions (`POST /chat/completions`,
//! `GET /models`; the base URL includes the version path, e.g. `https://api.openai.com/v1`).

use crate::http::{image_base64, model_of, u32_at, Http};
use crate::{ProviderConfig, ProviderKind};
use async_trait::async_trait;
use fc_providers::ai::{AiProvider, CompletionRequest, CompletionResponse, ContentPart, Usage};
use fc_providers::{Capability, ProviderInfo, ProviderResult};
use serde_json::{json, Value};

pub(crate) struct OpenAi {
    http: Http,
    config: ProviderConfig,
}

impl OpenAi {
    pub(crate) fn new(config: ProviderConfig) -> Self {
        Self {
            http: Http::new(&config),
            config,
        }
    }

    fn auth(&self) -> ProviderResult<Option<String>> {
        match self.config.kind {
            ProviderKind::Openai => Ok(Some(format!("Bearer {}", self.http.require_key()?))),
            // Local servers usually need no key; send one when the user set it (OpenRouter…).
            _ => Ok(self.http.key().map(|k| format!("Bearer {}", k.expose()))),
        }
    }

    fn body(&self, request: &CompletionRequest) -> ProviderResult<Value> {
        let mut messages = Vec::new();
        if let Some(system) = request.system.as_deref().filter(|s| !s.is_empty()) {
            messages.push(json!({"role": "system", "content": system}));
        }
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
                                "type": "image_url",
                                "image_url": {"url": format!("data:{mime};base64,{data}")},
                            }));
                        }
                    }
                }
            }
            messages.push(json!({"role": m.role, "content": content}));
        }
        let mut body = json!({"model": model_of(request)?, "messages": messages});
        // OpenAI renamed the cap; compatible servers still expect the classic name.
        let cap = match self.config.kind {
            ProviderKind::Openai => "max_completion_tokens",
            _ => "max_tokens",
        };
        body[cap] = json!(request.max_tokens);
        if let Some(t) = request.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(schema) = &request.json_schema {
            body["response_format"] = json!({
                "type": "json_schema",
                "json_schema": {"name": "result", "schema": schema, "strict": false},
            });
        }
        Ok(body)
    }

    fn headers(auth: &Option<String>) -> Vec<(&str, &str)> {
        auth.as_deref()
            .map(|a| vec![("authorization", a)])
            .unwrap_or_default()
    }
}

#[async_trait]
impl AiProvider for OpenAi {
    fn info(&self) -> ProviderInfo {
        let compatible = self.config.kind == ProviderKind::OpenaiCompatible;
        ProviderInfo {
            id: self.config.kind.provider_id().into(),
            name_key: if compatible {
                "ai.kind.openai_compatible".into()
            } else {
                "ai.kind.openai".into()
            },
            capabilities: vec![Capability::StructuredOutput, Capability::Vision],
            settings: vec![],
            secret_keys: vec!["api_key".into()],
            local: false,
        }
    }

    async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse> {
        let auth = self.auth()?;
        let body = self.body(&request)?;
        let v = self
            .http
            .post("/chat/completions", &Self::headers(&auth), &body)?;
        let choice = v
            .pointer("/choices/0")
            .ok_or_else(|| self.http.failed("response without choices"))?;
        let text = choice
            .pointer("/message/content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        Ok(CompletionResponse {
            model: v
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(model_of(&request)?)
                .to_owned(),
            text,
            usage: Usage {
                input_tokens: u32_at(&v, "/usage/prompt_tokens"),
                output_tokens: u32_at(&v, "/usage/completion_tokens"),
            },
            truncated: choice.get("finish_reason").and_then(Value::as_str) == Some("length"),
        })
    }

    async fn list_models(&self) -> ProviderResult<Vec<String>> {
        let auth = self.auth()?;
        let v = self.http.get("/models", &Self::headers(&auth))?;
        Ok(v.get("data")
            .and_then(Value::as_array)
            .map(|d| {
                d.iter()
                    .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }
}
