#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use fc_core::rng::{Rng, SplitMix64};

const DAY: i64 = MILLIS_PER_DAY;
const MIN: i64 = 60_000;
const T0: Timestamp = Timestamp(1_767_225_600_000); // 2026-01-01T00:00Z

fn params(learning: &[u32], relearning: &[u32]) -> SchedulerParams {
    SchedulerParams {
        desired_retention: 0.9,
        maximum_interval_days: DEFAULT_MAXIMUM_INTERVAL_DAYS,
        learning_steps_mins: learning.to_vec(),
        relearning_steps_mins: relearning.to_vec(),
        extra: serde_json::Value::Null,
    }
}

fn fsrs() -> &'static dyn Scheduler {
    builtin(FSRS_ID).unwrap()
}

fn sm2() -> &'static dyn Scheduler {
    builtin(SM2_ID).unwrap()
}

// ------------------------------------------------------------------ FSRS-6 reference
//
// An independent f64 implementation of the published FSRS-6 formulas
// (https://github.com/open-spaced-repetition/fsrs4anki/wiki/The-Algorithm), written from the
// spec rather than from `fsrs-rs`, so the simulation below checks the integration against the
// algorithm itself.

const W: [f64; 21] = [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835,
    0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
];

fn factor() -> f64 {
    0.9f64.powf(-1.0 / W[20]) - 1.0
}

fn ref_retrievability(t: f64, s: f64) -> f64 {
    (1.0 + factor() * t / s).powf(-W[20])
}

fn ref_interval(s: f64, r: f64) -> f64 {
    s / factor() * (r.powf(-1.0 / W[20]) - 1.0)
}

fn ref_init_difficulty(g: f64) -> f64 {
    W[4] - (W[5] * (g - 1.0)).exp() + 1.0
}

/// `(stability, difficulty)` after rating `g` (1–4) with `t` whole days elapsed.
fn ref_step(prev: Option<(f64, f64)>, g: f64, t: f64) -> (f64, f64) {
    let clamp_s = |s: f64| s.clamp(0.0001, 36500.0);
    let clamp_d = |d: f64| d.clamp(1.0, 10.0);
    let Some((s, d)) = prev else {
        return (clamp_s(W[g as usize - 1]), clamp_d(ref_init_difficulty(g)));
    };
    let s_new = if t == 0.0 {
        let sinc = (W[17] * (g - 3.0 + W[18])).exp() * s.powf(-W[19]);
        s * if g >= 2.0 { sinc.max(1.0) } else { sinc }
    } else {
        let r = ref_retrievability(t, s);
        if g == 1.0 {
            let fail =
                W[11] * d.powf(-W[12]) * ((s + 1.0).powf(W[13]) - 1.0) * (W[14] * (1.0 - r)).exp();
            fail.min(s / (W[17] * W[18]).exp())
        } else {
            let hard = if g == 2.0 { W[15] } else { 1.0 };
            let easy = if g == 4.0 { W[16] } else { 1.0 };
            s * ((W[8]).exp()
                * (11.0 - d)
                * s.powf(-W[9])
                * ((W[10] * (1.0 - r)).exp() - 1.0)
                * hard
                * easy
                + 1.0)
        }
    };
    let delta = -W[6] * (g - 3.0);
    let linear = d + (10.0 - d) / 9.0 * delta;
    let d_new = W[7] * ref_init_difficulty(4.0) + (1.0 - W[7]) * linear;
    (clamp_s(s_new), clamp_d(d_new))
}

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * a.abs().max(b.abs()).max(1.0)
}

fn rating_of(rng: &mut SplitMix64) -> Rating {
    // 10 % again, 10 % hard, 70 % good, 10 % easy.
    match rng.below(10) {
        0 => Rating::Again,
        1 => Rating::Hard,
        9 => Rating::Easy,
        _ => Rating::Good,
    }
}

fn g(r: Rating) -> f64 {
    (rating_index(r) + 1) as f64
}

