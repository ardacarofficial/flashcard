//! `examples` module (v1.4 W3, docs/02 §24.3): **the most teachable scene.**
//!
//! Ranks a word's example sentences with the pure scoring of [`fc_core::encounter_score`]: the
//! words of each sentence come from the language provider (`reading` lemmatizers), their
//! status from the word index (`words`) and their frequency rank from the word data pack
//! (`word_data`, when that module is on and a pack covers the language).
//!
//! * `subtitle.encounters.rank`: a Subtitle note's encounters, best first, with reasons and
//!   near-duplicates. Input order is the primary first, so a tie never displaces it.
//! * `subtitle.encounters.suggest_primary`: proposes a better example (a clear margin,
//!   [`SUGGEST_MARGIN`]); it never switches. Applying it is `subtitle.encounters.set_primary`.
//! * `examples.rank_sentences`: candidate sentences (reading mode) ranked the same way.
//! * Setting [`AUTO_BEST`] (default **off**): when a capture becomes another example of a
//!   note, the suggestion is applied at once through `subtitle.encounters.set_primary` — inside
//!   the capture's undo step; card, schedule and history stay.

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, QueryCtx};
use crate::module::ModuleDescriptor;
use crate::modules::reading::{language_or_default, lemmatizer_for, words::tokens};
use crate::modules::{word_data, words};
use crate::registry::Registry;
use fc_api::v1::examples::*;
use fc_api::v1::methods::*;
use fc_api::v1::subtitle::SubtitleEncounterRef;
use fc_api::{ApiError, ErrorCode};
use fc_core::encounter_score::{
    rank_scenes, Scene, SceneScore, SceneWord, ScoreOptions, DEFAULT_BASIC_RANK,
};
use fc_core::reading::{headword_key, WordStatus};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::subtitle::utf16_byte_range;
use fc_core::words::index_language;
use fc_core::NoteId;
use fc_storage::Reads;
use serde_json::json;
use std::collections::HashMap;

/// Module id.
pub const ID: &str = "examples";
/// Setting (app): apply the suggested best example when a capture adds one (default off).
pub const AUTO_BEST: &str = "subtitle.auto_best_example";
/// A suggestion needs at least this many points more than the example on the card.
pub const SUGGEST_MARGIN: i32 = 5;
/// Most candidate sentences per `examples.rank_sentences`.
const MAX_SENTENCES: usize = 200;
/// Longest candidate sentence (bytes).
const MAX_SENTENCE_BYTES: usize = 2000;

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.examples.name",
        description_key: "modules.examples.description",
        enabled_by_default: true,
        can_disable: true,
        // `subtitle.auto_best_example` belongs to the subtitle module ([`auto_best_setting`]).
        settings: vec![],
        ui: vec![],
    }
}

