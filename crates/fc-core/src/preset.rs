//! Deck option presets (stub in v0.1; algorithm parameters arrive with v0.2/v0.4).

use crate::id::PresetId;
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};

/// A named set of study options shared by decks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// Id.
    pub id: PresetId,
    /// Display name.
    pub name: String,
    /// Options, stored as JSON in `presets.config`.
    pub config: PresetConfig,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// Preset options. Unknown keys are preserved in `extra` so newer versions can add options
/// without a migration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetConfig {
    /// New cards introduced per day.
    pub new_per_day: u32,
    /// Maximum reviews per day.
    pub reviews_per_day: u32,
    /// Target probability of recall (FSRS).
    pub desired_retention: f64,
    /// Learning steps in minutes.
    pub learning_steps_mins: Vec<u32>,
    /// Relearning steps in minutes.
    pub relearning_steps_mins: Vec<u32>,
    /// Options this version does not know about.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for PresetConfig {
    fn default() -> Self {
        Self {
            new_per_day: 20,
            reviews_per_day: 200,
            desired_retention: 0.9,
            learning_steps_mins: vec![1, 10],
            relearning_steps_mins: vec![10],
            extra: serde_json::Map::new(),
        }
    }
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
}
