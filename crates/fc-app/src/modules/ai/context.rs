//! Turns an [`AiTask`] into the exact provider request: resolves provider and model (request
//! override → feature route → default provider), reads the context (note type fields, note
//! values, the rendered card, deck classifications), renders the feature's template and picks
//! the output schema. Read-only: used by the `ai.preview` query and again by `ai.run`.

use super::prompts::{
    classifications_block, default_template, fields_block, note_block, render_template,
    FieldSpec, OutputSpec, SYSTEM,
};
use super::{prompt_setting, ID};
use crate::error::{AppError, AppResult};
use crate::registry::Registry;
use crate::support::content::{card_seed, render_card, RenderSetup};
use crate::support::decks::DeckIndex;
use crate::support::notes::{require_live_note, require_notetype};
use crate::support::{deleted, err_id};
use fc_api::v1::ai::{AiFeature, AiTask, PreviewAi};
use fc_api::v1::attributes::AttributeAssignment;
use fc_api::{ApiError, ErrorCode};
use fc_core::attribute::{effective_attribute_defs, AttributeDef, AttributeTarget, AttributeValue, EntityRef};
use fc_core::card::Card;
use fc_core::i18n::Locale;
use fc_core::note::Note;
use fc_core::notetype::{FieldKind, NoteType, NoteTypeKind};
use fc_core::settings::SettingScope;
use fc_core::{AiProviderId, CardId, DeckId, NoteId, NoteTypeId};
use fc_providers::ai::{CompletionRequest, ContentPart, Message, Role};
use fc_providers::strategy::Interaction;
use fc_storage::repo::{AiProviderRecord, CardFilter};
use fc_storage::Reads;
use fc_study::text::html_to_text;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Longest source text (characters).
pub(crate) const MAX_SOURCE_CHARS: usize = 100_000;
/// Most images per request.
pub(crate) const MAX_IMAGES: usize = 10;

/// Where a result goes (kept in the job payload with the request).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub(crate) struct Target {
    pub deck_id: Option<DeckId>,
    pub notetype_id: Option<NoteTypeId>,
    pub note_id: Option<NoteId>,
    /// Classification values for generated notes.
    pub attributes: Vec<AttributeAssignment>,
    /// Distractors: the field that holds them. Meaning in context: the note's meaning field.
    pub field: Option<String>,
    /// Meaning in context: the meaning the note has now (the source's), shown next to the
    /// suggestion and never overwritten by it.
    #[serde(default)]
    pub original: String,
}

/// A request ready to preview or send.
pub(crate) struct Built {
    pub feature: AiFeature,
    pub provider: AiProviderRecord,
    pub model: String,
    pub request: CompletionRequest,
    pub spec: OutputSpec,
    pub target: Target,
    /// Items whose content is sent.
    pub items: u32,
}

pub(crate) fn feature_of(task: &AiTask) -> AiFeature {
    match task {
        AiTask::GenerateCards { .. } => AiFeature::GenerateCards,
        AiTask::FillFields { .. } => AiFeature::FillFields,
        AiTask::Explain { .. } => AiFeature::Explain,
        AiTask::Hint { .. } => AiFeature::Hint,
        AiTask::CheckAnswer { .. } => AiFeature::CheckAnswer,
        AiTask::Improve { .. } => AiFeature::Improve,
        AiTask::RewriteLeech { .. } => AiFeature::RewriteLeech,
        AiTask::Distractors { .. } => AiFeature::Distractors,
        AiTask::MeaningInContext { .. } => AiFeature::MeaningInContext,
    }
}

pub(crate) fn task_error(reason: &str, message: impl Into<String>) -> AppError {
    AppError::Api(ApiError::new(ErrorCode::ValidationAiTask, message).with_detail("reason", reason))
}

pub(crate) fn require_provider(reads: &dyn Reads, id: AiProviderId) -> AppResult<AiProviderRecord> {
    reads
        .get_ai_provider(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundAiProvider, format!("AI provider {id} not found"), id))
}

pub(crate) fn require_live_provider(reads: &dyn Reads, id: AiProviderId) -> AppResult<AiProviderRecord> {
    let p = require_provider(reads, id)?;
    if p.deleted_at.is_some() {
        return Err(deleted("ai_provider", id));
    }
    Ok(p)
}

