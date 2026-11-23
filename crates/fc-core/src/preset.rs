//! Deck option presets (docs/02 §10 "Deste ayar şablonları"): named study option sets shared
//! by decks. Edited by the `presets` module (v0.4); **read by the study engine (v0.2)** through
//! the effective config of a deck (`fc_app::modules::presets::effective`), so field names are
//! deliberately generic. The schema is documented in docs/10 §16.1.

use crate::id::PresetId;
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};

/// A named set of study options shared by decks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// Id.
    pub id: PresetId,
    /// Display name (user text).
    pub name: String,
    /// Options, stored as JSON in `presets.config`.
    pub config: PresetConfig,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// Order in which new cards are introduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum NewCardOrder {
    /// In the order they were added.
    #[default]
    Added,
    /// Shuffled.
    Random,
}

/// Order in which due reviews are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ReviewOrder {
    /// Earliest due first.
    #[default]
    Due,
    /// Shuffled.
    Random,
    /// Lowest recall probability first.
    Retrievability,
}

/// Preset options. Unknown keys are preserved in `extra` so newer versions can add options
/// without a migration; missing keys take their default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PresetConfig {
    /// New cards introduced per day (0–9999).
    pub new_per_day: u32,
    /// Maximum reviews per day (0–99999).
    pub reviews_per_day: u32,
    /// Learning steps in minutes (each 1–525600, at most 20; empty = straight to review).
    pub learning_steps_mins: Vec<u32>,
    /// Relearning steps in minutes (same rules).
    pub relearning_steps_mins: Vec<u32>,
    /// Target probability of recall (FSRS), 0.70–0.99.
    pub desired_retention: f64,
    /// Longest interval a card can get, in days (1–36500).
    pub maximum_interval_days: u32,
    /// FSRS parameters; empty = the algorithm's defaults. Otherwise 21 (FSRS-6) finite
    /// numbers within [`FSRS6_WEIGHT_BOUNDS`], as produced by the optimizer (v0.6). 17
    /// (FSRS-4.5) and 19 (FSRS-5) values are accepted on input and upgraded to FSRS-6 as they
    /// are read ([`upgrade_fsrs_weights`], v1.4.1): every reader sees 21 or none.
    #[serde(deserialize_with = "de_fsrs_weights")]
    pub fsrs_weights: Vec<f64>,
    /// Hide a card's siblings (other cards of the same note) until the next day once one of
    /// them was shown.
    pub bury_siblings: bool,
    /// New card order.
    pub new_order: NewCardOrder,
    /// Review order.
    pub review_order: ReviewOrder,
    /// Options this version does not know about.
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for PresetConfig {
    fn default() -> Self {
        Self {
            new_per_day: 20,
            reviews_per_day: 200,
            learning_steps_mins: vec![1, 10],
            relearning_steps_mins: vec![10],
            desired_retention: 0.9,
            maximum_interval_days: MAX_INTERVAL_DAYS,
            fsrs_weights: Vec::new(),
            // docs/02 §3: siblings are not shown on the same day unless turned off.
            bury_siblings: true,
            new_order: NewCardOrder::Added,
            review_order: ReviewOrder::Due,
            extra: serde_json::Map::new(),
        }
    }
}

/// Upper bound of `new_per_day`.
pub const MAX_NEW_PER_DAY: u32 = 9_999;
/// Upper bound of `reviews_per_day`.
pub const MAX_REVIEWS_PER_DAY: u32 = 99_999;
/// Longest step (one year, minutes).
pub const MAX_STEP_MINS: u32 = 525_600;
/// Most steps in one list.
pub const MAX_STEPS: usize = 20;
/// Upper bound (and default) of `maximum_interval_days`: 100 years.
pub const MAX_INTERVAL_DAYS: u32 = 36_500;
/// Allowed `desired_retention` range.
pub const RETENTION_RANGE: std::ops::RangeInclusive<f64> = 0.70..=0.99;
/// Allowed `fsrs_weights` lengths on input (besides empty); stored and read: 21.
pub const FSRS_WEIGHT_COUNTS: [usize; 3] = [17, 19, 21];
/// Number of FSRS-6 weights.
pub const FSRS6_WEIGHT_COUNT: usize = 21;
/// Decay (`w20`) the FSRS-4.5 / FSRS-5 upgrade fills in: the fixed decay of those versions
/// (`fsrs::FSRS5_DEFAULT_DECAY`).
pub const FSRS5_UPGRADE_DECAY: f64 = 0.5;

