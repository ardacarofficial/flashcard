//! v0.3 content building blocks: card type engine lookup, full note type validation, card
//! generation (through the registered engines), rendering with sanitizing, note reference
//! bookkeeping (`[[links]]`, media) and built-in note type seeding.

use super::decks::DeckIndex;
use crate::error::{AppError, AppResult};
use crate::handler::CommandCtx;
use crate::registry::Registry;
use fc_api::v1::cardtypes::{
    CardInteraction, NoteFieldsSnapshot, NoteTypeSnapshot, RenderedSides, TtsItem,
};
use fc_api::{ApiError, ErrorCode};
use fc_core::card::{Card, CardSpec};
use fc_core::content::builtin;
use fc_core::content::refs::{extract_links, extract_media, link_key};
use fc_core::content::{sanitize_css, sanitize_html, validate_templates};
use fc_core::i18n::Locale;
use fc_core::note::{fnv1a64, strip_html, Note};
use fc_core::notetype::{validate_notetype, CardTemplate, FieldKey, NoteType};
use fc_core::{CardId, DeckId, NoteId, Timestamp};
use fc_providers::strategy::{
    CardInput, CardTypeEngine, Interaction, RenderContext, RenderedCard, StrategyError,
};
use fc_providers::Capability;
use fc_storage::repo::{CardFilter, NoteRefs};
use fc_storage::{Reads, WriteTx};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use fc_providers::card_types::DISTRACTOR_POOL_CAPABILITY as DISTRACTOR_POOL;

/// Most notes sampled for automatic distractors.
const POOL_LIMIT: usize = 50;

/// `NOT_FOUND_CARD_TYPE`.
pub(crate) fn unknown_card_type(id: &str) -> AppError {
    AppError::Api(
        ApiError::new(
            ErrorCode::NotFoundCardType,
            format!("card type {id:?} is not registered"),
        )
        .with_detail("card_type", id),
    )
}

/// Engine by id.
pub(crate) fn engine<'r>(
    registry: &'r Registry,
    id: &str,
) -> AppResult<&'r Arc<dyn CardTypeEngine>> {
    registry
        .card_type_engine(id)
        .ok_or_else(|| unknown_card_type(id))
}

/// A strategy failure as `VALIDATION_NOTE_TYPE` (`details.reason`).
pub(crate) fn strategy_error(e: StrategyError) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationNoteType, e.0.clone()).with_detail("reason", e.0),
    )
}

/// Structural + template + engine validation of a note type.
pub(crate) fn validate_full(registry: &Registry, nt: &NoteType) -> AppResult<()> {
    validate_notetype(nt)?;
    validate_templates(nt)?;
    for t in &nt.templates {
        engine(registry, &t.card_type)?
            .validate(nt, t)
            .map_err(strategy_error)?;
    }
    Ok(())
}

/// The cards `note` should have, over every template (in template order).
pub(crate) fn card_specs(
    registry: &Registry,
    nt: &NoteType,
    note: &Note,
) -> AppResult<Vec<CardSpec>> {
    let mut out: Vec<CardSpec> = Vec::new();
    for t in &nt.templates {
        for spec in engine(registry, &t.card_type)?
            .generate_cards(note, nt, t)
            .map_err(strategy_error)?
        {
            if !out.iter().any(|s| s.template_key == spec.template_key) {
                out.push(spec);
            }
        }
    }
    Ok(out)
}

/// The template that produced a card: by key, else the note type's only template (cloze and
/// image occlusion cards are keyed `c<n>` / `m<n>`).
pub(crate) fn template_for<'n>(nt: &'n NoteType, template_key: &str) -> Option<&'n CardTemplate> {
    nt.templates
        .iter()
        .find(|t| t.key == template_key)
        .or_else(|| (nt.templates.len() == 1).then(|| &nt.templates[0]))
}

/// Creates the cards `note` should have but lacks (no **live** card with that template key)
/// in `deck`. Never removes cards (their history is kept). Returns the new card ids.
pub(crate) fn add_missing_cards(
    ctx: &mut CommandCtx<'_>,
    nt: &NoteType,
    note: &Note,
    deck: DeckId,
) -> AppResult<Vec<CardId>> {
    let existing: BTreeSet<String> = ctx
        .reads()
        .list_cards(&CardFilter {
            deck_ids: None,
            note_ids: Some(vec![note.id]),
            include_deleted: false,
        })?
        .into_iter()
        .map(|c| c.template_key)
        .collect();
    let mut created = Vec::new();
    for spec in card_specs(ctx.registry, nt, note)? {
        if existing.contains(&spec.template_key) {
            continue;
        }
        let card = Card::new(CardId::generate(ctx.ids), note.id, deck, spec, ctx.now);
        ctx.tx.insert_card(&card)?;
        created.push(card.id);
    }
    Ok(created)
}

