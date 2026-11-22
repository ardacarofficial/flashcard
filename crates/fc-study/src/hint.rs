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

/// What replaces the answer in a context hint.
pub const BLANK: &str = "___";

/// Stand-ins while the sentence is turned into text (private-use characters no real sentence
/// contains): the word's occurrence between [`OPEN`] and [`CLOSE`], the blank as [`HOLE`].
const OPEN: char = '\u{E000}';
const CLOSE: char = '\u{E001}';
const HOLE: char = '\u{E002}';

/// A context hint: the example sentence, the answer blanked where it occurs, the card's word
/// highlighted when it is not the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskedSentence {
    /// Plain text, the blank as [`BLANK`].
    pub text: String,
    /// The same as safe HTML: escaped text, the blank as `<span class="fc-blank">___</span>`,
    /// the highlighted word as `<b class="fc-context-word">…</b>`.
    pub html: String,
}

/// What a context hint is made from (v1.3 rule: a hint may reveal context but never the
/// answer — blank exactly the answer where it occurs in the example; where it does not, the
/// example is shown as context).
#[derive(Debug, Clone, Copy)]
pub struct ContextSource<'a> {
    /// The example sentence as rendered HTML. Its first bold element (`<b>`/`<strong>`: the
    /// occurrence a capture marked from its UTF-16 offsets, or one the user marked) is the
    /// card's word.
    pub example_html: &'a str,
    /// The card's word without a marked occurrence: forms to look for, in order (surface,
    /// then term); the first whole-word, case-insensitive match is the occurrence.
    pub word_forms: &'a [String],
    /// The card asks for the word (a reverse / production card): its occurrence is the
    /// answer and is blanked. Otherwise (it asks for the meaning) the word is highlighted.
    pub asks_for_word: bool,
    /// The expected answer and its alternatives: blanked where they occur (a card asking for
    /// the meaning), and never shown anywhere in the hint.
    pub answer_forms: &'a [String],
}

