//! v0.10 study modes (docs/02 §6.1, §14, §27): learn mode (Quizlet style), test generator,
//! confidence-based rating, timed mode, focus mode, inline card creation.
//!
//! Learn and test are **study session kinds** (`study.start { kind: learn | test }`); timed
//! and confidence are **options of a study session** (`study.start { timed, rating_scale }`).
//! Every answer goes through the v0.2 session engine and writes `review_log` with its own
//! `mode` (see [`crate::v1::study::ReviewLogDto::mode`]); the per-session state of a mode is
//! in [`SessionModesDto`] (`SessionDto.modes`). Focus mode is a frontend presentation with
//! settings only (`focus.*`).

use crate::v1::study::{AnswerGrade, DiffSpanDto, SessionDto};
use fc_core::card::Rating;
use fc_core::{CardId, DeckId, NoteId, NoteTypeId, SessionId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ------------------------------------------------------------------ session options

/// What happens when a card's time runs out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TimeoutAction {
    /// The card is answered "Again" (`review_log.mode = timed`).
    Again,
    /// The card leaves the session unanswered (no review, schedule unchanged).
    Skip,
}

/// Timed mode (`study.start.timed`): a time limit per card, measured on the backend's clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TimedOptions {
    /// Seconds per card (3–600).
    pub seconds: u32,
    /// On timeout.
    pub on_timeout: TimeoutAction,
}

/// How cards are rated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RatingScale {
    /// The four buttons (again / hard / good / easy).
    #[default]
    Buttons,
    /// "How sure was I?" 1–5, mapped onto the four ratings (`confidence.map.<n>` settings).
    Confidence,
}

/// Question type of a test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TestQuestionKind {
    /// Pick the answer among options.
    MultipleChoice,
    /// Type the answer.
    Written,
    /// Is the shown answer right?
    TrueFalse,
}

/// Test options (`study.start.test`, kind `test`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct TestOptions {
    /// Questions (default: the `practice_test.questions` setting; at most 200, at most one
    /// per card).
    #[serde(default)]
    pub questions: Option<u32>,
    /// Question types, mixed in turn (default: all three).
    #[serde(default)]
    pub kinds: Option<Vec<TestQuestionKind>>,
}

/// v0.10 options of `study.start` (`StartSession.modes`). Learn and test are kinds
/// (`kind: learn | test`); these options shape any kind they apply to.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct StartModes {
    /// Time limit per card (kinds `normal`, `custom`, `mistakes`; module `timed`).
    #[serde(default)]
    pub timed: Option<TimedOptions>,
    /// Rating scale (default: the `confidence.rating_scale` setting of the deck; module
    /// `confidence`). Kinds `normal`, `custom`, `mistakes`.
    #[serde(default)]
    pub rating_scale: Option<RatingScale>,
    /// Test paper options (kind `test`).
    #[serde(default)]
    pub test: Option<TestOptions>,
}

// ------------------------------------------------------------------ session state

/// Countdown of the shown card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TimerDto {
    /// Seconds per card.
    pub seconds: u32,
    /// On timeout.
    pub on_timeout: TimeoutAction,
    /// When the shown card's time runs out (`null` when no card is shown, or its countdown has
    /// not started: the screen calls `modes.timed.start` when it shows the card).
    pub deadline: Option<Timestamp>,
}

/// One button of the confidence scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConfidenceLevelDto {
    /// 1 (not at all) … 5 (perfectly).
    pub level: u8,
    /// Rating it maps to (settings).
    pub rating: Rating,
    /// Interval this answer would give, ms (as `IntervalPreview.interval_ms`).
    pub interval_ms: i64,
}

/// Stage a learn-mode card is asked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum LearnStage {
    /// Multiple choice.
    Choice,
    /// Written.
    Written,
}

/// Learn progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct LearnProgressDto {
    /// Cards in the session.
    pub total: u32,
    /// Learned (correct at every stage).
    pub learned: u32,
    /// Waiting at the multiple-choice stage.
    pub choice: u32,
    /// Waiting at the written stage.
    pub written: u32,
}

