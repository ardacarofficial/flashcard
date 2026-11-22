//! `word_data` module (v1.4 B2, docs/02 §24, docs/03 §4.3): how often a word is heard and an
//! estimated CEFR band, from a versioned local word-data pack per language, so the owner can
//! spend time first on the words he will hear most.
//!
//! * Packs: the app ships the English one (`fc_providers::word_data`, CC BY-SA 4.0 with the
//!   CEFR-J citation) and installs it at start-up ([`install_shipped`]: once, and again only
//!   when the shipped bytes change). `words.data.import` replaces a language's pack with one
//!   from a file or text; a shipped pack never overwrites an imported one, and
//!   `words.data.reset` puts the shipped one back. `words.data.sources` lists the installed
//!   packs with their sources, licences and attributions. No network at runtime.
//! * Enrichment ([`level_of`], [`families`]): `words.lookup` / `words.list` / `words.stats`
//!   carry rank and band (with whether the band is from a profile list or estimated), and
//!   `words.list` sorts and filters by them. Inflection joins the lemma (`went` has `go`'s
//!   rank); a derived word keeps its own entry and its family is only shown — knowing `act`
//!   never makes `actor` known.
//! * Switched off: the `words.*` results carry no level or family, and `words.list` refuses
//!   to sort or filter by them. The installed packs stay (reference data).
//!
//! Pack writes are reference data, not the user's: like the word-index rebuild they are
//! commands without undo (`words.data.reset` is the way back).

#[cfg(test)]
mod tests;

use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::module::ModuleDescriptor;
use crate::registry::Registry;
use crate::support::summary;
use fc_api::v1::common::Empty;
use fc_api::v1::io::ImportSource;
use fc_api::v1::methods::*;
use fc_api::v1::words::*;
use fc_api::ErrorCode;
use fc_core::note::fnv1a64;
use fc_core::reading::WordStatus;
use fc_core::word_data::{expression_band, BandOrigin, CefrBand, PackSource, WordPack};
use fc_core::words::index_language;
use fc_core::Timestamp;
use fc_storage::repo::{WordDataPackRecord, WordDataRecord};
use fc_storage::{Reads, WriteTx};
use std::collections::{BTreeSet, HashMap};
use fc_providers::word_data::ShippedPack;
use std::sync::{Arc, Mutex, OnceLock};

/// Module id.
pub const ID: &str = "word_data";
/// Largest pack accepted by `words.data.import` (bytes).
const MAX_PACK_BYTES: usize = 64 * 1024 * 1024;

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.word_data.name",
        description_key: "modules.word_data.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<WordsDataSources>(sources);
    reg.command::<WordsDataImport>(import);
    reg.command::<WordsDataReset>(reset);
}

// ------------------------------------------------------------------ packs

/// Checksum of a pack's bytes.
fn checksum(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a64(bytes))
}

/// A shipped pack, parsed once per process (by checksum). A shipped pack that does not parse
/// is a build error caught by `fc_providers::word_data`'s test; here it is an internal error.
fn parsed(p: &ShippedPack) -> AppResult<Arc<WordPack>> {
    static PACKS: OnceLock<Mutex<HashMap<&'static str, Arc<WordPack>>>> = OnceLock::new();
    let cache = PACKS.get_or_init(Default::default);
    if let Some(pack) = cache.lock().ok().and_then(|c| c.get(p.checksum).cloned()) {
        return Ok(pack);
    }
    let pack = Arc::new(
        WordPack::parse(p.bytes).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?,
    );
    if let Ok(mut c) = cache.lock() {
        c.insert(p.checksum, pack.clone());
    }
    Ok(pack)
}

/// Writes `pack` as `language`'s installed pack.
fn install(
    tx: &mut dyn WriteTx,
    pack: &WordPack,
    origin: &str,
    checksum: String,
    now: Timestamp,
) -> AppResult<WordDataPackRecord> {
    let rows: Vec<WordDataRecord> = pack
        .rows()
        .into_iter()
        .map(|r| WordDataRecord {
            language: pack.language.clone(),
            form: r.form,
            lemma: r.lemma,
            rank: r.rank,
            band: r.band.map(|b| b.as_str().to_owned()),
            band_origin: r.origin.map(|o| o.as_str().to_owned()),
        })
        .collect();
    let families = pack.families();
    let record = WordDataPackRecord {
        language: pack.language.clone(),
        version: pack.version.clone(),
        name: pack.name.clone(),
        licence: pack.licence.clone(),
        sources: serde_json::to_value(&pack.sources)
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?,
        notes: pack.notes.clone(),
        origin: origin.to_owned(),
        words: u32::try_from(pack.words.len()).unwrap_or(u32::MAX),
        forms: u32::try_from(rows.len()).unwrap_or(u32::MAX),
        families: u32::try_from(families.len()).unwrap_or(u32::MAX),
        checksum,
        installed_at: now,
    };
    tx.replace_word_data(&record, &rows, &families)?;
    Ok(record)
}

