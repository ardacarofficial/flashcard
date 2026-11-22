//! v1.4 (W1, docs/06 §5.1): a capture is an **encounter**, not a card.
//!
//! Every capture is kept with its source as a `subtitle_captures` row (an encounter) linked to
//! a note (term + meaning). A note may have several encounters (two scenes of one meaning =
//! one card, two examples); one of them is **primary**: the note's `surface`, `sentence`,
//! `translation`, `image`, `audio` and `source` fields show it, and its `examples` field lists
//! the others ([`fc_core::subtitle::examples_html`]).
//!
//! * **Placement** of a new capture ([`place`], setting [`SAME_WORD`]): with `add_example`
//!   (default) a capture of a word that already has a live Subtitle note in the same language
//!   becomes another example of the note whose meaning is the same
//!   ([`fc_core::subtitle::same_meaning`]) — no new card, schedule and history untouched.
//!   When the word has notes but no meaning matches, the capture gets its own note and is
//!   flagged for review (`review_note_id` = the word's first note): the user decides
//!   ([`resolve`]). `always_new_card` = the v1.3 behaviour (each line its own card).
//! * **Commands** (all undoable; the inverse is [`put`] of the rows as they were, plus the
//!   nested note edits): [`set_primary`], [`move_encounter`], [`merge_notes`], [`resolve`].
//! * **Merging** keeps the note with more reviews (a tie: the older note). The other note goes
//!   to the trash with its cards; its review log stays (the log outlives cards, migration
//!   0501), so statistics and the optimizer still count it, and undo brings the note back.

use super::*;
use fc_api::v1::notes::{NoteIds, UpdateNote};
use fc_core::subtitle::{example_fields, examples_html, same_meaning, EXAMPLES_FIELD, EXAMPLE_KEYS};
use fc_storage::repo::{CardFilter, EncounterFacts};

/// Setting (app): what a capture of an already captured word does.
pub const SAME_WORD: &str = "subtitle.same_word";
/// [`SAME_WORD`] (default): another example of the note with the same meaning.
pub const ADD_EXAMPLE: &str = "add_example";
/// [`SAME_WORD`]: every new line is a new card (v1.3).
pub const ALWAYS_NEW_CARD: &str = "always_new_card";

/// The [`SAME_WORD`] setting.
pub(super) fn same_word_setting() -> SettingDef {
    SettingDef {
        key: SAME_WORD.to_owned(),
        kind: SettingKind::Choice {
            options: vec![ADD_EXAMPLE.to_owned(), ALWAYS_NEW_CARD.to_owned()],
        },
        default: json!(ADD_EXAMPLE),
        label_key: format!("settings.{SAME_WORD}.label"),
        description_key: Some(format!("settings.{SAME_WORD}.description")),
        advanced: false,
        scopes: vec![SettingScopeKind::App],
    }
}

/// Whether new captures of a known word become examples ([`SAME_WORD`] = `add_example`).
pub(super) fn adds_examples(reads: &dyn Reads) -> AppResult<bool> {
    Ok(resolve_stored(reads, &SettingScope::App, SAME_WORD)?
        .and_then(|(v, _)| v.as_str().map(|s| s != ALWAYS_NEW_CARD))
        .unwrap_or(true))
}

// ------------------------------------------------------------------ rows

fn kind_of(s: Option<&str>) -> Option<SubtitleCaptureKind> {
    match s? {
        "word" => Some(SubtitleCaptureKind::Word),
        "phrase" => Some(SubtitleCaptureKind::Phrase),
        "sentence" => Some(SubtitleCaptureKind::Sentence),
        _ => None,
    }
}

pub(super) fn kind_name(k: SubtitleCaptureKind) -> &'static str {
    match k {
        SubtitleCaptureKind::Word => "word",
        SubtitleCaptureKind::Phrase => "phrase",
        SubtitleCaptureKind::Sentence => "sentence",
    }
}

fn source_of(s: Option<&str>) -> Option<SubtitleDefinitionSource> {
    match s? {
        "dict" => Some(SubtitleDefinitionSource::Dict),
        "ai" => Some(SubtitleDefinitionSource::Ai),
        "mt" => Some(SubtitleDefinitionSource::Mt),
        _ => None,
    }
}

pub(super) fn source_name(s: SubtitleDefinitionSource) -> &'static str {
    match s {
        SubtitleDefinitionSource::Dict => "dict",
        SubtitleDefinitionSource::Ai => "ai",
        SubtitleDefinitionSource::Mt => "mt",
    }
}

