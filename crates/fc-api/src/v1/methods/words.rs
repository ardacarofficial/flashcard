//! The word index (v1.4). Reading it needs `words:read` (its own permission, so a plugin or
//! an MCP client is granted the words explicitly, not with all of reading mode).

use crate::v1::common::Empty;
use crate::v1::jobs::JobDto;
use crate::v1::words::*;

methods! {
    /// Status and evidence of word forms (batch): forms → key → index row.
    WordsLookup = "words.lookup", Query, "words", "words:read", Public, undo: false, LookupWords => WordsLookupResult;
    /// Index rows, filtered by language and status, paged.
    WordsList = "words.list", Query, "words", "words:read", Public, undo: false, ListWords => WordList;
    /// Words per status per language.
    WordsStats = "words.stats", Query, "words", "words:read", Public, undo: false, Empty => WordStats;
    /// Rebuilds the whole index in a background job (`words.index.rebuild`).
    WordsIndexRebuild = "words.index.rebuild", Command, "words", "reading:write", Public, undo: false, Empty => JobDto;
    /// Empties the index and builds it again in one transaction (the job's step; derived
    /// data, so nothing to undo).
    WordsIndexRebuildNow = "words.index.rebuild_now", Command, "words", "reading:write", Public, undo: false, Empty => WordIndexRebuilt;
    /// Installed word-data packs (frequency, CEFR band, families) with their sources and
    /// licences, and the packs this build ships.
    WordsDataSources = "words.data.sources", Query, "word_data", "words:read", Public, undo: false, Empty => WordDataSources;
    /// Replaces a language's word-data pack with one from a file or text (reference data:
    /// `words.data.reset` puts the shipped one back).
    WordsDataImport = "words.data.import", Command, "word_data", "reading:write", Public, undo: false, ImportWordData => WordDataPackDto;
    /// Puts the shipped word-data pack of a language back.
    WordsDataReset = "words.data.reset", Command, "word_data", "reading:write", Public, undo: false, ResetWordData => WordDataSources;
}
