//! The word index (v1.4).

use crate::v1::common::Empty;
use crate::v1::jobs::JobDto;
use crate::v1::words::*;

methods! {
    /// Status and evidence of word forms (batch): forms → key → index row.
    WordsLookup = "words.lookup", Query, "words", "reading:read", Public, undo: false, LookupWords => WordsLookupResult;
    /// Index rows, filtered by language and status, paged.
    WordsList = "words.list", Query, "words", "reading:read", Public, undo: false, ListWords => WordList;
    /// Words per status per language.
    WordsStats = "words.stats", Query, "words", "reading:read", Public, undo: false, Empty => WordStats;
    /// Rebuilds the whole index in a background job (`words.index.rebuild`).
    WordsIndexRebuild = "words.index.rebuild", Command, "words", "reading:write", Public, undo: false, Empty => JobDto;
    /// Internal: empties the index and builds it again in one transaction (the job's step).
    WordsIndexRebuildNow = "words.index.rebuild_now", Command, "words", "reading:write", Internal, undo: false, Empty => WordIndexRebuilt;
}