/// Start-up: installs each of `packs` whose language has no pack yet, or whose installed
/// shipped pack has another checksum (a new app version). An imported pack is left alone.
/// Up to date, this is one read of the (one row per language) pack table: no hashing, no
/// parsing (`fc_providers::word_data::ShippedPack::checksum` is fixed at build time).
pub(crate) fn install_shipped(
    tx: &mut dyn WriteTx,
    now: Timestamp,
    packs: &[ShippedPack],
) -> AppResult<()> {
    let installed: HashMap<String, (String, String)> = tx
        .word_data_packs()?
        .into_iter()
        .map(|p| (p.language, (p.origin, p.checksum)))
        .collect();
    for shipped in packs {
        let stale = match installed.get(shipped.language) {
            None => true,
            Some((origin, sum)) => origin == "bundled" && sum != shipped.checksum,
        };
        if stale {
            install(tx, &*parsed(shipped)?, "bundled", shipped.checksum.to_owned(), now)?;
        }
    }
    Ok(())
}

fn pack_dto(p: WordDataPackRecord) -> WordDataPackDto {
    WordDataPackDto {
        sources: serde_json::from_value::<Vec<PackSource>>(p.sources).unwrap_or_default(),
        language: p.language,
        version: p.version,
        name: p.name,
        licence: p.licence,
        notes: p.notes,
        origin: p.origin,
        words: p.words,
        forms: p.forms,
        families: p.families,
        installed_at: p.installed_at,
    }
}

fn sources_of(reads: &dyn Reads) -> AppResult<WordDataSources> {
    Ok(WordDataSources {
        packs: reads.word_data_packs()?.into_iter().map(pack_dto).collect(),
        bundled: fc_providers::word_data::BUNDLED
            .iter()
            .map(|p| {
                Ok(BundledPackDto {
                    language: p.language.to_owned(),
                    version: parsed(p)?.version.clone(),
                })
            })
            .collect::<AppResult<_>>()?,
    })
}

fn sources(_req: Empty, ctx: &QueryCtx<'_>) -> AppResult<WordDataSources> {
    sources_of(ctx.tx)
}

