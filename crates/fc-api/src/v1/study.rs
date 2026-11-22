//! Study DTOs (v0.2): sessions, answers, hints, answer checking, suspend/bury/postpone, card
//! history.
//!
//! A session is a backend state machine (docs/03 §9): `study.start` builds the queue,
//! `study.session` shows the current card with the next interval of every button,
//! `study.answer` rates it (undoable: `oplog.undo` puts the card back exactly), `study.end`
//! closes it. The session state is persisted after every step (`study.active` resumes after a
//! crash). Trial sessions (`mode: trial`) write `review_log` with `mode = "trial"` but never
//! touch `cards`.

use super::cardtypes::RenderedSides;
use fc_core::card::{CardState, Rating};
use fc_core::{
    AttributeDefId, CardId, DeckId, NoteId, ReviewLogId, SavedQueryId, SessionId, Timestamp,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What a session studies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StudyKind {
    /// Cards due today (+ new cards) in a deck or the whole collection.
    Normal,
    /// Cards chosen by a filter (deck, classification values, not-yet-due cards…).
    Custom,
    /// The cards rated "Again" in an earlier session (`from_session`).
    Mistakes,
    /// v0.10 learn mode (Quizlet style): the chosen cards are asked as multiple choice, then
    /// written, until each is correct at every stage (`modes.learn.answer`).
    Learn,
    /// v0.10 test: a paper of mixed question types from the chosen cards, scored on
    /// submission (`modes.test.paper`, `modes.test.submit`). Never changes the schedule.
    Test,
    /// v1.1 match: rounds of terms and definitions to pair against the clock
    /// (`modes.match.start/pick/finish`). Practice: never changes the schedule.
    Match,
}

/// Whether answers change the schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StudyMode {
    /// Answers reschedule cards (default).
    #[default]
    Scheduled,
    /// Practice: answers are logged (`review_log.mode = "trial"`) but `cards` never changes.
    Trial,
}

/// Order of reviews and new cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StudyOrder {
    /// Reviews by due date, new cards in the order they were added.
    Due,
    /// Shuffled.
    Random,
    /// Most lapses / highest difficulty first.
    HardestFirst,
    /// Most recently added first.
    NewestFirst,
}

/// Classification condition: the card (or its note) has one of `values` for `attr_id`.
/// Values are stored values: option ids for choice kinds, the number/text/`true`/`false`
/// otherwise. (The query language — [`StudyFilter::query`] — covers this and more.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AttributeCondition {
    /// Classification.
    pub attr_id: AttributeDefId,
    /// Accepted values (any of).
    pub values: Vec<String>,
}

/// Card filter of a custom session. All conditions must hold.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct StudyFilter {
    /// Classification conditions (AND).
    #[serde(default)]
    pub attributes: Vec<AttributeCondition>,
    /// Learning states to include (`null` = all).
    #[serde(default)]
    pub states: Option<Vec<CardState>>,
    /// Only cards due today (default `false` for custom sessions: cramming).
    #[serde(default)]
    pub due_only: Option<bool>,
    /// At most this many cards.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Query-language text (v0.4: `seviye:A2 is:due`, `deck:İngilizce tag:fiil`, …): only
    /// matching cards. Positioned errors as in `search.cards` (`VALIDATION_QUERY`).
    #[serde(default)]
    pub query: Option<String>,
    /// A saved query to study ("filtered deck"): resolved live from its canonical form, so
    /// renamed classifications/options keep working. Not together with `query`.
    #[serde(default)]
    pub saved_query_id: Option<SavedQueryId>,
}

