//! A plugin's deck limit on the word index (v1.4, docs/05 §13).
//!
//! The word index (`words.*`) answers "which words does the user know" across every deck. A
//! plugin is granted it explicitly (`words:read`), and the user may further limit it to some
//! decks (`plugins.word_decks.set`, kept host-side in the plugin's settings scope under a key
//! no plugin schema can name). Under a limit the host cuts every word-index answer **after**
//! the call and before the plugin sees it:
//!
//! * `words.lookup`: a word shows only when it has notes and **every** one of them lives in an
//!   allowed deck (or below one); otherwise the form comes back as `new` with no evidence —
//!   what it would say if those other notes did not exist and nothing else knew the word.
//!   (A word with notes on both sides is hidden whole: its status mixes both.)
//!   Family members (word data) keep their keys but read `new`.
//! * `words.list`: rows that fail the same test are dropped (a page may come back shorter;
//!   `next_cursor` stays valid).
//! * `words.stats` and reading mode's word colours (`reading.analyze`,
//!   `reading.words.lookup`, which read the same index): refused.
//!
//! Manual marks belong to no deck: a word known only by a mark is not shown to a limited
//! plugin. The limit covers the word index only; it does not narrow `notes:read` or
//! `search:read` (docs/05 §13 says so).

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::support::decks::DeckIndex;
use crate::support::{err_id, summary};
use crate::App;
use fc_api::v1::methods::*;
use fc_api::v1::plugins::{PluginRef, PluginWordDecks, SetPluginWordDecks};
use fc_api::v1::words::{WordList, WordsLookupResult};
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::reading::WordStatus;
use fc_core::settings::SettingScope;
use fc_core::{DeckId, NoteId};
use fc_storage::Reads;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

/// Where the limit is kept: the plugin's settings scope, under a key no plugin schema can
/// name (schema keys are `[a-z0-9_.]`), so the plugin's own `settings.*` host functions
/// never reach it.
pub(crate) const KEY: &str = "host:word_decks";

/// Methods cut under a limit.
const CUT: &[&str] = &[WordsLookup::NAME, WordsList::NAME];
/// Methods refused under a limit (they answer about every deck at once).
const REFUSED: &[&str] = &[
    WordsStats::NAME,
    ReadingAnalyze::NAME,
    ReadingWordsLookup::NAME,
];

fn stored(reads: &dyn Reads, plugin: &str) -> AppResult<Option<Vec<DeckId>>> {
    Ok(reads
        .get_setting(&SettingScope::Plugin(plugin.to_owned()), KEY)?
        .and_then(|v| serde_json::from_value::<Vec<DeckId>>(v).ok()))
}

/// The plugin's limit: allowed decks (each limit deck and its live sub-decks), or `None`.
fn allowed(reads: &dyn Reads, plugin: &str) -> AppResult<Option<BTreeSet<DeckId>>> {
    let Some(decks) = stored(reads, plugin)? else {
        return Ok(None);
    };
    let index = DeckIndex::load(reads)?;
    Ok(Some(
        decks
            .into_iter()
            .flat_map(|d| index.live_subtree(d))
            .collect(),
    ))
}

/// Whether every note behind a word is in `allowed` (and there is at least one).
fn all_inside(
    reads: &dyn Reads,
    notes: &[NoteId],
    allowed: &BTreeSet<DeckId>,
) -> AppResult<HashMap<NoteId, bool>> {
    let mut inside = HashMap::with_capacity(notes.len());
    for chunk in notes.chunks(500) {
        for s in reads.word_note_sources(chunk)? {
            inside.insert(s.note_id, allowed.contains(&s.home_deck_id));
        }
    }
    Ok(inside)
}

fn word_ok(notes: &[NoteId], inside: &HashMap<NoteId, bool>) -> bool {
    let live: Vec<bool> = notes.iter().filter_map(|n| inside.get(n).copied()).collect();
    !live.is_empty() && live.iter().all(|ok| *ok)
}

fn internal(e: impl std::fmt::Display) -> AppError {
    AppError::api(ErrorCode::Internal, e.to_string())
}

/// Before a plugin's call of `method`: refuses the whole-index reads under a limit.
pub(crate) fn check(app: &App, plugin: &str, method: &str) -> AppResult<()> {
    if !REFUSED.contains(&method) {
        return Ok(());
    }
    let tx = app.store().read()?;
    if stored(&*tx, plugin)?.is_none() {
        return Ok(());
    }
    Err(AppError::Api(
        ApiError::new(
            ErrorCode::PermissionDenied,
            format!(
                "{method}: this plugin may learn words from some decks only; use words.lookup \
or words.list"
            ),
        )
        .with_detail("method", method)
        .with_detail("permission", "words:read"),
    ))
}