/// Range of each FSRS-6 weight, inclusive, in the scheduler's `f32` — the reference
/// optimizer's clipper (`fsrs-rs` 7 `clip_fsrs6_parameters`) in its widest form: `w17`/`w18`
/// at most 2.0 (the clipper may lower it further for many relearning steps) and `w19` at
/// least 0 (0.01 when short-term training is on). Every weight set the optimizer produces is
/// inside; inside, the model's memory states stay finite for every card state, so a preset
/// that validates always schedules.
pub const FSRS6_WEIGHT_BOUNDS: [(f32, f32); FSRS6_WEIGHT_COUNT] = [
    (0.0001, 100.0),
    (0.0001, 100.0),
    (0.0001, 100.0),
    (0.0001, 100.0),
    (1.0, 10.0),
    (0.001, 4.0),
    (0.001, 4.0),
    (0.001, 0.75),
    (0.0, 4.5),
    (0.0, 0.8),
    (0.001, 3.5),
    (0.001, 5.0),
    (0.001, 0.25),
    (0.001, 0.9),
    (0.0, 4.0),
    (0.0, 1.0),
    (1.0, 6.0),
    (0.0, 2.0),
    (0.0, 2.0),
    (0.0, 0.8),
    (0.1, 0.8),
];

/// FSRS weights of any accepted version as FSRS-6 weights — **the one conversion** (v1.4.1),
/// exactly the upgrade the reference implementation applies (`fsrs-rs` 7
/// `check_and_fill_parameters`, which Anki uses):
/// * empty → empty (the scheduler's defaults);
/// * 17 (FSRS-4.5) → `w4 += 2·w5`, `w5 = ln(1 + 3·w5) / 3`, `w6 += 0.5`, then `w17 = w18 =
///   w19 = 0` and `w20 =` [`FSRS5_UPGRADE_DECAY`];
/// * 19 (FSRS-5) → `w19 = 0`, `w20 =` [`FSRS5_UPGRADE_DECAY`];
/// * 21 → unchanged; any other length → `None`.
///
/// How close this is to the old models (docs/10 §16.1 `fsrs_weights`): FSRS-5 → FSRS-6 is exact (`w19 = 0`
/// removes the stability term of same-day reviews, `w20 = 0.5` is FSRS-5's curve) except that
/// FSRS-6 never lowers stability on a same-day Good/Easy; FSRS-4.5 → FSRS-6 keeps its curve and
/// its initial difficulty for Again and Easy, while Hard/Good start and later difficulty
/// updates follow FSRS-6 — an approximation, the same one Anki makes.
pub fn upgrade_fsrs_weights(w: &[f64]) -> Option<Vec<f64>> {
    let mut out = w.to_vec();
    match w.len() {
        0 | FSRS6_WEIGHT_COUNT => {}
        17 => {
            out[4] = out[5].mul_add(2.0, out[4]);
            out[5] = out[5].mul_add(3.0, 1.0).ln() / 3.0;
            out[6] += 0.5;
            out.extend_from_slice(&[0.0, 0.0, 0.0, FSRS5_UPGRADE_DECAY]);
        }
        19 => out.extend_from_slice(&[0.0, FSRS5_UPGRADE_DECAY]),
        _ => return None,
    }
    Some(out)
}

/// The first FSRS-6 weight outside [`FSRS6_WEIGHT_BOUNDS`] (or not finite), as its index.
pub fn fsrs6_weight_out_of_range(w: &[f64]) -> Option<usize> {
    w.iter().zip(FSRS6_WEIGHT_BOUNDS).position(|(x, (lo, hi))| {
        #[allow(clippy::cast_possible_truncation)]
        let x32 = *x as f32;
        !x.is_finite() || !(lo..=hi).contains(&x32)
    })
}

