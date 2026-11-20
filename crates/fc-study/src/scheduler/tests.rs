#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use fc_core::rng::{Rng, SplitMix64};
use fc_core::time::MILLIS_PER_DAY;

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
        let t = f64::from(elapsed_days(&state, now, StudyDay::default()));
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
    // Every answer counts as a repetition, Again included (Anki/FSRS).
    assert_eq!(out[0].state.reps, 1);
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
fn elapsed_days_counts_study_days() {
    let utc = StudyDay::default(); // new day at 04:00Z; T0 = 00:00Z belongs to Dec 31
    assert_eq!(elapsed_days(&SchedulingState::NEW, T0, utc), 0);
    let st = SchedulingState {
        last_review: Some(T0),
        ..SchedulingState::NEW
    };
    // 00:00 → 03:59 same day; 04:00 is the next study day though only 4 h passed.
    assert_eq!(elapsed_days(&st, T0.plus_millis(4 * HOUR - 1), utc), 0);
    assert_eq!(elapsed_days(&st, T0.plus_millis(4 * HOUR), utc), 1);
    assert_eq!(elapsed_days(&st, T0.plus_millis(DAY * 3 - 1), utc), 3);
    assert_eq!(elapsed_days(&st, T0.plus_millis(-DAY), utc), 0);
}

#[test]
fn study_day_travels_in_params() {
    let p = with_study_day(params(&[], &[]), StudyDay::new(180, 5));
    assert_eq!(study_day(&p).unwrap(), StudyDay::new(180, 5));
    assert_eq!(study_day(&params(&[], &[])).unwrap(), StudyDay::default());
    // Other extra keys survive.
    let mut w = params(&[], &[]);
    w.extra = serde_json::json!({ "weights": FSRS6_DEFAULT_WEIGHTS });
    let w = with_study_day(w, StudyDay::new(-300, 0));
    assert!(w.extra.get("weights").is_some());
    let mut bad = params(&[], &[]);
    bad.extra = serde_json::json!({ "day": { "tz_offset_mins": 5000, "rollover_hour": 4 } });
    assert!(fsrs()
        .schedule(&SchedulingState::NEW, Rating::Good, T0, &bad)
        .is_err());
    bad.extra = serde_json::json!({ "day": { "tz_offset_mins": 0, "rollover_hour": 24 } });
    assert!(sm2()
        .schedule(&SchedulingState::NEW, Rating::Good, T0, &bad)
        .is_err());
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
    let p = params(&[], &[]);
    assert!(close(retrievability(10.0, 10.0, &p).unwrap(), 0.9, 1e-9));
    assert!(close(retrievability(10.0, 0.0, &p).unwrap(), 1.0, 1e-12));
    assert!(retrievability(10.0, 30.0, &p).unwrap() < retrievability(10.0, 20.0, &p).unwrap());
    assert!(close(
        retrievability(5.0, 7.0, &p).unwrap(),
        ref_retrievability(7.0, 5.0),
        1e-9
    ));
    assert_eq!(retrievability(0.0, 1.0, &p), None);
}

#[test]
fn retrievability_uses_the_presets_decay() {
    // Custom weights with a different w20: R(S, S) is still 0.9, but elsewhere the curve
    // follows the custom decay, not the default one.
    let mut w = FSRS6_DEFAULT_WEIGHTS;
    w[20] = 0.5;
    let mut p = params(&[], &[]);
    p.extra = serde_json::json!({ "weights": w });
    let decay = -f64::from(w[20]);
    let factor = 0.9f64.powf(1.0 / decay) - 1.0;
    let expected = (1.0 + factor * 30.0 / 10.0).powf(decay);
    let got = retrievability(10.0, 30.0, &p).unwrap();
    assert!(close(got, expected, 1e-6), "{got} vs {expected}");
    let default = retrievability(10.0, 30.0, &params(&[], &[])).unwrap();
    assert!((got - default).abs() > 1e-3);
    assert!(close(retrievability(10.0, 10.0, &p).unwrap(), 0.9, 1e-6));
}