/// `study.start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct StartSession {
    /// Kind.
    pub kind: StudyKind,
    /// Default `scheduled`.
    #[serde(default)]
    pub mode: Option<StudyMode>,
    /// Deck (`null` = whole collection).
    #[serde(default)]
    pub deck_id: Option<DeckId>,
    /// Include sub-decks (default: the `decks.include_subdecks` setting).
    #[serde(default)]
    pub include_subdecks: Option<bool>,
    /// Custom sessions: the card filter.
    #[serde(default)]
    pub filter: Option<StudyFilter>,
    /// Mistakes sessions: the session whose "Again" cards to repeat.
    #[serde(default)]
    pub from_session: Option<SessionId>,
    /// New cards at most (default: `study.new_per_session`).
    #[serde(default)]
    pub new_limit: Option<u32>,
    /// Order (default: the deck preset's `new_order` / `review_order`).
    #[serde(default)]
    pub order: Option<StudyOrder>,
    /// Client UTC offset in minutes (Istanbul: 180), for the study-day boundary. Default 0.
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
    /// v0.10 study modes: time limit, rating scale, test options.
    #[serde(default)]
    pub modes: Option<super::modes::StartModes>,
    /// v1.3: which cards (`null` = the kind's own default: today's queue for `normal`, every
    /// card of the deck for `learn`/`test`/`match`, the filter for `custom`). Kinds `normal`,
    /// `learn`, `test`, `match` only. `studied_today`, `struggled_today` and `all` are
    /// practice (`mode` is forced to `trial`) over a set frozen at start. An empty set fails
    /// with `CONFLICT_NOTHING_TO_STUDY` and leaves every open session as it was.
    #[serde(default)]
    pub scope: Option<StudyScope>,
}

/// v1.3: which cards a session takes (`study.options`, `study.start`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StudyScope {
    /// Today's queue exactly as a normal session builds it: (re)learning cards due today,
    /// due reviews and new cards within the deck's daily and per-session limits.
    Due,
    /// Unseen cards only, within the new-card limits.
    New,
    /// Distinct cards answered during the current local study day (any mode). Practice.
    StudiedToday,
    /// Distinct cards rated Again / answered wrong during the current local study day (any
    /// mode). Practice.
    StruggledToday,
    /// Every card of the deck. Practice.
    All,
}

/// v1.3: why a scope has no cards (`StudyScopeOption::empty_reason`, details of
/// `CONFLICT_NOTHING_TO_STUDY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StudyEmptyReason {
    /// The deck has no cards.
    NoCards,
    /// Nothing is due today (and, for `new`, no unseen card is left).
    NothingDue,
    /// Today's new/review limit is used up; `next_available_at` = start of the next study
    /// day.
    DailyLimitReached,
    /// Only learning cards remain and none can be shown yet; `next_available_at` = when the
    /// first one can.
    Waiting,
    /// No card was answered during this study day.
    NoneStudiedToday,
    /// No card was rated Again / answered wrong during this study day.
    NoneStruggledToday,
}

/// `study.options`: what the study launcher offers for a deck. No side effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct StudyOptionsRequest {
    /// Deck (`null` = whole collection).
    #[serde(default)]
    pub deck_id: Option<DeckId>,
    /// Include sub-decks (default: the `decks.include_subdecks` setting, as `study.start`).
    #[serde(default)]
    pub include_subdecks: Option<bool>,
    /// Client UTC offset, minutes (study-day boundary). Default: the offset the client last
    /// gave `study.start`, else 0.
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// An unfinished session the launcher can continue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ResumableSession {
    /// The session as `study.session` returns it.
    pub session: SessionDto,
    /// Name (full path) of its deck (`null` = whole collection).
    pub deck_name: Option<String>,
    /// Answers given so far (test: questions answered on the saved draft).
    pub answered: u32,
    /// What is left: queued cards; test: unanswered questions; match: pairs left on the
    /// board (or usable cards before the first round).
    pub remaining: u32,
}

