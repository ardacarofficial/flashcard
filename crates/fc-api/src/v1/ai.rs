//! AI providers and features (v0.8, docs/06 §1).
//!
//! Flow of every AI request: `ai.preview` builds the exact request (provider, model, system
//! prompt, messages, token/cost estimate) and a `preview_hash` → the user sees it → `ai.run`
//! with that hash rebuilds the request, refuses if anything changed
//! (`CONFLICT_AI_PREVIEW_STALE`) and starts an `ai.request` job → the job's result is an
//! [`AiJobResult`]. Applying a result is a normal, undoable command (`ai.notes.add`,
//! `notes.update`). API keys go only to the OS keychain (`ai.providers.set_key`) and are never
//! returned, logged, stored or put in events.

use super::attributes::AttributeAssignment;
use fc_core::{AiProviderId, CardId, DeckId, NoteId, NoteTypeId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Wire protocol of a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    /// Anthropic Claude.
    Anthropic,
    /// OpenAI.
    Openai,
    /// Any OpenAI-compatible endpoint (OpenRouter, LM Studio, …).
    OpenaiCompatible,
    /// Ollama (local).
    Ollama,
}

/// An AI feature (docs/06 §1.4). Each can use its own provider/model (`ai.routes.set`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiFeature {
    /// Cards from a text/image source.
    GenerateCards,
    /// Fill the empty fields of a note being written (meaning, example sentence, …).
    FillFields,
    /// Explain a card's topic while studying.
    Explain,
    /// A hint that does not give the answer away.
    Hint,
    /// Meaning-based check of a typed answer.
    CheckAnswer,
    /// Clearer wording for a note (shown as a diff).
    Improve,
    /// A new angle + mnemonic for a leech.
    RewriteLeech,
    /// Plausible wrong options for multiple choice.
    Distractors,
}

impl AiFeature {
    /// Every feature, in menu order.
    pub const ALL: [AiFeature; 8] = [
        AiFeature::GenerateCards,
        AiFeature::FillFields,
        AiFeature::Explain,
        AiFeature::Hint,
        AiFeature::CheckAnswer,
        AiFeature::Improve,
        AiFeature::RewriteLeech,
        AiFeature::Distractors,
    ];

    /// Snake-case id (`generate_cards`).
    pub fn id(self) -> &'static str {
        match self {
            Self::GenerateCards => "generate_cards",
            Self::FillFields => "fill_fields",
            Self::Explain => "explain",
            Self::Hint => "hint",
            Self::CheckAnswer => "check_answer",
            Self::Improve => "improve",
            Self::RewriteLeech => "rewrite_leech",
            Self::Distractors => "distractors",
        }
    }
}

// ------------------------------------------------------------------ providers

/// A configured provider. The key itself is never returned — only whether one is set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AiProviderDto {
    /// Id.
    pub id: AiProviderId,
    /// Protocol.
    pub kind: AiProviderKind,
    /// Display name.
    pub name: String,
    /// Endpoint base URL.
    pub base_url: String,
    /// Model used when a feature names none (`""` = none chosen).
    pub default_model: String,
    /// Known models (refreshed from the provider or typed in).
    pub models: Vec<String>,
    /// The default provider.
    pub is_default: bool,
    /// A key is stored in the OS keychain.
    pub has_key: bool,
    /// Requests need a key.
    pub needs_key: bool,
    /// Runs on this computer (nothing leaves it).
    pub local: bool,
    /// Created.
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
}

/// What a new provider of a kind starts with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AiProviderKindInfo {
    /// Kind.
    pub kind: AiProviderKind,
    /// Suggested base URL.
    pub default_base_url: String,
    /// Suggested default model (`""` = pick after refreshing the model list).
    pub default_model: String,
    /// Known model ids to offer before the list is fetched.
    pub suggested_models: Vec<String>,
    /// Requests need a key.
    pub needs_key: bool,
    /// Local by default.
    pub local: bool,
}

/// Response of `ai.providers.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AiProviderList {
    /// Live providers, in order.
    pub providers: Vec<AiProviderDto>,
    /// Kinds that can be added (Claude first).
    pub kinds: Vec<AiProviderKindInfo>,
    /// Whether API keys can be stored on this system (an OS keychain is available).
    pub keychain: bool,
}

