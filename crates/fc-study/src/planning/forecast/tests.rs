#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn params(new_per_day: u32, days: u32) -> ForecastParams {
    ForecastParams {
        weights: Vec::new(),
        desired_retention: 0.9,
        max_interval: 36_500,
        learning_steps: 2,
        relearning_steps: 1,
        new_per_day,
        new_done_today: 0,
        days,
        secs_per_answer: 60.0,
    }
}

fn new_card() -> ForecastCard {
    ForecastCard {
        state: ForecastState::New,
        due_in: 0,
        stability: None,
        difficulty: None,
        elapsed: None,
    }
}

fn review(due_in: i64, s: f64, d: f64, elapsed: i64) -> ForecastCard {
    ForecastCard {
        state: ForecastState::Review,
        due_in,
        stability: Some(s),
        difficulty: Some(d),
        elapsed: Some(elapsed),
    }
}

/// A deterministic mixed collection: reviews of many stabilities/difficulties (some overdue),
/// learning cards and `new` unseen cards.
fn collection(reviews: usize, new: usize) -> Vec<ForecastCard> {
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % 10_000) as f64 / 10_000.0
    };
    let mut out = Vec::new();
    for i in 0..reviews {
        let s = 0.5 + next() * 120.0;
        let d = 1.0 + next() * 9.0;
        let due = (next() * 40.0) as i64 - 5;
        let elapsed = (next() * s) as i64 + 1;
        out.push(review(due, s, d, elapsed));
        if i % 10 == 0 {
            out.push(ForecastCard {
                state: ForecastState::Learning,
                due_in: (i % 3) as i64,
                stability: (i % 20 == 0).then_some(1.5),
                difficulty: None,
                elapsed: None,
            });
        }
    }
    out.extend((0..new).map(|_| new_card()));
    out
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn fsrs_curve_matches_its_definition() {
    let f = Fsrs::new(&[]);
    // Stability = days until recall falls to 90 %.
    assert!(close(f.retrievability(10.0, 10.0), 0.9));
    assert_eq!(f.interval(10.0, 0.9, 36_500), 10);
    assert_eq!(f.interval(10.0, 0.9, 7), 7);
    assert_eq!(f.interval(0.01, 0.9, 100), 1);
    // A higher retention means shorter intervals.
    assert!(f.interval(30.0, 0.95, 36_500) < f.interval(30.0, 0.8, 36_500));
    // Success grows stability, a lapse shrinks it; "Again" makes a card harder.
    assert!(f.stability_after_success(10.0, 5.0, 0.9) > 10.0);
    assert!(f.stability_after_failure(10.0, 5.0, 0.9) < 10.0);
    assert!(f.next_difficulty(5.0, 1) > f.next_difficulty(5.0, 3));
    // Bad weights fall back to the defaults; FSRS-4.5 weights decay with 0.5.
    assert_eq!(Fsrs::new(&[1.0, 2.0]), f);
    assert!(close(Fsrs::new(&DEFAULT_WEIGHTS[..17]).decay, 0.5));
}

#[test]
fn hand_computed_review_card() {
    // s = 10, reviewed 10 days ago → R = 0.9 today: 1 answer + 10 % relearning step = 1.1.
    let f = forecast(&[review(0, 10.0, 5.0, 10)], &params(0, 1));
    let day = f.days[0];
    assert!(close(day.reviews, 1.1), "{day:?}");
    assert!(close(day.answers, 1.1));
    assert!(close(day.minutes, 1.1)); // 60 s per answer
    assert!(close(day.started, 1.0));
    assert!(close(day.unseen, 0.0));
    assert_eq!(f.new_finished_on, None);
}

#[test]
fn hand_computed_new_cards() {
    // 3 unseen cards, 2 a day, 2 learning steps (3 answers each on their first day).
    let cards = vec![new_card(); 3];
    let f = forecast(&cards, &params(2, 3));
    let d: Vec<_> = f
        .days
        .iter()
        .map(|p| (p.introduced, p.new_answers, p.reviews))
        .collect();
    assert_eq!(d[0], (2.0, 6.0, 0.0));
    assert_eq!(d[1], (1.0, 3.0, 0.0));
    // Day 2: the two cards of day 0 come back (first interval = round(w2) = 2 days) with
    // R = (1 + factor·2/w2)^-decay; the forgotten part adds one relearning answer.
    let w = DEFAULT_WEIGHTS;
    let decay = w[20];
    let factor = 0.9f64.powf(-1.0 / decay) - 1.0;
    let r = (1.0 + factor * 2.0 / w[2]).powf(-decay);
    assert!(close(d[2].2, 2.0 * (1.0 + (1.0 - r))), "{:?} vs {r}", d[2]);
    assert_eq!(d[2].0, 0.0);
    assert_eq!(
        f.days.iter().map(|p| p.unseen).collect::<Vec<_>>(),
        vec![1.0, 0.0, 0.0]
    );
    assert_eq!(f.new_finished_on, Some(1));
    // Cards already started today use up today's allowance first.
    let f = forecast(
        &cards,
        &ForecastParams {
            new_done_today: 1,
            ..params(2, 3)
        },
    );
    assert_eq!(f.days[0].introduced, 1.0);
    assert_eq!(f.days[1].introduced, 2.0);
}

