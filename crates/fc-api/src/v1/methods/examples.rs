//! The most teachable scene (v1.4 W3): ranked examples with reasons. Queries only.

use crate::v1::examples::*;

methods! {
    /// v1.4: a Subtitle note's encounters ranked as examples, best first, with reasons and
    /// near-duplicates.
    SubtitleEncountersRank = "subtitle.encounters.rank", Query, "examples", "subtitle:read", Public, undo: false, RankEncounters => RankedExamples;
    /// v1.4: proposes a better example for the card (never switches; apply it with
    /// `subtitle.encounters.set_primary`).
    SubtitleEncountersSuggestPrimary = "subtitle.encounters.suggest_primary", Query, "examples", "subtitle:read", Public, undo: false, RankEncounters => PrimarySuggestion;
    /// v1.4: candidate sentences for a word (reading mode) ranked, with reasons.
    ExamplesRankSentences = "examples.rank_sentences", Query, "examples", "reading:read", Public, undo: false, RankSentences => RankedExamples;
}
