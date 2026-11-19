//! `io.import.apply`: writes the next chunk of a staged import, and the inverse/compensation
//! commands `io.import.trash` / `io.import.untrash` / `io.import.discard`.
//!
//! **Chunks.** A large import must not hold the single writer for seconds (study and editing
//! would time out), so the `io.import` job calls `io.import.apply` repeatedly: each call
//! writes at most [`CHUNK_ROWS`] units (rows, notes, cards, values, review rows; media also
//! by bytes) in its own transaction, keeps its progress in [`ApplyState`] and returns. The
//! job records all chunks as **one undo step** ([`crate::app::UndoGroup`]): each chunk's
//! inverse trashes exactly what that chunk created ([`ImportedIds`]); updates of existing
//! notes (`duplicates: update`) and options created for unknown labels go through nested
//! `notes.update` / `attributes.option.add` calls with their own inverses.
//!
//! **Cancel / failure.** The cancel token is checked as rows are written; a cancelled chunk
//! rolls back, and the job then runs `io.import.discard` with the inverses of the chunks
//! already committed: it hard-deletes what they created (nothing lands in the trash) and
//! reverts their nested calls, without recording an undo step.

use super::import::{PackImport, Staged};
use super::media::media_store;
use super::rows::{parse_value, AttrRef, ParsedValue, RowSpec, RowsImport};
use super::Staging;
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandHandler, CommandOutcome};
use crate::jobs::CancelToken;
use crate::support::decks::DeckIndex;
use crate::support::notes::{derive, insert_note, require_notetype};
use crate::support::summary;
use fc_api::v1::attributes::{AddAttributeOption, AttributeAssignment};
use fc_api::v1::common::{AffectedCount, Empty, I18nMessage};
use fc_api::v1::events::DomainEvent;
use fc_api::v1::io::*;
use fc_api::v1::methods::{
    AttributesOptionAdd, IoImportApply, IoImportDiscard, IoImportTrash, IoImportUntrash,
    NotesUpdate,
};
use fc_api::v1::notes::UpdateNote;
use fc_api::ApiMethod;
use fc_core::attribute::{
    effective_attribute_defs, prepare_attribute_values, validate_attribute_value, AttributeDef,
    AttributeTarget, AttributeValue, EntityRef,
};
use fc_core::card::{standard_card_specs, Card, SchedulingState};
use fc_core::deck::{normalize_deck_segment, Deck, DeckPath};
use fc_core::note::{normalize_note_fields, FieldValues, Note};
use fc_core::notetype::{validate_notetype, FieldKey, NoteType};
use fc_core::tag::{dedup_tags, TagName};
use fc_core::{AttributeDefId, AttributeOptionId, CardId, DeckId, NoteId, NoteTypeId, Timestamp};
use fc_interop::media::guess_mime;
use fc_storage::repo::{MediaRecord, PurgeRows, ReviewLogRecord};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Units (rows, notes, cards, values, review rows, media files) written per chunk.
pub(crate) const CHUNK_ROWS: usize = 500;
/// Media bytes written per chunk (a chunk ends after the file that crosses it).
const CHUNK_MEDIA_BYTES: usize = 32 * 1024 * 1024;
/// How often (units) the cancel token is checked.
const CHECK_EVERY: usize = 32;

/// Where an import is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Rows import: next row.
    Rows(usize),
    /// Package: next media file.
    Media(usize),
    /// Package: decks, note types, classification definitions (one chunk).
    Structure,
    /// Package: next note.
    Notes(usize),
    /// Package: next card.
    Cards(usize),
    /// Package: next classification value.
    Values(usize),
    /// Package: next review row.
    Reviews(usize),
    /// Finished.
    Done,
}

/// A staged import and its progress between chunks (kept in [`Staging`], in memory).
pub(crate) struct ApplyState {
    staged: Staged,
    step: Step,
    /// Units per chunk.
    chunk: usize,
    /// Totals so far.
    report: ImportReport,
    /// Rows: note types resolved by reference.
    notetype_refs: HashMap<String, Option<NoteType>>,
    /// Package: id maps from the package to the collection.
    deck_map: HashMap<DeckId, DeckId>,
    notetypes: HashMap<NoteTypeId, NoteType>,
    def_map: HashMap<AttributeDefId, AttributeDefId>,
    option_map: HashMap<AttributeOptionId, AttributeOptionId>,
    note_map: HashMap<NoteId, NoteId>,
    card_map: HashMap<CardId, CardId>,
    used_card_ids: HashSet<CardId>,
    /// Package: deck of each note's first card (home deck fallback).
    first_card_deck: HashMap<NoteId, DeckId>,
}

