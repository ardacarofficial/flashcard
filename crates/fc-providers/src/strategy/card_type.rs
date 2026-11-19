use super::StrategyResult;
use crate::provider::ProviderInfo;
use fc_core::card::{Card, CardSpec};
use fc_core::note::Note;
use fc_core::notetype::NoteType;
use serde::{Deserialize, Serialize};

/// Rendering context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderContext {
    /// Render the answer side too.
    pub reveal: bool,
    /// Deck path (for `{{Deck}}`).
    pub deck_path: String,
    /// UI locale.
    pub locale: fc_core::i18n::Locale,
}

/// A rendered card: sanitized HTML plus extracted interactive parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderedCard {
    /// Question HTML.
    pub question: String,
    /// Answer HTML (empty unless `reveal`).
    pub answer: String,
    /// Shared CSS.
    pub css: String,
    /// Expected typed answer (`{{type:field}}`), if any.
    pub expected_answer: Option<String>,
    /// Text to speak (`{{tts:field}}`), with BCP-47 language.
    pub tts: Vec<(String, String)>,
}

/// Card type engine (classic, reverse, cloze, multiple choice, typing, image occlusion, audio —
/// v0.3). Adding a card type = implementing this trait (docs/04 v0.3 acceptance).
pub trait CardTypeEngine: Send + Sync {
    /// Metadata; `info().id` is what `CardTemplate::card_type` refers to.
    fn info(&self) -> ProviderInfo;
    /// Which cards `note` should have.
    fn generate_cards(&self, note: &Note, notetype: &NoteType) -> StrategyResult<Vec<CardSpec>>;
    /// Renders one card.
    fn render(
        &self,
        card: &Card,
        note: &Note,
        notetype: &NoteType,
        ctx: &RenderContext,
    ) -> StrategyResult<RenderedCard>;
}