fn ms_in(v: Option<u64>) -> Option<i64> {
    v.and_then(|v| i64::try_from(v).ok())
}

fn ms_out(v: Option<i64>) -> Option<u64> {
    v.and_then(|v| u64::try_from(v).ok())
}

/// Storage row → API row.
pub(super) fn row_dto(r: SubtitleCaptureRecord) -> SubtitleCaptureRow {
    let f = r.facts;
    SubtitleCaptureRow {
        capture_id: r.capture_id,
        dedup_key: r.dedup_key,
        lang: r.lang,
        lemma: r.lemma,
        note_id: r.note_id,
        card_id: r.card_id,
        key_id: r.key_id,
        created_at: r.created_at,
        facts: SubtitleEncounterFacts {
            surface: f.surface,
            pos: f.pos,
            sentence: f.sentence,
            translation: f.translation,
            term_offset: f.term_start.zip(f.term_end),
            kind: kind_of(f.kind.as_deref()),
            definition: f.definition,
            definition_source: source_of(f.definition_source.as_deref()),
            video_id: f.video_id,
            title: f.title,
            site: f.site,
            url: f.url,
            t_start_ms: ms_out(f.t_start_ms),
            t_end_ms: ms_out(f.t_end_ms),
            screenshot_hash: f.screenshot_hash,
            audio_hash: f.audio_hash,
            example_fields: f.example_fields,
        },
        is_primary: r.is_primary,
        review_note_id: r.review_note_id,
    }
}

/// API row → storage row.
pub(super) fn row_record(r: SubtitleCaptureRow) -> SubtitleCaptureRecord {
    let f = r.facts;
    SubtitleCaptureRecord {
        capture_id: r.capture_id,
        dedup_key: r.dedup_key,
        lang: r.lang,
        lemma: r.lemma,
        note_id: r.note_id,
        card_id: r.card_id,
        key_id: r.key_id,
        created_at: r.created_at,
        facts: EncounterFacts {
            surface: f.surface,
            pos: f.pos,
            sentence: f.sentence,
            translation: f.translation,
            term_start: f.term_offset.map(|o| o.0),
            term_end: f.term_offset.map(|o| o.1),
            kind: f.kind.map(|k| kind_name(k).to_owned()),
            definition: f.definition,
            definition_source: f.definition_source.map(|s| source_name(s).to_owned()),
            video_id: f.video_id,
            title: f.title,
            site: f.site,
            url: f.url,
            t_start_ms: ms_in(f.t_start_ms),
            t_end_ms: ms_in(f.t_end_ms),
            screenshot_hash: f.screenshot_hash,
            audio_hash: f.audio_hash,
            example_fields: f.example_fields,
        },
        is_primary: r.is_primary,
        review_note_id: r.review_note_id,
    }
}

/// The facts of an incoming capture (`fields`: its note fields, media already resolved).
pub(super) fn facts_of(
    c: &SubtitleCapture,
    fields: &BTreeMap<String, String>,
    screenshot: Option<&str>,
    audio: Option<&str>,
) -> EncounterFacts {
    let text = |s: Option<&str>| {
        s.map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    EncounterFacts {
        surface: text(Some(&c.term.surface)),
        pos: text(c.term.pos.as_deref()),
        sentence: text(c.context.sentence.as_deref()),
        translation: text(c.context.translation.as_deref()),
        term_start: c.context.term_offset.map(|o| o.0),
        term_end: c.context.term_offset.map(|o| o.1),
        kind: Some(kind_name(c.kind).to_owned()),
        definition: text(Some(&c.definition.text)),
        definition_source: Some(source_name(c.definition.source).to_owned()),
        video_id: text(c.source.video_id.as_deref()),
        title: text(c.source.title.as_deref()),
        site: text(Some(&c.source.site)),
        url: text(c.source.url.as_deref()),
        t_start_ms: ms_in(c.source.t_start_ms),
        t_end_ms: ms_in(c.source.t_end_ms),
        screenshot_hash: screenshot.map(str::to_owned),
        audio_hash: audio.map(str::to_owned),
        example_fields: example_fields(fields),
    }
}

fn invalid(reason: &str, message: impl Into<String>) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationSubtitleEncounter, message).with_detail("reason", reason),
    )
}