impl ApplyState {
    /// Fresh state for `staged`, writing `chunk` units per call.
    pub(crate) fn new(staged: Staged, chunk: usize) -> Self {
        let step = match &staged {
            Staged::Rows(_) => Step::Rows(0),
            Staged::Pack(_) => Step::Media(0),
        };
        Self {
            staged,
            step,
            chunk: chunk.max(1),
            report: ImportReport::default(),
            notetype_refs: HashMap::new(),
            deck_map: HashMap::new(),
            notetypes: HashMap::new(),
            def_map: HashMap::new(),
            option_map: HashMap::new(),
            note_map: HashMap::new(),
            card_map: HashMap::new(),
            used_card_ids: HashSet::new(),
            first_card_deck: HashMap::new(),
        }
    }

    /// Whether everything is written.
    pub(crate) fn is_done(&self) -> bool {
        self.step == Step::Done
    }

    /// Rough fraction written (for progress events).
    pub(crate) fn progress(&self) -> f32 {
        let frac = |i: usize, n: usize| if n == 0 { 1.0 } else { i as f32 / n as f32 };
        match (&self.staged, self.step) {
            (_, Step::Done) => 1.0,
            (Staged::Rows(r), Step::Rows(i)) => frac(i, r.rows.len()),
            (Staged::Pack(p), step) => {
                let p = &p.pack;
                let total = (p.media.len() + p.notes.len() + p.cards.len()).max(1) as f32;
                let done = match step {
                    Step::Media(i) => i,
                    Step::Structure => p.media.len(),
                    Step::Notes(i) => p.media.len() + i,
                    Step::Cards(i) => p.media.len() + p.notes.len() + i,
                    _ => p.media.len() + p.notes.len() + p.cards.len(),
                };
                done as f32 / total
            }
            _ => 0.0,
        }
    }
}

/// Handler of `io.import.apply`.
pub(crate) struct ApplyHandler(pub Arc<Staging>);

impl CommandHandler<IoImportApply> for ApplyHandler {
    fn execute(
        &self,
        req: StagedJob,
        ctx: &mut CommandCtx<'_>,
    ) -> AppResult<CommandOutcome<ImportReport>> {
        // Taken out for the chunk; put back only if the chunk commits work and more is left
        // (a failed chunk rolls back and its half-advanced state is dropped).
        let (mut state, cancel) = self.0.take_import(req.job_id)?;
        let mut w = Writer {
            created: ImportedIds::default(),
            cancel: cancel.clone(),
            spent: 0,
            media_bytes: 0,
            budget: state.chunk,
        };
        w.run(ctx, &mut state)?;
        w.cancel.check()?;
        let index = DeckIndex::load(ctx.reads())?;
        let mut events: Vec<DomainEvent> = w
            .created
            .decks
            .iter()
            .filter_map(|d| index.get(*d))
            .map(|d| DomainEvent::DeckCreated { deck: index.dto(d) })
            .collect();
        if state.is_done() {
            events.push(DomainEvent::Imported {
                job_id: req.job_id,
                notes: state.report.added_notes + state.report.updated_notes,
            });
        }
        let inverse = if w.created.is_empty() {
            Vec::new()
        } else {
            vec![inverse_call::<IoImportTrash>(&w.created)?]
        };
        let response = state.report.clone();
        let added = w.created.notes.len();
        if !state.is_done() {
            self.0.put_import(req.job_id, state, cancel);
        }
        Ok(CommandOutcome {
            summary: summary::<IoImportApply>().param("count", added),
            response,
            inverse,
            events,
        })
    }
}

fn issue(row: u32, key: &str) -> ImportIssue {
    ImportIssue {
        row,
        message: I18nMessage::key(key.to_owned()),
    }
}

/// Writes one chunk.
struct Writer {
    /// Created by this chunk (its inverse).
    created: ImportedIds,
    cancel: CancelToken,
    /// Units written in this chunk.
    spent: usize,
    media_bytes: usize,
    budget: usize,
}

impl Writer {
    /// Counts one unit; checks the cancel token every [`CHECK_EVERY`] units.
    fn tick(&mut self) -> AppResult<()> {
        if self.spent % CHECK_EVERY == 0 {
            self.cancel.check()?;
        }
        self.spent += 1;
        Ok(())
    }

    fn full(&self) -> bool {
        self.spent >= self.budget || self.media_bytes >= CHUNK_MEDIA_BYTES
    }

    fn run(&mut self, ctx: &mut CommandCtx<'_>, st: &mut ApplyState) -> AppResult<()> {
        let staged = std::mem::replace(
            &mut st.staged,
            Staged::Rows(RowsImport {
                rows: Vec::new(),
                deck_id: None,
                notetype_id: fc_core::notetype::builtin::CLASSIC_ID,
                duplicates: DuplicatePolicy::Skip,
                create_missing_options: false,
            }),
        );
        let result = match &staged {
            Staged::Rows(r) => self.rows(ctx, st, r),
            Staged::Pack(p) => self.pack(ctx, st, p),
        };
        st.staged = staged;
        result
    }

