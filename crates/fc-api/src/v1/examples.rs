//! The most teachable scene (v1.4 W3, docs/03 §4.4): a word's example sentences ranked, each
//! with the reasons for its place (`fc_core::encounter_score`). Queries only: switching the
//! example on the card is `subtitle.encounters.set_primary` (undoable; the schedule stays).

use fc_core::encounter_score::SceneReason;
use fc_core::NoteId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `subtitle.encounters.rank` / `subtitle.encounters.suggest_primary`: a note by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RankEncounters {
    /// A Subtitle note.
    pub note_id: NoteId,
}

/// One ranked example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RankedExample {
    /// The encounter's capture id (`subtitle.encounters.*`), or the index of a candidate
    /// sentence (`examples.rank_sentences`) as text.
    pub id: String,
    /// Place, 1 = best.
    pub rank: u32,
    /// Sum of the reasons' points (higher = more teachable).
    pub score: i32,
    /// The example shown on the card now (encounters only).
    pub is_primary: bool,
    /// The sentence.
    pub sentence: String,
    /// The target word really occurs in it.
    pub target_found: bool,
    /// Unknown words besides the target (names excluded), as written.
    pub unknown_words: Vec<String>,
    /// Why, each with its points.
    pub reasons: Vec<SceneReason>,
    /// Ids of near-duplicate examples (same line with other punctuation/case, overlapping
    /// subtitle lines), best ranked first.
    pub duplicates: Vec<String>,
}

/// Ranked examples, best first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct RankedExamples {
    /// The note (encounters only).
    pub note_id: Option<NoteId>,
    /// Index language the words were looked up in.
    pub language: String,
    /// The word the examples are for.
    pub target: String,
    /// A frequency pack covers the language (else names are only recognised by capitals and
    /// no frequency reasons are given).
    pub frequency_pack: bool,
    /// Best first.
    pub items: Vec<RankedExample>,
}

/// Response of `subtitle.encounters.suggest_primary`: a proposal, never applied by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct PrimarySuggestion {
    /// The note.
    pub note_id: NoteId,
    /// The encounter shown on the card now.
    pub current: Option<String>,
    /// The better encounter to show (`subtitle.encounters.set_primary` with it), or `null`
    /// when the current one is the best (or not beaten by a clear margin).
    pub suggested: Option<String>,
    /// Score gained by switching (0 without a suggestion).
    pub gain: i32,
    /// The whole ranking.
    pub ranking: RankedExamples,
}

/// A candidate sentence of `examples.rank_sentences`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct CandidateSentence {
    /// The sentence.
    pub text: String,
    /// The word's UTF-16 range in `text`, when known (checked, not trusted).
    #[serde(default)]
    pub term_offset: Option<(i64, i64)>,
    /// The word as written in it.
    #[serde(default)]
    pub surface: Option<String>,
    /// Its sound is available.
    #[serde(default)]
    pub has_audio: Option<bool>,
    /// Its picture is available.
    #[serde(default)]
    pub has_image: Option<bool>,
}

/// `examples.rank_sentences`: candidate sentences for a word (reading mode, a pasted text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RankSentences {
    /// Language (BCP-47; empty = the reading-mode default).
    #[serde(default)]
    pub language: String,
    /// The word or expression (`pick up`).
    pub target: String,
    /// 1–200 sentences.
    pub sentences: Vec<CandidateSentence>,
}
