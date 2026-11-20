//! Match (v1.1, docs/11 §3): a round of terms and definitions shuffled into tiles; the learner
//! pairs each term with its definition against the clock. Pairs come from the session's cards;
//! a correct pair leaves the board, a wrong one counts a mistake. Practice only: the app logs
//! one `match` review per card and never changes the schedule.
//!
//! Pure: the round is dealt with an injected [`Rng`] and timed with timestamps the app takes
//! from its `Clock`.

use super::{option_key, shuffle};
use fc_core::card::Rating;
use fc_core::rng::Rng;
use fc_core::{CardId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Pairs on the board in one round.
pub const ROUND_PAIRS: usize = 6;
/// Fewest pairs that make a round (one pair is not a game).
pub const MIN_PAIRS: usize = 2;
/// Most cards a match session draws its rounds from.
pub const MAX_POOL: usize = 500;

/// One card of the session: its term (question) and definition (answer), as plain text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchItem {
    /// Card.
    pub card_id: CardId,
    /// Question side.
    pub term: String,
    /// Answer.
    pub definition: String,
}

/// Which half of a pair a tile shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The card's question.
    Term,
    /// The card's answer.
    Definition,
}

/// One tile on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    /// Index of its pair in [`Round::pairs`].
    pub pair: u32,
    /// Half.
    pub side: Side,
}

/// One pair of the round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pair {
    /// The card.
    pub item: MatchItem,
    /// Matched (its tiles left the board).
    pub matched: bool,
    /// Wrong picks that involved one of its tiles.
    pub misses: u32,
}

/// A round: the board and its clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Round {
    /// Pairs on the board.
    pub pairs: Vec<Pair>,
    /// Tiles in board order (two per pair, shuffled).
    pub tiles: Vec<Tile>,
    /// Wrong picks.
    pub mistakes: u32,
    /// When the board was dealt.
    pub started_at: Timestamp,
    /// When the last pair was matched (the clock stops there).
    #[serde(default)]
    pub completed_at: Option<Timestamp>,
    /// Board time once finished, ms.
    #[serde(default)]
    pub finished_ms: Option<u32>,
}

/// Match state of a session.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchState {
    /// The session's cards (rounds are dealt from them).
    pub pool: Vec<MatchItem>,
    /// Rounds dealt so far.
    pub rounds: u32,
    /// The current (or last) round.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round: Option<Round>,
}

/// Why a pick was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickError {
    /// No such tile.
    OutOfRange,
    /// Both tiles are terms, or both definitions.
    SameSide,
    /// A tile already left the board.
    Matched,
    /// The round is over.
    Finished,
}

/// The items a round can use: at most one per card, terms and definitions distinct (by
/// [`option_key`]) so every term has exactly one right definition on the board; empty texts
/// are skipped.
pub fn usable(pool: &[MatchItem]) -> Vec<&MatchItem> {
    let mut terms = HashSet::new();
    let mut defs = HashSet::new();
    let mut cards = HashSet::new();
    pool.iter()
        .filter(|i| {
            let (t, d) = (option_key(&i.term), option_key(&i.definition));
            !t.is_empty()
                && !d.is_empty()
                && !terms.contains(&t)
                && !defs.contains(&d)
                && cards.insert(i.card_id)
                && terms.insert(t)
                && defs.insert(d)
        })
        .collect()
}

