//! Reading mode, subtitles and incremental reading (v0.10).

use crate::v1::jobs::JobDto;
use crate::v1::notes::NoteDto;
use crate::v1::reading::*;

methods! {
    // ---- items ----
    /// Reading items (or the reading queue).
    ReadingItemsList = "reading.items.list", Query, "reading", "reading:read", Public, undo: false, ListReadingItems => ReadingItemList;
    /// One item.
    ReadingItemsGet = "reading.items.get", Query, "reading", "reading:read", Public, undo: false, ReadingItemRef => ReadingItemDto;
    /// Saves a text (or parsed subtitles) for reading.
    ReadingItemsCreate = "reading.items.create", Command, "reading", "reading:write", Public, undo: true, CreateReadingItem => ReadingItemDto;
    /// Reads a file or web page in a background job that creates the item.
    ReadingItemsImport = "reading.items.import", Command, "reading", "reading:write", Public, undo: false, ImportReadingItem => JobDto;
    /// Edits title, language, media path.
    ReadingItemsUpdate = "reading.items.update", Command, "reading", "reading:write", Public, undo: true, UpdateReadingItem => ReadingItemDto;
    /// Remembers the reading position.
    ReadingItemsSetPosition = "reading.items.set_position", Command, "reading", "reading:write", Public, undo: true, SetReadingPosition => ReadingItemDto;
    /// Moves an item to the trash.
    ReadingItemsDelete = "reading.items.delete", Command, "reading", "reading:write", Public, undo: true, ReadingItemRef => ReadingItemDto;
    /// Internal inverse of delete.
    ReadingItemsUndelete = "reading.items.undelete", Command, "reading", "reading:write", Internal, undo: true, ReadingItemRef => ReadingItemDto;
    /// Internal inverse of the item/queue commands: puts every mutable column back.
    ReadingItemsRestoreState = "reading.items.restore_state", Command, "reading", "reading:write", Internal, undo: true, ReadingItemState => ReadingItemDto;

    // ---- queue (incremental reading) ----
    /// Adds an item to the reading queue or takes it out.
    ReadingQueueSet = "reading.queue.set", Command, "reading", "reading:write", Public, undo: true, SetReadingQueue => ReadingItemDto;
    /// Done reading for now: schedules the next session.
    ReadingQueueRead = "reading.queue.read", Command, "reading", "reading:write", Public, undo: true, ReadingSessionDone => ReadingItemDto;
    /// Postpones a queued item.
    ReadingQueuePostpone = "reading.queue.postpone", Command, "reading", "reading:write", Public, undo: true, PostponeReading => ReadingItemDto;

    // ---- extracts ----
    /// Turns a selection into an extract (queued).
    ReadingExtractsCreate = "reading.extracts.create", Command, "reading", "reading:write", Public, undo: true, CreateExtract => ReadingItemDto;
    /// A cloze note from an item's text.
    ReadingExtractsCloze = "reading.extracts.cloze", Command, "reading", "notes:write", Public, undo: true, ClozeFromItem => NoteDto;

    // ---- words ----
    /// Word colours and statistics of a text.
    ReadingAnalyze = "reading.analyze", Query, "reading", "reading:read", Public, undo: false, AnalyzeText => TextAnalysis;
    /// Marks words as known / ignored (or clears the mark).
    ReadingWordsMark = "reading.words.mark", Command, "reading", "reading:write", Public, undo: true, MarkWords => WordMarksChanged;
    /// Internal inverse of mark.
    ReadingWordsRestore = "reading.words.restore", Command, "reading", "reading:write", Internal, undo: true, RestoreWordMarks => WordMarksChanged;
    /// Status, mark and cards of a word.
    ReadingWordsLookup = "reading.words.lookup", Query, "reading", "reading:read", Public, undo: false, LookupWord => WordInfo;

    // ---- cards ----
    /// A word or sentence card from what is being read.
    ReadingCardsMake = "reading.cards.make", Command, "reading", "notes:write", Public, undo: true, MakeReadingCard => NoteDto;
    /// Cuts a cue's audio (and a screenshot) from the item's video (background job).
    ReadingClipsExtract = "reading.clips.extract", Command, "reading", "media:write", Public, undo: false, ExtractClip => JobDto;
}
