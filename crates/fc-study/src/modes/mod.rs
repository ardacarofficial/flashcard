//! v0.10 study modes (docs/02 §6.1, §27): Quizlet-style learn mode, test generator,
//! confidence-based rating, timed mode. Pure: the app (`fc-app` `modes` modules) persists the
//! state below inside the v0.2 session state ([`crate::session::SessionState::modes`]) and runs
//! every answer through the ordinary session engine.
//!
//! * [`learn`]: per-card stages (multiple choice → written); a card is **learned** once it was
//!   answered correctly at every stage.
//! * [`test`]: a test paper with mixed question types (multiple choice, written, true/false)
//!   from a set of cards, and its scoring.
//! * [`confidence`]: the 1–5 "how sure am I?" scale and its mapping onto the four ratings.
//! * [`timed`]: the per-card time limit and what happens when it runs out.
//! * [`matching`] (v1.1): match terms with definitions against the clock.

pub mod confidence;
pub mod learn;
pub mod matching;
pub mod test;
pub mod timed;

use crate::answer::{normalize, CheckOptions};
use fc_core::rng::Rng;
use serde::{Deserialize, Serialize};

/// Mode state persisted with a session (`sessions.state.modes`; absent for plain sessions).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModesState {
    /// Learn sessions: per-card progress.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learn: Option<learn::LearnState>,
    /// Test sessions: the paper (and its result once submitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<test::TestState>,
    /// Timed sessions: cards whose time ran out.
    pub timed_out: u32,
    /// Timed sessions: the countdown of the card on screen, started when the screen put it up
    /// (`modes.timed.start`). Kept per card: a different card on screen, or an undo, means
    /// its countdown has not started yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timer: Option<timed::CardTimer>,
    /// Match sessions (v1.1): the cards and the current round.
    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    pub matching: Option<matching::MatchState>,
}

/// Mode options persisted with a session (`sessions.config.modes`; absent for plain
/// sessions).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModesConfig {
    /// Time limit per card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timed: Option<timed::Timed>,
    /// Cards are rated on the confidence scale.
    pub confidence: bool,
}

/// Comparison key of an answer text (options must be distinct by this key).
pub fn option_key(text: &str) -> String {
    normalize(text, &CheckOptions::default())
}

/// Fisher–Yates shuffle with a seeded generator.
pub fn shuffle<T>(items: &mut [T], rng: &mut dyn Rng) {
    for i in (1..items.len()).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        items.swap(i, j);
    }
}

/// Multiple-choice options for `correct`: up to `max - 1` distractors drawn from `pool`
/// (texts equal to the correct answer or to each other — by [`option_key`] — are skipped; empty
/// texts too), shuffled. Returns the options and the index of the correct one, or `None` when
/// the pool has no usable distractor (a one-option question is not a question).
pub fn choice_options(
    correct: &str,
    pool: &[String],
    max: usize,
    rng: &mut dyn Rng,
) -> Option<(Vec<String>, u32)> {
    let correct_key = option_key(correct);
    let mut seen = std::collections::HashSet::new();
    seen.insert(correct_key);
    let mut candidates: Vec<&String> = Vec::new();
    for p in pool {
        let key = option_key(p);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        candidates.push(p);
    }
    if candidates.is_empty() || max < 2 {
        return None;
    }
    shuffle(&mut candidates, rng);
    candidates.truncate(max - 1);
    let mut options: Vec<String> = candidates.into_iter().cloned().collect();
    options.push(correct.to_owned());
    shuffle(&mut options, rng);
    let index = options
        .iter()
        .position(|o| o.as_str() == correct)
        .and_then(|i| u32::try_from(i).ok())?;
    Some((options, index))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use fc_core::rng::SplitMix64;

    #[test]
    fn options_are_distinct_and_contain_the_answer_once() {
        let pool: Vec<String> = ["kedi", "Köpek", "köpek ", "", "kuş", "elma", "at"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        for seed in 0..50 {
            let mut rng = SplitMix64::new(seed);
            let (opts, i) = choice_options("elma", &pool, 4, &mut rng).unwrap();
            assert_eq!(opts.len(), 4);
            assert_eq!(opts[i as usize], "elma");
            let keys: std::collections::HashSet<_> = opts.iter().map(|o| option_key(o)).collect();
            assert_eq!(keys.len(), 4, "distinct: {opts:?}");
            assert!(!opts.iter().any(String::is_empty));
        }
        // No distractor → no question.
        let mut rng = SplitMix64::new(1);
        assert!(choice_options("elma", &["Elma".into(), String::new()], 4, &mut rng).is_none());
        // Deterministic per seed.
        let run = |s| choice_options("elma", &pool, 3, &mut SplitMix64::new(s));
        assert_eq!(run(9), run(9));
    }
}
