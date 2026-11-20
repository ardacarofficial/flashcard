//! Reading mode, subtitle mining and incremental reading (docs/02 §24–§26, v0.10).
//!
//! **Offsets are UTF-16 code units** (JavaScript string indices) everywhere in this API:
//! token and sentence ranges, reading positions, extract ranges and the target word of a
//! card. The backend converts them to byte offsets.

use crate::v1::cardtypes::MediaDto;
use fc_core::card::CardState;
use fc_core::reading::{ReadingPriority, TextSummary, WordMark, WordStatus};
use fc_core::{CardId, DeckId, NoteId, ReadingItemId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ------------------------------------------------------------------ items

/// What a reading item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ReadingItemKind {
    /// Pasted text, a file, a web page.
    Text,
    /// Subtitles: one cue per line of the body, with timings.
    Subtitle,
    /// A part of another item, queued on its own (incremental reading).
    Extract,
}

impl ReadingItemKind {
    /// Storage string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Subtitle => "subtitle",
            Self::Extract => "extract",
        }
    }

    /// From its storage string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "text" => Some(Self::Text),
            "subtitle" => Some(Self::Subtitle),
            "extract" => Some(Self::Extract),
            _ => None,
        }
    }
}

/// One subtitle cue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CueDto {
    /// Start, ms.
    pub start_ms: u64,
    /// End, ms.
    pub end_ms: u64,
    /// Text (one line).
    pub text: String,
}

/// A reading item (without its text; see `reading.analyze`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ReadingItemDto {
    /// Id.
    pub id: ReadingItemId,
    /// Kind.
    pub kind: ReadingItemKind,
    /// Extracts: the item it was taken from.
    pub parent_id: Option<ReadingItemId>,
    /// Title.
    pub title: String,
    /// BCP-47 language.
    pub language: String,
    /// URL or file name it came from.
    pub source: Option<String>,
    /// Local video/audio of a subtitle (clips for cards).
    pub media_path: Option<String>,
    /// Text length (UTF-16 units).
    pub length: u32,
    /// Read up to here (UTF-16 units).
    pub position: u32,
    /// `position / length`, 0–100.
    pub progress_pct: f64,
    /// Cues (subtitles).
    pub cue_count: u32,
    /// In the reading queue.
    pub queued: bool,
    /// Queue priority.
    pub priority: ReadingPriority,
    /// When it comes back (queued items).
    pub due: Option<Timestamp>,
    /// Queued and due now.
    pub is_due: bool,
    /// Last queue interval, days.
    pub interval_days: u32,
    /// Reading sessions done.
    pub read_count: u32,
    /// Last reading session.
    pub last_read: Option<Timestamp>,
    /// Created.
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
}

/// `reading.items.list`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ListReadingItems {
    /// Only the reading queue: due items first — by priority, then longest due — then the
    /// rest by due time; otherwise every item, newest first.
    #[serde(default)]
    pub queued_only: bool,
}

/// Response of `reading.items.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ReadingItemList {
    /// Items.
    pub items: Vec<ReadingItemDto>,
    /// Queued items due now.
    pub due_count: u32,
}

/// An item by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ReadingItemRef {
    /// Id.
    pub id: ReadingItemId,
}

/// `reading.items.create`: a text (or already parsed subtitles) to read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreateReadingItem {
    /// Title (empty = the first words of the text).
    #[serde(default)]
    pub title: String,
    /// BCP-47 language (empty = the `reading.default_language` setting).
    #[serde(default)]
    pub language: String,
    /// The text. Ignored when `cues` are given (the body is then one cue per line).
    #[serde(default)]
    pub body: String,
    /// Subtitle cues: makes a subtitle item.
    #[serde(default)]
    pub cues: Option<Vec<CueDto>>,
    /// URL or file name.
    #[serde(default)]
    pub source: Option<String>,
    /// Local video/audio next to subtitles (absolute path).
    #[serde(default)]
    pub media_path: Option<String>,
    /// Put it in the reading queue (due now).
    #[serde(default)]
    pub queue: bool,
    /// Queue priority.
    #[serde(default)]
    pub priority: ReadingPriority,
}

/// Where `reading.items.import` reads from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReadingSource {
    /// A local file (absolute path): `.srt`/`.vtt` subtitles, `.pdf`, `.html`, or plain text
    /// in any encoding the importer understands.
    File {
        /// Absolute path.
        path: String,
    },
    /// A web page (readable text).
    Url {
        /// `http(s)://` address.
        url: String,
    },
}