/// `ai.providers.create`. Missing values come from the kind's defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct CreateAiProvider {
    /// Kind.
    pub kind: AiProviderKind,
    /// Name (`null` = the kind's name).
    #[serde(default)]
    pub name: Option<String>,
    /// Base URL (`null` = the kind's default).
    #[serde(default)]
    pub base_url: Option<String>,
    /// Default model (`null` = the kind's default).
    #[serde(default)]
    pub default_model: Option<String>,
    /// Make it the default provider (the first provider always is).
    #[serde(default)]
    pub is_default: Option<bool>,
}

/// `ai.providers.update`: replaces the editable columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateAiProvider {
    /// Id.
    pub id: AiProviderId,
    /// Name.
    pub name: String,
    /// Base URL.
    pub base_url: String,
    /// Default model.
    pub default_model: String,
    /// Known models (typed in by hand, or the fetched list edited).
    pub models: Vec<String>,
    /// Make it the default (`false` on the default provider is ignored: pick another one).
    pub is_default: bool,
}

/// A provider by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiProviderRef {
    /// Id.
    pub id: AiProviderId,
}

/// `ai.providers.set_key`: stores (or with `null` removes) the provider's API key in the OS
/// keychain. Not an undo step and not in the oplog: the key never reaches the database.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetAiProviderKey {
    /// Provider.
    pub provider_id: AiProviderId,
    /// The key (`null` = remove).
    pub key: Option<String>,
}

impl std::fmt::Debug for SetAiProviderKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetAiProviderKey")
            .field("provider_id", &self.provider_id)
            .field("key", &self.key.as_ref().map(|_| "***"))
            .finish()
    }
}

/// A provider row exactly as stored (inverse of provider commands).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiProviderSnapshot {
    /// Id.
    pub id: AiProviderId,
    /// Kind.
    pub kind: AiProviderKind,
    /// Name.
    pub name: String,
    /// Base URL.
    pub base_url: String,
    /// Default model.
    pub default_model: String,
    /// Models.
    pub models: Vec<String>,
    /// Default flag.
    pub is_default: bool,
    /// Position.
    pub position: i32,
    /// Created.
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
    /// Trashed.
    pub deleted_at: Option<Timestamp>,
}

/// Internal `ai.providers.restore`: writes the rows back exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreAiProviders {
    /// Rows.
    pub rows: Vec<AiProviderSnapshot>,
}

/// Internal `ai.providers.set_models`: the model list fetched by a refresh job (a cache: not
/// an undo step).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetAiModels {
    /// Provider.
    pub id: AiProviderId,
    /// Model ids.
    pub models: Vec<String>,
}

// ------------------------------------------------------------------ routes & prices

/// Which provider/model a feature uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiRouteDto {
    /// Feature.
    pub feature: AiFeature,
    /// Provider (`null` = the default provider).
    pub provider_id: Option<AiProviderId>,
    /// Model (`null` = the provider's default model).
    pub model: Option<String>,
}

/// Response of `ai.routes.list`: one entry per feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AiRouteList {
    /// Routes, in [`AiFeature::ALL`] order.
    pub routes: Vec<AiRouteDto>,
}

/// One price row (USD per million tokens).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiPriceDto {
    /// Model id (exact, or a prefix of dated ids: `claude-sonnet-5-5` prices
    /// `claude-sonnet-5-5-20261001`).
    pub model: String,
    /// Input price.
    pub input_per_mtok: f64,
    /// Output price.
    pub output_per_mtok: f64,
}

/// Response of `ai.prices.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AiPriceList {
    /// Prices, by model.
    pub prices: Vec<AiPriceDto>,
}

/// A price by model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiPriceRef {
    /// Model.
    pub model: String,
}

// ------------------------------------------------------------------ usage

/// Totals over a period.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct AiUsageTotals {
    /// Requests sent.
    pub requests: u32,
    /// Input tokens.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Estimated cost (USD).
    pub cost_usd: f64,
}

/// Totals of one feature or model this month.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AiUsageGroup {
    /// Feature id or model id.
    pub key: String,
    /// Totals.
    pub totals: AiUsageTotals,
}

/// Monthly budget state.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct AiBudgetStatus {
    /// Spent this calendar month (UTC), USD.
    pub month_spent_usd: f64,
    /// Budget (`null` = none; setting `ai.monthly_budget_usd`).
    pub limit_usd: Option<f64>,
    /// At or over 80 % of the budget.
    pub warning: bool,
    /// Over the budget.
    pub exceeded: bool,
}