/// FSRS-6 weights a scheduler can always run with: [`upgrade_fsrs_weights`], then every
/// weight clamped into [`FSRS6_WEIGHT_BOUNDS`] (a no-op for a preset that validates; it only
/// keeps a row stored before v1.4.1 checked the ranges studyable). `None` for an unknown
/// length or a non-finite value; empty stays empty (the defaults).
pub fn schedulable_fsrs_weights(w: &[f64]) -> Option<Vec<f64>> {
    if w.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let mut out = upgrade_fsrs_weights(w)?;
    for (x, (lo, hi)) in out.iter_mut().zip(FSRS6_WEIGHT_BOUNDS) {
        *x = x.clamp(f64::from(lo), f64::from(hi));
    }
    Some(out)
}

/// Reads `fsrs_weights` and upgrades 17/19 values ([`upgrade_fsrs_weights`]); other lengths
/// stay as they are for [`PresetConfig::validate`] to name.
fn de_fsrs_weights<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<f64>, D::Error> {
    let w = Vec::<f64>::deserialize(d)?;
    Ok(if w.iter().all(|x| x.is_finite()) {
        upgrade_fsrs_weights(&w).unwrap_or(w)
    } else {
        w
    })
}
/// Longest preset name (chars).
pub const MAX_PRESET_NAME: usize = 100;

/// A rejected option: which field and why (English, for logs; the UI shows
/// `errors.validation.preset_config` with `field`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid preset option {field}: {reason}")]
pub struct PresetConfigError {
    /// Field name (`learning_steps_mins`, …).
    pub field: &'static str,
    /// Reason.
    pub reason: String,
}

fn bad(field: &'static str, reason: impl Into<String>) -> Result<(), PresetConfigError> {
    Err(PresetConfigError {
        field,
        reason: reason.into(),
    })
}

fn steps(field: &'static str, v: &[u32]) -> Result<(), PresetConfigError> {
    if v.len() > MAX_STEPS {
        return bad(field, format!("at most {MAX_STEPS} steps"));
    }
    if v.iter().any(|s| !(1..=MAX_STEP_MINS).contains(s)) {
        return bad(
            field,
            format!("each step must be 1..={MAX_STEP_MINS} minutes"),
        );
    }
    Ok(())
}

impl PresetConfig {
    /// Checks every option against its range.
    pub fn validate(&self) -> Result<(), PresetConfigError> {
        if self.new_per_day > MAX_NEW_PER_DAY {
            return bad("new_per_day", format!("must be 0..={MAX_NEW_PER_DAY}"));
        }
        if self.reviews_per_day > MAX_REVIEWS_PER_DAY {
            return bad(
                "reviews_per_day",
                format!("must be 0..={MAX_REVIEWS_PER_DAY}"),
            );
        }
        steps("learning_steps_mins", &self.learning_steps_mins)?;
        steps("relearning_steps_mins", &self.relearning_steps_mins)?;
        if !RETENTION_RANGE.contains(&self.desired_retention) {
            return bad("desired_retention", "must be 0.70..=0.99");
        }
        if !(1..=MAX_INTERVAL_DAYS).contains(&self.maximum_interval_days) {
            return bad(
                "maximum_interval_days",
                format!("must be 1..={MAX_INTERVAL_DAYS}"),
            );
        }
        let w = &self.fsrs_weights;
        if !w.is_empty() && !FSRS_WEIGHT_COUNTS.contains(&w.len()) {
            return bad("fsrs_weights", "must be empty or have 17, 19 or 21 values");
        }
        if w.iter().any(|x| !x.is_finite()) {
            return bad("fsrs_weights", "values must be finite");
        }
        // What the scheduler will run with must be inside the model's ranges (v1.4.1: a
        // preset that saves always studies).
        let upgraded = upgrade_fsrs_weights(w).unwrap_or_default();
        if let Some(i) = fsrs6_weight_out_of_range(&upgraded) {
            let (lo, hi) = FSRS6_WEIGHT_BOUNDS[i];
            return bad(
                "fsrs_weights",
                format!("w{i} must be {lo}..={hi} (as FSRS-6 weights)"),
            );
        }
        Ok(())
    }

