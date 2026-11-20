//! AI completion providers (Claude, OpenAI-compatible, Ollama… — v0.8).

use crate::provider::{ProviderInfo, ProviderResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Chat role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// End user / app prompt.
    User,
    /// Model output.
    Assistant,
}

/// One message part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Text.
    Text {
        /// Text.
        text: String,
    },
    /// Image by media hash (resolved by the app, requires `Capability::Vision`).
    Image {
        /// Media store hash.
        media_hash: String,
        /// MIME type.
        mime: String,
    },
}

/// A chat message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Role.
    pub role: Role,
    /// Content parts.
    pub content: Vec<ContentPart>,
}

/// A completion request. Content is previewed to the user before sending (docs/04 v0.8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Model id (provider-specific); `None` = provider default.
    pub model: Option<String>,
    /// System prompt.
    pub system: Option<String>,
    /// Conversation.
    pub messages: Vec<Message>,
    /// Output token cap.
    pub max_tokens: u32,
    /// Sampling temperature.
    pub temperature: Option<f32>,
    /// JSON schema the output must follow (requires `Capability::StructuredOutput`).
    pub json_schema: Option<serde_json::Value>,
    /// Feature tag for usage accounting (`ai.generate_cards`, …).
    pub feature: String,
}

/// Token usage for cost tracking (`ai_usage` table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Input tokens.
    pub input_tokens: u32,
    /// Output tokens.
    pub output_tokens: u32,
}

/// A completion result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Model that answered.
    pub model: String,
    /// Output text (JSON text when `json_schema` was set).
    pub text: String,
    /// Usage.
    pub usage: Usage,
    /// Stopped because of `max_tokens`.
    pub truncated: bool,
}

/// An AI text/structured-output provider.
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Metadata.
    fn info(&self) -> ProviderInfo;
    /// Runs a completion.
    async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse>;
    /// Runs a completion, calling `on_text` with each piece of output text as it arrives
    /// (`Capability::Streaming`). The response is the same as [`AiProvider::complete`]'s. The
    /// default implementation does not stream: it completes and hands over the whole text once.
    async fn complete_stream(
        &self,
        request: CompletionRequest,
        on_text: &(dyn for<'t> Fn(&'t str) + Send + Sync),
    ) -> ProviderResult<CompletionResponse> {
        let response = self.complete(request).await?;
        on_text(&response.text);
        Ok(response)
    }
    /// Available model ids.
    async fn list_models(&self) -> ProviderResult<Vec<String>>;
}
