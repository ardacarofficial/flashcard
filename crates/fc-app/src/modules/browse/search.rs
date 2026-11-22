//! `search.*` and `saved_queries.*`.

use super::{compile, query_error_dto, row, CatalogData};
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::support::decks::DeckIndex;
use crate::support::{err_id, summary};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::search::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::query::canonical::{canonicalize, render};
use fc_core::query::{normalize_saved_query_name, parse_query, SavedQuery};
use fc_core::{NoteId, SavedQueryId};
use fc_storage::repo::{CardSearch, CardSort};
use fc_storage::Reads;

/// Default and maximum page size.
const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 500;

fn sort_of(k: CardSortKey) -> CardSort {
    match k {
        CardSortKey::SortField => CardSort::SortField,
        CardSortKey::Created => CardSort::Created,
        CardSortKey::Modified => CardSort::Modified,
        CardSortKey::Due => CardSort::Due,
        CardSortKey::Stability => CardSort::Stability,
        CardSortKey::Difficulty => CardSort::Difficulty,
        CardSortKey::Lapses => CardSort::Lapses,
        CardSortKey::Reps => CardSort::Reps,
    }
}

pub(super) fn cards(req: SearchCards, ctx: &QueryCtx<'_>) -> AppResult<CardSearchPage> {
    let filter = compile(ctx.tx, &req.query, ctx.now)?;
    let offset = req.offset.unwrap_or(0);
    let page = ctx.tx.search_cards(&CardSearch {
        filter,
        now: ctx.now,
        sort: sort_of(req.sort.unwrap_or_default()),
        descending: req.descending.unwrap_or(false),
        notes_only: req.notes_only.unwrap_or(false),
        offset,
        limit: req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
    })?;
    let index = DeckIndex::load(ctx.tx)?;
    Ok(CardSearchPage {
        items: page
            .hits
            .into_iter()
            .map(|h| row(&index, h, ctx.now))
            .collect(),
        total: page.total,
        offset,
    })
}

pub(super) fn ids(req: QueryText, ctx: &QueryCtx<'_>) -> AppResult<SearchIds> {
    let filter = compile(ctx.tx, &req.query, ctx.now)?;
    let card_ids = ctx.tx.search_card_ids(&filter, ctx.now)?;
    let mut note_ids: Vec<NoteId> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for c in &card_ids {
        if let Some(card) = ctx.tx.get_card(*c)? {
            if seen.insert(card.note_id) {
                note_ids.push(card.note_id);
            }
        }
    }
    Ok(SearchIds { card_ids, note_ids })
}

pub(super) fn check(req: QueryText, ctx: &QueryCtx<'_>) -> AppResult<QueryCheck> {
    let normalized = match parse_query(&req.query) {
        Ok(expr) => expr.to_string(),
        Err(e) => {
            return Ok(QueryCheck {
                error: Some(query_error_dto(&e)),
                normalized: None,
            })
        }
    };
    let mut data = CatalogData::load(ctx.tx, ctx.now)?;
    data.lemmatize(ctx.tx, &req.query)?;
    Ok(match fc_core::query::compile_query(&req.query, &data.catalog()) {
        Ok(_) => QueryCheck {
            error: None,
            normalized: Some(normalized),
        },
        Err(e) => QueryCheck {
            error: Some(query_error_dto(&e)),
            normalized: None,
        },
    })
}

// ------------------------------------------------------------------ saved queries

/// Saved queries are stored canonical (classification and option ids, see
/// `fc_core::query::canonical`) and shown with the current names.
fn dto(q: &SavedQuery, cat: &CatalogData) -> SavedQueryDto {
    SavedQueryDto {
        query: render(&q.query, &cat.catalog()),
        ..stored(q)
    }
}

/// The stored (canonical) form, for inverses.
fn stored(q: &SavedQuery) -> SavedQueryDto {
    SavedQueryDto {
        id: q.id,
        name: q.name.clone(),
        query: q.query.clone(),
        icon: q.icon.clone(),
        position: q.position,
        created_at: q.created_at,
        updated_at: q.updated_at,
    }
}

fn model(d: &SavedQueryDto) -> SavedQuery {
    SavedQuery {
        id: d.id,
        name: d.name.clone(),
        query: d.query.clone(),
        icon: d.icon.clone(),
        position: d.position,
        created_at: d.created_at,
        updated_at: d.updated_at,
    }
}