/// The [`AUTO_BEST`] setting (declared by the subtitle module: settings carry their module's
/// prefix).
pub(crate) fn auto_best_setting() -> SettingDef {
    SettingDef {
        key: AUTO_BEST.to_owned(),
        kind: SettingKind::Bool,
        default: json!(false),
        label_key: format!("settings.{AUTO_BEST}.label"),
        description_key: Some(format!("settings.{AUTO_BEST}.description")),
        advanced: false,
        scopes: vec![SettingScopeKind::App],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<SubtitleEncountersRank>(rank);
    reg.query::<SubtitleEncountersSuggestPrimary>(suggest);
    reg.query::<ExamplesRankSentences>(rank_sentences);
}

// ------------------------------------------------------------------ scoring input

/// One sentence to rank, before tokenizing.
pub(crate) struct Candidate {
    /// Encounter capture id or candidate index.
    pub id: String,
    /// The sentence.
    pub sentence: String,
    /// The word's UTF-16 range in it.
    pub term_offset: Option<(i64, i64)>,
    /// The word as written.
    pub surface: Option<String>,
    /// Sound stored.
    pub has_audio: bool,
    /// Picture stored.
    pub has_image: bool,
}

/// Ranks `candidates` of `target` in `language` (a BCP-47 tag): `(scores best first,
/// index language, a frequency pack was used)`.
pub(crate) fn rank_candidates(
    reads: &dyn Reads,
    registry: &Registry,
    language: &str,
    target: &str,
    candidates: &[Candidate],
) -> AppResult<(Vec<SceneScore>, String, bool)> {
    let lem = lemmatizer_for(language);
    let index_lang = index_language(language);
    let mut scenes = Vec::with_capacity(candidates.len());
    let mut keys: Vec<String> = Vec::new();
    let mut forms: Vec<String> = Vec::new();
    for c in candidates {
        let toks = tokens(&*lem, &c.sentence, language)?;
        let words: Vec<SceneWord> = toks
            .into_iter()
            .map(|t| {
                let lemma = t.lemma.to_lowercase();
                if let Some(k) = headword_key(std::slice::from_ref(&lemma), language) {
                    keys.push(k);
                }
                forms.push(t.text.to_lowercase());
                forms.push(lemma.clone());
                SceneWord {
                    start: t.start,
                    end: t.end,
                    text: t.text,
                    lemma,
                    status: WordStatus::New,
                    rank: None,
                }
            })
            .collect();
        scenes.push(Scene {
            id: c.id.clone(),
            sentence: c.sentence.clone(),
            words,
            target: c
                .term_offset
                .and_then(|(s, e)| utf16_byte_range(&c.sentence, s, e)),
            surface: c.surface.clone(),
            has_audio: c.has_audio,
            has_image: c.has_image,
        });
    }
    keys.sort_unstable();
    keys.dedup();
    let threshold = words::threshold(reads)?;
    let statuses: HashMap<String, WordStatus> = words::rows_of(reads, &index_lang, &keys)?
        .into_iter()
        .map(|(k, r)| (k, words::row_status(&r, threshold)))
        .collect();
    let pack = word_data::enabled(reads, registry)?
        && reads
            .word_data_packs()?
            .iter()
            .any(|p| p.language == index_lang);
    let data = if pack {
        forms.sort_unstable();
        forms.dedup();
        word_data::rows_for(reads, &index_lang, forms)?
    } else {
        HashMap::new()
    };
    for scene in &mut scenes {
        for w in &mut scene.words {
            if let Some(s) = statuses.get(&w.lemma) {
                w.status = *s;
            }
            w.rank = data
                .get(&w.text.to_lowercase())
                .or_else(|| data.get(&w.lemma))
                .and_then(|r| r.rank);
        }
    }
    let opts = ScoreOptions {
        language: index_lang.clone(),
        target: target.to_owned(),
        frequency_pack: pack,
        basic_rank: DEFAULT_BASIC_RANK,
    };
    Ok((rank_scenes(&scenes, &opts), index_lang, pack))
}

fn dto(
    scores: Vec<SceneScore>,
    sentences: &HashMap<String, String>,
    primary: Option<&str>,
) -> Vec<RankedExample> {
    scores
        .into_iter()
        .enumerate()
        .map(|(i, s)| RankedExample {
            rank: u32::try_from(i + 1).unwrap_or(u32::MAX),
            score: s.score,
            is_primary: primary == Some(s.id.as_str()),
            sentence: sentences.get(&s.id).cloned().unwrap_or_default(),
            target_found: s.target_found,
            unknown_words: s.unknown,
            reasons: s.reasons,
            duplicates: s.duplicates,
            id: s.id,
        })
        .collect()
}

// ------------------------------------------------------------------ encounters

/// A note's encounters ranked (primary first in the input, so ties keep it on top).
pub(crate) fn rank_note(
    reads: &dyn Reads,
    registry: &Registry,
    note_id: NoteId,
) -> AppResult<RankedExamples> {
    crate::support::notes::require_live_note(reads, note_id)?;
    let mut rows = reads.subtitle_encounters_of_note(note_id)?;
    rows.sort_by_key(|r| !r.is_primary); // stable: primary first, then stored order
    let primary = rows.iter().find(|r| r.is_primary).map(|r| r.capture_id.clone());
    let (lang, target) = rows
        .first()
        .map(|r| (r.lang.clone(), r.lemma.clone()))
        .unwrap_or_default();
    let candidates: Vec<Candidate> = rows
        .iter()
        .map(|r| Candidate {
            id: r.capture_id.clone(),
            sentence: r.facts.sentence.clone().unwrap_or_default(),
            term_offset: r.facts.term_start.zip(r.facts.term_end),
            surface: r.facts.surface.clone(),
            has_audio: r.facts.audio_hash.is_some(),
            has_image: r.facts.screenshot_hash.is_some(),
        })
        .collect();
    let sentences: HashMap<String, String> = candidates
        .iter()
        .map(|c| (c.id.clone(), c.sentence.clone()))
        .collect();
    if candidates.is_empty() {
        return Ok(RankedExamples {
            note_id: Some(note_id),
            language: String::new(),
            target: String::new(),
            frequency_pack: false,
            items: vec![],
        });
    }
    let (scores, language, pack) = rank_candidates(reads, registry, &lang, &target, &candidates)?;
    Ok(RankedExamples {
        note_id: Some(note_id),
        language,
        target,
        frequency_pack: pack,
        items: dto(scores, &sentences, primary.as_deref()),
    })
}

/// The suggestion for a note (see [`SUGGEST_MARGIN`]).
pub(crate) fn suggestion(
    reads: &dyn Reads,
    registry: &Registry,
    note_id: NoteId,
) -> AppResult<PrimarySuggestion> {
    let ranking = rank_note(reads, registry, note_id)?;
    let current = ranking.items.iter().find(|i| i.is_primary);
    let best = ranking.items.first();
    let (suggested, gain) = match (current, best) {
        (Some(c), Some(b)) if b.id != c.id && b.score - c.score >= SUGGEST_MARGIN => {
            (Some(b.id.clone()), b.score - c.score)
        }
        (None, Some(b)) => (Some(b.id.clone()), 0),
        _ => (None, 0),
    };
    Ok(PrimarySuggestion {
        note_id,
        current: current.map(|c| c.id.clone()),
        suggested,
        gain,
        ranking,
    })
}

fn rank(req: RankEncounters, ctx: &QueryCtx<'_>) -> AppResult<RankedExamples> {
    rank_note(ctx.tx, ctx.registry, req.note_id)
}

fn suggest(req: RankEncounters, ctx: &QueryCtx<'_>) -> AppResult<PrimarySuggestion> {
    suggestion(ctx.tx, ctx.registry, req.note_id)
}

/// After a capture became another example of `note_id`: with [`AUTO_BEST`] on (and this
/// module on), shows the suggested example on the card (`subtitle.encounters.set_primary`,
/// nested: part of the capture's undo step). Never fails the capture: a ranking error leaves
/// the card as it is.
pub(crate) fn after_attach(ctx: &mut CommandCtx<'_>, note_id: NoteId) -> AppResult<()> {
    if !crate::support::module_enabled(ctx.reads(), ctx.registry, ID)? {
        return Ok(());
    }
    let on = crate::settings::resolve_stored(ctx.reads(), &SettingScope::App, AUTO_BEST)?
        .and_then(|(v, _)| v.as_bool())
        .unwrap_or(false);
    if !on {
        return Ok(());
    }
    let Ok(s) = suggestion(ctx.reads(), ctx.registry, note_id) else {
        return Ok(());
    };
    if let (Some(id), Some(_)) = (s.suggested, s.current) {
        ctx.call(&inverse_call::<SubtitleEncountersSetPrimary>(
            &SubtitleEncounterRef { capture_id: id },
        )?)?;
    }
    Ok(())
}

// ------------------------------------------------------------------ candidate sentences

fn rank_sentences(req: RankSentences, ctx: &QueryCtx<'_>) -> AppResult<RankedExamples> {
    let bad = |m: &str| {
        AppError::Api(
            ApiError::new(ErrorCode::ValidationInvalidParams, m.to_owned())
                .with_detail("reason", "sentences"),
        )
    };
    if req.target.trim().is_empty() {
        return Err(crate::support::invalid("target is empty"));
    }
    if req.sentences.is_empty() || req.sentences.len() > MAX_SENTENCES {
        return Err(bad("1–200 sentences"));
    }
    if req.sentences.iter().any(|s| s.text.len() > MAX_SENTENCE_BYTES) {
        return Err(bad("a sentence is longer than 2000 bytes"));
    }
    let language = language_or_default(ctx.tx, &req.language)?;
    let candidates: Vec<Candidate> = req
        .sentences
        .iter()
        .enumerate()
        .map(|(i, s)| Candidate {
            id: i.to_string(),
            sentence: s.text.clone(),
            term_offset: s.term_offset,
            surface: s.surface.clone(),
            has_audio: s.has_audio.unwrap_or(false),
            has_image: s.has_image.unwrap_or(false),
        })
        .collect();
    let sentences: HashMap<String, String> = candidates
        .iter()
        .map(|c| (c.id.clone(), c.sentence.clone()))
        .collect();
    let target = req.target.trim().to_lowercase();
    let (scores, index_lang, pack) =
        rank_candidates(ctx.tx, ctx.registry, &language, &target, &candidates)?;
    Ok(RankedExamples {
        note_id: None,
        language: index_lang,
        target,
        frequency_pack: pack,
        items: dto(scores, &sentences, None),
    })
}
