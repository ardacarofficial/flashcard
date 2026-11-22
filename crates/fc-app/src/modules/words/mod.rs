//! `words` module (v1.4, docs/02 §24, docs/03 §4.2): the word index — one persisted answer to
//! "which words does he know", read by reading mode (`reading.analyze`,
//! `reading.words.lookup`), the Subtitle extension (`GET /subtitle/v1/known-words`) and the
//! `words.*` API.
//!
//! * [`index`]: keeping it in step (trigger-filled queues drained in every write
//!   transaction) and the full rebuild.
//! * Sources: every note whose note type has a `term` field (a note type without any role
//!   falls back to its sort field), whatever made it — by hand, an Anki import, the Subtitle
//!   extension, reading mode — plus the manual marks of reading mode.
//! * Language of a note: its Subtitle capture's, else the reading item's it was made from,
//!   else the `words.language` setting of its home deck (deck scope, then app scope). The
//!   shipped default is empty: notes whose language is not known are counted in
//!   `words.stats.unassigned_notes`, never guessed.
//! * Status: a manual mark wins, else a card whose interval reached
//!   `words.known_interval_days` (default 21) → known, else any card → learning, else new.
//!
//! The module switch hides the `words.*` methods; the index itself is derived data and is
//! always kept (reading mode and the Subtitle endpoint read it).

pub(crate) mod index;

#[cfg(test)]
mod tests;

use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::jobs::{JobContext, JobHandler};
use crate::module::ModuleDescriptor;
use crate::modules::reading::{language_or_default, lemmatizer_for, words::tokens};
use crate::modules::word_data;
use crate::registry::Registry;
use crate::support::summary;
use base64::Engine;
use fc_api::v1::common::Empty;
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::*;
use fc_api::v1::words::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::reading::{headword_key, WordCounts, WordMark, WordStatus};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::words::{index_language, DEFAULT_KNOWN_INTERVAL_DAYS};
use fc_core::{CardId, NoteId};
use fc_core::word_data::CefrBand;
use fc_storage::repo::{
    WordDataQuery, WordDataRecord, WordDataSort, WordIndexQuery, WordIndexRecord, NO_BAND, NO_RANK,
};
use fc_storage::Reads;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Module id.
pub const ID: &str = "words";
/// Setting (app or deck scope): language of the notes of a deck (BCP-47; empty = unknown).
pub const LANGUAGE: &str = "words.language";
/// Setting: a card whose interval reached this many days makes its word known.
pub const KNOWN_INTERVAL: &str = "words.known_interval_days";
/// Job kind: full rebuild.
pub const REBUILD_JOB: &str = "words.index.rebuild";

/// Most forms per `words.lookup`.
const MAX_FORMS: usize = 2000;
/// Longest form (bytes).
const MAX_FORM_BYTES: usize = 200;
/// Default / largest `words.list` page.
const DEFAULT_PAGE: u32 = 200;
const MAX_PAGE: u32 = 1000;

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.words.name",
        description_key: "modules.words.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![
            SettingDef {
                key: LANGUAGE.to_owned(),
                kind: SettingKind::Text { max_len: Some(35) },
                default: Value::String(String::new()),
                label_key: format!("settings.{LANGUAGE}.label"),
                description_key: Some(format!("settings.{LANGUAGE}.description")),
                advanced: false,
                scopes: vec![SettingScopeKind::App, SettingScopeKind::Deck],
            },
            SettingDef {
                key: KNOWN_INTERVAL.to_owned(),
                kind: SettingKind::Integer {
                    min: Some(1),
                    max: Some(3650),
                },
                default: Value::from(DEFAULT_KNOWN_INTERVAL_DAYS),
                label_key: format!("settings.{KNOWN_INTERVAL}.label"),
                description_key: Some(format!("settings.{KNOWN_INTERVAL}.description")),
                advanced: true,
                scopes: vec![SettingScopeKind::App],
            },
        ],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<WordsLookup>(lookup);
    reg.query::<WordsList>(list);
    reg.query::<WordsStats>(stats);
    reg.command::<WordsIndexRebuild>(rebuild);
    reg.command::<WordsIndexRebuildNow>(rebuild_now);
    reg.job(Arc::new(RebuildJob));
}