/// Provider and model for `feature`: explicit choice → route → default provider; model:
/// explicit → route (when it is the route's provider) → provider default.
pub(crate) fn resolve(
    reads: &dyn Reads,
    feature: AiFeature,
    provider_id: Option<AiProviderId>,
    model: Option<&str>,
) -> AppResult<(AiProviderRecord, String)> {
    let route = reads
        .list_ai_routes()?
        .into_iter()
        .find(|r| r.feature == feature.id());
    let provider = match provider_id.or(route.as_ref().and_then(|r| r.provider_id)) {
        Some(id) => require_live_provider(reads, id)?,
        None => {
            let live = reads.list_ai_providers(false)?;
            live.iter()
                .find(|p| p.is_default)
                .or(live.first())
                .cloned()
                .ok_or_else(|| {
                    AppError::api(ErrorCode::ConflictAiNoProvider, "no AI provider is configured")
                })?
        }
    };
    let route_model = route
        .filter(|r| r.provider_id.is_none_or(|p| p == provider.id))
        .and_then(|r| r.model);
    let model = model
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_owned)
        .or(route_model.filter(|m| !m.trim().is_empty()))
        .or_else(|| Some(provider.default_model.trim().to_owned()).filter(|m| !m.is_empty()))
        .ok_or_else(|| {
            AppError::Api(
                ApiError::new(ErrorCode::ConflictAiNoModel, "no model chosen")
                    .with_detail("provider", provider.id.to_string()),
            )
        })?;
    Ok((provider, model))
}

fn template(reads: &dyn Reads, feature: AiFeature) -> AppResult<String> {
    let stored = crate::settings::resolve_stored(reads, &SettingScope::App, &prompt_setting(feature))?;
    Ok(stored
        .and_then(|(v, _)| v.as_str().map(str::to_owned))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| default_template(feature).to_owned()))
}

fn language(locale: Locale) -> &'static str {
    match locale {
        Locale::Tr => "Turkish",
        Locale::En => "English",
    }
}

/// Language name of a requested answer language (`tr`, `en-GB`, …; `None` = the app's).
fn target_language(code: Option<&str>, locale: Locale) -> AppResult<String> {
    let Some(code) = code.map(str::trim).filter(|c| !c.is_empty()) else {
        return Ok(language(locale).to_owned());
    };
    if code.len() > 20 || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(task_error("target_language", "unknown answer language"));
    }
    let primary = code.split(['-', '_']).next().unwrap_or(code).to_ascii_lowercase();
    Ok(match primary.as_str() {
        "tr" => "Turkish",
        "en" => "English",
        "de" => "German",
        "fr" => "French",
        "es" => "Spanish",
        "it" => "Italian",
        "pt" => "Portuguese",
        "nl" => "Dutch",
        "ru" => "Russian",
        "ar" => "Arabic",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" => "Chinese",
        _ => return Ok(code.to_owned()),
    }
    .to_owned())
}

/// Longest sentence of a meaning-in-context task (characters).
pub(crate) const MAX_SENTENCE_CHARS: usize = 2000;
/// Most offered meanings of a meaning-in-context task.
pub(crate) const MAX_CANDIDATES: usize = 10;

/// Text of the note field with `role` (`""` = none / empty).
fn role_text(nt: &NoteType, note: &Note, role: fc_core::notetype::FieldRole) -> String {
    nt.field_with_role(role)
        .and_then(|f| note.fields.get(&f.key))
        .map(|v| html_to_text(v).trim().to_owned())
        .unwrap_or_default()
}

/// The meaning field of a note type (`meaning` role), if any.
pub(crate) fn meaning_field(nt: &NoteType) -> Option<String> {
    nt.field_with_role(fc_core::notetype::FieldRole::Meaning)
        .map(|f| f.key.0.clone())
}

/// What a meaning-in-context task asks about, resolved from the task and its note.
pub(crate) struct MeaningInput {
    pub sentence: String,
    pub lemma: String,
    pub surface: String,
    pub position: Option<u32>,
    pub candidates: Vec<String>,
    pub note_id: Option<NoteId>,
    pub field: Option<String>,
    pub original: String,
}

