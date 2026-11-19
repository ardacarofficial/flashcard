//! Resolved filter: the query with every name turned into ids, numbers and times. This is what
//! storage compiles to SQL (`fc-storage`) and what [`super::eval`] evaluates in memory.

use super::ast::CmpOp;
use crate::attribute::AttributeTarget;
use crate::card::CardState;
use crate::id::{AttributeDefId, AttributeOptionId, CardId, DeckId, NoteId, NoteTypeId, TagId};
use crate::time::Timestamp;

/// A boolean filter over (card, note) pairs. Trashed cards/notes are excluded by the backends
/// regardless of the filter.
#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    /// Matches everything.
    All,
    /// Matches nothing.
    Nothing,
    /// All must hold.
    And(Vec<Filter>),
    /// Any must hold.
    Or(Vec<Filter>),
    /// Negation.
    Not(Box<Filter>),
    /// A predicate.
    Pred(Pred),
}

/// Numeric card property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardNum {
    /// FSRS stability (days); cards without one never match.
    Stability,
    /// FSRS difficulty; cards without one never match.
    Difficulty,
    /// Lapses.
    Lapses,
    /// Reps.
    Reps,
}

/// Time column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeCol {
    /// Card creation (`added:`).
    CardCreated,
    /// Note modification (`edited:`).
    NoteEdited,
    /// Last review (`reviewed:`).
    LastReview,
    /// Due time of a scheduled (non-new) card (`due:`).
    Due,
}

/// Test applied to the stored rows of one classification.
#[derive(Debug, Clone, PartialEq)]
pub enum AttrTest {
    /// Has any value.
    Any,
    /// Has one of these options (enum kinds).
    Options(Vec<AttributeOptionId>),
    /// Number comparison (`value_num <op> n`).
    Number(CmpOp, f64),
    /// Text glob (folded, whole value).
    Text(String),
    /// Date comparison on `YYYY-MM-DD` text.
    Date(CmpOp, String),
    /// Bool equality.
    Bool(bool),
}

/// One classification alternative (a key may be defined by several definitions).
#[derive(Debug, Clone, PartialEq)]
pub struct AttrAlt {
    /// Definition.
    pub attr_id: AttributeDefId,
    /// Values live on the note or the card.
    pub target: AttributeTarget,
    /// Test.
    pub test: AttrTest,
}

/// An atomic condition.
#[derive(Debug, Clone, PartialEq)]
pub enum Pred {
    /// Full-text: consecutive folded tokens in the note's indexed text; the last one as a
    /// prefix when `prefix`.
    Text {
        /// Folded tokens (non-empty).
        tokens: Vec<String>,
        /// Last token is a prefix.
        prefix: bool,
    },
    /// Whole-text `*` glob over all fields (stripped, folded); used for words with inner
    /// wildcards.
    TextGlob(String),
    /// Some of these fields matches the glob as a whole.
    Field {
        /// Field keys (across note types).
        keys: Vec<String>,
        /// Glob source ([`super::text::Glob::source`]).
        glob: String,
        /// Every field with one of these keys is full-text indexed, so backends may narrow
        /// candidates with [`super::text::glob_index_tokens`] first.
        indexed: bool,
    },
    /// Case-insensitive regex over the text of `keys` (all fields when `None`).
    Regex {
        /// Field keys.
        keys: Option<Vec<String>>,
        /// Pattern (validated).
        pattern: String,
    },
    /// Card in one of these decks (sub-decks already expanded).
    Decks(Vec<DeckId>),
    /// Note has one of these tags (sub-tags already expanded).
    Tags(Vec<TagId>),
    /// Note has no tags.
    Untagged,
    /// Note of one of these types.
    NoteTypes(Vec<NoteTypeId>),
    /// Card type engine id (lowercase).
    CardType(String),
    /// Template key (folded).
    Template(String),
    /// Learning state.
    State(CardState),
    /// Due for study now (scheduled, due ≤ now, not suspended, not buried).
    DueNow,
    /// Suspended.
    Suspended,
    /// Buried until after now.
    Buried,
    /// Any flag set.
    Flagged,
    /// This flag bit (0-based) set.
    FlagBit(u32),
    /// Card numeric property.
    Num(CardNum, CmpOp, f64),
    /// Retrievability comparison, expressed on `x = elapsed_days / stability`:
    /// `x <op> threshold` (see [`retrievability_threshold`]).
    Elapsed(CmpOp, f64),
    /// Time column `>=` / `<=` a point in time.
    Time(TimeCol, CmpOp, Timestamp),
    /// Classification.
    Attr(Vec<AttrAlt>),
    /// Notes by id.
    NoteIds(Vec<NoteId>),
    /// Cards by id.
    CardIds(Vec<CardId>),
}

/// FSRS forgetting curve factor: `R(t, S) = (1 + F·t/S)^-0.5` with `F = 19/81`.
pub const FSRS_FACTOR: f64 = 19.0 / 81.0;

/// Threshold on `x = t/S` equivalent to `R = r` (`R` decreases as `x` grows).
pub fn retrievability_threshold(r: f64) -> f64 {
    (1.0 / (r * r) - 1.0) / FSRS_FACTOR
}

impl Filter {
    /// `And` with trivial cases simplified.
    pub fn and(items: Vec<Filter>) -> Filter {
        let mut out = Vec::new();
        for f in items {
            match f {
                Filter::All => {}
                Filter::Nothing => return Filter::Nothing,
                Filter::And(inner) => out.extend(inner),
                other => out.push(other),
            }
        }
        match out.len() {
            0 => Filter::All,
            1 => out.remove(0),
            _ => Filter::And(out),
        }
    }

    /// `Or` with trivial cases simplified.
    pub fn or(items: Vec<Filter>) -> Filter {
        let mut out = Vec::new();
        for f in items {
            match f {
                Filter::Nothing => {}
                Filter::All => return Filter::All,
                Filter::Or(inner) => out.extend(inner),
                other => out.push(other),
            }
        }
        match out.len() {
            0 => Filter::Nothing,
            1 => out.remove(0),
            _ => Filter::Or(out),
        }
    }

    /// Negation with trivial cases simplified.
    pub fn negate(f: Filter) -> Filter {
        match f {
            Filter::All => Filter::Nothing,
            Filter::Nothing => Filter::All,
            Filter::Not(inner) => *inner,
            other => Filter::Not(Box::new(other)),
        }
    }
}