/// One scope of the launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StudyScopeOption {
    /// Scope.
    pub scope: StudyScope,
    /// Exactly how many cards a session started now with this scope contains (the card set
    /// of a `normal` session; learn takes at most its card limit from it, a test draws its
    /// questions from it).
    pub count: u32,
    /// `scheduled` for `due`/`new`; `trial` (practice, the schedule is untouched) otherwise.
    pub mode: StudyMode,
    /// Set when `count` is 0.
    pub empty_reason: Option<StudyEmptyReason>,
    /// When cards become available: limits reset (`daily_limit_reached`) or the first
    /// waiting learning card can be shown (`waiting`).
    pub next_available_at: Option<Timestamp>,
}

/// Response of `study.options`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StudyOptions {
    /// Deck asked about.
    pub deck_id: Option<DeckId>,
    /// Its full path (`null` = whole collection).
    pub deck_name: Option<String>,
    /// Unfinished sessions relevant to the deck (all kinds; every deck for `deck_id: null`),
    /// newest first.
    pub resumable: Vec<ResumableSession>,
    /// Always all five scopes: due, new, studied_today, struggled_today, all.
    pub scopes: Vec<StudyScopeOption>,
    /// Kinds that can be started now (module switches): a subset of normal, learn, test,
    /// match, in that order.
    pub modes: Vec<StudyKind>,
}

/// Request naming a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionRef {
    /// Session.
    pub session_id: SessionId,
}

/// Remaining cards per category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct StudyCounts {
    /// New.
    pub new: u32,
    /// (Re)learning.
    pub learning: u32,
    /// Due reviews.
    pub review: u32,
}

/// What a rating button would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct IntervalPreview {
    /// Button.
    pub rating: Rating,
    /// Time until the card would be due again, ms (`10m`, `3d`… formatting is the UI's).
    /// Whole-day intervals are `scheduled_days` × 24 h (the card is due from the start of
    /// that study day).
    pub interval_ms: i64,
    /// Whole days (0 for intraday steps).
    pub scheduled_days: u32,
}

/// The card on screen. Sides are **plain text** (display as text, never as HTML).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StudyCardDto {
    /// Card.
    pub card_id: CardId,
    /// Its note.
    pub note_id: NoteId,
    /// Its deck.
    pub deck_id: DeckId,
    /// Learning state (simulated state in trial sessions).
    pub state: CardState,
    /// The card rendered by its card type engine (`cards.render`): sanitized question and
    /// answer HTML, scoped CSS, interaction (self-grade / type / choice), TTS.
    pub sides: RenderedSides,
    /// Flag bits (`cards.flags`; flag `n` = bit `n - 1`, Anki numbering, names and colors
    /// from `flags.list`).
    pub flags: u32,
    /// Show an input for typing the answer (a typing card type, or `study.type_answer`).
    pub type_answer: bool,
    /// Buttons, in order again/hard/good/easy.
    pub intervals: Vec<IntervalPreview>,
    /// Hints already shown for this card.
    pub hints_used: u32,
}

/// A session as the study screen sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SessionDto {
    /// Id.
    pub id: SessionId,
    /// Kind.
    pub kind: StudyKind,
    /// Mode.
    pub mode: StudyMode,
    /// Deck (`null` = collection).
    pub deck_id: Option<DeckId>,
    /// Start.
    pub started_at: Timestamp,
    /// End (`null` while open).
    pub ended_at: Option<Timestamp>,
    /// Remaining cards.
    pub counts: StudyCounts,
    /// Cards answered so far.
    pub answered: u32,
    /// Answers other than "Again".
    pub correct: u32,
    /// Cards rated "Again" (what a mistakes session would repeat).
    pub mistakes: u32,
    /// Card to show now (`null` = finished, or waiting for `next_due`).
    pub current: Option<StudyCardDto>,
    /// When the next waiting learning card can be shown (its due time minus the learn-ahead
    /// window), while no card is current and the session is open. The session stays open
    /// while it waits.
    pub next_due: Option<Timestamp>,
    /// v0.10 study modes of the session (`null` when it uses none; may be absent from
    /// clients/mocks written before v0.10).
    #[serde(default)]
    #[ts(optional = nullable)]
    pub modes: Option<super::modes::SessionModesDto>,
}

