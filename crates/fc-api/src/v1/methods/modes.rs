//! v0.10 study modes: learn, test, confidence, timed, inline card creation; v1.1 match. Learn and test
//! sessions start with `study.start` (kinds `learn`, `test`); timed/confidence are its options.

use crate::v1::modes::*;
use crate::v1::study::SessionDto;

methods! {
    // ---- learn (module `learn`) ----
    /// Answers the shown card of a learn session (option or typed text; judged by the backend).
    /// Undo restores the card, the learn progress and removes the review.
    ModesLearnAnswer = "modes.learn.answer", Command, "learn", "study:write", Public, undo: true, LearnAnswer => LearnAnswerResult;

    // ---- quiz (module `quiz`, v1.4: "Test") ----
    /// Picks an option for the shown word of a quiz; the verdict comes back at once. Undo
    /// restores the word (and the card, when the quiz changes the schedule).
    ModesQuizAnswer = "modes.quiz.answer", Command, "quiz", "study:write", Public, undo: true, QuizAnswer => QuizAnswerResult;
    /// The score and the missed words of a quiz (also once it ended).
    ModesQuizResult = "modes.quiz.result", Query, "quiz", "study:read", Public, undo: false, QuizRef => QuizResultDto;

    // ---- confidence (module `confidence`) ----
    /// Rates the shown card on the 1–5 confidence scale (mapped onto the four ratings).
    ModesConfidenceAnswer = "modes.confidence.answer", Command, "confidence", "study:write", Public, undo: true, ConfidenceAnswer => SessionDto;

    // ---- timed (module `timed`) ----
    /// The shown card's time ran out: answers "Again" or skips it (session option).
    ModesTimedExpire = "modes.timed.expire", Command, "timed", "study:write", Public, undo: true, ExpireCard => SessionDto;
    /// Starts the countdown of the card the screen just put up (`TimerDto.deadline` was
    /// `null`). No-op while it runs; never restarts one (`CONFLICT_TIME_UP` once it ran out).
    /// Session bookkeeping.
    ModesTimedStart = "modes.timed.start", Command, "timed", "study:write", Public, undo: false, StartTimer => SessionDto;

    // ---- test (module `practice_test`) ----
    /// The questions of a test session (answer key after submission or on request).
    ModesTestPaper = "modes.test.paper", Query, "practice_test", "study:read", Public, undo: false, TestPaperRequest => TestPaperDto;
    /// v1.3: saves the answers of an unfinished paper (continued later). Session bookkeeping.
    ModesTestSaveDraft = "modes.test.save_draft", Command, "practice_test", "study:write", Public, undo: false, SaveTestDraft => TestPaperDto;
    /// Grades a test, logs one review per question (`mode = test`) and returns the score.
    ModesTestSubmit = "modes.test.submit", Command, "practice_test", "study:write", Public, undo: true, SubmitTest => TestPaperDto;

    // ---- match (module `match`, v1.1) ----
    /// Deals a new round of up to 6 pairs from the cards of a match session (`study.start
    /// { kind: match }`); replaces an unfinished round. Session bookkeeping.
    ModesMatchStart = "modes.match.start", Command, "match", "study:write", Public, undo: false, MatchRef => MatchRoundDto;
    /// Pairs two tiles of the current round: right pairs leave the board, wrong ones count a
    /// mistake. Session bookkeeping (a game move, not data).
    ModesMatchPick = "modes.match.pick", Command, "match", "study:write", Public, undo: false, MatchPick => MatchPickResult;
    /// Finishes a complete round: its time on the backend clock, the deck's best time, one
    /// practice review per card (`mode = match`, schedule unchanged). Undo removes them and
    /// restores the best time.
    ModesMatchFinish = "modes.match.finish", Command, "match", "study:write", Public, undo: true, MatchRef => MatchResultDto;

    // ---- inline cards (module `inline_cards`) ----
    /// The cards `question :: answer` lines of a text would make.
    ModesInlinePreview = "modes.inline.preview", Query, "inline_cards", "notes:read", Public, undo: false, InlinePreviewRequest => InlinePreview;
    /// Creates a note per `question :: answer` line (one undo step).
    ModesInlineCreate = "modes.inline.create", Command, "inline_cards", "notes:write", Public, undo: true, CreateInlineCards => InlineCardsCreated;
}
