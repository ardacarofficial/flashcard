//! Statistics (docs/02 §17, v0.6) as **pure functions over the review log** and card states.
//!
//! The app turns `review_log` rows into [`ReviewFact`]s (it knows the user's study calendar, so
//! every fact carries its study-day index and local hour) and cards into [`CardFact`]s. Nothing
//! here reads storage, clocks or settings.
//!
//! Review statistics are built in two steps so they can be cached:
//!
//! 1. [`aggregate_days`] folds facts into one [`DayAggregate`] per study day. A day aggregate is
//!    a commutative monoid ([`DayAggregate::merge`]): aggregating a log in pieces and merging
//!    gives exactly what aggregating it at once gives. A cache of day aggregates plus the facts
//!    appended since is therefore equal to a recomputation (`fc-app` tests this against every
//!    kind of review-log change).
//! 2. [`review_stats`] turns day aggregates into the report (streak, success rates, heatmap,
//!    trends, hours, answer times, true retention).
//!
//! Card statistics ([`card_stats`]: memory states, forecast, stability/difficulty
//! histograms, the deck report card) come from the current card states.

use crate::card::{CardState, Rating};
use crate::id::CardId;
use serde::{Deserialize, Serialize};

/// A review counts as **mature** when the card had waited at least this many days.
pub const MATURE_DAYS: u32 = 21;
/// Slowest reviews kept per aggregate (one per card).
pub const SLOWEST_KEPT: usize = 10;
/// Upper bounds (ms, inclusive) of the answer-time buckets; a last bucket takes the rest.
pub const TIME_BUCKETS_MS: [u32; 9] = [
    2_000, 4_000, 6_000, 8_000, 10_000, 15_000, 20_000, 30_000, 60_000,
];
/// Upper bounds of the stability histogram (days); a last bucket takes the rest.
pub const STABILITY_BUCKETS: [f64; 9] = [1.0, 3.0, 7.0, 14.0, 30.0, 60.0, 120.0, 365.0, 730.0];
/// Upper bounds of the difficulty histogram (FSRS difficulty, 1–10).
pub const DIFFICULTY_BUCKETS: [f64; 9] = [2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];

// ------------------------------------------------------------------ mastery

/// Where a card stands on the deck page's New / Learning / Mastered bar (docs/11 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Mastery {
    /// Never studied.
    New,
    /// In (re)learning steps, or a review card with an interval under [`MATURE_DAYS`].
    Learning,
    /// A review card whose interval reached [`MATURE_DAYS`] (a "mature" card).
    Mastered,
}

/// The [`Mastery`] of a card in `state` whose current interval is `interval_days`.
pub fn mastery(state: CardState, interval_days: Option<u32>) -> Mastery {
    match state {
        CardState::New => Mastery::New,
        CardState::Review if interval_days.unwrap_or(0) >= MATURE_DAYS => Mastery::Mastered,
        _ => Mastery::Learning,
    }
}

/// The [`Mastery`] of a card from its scheduling state: the interval is the time between its
/// last review and its due time, in whole days.
pub fn card_mastery(s: &crate::card::SchedulingState) -> Mastery {
    let interval = match (s.due, s.last_review) {
        (Some(due), Some(last)) => {
            u32::try_from((due.0 - last.0).max(0) / crate::time::MILLIS_PER_DAY).ok()
        }
        _ => None,
    };
    mastery(s.state, interval)
}

/// Mastered share in percent (0–100, rounded down) of `mastered` out of `total`.
pub fn mastered_percent(mastered: u32, total: u32) -> u8 {
    if total == 0 {
        return 0;
    }
    u8::try_from(u64::from(mastered.min(total)) * 100 / u64::from(total)).unwrap_or(100)
}

// ------------------------------------------------------------------ calendar