    /// Finds or creates the deck path `segments` under `parent`; a created leaf takes
    /// `leaf` (id if free, description, icon, colour).
    fn ensure_deck(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        report: &mut ImportReport,
        parent: Option<DeckId>,
        segments: &[String],
        leaf: Option<&Deck>,
    ) -> AppResult<DeckId> {
        let mut parent = parent;
        for (i, raw) in segments.iter().enumerate() {
            let name = normalize_deck_segment(raw)?;
            if let Some(d) = ctx.reads().find_child_deck(parent, &name)? {
                parent = Some(d.id);
                continue;
            }
            let is_leaf = i + 1 == segments.len();
            let template = leaf.filter(|_| is_leaf);
            let id = match template {
                Some(t) if ctx.reads().get_deck(t.id)?.is_none() => t.id,
                _ => DeckId::generate(ctx.ids),
            };
            let position = DeckIndex::load(ctx.reads())?.next_position(parent, None);
            let deck = Deck {
                id,
                parent_id: parent,
                name,
                description: template.map(|t| t.description.clone()).unwrap_or_default(),
                icon: template.and_then(|t| t.icon.clone()),
                color: template.and_then(|t| t.color.clone()),
                preset_id: None,
                position,
                created_at: ctx.now,
                updated_at: ctx.now,
                deleted_at: None,
            };
            ctx.tx.insert_deck(&deck)?;
            self.created.decks.push(id);
            report.added_decks += 1;
            parent = Some(id);
        }
        parent.ok_or_else(|| crate::support::invalid("empty deck path"))
    }

    /// Adds `label` to `def` (nested `attributes.option.add`, its own undo entry).
    fn add_option(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        report: &mut ImportReport,
        def: AttributeDefId,
        label: &str,
        color: Option<String>,
        id: Option<AttributeOptionId>,
    ) -> AppResult<()> {
        let call = inverse_call::<AttributesOptionAdd>(&AddAttributeOption {
            attr_id: def,
            label: label.to_owned(),
            color,
            after: None,
            id,
        })?;
        ctx.call(&call)?;
        report.added_options += 1;
        Ok(())
    }

    /// `duplicates: update`: nested `notes.update` (its own undo entry).
    fn update_note(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        report: &mut ImportReport,
        existing: NoteId,
        incoming: &Note,
        attributes: &[(AttributeDefId, Option<AttributeValue>)],
    ) -> AppResult<()> {
        let call = inverse_call::<NotesUpdate>(&UpdateNote {
            id: existing,
            deck_id: None,
            fields: incoming
                .fields
                .iter()
                .map(|(k, v)| (k.0.clone(), v.clone()))
                .collect(),
            tags: incoming.tags.iter().map(|t| t.as_str().to_owned()).collect(),
            attributes: attributes
                .iter()
                .map(|(id, v)| AttributeAssignment {
                    attr_id: *id,
                    value: v.clone(),
                })
                .collect(),
        })?;
        ctx.call(&call)?;
        report.updated_notes += 1;
        Ok(())
    }

    // ------------------------------------------------------------------ rows

    fn rows(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        st: &mut ApplyState,
        imp: &RowsImport,
    ) -> AppResult<()> {
        let default_nt = require_notetype(ctx.reads(), imp.notetype_id)?;
        if let Some(d) = imp.deck_id {
            DeckIndex::load(ctx.reads())?.require_live(d)?;
        }
        let Step::Rows(mut i) = st.step else {
            return Ok(());
        };
        while i < imp.rows.len() && !self.full() {
            self.tick()?;
            let row = &imp.rows[i];
            i += 1;
            let nt = match &row.notetype {
                None => default_nt.clone(),
                Some(r) => {
                    if !st.notetype_refs.contains_key(r) {
                        let found = find_notetype(ctx, r)?;
                        st.notetype_refs.insert(r.clone(), found);
                    }
                    match st.notetype_refs.get(r).cloned().flatten() {
                        Some(nt) => nt,
                        None => {
                            st.report
                                .issues
                                .push(issue(row.row, "io.issue.unknown_notetype"));
                            continue;
                        }
                    }
                }
            };
            self.row(ctx, &mut st.report, imp, &nt, row)?;
        }
        st.step = if i >= imp.rows.len() {
            Step::Done
        } else {
            Step::Rows(i)
        };
        Ok(())
    }

