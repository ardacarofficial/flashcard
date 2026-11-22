//! Meaning in context (v1.4): which meaning of a word fits THIS sentence.
//!
//! * Asking goes through the ordinary `ai.preview` → `ai.run` path
//!   (`AiTask::MeaningInContext`); the job's `AiOutput::MeaningInContext` writes nothing.
//! * `ai.meaning.apply` is the user accepting (maybe after editing) a meaning: it is stored in
//!   `note_meanings`, apart from the note's fields, with `original` = the meaning field's
//!   value before the first apply. The field itself changes only with `replace_field` (a
//!   nested `notes.update`), and the alternatives become accepted answers (a nested
//!   `accepted_answers.add`) — the whole apply is one undo step.
//! * `ai.meaning.suggest` asks without a preview, only after the user opted in
//!   (`ai.meaning.auto_suggest`, default off) and only while the monthly budget is not spent;
//!   the job stores the answer as a **suggestion** (the note is not changed, an applied meaning
//!   is never replaced). [`suggest_after_capture`] is the hook for the Subtitle capture.
//!
//! What never uses a model: scheduling, the written-answer check (it reads the accepted
//! answers offline), deciding a word is known.

use super::context::{build, meaning_field, task_error};
use super::prompts::MAX_MEANING_CHARS;
use super::run::{enqueue_auto, ensure_budget};
use super::{AiServices, AUTO_SUGGEST};
use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::jobs::JobContext;
use crate::registry::Registry;
use crate::support::notes::{require_live_note, require_notetype};
use crate::support::summary;
use fc_api::v1::accepted_answers::EditAcceptedAnswers;
use fc_api::v1::ai::*;
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::{
    AcceptedAnswersAdd, AiMeaningApply, AiMeaningGet, AiMeaningRestore, AiMeaningStoreSuggestion,
    AiMeaningSuggest, NotesUpdate,
};
use fc_api::v1::notes::UpdateNote;
use fc_api::{ApiError, ErrorCode};
use fc_core::settings::SettingScope;
use fc_core::NoteId;
use fc_storage::repo::NoteMeaningRecord;
use fc_storage::Reads;
use std::sync::Arc;

pub(super) fn register(reg: &mut Registry, services: &Arc<AiServices>) {
    reg.query::<AiMeaningGet>(|req: AiMeaningRef, ctx: &QueryCtx<'_>| {
        require_live_note(ctx.tx, req.note_id)?;
        state(ctx.tx, req.note_id)
    });
    reg.command::<AiMeaningApply>(apply);
    reg.command::<AiMeaningRestore>(restore);
    reg.command::<AiMeaningStoreSuggestion>(store);
    let s = Arc::clone(services);
    reg.command::<AiMeaningSuggest>(move |req: AiMeaningRef, ctx: &mut CommandCtx<'_>| suggest(&s, req, ctx));
}

fn status_str(s: AiMeaningStatus) -> &'static str {
    match s {
        AiMeaningStatus::Suggested => "suggested",
        AiMeaningStatus::Applied => "applied",
    }
}