fn require(reads: &dyn Reads, capture_id: &str) -> AppResult<SubtitleCaptureRecord> {
    reads.get_subtitle_capture(capture_id)?.ok_or_else(|| {
        AppError::Api(
            ApiError::new(
                ErrorCode::NotFoundSubtitleEncounter,
                format!("encounter {capture_id} not found"),
            )
            .with_detail("capture_id", capture_id),
        )
    })
}

/// A live note of the built-in Subtitle note type.
fn require_subtitle_note(ctx: &CommandCtx<'_>, id: NoteId) -> AppResult<fc_core::note::Note> {
    let note = crate::support::notes::require_live_note(ctx.reads(), id)?;
    if note.notetype_id != notetype(ctx)? {
        return Err(invalid(
            "not_subtitle_note",
            format!("note {id} is not a Subtitle card note"),
        ));
    }
    Ok(note)
}

/// The note's first live card.
fn card_of(reads: &dyn Reads, note: NoteId) -> AppResult<CardId> {
    reads
        .list_cards(&CardFilter {
            note_ids: Some(vec![note]),
            ..Default::default()
        })?
        .into_iter()
        .map(|c| c.id)
        .min()
        .ok_or_else(|| AppError::api(ErrorCode::Internal, "the subtitle note has no card"))
}

fn list_of(reads: &dyn Reads, note: NoteId) -> AppResult<SubtitleEncounterList> {
    Ok(SubtitleEncounterList {
        encounters: reads
            .subtitle_encounters_of_note(note)?
            .into_iter()
            .map(row_dto)
            .collect(),
    })
}

/// Rewrites `rows` (existing ones only) without ever having two primaries of one note: all
/// are first written as non-primary, then the primary ones. Returns the rows as they were.
fn put_rows(
    ctx: &mut CommandCtx<'_>,
    rows: &[SubtitleCaptureRecord],
) -> AppResult<Vec<SubtitleCaptureRecord>> {
    let mut old = Vec::new();
    for r in rows {
        if let Some(o) = ctx.reads().get_subtitle_capture(&r.capture_id)? {
            old.push(o);
            ctx.tx.update_subtitle_capture(&SubtitleCaptureRecord {
                is_primary: false,
                ..r.clone()
            })?;
        }
    }
    for r in rows.iter().filter(|r| r.is_primary) {
        if old.iter().any(|o| o.capture_id == r.capture_id) {
            ctx.tx.update_subtitle_capture(r)?;
        }
    }
    Ok(old)
}

fn put_inverse(old: Vec<SubtitleCaptureRecord>) -> AppResult<Vec<InvokeRequest>> {
    if old.is_empty() {
        return Ok(vec![]);
    }
    Ok(vec![inverse_call::<SubtitleEncountersPut>(
        &SubtitleCaptureRows {
            rows: old.into_iter().map(row_dto).collect(),
        },
    )?])
}

/// Brings a note's fields in line with its encounters: `examples` lists the non-primary ones;
/// with `with_primary`, the [`EXAMPLE_KEYS`] fields show the primary one (an attach leaves
/// them, so a field the user edited is kept). A nested `notes.update` only when something
/// changes (schedule and history are never touched by a field edit).
pub(super) fn sync_note(ctx: &mut CommandCtx<'_>, note_id: NoteId, with_primary: bool) -> AppResult<()> {
    let Some(note) = ctx.reads().get_note(note_id)? else {
        return Ok(());
    };
    if note.deleted_at.is_some() {
        return Ok(());
    }
    let has_examples = ctx
        .reads()
        .get_notetype(note.notetype_id)?
        .is_some_and(|nt| nt.fields.iter().any(|f| f.key.0 == EXAMPLES_FIELD));
    let rows = ctx.reads().subtitle_encounters_of_note(note_id)?;
    let old: BTreeMap<String, String> = note
        .fields
        .iter()
        .map(|(k, v)| (k.0.clone(), v.clone()))
        .collect();
    let mut fields = old.clone();
    if with_primary {
        if let Some(p) = rows.iter().find(|r| r.is_primary) {
            for k in EXAMPLE_KEYS {
                match p.facts.example_fields.get(k) {
                    Some(v) => fields.insert(k.to_owned(), v.clone()),
                    None => fields.remove(k),
                };
            }
        }
    }
    if has_examples {
        let others: Vec<&BTreeMap<String, String>> = rows
            .iter()
            .filter(|r| !r.is_primary)
            .map(|r| &r.facts.example_fields)
            .collect();
        let html = examples_html(&others);
        if html.is_empty() {
            fields.remove(EXAMPLES_FIELD);
        } else {
            fields.insert(EXAMPLES_FIELD.to_owned(), html);
        }
    }
    let same = |a: &BTreeMap<String, String>, b: &BTreeMap<String, String>| {
        let nonempty = |m: &BTreeMap<String, String>| -> Vec<(String, String)> {
            m.iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        nonempty(a) == nonempty(b)
    };
    if same(&fields, &old) {
        return Ok(());
    }
    ctx.call(&inverse_call::<NotesUpdate>(&UpdateNote {
        id: note_id,
        deck_id: None,
        fields,
        tags: note.tags.iter().map(|t| t.as_str().to_owned()).collect(),
        attributes: vec![],
    })?)?;
    Ok(())
}

// ------------------------------------------------------------------ placement

/// Where a new capture goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Placement {
    /// A new note; `review`: the word already has this note with another (or unknown) meaning.
    NewNote { review: Option<NoteId> },
    /// Another example of this note (whose card is `card_id`).
    Attach { note_id: NoteId, card_id: CardId },
}

