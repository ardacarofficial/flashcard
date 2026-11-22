//! Reading mode, subtitle mining and incremental reading (v0.10, docs/02 §24–§26): the pure
//! parts.
//!
//! * **Word status** ([`word_status`]): a word (lemma) is *known* when one of its cards is
//!   mature — a review card whose interval reached [`crate::stats::MATURE_DAYS`], the very
//!   definition the statistics use — *learning* when it has any other card, and otherwise
//!   follows the user's mark (*known* / *ignored*) or is *new*. Cards win over marks: once a
//!   word has a card, FSRS data is the single source of truth, so the colour of a word and the
//!   statistics never disagree.
//! * **Text summary** ([`summarize`]): running-word counts per status and the "you know 87% of
//!   this text" share.
//! * **Sentences** ([`sentence_spans`], [`sentence_at`]): the context a card is made with.
//! * **Incremental reading** ([`next_reading_interval`], [`reading_queue_key`]): the reading
//!   queue's own spacing and order.
//! * **Cards from text** ([`cloze_from_spans`], [`emphasize`], [`format_timestamp`]).

use crate::card::CardState;
use crate::content::html::escape_html;
use crate::stats::MATURE_DAYS;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Colour of a word in reading mode (LingQ style).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum WordStatus {
    /// Never seen: no card, no mark (blue).
    New,
    /// Has a card that is not mature yet (yellow).
    Learning,
    /// Has a mature card, or marked "I know this" (no colour).
    Known,
    /// Marked as not worth learning: names, numbers, foreign words (grey).
    Ignored,
}

impl WordStatus {
    /// Every status, in display order.
    pub const ALL: [Self; 4] = [Self::New, Self::Learning, Self::Known, Self::Ignored];

    /// Storage / wire string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Learning => "learning",
            Self::Known => "known",
            Self::Ignored => "ignored",
        }
    }
}

/// What the user can mark a word as (`word_marks.status`). Only matters while the word has no
/// card ([`word_status`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum WordMark {
    /// "I know this".
    Known,
    /// "Do not track this word".
    Ignored,
}

impl WordMark {
    /// Storage string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Known => "known",
            Self::Ignored => "ignored",
        }
    }

    /// From its storage string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "known" => Some(Self::Known),
            "ignored" => Some(Self::Ignored),
            _ => None,
        }
    }
}

/// The FSRS facts of one card of a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardEvidence {
    /// Learning state.
    pub state: CardState,
    /// Current interval in study days (`None` unless a review card).
    pub interval_days: Option<u32>,
}

impl CardEvidence {
    /// Mature: a review card whose interval reached [`MATURE_DAYS`] (same as
    /// `stats::CardFact::is_mature`).
    pub fn is_mature(&self) -> bool {
        self.state == CardState::Review && self.interval_days.unwrap_or(0) >= MATURE_DAYS
    }
}

/// Status of a word from its cards and the user's mark. Cards win: any mature card → known,
/// any other card → learning; without cards the mark decides, else new.
pub fn word_status(cards: &[CardEvidence], mark: Option<WordMark>) -> WordStatus {
    if cards.iter().any(CardEvidence::is_mature) {
        WordStatus::Known
    } else if !cards.is_empty() {
        WordStatus::Learning
    } else {
        match mark {
            Some(WordMark::Known) => WordStatus::Known,
            Some(WordMark::Ignored) => WordStatus::Ignored,
            None => WordStatus::New,
        }
    }
}

/// Per-status counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct WordCounts {
    /// New.
    pub new: u32,
    /// Learning.
    pub learning: u32,
    /// Known.
    pub known: u32,
    /// Ignored.
    pub ignored: u32,
}

impl WordCounts {
    /// Counts one more word of status `s`.
    pub fn add(&mut self, s: WordStatus) {
        let slot = match s {
            WordStatus::New => &mut self.new,
            WordStatus::Learning => &mut self.learning,
            WordStatus::Known => &mut self.known,
            WordStatus::Ignored => &mut self.ignored,
        };
        *slot = slot.saturating_add(1);
    }

    /// Sum of all statuses.
    pub fn total(&self) -> u32 {
        self.new
            .saturating_add(self.learning)
            .saturating_add(self.known)
            .saturating_add(self.ignored)
    }
}

/// Word statistics of a text.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TextSummary {
    /// Running words per status (every occurrence counts).
    pub running: WordCounts,
    /// Distinct lemmas per status.
    pub unique: WordCounts,
    /// Known running words over running words that are not ignored, 0–100, one decimal
    /// ("you know 87% of this text"). 0 for a text without countable words.
    pub known_pct: f64,
}

