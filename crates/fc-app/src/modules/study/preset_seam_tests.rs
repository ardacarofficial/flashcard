//! v1.4.1 (H1): the seam between a saved preset and the scheduler. "Any preset that validates
//! can schedule any card state": options are generated across their whole valid ranges (FSRS
//! weights of every accepted version, at and inside the model's bounds), card states are
//! anything a row can hold (including odd ones: missing or out-of-range memory, a last review
//! in the future), and every rating must give a finite, in-range outcome with both schedulers.
//!
//! Bounded for the default suite (256 cases); `FC_SOAK=1` runs 20 000.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::params_from_preset;
use fc_core::card::{CardState, Rating, SchedulingState};
use fc_core::preset::{PresetConfig, FSRS6_WEIGHT_BOUNDS, MAX_INTERVAL_DAYS, MAX_STEP_MINS};
use fc_core::time::MILLIS_PER_DAY;
use fc_core::Timestamp;
use fc_study::scheduler::{builtin, FSRS_ID, SM2_ID};
use proptest::prelude::*;

const NOW: Timestamp = Timestamp(1_767_225_600_000);

fn bound(lo: f32, hi: f32) -> impl Strategy<Value = f64> {
    let (lo, hi) = (f64::from(lo), f64::from(hi));
    prop_oneof![1 => Just(lo), 1 => Just(hi), 6 => lo..=hi]
}

fn fsrs6_weights() -> impl Strategy<Value = Vec<f64>> {
    FSRS6_WEIGHT_BOUNDS
        .iter()
        .map(|(lo, hi)| bound(*lo, *hi).boxed())
        .collect::<Vec<_>>()
}

fn weights() -> impl Strategy<Value = Vec<f64>> {
    prop_oneof![
        1 => Just(Vec::new()),
        4 => fsrs6_weights(),
        // FSRS-5: the first 19 of an FSRS-6 set.
        2 => fsrs6_weights().prop_map(|w| w[..19].to_vec()),
        // FSRS-4.5: its upgrade moves w4..w6, so keep those where it lands inside.
        2 => (fsrs6_weights(), 1.0f64..6.0, 0.001f64..2.0, 0.001f64..3.5).prop_map(
            |(w, w4, w5, w6)| {
                let mut w = w[..17].to_vec();
                (w[4], w[5], w[6]) = (w4, w5, w6);
                w
            }
        ),
    ]
}

fn steps() -> impl Strategy<Value = Vec<u32>> {
    prop::collection::vec(prop_oneof![Just(1), Just(MAX_STEP_MINS), 1..=MAX_STEP_MINS], 0..=20)
}

fn preset() -> impl Strategy<Value = PresetConfig> {
    (
        steps(),
        steps(),
        prop_oneof![Just(0.70), Just(0.99), 0.70f64..=0.99],
        prop_oneof![Just(1), Just(MAX_INTERVAL_DAYS), 1..=MAX_INTERVAL_DAYS],
        weights(),
    )
        .prop_map(|(learning, relearning, retention, max, w)| PresetConfig {
            learning_steps_mins: learning,
            relearning_steps_mins: relearning,
            desired_retention: retention,
            maximum_interval_days: max,
            fsrs_weights: w,
            ..PresetConfig::default()
        })
}

fn card() -> impl Strategy<Value = SchedulingState> {
    let day = MILLIS_PER_DAY;
    let time = prop_oneof![
        Just(None),
        (-40_000i64 * day..10 * day).prop_map(move |o| Some(Timestamp(NOW.0 + o))),
    ];
    (
        prop_oneof![
            Just(CardState::New),
            Just(CardState::Learning),
            Just(CardState::Review),
            Just(CardState::Relearning)
        ],
        time.clone(),
        prop_oneof![Just(None), Just(Some(0.0)), (-10.0f64..1e6).prop_map(Some)],
        prop_oneof![Just(None), (-50.0f64..50.0).prop_map(Some)],
        any::<u32>(),
        any::<u32>(),
        time,
    )
        .prop_map(|(state, due, stability, difficulty, reps, lapses, last)| SchedulingState {
            state,
            due,
            stability,
            difficulty,
            reps,
            lapses,
            last_review: last,
        })
}

fn cases() -> u32 {
    if std::env::var_os("FC_SOAK").is_some() {
        20_000
    } else {
        256
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: cases(),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn any_preset_that_validates_schedules_any_card_state(
        stored in preset(),
        state in card(),
    ) {
        prop_assume!(stored.validate().is_ok());
        // As it comes back from storage (17/19 weights are upgraded on read).
        let cfg: PresetConfig =
            serde_json::from_value(serde_json::to_value(&stored).unwrap()).unwrap();
        prop_assert!(cfg.validate().is_ok(), "{:?}", cfg.validate());
        prop_assert!(cfg.fsrs_weights.is_empty() || cfg.fsrs_weights.len() == 21);
        let params = params_from_preset(&cfg);
        for id in [FSRS_ID, SM2_ID] {
            let s = builtin(id).unwrap();
            let preview = s.preview(&state, NOW, &params);
            prop_assert!(preview.is_ok(), "{id} preview: {:?}", preview.err());
            for rating in [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy] {
                let out = s.schedule(&state, rating, NOW, &params);
                prop_assert!(out.is_ok(), "{id} {rating:?}: {:?}", out.err());
                let out = out.unwrap();
                prop_assert!(out.scheduled_days <= cfg.maximum_interval_days);
                let due = out.state.due.expect("a graded card is due again");
                prop_assert!(due.0 >= NOW.0, "{id} {rating:?}: due before now");
                if let Some(st) = out.state.stability {
                    prop_assert!(st.is_finite() && st > 0.0, "{id} stability {st}");
                }
                if let Some(d) = out.state.difficulty {
                    prop_assert!(d.is_finite(), "{id} difficulty {d}");
                }
            }
        }
    }
}
