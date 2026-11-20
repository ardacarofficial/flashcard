//! Confidence-based rating (docs/02 §27, Brainscape style): instead of the four buttons the
//! learner answers "how sure was I?" on a 1–5 scale; a mapping table (settings) turns the
//! level into one of the four scheduler ratings.

use fc_core::card::Rating;

/// Levels of the scale (1 = not at all … 5 = perfectly).
pub const LEVELS: u8 = 5;

/// Default mapping: 1–2 → Again, 3 → Hard, 4 → Good, 5 → Easy.
pub const DEFAULT_MAP: [Rating; LEVELS as usize] = [
    Rating::Again,
    Rating::Again,
    Rating::Hard,
    Rating::Good,
    Rating::Easy,
];

/// Rating of confidence `level` (1-based) under `map`; `None` outside 1–5.
pub fn rating(level: u8, map: &[Rating; LEVELS as usize]) -> Option<Rating> {
    if level == 0 {
        return None;
    }
    map.get(usize::from(level - 1)).copied()
}

/// Parses a rating id (`again`, `hard`, `good`, `easy`).
pub fn parse_rating(id: &str) -> Option<Rating> {
    match id {
        "again" => Some(Rating::Again),
        "hard" => Some(Rating::Hard),
        "good" => Some(Rating::Good),
        "easy" => Some(Rating::Easy),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_map_onto_ratings() {
        let got: Vec<_> = (0..=6).map(|l| rating(l, &DEFAULT_MAP)).collect();
        assert_eq!(
            got,
            vec![
                None,
                Some(Rating::Again),
                Some(Rating::Again),
                Some(Rating::Hard),
                Some(Rating::Good),
                Some(Rating::Easy),
                None
            ]
        );
        let custom = [
            Rating::Again,
            Rating::Hard,
            Rating::Good,
            Rating::Good,
            Rating::Easy,
        ];
        assert_eq!(rating(2, &custom), Some(Rating::Hard));
        assert_eq!(parse_rating("good"), Some(Rating::Good));
        assert_eq!(parse_rating("meh"), None);
    }
}
