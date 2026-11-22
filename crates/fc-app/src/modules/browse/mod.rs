//! `browse` module (v0.4, docs/02 §9–11): the query language behind search and saved queries,
//! the card browser's bulk operations, card info, duplicates, trash and the database check.
//!
//! Every query text goes through [`compile`]: parse + resolve against a fresh catalog snapshot
//! (decks, tags, note types, classifications), so names always mean what they mean *now*.

mod bulk;
mod maintenance;
mod notetype_change;
mod search;
pub(crate) use search::require as require_saved_query;
#[cfg(test)]
mod tests;
mod trash;

pub use trash::{purge_on_startup, PurgeTrashJob, DEFAULT_RETENTION_DAYS, RETENTION_KEY};

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
        settings: vec![trash::retention_setting()],
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
    reg.command::<BulkChangeNotetype>(notetype_change::change_notetype);
    reg.command::<BulkRestoreCards>(bulk::restore_cards);
    reg.command::<BulkRestoreNotes>(bulk::restore_notes);
    reg.command::<BulkRestoreValues>(bulk::restore_values);
    reg.query::<BrowseCardInfo>(maintenance::card_info);
    reg.query::<BrowseDuplicates>(maintenance::duplicates);
    reg.query::<BrowseTrash>(maintenance::trash);
    reg.query::<BrowseCheckDatabase>(maintenance::check_database);
    reg.command::<BrowseRepairDatabase>(maintenance::repair_database);
    reg.command::<BrowsePurgeTrash>(trash::purge);
    reg.job(std::sync::Arc::new(PurgeTrashJob));
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

/// Owned snapshot of everything query names resolve against.
pub(crate) struct CatalogData {
    decks: Vec<fc_core::deck::Deck>,
    tags: Vec<Tag>,
    notetypes: Vec<fc_core::notetype::NoteType>,
    attributes: Vec<fc_core::attribute::AttributeDef>,
    flag_names: Vec<(String, u32)>,
    now: Timestamp,
    /// Start of the study day (v1.3: `reviewed:1` = today).
    day_start: Timestamp,
    /// v1.4: decay of decks with personalised preset weights (`retrievability`).
    deck_decay: Vec<(fc_core::DeckId, f64)>,
    /// v1.4: word index threshold (`durum:`/`status:`).
    known_interval_days: u32,
    /// v1.4: lemma keys of the query's `kelime:`/`word:` values ([`Self::lemmatize`]).
    word_keys: Vec<(String, Vec<String>)>,
}

impl CatalogData {
    /// Loads the current catalog.
    pub(crate) fn load(reads: &dyn Reads, now: Timestamp) -> AppResult<Self> {
        Ok(Self {
            decks: reads.list_decks(true)?,
            tags: reads.list_tags_uncounted()?,
            notetypes: reads.list_notetypes(true)?,
            attributes: reads.list_attribute_defs(false)?,
            flag_names: flag_names(reads)?,
            now,
            day_start: crate::modules::study::today::today_start(reads, now)?,
            deck_decay: crate::modules::stats::facts::deck_decays(reads)?,
            known_interval_days: crate::modules::words::threshold(reads)?,
            word_keys: Vec::new(),
        })
    }

    /// v1.4: lemmatizes the `kelime:`/`word:` values of `text` (`went` → `go`) in every
    /// language of the word index (the reading default when it is empty), with the same
    /// lemmatizers and key rule the index uses. Unparsable text is left to the resolver.
    pub(crate) fn lemmatize(&mut self, reads: &dyn Reads, text: &str) -> AppResult<()> {
        let Ok(expr) = fc_core::query::parse_query(text) else {
            return Ok(());
        };
        let values = fc_core::query::lang::word_values(&expr);
        if values.is_empty() {
            return Ok(());
        }
        let mut langs: Vec<String> = reads
            .word_index_counts(self.known_interval_days)?
            .into_iter()
            .map(|(l, _, _)| l)
            .filter(|l| !l.is_empty())
            .collect();
        langs.sort();
        langs.dedup();
        if langs.is_empty() {
            langs.push(crate::modules::reading::language_or_default(reads, "")?);
        }
        for v in values {
            if self.word_keys.iter().any(|(w, _)| *w == v) {
                continue;
            }
            let mut keys: Vec<String> = Vec::new();
            for l in &langs {
                let lem = crate::modules::reading::lemmatizer_for(l);
                let lemmas: Vec<String> = crate::modules::reading::words::tokens(&*lem, v.trim(), l)?
                    .into_iter()
                    .map(|t| t.lemma)
                    .collect();
                if let Some(k) = fc_core::reading::headword_key(&lemmas, l) {
                    if !keys.contains(&k) {
                        keys.push(k);
                    }
                }
            }
            self.word_keys.push((v, keys));
        }
        Ok(())
    }

    /// The borrowed view the resolver takes.
    pub(crate) fn catalog(&self) -> Catalog<'_> {
        Catalog {
            decks: &self.decks,
            tags: &self.tags,
            notetypes: &self.notetypes,
            attributes: &self.attributes,
            flag_names: &self.flag_names,
            now: self.now,
            day_start: Some(self.day_start),
            deck_decay: &self.deck_decay,
            known_interval_days: self.known_interval_days,
            word_keys: &self.word_keys,
        }
    }
}

/// User-defined flag names for `flag:<name>` (in addition to the built-in color names,
/// `fc_core::query::resolve::FLAG_NAMES`, numbered like Anki: 1 red … 7 purple).
///
/// Bound to v0.3's `card_flag_defs` (the single source of flag names): every flag with a
/// custom name as `(name, flag number)`. Numbers are Anki's (1 red … 7 purple; bit = n − 1);
/// the built-in color names stay valid in both languages (`resolve::FLAG_NAMES`). A name with
/// spaces is also accepted with `_` (`flag:çok_önemli`), like classification keys.
fn flag_names(reads: &dyn Reads) -> AppResult<Vec<(String, u32)>> {
    let mut out = Vec::new();
    for d in reads.list_flag_defs()? {
        let Some(name) = d.name.filter(|n| !n.trim().is_empty()) else {
            continue;
        };
        let n = u32::from(d.flag);
        let underscored = name.split_whitespace().collect::<Vec<_>>().join("_");
        if underscored != name.trim() {
            out.push((underscored, n));
        }
        out.push((name, n));
    }
    Ok(out)
}

/// Parses and resolves `text` against the current catalog.
pub(crate) fn compile(reads: &dyn Reads, text: &str, now: Timestamp) -> AppResult<Filter> {
    let mut data = CatalogData::load(reads, now)?;
    data.lemmatize(reads, text)?;
    compile_query(text, &data.catalog()).map_err(|e| query_error(&e))
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

#[cfg(test)]
mod lang_tests;
