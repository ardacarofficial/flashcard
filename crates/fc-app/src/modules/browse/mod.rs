//! `browse` module (v0.4, docs/02 §9–11): the query language behind search and saved queries,
//! the card browser's bulk operations, card info, duplicates, trash and the database check.
//!
//! Every query text goes through [`compile`]: parse + resolve against a fresh catalog snapshot
//! (decks, tags, note types, classifications), so names always mean what they mean *now*.

mod bulk;
mod maintenance;
mod search;
#[cfg(test)]
mod tests;

use crate::error::{AppError, AppResult};
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::support::decks::DeckIndex;
use fc_api::v1::methods::*;
use fc_api::v1::search::{CardRow, QueryErrorDto};
use fc_api::{ApiError, ErrorCode};
use fc_core::query::{compile_query, Catalog, Filter, QueryError};
use fc_core::tag::Tag;
use fc_core::Timestamp;
use fc_storage::repo::CardHit;
use fc_storage::Reads;

/// Module id.
pub const ID: &str = "browse";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.browse.name",
        description_key: "modules.browse.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![
            main_menu("browse.cards", "nav.browse", "search", "/browse", 35),
            main_menu("browse.maintenance", "nav.maintenance", "wrench", "/maintenance", 90),
        ],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<SearchCardsMethod>(search::cards);
    reg.query::<SearchIdsMethod>(search::ids);
    reg.query::<SearchCheck>(search::check);
    reg.query::<SavedQueriesList>(search::saved_list);
    reg.command::<SavedQueriesCreate>(search::saved_create);
    reg.command::<SavedQueriesUpdate>(search::saved_update);
    reg.command::<SavedQueriesDelete>(search::saved_delete);
    reg.command::<SavedQueriesRestore>(search::saved_restore);
    reg.command::<BulkMoveCards>(bulk::move_cards);
    reg.command::<BulkSetSuspended>(bulk::set_suspended);
    reg.command::<BulkSetBuried>(bulk::set_buried);
    reg.command::<BulkSetFlag>(bulk::set_flag);
    reg.command::<BulkResetProgress>(bulk::reset_progress);
    reg.command::<BulkAddTags>(bulk::add_tags);
    reg.command::<BulkRemoveTags>(bulk::remove_tags);
    reg.command::<BulkFindReplace>(bulk::find_replace);
    reg.command::<BulkSetAttribute>(bulk::set_attribute);
    reg.command::<BulkRestoreCards>(bulk::restore_cards);
    reg.command::<BulkRestoreNotes>(bulk::restore_notes);
    reg.command::<BulkRestoreValues>(bulk::restore_values);
    reg.query::<BrowseCardInfo>(maintenance::card_info);
    reg.query::<BrowseDuplicates>(maintenance::duplicates);
    reg.query::<BrowseTrash>(maintenance::trash);
    reg.query::<BrowseCheckDatabase>(maintenance::check_database);
    reg.command::<BrowseRepairDatabase>(maintenance::repair_database);
}

/// The wire form of a query error.
pub(crate) fn query_error_dto(e: &QueryError) -> QueryErrorDto {
    QueryErrorDto {
        kind: e.kind.code().to_owned(),
        span: e.span,
        params: e
            .kind
            .params()
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    }
}

/// `VALIDATION_QUERY` with `kind`, `start`, `end` and the kind's params as details.
pub(crate) fn query_error(e: &QueryError) -> AppError {
    let mut api = ApiError::new(ErrorCode::ValidationQuery, e.to_string())
        .with_detail("kind", e.kind.code())
        .with_detail("start", e.span.start)
        .with_detail("end", e.span.end);
    for (k, v) in e.kind.params() {
        api = api.with_detail(k, v);
    }
    AppError::Api(api)
}

/// Parses and resolves `text` against the current catalog.
pub(crate) fn compile(reads: &dyn Reads, text: &str, now: Timestamp) -> AppResult<Filter> {
    let decks = reads.list_decks(true)?;
    let tags: Vec<Tag> = reads.list_tags()?.into_iter().map(|(t, _)| t).collect();
    let notetypes = reads.list_notetypes(true)?;
    let attributes = reads.list_attribute_defs(false)?;
    let cat = Catalog {
        decks: &decks,
        tags: &tags,
        notetypes: &notetypes,
        attributes: &attributes,
        now,
    };
    compile_query(text, &cat).map_err(|e| query_error(&e))
}

/// Browser row of a hit.
pub(crate) fn row(index: &DeckIndex, h: CardHit, now: Timestamp) -> CardRow {
    let c = h.card;
    let s = c.scheduling;
    CardRow {
        card_id: c.id,
        note_id: c.note_id,
        deck_id: c.deck_id,
        deck_path: index.path(c.deck_id),
        notetype_id: h.notetype_id,
        sort_field: h.sort_field,
        template_key: c.template_key,
        card_type: c.card_type,
        state: s.state,
        due: s.due,
        stability: s.stability,
        difficulty: s.difficulty,
        reps: s.reps,
        lapses: s.lapses,
        flags: c.flags,
        suspended: c.suspended,
        buried: c.buried_until.is_some_and(|b| b > now),
        tags: h.tags,
        created_at: c.created_at,
        note_updated_at: h.note_updated_at,
    }
}
