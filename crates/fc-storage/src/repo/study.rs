//! Study (v0.2): sessions, the review log and study-queue candidates.

use super::*;
use fc_core::card::{CardState, Rating};
use fc_core::{ReviewLogId, SessionId};

/// A `sessions` row. `config`/`state` are owned by the app's session engine.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord {
    /// Id.
    pub id: SessionId,
    /// Kind (`normal`, `custom`, `mistakes`).
    pub kind: String,
    /// Query/filter text, if any (v0.4 query language).
    pub query: Option<String>,
    /// Start options.
    pub config: Value,
    /// Engine state.
    pub state: Value,
    /// Start.
    pub started_at: Timestamp,
    /// End (`None` = open).
    pub ended_at: Option<Timestamp>,
}

/// `review_log.mode` of history brought in by an import (Anki revlog, `.fcpack`): shown in
/// card history and usable by the scheduler, but never counted against today's study limits
/// (those count `normal` rows only).
pub const REVIEW_MODE_IMPORTED: &str = "imported";

/// `review_log.mode` values of answers that changed the schedule (v0.2 `normal`, v0.10
/// `learn`, `timed`, `confidence`): the rows today's study limits count.
pub const SCHEDULED_REVIEW_MODES: [&str; 4] = ["normal", "learn", "timed", "confidence"];

/// Whether a `review_log.mode` is **practice** — the answer did not change the schedule:
/// `trial`, `trial_<mode>` (v0.10 practice variants: `trial_learn`, `trial_timed`,
/// `trial_confidence`) and `test`. Statistics and FSRS optimization skip these rows
/// ([`PRACTICE_MODE_SQL`] is the same rule in SQL).
pub fn is_practice_mode(mode: &str) -> bool {
    mode == "trial" || mode == "test" || mode.starts_with("trial_")
}

/// [`is_practice_mode`] as an SQL condition on a `mode` column.
pub const PRACTICE_MODE_SQL: &str =
    "(mode = 'trial' OR mode = 'test' OR substr(mode, 1, 6) = 'trial_')";

/// A `review_log` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewLogRecord {
    /// Id.
    pub id: ReviewLogId,
    /// Card.
    pub card_id: CardId,
    /// Session.
    pub session_id: Option<SessionId>,
    /// Rating.
    pub rating: Rating,
    /// State before.
    pub state_before: CardState,
    /// State after.
    pub state_after: CardState,
    /// Interval given, days.
    pub scheduled_days: u32,
    /// Days since the previous review.
    pub elapsed_days: u32,
    /// Show → rate, ms.
    pub duration_ms: u32,
    /// Show → reveal, ms.
    pub answer_duration_ms: Option<u32>,
    /// Hints used.
    pub hints_used: u32,
    /// `normal`, `trial`, [`REVIEW_MODE_IMPORTED`] or a v0.10 mode (see
    /// [`SCHEDULED_REVIEW_MODES`], [`is_practice_mode`]).
    pub mode: String,
    /// Stability after.
    pub stability_after: Option<f64>,
    /// Difficulty after.
    pub difficulty_after: Option<f64>,
    /// Time.
    pub created_at: Timestamp,
}

/// Which cards a study queue may contain. Always: live cards of live notes. All set
/// conditions must hold.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StudyCardFilter {
    /// Cards in these decks.
    pub deck_ids: Option<Vec<DeckId>>,
    /// Exactly these cards.
    pub card_ids: Option<Vec<CardId>>,
    /// Learning states.
    pub states: Option<Vec<CardState>>,
    /// Classification conditions: the card or its note has one of the stored values.
    pub attributes: Vec<(AttributeDefId, Vec<String>)>,
    /// Also suspended cards.
    pub include_suspended: bool,
    /// Cards that are not new only when due before this time (or without a due time):
    /// today's queue of a due-only session (v1.0: read through the due indexes instead of
    /// every card of the collection).
    pub due_before: Option<Timestamp>,
    /// New cards: only the first `n` in creation order (`newest`: the last `n`), ties by id.
    /// `None` = every new card.
    pub new_first: Option<NewCardsFirst>,
}

/// [`StudyCardFilter::new_first`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewCardsFirst {
    /// How many.
    pub n: u32,
    /// Newest first instead of oldest first.
    pub newest: bool,
}

/// One entry of a session's queue (`session_queue`, v1.0): the queue is stored as rows so an
/// answer writes the entries it changed, not the whole queue. `seq` orders the entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionQueueRow {
    /// Position key (ascending = queue order).
    pub seq: i64,
    /// Card.
    pub card_id: CardId,
    /// Its note.
    pub note_id: NoteId,
    /// Queue category (`learning`, `review`, `new`).
    pub kind: String,
    /// Not before (intraday learning).
    pub due: Option<Timestamp>,
}

/// Study reads.
pub trait StudyRead {
    /// Session by id.
    fn get_session(&self, id: SessionId) -> StorageResult<Option<SessionRecord>>;
    /// Most recently started open session.
    fn latest_open_session(&self) -> StorageResult<Option<SessionRecord>>;
    /// Candidate cards (ordered by creation time, id).
    fn study_cards(&self, filter: &StudyCardFilter) -> StorageResult<Vec<Card>>;
    /// Review log entry by id.
    fn get_review_log(&self, id: ReviewLogId) -> StorageResult<Option<ReviewLogRecord>>;
    /// Reviews of a card, oldest first.
    fn review_logs_for_card(&self, card: CardId) -> StorageResult<Vec<ReviewLogRecord>>;
    /// Reviews written at or after `since` with `mode`, oldest first (daily limits).
    fn review_logs_since(&self, since: Timestamp, mode: &str)
        -> StorageResult<Vec<ReviewLogRecord>>;
    /// The queue rows of a session, in `seq` order.
    fn session_queue(&self, session: SessionId) -> StorageResult<Vec<SessionQueueRow>>;
}

/// Study writes.
pub trait StudyWrite: StudyRead {
    /// Inserts or replaces a session.
    fn upsert_session(&mut self, session: &SessionRecord) -> StorageResult<()>;
    /// Appends a review (id clash → `Constraint`).
    fn insert_review_log(&mut self, review: &ReviewLogRecord) -> StorageResult<()>;
    /// Deletes a review (undo of the answer that wrote it). Returns whether it existed.
    fn delete_review_log(&mut self, id: ReviewLogId) -> StorageResult<bool>;
    /// Changes a session's queue rows: deletes the rows at `remove` (seqs), then inserts
    /// `add` (`clear`: deletes every row of the session first).
    fn edit_session_queue(
        &mut self,
        session: SessionId,
        clear: bool,
        remove: &[i64],
        add: &[SessionQueueRow],
    ) -> StorageResult<()>;
}