/// Study-day index of a civil date (days since 1970-01-01). A study day `i` starts at the
/// local rollover hour of date `i`, so this is also the index of that date's study day.
pub fn day_of_date(year: i64, month: u32, day: u32) -> i64 {
    // Howard Hinnant's `days_from_civil`.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Civil date `(year, month, day)` of a day index ([`day_of_date`]'s inverse).
pub fn date_of_day(index: i64) -> (i64, u32, u32) {
    let z = index + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    // `d` is 1–31 and `m` 1–12 by construction.
    (
        y,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

/// `YYYY-MM-DD` of a day index.
pub fn iso_date(index: i64) -> String {
    let (y, m, d) = date_of_day(index);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Day index of a `YYYY-MM-DD` date; `None` when it is not a valid calendar date.
pub fn parse_iso_date(text: &str) -> Option<i64> {
    let mut parts = text.trim().splitn(3, '-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if !(1..=9999).contains(&y) || !(1..=12).contains(&m) || d == 0 {
        return None;
    }
    let index = day_of_date(y, m, d);
    // Rejects 2026-02-30 and friends.
    (date_of_day(index) == (y, m, d)).then_some(index)
}

/// Weekday of a day index, `0` = Monday … `6` = Sunday (1970-01-01 was a Thursday).
pub fn weekday(index: i64) -> usize {
    // 0..7 after rem_euclid.
    usize::try_from((index + 3).rem_euclid(7)).unwrap_or(0)
}

/// First day of the month containing `index`.
pub fn month_start(index: i64) -> i64 {
    let (y, m, _) = date_of_day(index);
    day_of_date(y, m, 1)
}

/// Monday of the week containing `index`.
pub fn week_start(index: i64) -> i64 {
    index - i64::try_from(weekday(index)).unwrap_or(0)
}

// ------------------------------------------------------------------ facts

/// One review, as statistics see it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewFact {
    /// Card.
    pub card_id: CardId,
    /// Button pressed.
    pub rating: Rating,
    /// State before the answer.
    pub state_before: CardState,
    /// Study days since the previous review.
    pub elapsed_days: u32,
    /// Show → rate, ms.
    pub duration_ms: u32,
    /// Study-day index of the review.
    pub day: i64,
    /// Local hour (0–23).
    pub hour: u8,
    /// Brought in by an import (`review_log.mode = imported`).
    pub imported: bool,
}

/// What a review was, for success rates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewKind {
    /// New or in learning steps.
    Learn,
    /// Review of a card that waited less than [`MATURE_DAYS`].
    Young,
    /// Review of a card that waited at least [`MATURE_DAYS`].
    Mature,
    /// In relearning steps.
    Relearn,
}

impl ReviewFact {
    /// Kind of the review.
    pub fn kind(&self) -> ReviewKind {
        match self.state_before {
            CardState::New | CardState::Learning => ReviewKind::Learn,
            CardState::Relearning => ReviewKind::Relearn,
            CardState::Review if self.elapsed_days >= MATURE_DAYS => ReviewKind::Mature,
            CardState::Review => ReviewKind::Young,
        }
    }

    /// Recalled (anything but Again).
    pub fn passed(&self) -> bool {
        self.rating != Rating::Again
    }
}

// ------------------------------------------------------------------ day aggregates

/// Answers and how many of them were not "Again".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PassCount {
    /// Answers.
    pub count: u32,
    /// Answers other than Again.
    pub pass: u32,
}

impl PassCount {
    fn add(&mut self, passed: bool) {
        self.count = self.count.saturating_add(1);
        self.pass = self.pass.saturating_add(u32::from(passed));
    }

    fn merge(&mut self, o: PassCount) {
        self.count = self.count.saturating_add(o.count);
        self.pass = self.pass.saturating_add(o.pass);
    }

    /// `pass / count`, `None` without answers.
    pub fn rate(&self) -> Option<f64> {
        (self.count > 0).then(|| f64::from(self.pass) / f64::from(self.count))
    }
}

/// Answers, passes and their rate, as reported (the rate is computed here, not by clients).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RateStat {
    /// Answers.
    pub count: u32,
    /// Answers other than Again.
    pub pass: u32,
    /// `pass / count` (0–1), `None` without answers.
    pub rate: Option<f64>,
}

impl From<PassCount> for RateStat {
    fn from(p: PassCount) -> Self {
        Self {
            count: p.count,
            pass: p.pass,
            rate: p.rate(),
        }
    }
}

/// Success rate in one local hour.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HourStat {
    /// Hour (0–23).
    pub hour: u8,
    /// Answers and success rate.
    pub stat: RateStat,
}

/// Answers in one local hour (cached form).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HourCount {
    /// Hour (0–23).
    pub hour: u8,
    /// Answers.
    pub count: u32,
    /// Answers other than Again.
    pub pass: u32,
}

