//! `io.import.apply`: writes a staged import in the caller's transaction (one undo batch), and
//! its inverse pair `io.import.trash` / `io.import.untrash`.
//!
//! Rows the import creates are tracked in [`ImportedIds`]; the apply's inverse trashes exactly
//! those. Updates of existing notes (`duplicates: update`) and classification options created
//! for unknown labels go through nested `notes.update` / `attributes.option.add` calls, which
//! record their own inverses in the same batch. The cancel token is checked as rows are
//! written; a cancelled apply fails and its transaction rolls back.

use super::import::{PackImport, Staged};
use super::rows::{parse_value, AttrRef, ParsedValue, RowSpec, RowsImport};
use super::Staging;
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandHandler, CommandOutcome};
use crate::jobs::CancelToken;
use crate::support::decks::DeckIndex;
use crate::support::notes::{derive, insert_note, require_notetype};
use crate::support::summary;
use fc_api::v1::attributes::{AddAttributeOption, AttributeAssignment};
use fc_api::v1::common::{AffectedCount, I18nMessage};
use fc_api::v1::events::DomainEvent;
use fc_api::v1::io::*;
use fc_api::v1::methods::{AttributesOptionAdd, IoImportApply, IoImportTrash, IoImportUntrash, NotesUpdate};
use fc_api::v1::notes::UpdateNote;
use fc_core::attribute::{
    effective_attribute_defs, prepare_attribute_values, validate_attribute_value, AttributeDef,
    AttributeTarget, AttributeValue, EntityRef,
};
use fc_core::card::{standard_card_specs, Card, SchedulingState};
use fc_core::deck::{normalize_deck_segment, Deck, DeckPath};
use fc_core::note::{normalize_note_fields, FieldValues, Note};
use fc_core::notetype::{validate_notetype, FieldKey, NoteType};
use fc_core::tag::{dedup_tags, TagName};
use fc_core::{
    AttributeDefId, AttributeOptionId, CardId, DeckId, NoteId, NoteTypeId, Timestamp,
};
use fc_interop::media::guess_mime;
use fc_storage::repo::{MediaRecord, ReviewLogRecord};
use std::collections::HashMap;
use std::sync::Arc;

/// How often (rows) the cancel token is checked.
const CHECK_EVERY: usize = 32;

/// Handler of `io.import.apply`.
pub(crate) struct ApplyHandler(pub Arc<Staging>);