// ------------------------------------------------------------------ shared reads

/// The known-by-maturity threshold in force.
pub(crate) fn threshold(reads: &dyn Reads) -> AppResult<u32> {
    Ok(
        crate::settings::resolve_stored(reads, &SettingScope::App, KNOWN_INTERVAL)?
            .and_then(|(v, _)| v.as_u64())
            .and_then(|d| u32::try_from(d).ok())
            .filter(|d| *d > 0)
            .unwrap_or(DEFAULT_KNOWN_INTERVAL_DAYS),
    )
}

/// Status of a stored row (as `fc_core::words::WordEvidence::status`).
pub(crate) fn row_status(r: &WordIndexRecord, threshold_days: u32) -> WordStatus {
    match r.mark.as_deref().and_then(WordMark::parse) {
        Some(WordMark::Known) => WordStatus::Known,
        Some(WordMark::Ignored) => WordStatus::Ignored,
        None if r.cards > 0 && r.max_interval_days.is_some_and(|d| d >= threshold_days) => {
            WordStatus::Known
        }
        None if r.cards > 0 => WordStatus::Learning,
        None => WordStatus::New,
    }
}

/// Index rows of `keys` in `language` (a BCP-47 tag or primary subtag), by key. One indexed
/// query per 500 keys: no scan.
pub(crate) fn rows_of(
    reads: &dyn Reads,
    language: &str,
    keys: &[String],
) -> AppResult<HashMap<String, WordIndexRecord>> {
    let language = index_language(language);
    let mut out = HashMap::with_capacity(keys.len());
    for chunk in keys.chunks(500) {
        for r in reads.word_index_get(&language, chunk)? {
            out.insert(r.key.clone(), r);
        }
    }
    Ok(out)
}

/// The DTO of a row.
pub(crate) fn entry_dto(
    r: &WordIndexRecord,
    threshold_days: u32,
    note_ids: Vec<NoteId>,
    card_ids: Vec<CardId>,
) -> WordEntryDto {
    WordEntryDto {
        language: r.language.clone(),
        key: r.key.clone(),
        status: row_status(r, threshold_days),
        mark: r.mark.as_deref().and_then(WordMark::parse),
        mark_at: r.mark_at,
        notes: r.notes,
        cards: r.cards,
        max_interval_days: r.max_interval_days,
        known_by_maturity: r.cards > 0
            && r.max_interval_days.is_some_and(|d| d >= threshold_days),
        last_review: r.last_review,
        first_seen: r.first_seen,
        note_ids,
        card_ids,
        level: None,
    }
}

/// Opaque cursor of the page after `(language, key)`.
pub(crate) fn cursor_of(language: &str, key: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{language}\u{1f}{key}"))
}

/// `(language, key)` of a cursor.
pub(crate) fn parse_cursor(cursor: &str) -> Option<(String, String)> {
    parse_sorted_cursor(cursor).map(|(l, k, _)| (l, k))
}

/// Opaque cursor of the page after a row of a list sorted by rank or band (word data): the
/// row's `(language, key)` and its sort values.
fn sorted_cursor_of(language: &str, key: &str, band: &str, rank: u32) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(format!("{language}\u{1f}{key}\u{1f}{band}\u{1f}{rank}"))
}

/// `(language, key, (band, rank) sort values)` of a page cursor.
type SortedCursor = (String, String, Option<(String, u32)>);

