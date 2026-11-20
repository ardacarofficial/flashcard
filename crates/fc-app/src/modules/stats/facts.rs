//! Shared machinery of the v0.6 modules (`stats`, `goals`, `planning`, `optimizer`): the study
//! calendar, statistics scopes (deck / tag / query → card ids), review and card facts for
//! `fc_core::stats`, and the per-day statistics cache.

use crate::error::{AppError, AppResult};
use crate::settings::resolve_stored;
use crate::support::decks::DeckIndex;
use crate::support::invalid;
use fc_api::v1::stats::StatsScope;
use fc_api::{ApiError, ErrorCode};
use fc_core::card::{Card, CardState};
use fc_core::preset::PresetConfig;
use fc_core::query::filter::{Filter, Pred};
use fc_core::settings::SettingScope;
use fc_core::stats::{aggregate_days, merge_days, CardFact, DayAggregate, ReviewFact};
use fc_core::{CardId, DeckId, Timestamp};
use fc_providers::strategy::SchedulerParams;
use fc_storage::repo::{ReviewLogRecord, StatsCacheRecord, StudyCardFilter, REVIEW_MODE_IMPORTED};
use fc_storage::{Reads, Writes};
use fc_study::day::{StudyDay, DEFAULT_ROLLOVER_HOUR, MAX_TZ_OFFSET_MINS};
use std::collections::HashMap;

/// The study module's rollover-hour setting (read, not owned).
const DAY_START_HOUR: &str = "study.day_start_hour";

// ------------------------------------------------------------------ calendar

/// The user's study calendar: client UTC offset + the `study.day_start_hour` setting.
pub(crate) fn calendar(reads: &dyn Reads, tz_offset_mins: Option<i32>) -> AppResult<StudyDay> {
    let tz = tz_offset_mins.unwrap_or(0);
    if tz.abs() > MAX_TZ_OFFSET_MINS {
        return Err(invalid("tz_offset_mins out of range"));
    }
    let hour = resolve_stored(reads, &SettingScope::App, DAY_START_HOUR)?
        .and_then(|(v, _)| v.as_u64())
        .and_then(|h| u8::try_from(h).ok())
        .filter(|h| *h < 24)
        .unwrap_or(DEFAULT_ROLLOVER_HOUR);
    Ok(StudyDay::new(tz, hour))
}

/// Cache key of a calendar.
pub(crate) fn calendar_key(cal: StudyDay) -> String {
    format!("{}:{}", cal.tz_offset_mins, cal.rollover_hour)
}

/// Local hour of `t` (0–23).
pub(crate) fn local_hour(cal: StudyDay, t: Timestamp) -> u8 {
    let ms = t
        .as_millis()
        .saturating_add(i64::from(cal.tz_offset_mins) * 60_000);
    u8::try_from(ms.rem_euclid(fc_core::time::MILLIS_PER_DAY) / 3_600_000).unwrap_or(0)
}

/// `VALIDATION_DATE` for `field`.
pub(crate) fn bad_date(field: &str, value: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationDate, format!("invalid date {value:?}"))
            .with_detail("field", field),
    )
}

/// Day index of a `YYYY-MM-DD` request field.
pub(crate) fn parse_date(field: &str, value: &str) -> AppResult<i64> {
    fc_core::stats::parse_iso_date(value).ok_or_else(|| bad_date(field, value))
}

// ------------------------------------------------------------------ scopes

/// The filter of a scope (`None` = the whole collection).
pub(crate) fn scope_filter(
    reads: &dyn Reads,
    scope: &StatsScope,
    now: Timestamp,
) -> AppResult<Option<Filter>> {
    if scope.is_global() {
        return Ok(None);
    }
    let mut parts = Vec::new();
    if let Some(deck) = scope.deck_id {
        let index = DeckIndex::load(reads)?;
        index.require(deck)?;
        parts.push(Filter::Pred(Pred::Decks(index.live_subtree(deck))));
    }
    if let Some(tag) = scope.tag.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        let text = format!("tag:\"{}\"", tag.replace('"', ""));
        parts.push(crate::modules::browse::compile(reads, &text, now)?);
    }
    if let Some(q) = scope.query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        parts.push(crate::modules::browse::compile(reads, q, now)?);
    }
    Ok(Some(Filter::and(parts)))
}

/// Card ids of a scope (`None` = the whole collection).
pub(crate) fn scope_cards(
    reads: &dyn Reads,
    scope: &StatsScope,
    now: Timestamp,
) -> AppResult<Option<Vec<CardId>>> {
    match scope_filter(reads, scope, now)? {
        None => Ok(None),
        Some(f) => Ok(Some(reads.search_card_ids(&f, now)?)),
    }
}

