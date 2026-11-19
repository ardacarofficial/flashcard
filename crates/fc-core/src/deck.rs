//! Decks and deck paths (`Language::English::Verbs`).

use crate::error::{CoreError, CoreResult, NameProblem};
use crate::id::{DeckId, PresetId};
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Separator between hierarchy levels in deck paths and tag names.
pub const PATH_SEPARATOR: &str = "::";

/// Maximum length (in chars) of a single deck name segment.
pub const MAX_DECK_SEGMENT_LEN: usize = 100;

/// A deck. Hierarchy is expressed with `parent_id`; `name` is a single segment (never contains
/// `::`). The full path is derived by walking ancestors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deck {
    /// Id.
    pub id: DeckId,
    /// Parent deck, `None` for a top-level deck.
    pub parent_id: Option<DeckId>,
    /// Leaf name (one segment, normalized by [`normalize_deck_segment`]).
    pub name: String,
    /// Free-form description (Markdown).
    pub description: String,
    /// Optional icon identifier (emoji or icon key).
    pub icon: Option<String>,
    /// Optional color token or `#rrggbb`.
    pub color: Option<String>,
    /// Options preset; `None` = default preset.
    pub preset_id: Option<PresetId>,
    /// Sort position among siblings.
    pub position: i64,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
    /// Soft-delete time; `Some` = in trash.
    pub deleted_at: Option<Timestamp>,
}

impl Deck {
    /// Whether the deck is in the trash.
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// A parsed, normalized deck path: one or more non-empty segments.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeckPath {
    segments: Vec<String>,
}

impl DeckPath {
    /// Parses `A::B::C`. Each segment is trimmed and its inner whitespace collapsed.
    ///
    /// Errors: empty input, empty segment (`A::::B`, `::A`), control characters, segment longer
    /// than [`MAX_DECK_SEGMENT_LEN`].
    pub fn parse(input: &str) -> CoreResult<Self> {
        let trimmed = input.trim();
        let err = |reason| CoreError::InvalidDeckName {
            name: input.to_owned(),
            reason,
        };
        if trimmed.is_empty() {
            return Err(err(NameProblem::Empty));
        }
        let segments = trimmed
            .split(PATH_SEPARATOR)
            .map(|s| normalize_deck_segment(s).map_err(|e| with_name(e, input)))
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(Self { segments })
    }

    /// Builds a path from already-separate segments (each is normalized).
    pub fn from_segments<I, S>(segments: I) -> CoreResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let segments = segments
            .into_iter()
            .map(|s| normalize_deck_segment(s.as_ref()))
            .collect::<CoreResult<Vec<_>>>()?;
        if segments.is_empty() {
            return Err(CoreError::InvalidDeckName {
                name: String::new(),
                reason: NameProblem::Empty,
            });
        }
        Ok(Self { segments })
    }

    /// The segments, root first.
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Last segment (the deck's own name).
    pub fn leaf(&self) -> &str {
        self.segments.last().map(String::as_str).unwrap_or_default()
    }

    /// Path of the parent deck, `None` for a top-level path.
    pub fn parent(&self) -> Option<DeckPath> {
        (self.segments.len() > 1).then(|| DeckPath {
            segments: self.segments[..self.segments.len() - 1].to_vec(),
        })
    }

    /// Number of segments (1 = top level).
    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    /// Whether `self` equals `other` or is below it (case-insensitive).
    pub fn starts_with(&self, other: &DeckPath) -> bool {
        other.segments.len() <= self.segments.len()
            && other
                .segments
                .iter()
                .zip(&self.segments)
                .all(|(a, b)| name_eq(a, b))
    }

    /// Appends a child segment.
    pub fn join(&self, child: &str) -> CoreResult<DeckPath> {
        let mut segments = self.segments.clone();
        segments.push(normalize_deck_segment(child)?);
        Ok(DeckPath { segments })
    }
}

impl fmt::Display for DeckPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.segments.join(PATH_SEPARATOR))
    }
}

/// Normalizes one deck name segment: trims, collapses inner whitespace to one space, rejects
/// empty, `::`, control chars and over-long names.
pub fn normalize_deck_segment(raw: &str) -> CoreResult<String> {
    let err = |reason| CoreError::InvalidDeckName {
        name: raw.to_owned(),
        reason,
    };
    if raw.contains(PATH_SEPARATOR) {
        return Err(err(NameProblem::ForbiddenChar));
    }
    if raw.chars().any(char::is_control) {
        return Err(err(NameProblem::ForbiddenChar));
    }
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return Err(err(NameProblem::EmptySegment));
    }
    if collapsed.chars().count() > MAX_DECK_SEGMENT_LEN {
        return Err(err(NameProblem::TooLong));
    }
    Ok(collapsed)
}

