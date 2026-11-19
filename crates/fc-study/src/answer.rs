//! Typed-answer checking (docs/02 §7): exact, normalized, typo-tolerant and multi-answer
//! checkers, graded `correct` / `almost` / `wrong` with a character diff.
//!
//! [`check`] is the graded entry point the app uses; each checker is also exposed through
//! the [`AnswerChecker`] strategy interface (`correct` ⇔ grade `correct`), so plugins can add
//! more (AI meaning check — v0.8). The checker only **suggests** a rating
//! ([`Grade::suggested_rating`]); the user decides unless auto-grading is on.

use crate::render::html_to_text;
use fc_core::card::Rating;
use fc_providers::strategy::{
    AnswerChecker, AnswerVerdict, DiffSpan, DiffSpanKind, StrategyResult,
};
use fc_providers::ProviderInfo;
use serde::{Deserialize, Serialize};

/// Built-in checkers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckerKind {
    /// Character-for-character (surrounding whitespace ignored).
    Exact,
    /// Case, surrounding/duplicate whitespace, punctuation and (optionally) Turkish letter
    /// variants ignored.
    Normalized,
    /// Normalized, and a few typos count as "almost".
    Tolerant,
    /// Several accepted answers separated by `;` or `|`, each checked tolerantly.
    Multi,
}

impl CheckerKind {
    /// All, in settings order.
    pub const ALL: [CheckerKind; 4] = [Self::Exact, Self::Normalized, Self::Tolerant, Self::Multi];

    /// Stable id (`exact`, `normalized`, `tolerant`, `multi`).
    pub const fn id(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Normalized => "normalized",
            Self::Tolerant => "tolerant",
            Self::Multi => "multi",
        }
    }

    /// Parses an id.
    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }
}

/// Checker options (`config` of the strategy interface; every field optional).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CheckOptions {
    /// Treat `ı/i, ş/s, ğ/g, ü/u, ö/o, ç/c` as equal (normalized checkers).
    pub fold_turkish: bool,
    /// Ignore punctuation (normalized checkers).
    pub ignore_punctuation: bool,
    /// Typos allowed for "almost", as a share of the expected length (at least one).
    pub max_typo_ratio: f64,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self {
            fold_turkish: true,
            ignore_punctuation: true,
            max_typo_ratio: 0.2,
        }
    }
}

/// Graded result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grade {
    /// Accepted.
    Correct,
    /// Close (typo-level difference).
    Almost,
    /// Not accepted.
    Wrong,
}

impl Grade {
    /// Rating the checker suggests: correct → Good, almost → Hard, wrong → Again.
    pub const fn suggested_rating(self) -> Rating {
        match self {
            Self::Correct => Rating::Good,
            Self::Almost => Rating::Hard,
            Self::Wrong => Rating::Again,
        }
    }
}

/// Result of [`check`].
#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    /// Grade.
    pub grade: Grade,
    /// Similarity 0–1 (1 = equal after the checker's normalization).
    pub score: f32,
    /// Diff of the given answer against the (best matching) expected one.
    pub diff: Vec<DiffSpan>,
    /// The expected answer the given one was compared with (for multi: the closest one).
    pub matched: String,
}

/// Checks `given` against `expected` (expected may be HTML: it is reduced to text first).
pub fn check(kind: CheckerKind, expected: &str, given: &str, opts: &CheckOptions) -> Checked {
    let expected = html_to_text(expected);
    let given = given.trim();
    match kind {
        CheckerKind::Exact => single(&expected, given, opts, Mode::Exact),
        CheckerKind::Normalized => single(&expected, given, opts, Mode::Normalized),
        CheckerKind::Tolerant => single(&expected, given, opts, Mode::Tolerant),
        CheckerKind::Multi => {
            let mut best: Option<Checked> = None;
            for alt in expected
                .split([';', '|'])
                .map(str::trim)
                .filter(|a| !a.is_empty())
            {
                let c = single(alt, given, opts, Mode::Tolerant);
                let better = best.as_ref().is_none_or(|b| {
                    rank(c.grade) > rank(b.grade) || (c.grade == b.grade && c.score > b.score)
                });
                if better {
                    best = Some(c);
                }
            }
            best.unwrap_or_else(|| single(&expected, given, opts, Mode::Tolerant))
        }
    }
}