/// Deck new cards of an existing note go to: the deck of its first live card, else its home.
pub(crate) fn deck_for_new_cards(reads: &dyn Reads, note: &Note) -> AppResult<DeckId> {
    let mut cards = reads.list_cards(&CardFilter {
        deck_ids: None,
        note_ids: Some(vec![note.id]),
        include_deleted: false,
    })?;
    cards.sort_by_key(|c| (c.created_at, c.id));
    Ok(cards.first().map_or(note.home_deck_id, |c| c.deck_id))
}

/// Keeps `cards.card_type` equal to the producing template's engine for every card of the
/// note type's notes (after a schema change or its undo).
pub(crate) fn sync_card_types(
    ctx: &mut CommandCtx<'_>,
    nt: &NoteType,
    now: Timestamp,
) -> AppResult<()> {
    let notes = ctx.reads().note_ids_of_notetype(nt.id, true)?;
    if notes.is_empty() {
        return Ok(());
    }
    let cards = ctx.reads().list_cards(&CardFilter {
        deck_ids: None,
        note_ids: Some(notes),
        include_deleted: true,
    })?;
    for mut c in cards {
        if let Some(t) = template_for(nt, &c.template_key) {
            if c.card_type != t.card_type {
                c.card_type.clone_from(&t.card_type);
                c.updated_at = now;
                ctx.tx.update_card(&c)?;
            }
        }
    }
    Ok(())
}

/// Recomputes a note's reference rows: own link key, `[[links]]`, referenced media (only
/// hashes that exist in the media table). Called on every note write.
pub(crate) fn sync_note_refs(tx: &mut dyn WriteTx, note: &Note) -> AppResult<()> {
    let mut links: Vec<(String, String)> = Vec::new();
    let mut media = BTreeSet::new();
    for value in note.fields.values() {
        for l in extract_links(value) {
            if !links.iter().any(|(k, _)| *k == l.key) {
                links.push((l.key, l.label));
            }
        }
        media.extend(extract_media(value));
    }
    let mut known = Vec::new();
    for h in media {
        if tx.get_media(&h)?.is_some() {
            known.push(h);
        }
    }
    tx.set_note_refs(
        note.id,
        &NoteRefs {
            key: link_key(&note.sort_field),
            links,
            media: known,
        },
    )?;
    Ok(())
}

/// Seeds every missing built-in note type (localized). Idempotent; a built-in the user
/// trashed stays trashed.
pub(crate) fn seed_builtin_notetypes(
    tx: &mut dyn WriteTx,
    locale: Locale,
    now: Timestamp,
) -> AppResult<()> {
    for b in builtin::ALL {
        if tx.get_notetype(b.id)?.is_none() {
            tx.upsert_notetype(&b.build(now, locale))?;
        }
    }
    Ok(())
}

/// Note type → snapshot DTO.
pub(crate) fn snapshot(nt: &NoteType) -> NoteTypeSnapshot {
    NoteTypeSnapshot {
        id: nt.id,
        name: nt.name.clone(),
        kind: nt.kind,
        fields: nt.fields.clone(),
        templates: nt.templates.clone(),
        css: nt.css.clone(),
        sort_field: nt.sort_field.clone(),
        version: nt.version,
        builtin: nt.builtin.clone(),
        created_at: nt.created_at,
        updated_at: nt.updated_at,
        deleted_at: nt.deleted_at,
    }
}

/// Snapshot DTO → note type.
pub(crate) fn from_snapshot(s: NoteTypeSnapshot) -> NoteType {
    NoteType {
        id: s.id,
        name: s.name,
        kind: s.kind,
        fields: s.fields,
        templates: s.templates,
        css: s.css,
        sort_field: s.sort_field,
        version: s.version,
        builtin: s.builtin,
        created_at: s.created_at,
        updated_at: s.updated_at,
        deleted_at: s.deleted_at,
    }
}

/// A note's field values as a snapshot.
pub(crate) fn fields_snapshot(note: &Note) -> NoteFieldsSnapshot {
    NoteFieldsSnapshot {
        id: note.id,
        fields: note
            .fields
            .iter()
            .map(|(k, v)| (k.0.clone(), v.clone()))
            .collect(),
    }
}