/// Reviews one card on every due date for 365 days with a deterministic clock; every
/// transition must match the reference formulas and the scheduler's invariants.
fn simulate_365(p: &SchedulerParams, seed: u64) -> usize {
    let s = fsrs();
    let mut rng = SplitMix64::new(seed);
    let mut state = SchedulingState::NEW;
    let mut now = T0;
    let end = T0.plus_days(365);
    let mut reviews = 0;
    let mut lapses = 0;
    while now < end {
        let rating = rating_of(&mut rng);
        let out = s.schedule(&state, rating, now, p).unwrap();
        // Memory = reference formulas applied to the previous state.
        let prev = match (state.state, state.stability, state.difficulty) {
            (CardState::New, _, _) => None,
            (_, Some(st), Some(d)) => Some((st, d)),
            _ => None,
        };
        let t = f64::from(elapsed_days(&state, now));
        let (rs, rd) = ref_step(prev, g(rating), t);
        let (gs, gd) = (out.state.stability.unwrap(), out.state.difficulty.unwrap());
        assert!(
            close(gs, rs, 1e-4),
            "stability {gs} vs reference {rs} (review {reviews})"
        );
        assert!(
            close(gd, rd, 1e-4),
            "difficulty {gd} vs reference {rd} (review {reviews})"
        );
        // Whole-day intervals = reference intervals through the same rounding/ordering policy
        // (exact unless an interval sits on a rounding edge, where f32 may round the other way).
        if out.scheduled_days > 0 {
            let cands = RATINGS.map(|r| {
                let (s1, d1) = ref_step(prev, g(r), t);
                Candidate {
                    stability: s1,
                    difficulty: d1,
                    interval_days: ref_interval(s1, p.desired_retention),
                }
            });
            let expected = day_intervals(
                &cands,
                p.maximum_interval_days,
                state.state == CardState::Review,
            )[rating_index(rating)];
            let edge = cands
                .iter()
                .any(|c| (c.interval_days.fract() - 0.5).abs() < 1e-3);
            assert!(
                out.scheduled_days == expected || edge,
                "interval {} vs reference {expected} (review {reviews})",
                out.scheduled_days
            );
        }
        // Invariants.
        let due = out.state.due.unwrap();
        assert!(due > now, "due must move forward");
        assert!(out.scheduled_days <= p.maximum_interval_days);
        assert!((1.0..=10.0).contains(&gd));
        assert_eq!(out.state.last_review, Some(now));
        if state.state == CardState::Review && rating == Rating::Again {
            lapses += 1;
        }
        assert_eq!(out.state.lapses, lapses);
        state = out.state;
        now = due;
        reviews += 1;
    }
    reviews
}

#[test]
fn fsrs_365_day_simulation_matches_reference_without_steps() {
    let n = simulate_365(&params(&[], &[]), 7);
    assert!(n >= 5, "only {n} reviews");
}

#[test]
fn fsrs_365_day_simulation_matches_reference_with_steps() {
    for seed in [1, 2, 3, 99] {
        let n = simulate_365(&params(&[1, 10], &[10]), seed);
        assert!(n >= 5, "only {n} reviews");
    }
}

#[test]
fn fsrs_first_review_reference_values() {
    let p = params(&[], &[]);
    let out = fsrs().preview(&SchedulingState::NEW, T0, &p).unwrap();
    // S0 = w[rating-1]; D0(G) = w4 − e^(w5·(G−1)) + 1; at 90 % retention I(S) = S.
    let s: Vec<f64> = out.iter().map(|o| o.state.stability.unwrap()).collect();
    assert!(close(s[0], 0.212, 1e-6) && close(s[1], 1.2931, 1e-6));
    assert!(close(s[2], 2.3065, 1e-6) && close(s[3], 8.2956, 1e-6));
    let d_good = out[2].state.difficulty.unwrap();
    assert!(
        close(d_good, 6.4133 - (0.8334f64 * 2.0).exp() + 1.0, 1e-5),
        "{d_good}"
    );
    // Without learning steps every button graduates: 1 (again, floor), 1 (hard),
    // 2 (good: round(2.3065)), 8 (easy: round(8.2956)).
    let days: Vec<u32> = out.iter().map(|o| o.scheduled_days).collect();
    assert_eq!(days, vec![1, 1, 2, 8]);
    assert!(out
        .iter()
        .all(|o| o.state.state == CardState::Review && o.elapsed_days == 0));
    assert_eq!(out[0].state.reps, 0);
    assert_eq!(out[2].state.reps, 1);
}