/// `reading.items.import`: reads a file or web page in a background job (`reading.import`),
/// which then creates the item. The job result is the new [`ReadingItemDto`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImportReadingItem {
    /// Source.
    pub source: ReadingSource,
    /// Title (empty = the document title or file name).
    #[serde(default)]
    pub title: String,
    /// Language (empty = the default).
    #[serde(default)]
    pub language: String,
    /// Subtitles: the video/audio they belong to (absolute path), for clips.
    #[serde(default)]
    pub media_path: Option<String>,
    /// Queue it.
    #[serde(default)]
    pub queue: bool,
    /// Queue priority.
    #[serde(default)]
    pub priority: ReadingPriority,
}

/// `reading.items.update`: replaces the editable details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateReadingItem {
    /// Item.
    pub id: ReadingItemId,
    /// Title (non-empty).
    pub title: String,
    /// Language.
    pub language: String,
    /// Video/audio path (`null` = none).
    #[serde(default)]
    pub media_path: Option<String>,
}

/// `reading.items.set_position`: remembers how far the user read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetReadingPosition {
    /// Item.
    pub id: ReadingItemId,
    /// UTF-16 offset (clamped to the text).
    pub position: u32,
}

/// Every mutable column of an item: the internal inverse of the item commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ReadingItemState {
    /// Item.
    pub id: ReadingItemId,
    /// Title.
    pub title: String,
    /// Language.
    pub language: String,
    /// Video/audio path.
    pub media_path: Option<String>,
    /// Position (UTF-16).
    pub position: i64,
    /// Queued.
    pub queued: bool,
    /// Priority.
    pub priority: ReadingPriority,
    /// Due.
    pub due: Option<Timestamp>,
    /// Interval, days.
    pub interval_days: i64,
    /// Sessions.
    pub read_count: i64,
    /// Last session.
    pub last_read: Option<Timestamp>,
    /// Updated.
    pub updated_at: Timestamp,
}

// ------------------------------------------------------------------ queue

/// `reading.queue.set`: adds an item to the reading queue (due now) or takes it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetReadingQueue {
    /// Item.
    pub id: ReadingItemId,
    /// In the queue.
    pub queued: bool,
    /// Priority (`null` = keep).
    #[serde(default)]
    pub priority: Option<ReadingPriority>,
}

/// `reading.queue.read`: "done for today" — schedules the next session
/// (`fc_core::reading::next_reading_interval`) and remembers the position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ReadingSessionDone {
    /// Item.
    pub id: ReadingItemId,
    /// Read up to here (UTF-16; `null` = keep).
    #[serde(default)]
    pub position: Option<u32>,
}

/// `reading.queue.postpone`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PostponeReading {
    /// Item.
    pub id: ReadingItemId,
    /// Days from now (1–365).
    pub days: u32,
}

// ------------------------------------------------------------------ extracts

/// `reading.extracts.create`: the selected part of an item becomes an extract, queued with
/// high priority (SuperMemo-style incremental reading).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreateExtract {
    /// Source item.
    pub item_id: ReadingItemId,
    /// UTF-16 start.
    pub start: u32,
    /// UTF-16 end (exclusive).
    pub end: u32,
    /// Title (empty = the first words).
    #[serde(default)]
    pub title: String,
}

/// `reading.extracts.cloze`: a cloze note from (a part of) an item's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClozeFromItem {
    /// Item (usually an extract).
    pub item_id: ReadingItemId,
    /// The text range to put on the card (`null` = the whole item).
    #[serde(default)]
    pub range: Option<[u32; 2]>,
    /// Ranges to hide (UTF-16, inside `range`/the item).
    pub hide: Vec<[u32; 2]>,
    /// All hidden parts on one card (`c1`) instead of one card each.
    #[serde(default)]
    pub together: bool,
    /// Deck.
    pub deck_id: DeckId,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
}

// ------------------------------------------------------------------ analysis

/// `reading.analyze`: colours the words of an item or of a text.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AnalyzeText {
    /// A saved item…
    #[serde(default)]
    pub item_id: Option<ReadingItemId>,
    /// …or a text (pasted, not saved).
    #[serde(default)]
    pub text: Option<String>,
    /// Language of `text` (empty = default).
    #[serde(default)]
    pub language: Option<String>,
    /// Only return tokens and sentences of this UTF-16 range (long texts are read in
    /// pages); the summary always covers the whole text.
    #[serde(default)]
    pub range: Option<[u32; 2]>,
    /// Client UTC offset in minutes (study days of card intervals, as in statistics).
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// A word of the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordToken {
    /// UTF-16 start.
    pub start: u32,
    /// UTF-16 end.
    pub end: u32,
    /// Lemma (the key of the word: marks and cards match on it).
    pub lemma: String,
    /// Colour.
    pub status: WordStatus,
}