impl CommandHandler<IoImportApply> for ApplyHandler {
    fn execute(
        &self,
        req: StagedJob,
        ctx: &mut CommandCtx<'_>,
    ) -> AppResult<CommandOutcome<ImportReport>> {
        let (staged, cancel) = self.0.take_import(req.job_id)?;
        let mut w = Writer {
            created: ImportedIds::default(),
            report: ImportReport::default(),
            cancel,
        };
        match staged {
            Staged::Rows(r) => w.rows(ctx, r)?,
            Staged::Pack(p) => w.pack(ctx, *p)?,
        }
        w.cancel.check()?;
        let index = DeckIndex::load(ctx.reads())?;
        let mut events: Vec<DomainEvent> = w
            .created
            .decks
            .iter()
            .filter_map(|d| index.get(*d))
            .map(|d| DomainEvent::DeckCreated { deck: index.dto(d) })
            .collect();
        events.push(DomainEvent::Imported {
            job_id: req.job_id,
            notes: w.report.added_notes + w.report.updated_notes,
        });
        let inverse = if w.created.is_empty() {
            Vec::new()
        } else {
            vec![inverse_call::<IoImportTrash>(&w.created)?]
        };
        Ok(CommandOutcome {
            summary: summary::<IoImportApply>().param("count", w.report.added_notes),
            response: w.report,
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

struct Writer {
    created: ImportedIds,
    report: ImportReport,
    cancel: CancelToken,
}

impl Writer {
    fn tick(&self, i: usize) -> AppResult<()> {
        if i % CHECK_EVERY == 0 {
            self.cancel.check()?;
        }
        Ok(())
    }

    /// Finds or creates the deck path `segments` under `parent`; a created leaf takes
    /// `leaf` (id if free, description, icon, colour).
    fn ensure_deck(
        &mut self,
        ctx: &mut CommandCtx<'_>,
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
            self.report.added_decks += 1;
            parent = Some(id);
        }
        parent.ok_or_else(|| crate::support::invalid("empty deck path"))
    }

    /// Inserts cards for a new note.
    fn insert_cards(&mut self, ctx: &mut CommandCtx<'_>, cards: &[Card]) -> AppResult<()> {
        for c in cards {
            ctx.tx.insert_card(c)?;
            self.created.cards.push(c.id);
            self.report.added_cards += 1;
        }
        Ok(())
    }

    /// Adds `label` to `def` (nested `attributes.option.add`, its own undo entry).
    fn add_option(
        &mut self,
        ctx: &mut CommandCtx<'_>,
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
        self.report.added_options += 1;
        Ok(())
    }

    // ------------------------------------------------------------------ rows

    fn rows(&mut self, ctx: &mut CommandCtx<'_>, imp: RowsImport) -> AppResult<()> {
        let default_nt = require_notetype(ctx.reads(), imp.notetype_id)?;
        if let Some(d) = imp.deck_id {
            DeckIndex::load(ctx.reads())?.require_live(d)?;
        }
        let mut notetypes: HashMap<String, Option<NoteType>> = HashMap::new();
        for (i, row) in imp.rows.iter().enumerate() {
            self.tick(i)?;
            let nt = match &row.notetype {
                None => default_nt.clone(),
                Some(r) => {
                    if !notetypes.contains_key(r) {
                        let found = find_notetype(ctx, r)?;
                        notetypes.insert(r.clone(), found);
                    }
                    match notetypes.get(r).cloned().flatten() {
                        Some(nt) => nt,
                        None => {
                            self.report.issues.push(issue(row.row, "io.issue.unknown_notetype"));
                            continue;
                        }
                    }
                }
            };
            self.row(ctx, &imp, &nt, row)?;
        }
        Ok(())
    }

    fn row(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        imp: &RowsImport,
        nt: &NoteType,
        row: &RowSpec,
    ) -> AppResult<()> {
        let specs = standard_card_specs(nt);
        if nt.deleted_at.is_some() || specs.is_empty() {
            self.report.issues.push(issue(row.row, "io.issue.notetype_no_cards"));
            return Ok(());
        }
        let deck = match (&row.deck, imp.deck_id) {
            (Some(path), _) => match DeckPath::parse(path) {
                Ok(p) => self.ensure_deck(ctx, None, p.segments(), None)?,
                Err(_) => {
                    self.report.issues.push(issue(row.row, "io.issue.bad_deck"));
                    return Ok(());
                }
            },
            (None, Some(d)) => d,
            (None, None) => {
                self.report.issues.push(issue(row.row, "io.issue.no_deck"));
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
                None => self.report.issues.push(
                    issue(row.row, "io.issue.unknown_field").tap_param("field", k),
                ),
            }
        }
        let fields = match normalize_note_fields(nt, &values) {
            Ok(f) => f,
            Err(_) => {
                self.report.issues.push(issue(row.row, "io.issue.invalid_fields"));
                return Ok(());
            }
        };
        let mut tags = Vec::new();
        for t in &row.tags {
            match TagName::parse(t) {
                Ok(t) => tags.push(t),
                Err(_) => self
                    .report
                    .issues
                    .push(issue(row.row, "io.issue.bad_tag").tap_param("tag", t)),
            }
        }
        let tags = dedup_tags(tags);

        // Classifications applicable to the deck (note level).
        let provided = self.row_attributes(ctx, imp, deck, row)?;

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
                    self.report.skipped_duplicates += 1;
                    return Ok(());
                }
                DuplicatePolicy::Update => {
                    self.update_note(ctx, existing, &note, &provided)?;
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
                self.report.issues.push(issue(row.row, "io.issue.attribute_required"));
                return Ok(());
            }
        };
        insert_note(ctx, nt, &note)?;
        self.created.notes.push(note.id);
        self.report.added_notes += 1;
        let cards: Vec<Card> = specs
            .into_iter()
            .map(|spec| Card::new(CardId::generate(ctx.ids), note.id, deck, spec, ctx.now))
            .collect();
        self.insert_cards(ctx, &cards)?;
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
                self.report
                    .issues
                    .push(issue(row.row, "io.issue.attribute_not_applicable"));
                continue;
            };
            let mut parsed = parse_value(&def, text);
            if let ParsedValue::MissingLabels(labels) = &parsed {
                if imp.create_missing_options {
                    for l in labels.clone() {
                        self.add_option(ctx, def.id, &l, None, None)?;
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
                    Err(_) => self.report.issues.push(
                        issue(row.row, "io.issue.attribute_invalid").tap_param("value", text),
                    ),
                },
                ParsedValue::MissingLabels(_) => self.report.issues.push(
                    issue(row.row, "io.issue.attribute_unknown_option").tap_param("value", text),
                ),
                ParsedValue::Invalid => self.report.issues.push(
                    issue(row.row, "io.issue.attribute_invalid").tap_param("value", text),
                ),
            }
        }
        Ok(out)
    }

