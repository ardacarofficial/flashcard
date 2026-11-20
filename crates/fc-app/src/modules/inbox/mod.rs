//! `inbox` module (v0.9, docs/06 §4): proposed cards from outside the app wait here until the
//! user accepts them as notes or rejects them.
//!
//! * `inbox.add` records an item. Its **source** comes from the actor, never from the caller:
//!   a local API client is recorded with its id and name; only the app itself (`User`,
//!   `System`) may say it came from the clipboard or a deep link (`via`). Shapes and sizes are
//!   checked here; classification values are resolved against the deck when accepted, so an
//!   item proposed before the deck had a classification still works.
//! * A local API client's `client_ref` makes a resend idempotent (same item back, nothing
//!   written).
//! * `inbox.accept` runs one nested `notes.create` per item **in one transaction**: all or
//!   nothing, one undo step (undo deletes the notes and puts the items back to pending).
//! * Every write is undoable: add ↔ internal `inbox.remove`, accept/reject/remove ↔ internal
//!   `inbox.restore` (rows put back exactly).

#[cfg(test)]
pub(crate) mod tests;

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::support::{dto::attr_def, err_id, from_json, summary};
use fc_api::envelope::InvokeRequest;
use fc_api::v1::attributes::{AttributeAssignment, AttributeDefDto};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::inbox::*;
use fc_api::v1::methods::*;
use fc_api::v1::notes::{CreateNote, NoteDto};
use fc_api::v1::oplog::Actor;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::attribute::AttributeTarget;
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::notetype::NoteType;
use fc_core::{ApiClientId, DeckId, InboxItemId, NoteTypeId};
use fc_storage::repo::InboxRecord;
use fc_storage::Reads;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Module id.
pub const ID: &str = "inbox";

/// Most characters in one field value.
pub const MAX_FIELD_CHARS: usize = 20_000;
/// Most fields in one item.
pub const MAX_FIELDS: usize = 32;
/// Most tags in one item.
pub const MAX_TAGS: usize = 50;
/// Most classification values in one item.
pub const MAX_ATTRIBUTES: usize = 32;
/// Most items per `inbox.accept` / `inbox.reject`.
pub const MAX_ITEMS_PER_CALL: usize = 500;

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.inbox.name",
        description_key: "modules.inbox.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![main_menu("inbox.screen", "nav.inbox", "inbox", "/inbox", 25)],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.command::<InboxAdd>(add);
    reg.query::<InboxList>(list);
    reg.query::<InboxCountMethod>(count);
    reg.command::<InboxAccept>(accept);
    reg.command::<InboxReject>(reject);
    reg.command::<InboxRemove>(remove);
    reg.command::<InboxRestore>(restore);
}

// ------------------------------------------------------------------ conversions

fn status_str(s: InboxStatus) -> &'static str {
    match s {
        InboxStatus::Pending => "pending",
        InboxStatus::Accepted => "accepted",
        InboxStatus::Rejected => "rejected",
    }
}

fn status_of(s: &str) -> AppResult<InboxStatus> {
    match s {
        "pending" => Ok(InboxStatus::Pending),
        "accepted" => Ok(InboxStatus::Accepted),
        "rejected" => Ok(InboxStatus::Rejected),
        other => Err(AppError::api(
            ErrorCode::Internal,
            format!("unknown inbox status {other}"),
        )),
    }
}

/// Row → DTO.
pub(crate) fn dto(r: &InboxRecord) -> AppResult<InboxItemDto> {
    Ok(InboxItemDto {
        id: r.id,
        source: from_json(&r.source)?,
        status: status_of(&r.status)?,
        item: from_json(&r.payload)?,
        note_ids: r.note_ids.clone(),
        created_at: r.created_at,
        updated_at: r.updated_at,
    })
}

fn to_value<T: serde::Serialize>(v: &T) -> AppResult<Value> {
    serde_json::to_value(v).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
}