/// Response of `ai.usage.summary`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AiUsageSummary {
    /// Start of this calendar month (UTC).
    pub month_start: Timestamp,
    /// This month.
    pub month: AiUsageTotals,
    /// Last 7 days.
    pub week: AiUsageTotals,
    /// This month by feature, most expensive first.
    pub by_feature: Vec<AiUsageGroup>,
    /// This month by model, most expensive first.
    pub by_model: Vec<AiUsageGroup>,
    /// Budget.
    pub budget: AiBudgetStatus,
}

/// Internal `ai.usage.record`: one provider call (a ledger row, not an undo step).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RecordAiUsage {
    /// Provider.
    pub provider_id: AiProviderId,
    /// Model that answered.
    pub model: String,
    /// Feature.
    pub feature: AiFeature,
    /// Input tokens.
    pub input_tokens: u32,
    /// Output tokens.
    pub output_tokens: u32,
}

// ------------------------------------------------------------------ requests

/// What to ask the AI. Context (note fields, deck classifications…) is read by the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "feature", rename_all = "snake_case", deny_unknown_fields)]
pub enum AiTask {
    /// Cards from a source text (and/or images). Level-aware: the deck's classifications
    /// (`attributes`, defaults filled in) go into the prompt and onto the generated notes.
    GenerateCards {
        /// Deck the notes will go to.
        deck_id: DeckId,
        /// Note type the notes will have (its fields shape the output).
        notetype_id: NoteTypeId,
        /// Source text.
        source: String,
        /// How many notes to ask for (1–50).
        count: u32,
        /// Extra instructions ("focus on verbs").
        #[serde(default)]
        instructions: Option<String>,
        /// Images (media hashes; the provider must support images).
        #[serde(default)]
        images: Vec<String>,
        /// Classification values of the new notes (omitted = their defaults).
        #[serde(default)]
        attributes: Vec<AttributeAssignment>,
    },
    /// Fill the empty fields of a note being written. Level-aware like `generate_cards`.
    FillFields {
        /// Deck.
        deck_id: DeckId,
        /// Note type.
        notetype_id: NoteTypeId,
        /// Fields typed so far (field key → value).
        fields: BTreeMap<String, String>,
        /// Classification values.
        #[serde(default)]
        attributes: Vec<AttributeAssignment>,
    },
    /// Explain a card's topic.
    Explain {
        /// Card.
        card_id: CardId,
        /// A follow-up question.
        #[serde(default)]
        question: Option<String>,
    },
    /// A hint that does not reveal the answer.
    Hint {
        /// Card.
        card_id: CardId,
    },
    /// Meaning-based check of a typed answer.
    CheckAnswer {
        /// Card.
        card_id: CardId,
        /// What the user typed.
        given: String,
    },
    /// A clearer version of a note.
    Improve {
        /// Note.
        note_id: NoteId,
    },
    /// A new angle and a mnemonic for a leech.
    RewriteLeech {
        /// Note.
        note_id: NoteId,
    },
    /// Wrong options for a multiple-choice note.
    Distractors {
        /// Note.
        note_id: NoteId,
        /// How many (1–10).
        count: u32,
    },
}

/// `ai.preview`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct PreviewAi {
    /// What to ask.
    pub task: AiTask,
    /// Provider (`null` = the feature's route, else the default provider).
    #[serde(default)]
    pub provider_id: Option<AiProviderId>,
    /// Model (`null` = the route's, else the provider's default model).
    #[serde(default)]
    pub model: Option<String>,
}

/// `ai.run`: the previewed request, confirmed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct RunAi {
    /// Same as previewed.
    pub task: AiTask,
    /// Same as previewed.
    #[serde(default)]
    pub provider_id: Option<AiProviderId>,
    /// Same as previewed.
    #[serde(default)]
    pub model: Option<String>,
    /// [`AiPreview::preview_hash`] the user saw.
    pub preview_hash: String,
}

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiRole {
    /// Sent by the app.
    User,
    /// Model output.
    Assistant,
}

/// One message as it will be sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AiPreviewMessage {
    /// Role.
    pub role: AiRole,
    /// Text exactly as sent.
    pub text: String,
    /// Images sent with it (media hashes).
    pub images: Vec<String>,
}

