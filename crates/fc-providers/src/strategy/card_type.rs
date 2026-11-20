use super::StrategyResult;
use crate::provider::ProviderInfo;
use fc_core::card::CardSpec;
use fc_core::note::Note;
use fc_core::notetype::{CardTemplate, NoteType};
use fc_core::NoteId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What a card is rendered from. `template_key` is the card's own key (`forward`, `c2`, `m3`);
/// `template` is the note type template that produced it (for cloze and image occlusion the
/// single template shared by all cards).
#[derive(Debug, Clone, Copy)]
pub struct CardInput<'a> {
    /// The note.
    pub note: &'a Note,
    /// Its note type.
    pub notetype: &'a NoteType,
    /// The producing template.
    pub template: &'a CardTemplate,
    /// The card's template key.
    pub template_key: &'a str,
}

/// Everything an engine may need from outside the note, resolved by the app beforehand
/// (engines are pure: no storage, clock or randomness of their own).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RenderContext {
    /// Deck path (`{{Deck}}`).
    pub deck_path: String,
    /// UI locale.
    pub locale: fc_core::i18n::Locale,
    /// `[[link]]` targets that exist: normalized key → note id.
    pub links: BTreeMap<String, NoteId>,
    /// Answers of other notes (same note type and deck) for automatic distractors.
    pub distractor_pool: Vec<String>,
    /// Seed for deterministic shuffles (derived from the card id: the same card always
    /// shows its choices in the same order).
    pub seed: u64,
}

/// A TTS request found in a template (`{{tts:en:field}}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtsRequest {
    /// BCP-47 language (empty = unspecified).
    pub lang: String,
    /// Text to speak.
    pub text: String,
}

/// How the user answers the card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Interaction {
    /// Reveal, then grade yourself.
    SelfGrade,
    /// Type the answer; `expected` is plain text checked by an `AnswerChecker`.
    Type {
        /// Expected answer.
        expected: String,
    },
    /// Pick one option.
    Choice {
        /// Options (HTML), already shuffled.
        options: Vec<String>,
        /// Index of the right option.
        correct: usize,
    },
}

/// A rendered card. HTML is **unsanitized** engine output; the app sanitizes `question`,
/// `answer` and choice options before they leave the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderedCard {
    /// Question HTML.
    pub question: String,
    /// Answer HTML.
    pub answer: String,
    /// Note type CSS.
    pub css: String,
    /// Answer mode.
    pub interaction: Interaction,
    /// Text to speak, in order of appearance.
    pub tts: Vec<TtsRequest>,
}

/// What the user did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CardResponse {
    /// Revealed the answer (self-graded cards).
    Revealed,
    /// Typed text.
    Text(String),
    /// Picked option index.
    Choice(usize),
}

/// The engine's judgement of a response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Evaluation {
    /// The user grades themselves.
    SelfGraded,
    /// The engine decided.
    Decided {
        /// Correct?
        correct: bool,
    },
    /// Typed text to be judged by the configured `AnswerChecker` (v0.2).
    CheckText {
        /// Expected answer.
        expected: String,
        /// Given answer.
        given: String,
    },
}

/// A card type engine (docs/02 §4). Its three jobs: **generate** cards from a note, **render**
/// a card, **evaluate** an answer. Adding a card type = implementing this trait and
/// registering it (`fc_app::Registry::card_type`); no other code changes (docs/04 v0.3).
pub trait CardTypeEngine: Send + Sync {
    /// Metadata; `info().id` is what `CardTemplate::card_type` refers to.
    fn info(&self) -> ProviderInfo;

    /// Engine-specific checks of a note type that uses it in `template` (required fields…).
    fn validate(&self, _notetype: &NoteType, _template: &CardTemplate) -> StrategyResult<()> {
        Ok(())
    }

    /// Which cards `template` yields for `note` (e.g. one per template, one per cloze number,
    /// none when the question would be empty).
    fn generate_cards(
        &self,
        note: &Note,
        notetype: &NoteType,
        template: &CardTemplate,
    ) -> StrategyResult<Vec<CardSpec>>;

    /// Renders one card.
    fn render(&self, input: &CardInput<'_>, ctx: &RenderContext) -> StrategyResult<RenderedCard>;

    /// Judges a response to a rendered card. The default covers the built-in interactions.
    fn evaluate(&self, rendered: &RenderedCard, response: &CardResponse) -> Evaluation {
        match (&rendered.interaction, response) {
            (Interaction::Choice { correct, .. }, CardResponse::Choice(i)) => Evaluation::Decided {
                correct: i == correct,
            },
            (Interaction::Type { expected }, CardResponse::Text(given)) => Evaluation::CheckText {
                expected: expected.clone(),
                given: given.clone(),
            },
            _ => Evaluation::SelfGraded,
        }
    }
}