/// Learn mode: how the shown card is asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LearnDto {
    /// Stage of the shown card (`null` when none is shown).
    pub stage: Option<LearnStage>,
    /// Multiple-choice options (plain text; empty at the written stage).
    pub options: Vec<String>,
    /// Wrong answers of the shown card so far.
    pub mistakes: u32,
    /// Progress.
    pub progress: LearnProgressDto,
}

/// Test: paper size and state (`modes.test.paper` has the questions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TestInfoDto {
    /// Questions.
    pub questions: u32,
    /// Submitted.
    pub submitted: bool,
    /// Score once submitted.
    pub percent: Option<u32>,
}

/// Mode state of a session (`SessionDto.modes`; `null` for sessions using none).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SessionModesDto {
    /// Rating scale of the session.
    pub rating_scale: RatingScale,
    /// Confidence buttons for the shown card (empty unless the scale is `confidence` and a
    /// card is shown).
    pub confidence: Vec<ConfidenceLevelDto>,
    /// Timed sessions: the countdown.
    pub timer: Option<TimerDto>,
    /// Cards whose time ran out.
    pub timed_out: u32,
    /// Learn sessions.
    pub learn: Option<LearnDto>,
    /// Test sessions.
    pub test: Option<TestInfoDto>,
}

// ------------------------------------------------------------------ learn

/// `modes.learn.answer`: answers the shown learn card — an option at the multiple-choice
/// stage, typed text at the written stage. The backend judges it, rates the card
/// (correct → good, almost → hard, wrong → again) and moves it between stages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct LearnAnswer {
    /// Session (kind `learn`).
    pub session_id: SessionId,
    /// Shown card.
    pub card_id: CardId,
    /// Picked option (multiple-choice stage).
    #[serde(default)]
    pub choice: Option<u32>,
    /// Typed answer (written stage).
    #[serde(default)]
    pub given: Option<String>,
    /// Show → answer, ms (capped by the backend clock).
    pub duration_ms: u32,
}

/// Response of `modes.learn.answer`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct LearnAnswerResult {
    /// Verdict (multiple choice: correct or wrong).
    pub grade: AnswerGrade,
    /// Stage the card was asked at.
    pub stage: LearnStage,
    /// The answer completed the card.
    pub learned: bool,
    /// The correct answer.
    pub expected: String,
    /// Diff of a typed answer.
    pub diff: Vec<DiffSpanDto>,
    /// The session after the answer.
    pub session: SessionDto,
}

// ------------------------------------------------------------------ confidence

/// `modes.confidence.answer`: rates the shown card on the 1–5 confidence scale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ConfidenceAnswer {
    /// Session.
    pub session_id: SessionId,
    /// Shown card.
    pub card_id: CardId,
    /// 1–5.
    pub level: u8,
    /// Show → rate, ms.
    pub duration_ms: u32,
    /// Show → reveal, ms.
    #[serde(default)]
    pub reveal_ms: Option<u32>,
}

// ------------------------------------------------------------------ timed

/// `modes.timed.expire`: the shown card's time ran out (checked on the backend clock).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ExpireCard {
    /// Session (timed).
    pub session_id: SessionId,
    /// Shown card.
    pub card_id: CardId,
}

/// `modes.timed.start`: the screen put `card_id` up; its countdown starts now (backend clock).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct StartTimer {
    /// Session (timed).
    pub session_id: SessionId,
    /// The card the screen shows (must be the session's current card).
    pub card_id: CardId,
}

// ------------------------------------------------------------------ test

/// `modes.test.paper`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct TestPaperRequest {
    /// Session (kind `test`).
    pub session_id: SessionId,
    /// Include the answer key before submission (printing an answer sheet).
    #[serde(default)]
    pub with_key: Option<bool>,
}

/// Answer key of a question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TestKeyDto {
    /// Multiple choice: correct option.
    pub option: Option<u32>,
    /// True/false: the statement is right.
    pub truth: Option<bool>,
    /// Correct answer as text.
    pub expected: String,
}