    fn row(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        report: &mut ImportReport,
        imp: &RowsImport,
        nt: &NoteType,
        row: &RowSpec,
    ) -> AppResult<()> {
        let specs = standard_card_specs(nt);
        if nt.deleted_at.is_some() || specs.is_empty() {
            report.issues.push(issue(row.row, "io.issue.notetype_no_cards"));
            return Ok(());
        }
        let deck = match (&row.deck, imp.deck_id) {
            (Some(path), _) => match DeckPath::parse(path) {
                Ok(p) => self.ensure_deck(ctx, report, None, p.segments(), None)?,
                Err(_) => {
                    report.issues.push(issue(row.row, "io.issue.bad_deck"));
                    return Ok(());
                }
            },
            (None, Some(d)) => d,
            (None, None) => {
                report.issues.push(issue(row.row, "io.issue.no_deck"));
                return Ok(());
            }
        };
        // Fields by key or (case-insensitive) name.
        let mut values = FieldValues::new();
        for (k, v) in &row.fields {
            let wanted = k.to_lowercase();
            let resolved = nt
                .field(&FieldKey::new(k.clone()))
                .or_else(|| {
                    nt.fields.iter().find(|f| {
                        f.key.0.to_lowercase() == wanted || f.name.to_lowercase() == wanted
                    })
                })
                .map(|f| f.key.clone());
            match resolved {
                Some(key) => {
                    values.insert(key, v.clone());
                }
                None => report
                    .issues
                    .push(issue(row.row, "io.issue.unknown_field").tap_param("field", k)),
            }
        }
        let fields = match normalize_note_fields(nt, &values) {
            Ok(f) => f,
            Err(_) => {
                report.issues.push(issue(row.row, "io.issue.invalid_fields"));
                return Ok(());
            }
        };
        let mut tags = Vec::new();
        for t in &row.tags {
            match TagName::parse(t) {
                Ok(t) => tags.push(t),
                Err(_) => report
                    .issues
                    .push(issue(row.row, "io.issue.bad_tag").tap_param("tag", t)),
            }
        }
        let tags = dedup_tags(tags);

        // Classifications applicable to the deck (note level).
        let provided = self.row_attributes(ctx, report, imp, deck, row)?;

        let mut note = Note {
            id: NoteId::generate(ctx.ids),
            notetype_id: nt.id,
            home_deck_id: deck,
            fields,
            sort_field: String::new(),
            checksum: String::new(),
            tags,
            created_at: ctx.now,
            updated_at: ctx.now,
            deleted_at: None,
        };
        derive(nt, &mut note);
        if let Some(existing) = ctx
            .reads()
            .find_notes_by_checksum(nt.id, &note.checksum)?
            .first()
            .copied()
        {
            match imp.duplicates {
                DuplicatePolicy::Skip => {
                    report.skipped_duplicates += 1;
                    return Ok(());
                }
                DuplicatePolicy::Update => {
                    self.update_note(ctx, report, existing, &note, &provided)?;
                    return Ok(());
                }
                DuplicatePolicy::Add => {}
            }
        }
        let index = DeckIndex::load(ctx.reads())?;
        let ancestry = index.ancestry(deck)?;
        let defs = ctx.reads().list_attribute_defs(false)?;
        let effective = effective_attribute_defs(&ancestry, &defs);
        let values = match prepare_attribute_values(&effective, AttributeTarget::Note, &provided)
        {
            Ok(v) => v,
            Err(_) => {
                report
                    .issues
                    .push(issue(row.row, "io.issue.attribute_required"));
                return Ok(());
            }
        };
        insert_note(ctx, nt, &note)?;
        self.created.notes.push(note.id);
        report.added_notes += 1;
        for spec in specs {
            let card = Card::new(CardId::generate(ctx.ids), note.id, deck, spec, ctx.now);
            ctx.tx.insert_card(&card)?;
            self.created.cards.push(card.id);
            report.added_cards += 1;
        }
        let values: Vec<_> = values.into_iter().map(|(id, v)| (id, Some(v))).collect();
        ctx.tx
            .set_attribute_values(EntityRef::Note(note.id), &values)?;
        Ok(())
    }

    /// Resolves a row's classification values against the deck's note-level definitions,
    /// creating missing options when allowed. Unusable values become issues.
    fn row_attributes(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        report: &mut ImportReport,
        imp: &RowsImport,
        deck: DeckId,
        row: &RowSpec,
    ) -> AppResult<Vec<(AttributeDefId, Option<AttributeValue>)>> {
        let mut out = Vec::new();
        for (aref, text) in &row.attrs {
            let index = DeckIndex::load(ctx.reads())?;
            let ancestry = index.ancestry(deck)?;
            let defs = ctx.reads().list_attribute_defs(false)?;
            let effective: Vec<AttributeDef> = effective_attribute_defs(&ancestry, &defs)
                .into_iter()
                .map(|e| e.def.clone())
                .filter(|d| d.target == AttributeTarget::Note)
                .collect();
            let def = effective.into_iter().find(|d| match aref {
                AttrRef::Id(id) => d.id == *id,
                AttrRef::Name(n) => d.key == *n || d.name.to_lowercase() == n.to_lowercase(),
            });
            let Some(mut def) = def else {
                report
                    .issues
                    .push(issue(row.row, "io.issue.attribute_not_applicable"));
                continue;
            };
            let mut parsed = parse_value(&def, text);
            if let ParsedValue::MissingLabels(labels) = &parsed {
                if imp.create_missing_options {
                    for l in labels.clone() {
                        self.add_option(ctx, report, def.id, &l, None, None)?;
                    }
                    if let Some(fresh) = ctx.reads().get_attribute_def(def.id)? {
                        def = fresh;
                    }
                    parsed = parse_value(&def, text);
                }
            }
            match parsed {
                ParsedValue::Empty => {}
                ParsedValue::Value(v) => match validate_attribute_value(&def, &v) {
                    Ok(v) => out.push((def.id, Some(v))),
                    Err(_) => report.issues.push(
                        issue(row.row, "io.issue.attribute_invalid").tap_param("value", text),
                    ),
                },
                ParsedValue::MissingLabels(_) => report.issues.push(
                    issue(row.row, "io.issue.attribute_unknown_option").tap_param("value", text),
                ),
                ParsedValue::Invalid => report.issues.push(
                    issue(row.row, "io.issue.attribute_invalid").tap_param("value", text),
                ),
            }
        }
        Ok(out)
    }

