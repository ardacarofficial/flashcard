//! Cards from reading: word and sentence cards (`reading.cards.make`) and clozes from an
//! item's text (`reading.extracts.cloze`). Both run a nested `notes.create` on a built-in note
//! type (Vocabulary, Sentence card, Cloze), so validation, duplicates, cards and undo are
//! exactly those of creating the note by hand.

use super::items::{byte_range, require_item};
use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome};
use crate::registry::Registry;
use crate::support::{from_json, summary};
use fc_api::v1::methods::*;
use fc_api::v1::notes::{CreateNote, NoteDto};
use fc_api::v1::reading::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::content::builtin;
use fc_core::content::html::escape_html;
use fc_core::reading::{cloze_from_spans, emphasize, format_timestamp, ClozeSpanError};
use fc_core::NoteTypeId;
use std::collections::BTreeMap;

pub(super) fn register(reg: &mut Registry) {
    reg.command::<ReadingCardsMake>(make);
    reg.command::<ReadingExtractsCloze>(cloze);
}

/// Id of a seeded, live built-in note type.
fn builtin_notetype(ctx: &CommandCtx<'_>, key: &str) -> AppResult<NoteTypeId> {
    let conflict = || {
        AppError::Api(
            ApiError::new(
                ErrorCode::ConflictReadingNoteType,
                format!("the built-in note type {key} is missing or in the trash"),
            )
            .with_detail("notetype", key),
        )
    };
    let id = builtin::find(key).ok_or_else(conflict)?.id;
    match ctx.reads().get_notetype(id)? {
        Some(nt) if nt.deleted_at.is_none() => Ok(id),
        _ => Err(conflict()),
    }
}

fn create_note(
    ctx: &mut CommandCtx<'_>,
    notetype_id: NoteTypeId,
    deck_id: fc_core::DeckId,
    fields: BTreeMap<String, String>,
    tags: Vec<String>,
) -> AppResult<NoteDto> {
    let v = ctx.call(&inverse_call::<NotesCreate>(&CreateNote {
        notetype_id,
        deck_id,
        fields,
        tags,
        attributes: vec![],
        allow_duplicate: Some(false),
    })?)?;
    from_json(&v)
}

/// A `media:<hash>` reference or nothing.
fn media_ref(raw: Option<&str>) -> AppResult<String> {
    match raw.map(str::trim).filter(|r| !r.is_empty()) {
        None => Ok(String::new()),
        Some(r)
            if r.strip_prefix("media:")
                .is_some_and(fc_core::content::refs::is_media_hash) =>
        {
            Ok(r.to_owned())
        }
        Some(_) => Err(crate::support::invalid("media must be a media:<hash> reference")),
    }
}

/// Plain text → field HTML.
fn html(text: &str) -> String {
    escape_html(text.trim())
}

fn make(req: MakeReadingCard, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<NoteDto>> {
    let word = req.word.trim();
    if word.is_empty() {
        return Err(AppError::Api(
            ApiError::new(ErrorCode::ValidationReadingText, "the word is empty")
                .with_detail("field", "word"),
        ));
    }
    let sentence = req.sentence.trim_end();
    let example = match req.word_range {
        Some([s, e]) if !sentence.is_empty() => {
            let (bs, be) = byte_range(sentence, s, e, "word_range")?;
            emphasize(sentence, bs, be)
        }
        _ => html(sentence),
    };
    // Where it came from: "Title · 1:02" (subtitle cue time).
    let mut source = String::new();
    // The language the word was read in (v1.4 word index): the item's, else reading mode's.
    let mut language = None;
    if let Some(id) = req.item_id {
        let item = require_item(ctx.reads(), id)?;
        language = Some(item.language.clone());
        source = item.title.clone();
        if let Some(i) = req.cue_index {
            let cues: Vec<CueDto> = item
                .cues
                .as_deref()
                .and_then(|c| serde_json::from_str(c).ok())
                .unwrap_or_default();
            let cue = cues
                .get(usize::try_from(i).unwrap_or(usize::MAX))
                .ok_or_else(|| super::bad_range("cue_index"))?;
            source = format!("{source} · {}", format_timestamp(cue.start_ms));
        }
    }
    let audio = media_ref(req.audio.as_deref())?;
    let image = media_ref(req.image.as_deref())?;
    let (key, fields): (&str, Vec<(&str, String)>) = match req.kind {
        ReadingCardKind::Word => (
            "vocabulary",
            vec![
                ("word", html(word)),
                ("meaning", html(&req.meaning)),
                ("example", example),
                ("audio", audio),
                ("image", image),
            ],
        ),
        ReadingCardKind::Sentence => {
            if sentence.trim().is_empty() {
                return Err(AppError::Api(
                    ApiError::new(ErrorCode::ValidationReadingText, "the sentence is empty")
                        .with_detail("field", "sentence"),
                ));
            }
            (
                "sentence",
                vec![
                    ("sentence", example),
                    ("translation", html(&req.translation)),
                    ("word", html(word)),
                    ("meaning", html(&req.meaning)),
                    ("source", source),
                    ("audio", audio),
                    ("image", image),
                ],
            )
        }
    };
    let notetype = builtin_notetype(ctx, key)?;
    let fields = fields
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
    let note = create_note(ctx, notetype, req.deck_id, fields, req.tags)?;
    let language = match language {
        Some(l) => l,
        None => super::language_or_default(ctx.reads(), "")?,
    };
    // Derived data like the index itself: an undone note is simply not indexed.
    ctx.tx
        .set_word_note_language(note.id, &fc_core::words::index_language(&language))?;
    Ok(CommandOutcome {
        response: note,
        // Undone through the nested `notes.create` entry (same batch).
        inverse: vec![],
        summary: summary::<ReadingCardsMake>().param("word", word.to_owned()),
        events: vec![],
    })
}

fn span_error(e: ClozeSpanError) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationReadingRange, e.to_string()).with_detail("field", "hide"),
    )
}

fn cloze(req: ClozeFromItem, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<NoteDto>> {
    let item = require_item(ctx.reads(), req.item_id)?;
    let body = item.body.as_str();
    let (rs, re) = match req.range {
        Some([s, e]) => byte_range(body, s, e, "range")?,
        None => (0, body.len()),
    };
    let base = super::Utf16::new(body).unit_of(rs);
    let part = &body[rs..re];
    let mut spans = Vec::with_capacity(req.hide.len());
    for [s, e] in &req.hide {
        let (s, e) = (
            s.checked_sub(base).ok_or_else(|| super::bad_range("hide"))?,
            e.checked_sub(base).ok_or_else(|| super::bad_range("hide"))?,
        );
        spans.push(byte_range(part, s, e, "hide")?);
    }
    let text = cloze_from_spans(part, &spans, req.together).map_err(span_error)?;
    let notetype = builtin_notetype(ctx, "cloze")?;
    let mut fields = BTreeMap::new();
    fields.insert("text".to_owned(), text.trim().replace('\n', "<br>"));
    fields.insert("extra".to_owned(), html(&item.title));
    let note = create_note(ctx, notetype, req.deck_id, fields, req.tags)?;
    Ok(CommandOutcome {
        response: note,
        inverse: vec![],
        summary: summary::<ReadingExtractsCloze>().param("title", item.title.clone()),
        events: vec![],
    })
}