/// Ids matching `filter` within `within` (`None` = anywhere).
pub(crate) fn matching(
    reads: &dyn Reads,
    filter: Filter,
    within: Option<&Filter>,
    now: Timestamp,
) -> AppResult<Vec<CardId>> {
    let f = match within {
        Some(w) => Filter::and(vec![w.clone(), filter]),
        None => filter,
    };
    Ok(reads.search_card_ids(&f, now)?)
}

// ------------------------------------------------------------------ facts

/// A review row as a statistics fact on `cal`.
pub(crate) fn review_fact(r: &ReviewLogRecord, cal: StudyDay) -> ReviewFact {
    ReviewFact {
        card_id: r.card_id,
        rating: r.rating,
        state_before: r.state_before,
        elapsed_days: r.elapsed_days,
        duration_ms: r.duration_ms,
        day: cal.index(r.created_at),
        hour: local_hour(cal, r.created_at),
        imported: r.mode == REVIEW_MODE_IMPORTED,
    }
}

/// Facts of review rows.
pub(crate) fn review_facts(rows: &[(i64, ReviewLogRecord)], cal: StudyDay) -> Vec<ReviewFact> {
    rows.iter().map(|(_, r)| review_fact(r, cal)).collect()
}

/// Scheduler parameters of a preset config (FSRS weights in `extra.weights`).
pub(crate) fn params_of(p: &PresetConfig) -> SchedulerParams {
    SchedulerParams {
        desired_retention: p.desired_retention,
        maximum_interval_days: p.maximum_interval_days,
        learning_steps_mins: p.learning_steps_mins.clone(),
        relearning_steps_mins: p.relearning_steps_mins.clone(),
        extra: if p.fsrs_weights.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::json!({ "weights": p.fsrs_weights })
        },
    }
}

/// Effective preset of decks, cached.
pub(crate) struct Presets<'a> {
    reads: &'a dyn Reads,
    cache: HashMap<DeckId, PresetConfig>,
}

impl<'a> Presets<'a> {
    pub fn new(reads: &'a dyn Reads) -> Self {
        Self {
            reads,
            cache: HashMap::new(),
        }
    }

    /// Effective preset config of `deck` (defaults for a missing deck).
    pub fn of(&mut self, deck: DeckId) -> AppResult<&PresetConfig> {
        if !self.cache.contains_key(&deck) {
            let cfg = if self.reads.get_deck(deck)?.is_some() {
                crate::modules::presets::effective(self.reads, deck)?.config
            } else {
                PresetConfig::default()
            };
            self.cache.insert(deck, cfg);
        }
        self.cache
            .get(&deck)
            .ok_or_else(|| AppError::api(ErrorCode::Internal, "preset cache"))
    }
}

/// Live cards of a scope (suspended included).
pub(crate) fn scope_card_rows(
    reads: &dyn Reads,
    ids: Option<&[CardId]>,
) -> AppResult<Vec<Card>> {
    Ok(reads.study_cards(&StudyCardFilter {
        card_ids: ids.map(<[CardId]>::to_vec),
        include_suspended: true,
        ..StudyCardFilter::default()
    })?)
}

/// A card as a statistics fact on `cal` at `now`.
pub(crate) fn card_fact(
    card: &Card,
    cal: StudyDay,
    now: Timestamp,
    presets: &mut Presets<'_>,
) -> AppResult<CardFact> {
    let s = &card.scheduling;
    let due_day = s.due.filter(|_| s.state != CardState::New).map(|d| cal.index(d));
    let last_day = s.last_review.map(|l| cal.index(l));
    let interval_days = match (s.state, due_day, last_day) {
        (CardState::Review, Some(d), Some(l)) => u32::try_from((d - l).max(0)).ok(),
        _ => None,
    };
    let retrievability = match (s.state, s.stability, s.last_review) {
        (CardState::New, _, _) => None,
        (_, Some(st), Some(last)) => {
            let params = params_of(presets.of(card.deck_id)?);
            fc_study::scheduler::retrievability(st, f64::from(cal.days_between(last, now)), &params)
        }
        _ => None,
    };
    Ok(CardFact {
        card_id: card.id,
        state: s.state,
        suspended: card.suspended,
        buried: card.buried_until.is_some_and(|b| b > now),
        interval_days,
        due_day,
        stability: s.stability,
        difficulty: s.difficulty,
        lapses: s.lapses,
        retrievability,
    })
}

/// Facts of cards.
pub(crate) fn card_facts(
    reads: &dyn Reads,
    cards: &[Card],
    cal: StudyDay,
    now: Timestamp,
) -> AppResult<Vec<CardFact>> {
    let mut presets = Presets::new(reads);
    cards
        .iter()
        .map(|c| card_fact(c, cal, now, &mut presets))
        .collect()
}

// ------------------------------------------------------------------ day aggregates + cache

/// Where day aggregates came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Origin {
    /// From a valid cache plus the rows appended since.
    pub from_cache: bool,
    /// Rows folded on top of the cache (or all rows when recomputed).
    pub folded: u32,
}