    // ------------------------------------------------------------------ packages

    fn pack(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        st: &mut ApplyState,
        p: &PackImport,
    ) -> AppResult<()> {
        if let Some(d) = p.parent {
            DeckIndex::load(ctx.reads())?.require_live(d)?;
        }
        while !self.full() {
            st.step = match st.step {
                Step::Media(i) => self.pack_media(ctx, st, p, i)?,
                Step::Structure => self.pack_structure(ctx, st, p)?,
                Step::Notes(i) => self.pack_notes(ctx, st, p, i)?,
                Step::Cards(i) => self.pack_cards(ctx, st, p, i)?,
                Step::Values(i) => self.pack_values(ctx, st, p, i)?,
                Step::Reviews(i) => self.pack_reviews(ctx, st, p, i)?,
                Step::Rows(_) | Step::Done => Step::Done,
            };
            if st.step == Step::Done {
                break;
            }
        }
        Ok(())
    }

    /// Media (content-addressed; not undone: unreferenced media is cleaned up later).
    fn pack_media(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        st: &mut ApplyState,
        p: &PackImport,
        mut i: usize,
    ) -> AppResult<Step> {
        let media = &p.pack.media;
        while i < media.len() && !self.full() {
            self.tick()?;
            let m = &media[i];
            i += 1;
            let record = MediaRecord {
                hash: m.hash.clone(),
                mime: if m.mime.is_empty() {
                    guess_mime(m.name.as_deref().unwrap_or_default()).to_owned()
                } else {
                    m.mime.clone()
                },
                size: m.bytes.len() as u64,
                original_name: m.name.clone(),
                created_at: ctx.now,
            };
            if media_store().put(&mut *ctx.tx, &record, &m.bytes)? {
                st.report.added_media += 1;
            }
            self.media_bytes += m.bytes.len();
        }
        Ok(if i >= media.len() {
            Step::Structure
        } else {
            Step::Media(i)
        })
    }

    /// Decks, note types and classification definitions, in one chunk.
    fn pack_structure(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        st: &mut ApplyState,
        p: &PackImport,
    ) -> AppResult<Step> {
        let pack = &p.pack;
        st.report.issues.extend(p.issues.iter().cloned());
        // Decks, parents first, matched by path under `parent`.
        let mut decks = pack.decks.clone();
        decks.sort_by_key(|d| pack.deck_path(d.id).len());
        for d in &decks {
            self.tick()?;
            let path = pack.deck_path(d.id);
            let id = self.ensure_deck(ctx, &mut st.report, p.parent, &path, Some(d))?;
            st.deck_map.insert(d.id, id);
        }
        // Note types: reuse by id, else insert.
        for nt in &pack.notetypes {
            match ctx.reads().get_notetype(nt.id)? {
                Some(existing) if existing.deleted_at.is_none() => {
                    st.notetypes.insert(nt.id, existing);
                }
                Some(mut trashed) => {
                    trashed.deleted_at = None;
                    ctx.tx.upsert_notetype(&trashed)?;
                    self.created.revived_notetypes.push(trashed.id);
                    st.notetypes.insert(nt.id, trashed);
                }
                None => {
                    if validate_notetype(nt).is_err() {
                        st.report.issues.push(issue(0, "io.issue.invalid_notetype"));
                        continue;
                    }
                    let mut nt = nt.clone();
                    nt.builtin = None;
                    nt.deleted_at = None;
                    ctx.tx.upsert_notetype(&nt)?;
                    self.created.notetypes.push(nt.id);
                    st.report.added_notetypes += 1;
                    st.notetypes.insert(nt.id, nt);
                }
            }
        }
        self.pack_attributes(ctx, st, p)?;
        for c in &pack.cards {
            if let Some(d) = st.deck_map.get(&c.deck_id) {
                st.first_card_deck.entry(c.note_id).or_insert(*d);
            }
        }
        Ok(Step::Notes(0))
    }

