//! Hint strategies (docs/02 §6.2). A hint masks letters and digits of the answer with `_`
//! and reveals more at each level; spaces and punctuation stay visible. A strategy returns
//! `None` once the next level would give the whole answer away. Field hints (show another
//! field) are resolved by the app; AI hints come with v0.8.

use crate::text::html_to_text;
use fc_providers::strategy::{HintStrategy, StrategyResult};
use fc_providers::ProviderInfo;
use serde::{Deserialize, Serialize};

/// Mask character.
pub const MASK: char = '_';

/// Built-in hint strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HintKind {
    /// Level n: the first n letters of the answer.
    FirstLetters,
    /// Level n: the first n letters of every word.
    WordStarts,
    /// Level 1/2/3: every 4th / every 2nd / 3 of 4 letters.
    Partial,
}

impl HintKind {
    /// All, in settings order.
    pub const ALL: [HintKind; 3] = [Self::FirstLetters, Self::WordStarts, Self::Partial];

    /// Stable id.
    pub const fn id(self) -> &'static str {
        match self {
            Self::FirstLetters => "first_letters",
            Self::WordStarts => "word_starts",
            Self::Partial => "partial",
        }
    }

    /// Parses an id.
    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }
}

/// Hint for `answer` (HTML allowed) at `level` (1 = smallest); `None` at level 0 or when
/// nothing would stay hidden.
pub fn hint(kind: HintKind, answer: &str, level: u32) -> Option<String> {
    if level == 0 || (kind == HintKind::Partial && level >= 4) {
        return None;
    }
    let text = html_to_text(answer);
    let level = level as usize;
    let mut out = String::with_capacity(text.len());
    let mut hidden = 0usize;
    let mut letter_in_text = 0usize;
    let mut letter_in_word = 0usize;
    for c in text.chars() {
        if !c.is_alphanumeric() {
            letter_in_word = 0;
            out.push(c);
            continue;
        }
        let show = match kind {
            HintKind::FirstLetters => letter_in_text < level,
            HintKind::WordStarts => letter_in_word < level,
            HintKind::Partial => letter_in_text % 4 < level,
        };
        letter_in_text += 1;
        letter_in_word += 1;
        if show {
            out.push(c);
        } else {
            hidden += 1;
            out.push(MASK);
        }
    }
    (hidden > 0).then_some(out)
}

/// A built-in strategy through the [`HintStrategy`] interface.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinHint(pub HintKind);

impl HintStrategy for BuiltinHint {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.0.id().to_owned(),
            name_key: format!("study.hint.{}", self.0.id()),
            capabilities: Vec::new(),
            settings: Vec::new(),
            secret_keys: Vec::new(),
            local: true,
        }
    }

    fn hint(&self, answer: &str, level: u32) -> StrategyResult<Option<String>> {
        Ok(hint(self.0, answer, level))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn first_letters() {
        let k = HintKind::FirstLetters;
        assert_eq!(hint(k, "elma ağacı", 1).as_deref(), Some("e___ _____"));
        assert_eq!(hint(k, "elma ağacı", 5).as_deref(), Some("elma a____"));
        assert_eq!(hint(k, "elma ağacı", 9), None);
        assert_eq!(hint(k, "elma", 0), None);
    }

    #[test]
    fn word_starts_and_partial() {
        assert_eq!(
            hint(HintKind::WordStarts, "to be, or", 1).as_deref(),
            Some("t_ b_, o_")
        );
        assert_eq!(hint(HintKind::WordStarts, "to be", 2), None);
        assert_eq!(
            hint(HintKind::Partial, "abcdefgh", 1).as_deref(),
            Some("a___e___")
        );
        assert_eq!(
            hint(HintKind::Partial, "abcdefgh", 2).as_deref(),
            Some("ab__ef__")
        );
        assert_eq!(hint(HintKind::Partial, "abcdefgh", 4), None);
        assert_eq!(
            hint(HintKind::Partial, "<b>ab</b>", 1).as_deref(),
            Some("a_")
        );
    }

    #[test]
    fn interface_and_ids() {
        for k in HintKind::ALL {
            assert_eq!(HintKind::parse(k.id()), Some(k));
            assert_eq!(BuiltinHint(k).info().id, k.id());
        }
        assert_eq!(
            BuiltinHint(HintKind::FirstLetters)
                .hint("ab", 1)
                .unwrap()
                .as_deref(),
            Some("a_")
        );
        assert_eq!(HintKind::parse("ai"), None);
    }
}