const fn rank(g: Grade) -> u8 {
    match g {
        Grade::Correct => 2,
        Grade::Almost => 1,
        Grade::Wrong => 0,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Exact,
    Normalized,
    Tolerant,
}

fn single(expected: &str, given: &str, opts: &CheckOptions, mode: Mode) -> Checked {
    let expected = expected.trim();
    let (e, g) = match mode {
        Mode::Exact => (expected.to_owned(), given.to_owned()),
        Mode::Normalized | Mode::Tolerant => (normalize(expected, opts), normalize(given, opts)),
    };
    let e_chars: Vec<char> = e.chars().collect();
    let g_chars: Vec<char> = g.chars().collect();
    let distance = levenshtein(&e_chars, &g_chars);
    let longest = e_chars.len().max(g_chars.len()).max(1);
    let score = 1.0 - (distance as f32 / longest as f32);
    let allowed =
        ((e_chars.len() as f64 * opts.max_typo_ratio.clamp(0.0, 1.0)).floor() as usize).max(1);
    let grade = if distance == 0 {
        Grade::Correct
    } else if mode == Mode::Tolerant
        && distance <= allowed
        && !g_chars.is_empty()
        && !e_chars.is_empty()
    {
        Grade::Almost
    } else {
        Grade::Wrong
    };
    let fold = mode != Mode::Exact;
    Checked {
        grade,
        score,
        diff: diff(expected, given, |a, b| {
            if fold {
                fold_char(a, opts) == fold_char(b, opts)
            } else {
                a == b
            }
        }),
        matched: expected.to_owned(),
    }
}

/// Case/accents folding of one character for comparisons.
fn fold_char(c: char, opts: &CheckOptions) -> char {
    let lower = match c {
        'İ' => 'i',
        'I' => 'i',
        _ => c.to_lowercase().next().unwrap_or(c),
    };
    if !opts.fold_turkish {
        return lower;
    }
    match lower {
        'ı' => 'i',
        'ş' => 's',
        'ğ' => 'g',
        'ü' => 'u',
        'ö' => 'o',
        'ç' => 'c',
        other => other,
    }
}

/// Normalized comparison form: folded, punctuation dropped (optional), whitespace collapsed.
pub fn normalize(s: &str, opts: &CheckOptions) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if opts.ignore_punctuation && !c.is_alphanumeric() {
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        out.push(fold_char(c, opts));
    }
    out
}

/// Edit distance (insert/delete/substitute = 1).
pub fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Longest inputs diffed character by character; longer ones get a whole-string diff.
const MAX_DIFF_CHARS: usize = 2_000;

