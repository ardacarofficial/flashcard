//! Accepted answers (v1.4): per note and field, other answers the offline written-answer check
//! also accepts ("give up" → `vazgeçmek` and `bırakmak`). No model is asked at study time:
//! the list is data the user (or an applied AI meaning) put there.

use fc_core::NoteId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A note field's accepted answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AcceptedAnswers {
    /// Field key (the answer field the card asks for, e.g. `meaning`).
    pub field: String,
    /// Answers, in the order they were added.
    pub answers: Vec<String>,
}

/// Response of the accepted-answers methods: a note's lists, by field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NoteAcceptedAnswers {
    /// Note.
    pub note_id: NoteId,
    /// Lists, by field key (fields without answers are left out).
    pub fields: Vec<AcceptedAnswers>,
}

/// A note by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AcceptedAnswersRef {
    /// Note.
    pub note_id: NoteId,
}

/// `accepted_answers.add` / `accepted_answers.remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct EditAcceptedAnswers {
    /// Note.
    pub note_id: NoteId,
    /// Field key (`null` = the note type's meaning field).
    #[serde(default)]
    pub field: Option<String>,
    /// Answers to add (blank and duplicate ones are skipped) or remove.
    pub answers: Vec<String>,
}

/// Internal `accepted_answers.restore`: a note's lists exactly (inverse of add/remove).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreAcceptedAnswers {
    /// Note.
    pub note_id: NoteId,
    /// Every list of the note (empty = none).
    pub fields: Vec<AcceptedAnswers>,
}