/// Summarizes the `(lemma, status)` of every running word of a text.
pub fn summarize<'a>(words: impl IntoIterator<Item = (&'a str, WordStatus)>) -> TextSummary {
    let mut running = WordCounts::default();
    let mut seen: HashMap<&str, WordStatus> = HashMap::new();
    for (lemma, status) in words {
        running.add(status);
        seen.entry(lemma).or_insert(status);
    }
    let mut unique = WordCounts::default();
    for s in seen.into_values() {
        unique.add(s);
    }
    let counted = running.total().saturating_sub(running.ignored);
    let known_pct = if counted == 0 {
        0.0
    } else {
        (f64::from(running.known) * 1000.0 / f64::from(counted)).round() / 10.0
    };
    TextSummary {
        running,
        unique,
        known_pct,
    }
}

// ------------------------------------------------------------------ headwords

/// The headword part of a card's word field: `"run (v.)"`, `"run, ran, run"`, `"run / jog"`
/// → `"run"`. Cuts at the first bracket, list separator or line break and trims.
pub fn clean_headword(field_text: &str) -> &str {
    let cut = field_text
        .find(['(', '[', ',', ';', '/', '=', '\n', '—', '–', '|'])
        .unwrap_or(field_text.len());
    let head = &field_text[..cut];
    let head = head.split(" - ").next().unwrap_or(head);
    head.trim()
}

/// Vocabulary key of a headword from its lemmas: the lemmas joined by a space; an English
/// infinitive marker is dropped (`to run` → `run`). `None` for a headword without words or
/// longer than [`MAX_HEADWORD_WORDS`] words (a sentence, not a word).
pub fn headword_key(lemmas: &[String], language: &str) -> Option<String> {
    let mut words: &[String] = lemmas;
    if words.len() == 2 && words[0] == "to" && primary_subtag(language) == "en" {
        words = &words[1..];
    }
    if words.is_empty() || words.len() > MAX_HEADWORD_WORDS {
        return None;
    }
    Some(words.join(" "))
}

/// Longest card headword (in words) that still counts as vocabulary.
pub const MAX_HEADWORD_WORDS: usize = 4;

fn primary_subtag(language: &str) -> String {
    language
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

// ------------------------------------------------------------------ sentences

fn is_terminator(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…' | '。' | '！' | '？' | '؟')
}

fn is_closer(c: char) -> bool {
    matches!(
        c,
        '"' | '\'' | '”' | '’' | '»' | ')' | ']' | '」' | '』' | '›'
    )
}

/// Byte spans of the sentences of `text`, trimmed of surrounding white space. A sentence
/// ends at a line break, or at `. ! ? …` (plus closing quotes/brackets) followed by white
/// space and an upper-case letter, digit or opening quote — so `Mr. smith`-style
/// abbreviations followed by lower case and decimals (`3.5`) do not split.
pub fn sentence_spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let (pos, c) = chars[i];
        if c == '\n' {
            push_trimmed(text, start, pos, &mut out);
            start = pos + c.len_utf8();
            i += 1;
            continue;
        }
        if is_terminator(c) {
            let mut j = i + 1;
            while j < chars.len() && (is_terminator(chars[j].1) || is_closer(chars[j].1)) {
                j += 1;
            }
            let end = chars.get(j).map_or(text.len(), |&(p, _)| p);
            let mut k = j;
            while k < chars.len() && chars[k].1.is_whitespace() && chars[k].1 != '\n' {
                k += 1;
            }
            let breaks = match chars.get(k) {
                None => true,
                Some(&(_, '\n')) => true,
                Some(&(_, next)) if k > j => {
                    next.is_uppercase()
                        || next.is_numeric()
                        || matches!(
                            next,
                            '"' | '“' | '«' | '‘' | '\'' | '(' | '¿' | '¡' | '-' | '—'
                        )
                }
                _ => false,
            };
            if breaks {
                push_trimmed(text, start, end, &mut out);
                start = end;
                i = j;
                continue;
            }
        }
        i += 1;
    }
    push_trimmed(text, start, text.len(), &mut out);
    out
}

fn push_trimmed(text: &str, start: usize, end: usize, out: &mut Vec<(usize, usize)>) {
    let s = &text[start..end];
    let lead = s.len() - s.trim_start().len();
    let trail = s.len() - s.trim_end().len();
    if lead + trail < s.len() {
        out.push((start + lead, end - trail));
    }
}

/// The sentence containing byte `offset` (or the nearest one after it).
pub fn sentence_at(text: &str, offset: usize) -> Option<(usize, usize)> {
    let spans = sentence_spans(text);
    spans
        .iter()
        .copied()
        .find(|&(s, e)| offset >= s && offset < e)
        .or_else(|| spans.iter().copied().find(|&(s, _)| s >= offset))
        .or_else(|| spans.last().copied())
}

// ------------------------------------------------------------------ incremental reading

/// Priority of a text in the reading queue: how fast its intervals grow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ReadingPriority {
    /// Back soon (×1.5).
    High,
    /// Default (×2).
    #[default]
    Normal,
    /// Whenever (×3).
    Low,
}