/// Resolves a [`AiTask::MeaningInContext`] (the note's fields by role fill what is not given).
#[allow(clippy::too_many_arguments)]
pub(crate) fn meaning_input(
    reads: &dyn Reads,
    note_id: Option<NoteId>,
    sentence: Option<&str>,
    lemma: Option<&str>,
    surface: Option<&str>,
    position: Option<u32>,
    candidates: &[String],
) -> AppResult<MeaningInput> {
    use fc_core::notetype::FieldRole;
    let given = |v: Option<&str>| v.map(|s| html_to_text(s).trim().to_owned()).filter(|s| !s.is_empty());
    let (mut sentence, mut lemma, mut surface) = (given(sentence), given(lemma), given(surface));
    let (mut field, mut original) = (None, String::new());
    if let Some(id) = note_id {
        let note = require_live_note(reads, id)?;
        let nt = require_notetype(reads, note.notetype_id)?;
        let filled = |s: String| Some(s).filter(|s| !s.is_empty());
        sentence = sentence.or_else(|| filled(role_text(&nt, &note, FieldRole::Example)));
        lemma = lemma.or_else(|| filled(role_text(&nt, &note, FieldRole::Term)));
        surface = surface.or_else(|| filled(role_text(&nt, &note, FieldRole::Surface)));
        field = meaning_field(&nt);
        original = role_text(&nt, &note, FieldRole::Meaning);
    }
    let sentence = sentence.ok_or_else(|| task_error("no_sentence", "there is no sentence to read the word in"))?;
    let lemma = lemma.ok_or_else(|| task_error("no_word", "there is no word to explain"))?;
    if sentence.chars().count() > MAX_SENTENCE_CHARS || lemma.chars().count() > 200 {
        return Err(task_error("too_long", "the sentence or the word is too long"));
    }
    let surface = surface.unwrap_or_else(|| lemma.clone());
    if let Some(p) = position {
        let at: String = sentence.chars().skip(p as usize).take(surface.chars().count()).collect();
        if at.to_lowercase() != surface.to_lowercase() {
            return Err(task_error("position", "the word is not at that position of the sentence"));
        }
    }
    let mut offered: Vec<String> = Vec::new();
    for c in candidates.iter().map(|c| html_to_text(c).trim().to_owned()).chain(
        Some(original.clone()).filter(|o| !o.is_empty()),
    ) {
        if c.is_empty() || c.chars().count() > super::prompts::MAX_MEANING_CHARS {
            continue;
        }
        if !offered.iter().any(|o| o.to_lowercase() == c.to_lowercase()) {
            offered.push(c);
        }
    }
    if offered.len() > MAX_CANDIDATES {
        return Err(task_error("candidates", "at most 10 meanings can be offered"));
    }
    Ok(MeaningInput {
        sentence,
        lemma,
        surface,
        position,
        candidates: offered,
        note_id,
        field,
        original,
    })
}

/// Fields the model reads and writes: text-like ones (media fields are left alone).
fn text_fields(nt: &NoteType) -> Vec<FieldSpec> {
    nt.fields
        .iter()
        .filter(|f| matches!(f.kind, FieldKind::Text | FieldKind::RichText | FieldKind::Number))
        .map(|f| FieldSpec {
            key: f.key.0.clone(),
            name: f.name.clone(),
            required: f.required || f.key == nt.sort_field,
        })
        .collect()
}

fn note_values(note: &Note) -> BTreeMap<String, String> {
    note.fields.iter().map(|(k, v)| (k.0.clone(), v.clone())).collect()
}

/// Display text of a value (option labels for choices).
fn value_text(def: &AttributeDef, value: &AttributeValue) -> String {
    let label = |id| def.option(id).map(|o| o.label.clone()).unwrap_or_default();
    match value {
        AttributeValue::Choice(id) => label(*id),
        AttributeValue::Choices(ids) => ids.iter().map(|i| label(*i)).collect::<Vec<_>>().join(", "),
        AttributeValue::Number(n) => n.to_string(),
        AttributeValue::Text(s) | AttributeValue::Date(s) => s.clone(),
        AttributeValue::Bool(b) => if *b { "yes" } else { "no" }.to_owned(),
    }
}

/// `(name, value)` of a classification, for the prompt.
type Line = (String, String);

