//! Anki packages: `.apkg` (decks) and `.colpkg` (whole collection).
//!
//! **Reading** supports every package layout Anki has produced:
//!
//! | Layout | Collection entry | Note types / decks | Media list | Media files |
//! |---|---|---|---|---|
//! | Legacy 1 (Anki 2.0) | `collection.anki2` | JSON in `col` | JSON `{"0": "a.jpg"}` | plain |
//! | Legacy 2 (2.1, schema 11) | `collection.anki21` | JSON in `col` | JSON | plain |
//! | Latest (2.1.50+, schema 18) | `collection.anki21b` (zstd) | `notetypes`/`fields`/`templates`/`decks` tables, protobuf configs | zstd protobuf | zstd |
//!
//! **Writing** produces the legacy schema-11 layout (`collection.anki2` + JSON media list),
//! which every Anki version since 2.0 imports.
//!
//! The raw structures below mirror Anki's tables; [`convert`] maps them to and from a
//! [`Pack`](crate::pack::Pack).

pub mod convert;
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
#[cfg(any(test, feature = "fixtures"))]
pub mod hostile;
pub mod occlusion;
mod proto;
mod read;
pub mod schema;
pub mod template;
mod write;

pub use convert::{from_pack, to_pack, AnkiImportReport};
pub use read::{read_package, read_package_with_abort, QUERY_TIME_BUDGET};
pub use write::write_package;

/// Which package layout a file used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnkiLayout {
    /// `collection.anki2`.
    Legacy1,
    /// `collection.anki21`.
    Legacy2,
    /// `collection.anki21b`.
    Latest,
}

/// A card template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiTemplate {
    /// Name.
    pub name: String,
    /// Question format.
    pub qfmt: String,
    /// Answer format.
    pub afmt: String,
}

/// A note type ("model").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiNotetype {
    /// Model id (creation time, ms).
    pub id: i64,
    /// Name.
    pub name: String,
    /// `0` standard, `1` cloze.
    pub kind: i64,
    /// Index of the sort field.
    pub sort_field: usize,
    /// Styling.
    pub css: String,
    /// Field names, in order.
    pub fields: Vec<String>,
    /// Templates, in order (`ord`).
    pub templates: Vec<AnkiTemplate>,
}

/// A deck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiDeck {
    /// Deck id.
    pub id: i64,
    /// Full name with `::` separators.
    pub name: String,
    /// Description.
    pub description: String,
    /// Filtered ("dynamic") deck.
    pub filtered: bool,
}

/// A note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiNote {
    /// Id (creation time, ms).
    pub id: i64,
    /// Globally unique id (stable across collections).
    pub guid: String,
    /// Model id.
    pub mid: i64,
    /// Modification time (s).
    pub modified: i64,
    /// Tags.
    pub tags: Vec<String>,
    /// Field values, in model field order.
    pub fields: Vec<String>,
}

/// A card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiCard {
    /// Id (creation time, ms).
    pub id: i64,
    /// Note id.
    pub nid: i64,
    /// Deck id.
    pub did: i64,
    /// Template index (cloze: cloze number − 1).
    pub ord: i64,
    /// Modification time (s).
    pub modified: i64,
    /// 0 new, 1 learning, 2 review, 3 relearning.
    pub ctype: i64,
    /// −3/−2 buried, −1 suspended, 0 new, 1 learning, 2 review, 3 day-learning, 4 preview.
    pub queue: i64,
    /// New: position; learning: epoch s; review/day-learning: days since collection creation.
    pub due: i64,
    /// Interval (days; negative = seconds).
    pub ivl: i64,
    /// Ease factor (permille).
    pub factor: i64,
    /// Reviews.
    pub reps: i64,
    /// Lapses.
    pub lapses: i64,
    /// Learning steps left.
    pub left: i64,
    /// Original due (card in a filtered deck).
    pub odue: i64,
    /// Original deck (card in a filtered deck).
    pub odid: i64,
    /// Flags (low 3 bits = colour flag).
    pub flags: i64,
    /// Extra JSON (`{"s":…,"d":…}` FSRS memory state in recent versions).
    pub data: String,
}

/// A review log row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiRevlog {
    /// Id (review time, ms).
    pub id: i64,
    /// Card id.
    pub cid: i64,
    /// 1 again … 4 easy (0 = manual reschedule).
    pub ease: i64,
    /// New interval (days; negative = seconds).
    pub ivl: i64,
    /// Previous interval.
    pub last_ivl: i64,
    /// Ease factor.
    pub factor: i64,
    /// Answer time (ms).
    pub time: i64,
    /// 0 learn, 1 review, 2 relearn, 3 filtered, 4 manual.
    pub rtype: i64,
}

/// Everything read from a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiCollection {
    /// Layout of the file.
    pub layout: AnkiLayout,
    /// Collection creation time (s); review due days count from here.
    pub crt: i64,
    /// Note types.
    pub notetypes: Vec<AnkiNotetype>,
    /// Decks.
    pub decks: Vec<AnkiDeck>,
    /// Notes.
    pub notes: Vec<AnkiNote>,
    /// Cards.
    pub cards: Vec<AnkiCard>,
    /// Review history.
    pub revlog: Vec<AnkiRevlog>,
    /// `(file name, bytes)` of media files.
    pub media: Vec<(String, Vec<u8>)>,
}

#[cfg(test)]
mod real_tests;
#[cfg(test)]
mod tests;