    /// The same options with `fsrs_weights` in FSRS-6 form ([`upgrade_fsrs_weights`]); what is
    /// stored. Reading a stored preset does this already; this is for values built in code.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        if let Some(w) = upgrade_fsrs_weights(&self.fsrs_weights) {
            if self.fsrs_weights.iter().all(|x| x.is_finite()) {
                self.fsrs_weights = w;
            }
        }
        self
    }
}

/// Trims and collapses whitespace; `None` if empty or longer than [`MAX_PRESET_NAME`].
pub fn normalize_preset_name(raw: &str) -> Option<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty() && name.chars().count() <= MAX_PRESET_NAME).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_keys_survive_roundtrip() {
        let cfg: PresetConfig =
            serde_json::from_str(r#"{"new_per_day":5,"future_option":true}"#).unwrap();
        assert_eq!(cfg.new_per_day, 5);
        assert_eq!(cfg.reviews_per_day, 200);
        let json = serde_json::to_value(&cfg).unwrap();
        assert_eq!(json["future_option"], serde_json::json!(true));
    }

    #[test]
    fn v01_rows_read_with_new_defaults() {
        // A config written by v0.1 (no FSRS weights, order or bury keys).
        let cfg: PresetConfig = serde_json::from_str(
            r#"{"new_per_day":10,"reviews_per_day":100,"desired_retention":0.85,
                "learning_steps_mins":[5],"relearning_steps_mins":[]}"#,
        )
        .unwrap();
        assert_eq!(cfg.desired_retention, 0.85);
        assert!(cfg.fsrs_weights.is_empty() && cfg.bury_siblings);
        assert_eq!(
            (cfg.new_order, cfg.review_order),
            (NewCardOrder::Added, ReviewOrder::Due)
        );
        assert_eq!(cfg.validate(), Ok(()));
        assert_eq!(cfg.maximum_interval_days, MAX_INTERVAL_DAYS);
        // Phase-2 round 1 kept these two in `extra` under the same keys: they now land in
        // the typed fields.
        let r1: PresetConfig =
            serde_json::from_str(r#"{"maximum_interval_days":90,"fsrs_weights":[]}"#).unwrap();
        assert_eq!(r1.maximum_interval_days, 90);
        assert!(r1.extra.is_empty());
        let json = serde_json::to_value(PresetConfig::default()).unwrap();
        assert_eq!(json["new_order"], "added");
        assert_eq!(json["review_order"], "due");
    }

    #[test]
    fn validation_names_the_field() {
        let ok = PresetConfig::default();
        assert_eq!(ok.validate(), Ok(()));
        let field = |f: fn(&mut PresetConfig)| {
            let mut c = PresetConfig::default();
            f(&mut c);
            c.validate().unwrap_err().field
        };
        assert_eq!(field(|c| c.new_per_day = 10_000), "new_per_day");
        assert_eq!(field(|c| c.reviews_per_day = 100_000), "reviews_per_day");
        assert_eq!(
            field(|c| c.learning_steps_mins = vec![0]),
            "learning_steps_mins"
        );
        assert_eq!(
            field(|c| c.relearning_steps_mins = vec![1; 21]),
            "relearning_steps_mins"
        );
        assert_eq!(field(|c| c.desired_retention = 0.5), "desired_retention");
        assert_eq!(
            field(|c| c.desired_retention = f64::NAN),
            "desired_retention"
        );
        assert_eq!(field(|c| c.fsrs_weights = vec![1.0; 5]), "fsrs_weights");
        assert_eq!(
            field(|c| c.fsrs_weights = [vec![f64::INFINITY], vec![1.0; 18]].concat()),
            "fsrs_weights"
        );
        // v1.4.1: within the FSRS-6 ranges (w4, initial difficulty, is 1..=10).
        assert_eq!(field(|c| c.fsrs_weights = vec![0.5; 19]), "fsrs_weights");
        let c = PresetConfig {
            fsrs_weights: FSRS5_DEFAULTS.to_vec(),
            learning_steps_mins: Vec::new(),
            ..PresetConfig::default()
        };
        assert_eq!(c.validate(), Ok(()));
    }

    /// FSRS-4.5 and FSRS-5 default parameters (the reference implementation's).
    const FSRS45_DEFAULTS: [f64; 17] = [
        0.4872, 1.4003, 3.7145, 13.8206, 5.1618, 1.2298, 0.8975, 0.031, 1.6474, 0.1367, 1.0461,
        2.1072, 0.0793, 0.3246, 1.587, 0.2272, 2.8755,
    ];
    const FSRS5_DEFAULTS: [f64; 19] = [
        0.40255, 1.18385, 3.173, 15.69105, 7.1949, 0.5345, 1.4604, 0.0046, 1.54575, 0.1192,
        1.01925, 1.9395, 0.11, 0.29605, 2.2698, 0.2315, 2.9898, 0.51655, 0.6621,
    ];

    #[test]
    fn old_weights_are_upgraded_once_on_read() {
        let up = upgrade_fsrs_weights(&FSRS5_DEFAULTS).unwrap();
        assert_eq!(&up[..19], &FSRS5_DEFAULTS);
        assert_eq!(&up[19..], &[0.0, FSRS5_UPGRADE_DECAY]);
        let up = upgrade_fsrs_weights(&FSRS45_DEFAULTS).unwrap();
        assert_eq!(up.len(), 21);
        assert_eq!(up[4], 1.2298f64.mul_add(2.0, 5.1618));
        assert_eq!(up[5], 1.2298f64.mul_add(3.0, 1.0).ln() / 3.0);
        assert_eq!(up[6], 0.8975 + 0.5);
        assert_eq!(&up[17..], &[0.0, 0.0, 0.0, 0.5]);
        assert_eq!(upgrade_fsrs_weights(&[]), Some(Vec::new()));
        assert_eq!(upgrade_fsrs_weights(&[1.0; 20]), None);
        // A stored row with old weights reads as FSRS-6, validates, and the decay every
        // surface uses is the one the scheduler runs with.
        for old in [FSRS45_DEFAULTS.to_vec(), FSRS5_DEFAULTS.to_vec()] {
            let json = serde_json::json!({ "fsrs_weights": old });
            let cfg: PresetConfig = serde_json::from_value(json).unwrap();
            assert_eq!(cfg.fsrs_weights.len(), 21);
            assert_eq!(cfg.validate(), Ok(()));
            assert_eq!(crate::recall::decay_of_weights(&cfg.fsrs_weights), 0.5);
            assert_eq!(crate::recall::decay_of_weights(&old), 0.5);
            let again = serde_json::to_value(&cfg).unwrap();
            let back: PresetConfig = serde_json::from_value(again).unwrap();
            assert_eq!(back, cfg, "upgrading is idempotent");
        }
        // Unknown lengths are left for validation to name.
        let cfg: PresetConfig = serde_json::from_str(r#"{"fsrs_weights":[1,2]}"#).unwrap();
        assert_eq!(cfg.fsrs_weights, vec![1.0, 2.0]);
        assert_eq!(cfg.validate().unwrap_err().field, "fsrs_weights");
    }

    #[test]
    fn schedulable_weights_are_clamped_into_range() {
        let mut w = upgrade_fsrs_weights(&FSRS5_DEFAULTS).unwrap();
        assert_eq!(schedulable_fsrs_weights(&w), Some(w.clone()));
        w[4] = 0.5;
        w[9] = -3.0;
        let s = schedulable_fsrs_weights(&w).unwrap();
        assert_eq!((s[4], s[9]), (1.0, 0.0));
        assert_eq!(fsrs6_weight_out_of_range(&s), None);
        assert_eq!(schedulable_fsrs_weights(&[f64::NAN; 21]), None);
        assert_eq!(schedulable_fsrs_weights(&[1.0; 3]), None);
    }

    #[test]
    fn names_are_normalized() {
        assert_eq!(
            normalize_preset_name("  Sınav   haftası "),
            Some("Sınav haftası".into())
        );
        assert_eq!(normalize_preset_name("   "), None);
        assert_eq!(normalize_preset_name(&"x".repeat(101)), None);
    }
}