#[test]
fn learning_steps_state_machine() {
    let p = params(&[1, 10], &[10]);
    let s = fsrs();
    // New: again → step 1 (1 min); hard → avg of steps (5 min); good → step 2 (10 min).
    let new = SchedulingState::NEW;
    let [again, hard, good, easy] = s.preview(&new, T0, &p).unwrap();
    assert_eq!(again.state.state, CardState::Learning);
    assert_eq!(again.state.due, Some(T0.plus_millis(MIN)));
    assert_eq!(hard.state.due, Some(T0.plus_millis(5 * MIN)));
    assert_eq!(good.state.due, Some(T0.plus_millis(10 * MIN)));
    assert_eq!(easy.state.state, CardState::Review);
    assert!(easy.scheduled_days >= 1);

    // On the 10-minute step, good graduates; hard repeats the 10-minute step.
    let at = T0.plus_millis(10 * MIN);
    let [_, hard2, good2, _] = s.preview(&good.state, at, &p).unwrap();
    assert_eq!(hard2.state.state, CardState::Learning);
    assert_eq!(hard2.state.due, Some(at.plus_millis(10 * MIN)));
    assert_eq!(good2.state.state, CardState::Review);
    assert!(good2.scheduled_days >= 1);

    // Review lapse → relearning (10 min), lapses + 1; good after relearning → review.
    let review = good2.state;
    let later = review.due.unwrap();
    let lapse = s.schedule(&review, Rating::Again, later, &p).unwrap();
    assert_eq!(lapse.state.state, CardState::Relearning);
    assert_eq!(lapse.state.lapses, 1);
    assert_eq!(lapse.state.due, Some(later.plus_millis(10 * MIN)));
    let back = s
        .schedule(&lapse.state, Rating::Good, later.plus_millis(10 * MIN), &p)
        .unwrap();
    assert_eq!(back.state.state, CardState::Review);
    assert_eq!(back.state.lapses, 1);

    // Without relearning steps a lapse stays in review with a whole-day interval.
    let p2 = params(&[1, 10], &[]);
    let lapse2 = s.schedule(&review, Rating::Again, later, &p2).unwrap();
    assert_eq!(lapse2.state.state, CardState::Review);
    assert!(lapse2.scheduled_days >= 1);
}

#[test]
fn review_intervals_are_ordered_and_capped() {
    let mut p = params(&[], &[]);
    let s = fsrs();
    let first = s
        .schedule(&SchedulingState::NEW, Rating::Good, T0, &p)
        .unwrap()
        .state;
    let at = first.due.unwrap();
    let out = s.preview(&first, at, &p).unwrap();
    let d: Vec<u32> = out.iter().map(|o| o.scheduled_days).collect();
    assert!(d[1] <= d[2] && d[2] < d[3], "{d:?}");
    p.maximum_interval_days = 3;
    let capped = s.preview(&first, at, &p).unwrap();
    assert!(capped.iter().all(|o| o.scheduled_days <= 3));
}

#[test]
fn preview_equals_schedule_and_is_deterministic() {
    let p = params(&[1, 10], &[10]);
    for sched in [fsrs(), sm2()] {
        let mut state = SchedulingState::NEW;
        let mut now = T0;
        for r in [
            Rating::Good,
            Rating::Good,
            Rating::Easy,
            Rating::Again,
            Rating::Good,
        ] {
            let preview = sched.preview(&state, now, &p).unwrap();
            let out = sched.schedule(&state, r, now, &p).unwrap();
            assert_eq!(preview[rating_index(r)], out);
            assert_eq!(sched.schedule(&state, r, now, &p).unwrap(), out);
            state = out.state;
            now = state.due.unwrap();
        }
    }
}

