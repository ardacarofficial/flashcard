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
use crate::modules::words;
use fc_core::reading::{sentence_spans, summarize, CardEvidence, WordMark, WordStatus};
use fc_core::words::index_language;
use fc_core::{CardId, NoteId, Timestamp};
use fc_providers::language::{Lemmatizer, Token};
use fc_storage::repo::WordCardRow;
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

/// The word index (v1.4, `crate::modules::words`) as reading mode sees it: the status of a
/// lemma in one language, optionally narrowed to the cards of `reading.vocabulary_query`.
pub(crate) struct Vocabulary {
    language: String,
    threshold: u32,
    /// Cards that count (`None` = every card).
    scope: Option<HashSet<CardId>>,
    cal: StudyDay,
}

impl Vocabulary {
    /// The vocabulary of `language` (cards limited by `reading.vocabulary_query`).
    pub(crate) fn load(
        reads: &dyn Reads,
        language: &str,
        tz_offset_mins: Option<i32>,
        now: Timestamp,
    ) -> AppResult<Self> {
        let cal = calendar(reads, tz_offset_mins)?;
        let query = setting_text(reads, VOCABULARY_QUERY, "")?;
        let mut scope = None;
        if !query.trim().is_empty() {
            // A scope that no longer compiles counts every card rather than failing.
            if let Ok(filter) = crate::modules::browse::compile(reads, &query, now) {
                scope = Some(reads.search_card_ids(&filter, now)?.into_iter().collect());
            }
        }
        Ok(Self {
            language: index_language(language),
            threshold: words::threshold(reads)?,
            scope,
            cal,
        })
    }

    /// Statuses of `lemmas` (missing = new): one indexed read per 500 lemmas.
    pub(crate) fn statuses(
        &self,
        reads: &dyn Reads,
        lemmas: &[String],
    ) -> AppResult<HashMap<String, WordStatus>> {
        let rows = words::rows_of(reads, &self.language, lemmas)?;
        let Some(scope) = &self.scope else {
            return Ok(rows
                .into_iter()
                .map(|(k, r)| (k, words::row_status(&r, self.threshold)))
                .collect());
        };
        // Narrowed: the evidence of each word again, from its cards inside the scope.
        let keys: Vec<String> = rows.keys().cloned().collect();
        let mut out = HashMap::with_capacity(keys.len());
        for chunk in keys.chunks(500) {
            let notes = reads.word_index_notes_of(&self.language, chunk)?;
            let ids: Vec<NoteId> = notes.iter().map(|n| n.note_id).collect();
            let mut cards: HashMap<NoteId, Vec<CardEvidence>> = HashMap::new();
            for c in reads.word_card_rows(&ids)? {
                if scope.contains(&c.card_id) {
                    cards.entry(c.note_id).or_default().push(self.evidence(&c));
                }
            }
            let mut by_key: HashMap<&str, Vec<CardEvidence>> = HashMap::new();
            for n in &notes {
                by_key
                    .entry(n.key.as_str())
                    .or_default()
                    .extend(cards.get(&n.note_id).into_iter().flatten().copied());
            }
            for key in chunk {
                let r = &rows[key];
                let ev = by_key.get(key.as_str()).map_or(&[][..], Vec::as_slice);
                let mark = r.mark.as_deref().and_then(WordMark::parse);
                out.insert(key.clone(), scoped_status(ev, mark, self.threshold));
            }
        }
        Ok(out)
    }

    /// FSRS facts of a card (interval in study days, as `stats::card_fact` computes it).
    fn evidence(&self, c: &WordCardRow) -> CardEvidence {
        let interval_days = match (c.state, c.due, c.last_review) {
            (CardState::Review, Some(due), Some(last)) => {
                u32::try_from((self.cal.index(due) - self.cal.index(last)).max(0)).ok()
            }
            _ => None,
        };
        CardEvidence {
            state: c.state,
            interval_days,
        }
    }
}

/// Status from scoped cards and the mark: the mark wins (as in the index).
fn scoped_status(cards: &[CardEvidence], mark: Option<WordMark>, threshold: u32) -> WordStatus {
    match mark {
        Some(WordMark::Known) => WordStatus::Known,
        Some(WordMark::Ignored) => WordStatus::Ignored,
        None if cards.iter().any(|c| {
            c.state == CardState::Review && c.interval_days.unwrap_or(0) >= threshold
        }) =>
        {
            WordStatus::Known
        }
        None if !cards.is_empty() => WordStatus::Learning,
        None => WordStatus::New,
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
    let vocab = Vocabulary::load(ctx.tx, &language, req.tz_offset_mins, ctx.now)?;
    let toks = tokens(&*lem, &text, &language)?;
    let mut lemmas: Vec<String> = toks.iter().map(|t| t.lemma.clone()).collect();
    lemmas.sort_unstable();
    lemmas.dedup();
    let statuses = vocab.statuses(ctx.tx, &lemmas)?;
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
    let vocab = Vocabulary::load(ctx.tx, &language, req.tz_offset_mins, ctx.now)?;
    let lemma = req.lemma.trim().to_owned();
    let keys = [lemma.clone()];
    let status = vocab
        .statuses(ctx.tx, &keys)?
        .remove(&lemma)
        .unwrap_or(WordStatus::New);
    let notes = ctx.tx.word_index_notes_of(&vocab.language, &keys)?;
    let ids: Vec<NoteId> = notes.iter().map(|n| n.note_id).collect();
    let mut cards = Vec::new();
    for row in ctx.tx.word_card_rows(&ids)? {
        if vocab.scope.as_ref().is_some_and(|s| !s.contains(&row.card_id)) {
            continue;
        }
        let ev = vocab.evidence(&row);
        let note = ctx.tx.get_note(row.note_id)?;
        let word = note
            .as_ref()
            .map(|n| {
                plain_field(&n.fields, &["word"]).unwrap_or_else(|| strip_html(&n.sort_field).trim().to_owned())
            })
            .unwrap_or_default();
        let meaning = note.and_then(|n| plain_field(&n.fields, &["meaning", "back", "translation"]));
        cards.push(WordCard {
            card_id: row.card_id,
            note_id: row.note_id,
            word,
            meaning,
            state: row.state,
            interval_days: ev.interval_days,
            mature: ev.state == CardState::Review
                && ev.interval_days.unwrap_or(0) >= vocab.threshold,
        });
    }
    let mark = words::rows_of(ctx.tx, &vocab.language, &keys)?
        .remove(&lemma)
        .and_then(|r| r.mark.as_deref().and_then(WordMark::parse));
    Ok(WordInfo {
        status,
        mark,
        lemma,
        cards,
    })
}

// ------------------------------------------------------------------ marks

pub(crate) fn changed_event(language: &str) -> DomainEvent {
    DomainEvent::WordMarksChanged {
        language: language.to_owned(),
    }
}

/// Applies `marks`; returns the previous marks of the lemmas that changed.
pub(crate) fn apply(
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

/// Validates and applies a `MarkWords` request (`reading.words.mark`, `words.mark`): the
/// mark language and the previous marks of the lemmas that changed (the inverse's input).
pub(crate) fn mark_words(
    req: MarkWords,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<(String, Vec<WordMarkEntry>)> {
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
    Ok((language, previous))
}

fn mark(req: MarkWords, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<WordMarksChanged>> {
    let (language, previous) = mark_words(req, ctx)?;
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