/// A slow card: its slowest answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SlowCard {
    /// Card.
    pub card_id: CardId,
    /// Slowest show → rate time, ms.
    pub duration_ms: u32,
}

/// Everything review statistics need from one study day. Mergeable: see the module docs.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DayAggregate {
    /// Study-day index.
    pub day: i64,
    /// Answers.
    pub reviews: u32,
    /// Time spent answering, ms.
    pub time_ms: u64,
    /// Answers of new/learning cards.
    pub learn: PassCount,
    /// Answers of young review cards.
    pub young: PassCount,
    /// Answers of mature review cards.
    pub mature: PassCount,
    /// Answers in relearning.
    pub relearn: PassCount,
    /// First answers of new cards (cards introduced).
    pub new_cards: u32,
    /// Answers that came from an import.
    pub imported: u32,
    /// Answers per local hour (sorted by hour, non-empty hours only).
    pub hours: Vec<HourCount>,
    /// Answer-time histogram ([`TIME_BUCKETS_MS`] + one overflow bucket).
    pub time_buckets: Vec<u32>,
    /// Slowest cards (at most [`SLOWEST_KEPT`], slowest first, one entry per card).
    pub slowest: Vec<SlowCard>,
}

fn time_bucket(ms: u32) -> usize {
    TIME_BUCKETS_MS
        .iter()
        .position(|b| ms <= *b)
        .unwrap_or(TIME_BUCKETS_MS.len())
}

/// Keeps the slowest answer per card, slowest first (ties by card id), at most
/// [`SLOWEST_KEPT`].
fn top_slowest(mut all: Vec<SlowCard>) -> Vec<SlowCard> {
    all.sort_by(|a, b| {
        a.card_id
            .cmp(&b.card_id)
            .then(b.duration_ms.cmp(&a.duration_ms))
    });
    all.dedup_by_key(|s| s.card_id);
    all.sort_by(|a, b| {
        b.duration_ms
            .cmp(&a.duration_ms)
            .then(a.card_id.cmp(&b.card_id))
    });
    all.truncate(SLOWEST_KEPT);
    all
}

impl DayAggregate {
    /// An empty aggregate of `day`.
    pub fn empty(day: i64) -> Self {
        Self {
            day,
            time_buckets: vec![0; TIME_BUCKETS_MS.len() + 1],
            ..Self::default()
        }
    }

    /// Adds one review (of this day).
    pub fn add(&mut self, f: &ReviewFact) {
        let passed = f.passed();
        self.reviews = self.reviews.saturating_add(1);
        self.time_ms = self.time_ms.saturating_add(u64::from(f.duration_ms));
        match f.kind() {
            ReviewKind::Learn => self.learn.add(passed),
            ReviewKind::Young => self.young.add(passed),
            ReviewKind::Mature => self.mature.add(passed),
            ReviewKind::Relearn => self.relearn.add(passed),
        }
        if f.state_before == CardState::New {
            self.new_cards = self.new_cards.saturating_add(1);
        }
        if f.imported {
            self.imported = self.imported.saturating_add(1);
        }
        let hour = f.hour.min(23);
        match self.hours.binary_search_by_key(&hour, |h| h.hour) {
            Ok(i) => {
                let h = &mut self.hours[i];
                h.count = h.count.saturating_add(1);
                h.pass = h.pass.saturating_add(u32::from(passed));
            }
            Err(i) => self.hours.insert(
                i,
                HourCount {
                    hour,
                    count: 1,
                    pass: u32::from(passed),
                },
            ),
        }
        if self.time_buckets.len() != TIME_BUCKETS_MS.len() + 1 {
            self.time_buckets.resize(TIME_BUCKETS_MS.len() + 1, 0);
        }
        let b = time_bucket(f.duration_ms);
        self.time_buckets[b] = self.time_buckets[b].saturating_add(1);
        let mut slowest = std::mem::take(&mut self.slowest);
        slowest.push(SlowCard {
            card_id: f.card_id,
            duration_ms: f.duration_ms,
        });
        self.slowest = top_slowest(slowest);
    }