/// Character diff of `given` against `expected` (LCS): `same` runs, `missing` (in expected,
/// not given) and `extra` (given, not expected), adjacent runs of a kind merged.
pub fn diff(expected: &str, given: &str, eq: impl Fn(char, char) -> bool) -> Vec<DiffSpan> {
    let e: Vec<char> = expected.chars().collect();
    let g: Vec<char> = given.chars().collect();
    let mut spans: Vec<DiffSpan> = Vec::new();
    let mut push = |kind: DiffSpanKind, c: char| match spans.last_mut() {
        Some(last) if last.kind == kind => last.text.push(c),
        _ => spans.push(DiffSpan {
            kind,
            text: c.to_string(),
        }),
    };
    if e.len() > MAX_DIFF_CHARS || g.len() > MAX_DIFF_CHARS {
        e.iter().for_each(|c| push(DiffSpanKind::Missing, *c));
        g.iter().for_each(|c| push(DiffSpanKind::Extra, *c));
        return spans;
    }
    // lcs[i][j] = LCS length of e[i..], g[j..].
    let mut lcs = vec![vec![0u32; g.len() + 1]; e.len() + 1];
    for i in (0..e.len()).rev() {
        for j in (0..g.len()).rev() {
            lcs[i][j] = if eq(e[i], g[j]) {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < e.len() && j < g.len() {
        if eq(e[i], g[j]) {
            push(DiffSpanKind::Same, g[j]);
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            push(DiffSpanKind::Missing, e[i]);
            i += 1;
        } else {
            push(DiffSpanKind::Extra, g[j]);
            j += 1;
        }
    }
    e[i..].iter().for_each(|c| push(DiffSpanKind::Missing, *c));
    g[j..].iter().for_each(|c| push(DiffSpanKind::Extra, *c));
    spans
}

/// A built-in checker through the strategy interface.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinChecker(pub CheckerKind);

impl AnswerChecker for BuiltinChecker {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.0.id().to_owned(),
            name_key: format!("study.checker.{}", self.0.id()),
            capabilities: Vec::new(),
            settings: Vec::new(),
            secret_keys: Vec::new(),
            local: true,
        }
    }

    fn check(
        &self,
        expected: &str,
        given: &str,
        config: &serde_json::Value,
    ) -> StrategyResult<AnswerVerdict> {
        let opts: CheckOptions = if config.is_null() {
            CheckOptions::default()
        } else {
            serde_json::from_value(config.clone())
                .map_err(|e| fc_providers::strategy::StrategyError(e.to_string()))?
        };
        let c = check(self.0, expected, given, &opts);
        Ok(AnswerVerdict {
            correct: c.grade == Grade::Correct,
            score: c.score,
            diff: c.diff,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn grade(kind: CheckerKind, e: &str, g: &str) -> Grade {
        check(kind, e, g, &CheckOptions::default()).grade
    }

    #[test]
    fn exact() {
        assert_eq!(grade(CheckerKind::Exact, "Elma", " Elma "), Grade::Correct);
        assert_eq!(grade(CheckerKind::Exact, "Elma", "elma"), Grade::Wrong);
    }

    #[test]
    fn normalized_ignores_case_punctuation_and_turkish_variants() {
        let n = CheckerKind::Normalized;
        assert_eq!(grade(n, "Işık!", "isik"), Grade::Correct);
        assert_eq!(grade(n, "İstanbul", "istanbul"), Grade::Correct);
        assert_eq!(grade(n, "to  be, or", "TO BE OR"), Grade::Correct);
        assert_eq!(grade(n, "<b>çiçek</b>", "cicek"), Grade::Correct);
        assert_eq!(grade(n, "elma", "elmo"), Grade::Wrong);
        let strict = CheckOptions {
            fold_turkish: false,
            ..CheckOptions::default()
        };
        assert_eq!(check(n, "ışık", "isik", &strict).grade, Grade::Wrong);
        assert_eq!(check(n, "Şeker", "şeker", &strict).grade, Grade::Correct);
    }

    #[test]
    fn tolerant_grades_typos_as_almost() {
        let t = CheckerKind::Tolerant;
        assert_eq!(grade(t, "necessary", "neccessary"), Grade::Almost);
        assert_eq!(grade(t, "necessary", "Necessary"), Grade::Correct);
        assert_eq!(grade(t, "necessary", "nothing"), Grade::Wrong);
        assert_eq!(grade(t, "ev", "el"), Grade::Almost); // one typo always allowed
        assert_eq!(grade(t, "ev", ""), Grade::Wrong);
        assert_eq!(Grade::Almost.suggested_rating(), Rating::Hard);
    }

    #[test]
    fn multi_accepts_alternatives() {
        let m = CheckerKind::Multi;
        assert_eq!(grade(m, "elma; apple", "apple"), Grade::Correct);
        assert_eq!(grade(m, "elma | apple", "elma"), Grade::Correct);
        assert_eq!(grade(m, "elma; apple", "aple"), Grade::Almost);
        let c = check(m, "elma; apple", "aple", &CheckOptions::default());
        assert_eq!(c.matched, "apple");
        assert_eq!(grade(m, "elma; apple", "armut"), Grade::Wrong);
        assert_eq!(grade(m, ";", "x"), Grade::Wrong);
    }

    #[test]
    fn diff_marks_missing_and_extra() {
        let d = diff("house", "hoxse", |a, b| a == b);
        let kinds: Vec<(DiffSpanKind, &str)> =
            d.iter().map(|s| (s.kind, s.text.as_str())).collect();
        assert_eq!(
            kinds,
            vec![
                (DiffSpanKind::Same, "ho"),
                (DiffSpanKind::Missing, "u"),
                (DiffSpanKind::Extra, "x"),
                (DiffSpanKind::Same, "se"),
            ]
        );
        assert!(diff("", "", |a, b| a == b).is_empty());
        assert_eq!(levenshtein(&['a', 'b'], &[]), 2);
    }

    #[test]
    fn strategy_interface() {
        let c = BuiltinChecker(CheckerKind::Tolerant);
        assert_eq!(c.info().id, "tolerant");
        let v = c.check("apple", "apple", &serde_json::Value::Null).unwrap();
        assert!(v.correct && (v.score - 1.0).abs() < f32::EPSILON);
        assert!(c
            .check("a", "b", &serde_json::json!({"max_typo_ratio": "x"}))
            .is_err());
        assert_eq!(CheckerKind::parse("multi"), Some(CheckerKind::Multi));
        assert_eq!(CheckerKind::parse("x"), None);
    }
}