    /// `duplicates: update`: nested `notes.update` (its own undo entry).
    fn update_note(
        &mut self,
        ctx: &mut CommandCtx<'_>,
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
        self.report.updated_notes += 1;
        Ok(())
    }

    // ------------------------------------------------------------------ packages

    fn pack(&mut self, ctx: &mut CommandCtx<'_>, p: PackImport) -> AppResult<()> {
        let PackImport {
            pack,
            parent,
            duplicates,
            include_history,
            issues,
        } = p;
        self.report.issues.extend(issues);
        if let Some(d) = parent {
            DeckIndex::load(ctx.reads())?.require_live(d)?;
        }

        // Media (content-addressed; not undone, unreferenced media is cleaned up later).
        for m in &pack.media {
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
            if ctx.tx.put_media(&record, &m.bytes)? {
                self.report.added_media += 1;
            }
        }
        self.cancel.check()?;

        // Decks, parents first, matched by path under `parent`.
        let mut decks = pack.decks.clone();
        decks.sort_by_key(|d| pack.deck_path(d.id).len());
        let mut deck_map: HashMap<DeckId, DeckId> = HashMap::new();
        for d in &decks {
            let path = pack.deck_path(d.id);
            let id = self.ensure_deck(ctx, parent, &path, Some(d))?;
            deck_map.insert(d.id, id);
        }

        // Note types: reuse by id, else insert.
        let mut notetypes: HashMap<NoteTypeId, NoteType> = HashMap::new();
        for nt in &pack.notetypes {
            match ctx.reads().get_notetype(nt.id)? {
                Some(existing) if existing.deleted_at.is_none() => {
                    notetypes.insert(nt.id, existing);
                }
                Some(mut trashed) => {
                    trashed.deleted_at = None;
                    ctx.tx.upsert_notetype(&trashed)?;
                    self.created.notetypes.push(trashed.id);
                    notetypes.insert(nt.id, trashed);
                }
                None => {
                    if validate_notetype(nt).is_err() {
                        self.report.issues.push(issue(0, "io.issue.invalid_notetype"));
                        continue;
                    }
                    let mut nt = nt.clone();
                    nt.builtin = None;
                    nt.deleted_at = None;
                    ctx.tx.upsert_notetype(&nt)?;
                    self.created.notetypes.push(nt.id);
                    self.report.added_notetypes += 1;
                    notetypes.insert(nt.id, nt);
                }
            }
        }

        // Classification definitions and options.
        let (def_map, option_map) = self.pack_attributes(ctx, &pack, parent, &deck_map)?;

        // Notes.
        let mut note_map: HashMap<NoteId, NoteId> = HashMap::new();
        for (i, n) in pack.notes.iter().enumerate() {
            self.tick(i)?;
            let row = u32::try_from(i + 1).unwrap_or(u32::MAX);
            let Some(nt) = notetypes.get(&n.notetype_id) else {
                self.report.issues.push(issue(row, "io.issue.unknown_notetype"));
                continue;
            };
            let home = deck_map
                .get(&n.home_deck_id)
                .copied()
                .or_else(|| {
                    pack.cards
                        .iter()
                        .find(|c| c.note_id == n.id)
                        .and_then(|c| deck_map.get(&c.deck_id).copied())
                })
                .or(parent);
            let Some(home) = home else {
                self.report.issues.push(issue(row, "io.issue.no_deck"));
                continue;
            };
            let mut note = n.clone();
            note.notetype_id = nt.id;
            note.home_deck_id = home;
            note.deleted_at = None;
            derive(nt, &mut note);
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
                match duplicates {
                    DuplicatePolicy::Skip => {
                        self.report.skipped_duplicates += 1;
                        continue;
                    }
                    DuplicatePolicy::Update => {
                        self.update_note(ctx, existing, &note, &[])?;
                        continue;
                    }
                    DuplicatePolicy::Add => {}
                }
            }
            if ctx.reads().get_note(note.id)?.is_some() {
                note.id = NoteId::generate(ctx.ids);
            }
            insert_note(ctx, nt, &note)?;
            self.created.notes.push(note.id);
            self.report.added_notes += 1;
            note_map.insert(n.id, note.id);
        }