/// `(language, key, sort values)` of a cursor (sort values only in sorted cursors).
fn parse_sorted_cursor(cursor: &str) -> Option<SortedCursor> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor.trim())
        .ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let parts: Vec<&str> = text.split('\u{1f}').collect();
    match parts.as_slice() {
        [l, k] => Some(((*l).to_owned(), (*k).to_owned(), None)),
        [l, k, b, r] => Some((
            (*l).to_owned(),
            (*k).to_owned(),
            Some(((*b).to_owned(), r.parse().ok()?)),
        )),
        _ => None,
    }
}

fn bad_cursor() -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationInvalidParams, "cursor is not valid")
            .with_detail("field", "cursor"),
    )
}

// ------------------------------------------------------------------ queries

fn lookup(req: LookupWords, ctx: &QueryCtx<'_>) -> AppResult<WordsLookupResult> {
    if req.forms.is_empty()
        || req.forms.len() > MAX_FORMS
        || req.forms.iter().any(|f| f.len() > MAX_FORM_BYTES)
    {
        return Err(crate::support::invalid(
            "1–2000 forms of at most 200 bytes",
        ));
    }
    let tag = language_or_default(ctx.tx, &req.language)?;
    let language = index_language(&tag);
    let lem = lemmatizer_for(&tag);
    let mut keys_of_forms = Vec::with_capacity(req.forms.len());
    let mut cache: HashMap<&str, Option<String>> = HashMap::new();
    for form in &req.forms {
        let key = match cache.get(form.trim()) {
            Some(k) => k.clone(),
            None => {
                let lemmas: Vec<String> = tokens(&*lem, form.trim(), &tag)?
                    .into_iter()
                    .map(|t| t.lemma)
                    .collect();
                let k = headword_key(&lemmas, &tag);
                cache.insert(form.trim(), k.clone());
                k
            }
        };
        keys_of_forms.push(key);
    }
    let mut keys: Vec<String> = keys_of_forms.iter().flatten().cloned().collect();
    keys.sort_unstable();
    keys.dedup();
    let rows = rows_of(ctx.tx, &language, &keys)?;
    // The evidence behind each word: its notes and their live cards (indexed reads).
    let found: Vec<String> = rows.keys().cloned().collect();
    let mut notes_of: HashMap<String, Vec<NoteId>> = HashMap::new();
    let mut all_notes = Vec::new();
    for chunk in found.chunks(500) {
        for n in ctx.tx.word_index_notes_of(&language, chunk)? {
            all_notes.push(n.note_id);
            notes_of.entry(n.key).or_default().push(n.note_id);
        }
    }
    let mut cards_of: HashMap<NoteId, Vec<CardId>> = HashMap::new();
    for chunk in all_notes.chunks(500) {
        for c in ctx.tx.word_card_rows(chunk)? {
            cards_of.entry(c.note_id).or_default().push(c.card_id);
        }
    }
    let threshold_days = threshold(ctx.tx)?;
    // Word data (v1.4 B2): rank and band of each form, and the families of their lemmas.
    let with_data = word_data::enabled(ctx.tx, ctx.registry)?;
    let data = if with_data {
        let wanted = req
            .forms
            .iter()
            .map(|f| f.trim().to_lowercase())
            .chain(keys.iter().cloned());
        word_data::rows_for(ctx.tx, &language, wanted)?
    } else {
        HashMap::new()
    };
    let levels: Vec<Option<WordLevelDto>> = req
        .forms
        .iter()
        .zip(&keys_of_forms)
        .map(|(form, key)| {
            let key = key.as_deref()?;
            let form = form.trim().to_lowercase();
            word_data::level_of(&language, Some(&form), key, &data)
        })
        .collect();
    let lemmas: Vec<String> = levels
        .iter()
        .flatten()
        .filter_map(|l| l.lemma.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let families = if with_data {
        word_data::families(ctx.tx, &language, &lemmas, threshold_days)?
    } else {
        HashMap::new()
    };
    let forms = req
        .forms
        .iter()
        .zip(keys_of_forms)
        .zip(levels)
        .map(|((form, key), level)| {
            let word = key.as_ref().and_then(|k| rows.get(k)).map(|r| {
                let notes = notes_of.get(&r.key).cloned().unwrap_or_default();
                let cards = notes
                    .iter()
                    .flat_map(|n| cards_of.get(n).into_iter().flatten().copied())
                    .collect();
                let mut dto = entry_dto(r, threshold_days, notes, cards);
                dto.level.clone_from(&level);
                dto
            });
            let family = level
                .as_ref()
                .and_then(|l| l.lemma.as_ref())
                .and_then(|l| families.get(l))
                .cloned()
                .unwrap_or_default();
            WordFormResult {
                form: form.clone(),
                status: word.as_ref().map_or(WordStatus::New, |w| w.status),
                key,
                word,
                level,
                family,
            }
        })
        .collect();
    Ok(WordsLookupResult {
        language,
        lemmatizer: lem.info().id,
        threshold_days,
        forms,
    })
}

fn list(req: ListWords, ctx: &QueryCtx<'_>) -> AppResult<WordList> {
    let limit = req.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let sort = req.sort.unwrap_or_default();
    let with_data = word_data::enabled(ctx.tx, ctx.registry)?;
    if !with_data && (sort != WordSort::Key || !req.bands.is_empty() || req.max_rank.is_some()) {
        return Err(AppError::Api(ApiError::new(
            ErrorCode::ConflictModuleDisabled,
            "sorting or filtering by frequency needs the word-data module",
        )));
    }
    let (after, after_sort) = match req.cursor.as_deref().filter(|c| !c.trim().is_empty()) {
        Some(c) => {
            let (l, k, s) = parse_sorted_cursor(c).ok_or_else(bad_cursor)?;
            if sort != WordSort::Key && s.is_none() {
                return Err(bad_cursor());
            }
            (Some((l, k)), s)
        }
        None => (None, None),
    };
    let language = req
        .language
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(index_language);
    let threshold_days = threshold(ctx.tx)?;
    let q = WordIndexQuery {
        language,
        statuses: req.status.map(|s| s.as_str().to_owned()).into_iter().collect(),
        threshold_days,
        after,
        limit: limit.saturating_add(1),
    };
    let rows: Vec<(WordIndexRecord, Option<WordDataRecord>)> = if with_data {
        ctx.tx.word_index_page_with_data(
            &q,
            &WordDataQuery {
                sort: match sort {
                    WordSort::Key => WordDataSort::Key,
                    WordSort::Rank => WordDataSort::Rank,
                    WordSort::Band => WordDataSort::Band,
                },
                bands: req.bands.iter().map(|b| b.as_str().to_owned()).collect(),
                max_rank: req.max_rank,
                after_sort,
            },
        )?
    } else {
        ctx.tx
            .word_index_page(&q)?
            .into_iter()
            .map(|r| (r, None))
            .collect()
    };
    let more = rows.len() > limit as usize;
    let page = &rows[..rows.len().min(limit as usize)];
    // Expressions without a pack row of their own get the expression rule's band.
    let mut parts: HashMap<String, HashMap<String, WordDataRecord>> = HashMap::new();
    if with_data {
        let mut by_lang: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for (r, d) in page {
            if d.is_none() && r.key.contains(' ') {
                by_lang.entry(&r.language).or_default().push(r.key.clone());
            }
        }
        for (l, keys) in by_lang {
            parts.insert(l.to_owned(), word_data::rows_for(ctx.tx, l, keys)?);
        }
    }
    let words: Vec<WordEntryDto> = page
        .iter()
        .map(|(r, d)| {
            let mut dto = entry_dto(r, threshold_days, vec![], vec![]);
            if with_data {
                dto.level = match d {
                    Some(d) => word_data::level_of(
                        &r.language,
                        None,
                        &r.key,
                        &HashMap::from([(r.key.clone(), d.clone())]),
                    ),
                    None => parts
                        .get(&r.language)
                        .and_then(|rows| word_data::level_of(&r.language, None, &r.key, rows)),
                };
            }
            dto
        })
        .collect();
    let next_cursor = if more {
        page.last().map(|(r, d)| match sort {
            WordSort::Key => cursor_of(&r.language, &r.key),
            WordSort::Rank | WordSort::Band => sorted_cursor_of(
                &r.language,
                &r.key,
                d.as_ref().and_then(|d| d.band.as_deref()).unwrap_or(NO_BAND),
                d.as_ref().and_then(|d| d.rank).unwrap_or(NO_RANK),
            ),
        })
    } else {
        None
    };
    Ok(WordList { words, next_cursor })
}

fn stats(_req: Empty, ctx: &QueryCtx<'_>) -> AppResult<WordStats> {
    let threshold_days = threshold(ctx.tx)?;
    let mut per: BTreeMap<String, WordCounts> = BTreeMap::new();
    for (language, status, n) in ctx.tx.word_index_counts(threshold_days)? {
        let counts = per.entry(language).or_default();
        let s = WordStatus::ALL
            .into_iter()
            .find(|s| s.as_str() == status)
            .unwrap_or(WordStatus::New);
        for _ in 0..n {
            counts.add(s);
        }
    }
    // Word data (v1.4 B2): the same counts per CEFR band.
    let mut bands: BTreeMap<String, BTreeMap<Option<CefrBand>, WordCounts>> = BTreeMap::new();
    if word_data::enabled(ctx.tx, ctx.registry)? {
        for (language, band, status, n) in ctx.tx.word_index_band_counts(threshold_days)? {
            let s = WordStatus::ALL
                .into_iter()
                .find(|s| s.as_str() == status)
                .unwrap_or(WordStatus::New);
            let counts = bands
                .entry(language)
                .or_default()
                .entry(band.as_deref().and_then(CefrBand::parse))
                .or_default();
            for _ in 0..n {
                counts.add(s);
            }
        }
    }
    Ok(WordStats {
        threshold_days,
        languages: per
            .into_iter()
            .map(|(language, counts)| LanguageWordCounts {
                bands: bands
                    .remove(&language)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(band, counts)| BandWordCounts { band, counts })
                    .collect(),
                language,
                counts,
            })
            .collect(),
        unassigned_notes: ctx.tx.word_index_unassigned()?,
    })
}

