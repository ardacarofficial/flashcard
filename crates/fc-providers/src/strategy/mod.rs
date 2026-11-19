//! Pure, synchronous strategies. Implementations must be deterministic given their inputs
//! (time and randomness are passed in), so they can live in `fc-core` or a sandboxed plugin.

mod answer;
mod card_type;
mod hint;
mod interop;
mod scheduler;

pub use answer::{AnswerChecker, AnswerVerdict, DiffSpan, DiffSpanKind};
pub use card_type::{CardTypeEngine, RenderContext, RenderedCard};
pub use hint::HintStrategy;
pub use interop::{ExportBundle, Exporter, ImportBundle, ImportOptions, ImportedNote, Importer};
pub use scheduler::{ReviewOutcome, Scheduler, SchedulerParams};

/// Error of a strategy (invalid params, unsupported input).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct StrategyError(pub String);

/// Result alias.
pub type StrategyResult<T> = Result<T, StrategyError>;