fn dto(r: NoteMeaningRecord) -> AiNoteMeaning {
    AiNoteMeaning {
        note_id: r.note_id,
        field: r.field_key,
        status: if r.status == "applied" {
            AiMeaningStatus::Applied
        } else {
            AiMeaningStatus::Suggested
        },
        meaning: r.meaning,
        phrase: r.phrase,
        expression: r.expression,
        also_correct: r.also_correct,
        not_this: r.not_this,
        confidence: r.confidence.map(|c| c as f32),
        edited: r.edited,
        original: r.original,
        model: r.model,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

fn record(m: &AiNoteMeaning) -> NoteMeaningRecord {
    NoteMeaningRecord {
        note_id: m.note_id,
        field_key: m.field.clone(),
        status: status_str(m.status).to_owned(),
        meaning: m.meaning.clone(),
        phrase: m.phrase.clone(),
        expression: m.expression,
        also_correct: m.also_correct.clone(),
        not_this: m.not_this.clone(),
        confidence: m.confidence.map(f64::from),
        edited: m.edited,
        original: m.original.clone(),
        model: m.model.clone(),
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn state(reads: &dyn Reads, note: NoteId) -> AppResult<AiNoteMeaningState> {
    Ok(AiNoteMeaningState {
        note_id: note,
        meaning: reads.get_note_meaning(note)?.map(dto),
        accepted: crate::modules::accepted_answers::lists(reads, note)?,
    })
}

/// Same text, ignoring markup, case and surrounding spaces.
fn same_text(html: &str, text: &str) -> bool {
    fc_study::text::html_to_text(html).trim().to_lowercase() == text.trim().to_lowercase()
}

fn clean_list(items: &[String], max: usize) -> AppResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for s in items.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if s.chars().count() > MAX_MEANING_CHARS {
            return Err(task_error("too_long", "a meaning is too long"));
        }
        if !out.iter().any(|o| o.to_lowercase() == s.to_lowercase()) {
            out.push(s.to_owned());
        }
    }
    out.truncate(max);
    Ok(out)
}

fn apply(req: ApplyAiMeaning, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AiNoteMeaningState>> {
    let note = require_live_note(ctx.reads(), req.note_id)?;
    let nt = require_notetype(ctx.reads(), note.notetype_id)?;
    let field = meaning_field(&nt)
        .ok_or_else(|| task_error("no_meaning_field", "the note type has no meaning field"))?;
    let meaning = req.meaning.trim().to_owned();
    if meaning.is_empty() || meaning.chars().count() > MAX_MEANING_CHARS {
        return Err(task_error("meaning", "the meaning must be 1–300 characters"));
    }
    if req.confidence.is_some_and(|c| !(0.0..=1.0).contains(&c)) {
        return Err(task_error("confidence", "confidence must be from 0 to 1"));
    }
    let phrase = req.phrase.trim().to_owned();
    if phrase.chars().count() > MAX_MEANING_CHARS {
        return Err(task_error("too_long", "the phrase is too long"));
    }
    let also_correct = clean_list(&req.also_correct, super::prompts::MAX_ALSO_CORRECT)?;
    let not_this = clean_list(&req.not_this, super::prompts::MAX_NOT_THIS)?;
    let current = note
        .fields
        .iter()
        .find(|(k, _)| k.0 == field)
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let previous = ctx.reads().get_note_meaning(note.id)?;
    // The source's definition: what the field held before the FIRST apply.
    let original = match &previous {
        Some(p) if p.status == "applied" => p.original.clone(),
        _ => current.clone(),
    };
    let row = NoteMeaningRecord {
        note_id: note.id,
        field_key: field.clone(),
        status: "applied".into(),
        meaning: meaning.clone(),
        phrase,
        expression: req.expression,
        also_correct,
        not_this,
        confidence: req.confidence.map(f64::from),
        edited: req.edited,
        original,
        model: req.model.as_deref().map(str::trim).unwrap_or_default().to_owned(),
        created_at: previous.as_ref().map_or(ctx.now, |p| p.created_at),
        updated_at: ctx.now,
    };
    ctx.tx.put_note_meaning(&row)?;
    // Accepted answers: the alternatives the user kept, and — when the field keeps the
    // source's wording — the applied meaning itself.
    let mut accepted = req.accepted.clone();
    let same_as_field = same_text(&current, &meaning);
    if !req.replace_field && !same_as_field {
        accepted.insert(0, meaning.clone());
    }
    if accepted.iter().any(|a| !a.trim().is_empty()) {
        ctx.call(&inverse_call::<AcceptedAnswersAdd>(&EditAcceptedAnswers {
            note_id: note.id,
            field: Some(field.clone()),
            answers: accepted,
        })?)?;
    }
    if req.replace_field && !same_as_field {
        let mut fields: std::collections::BTreeMap<String, String> =
            note.fields.iter().map(|(k, v)| (k.0.clone(), v.clone())).collect();
        fields.insert(field.clone(), meaning.clone());
        ctx.call(&inverse_call::<NotesUpdate>(&UpdateNote {
            id: note.id,
            deck_id: None,
            fields,
            tags: note.tags.iter().map(|t| t.as_str().to_owned()).collect(),
            attributes: vec![],
        })?)?;
    }
    Ok(CommandOutcome {
        response: state(ctx.reads(), note.id)?,
        // The nested `accepted_answers.add` / `notes.update` undo themselves (same batch).
        inverse: vec![inverse_call::<AiMeaningRestore>(&RestoreAiMeaning {
            note_id: note.id,
            meaning: previous.map(dto),
        })?],
        summary: summary::<AiMeaningApply>().param("meaning", meaning),
        events: vec![DomainEvent::NoteUpdated { note_id: note.id }],
    })
}

fn write_row(ctx: &mut CommandCtx<'_>, note: NoteId, row: Option<&AiNoteMeaning>) -> AppResult<()> {
    match row {
        Some(m) => {
            if m.note_id != note {
                return Err(task_error("note", "the meaning belongs to another note"));
            }
            ctx.tx.put_note_meaning(&record(m))?;
        }
        None => ctx.tx.delete_note_meaning(note)?,
    }
    Ok(())
}

fn restore(req: RestoreAiMeaning, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let before = ctx.reads().get_note_meaning(req.note_id)?.map(dto);
    write_row(ctx, req.note_id, req.meaning.as_ref())?;
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![inverse_call::<AiMeaningRestore>(&RestoreAiMeaning {
            note_id: req.note_id,
            meaning: before,
        })?],
        summary: summary::<AiMeaningRestore>(),
        events: vec![DomainEvent::NoteUpdated { note_id: req.note_id }],
    })
}

/// Stores an automatic suggestion: never over an applied meaning, never for a trashed note.
fn store(req: RestoreAiMeaning, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let live = ctx
        .reads()
        .get_note(req.note_id)?
        .is_some_and(|n| n.deleted_at.is_none());
    let applied = ctx
        .reads()
        .get_note_meaning(req.note_id)?
        .is_some_and(|m| m.status == "applied");
    let suggestion = req
        .meaning
        .as_ref()
        .is_some_and(|m| m.status == AiMeaningStatus::Suggested);
    if live && !applied && suggestion {
        write_row(ctx, req.note_id, req.meaning.as_ref())?;
    }
    Ok(CommandOutcome {
        response: Empty {},
        // Like a fetched model list: no undo step (the note itself is unchanged).
        inverse: vec![],
        summary: summary::<AiMeaningStoreSuggestion>(),
        events: if live && !applied && suggestion {
            vec![DomainEvent::NoteUpdated { note_id: req.note_id }]
        } else {
            vec![]
        },
    })
}

/// The job's write of an automatic request's answer (see [`store`]).
pub(super) fn store_suggestion(ctx: &JobContext<'_>, output: &AiOutput, model: &str) -> AppResult<()> {
    let AiOutput::MeaningInContext {
        note_id: Some(note_id),
        field: Some(field),
        meaning,
        phrase,
        expression,
        also_correct,
        not_this,
        confidence,
        original,
    } = output
    else {
        return Ok(());
    };
    let now = ctx.now();
    ctx.invoke::<AiMeaningStoreSuggestion>(&RestoreAiMeaning {
        note_id: *note_id,
        meaning: Some(AiNoteMeaning {
            note_id: *note_id,
            field: field.clone(),
            status: AiMeaningStatus::Suggested,
            meaning: meaning.clone(),
            phrase: phrase.clone(),
            expression: *expression,
            also_correct: also_correct.clone(),
            not_this: not_this.clone(),
            confidence: Some(*confidence),
            edited: false,
            original: original.clone(),
            model: model.to_owned(),
            created_at: now,
            updated_at: now,
        }),
    })?;
    Ok(())
}

/// Whether the user opted in to asking without a preview.
pub(crate) fn auto_suggest_on(reads: &dyn Reads) -> AppResult<bool> {
    Ok(crate::settings::resolve_stored(reads, &SettingScope::App, AUTO_SUGGEST)?
        .and_then(|(v, _)| v.as_bool())
        .unwrap_or(false))
}

fn suggest(services: &AiServices, req: AiMeaningRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<JobDto>> {
    if !auto_suggest_on(ctx.reads())? {
        return Err(AppError::Api(
            ApiError::new(
                ErrorCode::ConflictAiAutoOff,
                "asking without a preview is off (setting ai.meaning.auto_suggest)",
            )
            .with_detail("setting", AUTO_SUGGEST),
        ));
    }
    require_live_note(ctx.reads(), req.note_id)?;
    ensure_budget(ctx.reads(), ctx.now)?;
    let built = build(
        ctx.reads(),
        ctx.registry,
        ctx.locale,
        &PreviewAi {
            task: AiTask::MeaningInContext {
                note_id: Some(req.note_id),
                sentence: None,
                lemma: None,
                surface: None,
                position: None,
                target_language: None,
                candidates: vec![],
            },
            provider_id: None,
            model: None,
        },
    )?;
    if built.target.field.is_none() {
        return Err(task_error("no_meaning_field", "the note type has no meaning field"));
    }
    let job = enqueue_auto(services, built, ctx)?;
    Ok(CommandOutcome {
        response: job,
        inverse: vec![],
        summary: summary::<AiMeaningSuggest>(),
        events: vec![],
    })
}

/// Hook for the Subtitle capture (W1): after a capture made or matched `note_id`, queue an
/// automatic meaning suggestion **if** the AI module is on, the user opted in
/// (`ai.meaning.auto_suggest`), the budget is not spent and a provider is set up. Never
/// fails the capture: any of those missing → `None`, and nothing was written. Runs
/// `ai.meaning.suggest` nested, so it is the same path as a direct call.
pub fn suggest_after_capture(ctx: &mut CommandCtx<'_>, note_id: NoteId) -> Option<JobDto> {
    let enabled = crate::support::module_enabled(ctx.reads(), ctx.registry, super::ID).ok()?;
    if !enabled || !auto_suggest_on(ctx.reads()).ok()? {
        return None;
    }
    let call = inverse_call::<AiMeaningSuggest>(&AiMeaningRef { note_id }).ok()?;
    let value = ctx.call(&call).ok()?;
    serde_json::from_value(value).ok()
}
