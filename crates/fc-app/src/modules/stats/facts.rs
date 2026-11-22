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
use fc_storage::repo::{ReviewLogRecord, StatsCacheRecord, StudyCardFilter, REVIEW_MODE_IMPORTED};
use fc_storage::{Reads, Writes};
use fc_study::day::{StudyDay, DEFAULT_ROLLOVER_HOUR, MAX_TZ_OFFSET_MINS};
use std::collections::HashMap;

/// The study module's rollover-hour setting (read, not owned).
const DAY_START_HOUR: &str = "study.day_start_hour";

// ------------------------------------------------------------------ calendar

/// The user's study calendar: client UTC offset (given, else the one the last `study.start`
/// recorded, else UTC — v1.4: the same fallback as the query language's `today_start`) + the
/// `study.day_start_hour` setting.
pub(crate) fn calendar(reads: &dyn Reads, tz_offset_mins: Option<i32>) -> AppResult<StudyDay> {
    if tz_offset_mins.is_some_and(|tz| tz.abs() > MAX_TZ_OFFSET_MINS) {
        return Err(invalid("tz_offset_mins out of range"));
    }
    let tz = crate::modules::study::today::client_tz(reads, tz_offset_mins)?;
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

/// [`review_fact`] of a lean statistics row (v1.0 streaming reads).
pub(crate) fn review_fact_of_row(r: &fc_storage::repo::StatsReviewRow, cal: StudyDay) -> ReviewFact {
    ReviewFact {
        card_id: r.card_id,
        rating: r.rating,
        state_before: r.state_before,
        elapsed_days: r.elapsed_days,
        duration_ms: r.duration_ms,
        day: cal.index(r.created_at),
        hour: local_hour(cal, r.created_at),
        imported: r.imported,
    }
}

/// Facts of review rows.
pub(crate) fn review_facts(rows: &[(i64, ReviewLogRecord)], cal: StudyDay) -> Vec<ReviewFact> {
    rows.iter().map(|(_, r)| review_fact(r, cal)).collect()
}

/// Forgetting-curve decay of a preset: [`fc_core::recall::decay_of_weights`] (FSRS-6 `w20`,
/// FSRS-4.5/5 their fixed 0.5, the default decay for default or unusable weights).
pub(crate) fn decay_of(p: &PresetConfig) -> f64 {
    fc_core::recall::decay_of_weights(&p.fsrs_weights)
}

/// **The** recall probability of `card` at `now` (v1.4, one truth): the card's preset decay,
/// whole study days on `cal` ([`fc_core::recall::card_retrievability`]). Card info, statistics,
/// the study history panel and the query language (`retrievability<0.9`, through
/// [`deck_decays`]) all give the same number.
pub(crate) fn card_retrievability(
    card: &Card,
    cal: StudyDay,
    now: Timestamp,
    presets: &mut Presets<'_>,
) -> AppResult<Option<f64>> {
    if card.scheduling.state == CardState::New {
        return Ok(None);
    }
    let decay = decay_of(presets.of(card.deck_id)?);
    Ok(fc_core::recall::card_retrievability(
        &card.scheduling,
        cal.start(cal.index(now)),
        decay,
    ))
}

/// Decay of every deck whose preset's differs from the default (the query catalog's
/// `deck_decay`), from one deck and preset listing.
pub(crate) fn deck_decays(reads: &dyn Reads) -> AppResult<Vec<(DeckId, f64)>> {
    let presets: HashMap<_, f64> = reads
        .list_presets()?
        .into_iter()
        .map(|p| (p.id, decay_of(&p.config)))
        .collect();
    if presets
        .values()
        .all(|d| d.to_bits() == fc_core::recall::DEFAULT_DECAY.to_bits())
    {
        return Ok(Vec::new());
    }
    let index = DeckIndex::load(reads)?;
    let mut out = Vec::new();
    // Trashed decks too: their cards still show a probability in card info.
    for deck in reads.list_decks(true)? {
        // The nearest assigned preset up the ancestry (`presets::effective`).
        let decay = index
            .ancestry(deck.id)?
            .into_iter()
            .rev()
            .find_map(|id| index.get(id)?.preset_id.and_then(|p| presets.get(&p)))
            .copied()
            .unwrap_or(fc_core::recall::DEFAULT_DECAY);
        if decay.to_bits() != fc_core::recall::DEFAULT_DECAY.to_bits() {
            out.push((deck.id, decay));
        }
    }
    Ok(out)
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
    let retrievability = card_retrievability(card, cal, now, presets)?;
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
    // Folded while streaming (v1.0: the history is never collected in memory).
    let mut days: std::collections::BTreeMap<i64, DayAggregate> = std::collections::BTreeMap::new();
    let mut n = 0u32;
    reads.for_each_stats_review_row(0, cards, &mut |_, r| {
        let f = review_fact_of_row(r, cal);
        days.entry(f.day)
            .or_insert_with(|| DayAggregate::empty(f.day))
            .add(&f);
        n = n.saturating_add(1);
    })?;
    Ok((days.into_values().collect(), n))
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
        fingerprint: None,
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

/// v1.4 A2: card info, statistics, the study history panel and the query language give one
/// recall probability — the preset's personalised decay, whole study days.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod recall_tests {
    use super::{calendar, card_facts};
    use crate::testing::{MemStore, TempSqlite, TestApp};
    use fc_api::v1::browse::CardInfoRef;
    use fc_api::v1::decks::CreateDeck;
    use fc_api::v1::methods::*;
    use fc_api::v1::notes::CreateNote;
    use fc_api::v1::presets::{AssignPreset, CreatePreset};
    use fc_api::v1::search::SearchCards;
    use fc_api::v1::study::{CardHistoryRequest, SessionRef, StartSession, StudyKind};
    use fc_core::card::CardState;
    use fc_core::notetype::builtin;
    use fc_core::preset::PresetConfig;
    use fc_core::time::{FixedClock, MILLIS_PER_DAY};
    use fc_core::{CardId, DeckId, Timestamp};
    use fc_storage::Store;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    /// 2026-01-01T00:00Z.
    const T0: i64 = 1_767_225_600_000;
    const HOUR: i64 = 3_600_000;
    /// Personalised decay (FSRS-6 `w20`).
    const DECAY: f64 = 0.5;

    fn deck(t: &TestApp, path: &str) -> DeckId {
        t.ok::<DecksCreate>(CreateDeck {
            path: path.into(),
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        })
        .id
    }

    fn card(t: &TestApp, deck: DeckId, front: &str) -> CardId {
        t.ok::<NotesCreate>(CreateNote {
            notetype_id: builtin::CLASSIC_ID,
            deck_id: deck,
            fields: BTreeMap::from([
                ("front".to_owned(), front.to_owned()),
                ("back".to_owned(), "x".to_owned()),
            ]),
            tags: vec![],
            attributes: vec![],
            allow_duplicate: None,
        })
        .cards[0]
            .id
    }

    fn reviewed(t: &TestApp, id: CardId, stability: f64, last: Timestamp) {
        let mut tx = t.store.write().unwrap();
        let mut c = tx.get_card(id).unwrap().unwrap();
        c.scheduling.state = CardState::Review;
        c.scheduling.stability = Some(stability);
        c.scheduling.difficulty = Some(5.0);
        c.scheduling.reps = 3;
        c.scheduling.last_review = Some(last);
        c.scheduling.due = Some(last.plus_days(3));
        tx.update_card(&c).unwrap();
        tx.commit().unwrap();
    }

    fn hits(t: &TestApp, query: &str) -> Vec<CardId> {
        let mut ids: Vec<CardId> = t
            .ok::<SearchCardsMethod>(SearchCards {
                query: query.into(),
                sort: None,
                descending: None,
                notes_only: None,
                offset: None,
                limit: None,
            })
            .items
            .into_iter()
            .map(|r| r.card_id)
            .collect();
        ids.sort();
        ids
    }

    fn agree(store: Arc<dyn Store>) {
        use fc_core::preset::ReviewOrder;
        use fc_core::recall::{retrievability, DEFAULT_DECAY};
        // 15:00 UTC on Jan 11; the study day (UTC, rollover 04:00) began at 04:00.
        let now = Timestamp(T0 + 10 * MILLIS_PER_DAY + 15 * HOUR);
        let t = TestApp::with_clock(store, Arc::new(FixedClock::new(now)));
        let mine = deck(&t, "Mine");
        let plain = deck(&t, "Mine::Plain");
        let mut weights: Vec<f64> = fc_study::scheduler::FSRS6_DEFAULT_WEIGHTS
            .iter()
            .map(|w| f64::from(*w))
            .collect();
        weights[20] = DECAY;
        // Both presets study "lowest recall first"; only Mine has personalised weights.
        let preset = |name: &str, w: Vec<f64>| {
            t.ok::<PresetsCreate>(CreatePreset {
                name: name.into(),
                config: Some(PresetConfig {
                    fsrs_weights: w,
                    review_order: ReviewOrder::Retrievability,
                    ..PresetConfig::default()
                }),
            })
            .id
        };
        let (p, std) = (preset("Personal", weights.clone()), preset("Std", vec![]));
        for (d, id) in [(mine, p), (plain, std)] {
            t.ok::<PresetsAssign>(AssignPreset {
                deck_ids: vec![d],
                preset_id: Some(id),
            });
        }
        // Reviewed at 03:00 on Jan 3 = the study day of Jan 2: 9 study days ago (8.5 × 24 h).
        let last = Timestamp(T0 + 2 * MILLIS_PER_DAY + 3 * HOUR);
        let (a, b, fresh) = (card(&t, mine, "a"), card(&t, plain, "b"), card(&t, plain, "c"));
        reviewed(&t, a, 3.0, last);
        reviewed(&t, b, 2.5, last);

        let info = |id| t.ok::<BrowseCardInfo>(CardInfoRef { card_id: id }).retrievability;
        let history = |id| t.ok::<StudyCardHistory>(CardHistoryRequest { card_id: id }).retrievability;
        let (ra, rb) = (info(a).unwrap(), info(b).unwrap());
        // The truth: the preset's decay over whole study days.
        assert_eq!(ra, retrievability(3.0, 9.0, DECAY).unwrap());
        assert_eq!(rb, retrievability(2.5, 9.0, DEFAULT_DECAY).unwrap());
        // Personalised weights decide the order: on the default curve `a` would be stronger.
        let ra_default = retrievability(3.0, 9.0, DEFAULT_DECAY).unwrap();
        assert!(ra < rb && rb < ra_default, "{ra} {rb} {ra_default}");
        assert_eq!(info(fresh), None);
        // Study history panel.
        assert_eq!(history(a), Some(ra));
        assert_eq!(history(b), Some(rb));
        // Statistics and the workload forecast's day 0.
        let reads = t.store.read().unwrap();
        let cards: Vec<_> = [a, b, fresh]
            .iter()
            .map(|id| reads.get_card(*id).unwrap().unwrap())
            .collect();
        let cal = calendar(&*reads, None).unwrap();
        let facts = card_facts(&*reads, &cards, cal, now).unwrap();
        let stats: Vec<Option<f64>> = facts.iter().map(|f| f.retrievability).collect();
        assert_eq!(stats, vec![Some(ra), Some(rb), None]);
        let today = cal.index(now);
        let day0: Vec<Option<f64>> = cards
            .iter()
            .zip([weights.clone(), vec![], vec![]])
            .map(|(c, w)| {
                let fc = crate::modules::planning::forecast_card(c, cal, today);
                fc_study::planning::forecast::Fsrs::new(&w).recall_today(&fc)
            })
            .collect();
        assert_eq!(day0, vec![Some(ra), Some(rb), None]);
        drop(reads);
        // The query, right at each card's own value.
        let eps = 1e-12;
        assert_eq!(hits(&t, &format!("retrievability<{}", ra + eps)), vec![a]);
        assert_eq!(hits(&t, &format!("retrievability<{}", ra - eps)), Vec::<CardId>::new());
        assert_eq!(hits(&t, &format!("retrievability>={}", ra - eps)).len(), 2);
        assert_eq!(hits(&t, &format!("retrievability<{}", rb + eps)).len(), 2);
        assert_eq!(hits(&t, &format!("-retrievability<{}", ra + eps)).len(), 2);
        assert_eq!(hits(&t, &format!("retrievability<{} deck:Mine", rb)), vec![a]);
        // "Lowest recall first" serves the card card info calls weakest.
        let session = t.ok::<StudyStart>(StartSession {
            kind: StudyKind::Normal,
            mode: None,
            deck_id: Some(mine),
            include_subdecks: Some(true),
            filter: None,
            from_session: None,
            new_limit: Some(0),
            order: None,
            tz_offset_mins: Some(0),
            modes: None,
            scope: None,
        });
        assert_eq!(session.current.unwrap().card_id, a);
        t.ok::<StudyEnd>(SessionRef {
            session_id: session.id,
        });
        // Without the personalised preset, `a` falls back to the default curve.
        t.ok::<PresetsAssign>(AssignPreset {
            deck_ids: vec![mine],
            preset_id: None,
        });
        assert_eq!(info(a), Some(ra_default));
        assert_eq!(hits(&t, &format!("retrievability<{}", ra + eps)), Vec::<CardId>::new());
    }

    #[test]
    fn card_info_stats_and_query_agree_in_memory() {
        agree(Arc::new(MemStore::new()));
    }

    #[test]
    fn card_info_stats_and_query_agree_on_sqlite() {
        agree(Arc::new(TempSqlite::new()));
    }
}
