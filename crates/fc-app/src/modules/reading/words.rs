//! `reading.analyze`, word marks and lookups.

use super::items::require_item;
use super::{
    bad_range, language_or_default, lemmatizer_for, mark_language, setting_text, Utf16,
    VOCABULARY_QUERY,
};
use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::modules::stats::facts::calendar;
use crate::registry::Registry;
use crate::support::summary;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::reading::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::card::CardState;
use fc_core::note::strip_html;
use fc_core::reading::{
    clean_headword, headword_key, sentence_spans, summarize, word_status, CardEvidence, WordMark,
    WordStatus,
};
use fc_core::{CardId, Timestamp};
use fc_providers::language::{Lemmatizer, Token};
use fc_storage::repo::VocabularyRow;
use fc_storage::Reads;
use fc_study::day::StudyDay;
use std::collections::{HashMap, HashSet};

pub(super) fn register(reg: &mut Registry) {
    reg.query::<ReadingAnalyze>(analyze);
    reg.query::<ReadingWordsLookup>(lookup);
    reg.command::<ReadingWordsMark>(mark);
    reg.command::<ReadingWordsRestore>(restore);
}

/// Most lemmas per mark call.
const MAX_MARK_LEMMAS: usize = 5000;
/// Longest pasted text to analyse (bytes).
const MAX_TEXT_BYTES: usize = super::MAX_BODY_BYTES;

fn provider_failed(e: fc_providers::ProviderError) -> AppError {
    AppError::api(ErrorCode::Internal, format!("lemmatizer: {e}"))
}

/// Tokens of `text` with lemmas.
pub(crate) fn tokens(lem: &dyn Lemmatizer, text: &str, language: &str) -> AppResult<Vec<Token>> {
    lem.analyze(text, language).map_err(provider_failed)
}

/// FSRS facts of a card row (interval in study days, as `stats::card_fact` computes it).
fn evidence(row: &VocabularyRow, cal: StudyDay) -> CardEvidence {
    let interval_days = match (row.state, row.due, row.last_review) {
        (CardState::Review, Some(due), Some(last)) => {
            u32::try_from((cal.index(due) - cal.index(last)).max(0)).ok()
        }
        _ => None,
    };
    CardEvidence {
        state: row.state,
        interval_days,
    }
}

/// The collection's vocabulary in one language: lemma key → its card rows.
pub(crate) struct Vocabulary {
    rows: Vec<VocabularyRow>,
    by_key: HashMap<String, Vec<usize>>,
    marks: HashMap<String, WordMark>,
    cal: StudyDay,
}

impl Vocabulary {
    /// Builds the vocabulary of `language` (cards limited by `reading.vocabulary_query`).
    pub(crate) fn load(
        reads: &dyn Reads,
        lem: &dyn Lemmatizer,
        language: &str,
        tz_offset_mins: Option<i32>,
        now: Timestamp,
    ) -> AppResult<Self> {
        let cal = calendar(reads, tz_offset_mins)?;
        let mut rows = reads.vocabulary_rows()?;
        let query = setting_text(reads, VOCABULARY_QUERY, "")?;
        if !query.trim().is_empty() {
            // A scope that no longer compiles counts every card rather than failing.
            if let Ok(filter) = crate::modules::browse::compile(reads, &query, now) {
                let ids: HashSet<CardId> = reads.search_card_ids(&filter, now)?.into_iter().collect();
                rows.retain(|r| ids.contains(&r.card_id));
            }
        }
        let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
        let mut cache: HashMap<String, Option<String>> = HashMap::new();
        for (i, row) in rows.iter().enumerate() {
            let plain = strip_html(&row.word);
            let head = clean_headword(&plain).to_owned();
            let key = match cache.get(&head) {
                Some(k) => k.clone(),
                None => {
                    let lemmas: Vec<String> = tokens(lem, &head, language)?
                        .into_iter()
                        .map(|t| t.lemma)
                        .collect();
                    let k = headword_key(&lemmas, language);
                    cache.insert(head, k.clone());
                    k
                }
            };
            if let Some(k) = key {
                by_key.entry(k).or_default().push(i);
            }
        }
        let marks = reads
            .list_word_marks(&mark_language(language))?
            .into_iter()
            .filter_map(|m| WordMark::parse(&m.status).map(|s| (m.lemma, s)))
            .collect();
        Ok(Self {
            rows,
            by_key,
            marks,
            cal,
        })
    }