#[test]
fn invalid_params_are_rejected() {
    let mut p = params(&[1], &[1]);
    p.desired_retention = 1.0;
    assert!(fsrs()
        .schedule(&SchedulingState::NEW, Rating::Good, T0, &p)
        .is_err());
    assert!(sm2()
        .schedule(&SchedulingState::NEW, Rating::Good, T0, &p)
        .is_err());
    let mut p = params(&[0], &[]);
    assert!(fsrs()
        .schedule(&SchedulingState::NEW, Rating::Good, T0, &p)
        .is_err());
    p.learning_steps_mins = vec![1];
    p.extra = serde_json::json!({ "weights": [1.0, 2.0] });
    assert!(fsrs()
        .schedule(&SchedulingState::NEW, Rating::Good, T0, &p)
        .is_err());
    p.extra = serde_json::json!({ "weights": FSRS6_DEFAULT_WEIGHTS });
    assert!(fsrs()
        .schedule(&SchedulingState::NEW, Rating::Good, T0, &p)
        .is_ok());
}

#[test]
fn sm2_progression() {
    let p = params(&[], &[]);
    let s = sm2();
    let first = s
        .schedule(&SchedulingState::NEW, Rating::Good, T0, &p)
        .unwrap();
    assert_eq!(first.scheduled_days, 1);
    assert_eq!(first.state.difficulty, Some(2.5));
    let at = first.state.due.unwrap();
    let second = s.schedule(&first.state, Rating::Good, at, &p).unwrap();
    // 1 day × ease 2.5 = 2.5 → 3 (rounded).
    assert_eq!(second.scheduled_days, 3);
    let at = second.state.due.unwrap();
    let easy = s.schedule(&second.state, Rating::Easy, at, &p).unwrap();
    assert_eq!(easy.state.difficulty, Some(2.65));
    let lapse = s.schedule(&second.state, Rating::Again, at, &p).unwrap();
    assert_eq!(lapse.state.difficulty, Some(2.3));
    assert_eq!(lapse.state.lapses, 1);
    // Ease never drops below 1.3.
    let mut st = second.state;
    for _ in 0..20 {
        st = s.schedule(&st, Rating::Again, at, &p).unwrap().state;
        st.state = CardState::Review;
    }
    assert!(st.difficulty.unwrap() >= MIN_EASE_FOR_TEST);
}

const MIN_EASE_FOR_TEST: f64 = 1.3;

#[test]
fn elapsed_days_floor_and_new() {
    assert_eq!(elapsed_days(&SchedulingState::NEW, T0), 0);
    let st = SchedulingState {
        last_review: Some(T0),
        ..SchedulingState::NEW
    };
    assert_eq!(elapsed_days(&st, T0.plus_millis(DAY * 3 - 1)), 2);
    assert_eq!(elapsed_days(&st, T0.plus_millis(-DAY)), 0);
}

#[test]
fn builtins_are_listed() {
    for id in BUILTIN_IDS {
        assert_eq!(builtin(id).unwrap().info().id, id);
    }
    assert!(builtin("x").is_none());
}

#[test]
fn retrievability_follows_the_forgetting_curve() {
    // R(S, S) = 0.9 by construction; R(0) = 1; decreasing in time.
    assert!(close(retrievability(10.0, 10.0).unwrap(), 0.9, 1e-9));
    assert!(close(retrievability(10.0, 0.0).unwrap(), 1.0, 1e-12));
    assert!(retrievability(10.0, 30.0).unwrap() < retrievability(10.0, 20.0).unwrap());
    assert!(close(
        retrievability(5.0, 7.0).unwrap(),
        ref_retrievability(7.0, 5.0),
        1e-9
    ));
    assert_eq!(retrievability(0.0, 1.0), None);
}