/// DTO → row (client columns follow the source and the item).
fn record(d: &InboxItemDto) -> AppResult<InboxRecord> {
    let client_id = match &d.source {
        InboxSource::LocalApi { client_id, .. } => Some(*client_id),
        _ => None,
    };
    Ok(InboxRecord {
        id: d.id,
        source: to_value(&d.source)?,
        payload: to_value(&d.item)?,
        status: status_str(d.status).to_owned(),
        note_ids: d.note_ids.clone(),
        client_id,
        client_ref: client_id.and(d.item.client_ref.clone()),
        created_at: d.created_at,
        updated_at: d.updated_at,
    })
}

fn require(reads: &dyn Reads, id: InboxItemId) -> AppResult<InboxRecord> {
    reads.get_inbox_item(id)?.ok_or_else(|| {
        err_id(
            ErrorCode::NotFoundInboxItem,
            format!("inbox item {id} not found"),
            id,
        )
    })
}

fn bad(field: &str, message: impl Into<String>) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationInboxItem, message).with_detail("field", field),
    )
}

fn changed(reads: &dyn Reads) -> AppResult<DomainEvent> {
    Ok(DomainEvent::InboxChanged {
        pending: reads.count_inbox("pending")?,
    })
}

// ------------------------------------------------------------------ validation

fn check_text(field: &str, value: &str, max: usize) -> AppResult<()> {
    if value.chars().count() > max {
        return Err(AppError::Api(
            ApiError::new(
                ErrorCode::ValidationInboxItem,
                format!("{field} is longer than {max} characters"),
            )
            .with_detail("field", field)
            .with_detail("max", max),
        ));
    }
    Ok(())
}

/// Stateless shape checks of a proposal (also used for review edits).
pub(crate) fn validate_item(item: &AddInboxItem) -> AppResult<()> {
    let texts = item
        .front
        .iter()
        .chain(item.back.iter())
        .chain(item.fields.values());
    let mut has_content = false;
    for t in texts {
        check_text("field", t, MAX_FIELD_CHARS)?;
        has_content |= !t.trim().is_empty();
    }
    if !has_content {
        return Err(bad("content", "an inbox item needs a front, a back or a field"));
    }
    validate_fields(&item.fields)?;
    validate_tags(&item.tags)?;
    validate_attributes(&item.attributes)?;
    if let Some(c) = &item.context {
        check_text("context", c.url.as_deref().unwrap_or_default(), 2048)?;
        check_text("context", c.title.as_deref().unwrap_or_default(), 500)?;
        check_text("context", c.sentence.as_deref().unwrap_or_default(), 2000)?;
    }
    if let Some(r) = &item.client_ref {
        let n = r.chars().count();
        if n == 0 || n > 100 || r.chars().any(char::is_control) {
            return Err(bad("client_ref", "client_ref must be 1–100 printable characters"));
        }
    }
    Ok(())
}

fn validate_fields(fields: &BTreeMap<String, String>) -> AppResult<()> {
    if fields.len() > MAX_FIELDS {
        return Err(bad("field", format!("at most {MAX_FIELDS} fields")));
    }
    for (k, v) in fields {
        if k.trim().is_empty() || k.chars().count() > 64 {
            return Err(bad("field", "field keys are 1–64 characters"));
        }
        check_text("field", v, MAX_FIELD_CHARS)?;
    }
    Ok(())
}

fn validate_tags(tags: &[String]) -> AppResult<()> {
    if tags.len() > MAX_TAGS {
        return Err(bad("tags", format!("at most {MAX_TAGS} tags")));
    }
    for t in tags {
        check_text("tags", t, 100)?;
    }
    Ok(())
}