    /// Card rows of a lemma.
    pub(crate) fn cards(&self, lemma: &str) -> impl Iterator<Item = &VocabularyRow> {
        self.by_key
            .get(lemma)
            .into_iter()
            .flatten()
            .map(|&i| &self.rows[i])
    }

    /// The user's mark of a lemma.
    pub(crate) fn mark(&self, lemma: &str) -> Option<WordMark> {
        self.marks.get(lemma).copied()
    }

    /// Status of a lemma.
    pub(crate) fn status(&self, lemma: &str) -> WordStatus {
        let ev: Vec<CardEvidence> = self.cards(lemma).map(|r| evidence(r, self.cal)).collect();
        word_status(&ev, self.mark(lemma))
    }
}

// ------------------------------------------------------------------ analyze

fn analyze(req: AnalyzeText, ctx: &QueryCtx<'_>) -> AppResult<TextAnalysis> {
    let (text, language, cues) = match (req.item_id, req.text) {
        (Some(id), None) => {
            let item = require_item(ctx.tx, id)?;
            let cues: Vec<CueDto> = match item.cues.as_deref() {
                Some(c) => serde_json::from_str(c)
                    .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?,
                None => Vec::new(),
            };
            (item.body, item.language, cues)
        }
        (None, Some(text)) => {
            if text.len() > MAX_TEXT_BYTES {
                return Err(AppError::Api(
                    ApiError::new(ErrorCode::ValidationReadingText, "the text is too long")
                        .with_detail("field", "text"),
                ));
            }
            let lang = language_or_default(ctx.tx, req.language.as_deref().unwrap_or(""))?;
            (text, lang, Vec::new())
        }
        _ => {
            return Err(crate::support::invalid(
                "give either item_id or text",
            ))
        }
    };
    let lem = lemmatizer_for(&language);
    let vocab = Vocabulary::load(ctx.tx, &*lem, &language, req.tz_offset_mins, ctx.now)?;
    let toks = tokens(&*lem, &text, &language)?;
    let mut statuses: HashMap<&str, WordStatus> = HashMap::new();
    for t in &toks {
        if !statuses.contains_key(t.lemma.as_str()) {
            statuses.insert(&t.lemma, vocab.status(&t.lemma));
        }
    }
    let status_of = |lemma: &str| statuses.get(lemma).copied().unwrap_or(WordStatus::New);
    let summary = summarize(toks.iter().map(|t| (t.lemma.as_str(), status_of(&t.lemma))));
    let (from, to) = match req.range {
        None => (0, text.len()),
        Some([s, e]) => {
            if s > e || e > Utf16::len(&text) {
                return Err(bad_range("range"));
            }
            (Utf16::byte_of(&text, s), Utf16::byte_of(&text, e))
        }
    };
    // Tokens and sentences are ascending: one forward cursor each.
    let mut cursor = Utf16::new(&text);
    let out_tokens: Vec<WordToken> = toks
        .iter()
        .filter(|t| t.end > from && t.start < to)
        .map(|t| WordToken {
            start: cursor.unit_of(t.start),
            end: cursor.unit_of(t.end),
            lemma: t.lemma.clone(),
            status: status_of(&t.lemma),
        })
        .collect();
    let mut sc = Utf16::new(&text);
    let sentences = sentence_spans(&text)
        .into_iter()
        .filter(|&(s, e)| e > from && s < to)
        .map(|(s, e)| [sc.unit_of(s), sc.unit_of(e)])
        .collect();
    Ok(TextAnalysis {
        text,
        language,
        lemmatizer: lem.info().id,
        tokens: out_tokens,
        sentences,
        cues,
        summary,
    })
}

// ------------------------------------------------------------------ lookup

fn plain_field(fields: &fc_core::note::FieldValues, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| {
        fields
            .get(&fc_core::notetype::FieldKey::new(*k))
            .map(|v| strip_html(v).trim().to_owned())
            .filter(|v| !v.is_empty())
    })
}