/// Response of `reading.analyze`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TextAnalysis {
    /// The analysed text (the item's body, or the given text).
    pub text: String,
    /// Language used.
    pub language: String,
    /// Lemmatizer used (`builtin.en`, `builtin.tr`, `builtin.plain`).
    pub lemmatizer: String,
    /// Words (of `range` if given), in text order.
    pub tokens: Vec<WordToken>,
    /// Sentence ranges (of `range` if given).
    pub sentences: Vec<[u32; 2]>,
    /// Subtitle cues (subtitle items; line `i` of `text` is cue `i`).
    pub cues: Vec<CueDto>,
    /// Statistics of the whole text.
    pub summary: TextSummary,
}

// ------------------------------------------------------------------ words

/// `reading.words.mark`: marks words without cards as known / ignored, or clears the mark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MarkWords {
    /// Language.
    pub language: String,
    /// Lemmas (1–5000; e.g. "mark every new word on this page as known").
    pub lemmas: Vec<String>,
    /// Mark (`null` = clear).
    pub mark: Option<WordMark>,
}

/// One previous mark (the internal inverse of `reading.words.mark`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordMarkEntry {
    /// Lemma.
    pub lemma: String,
    /// Mark (`null` = none).
    pub mark: Option<WordMark>,
}

/// `reading.words.restore` (internal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreWordMarks {
    /// Language.
    pub language: String,
    /// Marks to put back.
    pub marks: Vec<WordMarkEntry>,
}

/// Response of `reading.words.mark`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordMarksChanged {
    /// Marks that changed.
    pub changed: u32,
}

/// `reading.words.lookup`: what the collection knows about a word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct LookupWord {
    /// Language.
    pub language: String,
    /// Lemma (as in a [`WordToken`]).
    pub lemma: String,
    /// Client UTC offset in minutes.
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// A card of a word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordCard {
    /// Card.
    pub card_id: CardId,
    /// Note.
    pub note_id: NoteId,
    /// Headword as written on the card (plain text).
    pub word: String,
    /// The note's `meaning` (or `back`/`translation`) field as plain text.
    pub meaning: Option<String>,
    /// State.
    pub state: CardState,
    /// Interval in days (review cards).
    pub interval_days: Option<u32>,
    /// Mature (counts the word as known).
    pub mature: bool,
}

/// Response of `reading.words.lookup`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordInfo {
    /// Lemma.
    pub lemma: String,
    /// Colour.
    pub status: WordStatus,
    /// The user's mark (only used while there is no card).
    pub mark: Option<WordMark>,
    /// Cards of the word.
    pub cards: Vec<WordCard>,
}

// ------------------------------------------------------------------ cards

/// Which built-in note type a card from reading uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ReadingCardKind {
    /// "Vocabulary": word, meaning, the sentence as example.
    Word,
    /// "Sentence card": sentence (word in bold), translation, word, meaning, source.
    Sentence,
}

/// `reading.cards.make`: a card from a word in its sentence ("Kart yap").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MakeReadingCard {
    /// Note type.
    pub kind: ReadingCardKind,
    /// Deck.
    pub deck_id: DeckId,
    /// The word as it goes on the card (usually the lemma).
    pub word: String,
    /// Meaning (required for word cards).
    #[serde(default)]
    pub meaning: String,
    /// The sentence it was found in.
    #[serde(default)]
    pub sentence: String,
    /// The word's UTF-16 range inside `sentence` (shown in bold).
    #[serde(default)]
    pub word_range: Option<[u32; 2]>,
    /// Translation of the sentence (sentence cards).
    #[serde(default)]
    pub translation: String,
    /// Item it came from (title + cue time go to `source`).
    #[serde(default)]
    pub item_id: Option<ReadingItemId>,
    /// Subtitle cue index.
    #[serde(default)]
    pub cue_index: Option<u32>,
    /// Audio (`media:<hash>`, e.g. from `reading.clips.extract`).
    #[serde(default)]
    pub audio: Option<String>,
    /// Image (`media:<hash>`).
    #[serde(default)]
    pub image: Option<String>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// `reading.clips.extract`: cuts the audio of a subtitle cue (and a screenshot) from the
/// item's video with ffmpeg in a background job (`reading.clip`). The job result is a
/// [`ReadingClip`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ExtractClip {
    /// Subtitle item with a `media_path`.
    pub item_id: ReadingItemId,
    /// Cue index.
    pub cue_index: u32,
    /// Also take a screenshot from the middle of the cue.
    #[serde(default)]
    pub image: bool,
}

/// Result of a `reading.clip` job: media to put on the card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ReadingClip {
    /// The cue's audio (mp3).
    pub audio: MediaDto,
    /// Screenshot (jpeg).
    pub image: Option<MediaDto>,
    /// Clip start, ms.
    pub start_ms: u64,
    /// Clip end, ms.
    pub end_ms: u64,
}