/// The placement of a capture of `lang` + `lemma` with `definition` (normalised lang/lemma).
pub(super) fn place(reads: &dyn Reads, lang: &str, lemma: &str, definition: &str) -> AppResult<Placement> {
    let rows = reads.subtitle_captures_of(&[(lang.to_owned(), lemma.to_owned())])?;
    // Live notes of the word, in order of their first capture.
    let mut notes: Vec<(NoteId, CardId)> = Vec::new();
    for r in &rows {
        if !notes.iter().any(|(n, _)| *n == r.note_id) && note_is_live(reads, r.note_id)? {
            notes.push((r.note_id, r.card_id));
        }
    }
    for &(note_id, card_id) in &notes {
        // The note's meaning (it may have been edited) and every definition captured for it.
        let meaning = reads
            .get_note(note_id)?
            .and_then(|n| n.fields.get(&fc_core::notetype::FieldKey::new("meaning")).cloned())
            .unwrap_or_default();
        let matched = same_meaning(definition, &meaning)
            || rows
                .iter()
                .filter(|r| r.note_id == note_id)
                .filter_map(|r| r.facts.definition.as_deref())
                .any(|d| same_meaning(definition, d));
        if matched {
            return Ok(Placement::Attach { note_id, card_id });
        }
    }
    Ok(Placement::NewNote {
        review: notes.first().map(|(n, _)| *n),
    })
}

// ------------------------------------------------------------------ methods

pub(super) fn list(
    req: ListSubtitleEncounters,
    ctx: &QueryCtx<'_>,
) -> AppResult<SubtitleEncounterList> {
    let reads: &dyn Reads = ctx.tx;
    let rows = match (&req.note_id, &req.lang, &req.lemma, &req.video_id, req.needs_review) {
        (Some(n), None, None, None, None) => reads.subtitle_encounters_of_note(*n)?,
        (None, Some(l), Some(m), None, None) => reads.subtitle_captures_of(&[(
            normalize_lang(l),
            normalize_lemma(m),
        )])?,
        (None, None, None, Some(v), None) => reads.subtitle_encounters_of_video(v.trim())?,
        (None, None, None, None, Some(true)) => reads.subtitle_encounters_in_review()?,
        _ => {
            return Err(invalid(
                "selector",
                "give exactly one of: note_id, lang + lemma, video_id, needs_review: true",
            ))
        }
    };
    Ok(SubtitleEncounterList {
        encounters: rows.into_iter().map(row_dto).collect(),
    })
}