fn validate_attributes(attrs: &BTreeMap<String, Value>) -> AppResult<()> {
    if attrs.len() > MAX_ATTRIBUTES {
        return Err(bad(
            "attributes",
            format!("at most {MAX_ATTRIBUTES} classification values"),
        ));
    }
    for (k, v) in attrs {
        if k.trim().is_empty() || k.chars().count() > 100 {
            return Err(bad("attributes", "classification keys are 1–100 characters"));
        }
        let ok = match v {
            Value::String(s) => s.chars().count() <= 200,
            Value::Number(n) => n.as_f64().is_some_and(f64::is_finite),
            Value::Bool(_) => true,
            Value::Array(items) => {
                items.len() <= 50
                    && items
                        .iter()
                        .all(|i| i.as_str().is_some_and(|s| s.chars().count() <= 200))
            }
            _ => false,
        };
        if !ok {
            return Err(AppError::Api(
                ApiError::new(
                    ErrorCode::ValidationInboxItem,
                    format!("classification {k:?}: expected a label, a list of labels, a number or true/false"),
                )
                .with_detail("field", "attributes")
                .with_detail("key", k.clone()),
            ));
        }
    }
    Ok(())
}

fn trimmed(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn require_live_deck(reads: &dyn Reads, id: DeckId) -> AppResult<()> {
    match reads.get_deck(id)? {
        Some(d) if d.deleted_at.is_none() => Ok(()),
        Some(_) => Err(crate::support::deleted("deck", id)),
        None => Err(err_id(
            ErrorCode::NotFoundDeck,
            format!("deck {id} not found"),
            id,
        )),
    }
}

/// The source of a call by `actor` (`via` only from the app itself).
fn source_of(reads: &dyn Reads, actor: &Actor, via: Option<InboxVia>) -> AppResult<InboxSource> {
    if via.is_some() && !matches!(actor, Actor::User | Actor::System) {
        return Err(bad("via", "only the app itself may set via"));
    }
    Ok(match actor {
        Actor::User | Actor::System => match via {
            None | Some(InboxVia::User) => InboxSource::User,
            Some(InboxVia::Clipboard) => InboxSource::Clipboard,
            Some(InboxVia::DeepLink) => InboxSource::DeepLink,
        },
        Actor::LocalApi(id) => {
            let client_id = id
                .parse()
                .map_err(|_| AppError::api(ErrorCode::PermissionDenied, "unknown client"))?;
            let name = reads
                .get_api_client(client_id)?
                .map(|c| c.name)
                .unwrap_or_default();
            InboxSource::LocalApi {
                client_id,
                client_name: name,
            }
        }
        other => {
            let v = to_value(other)?;
            InboxSource::Other {
                actor: v["kind"].as_str().unwrap_or("unknown").to_owned(),
                id: v["id"].as_str().map(str::to_owned),
            }
        }
    })
}

/// A local API client may not pile up proposals without bound: its pending items (count and
/// text) stay under the user's quota (`local_api.inbox_max_items`, `local_api.inbox_max_kib`)
/// until some are accepted or rejected. A resend of a known `client_ref` never gets here.
fn check_quota(reads: &dyn Reads, client: ApiClientId, req: &AddInboxItem) -> AppResult<()> {
    let (max_items, max_bytes) = crate::modules::local_api::inbox_quota(reads)?;
    let (items, bytes) = reads.inbox_client_usage(client, "pending")?;
    let incoming = u64::try_from(to_value(req)?.to_string().len()).unwrap_or(u64::MAX);
    let over = |limit: &str, max: u64| {
        AppError::Api(
            ApiError::new(
                ErrorCode::ConflictInboxQuota,
                format!("this client has too much waiting in the inbox ({limit} limit {max})"),
            )
            .with_detail("limit", limit)
            .with_detail("max", max),
        )
    };
    if items >= max_items {
        return Err(over("items", u64::from(max_items)));
    }
    if bytes.saturating_add(incoming) > max_bytes {
        return Err(over("bytes", max_bytes));
    }
    Ok(())
}

// ------------------------------------------------------------------ commands

fn add(req: AddInboxItem, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<InboxItemDto>> {
    validate_item(&req)?;
    let source = source_of(ctx.reads(), ctx.actor, req.via)?;
    if let Some(d) = req.deck_id {
        require_live_deck(ctx.reads(), d)?;
    }
    if let Some(nt) = req.notetype_id {
        crate::support::notes::require_notetype(ctx.reads(), nt)?;
    }
    let client_id = match &source {
        InboxSource::LocalApi { client_id, .. } => Some(*client_id),
        _ => None,
    };
    // A resend of the same client reference returns the item it already made.
    if let (Some(c), Some(r)) = (client_id, req.client_ref.as_deref()) {
        if let Some(existing) = ctx.reads().inbox_item_by_client_ref(c, r)? {
            return Ok(CommandOutcome {
                response: dto(&existing)?,
                inverse: vec![],
                summary: summary::<InboxAdd>(),
                events: vec![],
            });
        }
    }
    if let Some(c) = client_id {
        check_quota(ctx.reads(), c, &req)?;
    }
    let item = AddInboxItem {
        front: trimmed(&req.front),
        back: trimmed(&req.back),
        fields: req.fields,
        deck_id: req.deck_id,
        notetype_id: req.notetype_id,
        tags: req
            .tags
            .iter()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
            .collect(),
        attributes: req.attributes,
        context: req.context,
        client_ref: client_id.and(req.client_ref),
        via: None,
    };
    let d = InboxItemDto {
        id: InboxItemId::generate(ctx.ids),
        source,
        status: InboxStatus::Pending,
        item,
        note_ids: vec![],
        created_at: ctx.now,
        updated_at: ctx.now,
    };
    ctx.tx.upsert_inbox_item(&record(&d)?)?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<InboxRemove>(&InboxItemIds { ids: vec![d.id] })?],
        summary: summary::<InboxAdd>(),
        events: vec![changed(ctx.reads())?],
        response: d,
    })
}