/// Deals a round of up to [`ROUND_PAIRS`] pairs drawn at random from `pool`, tiles shuffled.
/// `None` when fewer than [`MIN_PAIRS`] usable pairs exist.
pub fn deal(pool: &[MatchItem], now: Timestamp, rng: &mut dyn Rng) -> Option<Round> {
    let mut items = usable(pool);
    if items.len() < MIN_PAIRS {
        return None;
    }
    shuffle(&mut items, rng);
    items.truncate(ROUND_PAIRS);
    let pairs: Vec<Pair> = items
        .into_iter()
        .map(|i| Pair {
            item: i.clone(),
            matched: false,
            misses: 0,
        })
        .collect();
    let mut tiles: Vec<Tile> = (0..pairs.len())
        .flat_map(|p| {
            let pair = u32::try_from(p).unwrap_or(u32::MAX);
            [
                Tile {
                    pair,
                    side: Side::Term,
                },
                Tile {
                    pair,
                    side: Side::Definition,
                },
            ]
        })
        .collect();
    shuffle(&mut tiles, rng);
    Some(Round {
        pairs,
        tiles,
        mistakes: 0,
        started_at: now,
        completed_at: None,
        finished_ms: None,
    })
}

impl Round {
    /// Whether every pair is matched.
    pub fn complete(&self) -> bool {
        self.pairs.iter().all(|p| p.matched)
    }

    /// Matched pairs.
    pub fn matched(&self) -> usize {
        self.pairs.iter().filter(|p| p.matched).count()
    }

    /// Whether tile `i` is still on the board.
    pub fn on_board(&self, i: usize) -> bool {
        self.tiles
            .get(i)
            .and_then(|t| self.pairs.get(t.pair as usize))
            .is_some_and(|p| !p.matched)
    }

    /// Pairs tile `a` with tile `b` (either order): `Ok(true)` when they are one card's term
    /// and definition (the pair leaves the board), `Ok(false)` for a wrong pair (a mistake,
    /// counted against both pairs involved). The pick that matches the last pair stops the
    /// clock at `now`.
    pub fn pick(&mut self, a: usize, b: usize, now: Timestamp) -> Result<bool, PickError> {
        if self.finished_ms.is_some() {
            return Err(PickError::Finished);
        }
        let (ta, tb) = match (self.tiles.get(a), self.tiles.get(b)) {
            (Some(x), Some(y)) => (x.clone(), y.clone()),
            _ => return Err(PickError::OutOfRange),
        };
        if ta.side == tb.side {
            return Err(PickError::SameSide);
        }
        if !self.on_board(a) || !self.on_board(b) {
            return Err(PickError::Matched);
        }
        if ta.pair == tb.pair {
            if let Some(p) = self.pairs.get_mut(ta.pair as usize) {
                p.matched = true;
            }
            if self.complete() {
                self.completed_at = Some(now);
            }
            return Ok(true);
        }
        self.mistakes = self.mistakes.saturating_add(1);
        for i in [ta.pair, tb.pair] {
            if let Some(p) = self.pairs.get_mut(i as usize) {
                p.misses = p.misses.saturating_add(1);
            }
        }
        Ok(false)
    }

    /// Board time from the deal to the last match (or to `now` while pairs are left), ms
    /// (never negative).
    pub fn elapsed_ms(&self, now: Timestamp) -> u32 {
        let end = self.completed_at.unwrap_or(now);
        let ms = end
            .as_millis()
            .saturating_sub(self.started_at.as_millis())
            .max(0);
        u32::try_from(ms).unwrap_or(u32::MAX)
    }
}