/// One question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TestQuestionDto {
    /// 0-based position on the paper.
    pub index: u32,
    /// Card asked.
    pub card_id: CardId,
    /// Type.
    pub kind: TestQuestionKind,
    /// Question (sanitized HTML of the card's question side).
    pub prompt_html: String,
    /// Multiple choice: options (plain text).
    pub options: Vec<String>,
    /// True/false: the proposed answer (plain text).
    pub statement: Option<String>,
    /// Answer key (after submission, or with `with_key`).
    pub key: Option<TestKeyDto>,
}

/// Verdict on one answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TestVerdict {
    /// Right (1 point).
    Correct,
    /// Typo-level written answer (½ point).
    Almost,
    /// Wrong.
    Wrong,
    /// Not answered.
    Blank,
}

/// One graded answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TestItemDto {
    /// Question index.
    pub index: u32,
    /// Verdict.
    pub verdict: TestVerdict,
}

/// Score of a submitted test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TestResultDto {
    /// Questions.
    pub total: u32,
    /// Right answers.
    pub correct: u32,
    /// Typo-level written answers.
    pub almost: u32,
    /// Wrong answers.
    pub wrong: u32,
    /// Unanswered.
    pub blank: u32,
    /// Points (correct = 1, almost = ½).
    pub points: f64,
    /// Score 0–100.
    pub percent: u32,
    /// Per question.
    pub items: Vec<TestItemDto>,
}

/// A test paper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TestPaperDto {
    /// Session.
    pub session_id: SessionId,
    /// Questions, grouped by type.
    pub questions: Vec<TestQuestionDto>,
    /// The answers given (after submission; same order as `questions`).
    pub answers: Option<Vec<TestAnswerDto>>,
    /// Result (after submission).
    pub result: Option<TestResultDto>,
}

/// An answer to one question; set the field matching the question type (none = blank).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct TestAnswerDto {
    /// Question index.
    pub index: u32,
    /// Multiple choice: picked option.
    #[serde(default)]
    pub choice: Option<u32>,
    /// Written: typed text.
    #[serde(default)]
    pub text: Option<String>,
    /// True/false.
    #[serde(default)]
    pub truth: Option<bool>,
}

/// `modes.test.submit`: grades the paper, logs one review per question
/// (`review_log.mode = test`; the schedule never changes) and returns the score. Undoable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SubmitTest {
    /// Session (kind `test`).
    pub session_id: SessionId,
    /// Answers (questions not listed are blank).
    pub answers: Vec<TestAnswerDto>,
    /// Time spent on the paper, ms (capped by the backend clock).
    pub duration_ms: u32,
}

// ------------------------------------------------------------------ inline cards

/// `modes.inline.preview`: the cards a text would make (`question :: answer` lines).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct InlinePreviewRequest {
    /// The text.
    pub text: String,
    /// Deck the notes would go to.
    pub deck_id: DeckId,
    /// Note type (default: Classic). Its first field gets the question, its second the answer.
    #[serde(default)]
    pub notetype_id: Option<NoteTypeId>,
}

/// One card of the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InlineCardDto {
    /// 1-based line.
    pub line: u32,
    /// Question.
    pub front: String,
    /// Answer.
    pub back: String,
    /// A note like this exists already (it would be skipped).
    pub duplicate: bool,
}

/// Response of `modes.inline.preview`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InlinePreview {
    /// Cards, in text order.
    pub cards: Vec<InlineCardDto>,
    /// More lines matched than one call takes (500).
    pub truncated: bool,
}

/// `modes.inline.create`: one note per `question :: answer` line (duplicates skipped), as one
/// undo step. Works during a study session without touching it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct CreateInlineCards {
    /// The text.
    pub text: String,
    /// Home deck of the notes.
    pub deck_id: DeckId,
    /// Note type (default: Classic).
    #[serde(default)]
    pub notetype_id: Option<NoteTypeId>,
    /// Tags for every note.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Response of `modes.inline.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InlineCardsCreated {
    /// Notes created, in text order.
    pub note_ids: Vec<NoteId>,
    /// Lines skipped because the note exists already.
    pub duplicates: u32,
    /// More lines matched than one call takes (500; the rest was not created).
    pub truncated: bool,
}
