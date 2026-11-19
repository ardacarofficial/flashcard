//! Deck option presets (docs/02 §10 "Deste ayar şablonları"): named study option sets shared
//! by decks. Edited by the `presets` module (v0.4); **read by the study engine (v0.2)** through
//! the effective config of a deck (`fc_app::modules::presets::effective`), so field names are
//! deliberately generic. The schema is documented in docs/10 §11.

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
    /// FSRS parameters; empty = the algorithm's defaults. Otherwise 17 (FSRS-4.5),
    /// 19 (FSRS-5) or 21 (FSRS-6) finite numbers, as produced by the optimizer (v0.6).
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
            fsrs_weights: Vec::new(),
            bury_siblings: false,
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
/// Allowed `desired_retention` range.
pub const RETENTION_RANGE: std::ops::RangeInclusive<f64> = 0.70..=0.99;
/// Allowed `fsrs_weights` lengths (besides empty).
pub const FSRS_WEIGHT_COUNTS: [usize; 3] = [17, 19, 21];
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
        let w = &self.fsrs_weights;
        if !w.is_empty() && !FSRS_WEIGHT_COUNTS.contains(&w.len()) {
            return bad("fsrs_weights", "must be empty or have 17, 19 or 21 values");
        }
        if w.iter().any(|x| !x.is_finite()) {
            return bad("fsrs_weights", "values must be finite");
        }
        Ok(())
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
        assert!(cfg.fsrs_weights.is_empty() && !cfg.bury_siblings);
        assert_eq!(
            (cfg.new_order, cfg.review_order),
            (NewCardOrder::Added, ReviewOrder::Due)
        );
        assert_eq!(cfg.validate(), Ok(()));
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
        let c = PresetConfig {
            fsrs_weights: vec![0.5; 19],
            learning_steps_mins: Vec::new(),
            ..PresetConfig::default()
        };
        assert_eq!(c.validate(), Ok(()));
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
