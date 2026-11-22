//! Study (v0.2): sessions, answers, hints, answer checking, suspend/bury/postpone, history.

use crate::v1::common::*;
use crate::v1::study::*;

methods! {
    // ---- study: sessions ----
    /// Starts a session (builds and persists its queue). Not an undo step: session bookkeeping,
    /// not user data (answers inside it are).
    StudyStart = "study.start", Command, "study", "study:write", Public, undo: false, StartSession => SessionDto;
    /// A session with its current card and the next interval of every button.
    StudySession = "study.session", Query, "study", "study:read", Public, undo: false, SessionRef => SessionDto;
    /// v1.3 launcher: per scope the exact card count, empty reason and next availability,
    /// unfinished sessions to continue and the kinds that can start.
    StudyOptionsGet = "study.options", Query, "study", "study:read", Public, undo: false, StudyOptionsRequest => StudyOptions;
    /// The most recent open session (resume after restart/crash).
    StudyActive = "study.active", Query, "study", "study:read", Public, undo: false, Empty => ActiveSession;
    /// Rates the current card. Undo restores the card, the session and removes the review.
    StudyAnswer = "study.answer", Command, "study", "study:write", Public, undo: true, AnswerCard => SessionDto;
    /// Next hint for the current card (counted in its review).
    StudyHint = "study.hint", Command, "study", "study:write", Public, undo: false, HintRequest => HintDto;
    /// Checks a typed answer (suggests a rating; does not answer).
    StudyCheckAnswer = "study.check_answer", Query, "study", "study:read", Public, undo: false, CheckAnswer => AnswerCheckDto;
    /// Ends a session and returns its numbers.
    StudyEnd = "study.end", Command, "study", "study:write", Public, undo: false, SessionRef => SessionSummary;
    /// End-of-session numbers (also for open sessions).
    StudySummary = "study.summary", Query, "study", "study:read", Public, undo: false, SessionRef => SessionSummary;

    // ---- study: card actions ----
    /// Suspends/unsuspends cards.
    StudySuspend = "study.suspend", Command, "study", "study:write", Public, undo: true, SuspendCards => StudyCardsChanged;
    /// Buries cards until the next study day (or unburies).
    StudyBury = "study.bury", Command, "study", "study:write", Public, undo: true, BuryCards => StudyCardsChanged;
    /// Moves today's due cards N days later.
    StudyPostpone = "study.postpone", Command, "study", "study:write", Public, undo: true, PostponeCards => StudyCardsChanged;
    /// A card's scheduling state, recall probability and review history.
    StudyCardHistory = "study.card_history", Query, "study", "study:read", Public, undo: false, CardHistoryRequest => CardHistory;
    /// Internal inverse of the study commands: writes card columns/session state back and
    /// removes/re-adds review log rows.
    StudyRestore = "study.restore", Command, "study", "study:write", Internal, undo: true, RestoreStudy => StudyCardsChanged;
}