fn name(raw: &str) -> AppResult<String> {
    normalize_saved_query_name(raw).ok_or_else(|| {
        ApiError::new(
            ErrorCode::ValidationSavedQueryName,
            format!("invalid saved query name {raw:?}"),
        )
        .into()
    })
}

pub(crate) fn require(reads: &dyn Reads, id: SavedQueryId) -> AppResult<SavedQuery> {
    reads.get_saved_query(id)?.ok_or_else(|| {
        err_id(
            ErrorCode::NotFoundSavedQuery,
            format!("saved query {id} not found"),
            id,
        )
    })
}

pub(super) fn saved_list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<SavedQueryList> {
    let cat = CatalogData::load(ctx.tx, ctx.now)?;
    Ok(SavedQueryList {
        items: ctx
            .tx
            .list_saved_queries()?
            .iter()
            .map(|q| dto(q, &cat))
            .collect(),
    })
}

pub(super) fn saved_create(
    req: CreateSavedQuery,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SavedQueryDto>> {
    let name = name(&req.name)?;
    let cat = CatalogData::load(ctx.reads(), ctx.now)?;
    let query = canonicalize(req.query.trim(), &cat.catalog());
    compile(ctx.reads(), &query, ctx.now)?;
    let position = match req.position {
        Some(p) => p,
        None => ctx
            .reads()
            .list_saved_queries()?
            .iter()
            .map(|q| q.position + 1)
            .max()
            .unwrap_or(0),
    };
    let q = SavedQuery {
        id: SavedQueryId::generate(ctx.ids),
        name,
        query,
        icon: req.icon,
        position,
        created_at: ctx.now,
        updated_at: ctx.now,
    };
    ctx.tx.upsert_saved_query(&q)?;
    Ok(CommandOutcome {
        response: dto(&q, &cat),
        inverse: vec![inverse_call::<SavedQueriesDelete>(&SavedQueryRef { id: q.id })?],
        summary: summary::<SavedQueriesCreate>().param("name", &q.name),
        events: vec![DomainEvent::SavedQueriesChanged],
    })
}

pub(super) fn saved_update(
    req: UpdateSavedQuery,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SavedQueryDto>> {
    let old = require(ctx.reads(), req.id)?;
    let cat = CatalogData::load(ctx.reads(), ctx.now)?;
    let query = canonicalize(req.query.trim(), &cat.catalog());
    if ctx.batch_kind == fc_api::v1::oplog::OplogKind::Do {
        compile(ctx.reads(), &query, ctx.now)?;
    }
    let q = SavedQuery {
        name: name(&req.name)?,
        query,
        icon: req.icon,
        position: req.position.unwrap_or(old.position),
        updated_at: ctx.now,
        ..old.clone()
    };
    ctx.tx.upsert_saved_query(&q)?;
    let inverse = UpdateSavedQuery {
        id: old.id,
        name: old.name.clone(),
        query: old.query.clone(),
        icon: old.icon.clone(),
        position: Some(old.position),
    };
    Ok(CommandOutcome {
        response: dto(&q, &cat),
        inverse: vec![inverse_call::<SavedQueriesUpdate>(&inverse)?],
        summary: summary::<SavedQueriesUpdate>().param("name", &q.name),
        events: vec![DomainEvent::SavedQueriesChanged],
    })
}

pub(super) fn saved_delete(
    req: SavedQueryRef,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SavedQueryDto>> {
    let old = require(ctx.reads(), req.id)?;
    let cat = CatalogData::load(ctx.reads(), ctx.now)?;
    ctx.tx.delete_saved_query(old.id)?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<SavedQueriesRestore>(&RestoreSavedQuery {
            query: stored(&old),
        })?],
        summary: summary::<SavedQueriesDelete>().param("name", &old.name),
        events: vec![DomainEvent::SavedQueriesChanged],
        response: dto(&old, &cat),
    })
}

pub(super) fn saved_restore(
    req: RestoreSavedQuery,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SavedQueryDto>> {
    let cat = CatalogData::load(ctx.reads(), ctx.now)?;
    let mut q = model(&req.query);
    q.query = canonicalize(&q.query, &cat.catalog());
    ctx.tx.upsert_saved_query(&q)?;
    Ok(CommandOutcome {
        response: dto(&q, &cat),
        inverse: vec![inverse_call::<SavedQueriesDelete>(&SavedQueryRef { id: q.id })?],
        summary: summary::<SavedQueriesRestore>().param("name", &q.name),
        events: vec![DomainEvent::SavedQueriesChanged],
    })
}