/// Classification values for notes going to `deck`: the given ones, else each definition's
/// default. Returns the explicit assignments and the `(name, value)` prompt lines.
fn deck_classifications(
    reads: &dyn Reads,
    deck: DeckId,
    given: &[AttributeAssignment],
) -> AppResult<(Vec<AttributeAssignment>, Vec<Line>)> {
    let index = DeckIndex::load(reads)?;
    index.require_live(deck)?;
    let ancestry = index.ancestry(deck)?;
    let defs = reads.list_attribute_defs(false)?;
    let effective: Vec<AttributeDef> = effective_attribute_defs(&ancestry, &defs)
        .into_iter()
        .map(|e| e.def.clone())
        .filter(|d| d.target == AttributeTarget::Note)
        .collect();
    for a in given {
        if !effective.iter().any(|d| d.id == a.attr_id) {
            return Err(task_error("attribute", format!("classification {} does not apply to this deck", a.attr_id)));
        }
    }
    let mut assignments = Vec::new();
    let mut lines = Vec::new();
    for def in &effective {
        let value = match given.iter().find(|a| a.attr_id == def.id) {
            Some(a) => a.value.clone(),
            None => def.default_value.clone(),
        };
        if let Some(v) = value {
            lines.push((def.name.clone(), value_text(def, &v)));
            assignments.push(AttributeAssignment {
                attr_id: def.id,
                value: Some(v),
            });
        }
    }
    Ok((assignments, lines))
}

/// `(name, value)` lines of a note's classifications.
fn note_classifications(reads: &dyn Reads, note: NoteId) -> AppResult<Vec<(String, String)>> {
    let mut lines = Vec::new();
    for (id, value) in reads.get_attribute_values(EntityRef::Note(note))? {
        if let Some(def) = reads.get_attribute_def(id)? {
            lines.push((def.name.clone(), value_text(&def, &value)));
        }
    }
    Ok(lines)
}

/// Text after the answer separator (`<hr class="fc-answer">`, else the first `<hr>`), like the
/// study engine's expected answer.
fn answer_text(html: &str) -> String {
    let after_tag = |i: usize| html[i..].find('>').map(|j| &html[i + j + 1..]);
    let rest = html
        .find("fc-answer")
        .and_then(after_tag)
        .or_else(|| html.find("<hr").and_then(after_tag))
        .unwrap_or(html);
    html_to_text(rest)
}

/// Question and answer of a card as plain text (rendered by its card type engine).
fn card_qa(reads: &dyn Reads, registry: &Registry, locale: Locale, card: &Card) -> AppResult<(String, String)> {
    let note = require_live_note(reads, card.note_id)?;
    let nt = require_notetype(reads, note.notetype_id)?;
    let r = render_card(
        registry,
        reads,
        &nt,
        &note,
        &card.template_key,
        &RenderSetup {
            deck: Some(card.deck_id),
            seed: card_seed(card.id),
            locale,
        },
    )?;
    // v1.3: like the study engine — the typed key, else the field the template asks for (a
    // vocabulary card's meaning, not the example and source under it), else the back's
    // answer part.
    let answer = match &r.raw.interaction {
        Interaction::Type { expected } => expected.clone(),
        Interaction::Choice { .. } => answer_text(&r.sides.answer),
        Interaction::SelfGrade => crate::support::content::answer_field_text(&nt, &note, &card.template_key)
            .unwrap_or_else(|| answer_text(&r.sides.answer)),
    };
    Ok((html_to_text(&r.sides.question), answer))
}

fn require_card(reads: &dyn Reads, id: CardId) -> AppResult<Card> {
    let card = reads
        .get_card(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundCard, format!("card {id} not found"), id))?;
    if card.deleted_at.is_some() {
        return Err(deleted("card", id));
    }
    Ok(card)
}

/// Question/answer of a note's first live card.
fn note_qa(reads: &dyn Reads, registry: &Registry, locale: Locale, note: NoteId) -> AppResult<(String, String)> {
    let mut cards: Vec<Card> = reads.list_cards(&CardFilter {
        note_ids: Some(vec![note]),
        ..CardFilter::default()
    })?;
    cards.sort_by(|a, b| a.template_key.cmp(&b.template_key));
    let card = cards
        .first()
        .ok_or_else(|| task_error("no_cards", "the note has no cards"))?;
    card_qa(reads, registry, locale, card)
}

fn require_text_notetype(reads: &dyn Reads, id: NoteTypeId) -> AppResult<(NoteType, Vec<FieldSpec>)> {
    let nt = require_notetype(reads, id)?;
    if nt.deleted_at.is_some() {
        return Err(deleted("notetype", id));
    }
    let fields = text_fields(&nt);
    if fields.is_empty() {
        return Err(task_error("no_fields", "the note type has no text fields"));
    }
    Ok((nt, fields))
}

