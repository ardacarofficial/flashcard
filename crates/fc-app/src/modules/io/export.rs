//! `io.export.start` and the `io.export` job: read one snapshot into a [`Pack`], write the
//! chosen format to a file.

use super::{interop_err, text_options, write_file, JOB_EXPORT};
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome};
use crate::jobs::{enqueue, JobContext, JobHandler};
use crate::support::decks::DeckIndex;
use crate::support::{invalid, summary};
use fc_api::v1::io::*;
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::IoExportStart;
use fc_api::ErrorCode;
use fc_core::attribute::{AttributeValue, EntityRef};
use fc_core::card::SchedulingState;
use fc_core::{AttributeDefId, CardId, DeckId, NoteId, NoteTypeId};
use fc_interop::json::{JsonDocument, JsonNote};
use fc_interop::media::{hash_of, references};
use fc_interop::pack::{MediaFile, Pack, PackValue, ReviewEntry};
use fc_interop::text::{write_delimited, TextOptions, NEWLINE};
use fc_interop::{anki, fcpack, json};
use fc_storage::repo::CardFilter;
use fc_storage::Reads;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Validates and enqueues an export.
pub(super) fn start(req: StartExport, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<JobDto>> {
    if req.path.trim().is_empty() {
        return Err(invalid("path is empty"));
    }
    if let Some(d) = req.deck_id {
        DeckIndex::load(ctx.reads())?.require_live(d)?;
    }
    if let Some(o) = &req.text_options {
        text_options(o).validate().map_err(interop_err)?;
    }
    let payload = serde_json::to_value(&req)
        .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
    let job = enqueue(ctx, JOB_EXPORT, payload)?;
    Ok(CommandOutcome {
        response: job,
        inverse: Vec::new(),
        summary: summary::<IoExportStart>(),
        events: Vec::new(),
    })
}

/// Everything under `deck` (with sub-decks; `None` = whole collection) as a [`Pack`]: live
/// decks, cards, their notes and note types, classification definitions and values, review
/// history (`include_history`, else cards are reset to new) and referenced media.
pub(crate) fn collect_pack(
    reads: &dyn Reads,
    deck: Option<DeckId>,
    include_history: bool,
) -> AppResult<Pack> {
    let index = DeckIndex::load(reads)?;
    let deck_ids: Vec<DeckId> = match deck {
        Some(d) => {
            index.require_live(d)?;
            index.live_subtree(d)
        }
        None => index.live().map(|d| d.id).collect(),
    };
    let in_pack: HashSet<DeckId> = deck_ids.iter().copied().collect();
    let mut pack = Pack::default();
    for id in &deck_ids {
        if let Some(d) = index.get(*id) {
            let mut d = d.clone();
            if d.parent_id.is_some_and(|p| !in_pack.contains(&p)) {
                d.parent_id = None;
            }
            pack.decks.push(d);
        }
    }
    let mut cards = reads.list_cards(&CardFilter {
        deck_ids: Some(deck_ids.clone()),
        note_ids: None,
        include_deleted: false,
    })?;
    cards.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
    let note_ids: BTreeSet<NoteId> = cards.iter().map(|c| c.note_id).collect();
    let mut notetype_ids: BTreeSet<NoteTypeId> = BTreeSet::new();
    let mut media: BTreeSet<String> = BTreeSet::new();
    let mut def_ids: BTreeSet<AttributeDefId> = BTreeSet::new();
    for id in &note_ids {
        let Some(note) = reads.get_note(*id)? else {
            continue;
        };
        if note.deleted_at.is_some() {
            continue;
        }
        notetype_ids.insert(note.notetype_id);
        for v in note.fields.values() {
            for r in references(v) {
                if let Some(h) = hash_of(&r) {
                    media.insert(h.to_owned());
                }
            }
        }
        for (attr, value) in reads.get_attribute_values(EntityRef::Note(note.id))? {
            def_ids.insert(attr);
            pack.attribute_values.push(PackValue {
                attr_id: attr,
                entity: EntityRef::Note(note.id),
                value,
            });
        }
        pack.notes.push(note);
    }
    let live_notes: HashSet<NoteId> = pack.notes.iter().map(|n| n.id).collect();
    cards.retain(|c| live_notes.contains(&c.note_id));
    for c in &mut cards {
        for (attr, value) in reads.get_attribute_values(EntityRef::Card(c.id))? {
            def_ids.insert(attr);
            pack.attribute_values.push(PackValue {
                attr_id: attr,
                entity: EntityRef::Card(c.id),
                value,
            });
        }
        if !include_history {
            c.scheduling = SchedulingState::NEW;
            c.suspended = false;
            c.buried_until = None;
        }
    }
    for id in notetype_ids {
        if let Some(nt) = reads.get_notetype(id)? {
            pack.notetypes.push(nt);
        }
    }
    for def in reads.list_attribute_defs(false)? {
        let scoped_here = def.scope_deck_id.is_some_and(|s| in_pack.contains(&s));
        if def_ids.contains(&def.id) || scoped_here {
            pack.attribute_defs.push(def);
        }
    }
    if include_history {
        let ids: Vec<CardId> = cards.iter().map(|c| c.id).collect();
        for chunk in ids.chunks(500) {
            for r in reads.review_logs_for_cards(chunk)? {
                pack.review_logs.push(ReviewEntry {
                    id: r.id,
                    card_id: r.card_id,
                    session_id: r.session_id,
                    rating: r.rating,
                    state_before: r.state_before,
                    state_after: r.state_after,
                    scheduled_days: i64::from(r.scheduled_days),
                    elapsed_days: i64::from(r.elapsed_days),
                    duration_ms: i64::from(r.duration_ms),
                    answer_duration_ms: r.answer_duration_ms.map(i64::from),
                    hints_used: r.hints_used,
                    mode: r.mode,
                    stability_after: r.stability_after,
                    difficulty_after: r.difficulty_after,
                    created_at: r.created_at,
                });
            }
        }
    }
    pack.cards = cards;
    for hash in media {
        let store = super::media::media_store();
        let (Some(record), Some(bytes)) = (store.record(reads, &hash)?, store.bytes(reads, &hash)?)
        else {
            continue;
        };
        pack.media.push(MediaFile {
            hash,
            mime: record.mime,
            name: record.original_name,
            bytes,
        });
    }
    Ok(pack)
}

/// Text rows: note type fields in order, then the tags.
fn text_rows(pack: &Pack) -> Vec<Vec<String>> {
    pack.notes
        .iter()
        .filter_map(|n| {
            let nt = pack.notetypes.iter().find(|t| t.id == n.notetype_id)?;
            let mut row: Vec<String> = nt
                .fields
                .iter()
                .map(|f| n.fields.get(&f.key).cloned().unwrap_or_default())
                .collect();
            row.push(
                n.tags
                    .iter()
                    .map(|t| t.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            Some(row)
        })
        .collect()
}

/// Our JSON: deck path, note type name, fields by key, tags, classifications by key (option
/// labels).
fn json_doc(pack: &Pack, index: &DeckIndex) -> JsonDocument {
    let notes = pack
        .notes
        .iter()
        .map(|n| {
            let mut attributes = BTreeMap::new();
            for v in pack
                .attribute_values
                .iter()
                .filter(|v| v.entity == EntityRef::Note(n.id))
            {
                let Some(def) = pack.attribute_defs.iter().find(|d| d.id == v.attr_id) else {
                    continue;
                };
                let label = |o| def.option(o).map(|x| x.label.clone()).unwrap_or_default();
                let value = match &v.value {
                    AttributeValue::Choice(o) => Value::from(label(*o)),
                    AttributeValue::Choices(list) => {
                        Value::from(list.iter().map(|o| label(*o)).collect::<Vec<_>>())
                    }
                    AttributeValue::Number(x) => Value::from(*x),
                    AttributeValue::Text(t) | AttributeValue::Date(t) => Value::from(t.clone()),
                    AttributeValue::Bool(b) => Value::from(*b),
                };
                attributes.insert(def.key.clone(), value);
            }
            JsonNote {
                deck: Some(index.path(n.home_deck_id)),
                notetype: pack
                    .notetypes
                    .iter()
                    .find(|t| t.id == n.notetype_id)
                    .map(|t| t.name.clone()),
                fields: n
                    .fields
                    .iter()
                    .map(|(k, v)| (k.0.clone(), v.clone()))
                    .collect(),
                tags: n.tags.iter().map(|t| t.as_str().to_owned()).collect(),
                attributes,
            }
        })
        .collect();
    JsonDocument::new(notes)
}

/// The `io.export` job.
pub(crate) struct ExportJob;

impl JobHandler for ExportJob {
    fn kind(&self) -> &'static str {
        JOB_EXPORT
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let req: StartExport = serde_json::from_value(payload)
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
        let history = req.include_history.unwrap_or(true);
        ctx.report(Some(0.1), "io.progress.collecting");
        let (bytes, pack) = {
            let tx = ctx.read()?;
            let deck = if req.format == ExportFormat::Colpkg {
                None
            } else {
                req.deck_id
            };
            let pack = collect_pack(&*tx, deck, history)?;
            ctx.cancel.check()?;
            ctx.report(Some(0.5), "io.progress.writing_file");
            let bytes = match req.format {
                ExportFormat::Text => {
                    let opts = req
                        .text_options
                        .as_ref()
                        .map(text_options)
                        .unwrap_or_else(|| TextOptions::new("\t", NEWLINE));
                    write_delimited(&text_rows(&pack), &opts).into_bytes()
                }
                ExportFormat::Json => {
                    let index = DeckIndex::load(&*tx)?;
                    json::write(&json_doc(&pack, &index))
                        .map_err(interop_err)?
                        .into_bytes()
                }
                ExportFormat::Apkg | ExportFormat::Colpkg => {
                    let col = anki::from_pack(&pack, history, ctx.now());
                    anki::write_package(&col, ctx.now()).map_err(interop_err)?
                }
                ExportFormat::Fcpack => {
                    let name = req
                        .deck_id
                        .and_then(|d| pack.decks.iter().find(|x| x.id == d))
                        .map_or_else(|| "Collection".to_owned(), |d| d.name.clone());
                    let manifest = fcpack::Manifest::for_pack(&pack, &name, history, ctx.now());
                    fcpack::write(&pack, &manifest).map_err(interop_err)?
                }
            };
            (bytes, pack)
        };
        ctx.cancel.check()?;
        write_file(&req.path, &bytes)?;
        let report = ExportReport {
            path: req.path,
            bytes: bytes.len() as u64,
            notes: u32::try_from(pack.notes.len()).unwrap_or(u32::MAX),
            cards: u32::try_from(pack.cards.len()).unwrap_or(u32::MAX),
            media: u32::try_from(pack.media.len()).unwrap_or(u32::MAX),
        };
        serde_json::to_value(report).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
    }
}
