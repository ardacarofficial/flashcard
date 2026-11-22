//! v1.3: the statistics overview answers three questions — "how am I doing today?"
//! (`stats.today`), "what is ahead?" (the forecast of `stats.overview`) and "what am I
//! forgetting?" (`stats.struggles`, whose list comes with the query to practise exactly those
//! cards).

use super::facts::{calendar, scope_card_rows, scope_cards, scope_days};
use crate::error::AppResult;
use crate::handler::QueryCtx;
use crate::support::decks::DeckIndex;
use fc_api::v1::stats::*;
use fc_core::card::Rating;
use fc_core::notetype::NoteType;
use fc_core::stats::{iso_date, streak, DayAggregate};
use fc_core::{CardId, NoteTypeId, Timestamp};
use fc_storage::repo::{is_practice_mode, SCHEDULED_REVIEW_MODES};
use fc_storage::Reads;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Default window of `stats.struggles` (study days).
const STRUGGLE_DAYS: u32 = 7;
/// Longest window.
const MAX_STRUGGLE_DAYS: u32 = 365;
/// Default / largest list.
const STRUGGLE_LIMIT: u32 = 20;
const MAX_STRUGGLE_LIMIT: u32 = 200;
/// "About the same" band of the pace verdict.
const SAME_BAND: f64 = 0.15;

/// Every `review_log.mode` an answer can have besides imports: the scheduled ones and the
/// practice ones (`trial`, `trial_<mode>`, `test`, `match`).
const ANSWER_MODES: [&str; 10] = [
    SCHEDULED_REVIEW_MODES[0],
    SCHEDULED_REVIEW_MODES[1],
    SCHEDULED_REVIEW_MODES[2],
    SCHEDULED_REVIEW_MODES[3],
    "trial",
    "trial_learn",
    "trial_timed",
    "trial_confidence",
    "test",
    "match",
];

// ------------------------------------------------------------------ today

fn figures(d: Option<&DayAggregate>) -> DayFigures {
    let Some(d) = d else {
        return DayFigures {
            answered: 0,
            correct: 0,
            rate: None,
            time_ms: 0,
            new_cards: 0,
        };
    };
    let correct = d.passed();
    DayFigures {
        answered: d.reviews,
        correct,
        rate: (d.reviews > 0).then(|| f64::from(correct) / f64::from(d.reviews)),
        time_ms: d.time_ms,
        new_cards: d.new_cards,
    }
}

/// The verdict: today's answers against a reference (yesterday, else the 7-day mean).
pub(crate) fn pace(today: u32, reference: f64) -> TodayPace {
    if today == 0 {
        TodayPace::NotStarted
    } else if reference <= 0.0 {
        TodayPace::FirstDay
    } else {
        let ratio = f64::from(today) / reference;
        if ratio > 1.0 + SAME_BAND {
            TodayPace::Ahead
        } else if ratio < 1.0 - SAME_BAND {
            TodayPace::Behind
        } else {
            TodayPace::Same
        }
    }
}

pub(super) fn today(req: TodayRequest, ctx: &QueryCtx<'_>) -> AppResult<TodayStats> {
    let reads = ctx.tx;
    let cal = calendar(reads, req.tz_offset_mins)?;
    let today = cal.index(ctx.now);
    let ids = scope_cards(reads, &req.scope, ctx.now)?;
    let (days, _) = scope_days(reads, cal, ids.as_deref(), false)?;
    let by_day: HashMap<i64, &DayAggregate> = days.iter().map(|d| (d.day, d)).collect();
    let t = figures(by_day.get(&today).copied());
    let y = figures(by_day.get(&(today - 1)).copied());
    let week: u32 = (1..=7)
        .filter_map(|i| by_day.get(&(today - i)))
        .map(|d| d.reviews)
        .sum();
    let week_mean = f64::from(week) / 7.0;
    let (reference, is_yesterday) = if y.answered > 0 {
        (f64::from(y.answered), true)
    } else {
        (week_mean, false)
    };
    let studied: Vec<i64> = days.iter().filter(|d| d.reviews > 0).map(|d| d.day).collect();
    Ok(TodayStats {
        today_date: iso_date(today),
        today: t,
        yesterday: y,
        week_mean_answered: week_mean,
        pace: pace(t.answered, reference),
        reference_answered: reference,
        reference_is_yesterday: is_yesterday,
        streak: streak(&studied, today),
    })
}

// ------------------------------------------------------------------ struggles

#[derive(Debug, Clone, Copy)]
struct Tally {
    misses: u32,
    answers: u32,
    last_miss: Timestamp,
    last_seen: Timestamp,
}

/// Stable field keys of the word and of its meaning in the built-in note types (Classic,
/// Vocabulary, Sentence, Subtitle, …), in order of preference. Keys, never translated labels.
const TERM_KEYS: [&str; 5] = ["word", "front", "text", "question", "sentence"];
const MEANING_KEYS: [&str; 5] = ["meaning", "back", "answer", "translation", "extra"];