fn import(req: ImportWordData, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<WordDataPackDto>> {
    let bytes = match &req.source {
        ImportSource::File { path } if path.trim().is_empty() => {
            return Err(crate::support::invalid("path is empty"))
        }
        ImportSource::File { path } => crate::modules::io::read_file(path)?,
        ImportSource::Text { text } => text.as_bytes().to_vec(),
    };
    if bytes.len() > MAX_PACK_BYTES {
        return Err(crate::support::invalid("the pack is larger than 64 MB"));
    }
    let pack = WordPack::parse(&bytes).map_err(|e| crate::support::invalid(e.to_string()))?;
    let record = install(ctx.tx, &pack, "imported", checksum(&bytes), ctx.now)?;
    Ok(CommandOutcome {
        response: pack_dto(record),
        inverse: vec![],
        summary: summary::<WordsDataImport>(),
        events: vec![],
    })
}

fn reset(req: ResetWordData, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<WordDataSources>> {
    let language = index_language(&req.language);
    if language.is_empty() {
        return Err(crate::support::invalid("language is not a BCP-47 tag"));
    }
    match fc_providers::word_data::bundled(&language) {
        Some(shipped) => {
            install(ctx.tx, &*parsed(shipped)?, "bundled", shipped.checksum.to_owned(), ctx.now)?;
        }
        None => ctx.tx.remove_word_data(&language)?,
    }
    Ok(CommandOutcome {
        response: sources_of(ctx.reads())?,
        inverse: vec![],
        summary: summary::<WordsDataReset>(),
        events: vec![],
    })
}

// ------------------------------------------------------------------ enrichment

/// Whether the module is on (enrichment and the rank/band options of `words.list`).
pub(crate) fn enabled(reads: &dyn Reads, registry: &Registry) -> AppResult<bool> {
    crate::support::module_enabled(reads, registry, ID)
}

/// Pack rows of `forms` in `language` (index language), by form; also the single words of
/// multi-word forms (for the expression rule). Indexed reads, 500 forms per query.
pub(crate) fn rows_for(
    reads: &dyn Reads,
    language: &str,
    forms: impl IntoIterator<Item = String>,
) -> AppResult<HashMap<String, WordDataRecord>> {
    let mut wanted = BTreeSet::new();
    for f in forms {
        if f.contains(' ') {
            wanted.extend(f.split(' ').filter(|w| !w.is_empty()).map(str::to_owned));
        }
        wanted.insert(f);
    }
    let wanted: Vec<String> = wanted.into_iter().collect();
    let mut out = HashMap::with_capacity(wanted.len());
    for chunk in wanted.chunks(500) {
        for r in reads.word_data_get(language, chunk)? {
            out.insert(r.form.clone(), r);
        }
    }
    Ok(out)
}

fn dto_of(r: &WordDataRecord) -> WordLevelDto {
    let origin = r.band_origin.as_deref().and_then(BandOrigin::parse);
    WordLevelDto {
        lemma: Some(r.lemma.clone()),
        rank: r.rank,
        band: r.band.as_deref().and_then(CefrBand::parse),
        band_origin: origin,
        estimated: origin.is_none_or(BandOrigin::estimated),
    }
}

/// Level of a word: the pack row of the form as written (`things` → `thing`, `tired` keeps
/// its own; an expression that is a profile headword such as `according to`), else of its
/// index key; for an expression without a row of its own, the expression rule of `fc_core::word_data::expression_band`
/// (estimated). `rows` from [`rows_for`] with the form and the key.
pub(crate) fn level_of(
    language: &str,
    form: Option<&str>,
    key: &str,
    rows: &HashMap<String, WordDataRecord>,
) -> Option<WordLevelDto> {
    if let Some(r) = form
        .and_then(|f| rows.get(f))
        .or_else(|| rows.get(key))
    {
        return Some(dto_of(r));
    }
    if !key.contains(' ') {
        return None;
    }
    let words: Vec<(&str, Option<CefrBand>)> = key
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let band = rows
                .get(w)
                .and_then(|r| r.band.as_deref())
                .and_then(CefrBand::parse);
            (w, band)
        })
        .collect();
    expression_band(language, &words).map(|(band, _)| WordLevelDto {
        lemma: None,
        rank: None,
        band: Some(band),
        band_origin: Some(BandOrigin::Expression),
        estimated: true,
    })
}

/// The word families of `lemmas` (pack lemmas) in `language`: lemma → the other members
/// (the head first), each with its own status from the index. Two indexed reads plus the
/// index rows of the members.
pub(crate) fn families(
    reads: &dyn Reads,
    language: &str,
    lemmas: &[String],
    threshold_days: u32,
) -> AppResult<HashMap<String, Vec<FamilyMemberDto>>> {
    if lemmas.is_empty() {
        return Ok(HashMap::new());
    }
    // Head of each asked lemma: its link's head, or itself when it heads a family.
    let mut head_of: HashMap<String, String> = HashMap::new();
    for chunk in lemmas.chunks(500) {
        for (m, h) in reads.word_families_of(language, chunk)? {
            if chunk.contains(&m) {
                head_of.insert(m, h.clone());
            }
            if chunk.contains(&h) {
                head_of.entry(h.clone()).or_insert(h);
            }
        }
    }
    let heads: Vec<String> = head_of
        .values()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut members: HashMap<String, Vec<String>> = HashMap::new();
    for chunk in heads.chunks(500) {
        for (m, h) in reads.word_families_of(language, chunk)? {
            members.entry(h).or_default().push(m);
        }
    }
    let all: Vec<String> = members
        .iter()
        .flat_map(|(h, ms)| std::iter::once(h.clone()).chain(ms.iter().cloned()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let index = crate::modules::words::rows_of(reads, language, &all)?;
    let status = |k: &str| {
        index
            .get(k)
            .map_or(WordStatus::New, |r| crate::modules::words::row_status(r, threshold_days))
    };
    Ok(head_of
        .into_iter()
        .map(|(lemma, head)| {
            let mut out = Vec::new();
            if head != lemma {
                out.push(FamilyMemberDto {
                    status: status(&head),
                    key: head.clone(),
                    head: true,
                });
            }
            let mut ms = members.get(&head).cloned().unwrap_or_default();
            ms.sort();
            for m in ms.into_iter().filter(|m| *m != lemma) {
                out.push(FamilyMemberDto {
                    status: status(&m),
                    key: m,
                    head: false,
                });
            }
            (lemma, out)
        })
        .collect())
}