    /// Merges another aggregate into this one (any day; the result keeps `self.day`).
    pub fn merge(&mut self, o: &DayAggregate) {
        self.reviews = self.reviews.saturating_add(o.reviews);
        self.time_ms = self.time_ms.saturating_add(o.time_ms);
        self.learn.merge(o.learn);
        self.young.merge(o.young);
        self.mature.merge(o.mature);
        self.relearn.merge(o.relearn);
        self.new_cards = self.new_cards.saturating_add(o.new_cards);
        self.imported = self.imported.saturating_add(o.imported);
        for h in &o.hours {
            match self.hours.binary_search_by_key(&h.hour, |x| x.hour) {
                Ok(i) => {
                    let x = &mut self.hours[i];
                    x.count = x.count.saturating_add(h.count);
                    x.pass = x.pass.saturating_add(h.pass);
                }
                Err(i) => self.hours.insert(i, *h),
            }
        }
        let n = self.time_buckets.len().max(o.time_buckets.len());
        self.time_buckets.resize(n, 0);
        for (i, c) in o.time_buckets.iter().enumerate() {
            self.time_buckets[i] = self.time_buckets[i].saturating_add(*c);
        }
        let mut slowest = std::mem::take(&mut self.slowest);
        slowest.extend_from_slice(&o.slowest);
        self.slowest = top_slowest(slowest);
    }

    /// Answers other than Again.
    pub fn passed(&self) -> u32 {
        self.learn.pass + self.young.pass + self.mature.pass + self.relearn.pass
    }
}

/// One aggregate per study day that has reviews, sorted by day.
pub fn aggregate_days(facts: &[ReviewFact]) -> Vec<DayAggregate> {
    let mut out: std::collections::BTreeMap<i64, DayAggregate> = std::collections::BTreeMap::new();
    for f in facts {
        out.entry(f.day)
            .or_insert_with(|| DayAggregate::empty(f.day))
            .add(f);
    }
    out.into_values().collect()
}

/// Merges two per-day aggregate lists (each sorted by day) into one, sorted by day.
pub fn merge_days(a: &[DayAggregate], b: &[DayAggregate]) -> Vec<DayAggregate> {
    let mut out: std::collections::BTreeMap<i64, DayAggregate> =
        a.iter().map(|d| (d.day, d.clone())).collect();
    for d in b {
        match out.get_mut(&d.day) {
            Some(x) => x.merge(d),
            None => {
                out.insert(d.day, d.clone());
            }
        }
    }
    out.into_values().collect()
}

// ------------------------------------------------------------------ review report

/// Consecutive study days with reviews.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Streak {
    /// Run ending today (or yesterday, when today has no review yet).
    pub current: u32,
    /// Longest run ever.
    pub longest: u32,
    /// Today has a review.
    pub studied_today: bool,
}

/// Success rates per kind of review.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SuccessRates {
    /// New and learning cards.
    pub learn: RateStat,
    /// Young review cards.
    pub young: RateStat,
    /// Mature review cards.
    pub mature: RateStat,
    /// Relearning.
    pub relearn: RateStat,
    /// All answers.
    pub all: RateStat,
}

/// One heatmap cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HeatDay {
    /// Study-day index.
    pub day: i64,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Answers.
    pub reviews: u32,
    /// Time, ms.
    pub time_ms: u64,
}

/// Totals of a week or month.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PeriodPoint {
    /// First day of the period.
    pub start_day: i64,
    /// `YYYY-MM-DD` of `start_day`.
    pub start_date: String,
    /// Answers.
    pub reviews: u32,
    /// Answers other than Again.
    pub pass: u32,
    /// `pass / reviews`.
    pub rate: Option<f64>,
    /// New cards introduced.
    pub new_cards: u32,
    /// Time, ms.
    pub time_ms: u64,
}

/// One answer-time histogram bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TimeBucket {
    /// Inclusive upper bound, ms (`None` = the overflow bucket).
    pub up_to_ms: Option<u32>,
    /// Answers.
    pub count: u32,
}

/// Answer times.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AnswerTimes {
    /// Mean, ms (`None` without answers).
    pub mean_ms: Option<f64>,
    /// Histogram.
    pub buckets: Vec<TimeBucket>,
    /// Slowest cards, slowest first.
    pub slowest: Vec<SlowCard>,
}