/// Builds the request of `req`.
pub(crate) fn build(
    reads: &dyn Reads,
    registry: &Registry,
    locale: Locale,
    req: &PreviewAi,
) -> AppResult<Built> {
    let feature = feature_of(&req.task);
    let (provider, model) = resolve(reads, feature, req.provider_id, req.model.as_deref())?;
    let mut vars: BTreeMap<&str, String> = BTreeMap::new();
    vars.insert("language", language(locale).into());
    let mut images: Vec<ContentPart> = Vec::new();
    let mut target = Target::default();
    let items: u32;
    let max_tokens: u32;
    let spec = match &req.task {
        AiTask::GenerateCards {
            deck_id,
            notetype_id,
            source,
            count,
            instructions,
            images: hashes,
            attributes,
        } => {
            let source = source.trim();
            if source.is_empty() && hashes.is_empty() {
                return Err(task_error("empty_source", "nothing to make cards from"));
            }
            if source.chars().count() > MAX_SOURCE_CHARS {
                return Err(task_error("source_too_long", "the source is too long"));
            }
            if !(1..=50).contains(count) {
                return Err(task_error("count", "count must be 1–50"));
            }
            if hashes.len() > MAX_IMAGES {
                return Err(task_error("images", "too many images"));
            }
            for h in hashes {
                let media = reads
                    .get_media(h)?
                    .filter(|m| m.mime.starts_with("image/"))
                    .ok_or_else(|| task_error("image", format!("image {h} not found")))?;
                images.push(ContentPart::Image {
                    media_hash: media.hash,
                    mime: media.mime,
                });
            }
            let (nt, fields) = require_text_notetype(reads, *notetype_id)?;
            let (assignments, lines) = deck_classifications(reads, *deck_id, attributes)?;
            vars.insert("count", count.to_string());
            vars.insert("notetype", nt.name.clone());
            vars.insert("fields", fields_block(&fields, nt.kind == NoteTypeKind::Cloze));
            vars.insert("classifications", classifications_block(&lines));
            vars.insert(
                "instructions",
                instructions
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| format!("Also: {}\n", s.chars().take(2000).collect::<String>()))
                    .unwrap_or_default(),
            );
            vars.insert("source", source.to_owned());
            target = Target {
                deck_id: Some(*deck_id),
                notetype_id: Some(*notetype_id),
                attributes: assignments,
                ..Target::default()
            };
            items = 1 + hashes.len() as u32;
            max_tokens = (200 + 250 * count).clamp(512, 16_000);
            OutputSpec::Notes {
                fields,
                count: *count,
            }
        }
        AiTask::FillFields {
            deck_id,
            notetype_id,
            fields: given,
            attributes,
        } => {
            let (nt, fields) = require_text_notetype(reads, *notetype_id)?;
            let filled = |f: &FieldSpec| given.get(&f.key).is_some_and(|v| !html_to_text(v).is_empty());
            let empty: Vec<FieldSpec> = fields.iter().filter(|f| !filled(f)).cloned().collect();
            if empty.is_empty() {
                return Err(task_error("nothing_to_fill", "every field is filled"));
            }
            if !fields.iter().any(filled) {
                return Err(task_error("no_fields", "type at least one field first"));
            }
            let (assignments, lines) = deck_classifications(reads, *deck_id, attributes)?;
            vars.insert("notetype", nt.name.clone());
            vars.insert("fields", fields_block(&fields, nt.kind == NoteTypeKind::Cloze));
            vars.insert("classifications", classifications_block(&lines));
            vars.insert("note", note_block(given, &fields));
            vars.insert(
                "empty_fields",
                empty.iter().map(|f| f.key.as_str()).collect::<Vec<_>>().join(", "),
            );
            target = Target {
                deck_id: Some(*deck_id),
                notetype_id: Some(*notetype_id),
                attributes: assignments,
                ..Target::default()
            };
            items = 1;
            max_tokens = 1024;
            OutputSpec::Fill { fields: empty }
        }
        AiTask::Explain { card_id, question } => {
            let card = require_card(reads, *card_id)?;
            let (q, a) = card_qa(reads, registry, locale, &card)?;
            vars.insert("question", q);
            vars.insert("answer", a);
            vars.insert(
                "classifications",
                classifications_block(&note_classifications(reads, card.note_id)?),
            );
            vars.insert(
                "followup",
                question
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| format!("The learner asks: {}", s.chars().take(1000).collect::<String>()))
                    .unwrap_or_default(),
            );
            target.note_id = Some(card.note_id);
            items = 1;
            max_tokens = 1024;
            OutputSpec::Text
        }
        AiTask::Hint { card_id } => {
            let card = require_card(reads, *card_id)?;
            let (q, a) = card_qa(reads, registry, locale, &card)?;
            vars.insert("question", q);
            vars.insert("answer", a);
            target.note_id = Some(card.note_id);
            items = 1;
            max_tokens = 200;
            OutputSpec::Text
        }
        AiTask::CheckAnswer { card_id, given } => {
            let given = given.trim();
            if given.is_empty() || given.chars().count() > 2000 {
                return Err(task_error("no_answer", "type an answer first"));
            }
            let card = require_card(reads, *card_id)?;
            let (q, a) = card_qa(reads, registry, locale, &card)?;
            vars.insert("question", q);
            vars.insert("answer", a);
            vars.insert("given", given.to_owned());
            target.note_id = Some(card.note_id);
            items = 1;
            max_tokens = 300;
            OutputSpec::Verdict
        }
        AiTask::Improve { note_id } | AiTask::RewriteLeech { note_id } => {
            let note = require_live_note(reads, *note_id)?;
            let (_, fields) = require_text_notetype(reads, note.notetype_id)?;
            let current = note_values(&note);
            vars.insert("fields", fields_block(&fields, false));
            vars.insert("note", note_block(&current, &fields));
            vars.insert(
                "classifications",
                classifications_block(&note_classifications(reads, note.id)?),
            );
            target.note_id = Some(note.id);
            items = 1;
            max_tokens = 2048;
            let extra = if feature == AiFeature::Improve { "reason" } else { "mnemonic" };
            OutputSpec::Rewrite {
                fields,
                current,
                extra: extra.into(),
            }
        }
        AiTask::Distractors { note_id, count } => {
            if !(1..=10).contains(count) {
                return Err(task_error("count", "count must be 1–10"));
            }
            let note = require_live_note(reads, *note_id)?;
            let nt = require_notetype(reads, note.notetype_id)?;
            let (q, a) = note_qa(reads, registry, locale, note.id)?;
            vars.insert("count", count.to_string());
            vars.insert("question", q);
            vars.insert("answer", a.clone());
            vars.insert(
                "classifications",
                classifications_block(&note_classifications(reads, note.id)?),
            );
            target.note_id = Some(note.id);
            target.field = nt
                .fields
                .iter()
                .find(|f| f.key.0 == "distractors")
                .map(|f| f.key.0.clone());
            items = 1;
            max_tokens = 400;
            OutputSpec::Options {
                count: *count,
                answer: a,
            }
        }
        AiTask::MeaningInContext {
            note_id,
            sentence,
            lemma,
            surface,
            position,
            target_language: answer_lang,
            candidates,
        } => {
            let m = meaning_input(
                reads,
                *note_id,
                sentence.as_deref(),
                lemma.as_deref(),
                surface.as_deref(),
                *position,
                candidates,
            )?;
            vars.insert("language", target_language(answer_lang.as_deref(), locale)?);
            vars.insert("sentence", m.sentence.clone());
            vars.insert("lemma", m.lemma.clone());
            vars.insert(
                "surface",
                match m.position {
                    Some(p) => format!("{} (at character {p})", m.surface),
                    None => m.surface.clone(),
                },
            );
            vars.insert(
                "candidates",
                if m.candidates.is_empty() {
                    String::new()
                } else {
                    format!("Meanings offered by the dictionary: {}\n", m.candidates.join("; "))
                },
            );
            target.note_id = m.note_id;
            target.field = m.field;
            target.original = m.original;
            items = 1;
            max_tokens = 600;
            OutputSpec::Meaning { sentence: m.sentence }
        }
    };
    let text = render_template(&template(reads, feature)?, &vars);
    let mut content = vec![ContentPart::Text { text }];
    content.extend(images);
    let request = CompletionRequest {
        model: Some(model.clone()),
        system: Some(SYSTEM.to_owned()),
        messages: vec![Message {
            role: Role::User,
            content,
        }],
        max_tokens,
        temperature: None,
        json_schema: spec.schema(),
        feature: format!("{ID}.{}", feature.id()),
    };
    Ok(Built {
        feature,
        provider,
        model,
        request,
        spec,
        target,
        items,
    })
}