// ------------------------------------------------------------------ rebuild

fn rebuild(_req: Empty, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<JobDto>> {
    let job = crate::jobs::enqueue(ctx, REBUILD_JOB, Value::Object(Default::default()))?;
    Ok(CommandOutcome {
        response: job,
        inverse: vec![],
        summary: summary::<WordsIndexRebuild>(),
        events: vec![],
    })
}

fn rebuild_now(
    _req: Empty,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<WordIndexRebuilt>> {
    index::rebuild(ctx.tx)?;
    let (words, notes) = ctx.reads().word_index_dump()?;
    Ok(CommandOutcome {
        response: WordIndexRebuilt {
            words: u32::try_from(words.len()).unwrap_or(u32::MAX),
            notes: u32::try_from(notes.len()).unwrap_or(u32::MAX),
        },
        inverse: vec![],
        summary: summary::<WordsIndexRebuildNow>(),
        events: vec![],
    })
}

struct RebuildJob;

impl JobHandler for RebuildJob {
    fn kind(&self) -> &'static str {
        REBUILD_JOB
    }

    fn run(&self, _payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        ctx.ensure_module_enabled(ID)?;
        ctx.cancel.check()?;
        ctx.report(None, "words.job.rebuilding");
        let done = ctx.invoke::<WordsIndexRebuildNow>(&Empty {})?;
        serde_json::to_value(done).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
    }
}