/// Period totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReviewTotals {
    /// Answers.
    pub reviews: u32,
    /// Time, ms.
    pub time_ms: u64,
    /// Days with at least one answer.
    pub days_studied: u32,
    /// Days in the period (from the first day with data, or the range start, to today).
    pub days_in_range: u32,
    /// New cards introduced.
    pub new_cards: u32,
    /// Answers brought in by imports (also counted in `reviews`).
    pub imported: u32,
}

/// Review statistics of a period.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReviewStats {
    /// Streak (over all days, not only the range).
    pub streak: Streak,
    /// Totals.
    pub totals: ReviewTotals,
    /// Success rates.
    pub success: SuccessRates,
    /// True retention: pass rate of review answers (young + mature).
    pub true_retention: Option<f64>,
    /// Answers per day (days with answers).
    pub heatmap: Vec<HeatDay>,
    /// Weekly trend (weeks start on Monday).
    pub weekly: Vec<PeriodPoint>,
    /// Monthly trend.
    pub monthly: Vec<PeriodPoint>,
    /// Success by local hour, all 24 hours.
    pub hours: Vec<HourStat>,
    /// Answer times.
    pub answer_times: AnswerTimes,
}

/// Longest and current run of consecutive days among `days` (sorted ascending).
pub fn streak(days: &[i64], today: i64) -> Streak {
    let mut longest = 0u32;
    let mut run = 0u32;
    let mut prev: Option<i64> = None;
    for &d in days {
        if d > today {
            break;
        }
        run = match prev {
            Some(p) if d == p + 1 => run.saturating_add(1),
            Some(p) if d == p => run,
            _ => 1,
        };
        longest = longest.max(run);
        prev = Some(d);
    }
    let studied_today = prev == Some(today);
    let current = match prev {
        Some(p) if p == today || p + 1 == today => run,
        _ => 0,
    };
    Streak {
        current,
        longest,
        studied_today,
    }
}

fn period_points(days: &[&DayAggregate], start_of: impl Fn(i64) -> i64) -> Vec<PeriodPoint> {
    let mut out: Vec<PeriodPoint> = Vec::new();
    for d in days {
        let start = start_of(d.day);
        if out.last().is_none_or(|p| p.start_day != start) {
            out.push(PeriodPoint {
                start_day: start,
                start_date: iso_date(start),
                reviews: 0,
                pass: 0,
                rate: None,
                new_cards: 0,
                time_ms: 0,
            });
        }
        if let Some(p) = out.last_mut() {
            p.reviews = p.reviews.saturating_add(d.reviews);
            p.pass = p.pass.saturating_add(d.passed());
            p.new_cards = p.new_cards.saturating_add(d.new_cards);
            p.time_ms = p.time_ms.saturating_add(d.time_ms);
        }
    }
    for p in &mut out {
        p.rate = PassCount {
            count: p.reviews,
            pass: p.pass,
        }
        .rate();
    }
    out
}

/// The review report of `days` (sorted by day, as [`aggregate_days`] returns) for the days
/// `from..=today` (`from = None`: everything up to today). The streak always looks at every
/// day given.
pub fn review_stats(days: &[DayAggregate], today: i64, from: Option<i64>) -> ReviewStats {
    let all_days: Vec<i64> = days
        .iter()
        .filter(|d| d.reviews > 0)
        .map(|d| d.day)
        .collect();
    let in_range: Vec<&DayAggregate> = days
        .iter()
        .filter(|d| d.day <= today && from.is_none_or(|f| d.day >= f))
        .collect();
    let mut total = DayAggregate::empty(today);
    for d in &in_range {
        total.merge(d);
    }
    let first = from.or_else(|| in_range.first().map(|d| d.day));
    let days_in_range = first.map_or(0, |f| {
        u32::try_from((today - f + 1).max(0)).unwrap_or(u32::MAX)
    });
    let mut all = PassCount::default();
    for k in [total.learn, total.young, total.mature, total.relearn] {
        all.merge(k);
    }
    let mut reviews_pass = total.young;
    reviews_pass.merge(total.mature);
    let hours = (0u8..24)
        .map(|h| {
            let c = total.hours.iter().find(|x| x.hour == h);
            HourStat {
                hour: h,
                stat: PassCount {
                    count: c.map_or(0, |c| c.count),
                    pass: c.map_or(0, |c| c.pass),
                }
                .into(),
            }
        })
        .collect();
    let buckets = total
        .time_buckets
        .iter()
        .enumerate()
        .map(|(i, c)| TimeBucket {
            up_to_ms: TIME_BUCKETS_MS.get(i).copied(),
            count: *c,
        })
        .collect();
    ReviewStats {
        streak: streak(&all_days, today),
        totals: ReviewTotals {
            reviews: total.reviews,
            time_ms: total.time_ms,
            days_studied: u32::try_from(in_range.iter().filter(|d| d.reviews > 0).count())
                .unwrap_or(u32::MAX),
            days_in_range,
            new_cards: total.new_cards,
            imported: total.imported,
        },
        success: SuccessRates {
            learn: total.learn.into(),
            young: total.young.into(),
            mature: total.mature.into(),
            relearn: total.relearn.into(),
            all: all.into(),
        },
        true_retention: reviews_pass.rate(),
        heatmap: in_range
            .iter()
            .filter(|d| d.reviews > 0)
            .map(|d| HeatDay {
                day: d.day,
                date: iso_date(d.day),
                reviews: d.reviews,
                time_ms: d.time_ms,
            })
            .collect(),
        weekly: period_points(&in_range, week_start),
        monthly: period_points(&in_range, month_start),
        hours,
        answer_times: AnswerTimes {
            mean_ms: (total.reviews > 0).then(|| total.time_ms as f64 / f64::from(total.reviews)),
            buckets,
            slowest: total.slowest,
        },
    }
}