    fn pack_notes(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        st: &mut ApplyState,
        p: &PackImport,
        mut i: usize,
    ) -> AppResult<Step> {
        let notes = &p.pack.notes;
        while i < notes.len() && !self.full() {
            self.tick()?;
            let n = &notes[i];
            i += 1;
            let row = u32::try_from(i).unwrap_or(u32::MAX);
            let Some(nt) = st.notetypes.get(&n.notetype_id).cloned() else {
                st.report.issues.push(issue(row, "io.issue.unknown_notetype"));
                continue;
            };
            let home = st
                .deck_map
                .get(&n.home_deck_id)
                .copied()
                .or_else(|| st.first_card_deck.get(&n.id).copied())
                .or(p.parent);
            let Some(home) = home else {
                st.report.issues.push(issue(row, "io.issue.no_deck"));
                continue;
            };
            let mut note = n.clone();
            note.notetype_id = nt.id;
            note.home_deck_id = home;
            note.deleted_at = None;
            derive(&nt, &mut note);
            let by_id = ctx
                .reads()
                .get_note(n.id)?
                .filter(|e| e.deleted_at.is_none())
                .map(|e| e.id);
            let duplicate = match by_id {
                Some(id) => Some(id),
                None => ctx
                    .reads()
                    .find_notes_by_checksum(nt.id, &note.checksum)?
                    .first()
                    .copied(),
            };
            if let Some(existing) = duplicate {
                match p.duplicates {
                    DuplicatePolicy::Skip => {
                        st.report.skipped_duplicates += 1;
                        continue;
                    }
                    DuplicatePolicy::Update => {
                        self.update_note(ctx, &mut st.report, existing, &note, &[])?;
                        continue;
                    }
                    DuplicatePolicy::Add => {}
                }
            }
            if ctx.reads().get_note(note.id)?.is_some() {
                note.id = NoteId::generate(ctx.ids);
            }
            insert_note(ctx, &nt, &note)?;
            self.created.notes.push(note.id);
            st.report.added_notes += 1;
            st.note_map.insert(n.id, note.id);
        }
        Ok(if i >= notes.len() {
            Step::Cards(0)
        } else {
            Step::Notes(i)
        })
    }

    /// Cards of the inserted notes.
    fn pack_cards(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        st: &mut ApplyState,
        p: &PackImport,
        mut i: usize,
    ) -> AppResult<Step> {
        let cards = &p.pack.cards;
        while i < cards.len() && !self.full() {
            self.tick()?;
            let c = &cards[i];
            i += 1;
            let Some(note_id) = st.note_map.get(&c.note_id).copied() else {
                continue;
            };
            let mut card = c.clone();
            card.note_id = note_id;
            card.deck_id = match st.deck_map.get(&c.deck_id) {
                Some(d) => *d,
                None => match ctx.reads().get_note(note_id)? {
                    Some(n) => n.home_deck_id,
                    None => continue,
                },
            };
            card.deleted_at = None;
            if !p.include_history {
                card.scheduling = SchedulingState::NEW;
                card.suspended = false;
                card.buried_until = None;
            }
            if st.used_card_ids.contains(&card.id) || ctx.reads().get_card(card.id)?.is_some() {
                card.id = CardId::generate(ctx.ids);
            }
            st.used_card_ids.insert(card.id);
            st.card_map.insert(c.id, card.id);
            ctx.tx.insert_card(&card)?;
            self.created.cards.push(card.id);
            st.report.added_cards += 1;
        }
        Ok(if i >= cards.len() {
            Step::Values(0)
        } else {
            Step::Cards(i)
        })
    }

    /// Classification values of the inserted notes/cards.
    fn pack_values(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        st: &mut ApplyState,
        p: &PackImport,
        mut i: usize,
    ) -> AppResult<Step> {
        let values = &p.pack.attribute_values;
        while i < values.len() && !self.full() {
            self.tick()?;
            let v = &values[i];
            i += 1;
            let entity = match v.entity {
                EntityRef::Note(n) => st.note_map.get(&n).map(|n| EntityRef::Note(*n)),
                EntityRef::Card(c) => st.card_map.get(&c).map(|c| EntityRef::Card(*c)),
            };
            let (Some(entity), Some(attr)) = (entity, st.def_map.get(&v.attr_id).copied()) else {
                continue;
            };
            let Some(def) = ctx.reads().get_attribute_def(attr)? else {
                continue;
            };
            let remap = |o: &AttributeOptionId| st.option_map.get(o).copied().unwrap_or(*o);
            let value = match &v.value {
                AttributeValue::Choice(o) => AttributeValue::Choice(remap(o)),
                AttributeValue::Choices(list) => {
                    AttributeValue::Choices(list.iter().map(remap).collect())
                }
                other => other.clone(),
            };
            match validate_attribute_value(&def, &value) {
                Ok(value) => ctx
                    .tx
                    .set_attribute_values(entity, &[(attr, Some(value))])?,
                Err(_) => st.report.issues.push(issue(0, "io.issue.attribute_invalid")),
            }
        }
        Ok(if i >= values.len() {
            Step::Reviews(0)
        } else {
            Step::Values(i)
        })
    }

