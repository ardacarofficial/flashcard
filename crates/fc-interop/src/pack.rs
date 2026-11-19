//! The in-memory package every structured import produces and every structured export
//! consumes: `fc-core` models (with ids) plus media bytes and review history.
//!
//! * `.fcpack` is a [`Pack`] serialized as-is, so classification definitions, options and
//!   values survive exactly (same ids).
//! * Anki packages are converted into a [`Pack`] with ids derived deterministically from the
//!   Anki ids ([`derived_id`]): importing the same file twice finds the same notes again.
//!
//! The app applies a pack in one command (one undo step): decks are matched by path, entities
//! keep their ids when free, duplicates follow the chosen policy.

use fc_core::attribute::{AttributeDef, AttributeValue, EntityRef};
use fc_core::card::{Card, CardState, Rating};
use fc_core::deck::Deck;
use fc_core::id::uuid_v7_from_parts;
use fc_core::note::Note;
use fc_core::notetype::NoteType;
use fc_core::{AttributeDefId, CardId, ReviewLogId, SessionId, Timestamp};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// A classification value on a note or card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackValue {
    /// Definition.
    pub attr_id: AttributeDefId,
    /// Note or card.
    pub entity: EntityRef,
    /// Value.
    pub value: AttributeValue,
}

/// One `review_log` row (immutable history; docs/03 §4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewEntry {
    /// Id.
    pub id: ReviewLogId,
    /// Card.
    pub card_id: CardId,
    /// Session (if any).
    pub session_id: Option<SessionId>,
    /// Answer.
    pub rating: Rating,
    /// State before the answer.
    pub state_before: CardState,
    /// State after the answer.
    pub state_after: CardState,
    /// Interval chosen (days).
    pub scheduled_days: i64,
    /// Days since the previous review.
    pub elapsed_days: i64,
    /// Time on the card (ms).
    pub duration_ms: i64,
    /// Time to answer (ms).
    pub answer_duration_ms: Option<i64>,
    /// Hints shown.
    pub hints_used: u32,
    /// `normal`, `preview`, …
    pub mode: String,
    /// FSRS stability after the review.
    pub stability_after: Option<f64>,
    /// FSRS difficulty after the review.
    pub difficulty_after: Option<f64>,
    /// When.
    pub created_at: Timestamp,
}

/// A media file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaFile {
    /// SHA-256 hex of `bytes`.
    pub hash: String,
    /// MIME type.
    pub mime: String,
    /// Original file name, if known.
    pub name: Option<String>,
    /// Content (stored as a separate zip entry in packages, not in JSON).
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

/// See the module docs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Pack {
    /// Decks; a deck whose `parent_id` is not in the pack is a root of the package.
    pub decks: Vec<Deck>,
    /// Note types used by the notes.
    pub notetypes: Vec<NoteType>,
    /// Notes.
    pub notes: Vec<Note>,
    /// Cards.
    pub cards: Vec<Card>,
    /// Classification definitions (with options) the values refer to.
    pub attribute_defs: Vec<AttributeDef>,
    /// Classification values on the notes/cards above.
    pub attribute_values: Vec<PackValue>,
    /// Review history of the cards above (empty when exported without history).
    pub review_logs: Vec<ReviewEntry>,
    /// Media referenced by the notes.
    pub media: Vec<MediaFile>,
}

/// Counts for reports and previews.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackCounts {
    /// Decks.
    pub decks: usize,
    /// Note types.
    pub notetypes: usize,
    /// Notes.
    pub notes: usize,
    /// Cards.
    pub cards: usize,
    /// Review log entries.
    pub review_logs: usize,
    /// Media files.
    pub media: usize,
    /// Classification definitions.
    pub attribute_defs: usize,
}

impl Pack {
    /// Counts of each kind.
    pub fn counts(&self) -> PackCounts {
        PackCounts {
            decks: self.decks.len(),
            notetypes: self.notetypes.len(),
            notes: self.notes.len(),
            cards: self.cards.len(),
            review_logs: self.review_logs.len(),
            media: self.media.len(),
            attribute_defs: self.attribute_defs.len(),
        }
    }

    /// Full `A::B` path segments of `deck` inside the pack (stops at the package root).
    pub fn deck_path(&self, deck: fc_core::DeckId) -> Vec<String> {
        let mut segments = Vec::new();
        let mut current = self.decks.iter().find(|d| d.id == deck);
        let mut guard = 0;
        while let Some(d) = current {
            segments.push(d.name.clone());
            guard += 1;
            if guard > 256 {
                break;
            }
            current = d
                .parent_id
                .and_then(|p| self.decks.iter().find(|x| x.id == p));
        }
        segments.reverse();
        segments
    }
}

/// A UUIDv7 derived deterministically from a foreign id: the timestamp part is `millis`
/// (Anki ids are creation times in ms), the random part a hash of `salt` (`kind:anki-id:…`).
/// The same input always yields the same id, so re-importing finds existing rows.
pub fn derived_id(millis: i64, salt: &str) -> Uuid {
    let digest = Sha256::digest(salt.as_bytes());
    let mut random = [0u8; 10];
    random.copy_from_slice(&digest[..10]);
    let ms = millis.clamp(0, 0xFFFF_FFFF_FFFF);
    uuid_v7_from_parts(Timestamp(ms), &random)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_ids_are_stable_v7() {
        let a = derived_id(1_700_000_000_000, "note:x");
        assert_eq!(a, derived_id(1_700_000_000_000, "note:x"));
        assert_ne!(a, derived_id(1_700_000_000_000, "note:y"));
        assert_eq!(a.get_version_num(), 7);
        assert_eq!(derived_id(-5, "x").get_version_num(), 7);
    }
}