// ------------------------------------------------------------------ card report

/// One card, as statistics see it.
#[derive(Debug, Clone, PartialEq)]
pub struct CardFact {
    /// Card.
    pub card_id: CardId,
    /// Learning state.
    pub state: CardState,
    /// Suspended.
    pub suspended: bool,
    /// Buried (until after now).
    pub buried: bool,
    /// Current interval in days (review cards: due day − last review day).
    pub interval_days: Option<u32>,
    /// Study-day index the card is due on (scheduled cards).
    pub due_day: Option<i64>,
    /// FSRS stability, days.
    pub stability: Option<f64>,
    /// FSRS difficulty.
    pub difficulty: Option<f64>,
    /// Lapses.
    pub lapses: u32,
    /// Probability of recall now (reviewed cards), with the card's deck parameters.
    pub retrievability: Option<f64>,
}

impl CardFact {
    /// A review card whose interval reached [`MATURE_DAYS`].
    pub fn is_mature(&self) -> bool {
        self.state == CardState::Review && self.interval_days.unwrap_or(0) >= MATURE_DAYS
    }
}

/// Cards per memory state (docs/02 §17 "Hafıza durumu"). Suspended cards count only as
/// suspended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StateCounts {
    /// Never studied.
    pub new: u32,
    /// In learning steps.
    pub learning: u32,
    /// In relearning steps.
    pub relearning: u32,
    /// Review cards with an interval under [`MATURE_DAYS`].
    pub young: u32,
    /// Review cards with an interval of at least [`MATURE_DAYS`].
    pub mature: u32,
    /// Suspended.
    pub suspended: u32,
    /// Buried right now (also counted in their state).
    pub buried: u32,
    /// All cards.
    pub total: u32,
}

/// Cards due on one day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ForecastDay {
    /// Study-day index.
    pub day: i64,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Cards due that day (today includes overdue cards).
    pub due: u32,
}

/// One histogram bucket.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HistBucket {
    /// Exclusive lower bound (`None` = below the first bound).
    pub from: Option<f64>,
    /// Inclusive upper bound (`None` = the overflow bucket).
    pub to: Option<f64>,
    /// Cards.
    pub count: u32,
}

/// How settled a deck (or any scope) is (docs/02 §17 "Deste karnesi").
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReportCard {
    /// Cards (suspended excluded).
    pub cards: u32,
    /// Cards studied at least once.
    pub studied: u32,
    /// Mature cards.
    pub mature: u32,
    /// `mature / cards`, 0–100 (the "settled" share).
    pub settled_pct: f64,
    /// `studied / cards`, 0–100.
    pub studied_pct: f64,
    /// Mean probability of recall of studied cards now.
    pub mean_recall: Option<f64>,
}

