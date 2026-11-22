//! Accepted answers (v1.4): what the offline written-answer check also accepts, per note.

use crate::v1::accepted_answers::*;

methods! {
    /// A note's accepted answers, by field.
    AcceptedAnswersList = "accepted_answers.list", Query, "accepted_answers", "notes:read", Public, undo: false, AcceptedAnswersRef => NoteAcceptedAnswers;
    /// Adds answers the written check also accepts for a note's field — one undo step.
    AcceptedAnswersAdd = "accepted_answers.add", Command, "accepted_answers", "notes:write", Public, undo: true, EditAcceptedAnswers => NoteAcceptedAnswers;
    /// Removes accepted answers of a note's field — one undo step.
    AcceptedAnswersRemove = "accepted_answers.remove", Command, "accepted_answers", "notes:write", Public, undo: true, EditAcceptedAnswers => NoteAcceptedAnswers;
    /// Internal: writes a note's lists back exactly (inverse of add/remove).
    AcceptedAnswersRestore = "accepted_answers.restore", Command, "accepted_answers", "notes:write", Internal, undo: true, RestoreAcceptedAnswers => NoteAcceptedAnswers;
}
