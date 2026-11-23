//! OpenAI and OpenAI-compatible endpoints — Chat Completions (`POST /chat/completions`,
//! `GET /models`; the base URL includes the version path, e.g. `https://api.openai.com/v1`).

use crate::http::{cost_at, image_base64, model_of, u32_at, Http};
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

    pub(crate) fn body(&self, request: &CompletionRequest) -> ProviderResult<Value> {
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

impl OpenAi {
    /// A Chat Completions response → [`CompletionResponse`].
    fn response(
        &self,
        v: &Value,
        request: &CompletionRequest,
    ) -> ProviderResult<CompletionResponse> {
        let choice = v
            .pointer("/choices/0")
            .ok_or_else(|| self.http.failed("response without choices"))?;
        // A structured-output refusal has `content: null` and the reason in `refusal` (v1.4):
        // it is passed on as text, so the app's validation says what happened.
        let text = choice
            .pointer("/message/content")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .or_else(|| choice.pointer("/message/refusal").and_then(Value::as_str))
            .unwrap_or_default()
            .to_owned();
        Ok(CompletionResponse {
            model: v
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(model_of(request)?)
                .to_owned(),
            text,
            usage: Usage {
                input_tokens: u32_at(v, "/usage/prompt_tokens"),
                output_tokens: u32_at(v, "/usage/completion_tokens"),
                cost_usd: cost_at(v),
            },
            truncated: choice.get("finish_reason").and_then(Value::as_str) == Some("length"),
        })
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
            capabilities: vec![
                Capability::Streaming,
                Capability::StructuredOutput,
                Capability::Vision,
            ],
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
        self.response(&v, &request)
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
        on_text: &(dyn for<'t> Fn(&'t str) + Send + Sync),
    ) -> ProviderResult<CompletionResponse> {
        if request.json_schema.is_some() {
            let response = self.complete(request).await?;
            on_text(&response.text);
            return Ok(response);
        }
        let auth = self.auth()?;
        let mut body = self.body(&request)?;
        body["stream"] = json!(true);
        if self.config.kind == ProviderKind::Openai {
            // Usage comes in a last chunk only when asked for.
            body["stream_options"] = json!({"include_usage": true});
        }
        let mut out = CompletionResponse {
            model: model_of(&request)?.to_owned(),
            text: String::new(),
            usage: Usage::default(),
            truncated: false,
        };
        let whole = self.http.post_stream(
            "/chat/completions",
            &Self::headers(&auth),
            &body,
            &mut |line| {
                let Some(data) = line.strip_prefix("data:").map(str::trim) else {
                    return Ok(true);
                };
                if data == "[DONE]" {
                    return Ok(false);
                }
                let Ok(v) = serde_json::from_str::<Value>(data) else {
                    return Ok(true);
                };
                if let Some(m) = v.get("model").and_then(Value::as_str) {
                    out.model = m.to_owned();
                }
                if let Some(t) = v
                    .pointer("/choices/0/delta/content")
                    .and_then(Value::as_str)
                {
                    if !t.is_empty() {
                        out.text.push_str(t);
                        on_text(t);
                    }
                }
                if v.pointer("/choices/0/finish_reason")
                    .and_then(Value::as_str)
                    == Some("length")
                {
                    out.truncated = true;
                }
                if v.get("usage").is_some_and(Value::is_object) {
                    out.usage = Usage {
                        input_tokens: u32_at(&v, "/usage/prompt_tokens"),
                        output_tokens: u32_at(&v, "/usage/completion_tokens"),
                        cost_usd: cost_at(&v),
                    };
                }
                Ok(true)
            },
        )?;
        if let Some(v) = whole {
            let response = self.response(&v, &request)?;
            on_text(&response.text);
            return Ok(response);
        }
        Ok(out)
    }

    async fn list_models(&self) -> ProviderResult<Vec<String>> {
        let auth = self.auth()?;
        let v = self.http.get("/models", &Self::headers(&auth))?;
        // `{"data": [...]}`; some compatible servers answer with the bare array.
        Ok(v.get("data")
            .and_then(Value::as_array)
            .or_else(|| v.as_array())
            .map(|d| {
                d.iter()
                    .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }
}