fn check_ids(ids: &[InboxItemId]) -> AppResult<()> {
    if ids.is_empty() || ids.len() > MAX_ITEMS_PER_CALL {
        return Err(bad(
            "items",
            format!("give 1–{MAX_ITEMS_PER_CALL} items"),
        ));
    }
    let unique: BTreeSet<_> = ids.iter().collect();
    if unique.len() != ids.len() {
        return Err(bad("items", "an item is listed twice"));
    }
    Ok(())
}

fn require_pending(reads: &dyn Reads, id: InboxItemId) -> AppResult<InboxRecord> {
    let r = require(reads, id)?;
    if r.status != "pending" {
        return Err(AppError::Api(
            ApiError::new(
                ErrorCode::ConflictInboxItemState,
                format!("inbox item {id} is {}", r.status),
            )
            .with_detail("id", id.to_string())
            .with_detail("status", r.status.clone()),
        ));
    }
    Ok(r)
}

/// Field values keyed by the note type's keys: explicit `fields` (by key or name), then the
/// Classic shorthand (`front` → first field, `back` → second) where not given.
fn note_fields(
    nt: &NoteType,
    front: Option<&str>,
    back: Option<&str>,
    given: &BTreeMap<String, String>,
) -> AppResult<BTreeMap<String, String>> {
    let fold = |s: &str| fc_core::query::text::fold(s.trim());
    let mut out = BTreeMap::new();
    for (k, v) in given {
        let f = nt
            .fields
            .iter()
            .find(|f| fold(&f.key.0) == fold(k))
            .or_else(|| nt.fields.iter().find(|f| fold(&f.name) == fold(k)))
            .ok_or_else(|| {
                AppError::Api(
                    ApiError::new(
                        ErrorCode::ValidationInboxItem,
                        format!("{} has no field {k:?}", nt.name),
                    )
                    .with_detail("field", "field")
                    .with_detail("key", k.clone()),
                )
            })?;
        out.insert(f.key.0.clone(), v.clone());
    }
    for (i, v) in [front, back].into_iter().enumerate() {
        if let (Some(v), Some(f)) = (v, nt.fields.get(i)) {
            out.entry(f.key.0.clone()).or_insert_with(|| v.to_owned());
        }
    }
    Ok(out)
}