/// The context hint of `src`, or `None` when the sentence is unusable (nothing but the word,
/// no text) or would still give the answer away after blanking.
pub fn context_hint(src: &ContextSource<'_>) -> Option<MaskedSentence> {
    let text = match marked_range(src.example_html) {
        Some((s, e, inner)) => html_to_text(&format!(
            "{}{OPEN}{}{CLOSE}{}",
            &src.example_html[..s],
            &src.example_html[inner.0..inner.1],
            &src.example_html[e..]
        )),
        None => {
            let text = html_to_text(src.example_html);
            match src.word_forms.iter().find_map(|w| find_word(&text, w)) {
                Some((s, e)) => format!("{}{OPEN}{}{CLOSE}{}", &text[..s], &text[s..e], &text[e..]),
                None => text,
            }
        }
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let (opens, closes) = (text.matches(OPEN).count(), text.matches(CLOSE).count());
    if opens > 1 || opens != closes || text.contains(HOLE) {
        return None;
    }
    let word = match (text.find(OPEN), text.find(CLOSE)) {
        (Some(s), Some(e)) if s < e => Some((s, e + CLOSE.len_utf8())),
        (None, None) => None,
        _ => return None,
    };
    let text = if src.asks_for_word {
        // The word is the answer: blank its occurrence (none: the sentence is context).
        match word {
            Some((s, e)) => format!("{}{HOLE}{}", &text[..s], &text[e..]),
            None => text,
        }
    } else {
        // The answer (the meaning) is blanked where it occurs, outside the word.
        match src.answer_forms.iter().find_map(|a| find_word(&text, a)) {
            Some((s, e)) if word.is_none_or(|(ws, we)| e <= ws || s >= we) => {
                format!("{}{HOLE}{}", &text[..s], &text[e..])
            }
            Some(_) => return None,
            None => text,
        }
    };
    // Something besides the word and the blank: a sentence that is only the term is no hint.
    let mut rest = String::with_capacity(text.len());
    let mut inside = false;
    for c in text.chars() {
        match c {
            OPEN => inside = true,
            CLOSE => inside = false,
            HOLE => {}
            c if !inside => rest.push(c),
            _ => {}
        }
    }
    if !rest.chars().any(char::is_alphanumeric) {
        return None;
    }
    let visible: String = text
        .chars()
        .filter(|c| *c != OPEN && *c != CLOSE)
        .collect::<String>()
        .replace(HOLE, BLANK);
    // Never the answer: not its text, nor (asking for the word) any form of the word.
    let mut forbidden: Vec<&str> = src.answer_forms.iter().map(String::as_str).collect();
    if src.asks_for_word {
        forbidden.extend(src.word_forms.iter().map(String::as_str));
    }
    if forbidden
        .iter()
        .any(|f| leaks(&visible, f, src.asks_for_word))
    {
        return None;
    }
    let mut html = String::with_capacity(text.len() + 64);
    let mut run = String::new();
    for c in text.chars() {
        let tag = match c {
            OPEN => "<b class=\"fc-context-word\">",
            CLOSE => "</b>",
            HOLE => "<span class=\"fc-blank\">___</span>",
            c => {
                run.push(c);
                continue;
            }
        };
        html.push_str(&escape_html(&run));
        run.clear();
        html.push_str(tag);
    }
    html.push_str(&escape_html(&run));
    Some(MaskedSentence {
        text: visible,
        html,
    })
}

/// Whether `text` gives `answer` away: the answer as whole words; when the answer is the
/// card's word (`inflected`), also its inflections — every word of the answer starting a
/// word of the text ("goes" for "go", "picked it up" for "pick up").
fn leaks(text: &str, answer: &str, inflected: bool) -> bool {
    let answer = html_to_text(answer);
    if find_word(text, answer.trim()).is_some() {
        return true;
    }
    let parts: Vec<String> = answer.split_whitespace().map(str::to_lowercase).collect();
    if !inflected || parts.is_empty() || parts.iter().any(|p| p.chars().count() < 2) {
        return false;
    }
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    parts
        .iter()
        .all(|p| words.iter().any(|w| w.starts_with(p.as_str())))
}

/// The forms an expected answer is accepted in: the whole text and each alternative
/// separated by `,` `;` `|` `/` or a line break (trimmed, empty ones left out).
pub fn answer_forms(expected: &str) -> Vec<String> {
    let whole = expected.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = Vec::new();
    if !whole.is_empty() {
        out.push(whole);
    }
    for part in expected.split([',', ';', '|', '/', '\n']) {
        let part = part.split_whitespace().collect::<Vec<_>>().join(" ");
        if !part.is_empty() && !out.contains(&part) {
            out.push(part);
        }
    }
    out
}

/// Byte range of the first `<b>…</b>` / `<strong>…</strong>` element of `html` with visible
/// text inside, and the range of its inner HTML.
fn marked_range(html: &str) -> Option<(usize, usize, (usize, usize))> {
    let lower = html.to_ascii_lowercase();
    let mut best: Option<(usize, usize, (usize, usize))> = None;
    for (open, close) in [("<b>", "</b>"), ("<strong>", "</strong>")] {
        let Some(s) = lower.find(open) else { continue };
        let Some(len) = lower[s + open.len()..].find(close) else {
            continue;
        };
        let inner = (s + open.len(), s + open.len() + len);
        if html_to_text(&html[inner.0..inner.1]).trim().is_empty() {
            continue;
        }
        let e = inner.1 + close.len();
        if best.is_none_or(|(bs, _, _)| s < bs) {
            best = Some((s, e, inner));
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

    fn reverse<'a>(html: &'a str, words: &'a [String], answers: &'a [String]) -> ContextSource<'a> {
        ContextSource {
            example_html: html,
            word_forms: words,
            asks_for_word: true,
            answer_forms: answers,
        }
    }

    fn forward<'a>(html: &'a str, words: &'a [String], answers: &'a [String]) -> ContextSource<'a> {
        ContextSource {
            asks_for_word: false,
            ..reverse(html, words, answers)
        }
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn reverse_card_blanks_the_marked_occurrence() {
        // A capture: the occurrence in bold (from its UTF-16 offsets).
        let words = strs(&["went", "go"]);
        let m = context_hint(&reverse(
            "<p>I <b>went</b> home.</p>",
            &words,
            &strs(&["go"]),
        ))
        .unwrap();
        assert_eq!(m.text, "I ___ home.");
        assert_eq!(m.html, "I <span class=\"fc-blank\">___</span> home.");
        // The marked occurrence wins over an earlier plain match of a form.
        let m = context_hint(&reverse(
            "Go! I <strong>went</strong>.",
            &strs(&["went"]),
            &[],
        ))
        .unwrap();
        assert_eq!(m.text, "Go! I ___.");
        // Escaped text stays escaped.
        let m = context_hint(&reverse("a &lt;b&gt; <b>x</b>", &[], &[])).unwrap();
        assert_eq!(m.html, "a &lt;b&gt; <span class=\"fc-blank\">___</span>");
    }

    #[test]
    fn reverse_card_finds_the_surface_then_the_term() {
        // Whole word, case-insensitive: the surface first.
        let m = context_hint(&reverse(
            "<p>We WENT home.</p>",
            &strs(&["went", "go"]),
            &strs(&["go"]),
        ))
        .unwrap();
        assert_eq!(m.text, "We ___ home.");
        // Not inside another word, then the term.
        let m = context_hint(&reverse("Let's go now.", &strs(&["gone", "go"]), &[])).unwrap();
        assert_eq!(m.text, "Let's ___ now.");
        // Phrases match across any whitespace; Turkish letters fold.
        let m = context_hint(&reverse(
            "I must pick\n up my keys",
            &strs(&["pick up"]),
            &[],
        ))
        .unwrap();
        assert_eq!(m.text, "I must ___ my keys");
        let m = context_hint(&reverse("Çiçek ÇOK güzel.", &strs(&["çok"]), &[])).unwrap();
        assert_eq!(m.text, "Çiçek ___ güzel.");
        // The word does not occur: the sentence is context as it is.
        let m = context_hint(&reverse(
            "We left early.",
            &strs(&["went", "go"]),
            &strs(&["go"]),
        ))
        .unwrap();
        assert_eq!(
            (m.text.as_str(), m.html.as_str()),
            ("We left early.", "We left early.")
        );
    }

    #[test]
    fn reverse_card_never_leaks_the_word() {
        let words = strs(&["went", "go"]);
        let answers = strs(&["go"]);
        // Still in the sentence elsewhere, or an inflection of it → no context hint.
        assert_eq!(
            context_hint(&reverse("I <b>went</b> where you go.", &words, &answers)),
            None
        );
        assert_eq!(
            context_hint(&reverse("I went and went.", &words, &answers)),
            None
        );
        assert_eq!(
            context_hint(&reverse("I <b>went</b>; she goes.", &words, &answers)),
            None
        );
        assert_eq!(
            context_hint(&reverse("She is going.", &words, &answers)),
            None
        );
        assert_eq!(
            context_hint(&reverse(
                "I picked it up.",
                &strs(&["pick up"]),
                &strs(&["pick up"])
            )),
            None
        );
        // Nothing but the term, empty sentences → none.
        assert_eq!(context_hint(&reverse("<b>went</b>!", &words, &[])), None);
        assert_eq!(context_hint(&reverse("", &words, &[])), None);
        assert_eq!(context_hint(&reverse("<b> </b> went", &words, &[])), None);
        // An answer given as HTML is compared as text.
        assert_eq!(
            context_hint(&reverse(
                "I <b>went</b> to Gitmek.",
                &[],
                &strs(&["<p>gitmek</p>"])
            )),
            None
        );
    }

    #[test]
    fn forward_card_highlights_the_word_and_blanks_only_the_answer() {
        let words = strs(&["went", "go"]);
        // The meaning is not in the sentence: the sentence as context, the word highlighted.
        let m = context_hint(&forward(
            "I <b>went</b> home.",
            &words,
            &answer_forms("gitmek"),
        ))
        .unwrap();
        assert_eq!(m.text, "I went home.");
        assert_eq!(m.html, "I <b class=\"fc-context-word\">went</b> home.");
        // Without a marked occurrence the surface (then the term) is found and highlighted.
        let m = context_hint(&forward("We WENT home.", &words, &answer_forms("gitmek"))).unwrap();
        assert_eq!(m.html, "We <b class=\"fc-context-word\">WENT</b> home.");
        // No occurrence: the sentence alone.
        let m = context_hint(&forward("Home at last.", &words, &[])).unwrap();
        assert_eq!(m.html, "Home at last.");
        // The answer occurs in the example: exactly it is blanked, the word stays.
        let m = context_hint(&forward(
            "Le <b>chat</b> dort, the cat sleeps.",
            &strs(&["chat"]),
            &answer_forms("cat, kitty"),
        ))
        .unwrap();
        assert_eq!(m.text, "Le chat dort, the ___ sleeps.");
        assert_eq!(
            m.html,
            "Le <b class=\"fc-context-word\">chat</b> dort, the <span class=\"fc-blank\">___</span> sleeps."
        );
        // Twice, or inside the word → never shown.
        assert_eq!(
            context_hint(&forward("cat and cat", &[], &strs(&["cat"]))),
            None
        );
        assert_eq!(
            context_hint(&forward("a <b>cat</b> b", &[], &strs(&["cat"]))),
            None
        );
        // Only the word → no hint.
        assert_eq!(context_hint(&forward("<b>went</b>.", &words, &[])), None);
    }

    #[test]
    fn answer_forms_split_alternatives() {
        assert_eq!(answer_forms(" gitmek "), strs(&["gitmek"]));
        assert_eq!(
            answer_forms("gitmek, yürümek; ayrılmak"),
            strs(&["gitmek, yürümek; ayrılmak", "gitmek", "yürümek", "ayrılmak"])
        );
        assert!(answer_forms("  ").is_empty());
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