    /// Review history (when kept).
    fn pack_reviews(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        st: &mut ApplyState,
        p: &PackImport,
        i: usize,
    ) -> AppResult<Step> {
        let logs = &p.pack.review_logs;
        if !p.include_history || i >= logs.len() {
            return Ok(Step::Done);
        }
        let end = (i + self.budget.saturating_sub(self.spent).max(1)).min(logs.len());
        self.cancel.check()?;
        self.spent += end - i;
        let rows: Vec<ReviewLogRecord> = logs[i..end]
            .iter()
            .filter_map(|r| {
                st.card_map.get(&r.card_id).map(|card| ReviewLogRecord {
                    id: r.id,
                    card_id: *card,
                    session_id: r.session_id,
                    rating: r.rating,
                    state_before: r.state_before,
                    state_after: r.state_after,
                    scheduled_days: r.scheduled_days,
                    elapsed_days: r.elapsed_days,
                    duration_ms: r.duration_ms,
                    answer_duration_ms: r.answer_duration_ms,
                    hints_used: r.hints_used,
                    mode: r.mode.clone(),
                    stability_after: r.stability_after,
                    difficulty_after: r.difficulty_after,
                    created_at: r.created_at,
                })
            })
            .collect();
        st.report.added_reviews += ctx.tx.insert_review_logs(&rows)?;
        Ok(if end >= logs.len() {
            Step::Done
        } else {
            Step::Reviews(end)
        })
    }

    /// Definitions keep their ids (exact round trip of `.fcpack`): an existing definition is
    /// reused and gets the options it lacks (same ids; same label = same option); a new one
    /// is inserted with its options, scoped to the mapped deck (or the import's parent deck
    /// if its scope is outside the package). A definition whose key is taken at that scope
    /// maps onto the existing one by label.
    fn pack_attributes(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        st: &mut ApplyState,
        p: &PackImport,
    ) -> AppResult<()> {
        for def in &p.pack.attribute_defs {
            let scope = match def.scope_deck_id {
                Some(s) => st.deck_map.get(&s).copied().or(p.parent),
                None => None,
            };
            let existing = match ctx.reads().get_attribute_def(def.id)? {
                Some(e) if e.deleted_at.is_none() => Some(e),
                _ => ctx
                    .reads()
                    .list_attribute_defs(false)?
                    .into_iter()
                    .find(|d| d.scope_deck_id == scope && d.key == def.key),
            };
            match existing {
                Some(e) => {
                    for o in def.sorted_options() {
                        if let Some(same) = e.option(o.id).or_else(|| e.option_by_label(&o.label)) {
                            st.option_map.insert(o.id, same.id);
                            continue;
                        }
                        let id = (!option_used(ctx, o.id)?).then_some(o.id);
                        self.add_option(ctx, &mut st.report, e.id, &o.label, o.color.clone(), id)?;
                        if let Some(fresh) = ctx.reads().get_attribute_def(e.id)? {
                            if let Some(added) = fresh.option_by_label(&o.label) {
                                st.option_map.insert(o.id, added.id);
                            }
                        }
                    }
                    st.def_map.insert(def.id, e.id);
                }
                None => {
                    let mut new = def.clone();
                    if ctx.reads().get_attribute_def(def.id)?.is_some() {
                        new.id = AttributeDefId::generate(ctx.ids);
                    }
                    new.scope_deck_id = scope;
                    new.deleted_at = None;
                    new.template_id = def.template_id.clone();
                    ctx.tx.insert_attribute_def(&new)?;
                    self.created.attribute_defs.push(new.id);
                    st.report.added_attributes += 1;
                    st.def_map.insert(def.id, new.id);
                    for o in &new.options {
                        st.option_map.insert(o.id, o.id);
                    }
                }
            }
        }
        Ok(())
    }
}

/// Whether an option id is already used by any definition.
fn option_used(ctx: &CommandCtx<'_>, id: AttributeOptionId) -> AppResult<bool> {
    Ok(ctx
        .reads()
        .list_attribute_defs(true)?
        .iter()
        .any(|d| d.option(id).is_some()))
}

fn find_notetype(ctx: &CommandCtx<'_>, r: &str) -> AppResult<Option<NoteType>> {
    if let Ok(id) = r.parse::<NoteTypeId>() {
        if let Some(nt) = ctx.reads().get_notetype(id)? {
            return Ok(Some(nt));
        }
    }
    let wanted = r.to_lowercase();
    Ok(ctx.reads().list_notetypes(false)?.into_iter().find(|n| {
        n.name.to_lowercase() == wanted || n.builtin.as_deref() == Some(wanted.as_str())
    }))
}

trait TapParam {
    fn tap_param(self, name: &str, value: &str) -> Self;
}