#[test]
fn learning_and_overdue_cards() {
    let cards = [
        // Overdue by 5 days: due today.
        review(-5, 20.0, 5.0, 25),
        ForecastCard {
            state: ForecastState::Learning,
            due_in: 1,
            stability: None,
            difficulty: None,
            elapsed: None,
        },
    ];
    let f = forecast(&cards, &params(0, 3));
    assert!(f.days[0].reviews > 1.0 && f.days[0].reviews < 1.5);
    // The learning card's last step is tomorrow; it graduates for ≥ 1 day.
    assert!(close(f.days[1].reviews, 1.0));
    assert!(f.days.iter().all(|p| close(p.started, 2.0)));
}

#[test]
fn conserves_cards() {
    let cards = collection(400, 250);
    for pace in [0, 7, 20, 300] {
        let f = forecast(&cards, &params(pace, 120));
        assert!(close(f.total_cards, cards.len() as f64));
        for p in &f.days {
            assert!(
                (p.started + p.unseen - f.total_cards).abs() < 1e-6,
                "pace {pace} day {}: {} + {} ≠ {}",
                p.offset,
                p.started,
                p.unseen,
                f.total_cards
            );
            assert!(close(p.answers, p.reviews + p.new_answers));
        }
        let introduced: f64 = f.days.iter().map(|p| p.introduced).sum();
        assert!(introduced <= 250.0 + 1e-9);
    }
}

#[test]
fn monotonic_in_new_cards_per_day() {
    // Plenty of new cards: every day's load grows with the pace.
    let cards = collection(300, 5_000);
    let mut prev: Option<Forecast> = None;
    for pace in [0, 1, 5, 10, 20, 40] {
        let f = forecast(&cards, &params(pace, 60));
        if let Some(p) = &prev {
            for (a, b) in p.days.iter().zip(&f.days) {
                assert!(
                    b.answers + 1e-9 >= a.answers,
                    "pace {pace} day {}",
                    a.offset
                );
                assert!(b.minutes + 1e-9 >= a.minutes);
            }
        }
        prev = Some(f);
    }
    // Few new cards: a faster pace front-loads them, but the work done up to any day never
    // drops.
    let cards = collection(100, 60);
    let mut prev: Option<Vec<f64>> = None;
    for pace in 0..70 {
        let f = forecast(&cards, &params(pace, 45));
        let cumulative: Vec<f64> = f
            .days
            .iter()
            .scan(0.0, |acc, p| {
                *acc += p.answers;
                Some(*acc)
            })
            .collect();
        if let Some(p) = &prev {
            for (day, (a, b)) in p.iter().zip(&cumulative).enumerate() {
                assert!(b + 1e-9 >= *a, "pace {pace} day {day}: {b} < {a}");
            }
        }
        prev = Some(cumulative);
    }
}

#[test]
fn deterministic_and_bounded() {
    let cards = collection(500, 100);
    let a = forecast(&cards, &params(15, 90));
    let b = forecast(&cards, &params(15, 90));
    assert_eq!(a, b);
    assert_eq!(a.days.len(), 90);
    assert_eq!(forecast(&cards, &params(15, 0)).days.len(), 1);
    assert_eq!(
        forecast(&cards, &params(15, 10_000)).days.len(),
        MAX_DAYS as usize
    );
    // A higher retention costs more reviews.
    let hi = forecast(
        &cards,
        &ForecastParams {
            desired_retention: 0.95,
            ..params(15, 90)
        },
    );
    let total = |f: &Forecast| f.days.iter().map(|p| p.answers).sum::<f64>();
    assert!(total(&hi) > total(&a));
    // Week means.
    let (cards_pd, mins_pd) = a.week_ending(20);
    let manual: f64 = a.days[14..=20].iter().map(|p| p.answers).sum::<f64>() / 7.0;
    assert!(close(cards_pd, manual));
    assert!(close(mins_pd, manual)); // 60 s per answer
}