// ------------------------------------------------------------------ study days
//
// An independent calendar: UTC instant → local civil date/time (Hinnant's civil algorithms),
// minus one date before the rollover hour. Shares no code with `crate::day`.

const HOUR: i64 = 3_600_000;

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

/// The local calendar date (y, m, d) a study day belongs to.
fn ref_study_date(t: Timestamp, tz_mins: i64, rollover: i64) -> (i64, i64, i64) {
    let local = t.as_millis() + tz_mins * 60_000;
    let civil_day = local.div_euclid(DAY);
    let hour = local.rem_euclid(DAY) / HOUR;
    civil_from_days(if hour < rollover {
        civil_day - 1
    } else {
        civil_day
    })
}

fn ref_days_between(a: Timestamp, b: Timestamp, tz: i64, roll: i64) -> i64 {
    let (y1, m1, d1) = ref_study_date(a, tz, roll);
    let (y2, m2, d2) = ref_study_date(b, tz, roll);
    days_from_civil(y2, m2, d2) - days_from_civil(y1, m1, d1)
}

/// UTC instant of local `rollover:00` on the civil date `date`.
fn ref_rollover_instant(date: (i64, i64, i64), tz: i64, roll: i64) -> Timestamp {
    Timestamp(days_from_civil(date.0, date.1, date.2) * DAY + roll * HOUR - tz * 60_000)
}

/// A random instant of the study day starting at `start`, biased to the edges: right after
/// the rollover (early morning), just before the next one (late night, past local midnight),
/// or anywhere in between.
fn random_time_in_day(rng: &mut SplitMix64, start: Timestamp) -> Timestamp {
    let span = DAY as u64;
    let offset = match rng.below(4) {
        0 => rng.below(10 * MIN as u64),
        1 => span - 1 - rng.below(10 * MIN as u64),
        _ => rng.below(span),
    };
    start.plus_millis(offset as i64)
}