/// Rating logged for a pair (practice: never applied to the schedule): matched without a
/// miss → good, else again.
pub const fn rating(pair: &Pair) -> Rating {
    if pair.misses == 0 {
        Rating::Good
    } else {
        Rating::Again
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use fc_core::rng::SplitMix64;

    fn item(n: u128, term: &str, def: &str) -> MatchItem {
        MatchItem {
            card_id: CardId(uuid::Uuid::from_u128(n)),
            term: term.into(),
            definition: def.into(),
        }
    }

    fn pool(n: usize) -> Vec<MatchItem> {
        (0..n)
            .map(|i| item(i as u128 + 1, &format!("t{i}"), &format!("d{i}")))
            .collect()
    }

    #[test]
    fn deals_six_pairs_deterministically() {
        let p = pool(10);
        let a = deal(&p, Timestamp(5), &mut SplitMix64::new(7)).unwrap();
        let b = deal(&p, Timestamp(5), &mut SplitMix64::new(7)).unwrap();
        assert_eq!(a, b, "same seed, same board");
        assert_eq!(a.pairs.len(), ROUND_PAIRS);
        assert_eq!(a.tiles.len(), 2 * ROUND_PAIRS);
        for p in 0..ROUND_PAIRS as u32 {
            let sides: Vec<Side> = a
                .tiles
                .iter()
                .filter(|t| t.pair == p)
                .map(|t| t.side)
                .collect();
            assert_eq!(sides.len(), 2);
            assert_ne!(sides[0], sides[1]);
        }
        let c = deal(&p, Timestamp(5), &mut SplitMix64::new(8)).unwrap();
        assert_ne!(a.tiles, c.tiles, "another seed shuffles differently");
        // Small pools: all of them; too small: no round.
        assert_eq!(
            deal(&pool(3), Timestamp(0), &mut SplitMix64::new(1))
                .unwrap()
                .pairs
                .len(),
            3
        );
        assert!(deal(&pool(1), Timestamp(0), &mut SplitMix64::new(1)).is_none());
    }

    #[test]
    fn skips_ambiguous_and_empty_pairs() {
        let p = vec![
            item(1, "dog", "köpek"),
            item(2, "Dog ", "it"),     // same term
            item(3, "hound", "Köpek"), // same definition
            item(4, "cat", ""),        // empty
            item(1, "again", "card"),  // same card
            item(5, "cat", "kedi"),
        ];
        let u: Vec<&str> = usable(&p).iter().map(|i| i.term.as_str()).collect();
        assert_eq!(u, ["dog", "cat"]);
    }

    #[test]
    fn picks_validate_and_count_mistakes() {
        let mut r = deal(&pool(3), Timestamp(1_000), &mut SplitMix64::new(3)).unwrap();
        let find = |r: &Round, pair: u32, side: Side| {
            r.tiles
                .iter()
                .position(|t| t.pair == pair && t.side == side)
                .unwrap()
        };
        let (t0, d0, d1) = (
            find(&r, 0, Side::Term),
            find(&r, 0, Side::Definition),
            find(&r, 1, Side::Definition),
        );
        assert_eq!(r.pick(t0, 99, Timestamp(2_000)), Err(PickError::OutOfRange));
        assert_eq!(r.pick(d0, d1, Timestamp(2_000)), Err(PickError::SameSide));
        assert_eq!(r.pick(t0, d1, Timestamp(2_000)), Ok(false));
        assert_eq!(
            (r.mistakes, r.pairs[0].misses, r.pairs[1].misses),
            (1, 1, 1)
        );
        assert_eq!(r.pick(d0, t0, Timestamp(2_000)), Ok(true), "either order");
        assert!(!r.on_board(t0));
        assert_eq!(r.pick(t0, d0, Timestamp(2_000)), Err(PickError::Matched));
        assert_eq!(r.matched(), 1);
        for p in 1..3 {
            let (t, d) = (find(&r, p, Side::Term), find(&r, p, Side::Definition));
            assert_eq!(r.pick(t, d, Timestamp(4_500)), Ok(true));
        }
        assert!(r.complete());
        // The clock stopped at the last match.
        assert_eq!(r.elapsed_ms(Timestamp(60_000)), 3_500);
        let fresh = deal(&pool(2), Timestamp(1_000), &mut SplitMix64::new(3)).unwrap();
        assert_eq!(fresh.elapsed_ms(Timestamp(1_250)), 250);
        assert_eq!(fresh.elapsed_ms(Timestamp(0)), 0);
        assert_eq!(rating(&r.pairs[0]), Rating::Again);
        assert_eq!(rating(&r.pairs[2]), Rating::Good);
        r.finished_ms = Some(3_500);
        assert_eq!(r.pick(t0, d0, Timestamp(9_000)), Err(PickError::Finished));
    }
}