/// Response of `study.active`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ActiveSession {
    /// The most recent open session, if any (resume after a crash/restart).
    pub session: Option<SessionDto>,
}

/// `study.answer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct AnswerCard {
    /// Session.
    pub session_id: SessionId,
    /// Card being answered (the card shown: queued and available now).
    pub card_id: CardId,
    /// Rating.
    pub rating: Rating,
    /// Time from showing the card to rating it, ms, as measured by the client. The backend
    /// caps it by its own clock (time since the previous answer, and one hour).
    pub duration_ms: u32,
    /// Time from showing the card to revealing the answer, ms (capped by `duration_ms`).
    #[serde(default)]
    pub reveal_ms: Option<u32>,
}

/// `study.hint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct HintRequest {
    /// Session.
    pub session_id: SessionId,
    /// Current card.
    pub card_id: CardId,
}

/// Response of `study.hint`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HintDto {
    /// Hints used for the card (counted in the review log).
    pub level: u32,
    /// Hint text (`null` = no further hint; the counter does not move).
    pub text: Option<String>,
}

/// `study.check_answer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct CheckAnswer {
    /// Session.
    pub session_id: SessionId,
    /// Current card.
    pub card_id: CardId,
    /// Typed answer (judged by the configured answer checker).
    #[serde(default)]
    pub given: String,
    /// Picked option of a choice card (index into `sides.interaction.options`); the card
    /// type engine decides. Wins over `given`.
    #[serde(default)]
    pub choice: Option<u32>,
}

/// Grade of a typed answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AnswerGrade {
    /// Accepted.
    Correct,
    /// Typo-level difference.
    Almost,
    /// Not accepted.
    Wrong,
}

/// Diff span kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    /// Matches.
    Same,
    /// In the expected answer, missing from the given one.
    Missing,
    /// In the given answer only.
    Extra,
}

/// One diff span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DiffSpanDto {
    /// Kind.
    pub kind: DiffKind,
    /// Text.
    pub text: String,
}

/// Response of `study.check_answer`. The rating is only suggested.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AnswerCheckDto {
    /// Grade.
    pub grade: AnswerGrade,
    /// Similarity 0–1.
    pub score: f32,
    /// Diff of the given answer against `expected`.
    pub diff: Vec<DiffSpanDto>,
    /// The expected answer compared with (closest alternative for multi-answers).
    pub expected: String,
    /// Suggested rating (correct → good, almost → hard, wrong → again).
    pub suggested_rating: Rating,
    /// `study.auto_grade` is on: the client may submit `suggested_rating` directly.
    pub auto_grade: bool,
}

/// End-of-session numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SessionSummary {
    /// Session.
    pub session_id: SessionId,
    /// Kind.
    pub kind: StudyKind,
    /// Mode.
    pub mode: StudyMode,
    /// Answers given.
    pub answered: u32,
    /// Answers other than "Again".
    pub correct: u32,
    /// Distinct cards rated "Again" (a mistakes session repeats these).
    pub mistakes: u32,
    /// Total answer time, ms.
    pub time_ms: u64,
    /// Start.
    pub started_at: Timestamp,
    /// End (`null` while open).
    pub ended_at: Option<Timestamp>,
}

/// `study.suspend`: suspend (or unsuspend) cards. With `session_id`, they also leave that
/// session's queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct SuspendCards {
    /// Cards.
    pub card_ids: Vec<CardId>,
    /// `true` = suspend, `false` = unsuspend.
    pub suspended: bool,
    /// Session to drop them from.
    #[serde(default)]
    pub session_id: Option<SessionId>,
}

