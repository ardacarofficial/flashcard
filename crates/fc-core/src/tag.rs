//! Tags: free, hierarchical labels on notes (`dil::fiil::düzensiz`).

use crate::deck::PATH_SEPARATOR;
use crate::error::{CoreError, CoreResult, NameProblem};
use crate::id::TagId;
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Maximum length (in chars) of a full tag name.
pub const MAX_TAG_LEN: usize = 200;

/// A tag row. Notes reference tags through the `note_tags` relation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    /// Id.
    pub id: TagId,
    /// Normalized hierarchical name.
    pub name: TagName,
    /// Optional color token or `#rrggbb`.
    pub color: Option<String>,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// A normalized tag name. Construct with [`TagName::parse`].
///
/// Normal form: no surrounding whitespace, no leading `#`, segments joined by `::`, each segment
/// non-empty with inner whitespace replaced by `_`. Case is preserved; comparisons for
/// uniqueness use [`TagName::key`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string"))]
pub struct TagName(String);

impl TagName {
    /// Normalizes and validates a single tag.
    pub fn parse(raw: &str) -> CoreResult<Self> {
        normalize_tag(raw).map(Self)
    }

    /// The normalized name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Case-insensitive identity key (Unicode lowercase).
    pub fn key(&self) -> String {
        self.0.to_lowercase()
    }

    /// Segments, root first.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split(PATH_SEPARATOR)
    }

    /// Whether `self` equals `ancestor` or is nested below it (case-insensitive).
    pub fn is_within(&self, ancestor: &TagName) -> bool {
        let mine: Vec<&str> = self.segments().collect();
        let theirs: Vec<&str> = ancestor.segments().collect();
        theirs.len() <= mine.len()
            && theirs
                .iter()
                .zip(&mine)
                .all(|(a, b)| a.to_lowercase() == b.to_lowercase())
    }

    /// Re-roots `self` from `from` to `to` (used by `tags.rename`, which renames children
    /// too). Returns `None` if `self` is not within `from`.
    pub fn rebase(&self, from: &TagName, to: &TagName) -> Option<TagName> {
        if !self.is_within(from) {
            return None;
        }
        let mut out = to.0.clone();
        for seg in self.segments().skip(from.segments().count()) {
            out.push_str(PATH_SEPARATOR);
            out.push_str(seg);
        }
        Some(TagName(out))
    }
}

impl fmt::Display for TagName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for TagName {
    type Error = CoreError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        TagName::parse(&value)
    }
}

impl From<TagName> for String {
    fn from(t: TagName) -> Self {
        t.0
    }
}

const FORBIDDEN_TAG_CHARS: &[char] = &['"', ',', ';'];

/// Normalizes one tag. See [`TagName`] for the normal form.
pub fn normalize_tag(raw: &str) -> CoreResult<String> {
    let err = |reason| CoreError::InvalidTagName {
        name: raw.to_owned(),
        reason,
    };
    let trimmed = raw.trim().trim_start_matches('#').trim();
    if trimmed.is_empty() {
        return Err(err(NameProblem::Empty));
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() || FORBIDDEN_TAG_CHARS.contains(&c))
    {
        return Err(err(NameProblem::ForbiddenChar));
    }
    let mut segments = Vec::new();
    for seg in trimmed.split(PATH_SEPARATOR) {
        let seg = seg.split_whitespace().collect::<Vec<_>>().join("_");
        if seg.is_empty() {
            return Err(err(NameProblem::EmptySegment));
        }
        segments.push(seg);
    }
    let out = segments.join(PATH_SEPARATOR);
    if out.chars().count() > MAX_TAG_LEN {
        return Err(err(NameProblem::TooLong));
    }
    Ok(out)
}

/// Parses a user-typed tag list (`"a b, c::d"`): splits on whitespace and commas, normalizes,
/// and removes case-insensitive duplicates keeping the first spelling. Order is preserved.
pub fn parse_tag_list(input: &str) -> CoreResult<Vec<TagName>> {
    let mut out: Vec<TagName> = Vec::new();
    for raw in input.split(|c: char| c.is_whitespace() || c == ',') {
        if raw.is_empty() {
            continue;
        }
        let tag = TagName::parse(raw)?;
        if !out.iter().any(|t| t.key() == tag.key()) {
            out.push(tag);
        }
    }
    Ok(out)
}

/// Removes case-insensitive duplicates from an already-parsed tag list, preserving order.
pub fn dedup_tags(tags: impl IntoIterator<Item = TagName>) -> Vec<TagName> {
    let mut out: Vec<TagName> = Vec::new();
    for t in tags {
        if !out.iter().any(|o| o.key() == t.key()) {
            out.push(t);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> TagName {
        TagName::parse(s).unwrap()
    }

    #[test]
    fn normalizes() {
        assert_eq!(
            t("  #dil :: fiil::  düzensiz fiil ").as_str(),
            "dil::fiil::düzensiz_fiil"
        );
        assert_eq!(t("Grammar").as_str(), "Grammar");
        assert_eq!(t("a:b").as_str(), "a:b");
    }

    #[test]
    fn rejects() {
        for bad in [
            "", "  ", "#", "a::::b", "::a", "a::", "a\"b", "a,b", "x\u{1}",
        ] {
            assert!(TagName::parse(bad).is_err(), "{bad:?} should fail");
        }
        assert!(TagName::parse(&"a".repeat(201)).is_err());
    }

    #[test]
    fn hierarchy() {
        let parent = t("dil::fiil");
        assert!(t("Dil::Fiil::düzensiz").is_within(&parent));
        assert!(t("dil::fiil").is_within(&parent));
        assert!(!t("dil::fiiller").is_within(&parent));
        assert_eq!(
            t("dil::fiil::düzensiz").rebase(&parent, &t("lang::verb")),
            Some(t("lang::verb::düzensiz"))
        );
        assert_eq!(t("other").rebase(&parent, &t("x")), None);
        assert_eq!(t("a::b::c").segments().collect::<Vec<_>>(), ["a", "b", "c"]);
    }

    #[test]
    fn list_parsing_dedups_case_insensitively() {
        let tags = parse_tag_list("a, B  b c::d,,A").unwrap();
        let names: Vec<_> = tags.iter().map(TagName::as_str).collect();
        assert_eq!(names, ["a", "B", "c::d"]);
        assert!(parse_tag_list("ok bad\"").is_err());
        assert_eq!(dedup_tags([t("x"), t("X"), t("y")]).len(), 2);
    }

    #[test]
    fn serde_validates() {
        let ok: TagName = serde_json::from_str("\" a b \"").unwrap();
        assert_eq!(ok.as_str(), "a_b");
        assert!(serde_json::from_str::<TagName>("\"\"").is_err());
    }
}