/// Plain `(term, meaning)` of a note: the first non-empty field among the preferred keys,
/// else the first / second field.
fn term_and_meaning(nt: &NoteType, fields: &fc_core::note::FieldValues) -> (String, String) {
    let text = |key: &str| -> Option<String> {
        let v = fields.iter().find(|(k, _)| k.as_str() == key).map(|(_, v)| v)?;
        let t = fc_core::note::strip_html(v).trim().to_owned();
        (!t.is_empty()).then_some(t)
    };
    // The first preferred key with text, else the first field (other than `except`).
    let pick = |keys: &[&str], except: Option<&str>| -> (Option<String>, String) {
        let has = |k: &str| nt.fields.iter().any(|f| f.key.as_str() == k);
        for k in keys.iter().filter(|k| Some(**k) != except && has(k)) {
            if let Some(t) = text(k) {
                return (Some((*k).to_owned()), t);
            }
        }
        let key = nt
            .fields
            .iter()
            .map(|f| f.key.as_str())
            .find(|k| Some(*k) != except);
        (key.map(str::to_owned), key.and_then(text).unwrap_or_default())
    };
    let (term_key, term) = pick(&TERM_KEYS, None);
    let (_, meaning) = pick(&MEANING_KEYS, term_key.as_deref());
    (term, meaning)
}

pub(super) fn struggles(req: StrugglesRequest, ctx: &QueryCtx<'_>) -> AppResult<StruggleList> {
    let reads: &dyn Reads = ctx.tx;
    let cal = calendar(reads, req.tz_offset_mins)?;
    let today = cal.index(ctx.now);
    let window = req.days.unwrap_or(STRUGGLE_DAYS).clamp(1, MAX_STRUGGLE_DAYS);
    let limit = req.limit.unwrap_or(STRUGGLE_LIMIT).clamp(1, MAX_STRUGGLE_LIMIT) as usize;
    let since = cal.start(today - i64::from(window) + 1);
    let today_start = cal.start(today);
    let scope: Option<HashSet<CardId>> =
        scope_cards(reads, &req.scope, ctx.now)?.map(|ids| ids.into_iter().collect());

    // Every answer of the window, any study mode (practice included: a word missed in a
    // test is a word being forgotten).
    let mut tally: BTreeMap<CardId, Tally> = BTreeMap::new();
    for mode in ANSWER_MODES {
        debug_assert!(SCHEDULED_REVIEW_MODES.contains(&mode) || is_practice_mode(mode));
        for r in reads.review_logs_since(since, mode)? {
            if scope.as_ref().is_some_and(|s| !s.contains(&r.card_id)) {
                continue;
            }
            let missed = r.rating == Rating::Again;
            let t = tally.entry(r.card_id).or_insert(Tally {
                misses: 0,
                answers: 0,
                last_miss: Timestamp(i64::MIN),
                last_seen: Timestamp(i64::MIN),
            });
            t.answers += 1;
            t.last_seen = t.last_seen.max(r.created_at);
            if missed {
                t.misses += 1;
                t.last_miss = t.last_miss.max(r.created_at);
            }
        }
    }
    let missed: Vec<CardId> = tally
        .iter()
        .filter(|(_, t)| t.misses > 0)
        .map(|(id, _)| *id)
        .collect();
    // Live cards only (history outlives trashed cards).
    let cards = if missed.is_empty() {
        Vec::new()
    } else {
        scope_card_rows(reads, Some(missed.as_slice()))?
    };
    let mut rows: Vec<(&fc_core::card::Card, Tally)> = cards
        .iter()
        .filter_map(|c| tally.get(&c.id).map(|t| (c, *t)))
        .collect();
    rows.sort_by(|(a, x), (b, y)| {
        y.misses
            .cmp(&x.misses)
            .then(y.last_miss.cmp(&x.last_miss))
            .then(a.id.cmp(&b.id))
    });
    let total = u32::try_from(rows.len()).unwrap_or(u32::MAX);
    let today_n = rows.iter().filter(|(_, t)| t.last_miss >= today_start).count();
    let index = DeckIndex::load(reads)?;
    let mut types: HashMap<NoteTypeId, Option<NoteType>> = HashMap::new();
    let mut items = Vec::new();
    for (card, t) in rows.into_iter().take(limit) {
        let Some(note) = reads.get_note(card.note_id)? else {
            continue;
        };
        if let std::collections::hash_map::Entry::Vacant(e) = types.entry(note.notetype_id) {
            e.insert(reads.get_notetype(note.notetype_id)?);
        }
        let (term, meaning) = match types.get(&note.notetype_id).and_then(Option::as_ref) {
            Some(nt) => term_and_meaning(nt, &note.fields),
            None => (note.sort_field.clone(), String::new()),
        };
        items.push(StruggleItem {
            card_id: card.id,
            note_id: card.note_id,
            deck_id: card.deck_id,
            deck_path: index.path(card.deck_id),
            term: if term.is_empty() { note.sort_field.clone() } else { term },
            meaning,
            misses: t.misses,
            answers: t.answers,
            last_miss_at: t.last_miss,
            last_seen_at: t.last_seen,
            lapses: card.scheduling.lapses,
            suspended: card.suspended,
        });
    }
    let studyable: Vec<String> = items
        .iter()
        .filter(|i| !i.suspended)
        .map(|i| i.card_id.to_string())
        .collect();
    Ok(StruggleList {
        days: window,
        total,
        today: u32::try_from(today_n).unwrap_or(u32::MAX),
        items,
        study_query: (!studyable.is_empty()).then(|| format!("cid:{}", studyable.join(","))),
    })
}