/// What a render needs besides the note.
pub(crate) struct RenderSetup {
    /// Deck for `{{Deck}}` and distractors.
    pub deck: Option<DeckId>,
    /// Shuffle seed.
    pub seed: u64,
    /// UI locale.
    pub locale: Locale,
}

/// Seed of a card (stable across renders).
pub(crate) fn card_seed(id: CardId) -> u64 {
    fnv1a64(id.to_string().as_bytes())
}

/// Renders one card of `note` and sanitizes everything that leaves the backend.
pub(crate) fn render(
    registry: &Registry,
    reads: &dyn Reads,
    nt: &NoteType,
    note: &Note,
    template_key: &str,
    setup: &RenderSetup,
) -> AppResult<RenderedSides> {
    let template = template_for(nt, template_key).ok_or_else(|| {
        AppError::Api(
            ApiError::new(
                ErrorCode::ValidationNoteType,
                format!("no template for card {template_key}"),
            )
            .with_detail("template", template_key),
        )
    })?;
    let eng = engine(registry, &template.card_type)?;
    let info = eng.info();

    // [[links]] that resolve to live notes.
    let mut keys: Vec<String> = note
        .fields
        .values()
        .flat_map(|v| extract_links(v).into_iter().map(|l| l.key))
        .collect();
    keys.sort();
    keys.dedup();
    let mut links = BTreeMap::new();
    for (k, id) in reads.notes_by_link_key(&keys)? {
        links.entry(k).or_insert(id);
    }

    let deck_path = match setup.deck {
        Some(d) => DeckIndex::load(reads)?.path(d),
        None => String::new(),
    };
    let distractor_pool = if info
        .capabilities
        .contains(&Capability::Other(DISTRACTOR_POOL.to_owned()))
    {
        distractor_pool(reads, nt, note, setup.deck)?
    } else {
        Vec::new()
    };
    let ctx = RenderContext {
        deck_path,
        locale: setup.locale,
        links,
        distractor_pool,
        seed: setup.seed,
    };
    let input = CardInput {
        note,
        notetype: nt,
        template,
        template_key,
    };
    let rendered = eng.render(&input, &ctx).map_err(strategy_error)?;
    Ok(sides(template_key, template, rendered))
}

fn sides(template_key: &str, template: &CardTemplate, r: RenderedCard) -> RenderedSides {
    let template_name = if template.key == template_key {
        template.name.clone()
    } else {
        template_key.to_owned()
    };
    RenderedSides {
        template_key: template_key.to_owned(),
        template_name,
        card_type: template.card_type.clone(),
        question: sanitize_html(&r.question),
        answer: sanitize_html(&r.answer),
        css: sanitize_css(&r.css),
        interaction: match r.interaction {
            Interaction::SelfGrade => CardInteraction::SelfGrade,
            Interaction::Type { expected } => CardInteraction::Type { expected },
            Interaction::Choice { options, correct } => CardInteraction::Choice {
                options: options.iter().map(|o| sanitize_html(o)).collect(),
                correct: u32::try_from(correct).unwrap_or(0),
            },
        },
        tts: r
            .tts
            .into_iter()
            .map(|t| TtsItem {
                lang: t.lang,
                text: t.text,
            })
            .collect(),
    }
}

/// Plain-text `answer` values of other live notes of the same type in `deck`.
fn distractor_pool(
    reads: &dyn Reads,
    nt: &NoteType,
    note: &Note,
    deck: Option<DeckId>,
) -> AppResult<Vec<String>> {
    let Some(deck) = deck else {
        return Ok(Vec::new());
    };
    let answer = FieldKey::new("answer");
    let mut note_ids: Vec<NoteId> = reads
        .list_cards(&CardFilter {
            deck_ids: Some(vec![deck]),
            note_ids: None,
            include_deleted: false,
        })?
        .into_iter()
        .map(|c| c.note_id)
        .filter(|id| *id != note.id)
        .collect();
    note_ids.sort();
    note_ids.dedup();
    let mut out = Vec::new();
    for id in note_ids {
        if out.len() == POOL_LIMIT {
            break;
        }
        if let Some(n) = reads.get_note(id)? {
            if n.notetype_id == nt.id && n.deleted_at.is_none() {
                if let Some(v) = n.fields.get(&answer) {
                    let text = strip_html(v);
                    if !text.is_empty() {
                        out.push(text);
                    }
                }
            }
        }
    }
    Ok(out)
}
