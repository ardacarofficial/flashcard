use super::*;
use proptest::prelude::*;

fn card(n: u8) -> CardId {
    CardId::from_uuid(crate::id::uuid_v7_from_parts(crate::Timestamp(1), &[n; 10]))
}

fn fact(
    card_n: u8,
    day: i64,
    rating: Rating,
    state: CardState,
    elapsed: u32,
    ms: u32,
) -> ReviewFact {
    ReviewFact {
        card_id: card(card_n),
        rating,
        state_before: state,
        elapsed_days: elapsed,
        duration_ms: ms,
        day,
        hour: u8::try_from(day.rem_euclid(24)).unwrap_or(0),
        imported: false,
    }
}

#[test]
fn calendar_round_trips() {
    assert_eq!(day_of_date(1970, 1, 1), 0);
    assert_eq!(date_of_day(0), (1970, 1, 1));
    assert_eq!(weekday(0), 3, "1970-01-01 was a Thursday");
    let d = day_of_date(2027, 6, 20);
    assert_eq!(iso_date(d), "2027-06-20");
    assert_eq!(weekday(d), 6, "2027-06-20 is a Sunday");
    assert_eq!(parse_iso_date("2027-06-20"), Some(d));
    assert_eq!(parse_iso_date("2026-02-30"), None);
    assert_eq!(parse_iso_date("2026-13-01"), None);
    assert_eq!(parse_iso_date("nope"), None);
    assert_eq!(month_start(d), day_of_date(2027, 6, 1));
    assert_eq!(week_start(d), day_of_date(2027, 6, 14));
    for i in -800_000..800_000i64 {
        if i % 997 == 0 {
            let (y, m, dd) = date_of_day(i);
            assert_eq!(day_of_date(y, m, dd), i);
        }
    }
}

#[test]
fn streaks() {
    assert_eq!(streak(&[], 10), Streak::default());
    let s = streak(&[1, 2, 3, 5, 6, 9, 10], 10);
    assert_eq!((s.current, s.longest, s.studied_today), (2, 3, true));
    // Yesterday still counts as a running streak.
    let s = streak(&[7, 8, 9], 10);
    assert_eq!((s.current, s.longest, s.studied_today), (3, 3, false));
    // A gap ends it.
    assert_eq!(streak(&[7, 8], 10).current, 0);
}

#[test]
fn review_report_counts_kinds_and_retention() {
    let facts = vec![
        fact(1, 0, Rating::Good, CardState::New, 0, 3_000),
        fact(1, 1, Rating::Good, CardState::Learning, 1, 5_000),
        fact(1, 5, Rating::Again, CardState::Review, 4, 9_000),
        fact(2, 5, Rating::Good, CardState::Review, 30, 61_000),
        fact(2, 5, Rating::Easy, CardState::Relearning, 0, 1_000),
    ];
    let days = aggregate_days(&facts);
    assert_eq!(days.len(), 3);
    let r = review_stats(&days, 6, None);
    assert_eq!(r.totals.reviews, 5);
    assert_eq!(r.totals.new_cards, 1);
    assert_eq!(r.totals.days_studied, 3);
    assert_eq!(r.totals.days_in_range, 7);
    assert_eq!((r.success.learn.count, r.success.learn.pass), (2, 2));
    assert_eq!(r.success.young.rate, Some(0.0));
    assert_eq!(r.success.mature.rate, Some(1.0));
    assert_eq!(r.success.relearn.count, 1);
    assert_eq!(r.success.all.rate, Some(0.8));
    assert_eq!(r.hours[5].stat.count, 3);
    assert_eq!(r.weekly[0].rate, Some(1.0));
    assert_eq!(r.true_retention, Some(0.5));
    assert_eq!(r.streak.current, 1, "yesterday keeps the streak");
    assert_eq!(r.streak.longest, 2);
    assert_eq!(r.heatmap.len(), 3);
    assert_eq!(r.hours.len(), 24);
    assert_eq!(r.answer_times.slowest[0].card_id, card(2));
    assert_eq!(r.answer_times.slowest[0].duration_ms, 61_000);
    assert_eq!(r.answer_times.slowest.len(), 2, "one entry per card");
    assert_eq!(r.answer_times.buckets.last().map(|b| b.count), Some(1));
    // A range drops older days but keeps the streak.
    let r = review_stats(&days, 6, Some(5));
    assert_eq!(r.totals.reviews, 3);
    assert_eq!(r.totals.days_in_range, 2);
    assert_eq!(r.streak.longest, 2);
}

#[test]
fn card_report_and_forecast() {
    let mk = |state, interval, due, susp| CardFact {
        card_id: card(1),
        state,
        suspended: susp,
        buried: false,
        interval_days: interval,
        due_day: due,
        stability: Some(10.0),
        difficulty: Some(5.0),
        lapses: 0,
        retrievability: (state != CardState::New).then_some(0.9),
    };
    let cards = vec![
        mk(CardState::New, None, None, false),
        mk(CardState::Review, Some(30), Some(8), false),
        mk(CardState::Review, Some(3), Some(10), false),
        mk(CardState::Learning, None, Some(10), false),
        mk(CardState::Review, Some(40), Some(12), true),
    ];
    let s = card_stats(&cards, 10, 7);
    assert_eq!(s.states.new, 1);
    assert_eq!(s.states.mature, 1);
    assert_eq!(s.states.young, 1);
    assert_eq!(s.states.suspended, 1);
    assert_eq!(s.states.total, 5);
    assert_eq!(s.overdue, 1);
    assert_eq!(s.forecast.len(), 7);
    assert_eq!(s.forecast[0].due, 3, "overdue + due today");
    assert_eq!(s.forecast[2].due, 0, "suspended card not forecast");
    assert_eq!(s.report.cards, 4);
    assert_eq!(s.report.mature, 1);
    assert!((s.report.settled_pct - 25.0).abs() < 1e-9);
    assert!((s.report.studied_pct - 75.0).abs() < 1e-9);
    assert_eq!(s.stability.iter().map(|b| b.count).sum::<u32>(), 3);
}

fn arb_fact() -> impl Strategy<Value = ReviewFact> {
    (
        0u8..6,
        0i64..40,
        0u8..4,
        0u8..4,
        0u32..60,
        0u32..90_000,
        0u8..24,
        any::<bool>(),
    )
        .prop_map(|(c, day, r, s, el, ms, hour, imported)| ReviewFact {
            card_id: card(c),
            rating: [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy][usize::from(r)],
            state_before: [
                CardState::New,
                CardState::Learning,
                CardState::Review,
                CardState::Relearning,
            ][usize::from(s)],
            elapsed_days: el,
            duration_ms: ms,
            day,
            hour,
            imported,
        })
}

proptest! {
    /// Aggregating in two pieces and merging equals aggregating at once (what the cache
    /// relies on).
    #[test]
    fn aggregates_are_a_monoid(facts in proptest::collection::vec(arb_fact(), 0..120), split in 0usize..120) {
        let split = split.min(facts.len());
        let whole = aggregate_days(&facts);
        let merged = merge_days(&aggregate_days(&facts[..split]), &aggregate_days(&facts[split..]));
        prop_assert_eq!(&whole, &merged);
        prop_assert_eq!(review_stats(&whole, 40, None), review_stats(&merged, 40, None));
    }
}
