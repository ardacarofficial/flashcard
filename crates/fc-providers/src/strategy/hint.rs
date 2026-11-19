use super::StrategyResult;
use crate::provider::ProviderInfo;

/// Hint generator (first letter, word starts, partial reveal, AI — v0.2).
pub trait HintStrategy: Send + Sync {
    /// Metadata.
    fn info(&self) -> ProviderInfo;
    /// Hint for `answer` at `level` (1 = smallest). `None` when no further hint exists.
    fn hint(&self, answer: &str, level: u32) -> StrategyResult<Option<String>>;
}