impl ReadingPriority {
    /// Storage string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Normal => "normal",
            Self::Low => "low",
        }
    }

    /// From its storage string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "high" => Some(Self::High),
            "normal" => Some(Self::Normal),
            "low" => Some(Self::Low),
            _ => None,
        }
    }

    /// Interval growth factor ×10 (integer maths keeps it deterministic).
    const fn factor_x10(self) -> u64 {
        match self {
            Self::High => 15,
            Self::Normal => 20,
            Self::Low => 30,
        }
    }

    /// First interval in days.
    const fn first_days(self) -> u32 {
        match self {
            Self::High | Self::Normal => 1,
            Self::Low => 2,
        }
    }
}

/// Longest reading-queue interval.
pub const MAX_READING_INTERVAL_DAYS: u32 = 365;

/// Days until a queued text comes back after a reading session (SuperMemo-style "A-factor"):
/// the first interval by priority, then the previous one × 1.5 / 2 / 3 (rounded up, at least
/// one day longer), capped at [`MAX_READING_INTERVAL_DAYS`].
pub fn next_reading_interval(previous_days: Option<u32>, priority: ReadingPriority) -> u32 {
    let days = match previous_days.filter(|d| *d > 0) {
        None => priority.first_days(),
        Some(prev) => {
            let grown = (u64::from(prev) * priority.factor_x10()).div_ceil(10);
            u32::try_from(grown)
                .unwrap_or(u32::MAX)
                .max(prev.saturating_add(1))
        }
    };
    days.min(MAX_READING_INTERVAL_DAYS)
}

/// Rank of a priority in the queue (high first).
const fn priority_rank(p: ReadingPriority) -> i64 {
    match p {
        ReadingPriority::High => 0,
        ReadingPriority::Normal => 1,
        ReadingPriority::Low => 2,
    }
}

/// Sort key of a queued text (ascending = read first): every **due** text (due ≤ `now`)
/// comes before any text not due yet, ordered by **priority** (high, normal, low) and then
/// by how long it has been due; texts not due yet follow by due time (priority breaking
/// ties); texts without a due time come last.
pub fn reading_queue_key(
    due_ms: Option<i64>,
    priority: ReadingPriority,
    now_ms: i64,
) -> (u8, i64, i64) {
    let rank = priority_rank(priority);
    match due_ms {
        Some(due) if due <= now_ms => (0, rank, due),
        Some(due) => (1, due, rank),
        None => (2, rank, 0),
    }
}

// ------------------------------------------------------------------ cards from text

/// Why a cloze could not be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClozeSpanError {
    /// No span.
    #[error("no span to hide")]
    Empty,
    /// A span is empty, out of range or not on character boundaries.
    #[error("span {0}..{1} is not a range of the text")]
    OutOfRange(usize, usize),
    /// Two spans overlap.
    #[error("spans overlap")]
    Overlap,
    /// A span contains cloze syntax (`{{`, `}}`, `::`).
    #[error("span contains cloze syntax")]
    Syntax,
}

/// Cloze text (`{{c1::…}}`) of a plain `text` hiding the byte `spans`, HTML-escaped. Each span
/// becomes its own card (`c1`, `c2`, … in text order) unless `together` (all `c1`).
pub fn cloze_from_spans(
    text: &str,
    spans: &[(usize, usize)],
    together: bool,
) -> Result<String, ClozeSpanError> {
    if spans.is_empty() {
        return Err(ClozeSpanError::Empty);
    }
    let mut sorted = spans.to_vec();
    sorted.sort_unstable();
    for &(s, e) in &sorted {
        if s >= e || e > text.len() || !text.is_char_boundary(s) || !text.is_char_boundary(e) {
            return Err(ClozeSpanError::OutOfRange(s, e));
        }
        let hidden = &text[s..e];
        if hidden.contains("{{") || hidden.contains("}}") || hidden.contains("::") {
            return Err(ClozeSpanError::Syntax);
        }
    }
    if sorted.windows(2).any(|w| w[1].0 < w[0].1) {
        return Err(ClozeSpanError::Overlap);
    }
    let mut out = String::with_capacity(text.len() + sorted.len() * 10);
    let mut pos = 0;
    for (n, &(s, e)) in sorted.iter().enumerate() {
        out.push_str(&escape_html(&text[pos..s]));
        let num = if together { 1 } else { n + 1 };
        out.push_str(&format!("{{{{c{num}::{}}}}}", escape_html(&text[s..e])));
        pos = e;
    }
    out.push_str(&escape_html(&text[pos..]));
    Ok(out)
}

/// `sentence` as HTML with the byte range `start..end` in bold (the target word of a card).
/// An invalid range leaves the sentence plain.
pub fn emphasize(sentence: &str, start: usize, end: usize) -> String {
    if start < end
        && end <= sentence.len()
        && sentence.is_char_boundary(start)
        && sentence.is_char_boundary(end)
    {
        format!(
            "{}<b>{}</b>{}",
            escape_html(&sentence[..start]),
            escape_html(&sentence[start..end]),
            escape_html(&sentence[end..])
        )
    } else {
        escape_html(sentence)
    }
}

/// `h:mm:ss` (or `m:ss` under an hour) of a media position in milliseconds.
pub fn format_timestamp(ms: u64) -> String {
    let secs = ms / 1000;
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests;