/// Classification assignments for a note in `deck`: references by id, key or name; values
/// by option id or label (see `support::attr_values::parse_value`).
fn assignments(
    reads: &dyn Reads,
    deck: DeckId,
    given: &BTreeMap<String, Value>,
) -> AppResult<Vec<AttributeAssignment>> {
    if given.is_empty() {
        return Ok(vec![]);
    }
    let defs: Vec<AttributeDefDto> = reads
        .effective_attribute_defs_for_deck(Some(deck))?
        .into_iter()
        .filter(|(d, _)| d.target == AttributeTarget::Note)
        .map(|(d, _)| attr_def(&d))
        .collect();
    given
        .iter()
        .map(|(reference, raw)| {
            let def = match defs.iter().find(|d| d.id.to_string() == reference.trim()) {
                Some(d) => d,
                None => crate::support::attr_values::match_def(&defs, reference, "on this deck")?,
            };
            Ok(AttributeAssignment {
                attr_id: def.id,
                value: crate::support::attr_values::parse_value(def, raw)?,
            })
        })
        .collect::<Result<_, ApiError>>()
        .map_err(AppError::Api)
}

fn accept(
    req: AcceptInbox,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AcceptInboxResult>> {
    let ids: Vec<InboxItemId> = req.items.iter().map(|i| i.id).collect();
    check_ids(&ids)?;
    let mut old = Vec::with_capacity(req.items.len());
    let mut accepted = Vec::with_capacity(req.items.len());
    for (index, a) in req.items.iter().enumerate() {
        let at = |e: AppError| match e {
            AppError::Api(e) => AppError::Api(e.with_detail("item_index", index)),
            other => other,
        };
        let row = require_pending(ctx.reads(), a.id).map_err(at)?;
        let before = dto(&row)?;
        let item = &before.item;
        let deck = a
            .deck_id
            .or(item.deck_id)
            .ok_or_else(|| bad("deck", "choose a deck for this item"))
            .map_err(at)?;
        let nt_id: NoteTypeId = a.notetype_id.or(item.notetype_id).unwrap_or(CLASSIC_ID);
        let nt = crate::support::notes::require_notetype(ctx.reads(), nt_id).map_err(at)?;
        let fields = match &a.fields {
            Some(f) => {
                validate_fields(f).map_err(at)?;
                note_fields(&nt, None, None, f)
            }
            None => note_fields(&nt, item.front.as_deref(), item.back.as_deref(), &item.fields),
        }
        .map_err(at)?;
        let tags = match &a.tags {
            Some(t) => {
                validate_tags(t).map_err(at)?;
                t.clone()
            }
            None => item.tags.clone(),
        };
        let attrs = match &a.attributes {
            Some(m) => {
                validate_attributes(m).map_err(at)?;
                m
            }
            None => &item.attributes,
        };
        let attributes = assignments(ctx.reads(), deck, attrs).map_err(at)?;
        let create = CreateNote {
            notetype_id: nt.id,
            deck_id: deck,
            fields,
            tags,
            attributes,
            allow_duplicate: req.allow_duplicates,
        };
        let note: NoteDto = from_json(
            &ctx.call(&InvokeRequest {
                method: format!("{}.{}", fc_api::API_VERSION, NotesCreate::NAME),
                params: to_value(&create)?,
            })
            .map_err(at)?,
        )?;
        let after = InboxItemDto {
            status: InboxStatus::Accepted,
            note_ids: vec![note.id],
            updated_at: ctx.now,
            ..before.clone()
        };
        ctx.tx.upsert_inbox_item(&record(&after)?)?;
        accepted.push(AcceptedInboxItem {
            id: a.id,
            note_id: note.id,
        });
        old.push(before);
    }
    Ok(CommandOutcome {
        response: AcceptInboxResult { accepted },
        inverse: vec![inverse_call::<InboxRestore>(&RestoreInboxItems { items: old })?],
        summary: summary::<InboxAccept>().param("count", ids.len()),
        events: vec![changed(ctx.reads())?],
    })
}

fn reject(req: InboxItemIds, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<InboxChangeCount>> {
    check_ids(&req.ids)?;
    let mut old = Vec::with_capacity(req.ids.len());
    for id in &req.ids {
        let before = dto(&require_pending(ctx.reads(), *id)?)?;
        let after = InboxItemDto {
            status: InboxStatus::Rejected,
            updated_at: ctx.now,
            ..before.clone()
        };
        ctx.tx.upsert_inbox_item(&record(&after)?)?;
        old.push(before);
    }
    Ok(CommandOutcome {
        response: InboxChangeCount {
            changed: u32::try_from(old.len()).unwrap_or(u32::MAX),
        },
        inverse: vec![inverse_call::<InboxRestore>(&RestoreInboxItems { items: old })?],
        summary: summary::<InboxReject>().param("count", req.ids.len()),
        events: vec![changed(ctx.reads())?],
    })
}

fn remove(req: InboxItemIds, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<InboxChangeCount>> {
    let mut old = Vec::new();
    for id in &req.ids {
        if let Some(r) = ctx.reads().get_inbox_item(*id)? {
            old.push(dto(&r)?);
            ctx.tx.delete_inbox_item(*id)?;
        }
    }
    let inverse = if old.is_empty() {
        vec![]
    } else {
        vec![inverse_call::<InboxRestore>(&RestoreInboxItems { items: old.clone() })?]
    };
    Ok(CommandOutcome {
        response: InboxChangeCount {
            changed: u32::try_from(old.len()).unwrap_or(u32::MAX),
        },
        inverse,
        summary: summary::<InboxRemove>(),
        events: vec![changed(ctx.reads())?],
    })
}

fn restore(
    req: RestoreInboxItems,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<InboxChangeCount>> {
    let mut previous = Vec::new();
    let mut absent = Vec::new();
    for d in &req.items {
        match ctx.reads().get_inbox_item(d.id)? {
            Some(p) => previous.push(dto(&p)?),
            None => absent.push(d.id),
        }
        ctx.tx.upsert_inbox_item(&record(d)?)?;
    }
    let mut inverse = Vec::new();
    if !absent.is_empty() {
        inverse.push(inverse_call::<InboxRemove>(&InboxItemIds { ids: absent })?);
    }
    if !previous.is_empty() {
        inverse.push(inverse_call::<InboxRestore>(&RestoreInboxItems {
            items: previous,
        })?);
    }
    Ok(CommandOutcome {
        response: InboxChangeCount {
            changed: u32::try_from(req.items.len()).unwrap_or(u32::MAX),
        },
        inverse,
        summary: summary::<InboxRestore>(),
        events: vec![changed(ctx.reads())?],
    })
}

// ------------------------------------------------------------------ queries

fn list(req: ListInbox, ctx: &QueryCtx<'_>) -> AppResult<InboxPage> {
    let status = status_str(req.status.unwrap_or(InboxStatus::Pending));
    let page = ctx.tx.list_inbox(status, &req.page)?;
    Ok(InboxPage {
        items: page.items.iter().map(dto).collect::<AppResult<_>>()?,
        next_cursor: page.next_cursor,
        pending: ctx.tx.count_inbox("pending")?,
    })
}

fn count(_req: Empty, ctx: &QueryCtx<'_>) -> AppResult<InboxCount> {
    Ok(InboxCount {
        pending: ctx.tx.count_inbox("pending")?,
    })
}