/// After a plugin's call of `method`: cuts a word-index answer to the plugin's decks.
pub(crate) fn cut(app: &App, plugin: &str, method: &str, value: Value) -> AppResult<Value> {
    if !CUT.contains(&method) {
        return Ok(value);
    }
    let tx = app.store().read()?;
    let reads: &dyn Reads = &*tx;
    let Some(allowed) = allowed(reads, plugin)? else {
        return Ok(value);
    };
    if method == WordsLookup::NAME {
        let mut r: WordsLookupResult = serde_json::from_value(value).map_err(internal)?;
        let notes: Vec<NoteId> = r
            .forms
            .iter()
            .filter_map(|f| f.word.as_ref())
            .flat_map(|w| w.note_ids.iter().copied())
            .collect();
        let inside = all_inside(reads, &notes, &allowed)?;
        for f in &mut r.forms {
            if !f.word.as_ref().is_some_and(|w| word_ok(&w.note_ids, &inside)) {
                f.word = None;
                f.status = WordStatus::New;
            }
            // Word data (v1.4 B2): family members are pack reference data, but their statuses
            // come from the whole index, so a limited plugin sees them as `new`.
            for m in &mut f.family {
                m.status = WordStatus::New;
            }
        }
        return serde_json::to_value(r).map_err(internal);
    }
    let mut page: WordList = serde_json::from_value(value).map_err(internal)?;
    let mut notes_of: HashMap<(String, String), Vec<NoteId>> = HashMap::new();
    let mut by_language: HashMap<String, Vec<String>> = HashMap::new();
    for w in &page.words {
        by_language
            .entry(w.language.clone())
            .or_default()
            .push(w.key.clone());
    }
    for (language, keys) in &by_language {
        for n in reads.word_index_notes_of(language, keys)? {
            notes_of
                .entry((n.language.clone(), n.key.clone()))
                .or_default()
                .push(n.note_id);
        }
    }
    let all: Vec<NoteId> = notes_of.values().flatten().copied().collect();
    let inside = all_inside(reads, &all, &allowed)?;
    page.words.retain(|w| {
        notes_of
            .get(&(w.language.clone(), w.key.clone()))
            .is_some_and(|n| word_ok(n, &inside))
    });
    serde_json::to_value(page).map_err(internal)
}

// ------------------------------------------------------------------ the user's methods

fn dto(id: &str, deck_ids: Option<Vec<DeckId>>) -> PluginWordDecks {
    PluginWordDecks {
        id: id.to_owned(),
        deck_ids,
    }
}

/// `plugins.word_decks.get`.
pub(super) fn get(req: PluginRef, ctx: &QueryCtx<'_>) -> AppResult<PluginWordDecks> {
    super::live(ctx.tx, &req.id)?;
    Ok(dto(&req.id, stored(ctx.tx, &req.id)?))
}

/// `plugins.word_decks.set`: live decks only, each once; `null` lifts the limit.
pub(super) fn set(
    req: SetPluginWordDecks,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<PluginWordDecks>> {
    let record = super::live(ctx.reads(), &req.id)?;
    let decks = match req.deck_ids {
        Some(ids) => {
            let index = DeckIndex::load(ctx.reads())?;
            let mut seen = BTreeSet::new();
            let mut out = Vec::new();
            for id in ids {
                index.require_live(id).map_err(|_| {
                    err_id(ErrorCode::NotFoundDeck, format!("deck {id} not found"), id)
                })?;
                if seen.insert(id) {
                    out.push(id);
                }
            }
            Some(out)
        }
        None => None,
    };
    let old = stored(ctx.reads(), &req.id)?;
    let scope = SettingScope::Plugin(req.id.clone());
    match &decks {
        Some(ids) => {
            let v = serde_json::to_value(ids).map_err(internal)?;
            ctx.tx.set_setting(&scope, KEY, &v, ctx.now)?;
        }
        None => {
            ctx.tx.delete_setting(&scope, KEY)?;
        }
    }
    let inverse = if old == decks {
        vec![]
    } else {
        vec![inverse_call::<PluginsWordDecksSet>(&SetPluginWordDecks {
            id: req.id.clone(),
            deck_ids: old,
        })?]
    };
    Ok(CommandOutcome {
        response: dto(&req.id, decks),
        inverse,
        summary: summary::<PluginsWordDecksSet>().param("name", super::name_of(&record)),
        events: vec![super::changed(&req.id)],
    })
}
