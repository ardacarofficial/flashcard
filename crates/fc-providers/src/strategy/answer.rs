use super::StrategyResult;
use crate::provider::ProviderInfo;
use serde::{Deserialize, Serialize};

/// Kind of a diff span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffSpanKind {
    /// Matches.
    Same,
    /// Missing from the given answer.
    Missing,
    /// Extra/wrong in the given answer.
    Extra,
}

/// A span of the answer diff shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSpan {
    /// Kind.
    pub kind: DiffSpanKind,
    /// Text.
    pub text: String,
}

/// Verdict of an answer check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerVerdict {
    /// Accepted as correct.
    pub correct: bool,
    /// Similarity 0.0–1.0.
    pub score: f32,
    /// Diff for display.
    pub diff: Vec<DiffSpan>,
}

/// Typed-answer checker (exact, normalized, tolerant, multi; AI — v0.2/v0.8).
/// `check_answer(expected, given, cfg) → verdict` (docs/03 §3).
pub trait AnswerChecker: Send + Sync {
    /// Metadata.
    fn info(&self) -> ProviderInfo;
    /// Checks `given` against `expected` with checker-specific `config`.
    fn check(
        &self,
        expected: &str,
        given: &str,
        config: &serde_json::Value,
    ) -> StrategyResult<AnswerVerdict>;
}
