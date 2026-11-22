//! Hint strategies (docs/02 §6.2). A hint masks letters and digits of the answer with `_`
//! and reveals more at each level; spaces and punctuation stay visible. A strategy returns
//! `None` once the next level would give the whole answer away. Field hints (show another
//! field) are resolved by the app; AI hints come with v0.8.
//!
//! v1.3 **context hints**: when a vocabulary note has an example sentence containing the
//! term, the first hint is that sentence with the occurrence blanked (`I ___ home.`),
//! [`context_hint`]; later levels are the letter hints ([`leveled_hint`]). A sentence that
//! would still show the answer somewhere is never offered.

use crate::text::html_to_text;
use fc_core::content::html::escape_html;
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

/// What replaces the term in a context hint.
pub const BLANK: &str = "___";

/// Stand-in for the blanked occurrence while the sentence is turned into text (a private-use
/// character no real sentence contains).
const SENTINEL: char = '\u{E000}';

/// An example sentence with the term blanked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskedSentence {
    /// Plain text, the occurrence replaced by [`BLANK`].
    pub text: String,
    /// The same as safe HTML: escaped text, the blank as `<span class="fc-blank">___</span>`.
    pub html: String,
}

/// What a context hint is made from.
#[derive(Debug, Clone, Copy)]
pub struct ContextSource<'a> {
    /// The example sentence as rendered HTML. Its first bold element (`<b>`/`<strong>`: the
    /// occurrence a capture marked from its UTF-16 offsets, or one the user marked) is the
    /// occurrence to blank.
    pub example_html: &'a str,
    /// Without a marked occurrence: forms to look for, in order (surface, then term); the
    /// first whole-word, case-insensitive match is blanked.
    pub candidates: &'a [String],
    /// Texts the masked sentence must not contain anywhere (the expected answer, the term,
    /// the surface form): if one is still there, no context hint is given.
    pub forbidden: &'a [String],
}

/// The example sentence with the term's occurrence blanked, or `None` when there is no
/// usable occurrence or the rest of the sentence would still give the answer away.
pub fn context_hint(src: &ContextSource<'_>) -> Option<MaskedSentence> {
    let with_sentinel = match marked_range(src.example_html) {
        Some((s, e)) => html_to_text(&format!(
            "{}{SENTINEL}{}",
            &src.example_html[..s],
            &src.example_html[e..]
        )),
        None => {
            let text = html_to_text(src.example_html);
            let (s, e) = src.candidates.iter().find_map(|c| find_word(&text, c))?;
            format!("{}{SENTINEL}{}", &text[..s], &text[e..])
        }
    };
    let collapsed = with_sentinel
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.matches(SENTINEL).count() != 1 {
        return None;
    }
    let (before, after) = collapsed.split_once(SENTINEL)?;
    // Something besides the blank: a sentence that is only the term is no hint.
    if !before
        .chars()
        .chain(after.chars())
        .any(char::is_alphanumeric)
    {
        return None;
    }
    let text = format!("{before}{BLANK}{after}");
    if src
        .forbidden
        .iter()
        .any(|f| find_word(&text, &html_to_text(f)).is_some())
    {
        return None;
    }
    Some(MaskedSentence {
        html: format!(
            "{}<span class=\"fc-blank\">{BLANK}</span>{}",
            escape_html(before),
            escape_html(after)
        ),
        text,
    })
}

/// Byte range of the first `<b>…</b>` / `<strong>…</strong>` element of `html` with visible
/// text inside.
fn marked_range(html: &str) -> Option<(usize, usize)> {
    let lower = html.to_ascii_lowercase();
    let mut best: Option<(usize, usize)> = None;
    for (open, close) in [("<b>", "</b>"), ("<strong>", "</strong>")] {
        let Some(s) = lower.find(open) else { continue };
        let Some(len) = lower[s + open.len()..].find(close) else {
            continue;
        };
        let inner = &html[s + open.len()..s + open.len() + len];
        if html_to_text(inner).trim().is_empty() {
            continue;
        }
        let e = s + open.len() + len + close.len();
        if best.is_none_or(|(bs, _)| s < bs) {
            best = Some((s, e));
        }
    }
    best
}

/// Byte range of the first whole-word, case-insensitive occurrence of `needle` in `hay`
/// (runs of whitespace match runs of whitespace). `None` for a needle without letters or
/// digits.
pub fn find_word(hay: &str, needle: &str) -> Option<(usize, usize)> {
    let needle: Vec<char> = needle
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect();
    if !needle.iter().any(|c| c.is_alphanumeric()) {
        return None;
    }
    let same = |a: char, b: char| a == b || a.to_lowercase().eq(b.to_lowercase());
    let mut prev: Option<char> = None;
    for (start, c0) in hay.char_indices() {
        let boundary = prev.is_none_or(|p| !p.is_alphanumeric()) || !c0.is_alphanumeric();
        prev = Some(c0);
        if !boundary || !same(c0, needle[0]) {
            continue;
        }
        let mut it = hay[start..].char_indices().peekable();
        let mut ok = true;
        for &n in &needle {
            match it.next() {
                Some((_, c)) if n == ' ' && c.is_whitespace() => {
                    while it.peek().is_some_and(|(_, c)| c.is_whitespace()) {
                        it.next();
                    }
                }
                Some((_, c)) if same(c, n) => {}
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        let end = it.peek().map_or(hay.len(), |(o, _)| start + o);
        let last = needle[needle.len() - 1];
        let next_ok = hay[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() || !last.is_alphanumeric());
        if next_ok {
            return Some((start, end));
        }
    }
    None
}

/// The shape of a hint (`HintDto.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HintShape {
    /// The example sentence with the term blanked.
    Context,
    /// Letters of the expected answer.
    Letters,
}

/// One hint level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeveledHint {
    /// Context or letters.
    pub shape: HintShape,
    /// Plain text.
    pub text: String,
    /// Safe HTML of the same.
    pub html: String,
}