/// `study.bury`: hide cards until the next study day (or unbury).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct BuryCards {
    /// Cards.
    pub card_ids: Vec<CardId>,
    /// `true` = bury, `false` = unbury.
    pub buried: bool,
    /// Session to drop them from.
    #[serde(default)]
    pub session_id: Option<SessionId>,
    /// Client UTC offset, minutes (study-day boundary). Default: the session's, else 0.
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// `study.postpone`: moves the cards due today (reviews and learning cards) `days` days
/// later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct PostponeCards {
    /// Deck (`null` = whole collection).
    #[serde(default)]
    pub deck_id: Option<DeckId>,
    /// Include sub-decks (default `true`).
    #[serde(default)]
    pub include_subdecks: Option<bool>,
    /// Days to move (1–365).
    pub days: u32,
    /// Client UTC offset, minutes. Default 0.
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// Response of the card-changing study commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct StudyCardsChanged {
    /// Cards changed.
    pub cards: u32,
}

/// Study-owned columns of a card (what `study.restore` writes back).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CardStudySnapshot {
    /// Card.
    pub id: CardId,
    /// Learning state.
    pub state: CardState,
    /// Due.
    pub due: Option<Timestamp>,
    /// Stability.
    pub stability: Option<f64>,
    /// Difficulty.
    pub difficulty: Option<f64>,
    /// Successful reviews.
    pub reps: u32,
    /// Lapses.
    pub lapses: u32,
    /// Last review.
    pub last_review: Option<Timestamp>,
    /// Suspended.
    pub suspended: bool,
    /// Buried until.
    pub buried_until: Option<Timestamp>,
    /// Modification time.
    pub updated_at: Timestamp,
}

/// Persisted state of a session (`study.restore`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionStateSnapshot {
    /// Session.
    pub id: SessionId,
    /// Opaque engine state (`sessions.state`).
    #[ts(type = "unknown")]
    pub state: serde_json::Value,
}

/// One review (`review_log` row, immutable except for undo of the answer that wrote it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ReviewLogDto {
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
    /// Interval given, days (0 = intraday).
    pub scheduled_days: u32,
    /// Days since the previous review.
    pub elapsed_days: u32,
    /// Show → rate, ms.
    pub duration_ms: u32,
    /// Show → reveal, ms.
    pub answer_duration_ms: Option<u32>,
    /// Hints used.
    pub hints_used: u32,
    /// Where the review came from. Changes the schedule: `normal`, `learn`, `timed`,
    /// `confidence` (v0.10 modes), `imported` (v0.5). Practice, schedule unchanged: `trial`,
    /// `trial_learn`, `trial_timed`, `trial_confidence`, `test`, `match` (v1.1).
    pub mode: String,
    /// Stability after.
    pub stability_after: Option<f64>,
    /// Difficulty after.
    pub difficulty_after: Option<f64>,
    /// Time.
    pub created_at: Timestamp,
}

/// `study.restore` (internal): inverse of the study commands. Writes the listed card columns
/// and session state back, deletes/re-inserts review log rows.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreStudy {
    /// Card columns to write back.
    #[serde(default)]
    pub cards: Vec<CardStudySnapshot>,
    /// Session state to write back.
    #[serde(default)]
    pub session: Option<SessionStateSnapshot>,
    /// Review log rows to delete.
    #[serde(default)]
    pub remove_reviews: Vec<ReviewLogId>,
    /// Review log rows to insert.
    #[serde(default)]
    pub add_reviews: Vec<ReviewLogDto>,
}

/// `study.card_history`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CardHistoryRequest {
    /// Card.
    pub card_id: CardId,
}

/// A card's scheduling state and full review history (oldest first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CardHistory {
    /// The card's study columns.
    pub card: CardStudySnapshot,
    /// Probability of recall now (FSRS; `null` for new cards).
    pub retrievability: Option<f64>,
    /// Reviews, oldest first (trial reviews included, see `mode`).
    pub reviews: Vec<ReviewLogDto>,
}