impl TapParam for ImportIssue {
    fn tap_param(mut self, name: &str, value: &str) -> Self {
        self.message = self.message.param(name, value);
        self
    }
}

// ------------------------------------------------------------------ trash / untrash

fn set_deleted(
    ctx: &mut CommandCtx<'_>,
    ids: &ImportedIds,
    at: Option<Timestamp>,
) -> AppResult<AffectedCount> {
    let cards = ctx.tx.set_cards_deleted(&ids.cards, at)?;
    let notes = ctx.tx.set_notes_deleted(&ids.notes, at)?;
    let decks = ctx.tx.set_decks_deleted(&ids.decks, at)?;
    for d in &ids.attribute_defs {
        ctx.tx.set_attribute_def_deleted(*d, at)?;
    }
    for id in ids.notetypes.iter().chain(&ids.revived_notetypes) {
        if let Some(mut nt) = ctx.reads().get_notetype(*id)? {
            nt.deleted_at = at;
            ctx.tx.upsert_notetype(&nt)?;
        }
    }
    Ok(AffectedCount {
        decks,
        notes,
        cards,
    })
}

/// Inverse of `io.import.apply`: trashes what the import created.
pub(super) fn trash(
    req: ImportedIds,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AffectedCount>> {
    let now = ctx.now;
    let affected = set_deleted(ctx, &req, Some(now))?;
    let mut events: Vec<DomainEvent> = req
        .decks
        .iter()
        .map(|d| DomainEvent::DeckDeleted {
            id: *d,
            affected: AffectedCount::default(),
        })
        .collect();
    events.push(DomainEvent::NotesDeleted {
        ids: req.notes.clone(),
    });
    Ok(CommandOutcome {
        response: affected,
        inverse: vec![inverse_call::<IoImportUntrash>(&req)?],
        summary: summary::<IoImportTrash>().param("count", req.notes.len()),
        events,
    })
}

/// Inverse of `io.import.trash`.
pub(super) fn untrash(
    req: ImportedIds,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AffectedCount>> {
    let affected = set_deleted(ctx, &req, None)?;
    let index = DeckIndex::load(ctx.reads())?;
    let mut events: Vec<DomainEvent> = req
        .decks
        .iter()
        .filter_map(|d| index.get(*d))
        .map(|d| DomainEvent::DeckRestored { deck: index.dto(d) })
        .collect();
    events.push(DomainEvent::NotesRestored {
        ids: req.notes.clone(),
    });
    Ok(CommandOutcome {
        response: affected,
        inverse: vec![inverse_call::<IoImportTrash>(&req)?],
        summary: summary::<IoImportUntrash>().param("count", req.notes.len()),
        events,
    })
}

/// `io.import.discard`: reverts the committed chunks of an unfinished import (the job's
/// [`crate::app::UndoGroup`] inverses, newest first). Rows the chunks created are
/// hard-deleted — with their cards, review rows, values, search rows and the tags only they
/// introduced — instead of trashed; trashed note types they revived are trashed again; the
/// other inverses (notes updated, options added) run as recorded. Records no undo step.
pub(super) fn discard(
    req: DiscardImport,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<Empty>> {
    let trash_method = format!("{}.{}", fc_api::API_VERSION, IoImportTrash::NAME);
    let mut purge = PurgeRows {
        tags_created_since: Some(req.since),
        ..PurgeRows::default()
    };
    let mut revived = Vec::new();
    let mut others = Vec::new();
    for call in &req.calls {
        if call.method == trash_method {
            let ids: ImportedIds = serde_json::from_value(call.params.clone()).map_err(|e| {
                crate::error::AppError::api(fc_api::ErrorCode::Internal, e.to_string())
            })?;
            purge.decks.extend(ids.decks);
            purge.notetypes.extend(ids.notetypes);
            purge.notes.extend(ids.notes);
            purge.cards.extend(ids.cards);
            purge.attribute_defs.extend(ids.attribute_defs);
            revived.extend(ids.revived_notetypes);
        } else {
            others.push(call);
        }
    }
    ctx.tx.purge_rows(&purge)?;
    let now = ctx.now;
    for id in &revived {
        if let Some(mut nt) = ctx.reads().get_notetype(*id)? {
            nt.deleted_at = Some(now);
            ctx.tx.upsert_notetype(&nt)?;
        }
    }
    for call in others {
        ctx.call(call)?;
    }
    let mut events: Vec<DomainEvent> = purge
        .decks
        .iter()
        .map(|d| DomainEvent::DeckDeleted {
            id: *d,
            affected: AffectedCount::default(),
        })
        .collect();
    if !purge.notes.is_empty() {
        events.push(DomainEvent::NotesDeleted { ids: purge.notes });
    }
    Ok(CommandOutcome {
        response: Empty {},
        inverse: Vec::new(),
        summary: summary::<IoImportDiscard>(),
        events,
    })
}