/// Reviews one card for two years at random times of its due study day (sometimes a few days
/// late) in the calendar `(tz, roll)`. Every elapsed day count, interval, due date and memory
/// update must match the independent day-based computation, and the outcome must not depend
/// on the time of day.
fn simulate_random_times(tz: i32, roll: u8, seed: u64) -> usize {
    let s = fsrs();
    let p = with_study_day(params(&[], &[]), StudyDay::new(tz, roll));
    let (tzi, rolli) = (i64::from(tz), i64::from(roll));
    let mut rng = SplitMix64::new(seed);
    let mut state = SchedulingState::NEW;
    let mut now = random_time_in_day(
        &mut rng,
        ref_rollover_instant(ref_study_date(T0, tzi, rolli), tzi, rolli),
    );
    let end = T0.plus_days(730);
    let mut reviews = 0;
    let mut long_term = 0;
    while now < end {
        let rating = rating_of(&mut rng);
        let out = s.schedule(&state, rating, now, &p).unwrap();

        // Elapsed = calendar days of the independent calendar.
        let t = state
            .last_review
            .map_or(0, |last| ref_days_between(last, now, tzi, rolli));
        assert_eq!(
            i64::from(out.elapsed_days),
            t,
            "elapsed (tz {tz}, roll {roll}, review {reviews}, now {now:?})"
        );
        if t > 0 {
            long_term += 1;
        }

        // Memory and interval = reference formulas with that elapsed count.
        let prev = match (state.state, state.stability, state.difficulty) {
            (CardState::New, _, _) => None,
            (_, Some(st), Some(d)) => Some((st, d)),
            _ => None,
        };
        let (rs, rd) = ref_step(prev, g(rating), t as f64);
        let (gs, gd) = (out.state.stability.unwrap(), out.state.difficulty.unwrap());
        assert!(
            close(gs, rs, 1e-4),
            "stability {gs} vs {rs} (review {reviews})"
        );
        assert!(
            close(gd, rd, 1e-4),
            "difficulty {gd} vs {rd} (review {reviews})"
        );
        let cands = RATINGS.map(|r| {
            let (s1, d1) = ref_step(prev, g(r), t as f64);
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
            "interval {} vs {expected} (review {reviews})",
            out.scheduled_days
        );

        // Due = local rollover of (today + interval), independent calendar.
        let today = ref_study_date(now, tzi, rolli);
        let due_date = civil_from_days(
            days_from_civil(today.0, today.1, today.2) + i64::from(out.scheduled_days),
        );
        let due = out.state.due.unwrap();
        assert_eq!(due, ref_rollover_instant(due_date, tzi, rolli));
        assert_eq!(ref_study_date(due, tzi, rolli), due_date);

        // Same study day, other time (its first and last millisecond): same outcome.
        let day_start = ref_rollover_instant(today, tzi, rolli);
        for other in [day_start, day_start.plus_millis(DAY - 1)] {
            assert_eq!(
                s.schedule(&state, rating, other, &p).unwrap().state.due,
                Some(due)
            );
            let o = s.schedule(&state, rating, other, &p).unwrap();
            assert_eq!(
                (o.scheduled_days, o.elapsed_days, o.state.stability),
                (out.scheduled_days, out.elapsed_days, out.state.stability)
            );
        }

        state = out.state;
        // Next answer: a random time of the due day, sometimes 1–3 days late.
        let late = if rng.below(5) == 0 {
            rng.below(3) + 1
        } else {
            0
        };
        now = random_time_in_day(&mut rng, due.plus_days(late as i64));
        reviews += 1;
    }
    assert!(long_term >= 3, "only {long_term} long-term reviews");
    reviews
}

#[test]
fn fsrs_random_review_times_follow_study_days() {
    // (UTC offset mins, rollover hour): UTC, Istanbul, New York, India, Kiribati, Baker Island,
    // midnight and late rollovers.
    let zones = [
        (0, 4),
        (180, 4),
        (-300, 4),
        (330, 0),
        (840, 23),
        (-720, 12),
        (-600, 0),
        (345, 6),
    ];
    let mut total = 0;
    for (i, (tz, roll)) in zones.into_iter().enumerate() {
        for seed in 0..4 {
            total += simulate_random_times(tz, roll, seed * 31 + i as u64);
        }
    }
    assert!(total >= 200, "only {total} reviews");
}

#[test]
fn next_morning_review_is_one_elapsed_day() {
    // The reviewer's case: graduated at 15:00 with a 1-day interval, answered next day 09:00
    // (Istanbul, rollover 04:00). Elapsed is 1, so FSRS uses the long-term stability formula,
    // not the same-day one.
    let p = with_study_day(params(&[], &[]), StudyDay::new(180, 4));
    let s = fsrs();
    let at_15 = T0.plus_millis(12 * HOUR); // 15:00 local
    let first = s
        .schedule(&SchedulingState::NEW, Rating::Hard, at_15, &p)
        .unwrap();
    assert_eq!(first.scheduled_days, 1);
    // Due at the next study day's start (04:00 local = 01:00Z), not 24 h later.
    assert_eq!(first.state.due, Some(T0.plus_millis(25 * HOUR)));
    let next_9 = T0.plus_millis(30 * HOUR); // 09:00 local the next day, 18 h later
    let second = s.schedule(&first.state, Rating::Good, next_9, &p).unwrap();
    assert_eq!(second.elapsed_days, 1);
    let prev = (
        first.state.stability.unwrap(),
        first.state.difficulty.unwrap(),
    );
    let (long, _) = ref_step(Some(prev), 3.0, 1.0);
    let (same_day, _) = ref_step(Some(prev), 3.0, 0.0);
    assert!(close(second.state.stability.unwrap(), long, 1e-4));
    assert!((long - same_day).abs() > 0.1);
}
