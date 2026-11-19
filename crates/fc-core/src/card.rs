//! Cards (derived from notes) and their scheduling state.

use crate::id::{CardId, DeckId, NoteId};
use crate::notetype::{NoteType, NoteTypeKind};
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};

/// Learning state of a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum CardState {
    /// Never studied.
    New,
    /// In initial learning steps.
    Learning,
    /// Graduated; scheduled by the algorithm.
    Review,
    /// Lapsed; in relearning steps.
    Relearning,
}

impl CardState {
    /// Storage string (`cards.state`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Learning => "learning",
            Self::Review => "review",
            Self::Relearning => "relearning",
        }
    }

    /// Parses the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "new" => Self::New,
            "learning" => Self::Learning,
            "review" => Self::Review,
            "relearning" => Self::Relearning,
            _ => return None,
        })
    }
}

/// Answer button pressed during review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Rating {
    /// Forgot.
    Again,
    /// Recalled with difficulty.
    Hard,
    /// Recalled.
    Good,
    /// Recalled easily.
    Easy,
}

/// The scheduling-relevant subset of a card, as consumed by a `Scheduler`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchedulingState {
    /// Learning state.
    pub state: CardState,
    /// Next due time (`None` for new cards).
    pub due: Option<Timestamp>,
    /// FSRS stability (days).
    pub stability: Option<f64>,
    /// FSRS difficulty (1–10).
    pub difficulty: Option<f64>,
    /// Successful reviews.
    pub reps: u32,
    /// Times forgotten after graduating.
    pub lapses: u32,
    /// Last review time.
    pub last_review: Option<Timestamp>,
}

impl SchedulingState {
    /// State of a brand new card.
    pub const NEW: SchedulingState = SchedulingState {
        state: CardState::New,
        due: None,
        stability: None,
        difficulty: None,
        reps: 0,
        lapses: 0,
        last_review: None,
    };
}

/// A card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    /// Id.
    pub id: CardId,
    /// Source note.
    pub note_id: NoteId,
    /// Deck the card lives in (cards, not notes, belong to decks).
    pub deck_id: DeckId,
    /// Template that produced the card (`CardTemplate::key`; `c1`, `c2`… for cloze).
    pub template_key: String,
    /// Card type engine id (`classic`, `typing`, …).
    pub card_type: String,
    /// Scheduling state.
    pub scheduling: SchedulingState,
    /// Flag bitmask (meaning of bits user-configurable, v0.3).
    pub flags: u32,
    /// Excluded from study.
    pub suspended: bool,
    /// Hidden from study until this time.
    pub buried_until: Option<Timestamp>,
    /// Private per-card note.
    pub user_note: Option<String>,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
    /// Soft-delete time.
    pub deleted_at: Option<Timestamp>,
}

/// Which card to create for a note: output of card generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardSpec {
    /// Template key.
    pub template_key: String,
    /// Card type engine id.
    pub card_type: String,
}

/// v0.1 card generation for standard note types: one card per template. Cloze generation
/// belongs to the cloze `CardTypeEngine` (v0.3) and returns an empty list here.
pub fn standard_card_specs(nt: &NoteType) -> Vec<CardSpec> {
    match nt.kind {
        NoteTypeKind::Standard => nt
            .templates
            .iter()
            .map(|t| CardSpec {
                template_key: t.key.clone(),
                card_type: t.card_type.clone(),
            })
            .collect(),
        NoteTypeKind::Cloze => Vec::new(),
    }
}

impl Card {
    /// A new, never-studied card.
    pub fn new(
        id: CardId,
        note_id: NoteId,
        deck_id: DeckId,
        spec: CardSpec,
        now: Timestamp,
    ) -> Self {
        Self {
            id,
            note_id,
            deck_id,
            template_key: spec.template_key,
            card_type: spec.card_type,
            scheduling: SchedulingState::NEW,
            flags: 0,
            suspended: false,
            buried_until: None,
            user_note: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notetype::builtin;
    use uuid::Uuid;

    #[test]
    fn state_strings_roundtrip() {
        for s in [
            CardState::New,
            CardState::Learning,
            CardState::Review,
            CardState::Relearning,
        ] {
            assert_eq!(CardState::parse(s.as_str()), Some(s));
            assert_eq!(
                serde_json::to_string(&s).unwrap(),
                format!("\"{}\"", s.as_str())
            );
        }
        assert_eq!(CardState::parse("x"), None);
    }

    #[test]
    fn classic_yields_one_new_card() {
        let nt = builtin::classic(Timestamp(1), "C", "F", "B");
        let specs = standard_card_specs(&nt);
        assert_eq!(specs.len(), 1);
        let card = Card::new(
            CardId(Uuid::from_u128(1)),
            NoteId(Uuid::from_u128(2)),
            DeckId(Uuid::from_u128(3)),
            specs[0].clone(),
            Timestamp(5),
        );
        assert_eq!(card.scheduling, SchedulingState::NEW);
        assert_eq!(card.template_key, builtin::FORWARD);

        let mut cloze = nt;
        cloze.kind = NoteTypeKind::Cloze;
        assert!(standard_card_specs(&cloze).is_empty());
    }
}