fn lookup(req: LookupWord, ctx: &QueryCtx<'_>) -> AppResult<WordInfo> {
    let language = language_or_default(ctx.tx, &req.language)?;
    let lem = lemmatizer_for(&language);
    let vocab = Vocabulary::load(ctx.tx, &*lem, &language, req.tz_offset_mins, ctx.now)?;
    let lemma = req.lemma.trim().to_owned();
    let mut cards = Vec::new();
    for row in vocab.cards(&lemma) {
        let ev = evidence(row, vocab.cal);
        let meaning = ctx
            .tx
            .get_note(row.note_id)?
            .and_then(|n| plain_field(&n.fields, &["meaning", "back", "translation"]));
        cards.push(WordCard {
            card_id: row.card_id,
            note_id: row.note_id,
            word: strip_html(&row.word).trim().to_owned(),
            meaning,
            state: row.state,
            interval_days: ev.interval_days,
            mature: ev.is_mature(),
        });
    }
    Ok(WordInfo {
        status: vocab.status(&lemma),
        mark: vocab.mark(&lemma),
        lemma,
        cards,
    })
}

// ------------------------------------------------------------------ marks

fn changed_event(language: &str) -> DomainEvent {
    DomainEvent::WordMarksChanged {
        language: language.to_owned(),
    }
}

/// Applies `marks`; returns the previous marks of the lemmas that changed.
fn apply(
    ctx: &mut CommandCtx<'_>,
    language: &str,
    marks: &[(String, Option<WordMark>)],
) -> AppResult<Vec<WordMarkEntry>> {
    let current: HashMap<String, WordMark> = ctx
        .reads()
        .list_word_marks(language)?
        .into_iter()
        .filter_map(|m| WordMark::parse(&m.status).map(|s| (m.lemma, s)))
        .collect();
    let mut previous = Vec::new();
    let mut seen = HashSet::new();
    for (lemma, mark) in marks {
        if !seen.insert(lemma.clone()) {
            continue;
        }
        let before = current.get(lemma).copied();
        if before == *mark {
            continue;
        }
        ctx.tx
            .set_word_mark(language, lemma, mark.map(WordMark::as_str), ctx.now)?;
        previous.push(WordMarkEntry {
            lemma: lemma.clone(),
            mark: before,
        });
    }
    Ok(previous)
}

fn mark(req: MarkWords, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<WordMarksChanged>> {
    let language = mark_language(&language_or_default(ctx.reads(), &req.language)?);
    let lemmas: Vec<String> = req
        .lemmas
        .iter()
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect();
    if lemmas.is_empty() || lemmas.len() > MAX_MARK_LEMMAS || lemmas.iter().any(|l| l.len() > 200)
    {
        return Err(crate::support::invalid("1–5000 lemmas of at most 200 bytes"));
    }
    let marks: Vec<(String, Option<WordMark>)> =
        lemmas.into_iter().map(|l| (l, req.mark)).collect();
    let previous = apply(ctx, &language, &marks)?;
    let changed = u32::try_from(previous.len()).unwrap_or(u32::MAX);
    Ok(CommandOutcome {
        response: WordMarksChanged { changed },
        inverse: vec![inverse_call::<ReadingWordsRestore>(&RestoreWordMarks {
            language: language.clone(),
            marks: previous,
        })?],
        summary: summary::<ReadingWordsMark>().param("count", changed),
        events: vec![changed_event(&language)],
    })
}

fn restore(
    req: RestoreWordMarks,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<WordMarksChanged>> {
    let marks: Vec<(String, Option<WordMark>)> =
        req.marks.into_iter().map(|m| (m.lemma, m.mark)).collect();
    let previous = apply(ctx, &req.language, &marks)?;
    let changed = u32::try_from(previous.len()).unwrap_or(u32::MAX);
    Ok(CommandOutcome {
        response: WordMarksChanged { changed },
        inverse: vec![inverse_call::<ReadingWordsRestore>(&RestoreWordMarks {
            language: req.language.clone(),
            marks: previous,
        })?],
        summary: summary::<ReadingWordsRestore>().param("count", changed),
        events: vec![changed_event(&req.language)],
    })
}