/// Hint at `level` (1 = smallest): with a context sentence, level 1 is that sentence and
/// level n the letter hint n − 1; without one, the letter hint n. `None` when nothing more
/// can be given.
pub fn leveled_hint(
    kind: HintKind,
    answer: &str,
    context: Option<&MaskedSentence>,
    level: u32,
) -> Option<LeveledHint> {
    let letters_level = match context {
        Some(c) if level == 1 => {
            return Some(LeveledHint {
                shape: HintShape::Context,
                text: c.text.clone(),
                html: c.html.clone(),
            })
        }
        Some(_) => level.saturating_sub(1),
        None => level,
    };
    hint(kind, answer, letters_level).map(|text| LeveledHint {
        shape: HintShape::Letters,
        html: escape_html(&text),
        text,
    })
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

    fn src<'a>(html: &'a str, cands: &'a [String], forbidden: &'a [String]) -> ContextSource<'a> {
        ContextSource {
            example_html: html,
            candidates: cands,
            forbidden,
        }
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn context_hint_blanks_the_marked_occurrence() {
        // A capture: the occurrence in bold (from its UTF-16 offsets).
        let cands = strs(&["went", "go"]);
        let forb = strs(&["gitmek", "go", "went"]);
        let m = context_hint(&src("<p>I <b>went</b> home.</p>", &cands, &forb)).unwrap();
        assert_eq!(m.text, "I ___ home.");
        assert_eq!(m.html, "I <span class=\"fc-blank\">___</span> home.");
        // The marked occurrence wins over an earlier plain match of a candidate.
        let m = context_hint(&src("Go! I <strong>went</strong>.", &strs(&["go"]), &[])).unwrap();
        assert_eq!(m.text, "Go! I ___.");
        // Escaped text stays escaped.
        let m = context_hint(&src("a &lt;b&gt; <b>x</b>", &[], &[])).unwrap();
        assert_eq!(m.html, "a &lt;b&gt; <span class=\"fc-blank\">___</span>");
    }

    #[test]
    fn context_hint_finds_the_surface_then_the_term() {
        let forb = strs(&["gitmek"]);
        // Whole word, case-insensitive: the surface first.
        let m = context_hint(&src("<p>We WENT home.</p>", &strs(&["went", "go"]), &forb)).unwrap();
        assert_eq!(m.text, "We ___ home.");
        // Not inside another word ("going" is not "go"), then the term.
        let m = context_hint(&src("Going? Let's go now.", &strs(&["gone", "go"]), &[])).unwrap();
        assert_eq!(m.text, "Going? Let's ___ now.");
        // Phrases match across any whitespace; Turkish letters fold.
        let m = context_hint(&src("I must pick\n up my keys", &strs(&["pick up"]), &[])).unwrap();
        assert_eq!(m.text, "I must ___ my keys");
        let m = context_hint(&src("Çiçek ÇOK güzel.", &strs(&["çok"]), &[])).unwrap();
        assert_eq!(m.text, "Çiçek ___ güzel.");
    }

    #[test]
    fn context_hint_never_leaks_the_answer() {
        // The answer is still in the sentence elsewhere → no context hint.
        let cands = strs(&["went", "go"]);
        let forb = strs(&["go", "went"]);
        assert_eq!(
            context_hint(&src("I <b>went</b> where you go.", &cands, &forb)),
            None
        );
        assert_eq!(context_hint(&src("I went and went.", &cands, &forb)), None);
        // No occurrence, nothing but the term, empty sentences → none.
        assert_eq!(context_hint(&src("Nothing here.", &cands, &forb)), None);
        assert_eq!(context_hint(&src("<b>went</b>!", &cands, &[])), None);
        assert_eq!(context_hint(&src("", &cands, &[])), None);
        assert_eq!(context_hint(&src("<b> </b> x", &[], &[])), None);
        // A forbidden text given as HTML is compared as text.
        assert_eq!(
            context_hint(&src(
                "I <b>went</b> to Gitmek.",
                &[],
                &strs(&["<p>gitmek</p>"])
            )),
            None
        );
    }

    #[test]
    fn levels_start_with_the_context() {
        let ctx = MaskedSentence {
            text: "I ___ home.".into(),
            html: "I <span class=\"fc-blank\">___</span> home.".into(),
        };
        let k = HintKind::FirstLetters;
        let one = leveled_hint(k, "gitmek", Some(&ctx), 1).unwrap();
        assert_eq!(
            (one.shape, one.text.as_str()),
            (HintShape::Context, "I ___ home.")
        );
        let two = leveled_hint(k, "gitmek", Some(&ctx), 2).unwrap();
        assert_eq!(
            (two.shape, two.text.as_str()),
            (HintShape::Letters, "g_____")
        );
        assert_eq!(leveled_hint(k, "gitmek", Some(&ctx), 7), None);
        let plain = leveled_hint(k, "a<b", None, 1).unwrap();
        assert_eq!(
            (plain.text.as_str(), plain.html.as_str()),
            ("a<_", "a&lt;_")
        );
        assert_eq!(leveled_hint(k, "gitmek", None, 1).unwrap().text, "g_____");
        assert_eq!(leveled_hint(k, "gitmek", Some(&ctx), 0), None);
        assert_eq!(find_word("x", "  "), None);
        assert_eq!(find_word("a-b", "b"), Some((2, 3)));
    }
}