pub(super) fn put(
    req: SubtitleCaptureRows,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AffectedCount>> {
    let rows: Vec<SubtitleCaptureRecord> = req.rows.into_iter().map(row_record).collect();
    let old = put_rows(ctx, &rows)?;
    Ok(CommandOutcome {
        response: AffectedCount::default(),
        inverse: put_inverse(old)?,
        summary: summary::<SubtitleEncountersPut>(),
        events: vec![DomainEvent::SubtitleChanged],
    })
}

pub(super) fn set_primary(
    req: SubtitleEncounterRef,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SubtitleEncounterList>> {
    let r = require(ctx.reads(), &req.capture_id)?;
    require_subtitle_note(ctx, r.note_id)?;
    let mut inverse = vec![];
    if !r.is_primary {
        let rows: Vec<SubtitleCaptureRecord> = ctx
            .reads()
            .subtitle_encounters_of_note(r.note_id)?
            .into_iter()
            .map(|mut x| {
                x.is_primary = x.capture_id == r.capture_id;
                x
            })
            .collect();
        inverse = put_inverse(put_rows(ctx, &rows)?)?;
        sync_note(ctx, r.note_id, true)?;
    }
    Ok(CommandOutcome {
        response: list_of(ctx.reads(), r.note_id)?,
        inverse,
        summary: summary::<SubtitleEncountersSetPrimary>(),
        events: vec![DomainEvent::SubtitleChanged],
    })
}

pub(super) fn move_encounter(
    req: MoveSubtitleEncounter,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SubtitleEncounterList>> {
    let r = require(ctx.reads(), &req.capture_id)?;
    let from = require_subtitle_note(ctx, r.note_id)?;
    let siblings = ctx.reads().subtitle_encounters_of_note(r.note_id)?;
    if siblings.len() < 2 {
        return Err(invalid(
            "last_encounter",
            "the only encounter of a note cannot move; merge the notes instead",
        ));
    }
    let (to, to_card, primary) = match req.to_note_id {
        Some(to) if to == r.note_id => {
            return Err(invalid("same_note", "the encounter is already in this note"))
        }
        Some(to) => {
            require_subtitle_note(ctx, to)?;
            let card = card_of(ctx.reads(), to)?;
            let empty = ctx.reads().subtitle_encounters_of_note(to)?.is_empty();
            (to, card, empty)
        }
        None => {
            // A new note: the old note's word, the encounter's own meaning and scene.
            let mut fields: BTreeMap<String, String> = from
                .fields
                .iter()
                .filter(|(k, _)| matches!(k.0.as_str(), "word" | "meaning" | "pos"))
                .map(|(k, v)| (k.0.clone(), v.clone()))
                .collect();
            if let Some(d) = r.facts.definition.as_deref() {
                fields.insert("meaning".into(), fc_core::subtitle::rich_text(d));
            }
            if let Some(p) = r.facts.pos.as_deref() {
                fields.insert("pos".into(), fc_core::subtitle::collapse_spaces(p));
            }
            fields.extend(r.facts.example_fields.clone());
            let v = ctx.call(&inverse_call::<NotesCreate>(&CreateNote {
                notetype_id: from.notetype_id,
                deck_id: from.home_deck_id,
                fields,
                tags: from.tags.iter().map(|t| t.as_str().to_owned()).collect(),
                attributes: vec![],
                allow_duplicate: Some(true),
            })?)?;
            let note: NoteDto = from_json(&v)?;
            let card = note
                .cards
                .first()
                .map(|c| c.id)
                .ok_or_else(|| AppError::api(ErrorCode::Internal, "the subtitle note has no card"))?;
            ctx.tx.set_note_origin(&fc_storage::repo::NoteOriginRecord {
                note_id: note.id,
                origin: fc_core::query::NoteOrigin::Subtitle,
                source: r.facts.title.clone().unwrap_or_default(),
                site: r.facts.site.clone().unwrap_or_default(),
                video_id: r.facts.video_id.clone().unwrap_or_default(),
                created_at: ctx.now,
            })?;
            (note.id, card, true)
        }
    };
    let mut changes = vec![SubtitleCaptureRecord {
        note_id: to,
        card_id: to_card,
        is_primary: primary,
        review_note_id: None,
        ..r.clone()
    }];
    if r.is_primary {
        // The next encounter of the old note shows on its card.
        if let Some(next) = siblings.iter().find(|s| s.capture_id != r.capture_id) {
            changes.push(SubtitleCaptureRecord {
                is_primary: true,
                ..next.clone()
            });
        }
    }
    let inverse = put_inverse(put_rows(ctx, &changes)?)?;
    sync_note(ctx, r.note_id, r.is_primary)?;
    sync_note(ctx, to, primary)?;
    Ok(CommandOutcome {
        response: list_of(ctx.reads(), to)?,
        inverse,
        summary: summary::<SubtitleEncountersMove>(),
        events: vec![DomainEvent::SubtitleChanged],
    })
}

/// Reviews of a note's cards (trashed cards too).
fn reviews_of(reads: &dyn Reads, note: NoteId) -> AppResult<usize> {
    let cards: Vec<CardId> = reads
        .list_cards(&CardFilter {
            note_ids: Some(vec![note]),
            include_deleted: true,
            ..Default::default()
        })?
        .into_iter()
        .map(|c| c.id)
        .collect();
    Ok(reads.review_logs_for_cards(&cards)?.len())
}

/// Merges two notes; returns the response and the inverse of the encounter rows.
fn merge(
    ctx: &mut CommandCtx<'_>,
    a: NoteId,
    b: NoteId,
) -> AppResult<(MergedSubtitleNotes, Vec<InvokeRequest>)> {
    if a == b {
        return Err(invalid("same_note", "a note cannot be merged with itself"));
    }
    let na = require_subtitle_note(ctx, a)?;
    let nb = require_subtitle_note(ctx, b)?;
    let (ra, rb) = (reviews_of(ctx.reads(), a)?, reviews_of(ctx.reads(), b)?);
    // More history wins; a tie keeps the older note (then the first named).
    let a_wins = ra > rb || (ra == rb && (na.created_at, na.id) <= (nb.created_at, nb.id));
    let (kept, archived, archived_reviews) = if a_wins { (a, b, rb) } else { (b, a, ra) };
    let card = card_of(ctx.reads(), kept)?;
    let kept_rows = ctx.reads().subtitle_encounters_of_note(kept)?;
    let kept_has_primary = kept_rows.iter().any(|r| r.is_primary);
    let moving = ctx.reads().subtitle_encounters_of_note(archived)?;
    let moved = moving.len();
    let mut changes: Vec<SubtitleCaptureRecord> = Vec::new();
    for (i, r) in moving.into_iter().enumerate() {
        changes.push(SubtitleCaptureRecord {
            note_id: kept,
            card_id: card,
            is_primary: !kept_has_primary && i == 0,
            review_note_id: None,
            ..r
        });
    }
    // The decision is made: flags between the two notes go; flags on the archived note now
    // point to the kept one.
    for r in ctx.reads().subtitle_encounters_in_review()? {
        let pair = (r.note_id == kept && r.review_note_id == Some(archived))
            || (r.note_id == archived && r.review_note_id == Some(kept));
        if changes.iter().any(|c| c.capture_id == r.capture_id) {
            continue;
        }
        if pair {
            changes.push(SubtitleCaptureRecord {
                review_note_id: None,
                ..r
            });
        } else if r.review_note_id == Some(archived) {
            changes.push(SubtitleCaptureRecord {
                review_note_id: Some(kept),
                ..r
            });
        }
    }
    let inverse = put_inverse(put_rows(ctx, &changes)?)?;
    sync_note(ctx, kept, !kept_has_primary)?;
    ctx.call(&inverse_call::<NotesDelete>(&NoteIds {
        ids: vec![archived],
    })?)?;
    let encounters = list_of(ctx.reads(), kept)?.encounters;
    Ok((
        MergedSubtitleNotes {
            kept_note_id: kept,
            archived_note_id: archived,
            moved: u32::try_from(moved).unwrap_or(u32::MAX),
            archived_reviews: u32::try_from(archived_reviews).unwrap_or(u32::MAX),
            encounters,
        },
        inverse,
    ))
}

pub(super) fn merge_notes(
    req: MergeSubtitleNotes,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<MergedSubtitleNotes>> {
    let (response, inverse) = merge(ctx, req.a, req.b)?;
    Ok(CommandOutcome {
        response,
        inverse,
        summary: summary::<SubtitleEncountersMergeNotes>(),
        events: vec![DomainEvent::SubtitleChanged],
    })
}

pub(super) fn resolve(
    req: ResolveSubtitleEncounter,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SubtitleEncounterList>> {
    let r = require(ctx.reads(), &req.capture_id)?;
    let Some(other) = r.review_note_id else {
        return Err(invalid("not_in_review", "this encounter is not waiting for a decision"));
    };
    let other_live = note_is_live(ctx.reads(), other)?;
    let (response, inverse) = if req.same_meaning && other_live {
        let (m, inverse) = merge(ctx, r.note_id, other)?;
        (
            SubtitleEncounterList {
                encounters: m.encounters,
            },
            inverse,
        )
    } else {
        let old = put_rows(
            ctx,
            &[SubtitleCaptureRecord {
                review_note_id: None,
                ..r.clone()
            }],
        )?;
        (list_of(ctx.reads(), r.note_id)?, put_inverse(old)?)
    };
    Ok(CommandOutcome {
        response,
        inverse,
        summary: summary::<SubtitleEncountersResolve>(),
        events: vec![DomainEvent::SubtitleChanged],
    })
}