/// Response of `ai.preview`: exactly what `ai.run` will send.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AiPreview {
    /// Feature.
    pub feature: AiFeature,
    /// Provider.
    pub provider: AiProviderDto,
    /// Model.
    pub model: String,
    /// System prompt.
    pub system: String,
    /// Messages.
    pub messages: Vec<AiPreviewMessage>,
    /// Output token cap.
    pub max_tokens: u32,
    /// The answer must follow a JSON schema.
    pub structured: bool,
    /// Rough input size (≈ characters / 4, plus images).
    pub estimated_input_tokens: u32,
    /// Upper bound of the cost (input estimate + `max_tokens` output); `null` = no price for
    /// the model (local models: free).
    pub estimated_cost_usd: Option<f64>,
    /// Number of items whose content is sent (notes, images…), for "these 3 cards will be
    /// sent".
    pub items: u32,
    /// Send this back to `ai.run`.
    pub preview_hash: String,
    /// The provider needs a key and none is set (`ai.run` would fail).
    pub missing_key: bool,
    /// Budget state.
    pub budget: AiBudgetStatus,
}

// ------------------------------------------------------------------ results

/// A generated note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiNoteDraft {
    /// Field key → value.
    pub fields: BTreeMap<String, String>,
}

/// Verdict of a meaning-based answer check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiVerdict {
    /// Right (same meaning).
    Correct,
    /// Nearly right.
    Almost,
    /// Wrong.
    Wrong,
}

/// One changed field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AiFieldChange {
    /// Field key.
    pub key: String,
    /// Current value.
    pub before: String,
    /// Suggested value.
    pub after: String,
}

/// What a feature returned (validated against its schema).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "feature", rename_all = "snake_case")]
pub enum AiOutput {
    /// Notes to review and add with `ai.notes.add`.
    GenerateCards {
        /// Target deck.
        deck_id: DeckId,
        /// Note type.
        notetype_id: NoteTypeId,
        /// Drafts.
        notes: Vec<AiNoteDraft>,
        /// Classification values to give them (the level the prompt asked for).
        attributes: Vec<AttributeAssignment>,
    },
    /// Values for the fields that were empty.
    FillFields {
        /// Field key → value (only previously empty fields).
        fields: BTreeMap<String, String>,
    },
    /// Explanation (plain text).
    Explain {
        /// Text.
        text: String,
    },
    /// Hint (plain text).
    Hint {
        /// Text.
        text: String,
    },
    /// Answer check.
    CheckAnswer {
        /// Verdict.
        verdict: AiVerdict,
        /// Short reason.
        feedback: String,
    },
    /// Suggested rewording (apply with `notes.update`).
    Improve {
        /// Note.
        note_id: NoteId,
        /// Changed fields only.
        changes: Vec<AiFieldChange>,
        /// Why.
        reason: String,
    },
    /// New wording + mnemonic for a leech.
    RewriteLeech {
        /// Note.
        note_id: NoteId,
        /// Changed fields only.
        changes: Vec<AiFieldChange>,
        /// Memory aid.
        mnemonic: String,
    },
    /// Wrong options.
    Distractors {
        /// Note.
        note_id: NoteId,
        /// Options (distinct, none equal to the answer).
        items: Vec<String>,
        /// The note's field that holds wrong options, one per line (`distractors` of the
        /// multiple-choice type); `null` = the note type has none.
        field: Option<String>,
    },
}

/// Result of an `ai.request` job (`JobDto.result`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AiJobResult {
    /// The validated output.
    pub output: AiOutput,
    /// Provider used.
    pub provider_id: AiProviderId,
    /// Model that answered.
    pub model: String,
    /// Tokens over all attempts (a retry or a repair is a second request).
    pub input_tokens: u32,
    /// Output tokens over all attempts.
    pub output_tokens: u32,
    /// Estimated cost over all attempts (`null` = no price).
    pub cost_usd: Option<f64>,
    /// Requests sent (1 + retries + repair).
    pub attempts: u32,
    /// The answer hit `max_tokens` (fewer notes than asked, a cut-off text).
    pub truncated: bool,
}

/// `ai.notes.add`: adds reviewed drafts as notes — one undo step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AddAiNotes {
    /// Deck.
    pub deck_id: DeckId,
    /// Note type.
    pub notetype_id: NoteTypeId,
    /// Drafts (empty ones are skipped).
    pub notes: Vec<AiNoteDraft>,
    /// Classification values for every note.
    #[serde(default)]
    pub attributes: Vec<AttributeAssignment>,
    /// Tags for every note.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Response of `ai.notes.add`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AiNotesAdded {
    /// Created notes.
    pub note_ids: Vec<NoteId>,
    /// Drafts skipped because an equal note exists (or they were empty).
    pub skipped: u32,
}

/// `ai.notes.update`: applies a suggestion — sets the listed fields of a note and keeps
/// everything else (other fields, tags, classifications). One undo step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateAiNote {
    /// Note.
    pub note_id: NoteId,
    /// Field key → new value.
    pub fields: BTreeMap<String, String>,
}