/// Day aggregates recomputed from `review_log` (of `cards` only, when given).
pub(crate) fn fresh_days(
    reads: &dyn Reads,
    cal: StudyDay,
    cards: Option<&[CardId]>,
) -> AppResult<(Vec<DayAggregate>, u32)> {
    let rows = reads.stats_reviews(0, cards)?;
    let n = u32::try_from(rows.len()).unwrap_or(u32::MAX);
    Ok((aggregate_days(&review_facts(&rows, cal)), n))
}

fn decode_days(rec: &StatsCacheRecord) -> AppResult<Vec<DayAggregate>> {
    rec.days
        .iter()
        .map(|(_, v)| {
            serde_json::from_value(v.clone())
                .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
        })
        .collect()
}

/// Whole-collection day aggregates: the cache of `cal` plus the rows appended since when it
/// is still valid (no row changed or disappeared since it was built), else a recomputation.
/// Equal to [`fresh_days`] either way.
pub(crate) fn collection_days(
    reads: &dyn Reads,
    cal: StudyDay,
) -> AppResult<(Vec<DayAggregate>, Origin)> {
    let mark = reads.review_log_mark()?;
    if let Some(cache) = reads.stats_cache(&calendar_key(cal))? {
        let valid = cache.generation == mark.generation
            && cache.upto_seq <= mark.max_seq
            && reads.review_count_upto(cache.upto_seq)? == cache.row_count;
        if valid {
            let base = decode_days(&cache)?;
            let tail = reads.stats_reviews(cache.upto_seq, None)?;
            let folded = u32::try_from(tail.len()).unwrap_or(u32::MAX);
            let days = merge_days(&base, &aggregate_days(&review_facts(&tail, cal)));
            return Ok((
                days,
                Origin {
                    from_cache: true,
                    folded,
                },
            ));
        }
    }
    let (days, folded) = fresh_days(reads, cal, None)?;
    Ok((
        days,
        Origin {
            from_cache: false,
            folded,
        },
    ))
}

/// Day aggregates of a scope (`None` = whole collection, through the cache unless `fresh`).
pub(crate) fn scope_days(
    reads: &dyn Reads,
    cal: StudyDay,
    cards: Option<&[CardId]>,
    fresh: bool,
) -> AppResult<(Vec<DayAggregate>, Origin)> {
    match cards {
        None if !fresh => collection_days(reads, cal),
        _ => {
            let (days, folded) = fresh_days(reads, cal, cards)?;
            Ok((
                days,
                Origin {
                    from_cache: false,
                    folded,
                },
            ))
        }
    }
}

/// Brings the cache of `cal` up to date; returns `(rebuilt, days, folded)`.
pub(crate) fn refresh_cache(
    tx: &mut dyn Writes,
    cal: StudyDay,
    now: Timestamp,
) -> AppResult<(bool, u32, u32)> {
    let mark = tx.review_log_mark()?;
    let (days, origin) = collection_days(&*tx, cal)?;
    if origin.from_cache && origin.folded == 0 {
        let n = u32::try_from(days.len()).unwrap_or(u32::MAX);
        return Ok((false, n, 0));
    }
    let encoded = days
        .iter()
        .map(|d| {
            serde_json::to_value(d)
                .map(|v| (d.day, v))
                .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
        })
        .collect::<AppResult<Vec<_>>>()?;
    let row_count = tx.review_count_upto(mark.max_seq)?;
    tx.put_stats_cache(&StatsCacheRecord {
        calendar: calendar_key(cal),
        upto_seq: mark.max_seq,
        row_count,
        generation: mark.generation,
        days: encoded,
        updated_at: now,
    })?;
    Ok((
        !origin.from_cache,
        u32::try_from(days.len()).unwrap_or(u32::MAX),
        origin.folded,
    ))
}

// ------------------------------------------------------------------ stored queries

/// Canonical form of a scope query for storage (`None` for empty), checked to compile unless
/// `validate` is off (undo/redo replay stored values).
pub(crate) fn canonical_query(
    reads: &dyn Reads,
    text: Option<&str>,
    now: Timestamp,
    validate: bool,
) -> AppResult<Option<String>> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    let cat = crate::modules::browse::CatalogData::load(reads, now)?;
    let query = fc_core::query::canonical::canonicalize(text, &cat.catalog());
    if validate {
        crate::modules::browse::compile(reads, &query, now)?;
    }
    Ok(Some(query))
}

/// Display form of a stored (canonical) query.
pub(crate) fn rendered_query(
    reads: &dyn Reads,
    stored: Option<&str>,
    now: Timestamp,
) -> AppResult<Option<String>> {
    let Some(q) = stored else { return Ok(None) };
    let cat = crate::modules::browse::CatalogData::load(reads, now)?;
    Ok(Some(fc_core::query::canonical::render(q, &cat.catalog())))
}