        // Cards of the inserted notes.
        let mut card_map: HashMap<CardId, CardId> = HashMap::new();
        let mut new_cards = Vec::new();
        for (i, c) in pack.cards.iter().enumerate() {
            self.tick(i)?;
            let Some(note_id) = note_map.get(&c.note_id).copied() else {
                continue;
            };
            let mut card = c.clone();
            card.note_id = note_id;
            card.deck_id = match deck_map.get(&c.deck_id) {
                Some(d) => *d,
                None => match ctx.reads().get_note(note_id)? {
                    Some(n) => n.home_deck_id,
                    None => continue,
                },
            };
            card.deleted_at = None;
            if !include_history {
                card.scheduling = SchedulingState::NEW;
                card.suspended = false;
                card.buried_until = None;
            }
            if ctx.reads().get_card(card.id)?.is_some() || card_map.values().any(|v| *v == card.id)
            {
                card.id = CardId::generate(ctx.ids);
            }
            card_map.insert(c.id, card.id);
            new_cards.push(card);
        }
        self.insert_cards(ctx, &new_cards)?;

        // Classification values of the inserted notes/cards.
        for v in &pack.attribute_values {
            let entity = match v.entity {
                EntityRef::Note(n) => note_map.get(&n).map(|n| EntityRef::Note(*n)),
                EntityRef::Card(c) => card_map.get(&c).map(|c| EntityRef::Card(*c)),
            };
            let (Some(entity), Some(attr)) = (entity, def_map.get(&v.attr_id).copied()) else {
                continue;
            };
            let Some(def) = ctx.reads().get_attribute_def(attr)? else {
                continue;
            };
            let remap = |o: &AttributeOptionId| option_map.get(o).copied().unwrap_or(*o);
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
                Err(_) => self.report.issues.push(issue(0, "io.issue.attribute_invalid")),
            }
        }

        // Review history.
        if include_history {
            let rows: Vec<ReviewLogRecord> = pack
                .review_logs
                .iter()
                .filter_map(|r| {
                    card_map.get(&r.card_id).map(|card| ReviewLogRecord {
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
            self.report.added_reviews = ctx.tx.insert_review_logs(&rows)?;
        }
        Ok(())
    }

    /// Definitions keep their ids (exact round trip of `.fcpack`): an existing definition is
    /// reused and gets the options it lacks (same ids; same label = same option); a new one
    /// is inserted with its options, scoped to the mapped deck (or the import's parent deck
    /// if its scope is outside the package). A definition whose key is taken at that scope
    /// maps onto the existing one by label.
    #[allow(clippy::type_complexity)]
    fn pack_attributes(
        &mut self,
        ctx: &mut CommandCtx<'_>,
        pack: &fc_interop::pack::Pack,
        parent: Option<DeckId>,
        deck_map: &HashMap<DeckId, DeckId>,
    ) -> AppResult<(
        HashMap<AttributeDefId, AttributeDefId>,
        HashMap<AttributeOptionId, AttributeOptionId>,
    )> {
        let mut def_map = HashMap::new();
        let mut option_map = HashMap::new();
        for def in &pack.attribute_defs {
            let scope = match def.scope_deck_id {
                Some(s) => deck_map.get(&s).copied().or(parent),
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
                            option_map.insert(o.id, same.id);
                            continue;
                        }
                        let id = (!option_used(ctx, o.id)?).then_some(o.id);
                        self.add_option(ctx, e.id, &o.label, o.color.clone(), id)?;
                        if let Some(fresh) = ctx.reads().get_attribute_def(e.id)? {
                            if let Some(added) = fresh.option_by_label(&o.label) {
                                option_map.insert(o.id, added.id);
                            }
                        }
                    }
                    def_map.insert(def.id, e.id);
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
                    self.report.added_attributes += 1;
                    def_map.insert(def.id, new.id);
                    for o in &new.options {
                        option_map.insert(o.id, o.id);
                    }
                }
            }
        }
        Ok((def_map, option_map))
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
    for id in &ids.notetypes {
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