/// Case-insensitive (Unicode lowercase) name equality used for sibling uniqueness.
pub fn name_eq(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn with_name(e: CoreError, full: &str) -> CoreError {
    match e {
        CoreError::InvalidDeckName { reason, .. } => CoreError::InvalidDeckName {
            name: full.to_owned(),
            reason,
        },
        other => other,
    }
}

/// Returns the ancestry of `id` (root first, `id` last) given a lookup of parent links.
/// Stops (without error) if a link is missing; returns [`CoreError::DeckCycle`] on a cycle.
pub fn deck_ancestry(
    id: DeckId,
    parent_of: impl Fn(DeckId) -> Option<Option<DeckId>>,
) -> CoreResult<Vec<DeckId>> {
    let mut chain = vec![id];
    let mut cur = id;
    while let Some(Some(parent)) = parent_of(cur) {
        if chain.contains(&parent) {
            return Err(CoreError::DeckCycle);
        }
        chain.push(parent);
        cur = parent;
    }
    chain.reverse();
    Ok(chain)
}

/// Checks that moving `deck` under `new_parent` keeps the tree acyclic.
/// `new_parent_ancestry` is the ancestry of the new parent (root first, parent last).
pub fn check_move(deck: DeckId, new_parent_ancestry: &[DeckId]) -> CoreResult<()> {
    if new_parent_ancestry.contains(&deck) {
        Err(CoreError::DeckCycle)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use uuid::Uuid;

    #[test]
    fn parses_and_normalizes_paths() {
        let p = DeckPath::parse("  Dil :: İngilizce   Kelimeler::Fiiller ").unwrap();
        assert_eq!(p.segments(), ["Dil", "İngilizce Kelimeler", "Fiiller"]);
        assert_eq!(p.to_string(), "Dil::İngilizce Kelimeler::Fiiller");
        assert_eq!(p.leaf(), "Fiiller");
        assert_eq!(p.depth(), 3);
        assert_eq!(p.parent().unwrap().to_string(), "Dil::İngilizce Kelimeler");
        assert!(DeckPath::parse("Solo").unwrap().parent().is_none());
    }

    #[test]
    fn rejects_bad_paths() {
        let reason = |s: &str| match DeckPath::parse(s) {
            Err(CoreError::InvalidDeckName { reason, .. }) => reason,
            other => panic!("expected error for {s:?}, got {other:?}"),
        };
        assert_eq!(reason(""), NameProblem::Empty);
        assert_eq!(reason("   "), NameProblem::Empty);
        assert_eq!(reason("A::::B"), NameProblem::EmptySegment);
        assert_eq!(reason("::A"), NameProblem::EmptySegment);
        assert_eq!(reason("A::"), NameProblem::EmptySegment);
        assert_eq!(reason("A\u{0007}"), NameProblem::ForbiddenChar);
        assert_eq!(reason(&"x".repeat(101)), NameProblem::TooLong);
        assert!(DeckPath::parse(&"x".repeat(100)).is_ok());
    }

    #[test]
    fn single_colon_is_allowed() {
        assert_eq!(DeckPath::parse("Ch: 1").unwrap().segments(), ["Ch: 1"]);
    }

    #[test]
    fn path_prefix_is_case_insensitive() {
        let a = DeckPath::parse("Dil::İngilizce::Fiiller").unwrap();
        let b = DeckPath::parse("dil::ingilizce").unwrap();
        // Unicode lowercase of "İ" is "i̇" (with combining dot), so this is NOT a prefix: the
        // core does not guess Turkish casing rules. Exact-case prefix works:
        assert!(!a.starts_with(&b));
        assert!(a.starts_with(&DeckPath::parse("DIL::İngilizce").unwrap()));
        assert!(!b.starts_with(&a));
        assert_eq!(a.join("Düzensiz").unwrap().depth(), 4);
        assert!(a.join("a::b").is_err());
        assert!(DeckPath::from_segments(Vec::<String>::new()).is_err());
        assert_eq!(
            DeckPath::from_segments(["A", " B "]).unwrap().to_string(),
            "A::B"
        );
    }

    fn id(n: u128) -> DeckId {
        DeckId(Uuid::from_u128(n))
    }

    #[test]
    fn ancestry_and_cycles() {
        let parents: HashMap<DeckId, Option<DeckId>> =
            [(id(1), None), (id(2), Some(id(1))), (id(3), Some(id(2)))].into();
        let anc = deck_ancestry(id(3), |d| parents.get(&d).copied()).unwrap();
        assert_eq!(anc, vec![id(1), id(2), id(3)]);
        assert!(check_move(id(1), &anc).is_err());
        assert!(check_move(id(9), &anc).is_ok());

        let cyclic: HashMap<DeckId, Option<DeckId>> =
            [(id(1), Some(id(2))), (id(2), Some(id(1)))].into();
        assert_eq!(
            deck_ancestry(id(1), |d| cyclic.get(&d).copied()),
            Err(CoreError::DeckCycle)
        );
    }
}
