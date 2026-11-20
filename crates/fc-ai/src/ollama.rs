//! Ollama (local) — native API (`POST /api/chat`, `GET /api/tags`). Structured output via
//! `format: <JSON schema>`.

use crate::http::{image_base64, model_of, u32_at, Http};
use crate::ProviderConfig;
use async_trait::async_trait;
use fc_providers::ai::{AiProvider, CompletionRequest, CompletionResponse, ContentPart, Usage};
use fc_providers::{Capability, ProviderInfo, ProviderResult};
use serde_json::{json, Value};

pub(crate) struct Ollama {
    http: Http,
    config: ProviderConfig,
}

impl Ollama {
    pub(crate) fn new(config: ProviderConfig) -> Self {
        Self {
            http: Http::new(&config),
            config,
        }
    }

    fn auth(&self) -> Option<String> {
        // Plain Ollama has no auth; a key is sent only if the user put one behind a proxy.
        self.http.key().map(|k| format!("Bearer {}", k.expose()))
    }

    fn body(&self, request: &CompletionRequest) -> ProviderResult<Value> {
        let mut messages = Vec::new();
        if let Some(system) = request.system.as_deref().filter(|s| !s.is_empty()) {
            messages.push(json!({"role": "system", "content": system}));
        }
        for m in &request.messages {
            let mut text = String::new();
            let mut images = Vec::new();
            for part in &m.content {
                match part {
                    ContentPart::Text { text: t } => {
                        if !text.is_empty() {
                            text.push_str("\n\n");
                        }
                        text.push_str(t);
                    }
                    image => {
                        if let Some((_, data)) = image_base64(self.config.media.as_deref(), image)?
                        {
                            images.push(data);
                        }
                    }
                }
            }
            let mut msg = json!({"role": m.role, "content": text});
            if !images.is_empty() {
                msg["images"] = json!(images);
            }
            messages.push(msg);
        }
        let mut options = json!({"num_predict": request.max_tokens});
        if let Some(t) = request.temperature {
            options["temperature"] = json!(t);
        }
        let mut body = json!({
            "model": model_of(request)?,
            "messages": messages,
            "stream": false,
            "options": options,
        });
        if let Some(schema) = &request.json_schema {
            body["format"] = schema.clone();
        }
        Ok(body)
    }
}

impl Ollama {
    /// A `/api/chat` response → [`CompletionResponse`].
    fn response(v: &Value, request: &CompletionRequest) -> ProviderResult<CompletionResponse> {
        Ok(CompletionResponse {
            model: v
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(model_of(request)?)
                .to_owned(),
            text: v
                .pointer("/message/content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            usage: Usage {
                input_tokens: u32_at(v, "/prompt_eval_count"),
                output_tokens: u32_at(v, "/eval_count"),
            },
            truncated: v.get("done_reason").and_then(Value::as_str) == Some("length"),
        })
    }
}

#[async_trait]
impl AiProvider for Ollama {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.config.kind.provider_id().into(),
            name_key: "ai.kind.ollama".into(),
            capabilities: vec![
                Capability::Streaming,
                Capability::StructuredOutput,
                Capability::Vision,
            ],
            settings: vec![],
            secret_keys: vec![],
            local: true,
        }
    }

    async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse> {
        let auth = self.auth();
        let headers: Vec<(&str, &str)> = auth
            .as_deref()
            .map(|a| vec![("authorization", a)])
            .unwrap_or_default();
        let v = self
            .http
            .post("/api/chat", &headers, &self.body(&request)?)?;
        Self::response(&v, &request)
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
        let auth = self.auth();
        let headers: Vec<(&str, &str)> = auth
            .as_deref()
            .map(|a| vec![("authorization", a)])
            .unwrap_or_default();
        let mut body = self.body(&request)?;
        body["stream"] = json!(true);
        let mut out = CompletionResponse {
            model: model_of(&request)?.to_owned(),
            text: String::new(),
            usage: Usage::default(),
            truncated: false,
        };
        let mut failure = None;
        // NDJSON: one object per line, the last one with `done: true` and the counts.
        let whole = self
            .http
            .post_stream("/api/chat", &headers, &body, &mut |line| {
                if line.trim().is_empty() {
                    return Ok(true);
                }
                let Ok(v) = serde_json::from_str::<Value>(line) else {
                    return Ok(true);
                };
                if let Some(e) = v.get("error").and_then(Value::as_str) {
                    failure = Some(self.http.failed(format!(
                        "builtin.ai.ollama: {}",
                        e.chars().take(300).collect::<String>()
                    )));
                    return Ok(false);
                }
                if let Some(m) = v.get("model").and_then(Value::as_str) {
                    out.model = m.to_owned();
                }
                if let Some(t) = v.pointer("/message/content").and_then(Value::as_str) {
                    if !t.is_empty() {
                        out.text.push_str(t);
                        on_text(t);
                    }
                }
                if v.get("done").and_then(Value::as_bool) == Some(true) {
                    out.usage = Usage {
                        input_tokens: u32_at(&v, "/prompt_eval_count"),
                        output_tokens: u32_at(&v, "/eval_count"),
                    };
                    out.truncated = v.get("done_reason").and_then(Value::as_str) == Some("length");
                    return Ok(false);
                }
                Ok(true)
            })?;
        if let Some(v) = whole {
            let response = Self::response(&v, &request)?;
            on_text(&response.text);
            return Ok(response);
        }
        match failure {
            Some(e) => Err(e),
            None => Ok(out),
        }
    }

    async fn list_models(&self) -> ProviderResult<Vec<String>> {
        let auth = self.auth();
        let headers: Vec<(&str, &str)> = auth
            .as_deref()
            .map(|a| vec![("authorization", a)])
            .unwrap_or_default();
        let v = self.http.get("/api/tags", &headers)?;
        Ok(v.get("models")
            .and_then(Value::as_array)
            .map(|d| {
                d.iter()
                    .filter_map(|m| m.get("name").and_then(Value::as_str).map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }
}