/// Card statistics.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CardStats {
    /// Memory states.
    pub states: StateCounts,
    /// Cards due today (incl. overdue) and on each following day.
    pub forecast: Vec<ForecastDay>,
    /// Overdue cards (due before today; included in the first forecast day).
    pub overdue: u32,
    /// FSRS stability histogram (days).
    pub stability: Vec<HistBucket>,
    /// FSRS difficulty histogram.
    pub difficulty: Vec<HistBucket>,
    /// Report card.
    pub report: ReportCard,
}

fn histogram(values: impl Iterator<Item = f64>, bounds: &[f64]) -> Vec<HistBucket> {
    let mut counts = vec![0u32; bounds.len() + 1];
    for v in values.filter(|v| v.is_finite()) {
        let i = bounds.iter().position(|b| v <= *b).unwrap_or(bounds.len());
        counts[i] = counts[i].saturating_add(1);
    }
    counts
        .into_iter()
        .enumerate()
        .map(|(i, count)| HistBucket {
            from: i.checked_sub(1).and_then(|j| bounds.get(j).copied()),
            to: bounds.get(i).copied(),
            count,
        })
        .collect()
}

fn pct(part: u32, whole: u32) -> f64 {
    if whole == 0 {
        0.0
    } else {
        f64::from(part) * 100.0 / f64::from(whole)
    }
}

/// Memory states only (cheap; used by breakdowns).
pub fn state_counts(cards: &[CardFact]) -> StateCounts {
    let mut s = StateCounts::default();
    for c in cards {
        s.total += 1;
        if c.buried {
            s.buried += 1;
        }
        if c.suspended {
            s.suspended += 1;
            continue;
        }
        match c.state {
            CardState::New => s.new += 1,
            CardState::Learning => s.learning += 1,
            CardState::Relearning => s.relearning += 1,
            CardState::Review if c.is_mature() => s.mature += 1,
            CardState::Review => s.young += 1,
        }
    }
    s
}

/// The report card of `cards`.
pub fn report_card(cards: &[CardFact]) -> ReportCard {
    let active: Vec<&CardFact> = cards.iter().filter(|c| !c.suspended).collect();
    let n = u32::try_from(active.len()).unwrap_or(u32::MAX);
    let studied = u32::try_from(active.iter().filter(|c| c.state != CardState::New).count())
        .unwrap_or(u32::MAX);
    let mature = u32::try_from(active.iter().filter(|c| c.is_mature()).count()).unwrap_or(u32::MAX);
    let recalls: Vec<f64> = active
        .iter()
        .filter_map(|c| c.retrievability)
        .filter(|r| r.is_finite())
        .collect();
    ReportCard {
        cards: n,
        studied,
        mature,
        settled_pct: pct(mature, n),
        studied_pct: pct(studied, n),
        mean_recall: (!recalls.is_empty())
            .then(|| recalls.iter().sum::<f64>() / recalls.len() as f64),
    }
}

/// Card statistics of `cards` on `today`, with a forecast of `forecast_days` days (today
/// included).
pub fn card_stats(cards: &[CardFact], today: i64, forecast_days: u32) -> CardStats {
    let n = i64::from(forecast_days.max(1));
    let mut due = vec![0u32; usize::try_from(n).unwrap_or(1)];
    let mut overdue = 0u32;
    for c in cards {
        if c.suspended || c.state == CardState::New {
            continue;
        }
        let Some(d) = c.due_day else { continue };
        if d < today {
            overdue += 1;
        }
        let i = (d - today).max(0);
        if i < n {
            if let Some(slot) = usize::try_from(i).ok().and_then(|i| due.get_mut(i)) {
                *slot += 1;
            }
        }
    }
    let reviewed = || {
        cards
            .iter()
            .filter(|c| !c.suspended && c.state != CardState::New)
    };
    CardStats {
        states: state_counts(cards),
        forecast: due
            .into_iter()
            .enumerate()
            .map(|(i, count)| {
                let day = today + i64::try_from(i).unwrap_or(0);
                ForecastDay {
                    day,
                    date: iso_date(day),
                    due: count,
                }
            })
            .collect(),
        overdue,
        stability: histogram(reviewed().filter_map(|c| c.stability), &STABILITY_BUCKETS),
        difficulty: histogram(reviewed().filter_map(|c| c.difficulty), &DIFFICULTY_BUCKETS),
        report: report_card(cards),
    }
}

#[cfg(test)]
mod tests;
