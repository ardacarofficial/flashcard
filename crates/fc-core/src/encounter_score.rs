//! v1.4 (W3, docs/03 §4.4): **the most teachable scene.** Ranks the example sentences of one
//! word (the encounters of a note, or the candidate sentences of reading mode) and says *why*.
//!
//! Pure: the caller tokenizes the sentences (the language provider), looks every word up in the
//! word index (its [`WordStatus`]) and in the frequency pack (its rank), and passes the facts in
//! as [`Scene`]s. [`rank_scenes`] scores each scene as a sum of explained points
//! ([`SceneReason`]) and orders them best first; near-duplicate sentences (case, punctuation,
//! overlapping subtitle lines) are reported as duplicates of each other.
//!
//! What counts (points in [`points`]):
//! * **the target really occurs** (its lemma or surface form among the words; an expression's
//!   words in order, a few words apart at most) — a scene without it is nearly useless;
//! * **other unknown words**: words the index does not know (new), except function words and
//!   very common words (rank ≤ [`ScoreOptions::basic_rank`]), which a learner is assumed to
//!   know. **Proper names never count**: a capitalised word in the middle of a sentence, or —
//!   when a frequency pack covers the language — a word the pack does not know (a name, a
//!   made-up word) unless the index has it. Words being learnt count a little;
//! * **completeness**: a line cut at the start (`...and then`, a lowercase start) or end
//!   (`…`, `,`), a dangling dash (`If you —`), no closing punctuation, two speakers in one
//!   line (`- Where? - Here.`), too short or too long;
//! * **audio** and **image** present;
//! * **frequency of the surrounding words** (median rank): common context is easier.

use crate::reading::WordStatus;
use crate::word_data::is_function_word;
use serde::{Deserialize, Serialize};

/// Points of each reason (the score is their sum).
pub mod points {
    /// The target word occurs in the sentence.
    pub const TARGET_PRESENT: i32 = 10;
    /// The target word does not occur (a wrong line, a wrong offset).
    pub const TARGET_MISSING: i32 = -40;
    /// No unknown word besides the target.
    pub const NO_OTHER_UNKNOWN: i32 = 10;
    /// Per unknown word besides the target.
    pub const PER_UNKNOWN: i32 = -10;
    /// Per word still being learnt.
    pub const PER_LEARNING: i32 = -3;
    /// The line starts in the middle of a sentence.
    pub const CUT_START: i32 = -10;
    /// The line stops in the middle of a sentence (`…`, `,`).
    pub const CUT_END: i32 = -10;
    /// The line ends with a dash (interrupted speech).
    pub const DANGLING_DASH: i32 = -15;
    /// No closing punctuation.
    pub const NO_END_PUNCTUATION: i32 = -4;
    /// Two speakers in one line.
    pub const TWO_SPEAKERS: i32 = -5;
    /// Fewer than [`super::MIN_WORDS`] words.
    pub const TOO_SHORT: i32 = -15;
    /// More than [`super::MAX_WORDS`] words.
    pub const TOO_LONG: i32 = -10;
    /// A comfortable length.
    pub const GOOD_LENGTH: i32 = 4;
    /// The scene's sound is there.
    pub const HAS_AUDIO: i32 = 8;
    /// The scene's picture is there.
    pub const HAS_IMAGE: i32 = 4;
    /// The surrounding words are common (median rank ≤ [`super::COMMON_RANK`]).
    pub const COMMON_WORDS: i32 = 5;
    /// The surrounding words are rare (median rank > [`super::RARE_RANK`]).
    pub const RARE_WORDS: i32 = -5;
}

/// Fewer words than this: too short to teach.
pub const MIN_WORDS: usize = 4;
/// More words than this: too long to take in at a glance.
pub const MAX_WORDS: usize = 24;
/// A comfortable length (words).
pub const GOOD_LENGTH: std::ops::RangeInclusive<usize> = 6..=16;
/// Median rank of the context at or under this: common words.
pub const COMMON_RANK: u32 = 2_000;
/// Median rank of the context over this: rare words.
pub const RARE_RANK: u32 = 6_000;
/// Default [`ScoreOptions::basic_rank`]: the A1 band of the frequency pack.
pub const DEFAULT_BASIC_RANK: u32 = 1_000;
/// How many words an expression's parts may be apart (`pick it up`).
const EXPRESSION_GAP: usize = 3;

/// One word of a scene, with what the index and the frequency pack say about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneWord {
    /// Byte range in the sentence.
    pub start: usize,
    /// Byte end.
    pub end: usize,
    /// As written.
    pub text: String,
    /// Lemma (lowercase).
    pub lemma: String,
    /// Word index status of the lemma (`New` when the index has no row).
    pub status: WordStatus,
    /// Frequency rank (1 = most frequent; `None` = not in the pack, or no pack).
    pub rank: Option<u32>,
}

/// A sentence to rank.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scene {
    /// The caller's id (an encounter's capture id, a candidate's index).
    pub id: String,
    /// The sentence as shown.
    pub sentence: String,
    /// Its words (from the language provider).
    pub words: Vec<SceneWord>,
    /// Byte range of the target as the source marked it (may be stale; checked, not trusted).
    pub target: Option<(usize, usize)>,
    /// The target as written in this scene (`picked`), when known.
    pub surface: Option<String>,
    /// The scene's sound is stored.
    pub has_audio: bool,
    /// The scene's picture is stored.
    pub has_image: bool,
}

/// What the ranking needs besides the scenes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreOptions {
    /// Index language (`en`).
    pub language: String,
    /// The word the scenes are examples of: its lemma or expression (`pick up`).
    pub target: String,
    /// A frequency pack covers the language (then a word unknown to it is taken for a name).
    pub frequency_pack: bool,
    /// A new word ranked at or under this is assumed known (basic vocabulary).
    pub basic_rank: u32,
}

/// Why a scene got its points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SceneReasonCode {
    /// The target word occurs.
    TargetPresent,
    /// The target word does not occur.
    TargetMissing,
    /// No unknown word besides the target.
    NoOtherUnknown,
    /// Unknown words besides the target (`words`).
    OtherUnknown,
    /// Words still being learnt (`words`).
    LearningWords,
    /// Proper names, not counted as unknown (`words`; 0 points).
    NamesIgnored,
    /// The line starts in the middle of a sentence.
    CutStart,
    /// The line stops in the middle of a sentence.
    CutEnd,
    /// The line ends with a dash.
    DanglingDash,
    /// No closing punctuation.
    NoEndPunctuation,
    /// Two speakers in one line.
    TwoSpeakers,
    /// Too few words (`count`).
    TooShort,
    /// Too many words (`count`).
    TooLong,
    /// A comfortable length (`count`).
    GoodLength,
    /// Sound present.
    HasAudio,
    /// Picture present.
    HasImage,
    /// Common surrounding words (`count` = median rank).
    CommonWords,
    /// Rare surrounding words (`count` = median rank).
    RareWords,
}

/// One explained part of a score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SceneReason {
    /// What.
    pub code: SceneReasonCode,
    /// Points it adds (negative = a drawback).
    pub points: i32,
    /// A number that goes with it (words, median rank), when there is one.
    pub count: Option<u32>,
    /// The words concerned (unknown words, names…), as written.
    pub words: Vec<String>,
}

/// A ranked scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneScore {
    /// The scene's id.
    pub id: String,
    /// Sum of the reasons' points (higher = more teachable).
    pub score: i32,
    /// Why, in a stable order.
    pub reasons: Vec<SceneReason>,
    /// The target occurs.
    pub target_found: bool,
    /// Unknown words besides the target, as written.
    pub unknown: Vec<String>,
    /// Ids of the scenes that are near-duplicates of this one (best ranked first).
    pub duplicates: Vec<String>,
}

fn reason(code: SceneReasonCode, points: i32) -> SceneReason {
    SceneReason {
        code,
        points,
        count: None,
        words: Vec::new(),
    }
}

fn count_u32(n: usize) -> Option<u32> {
    u32::try_from(n).ok()
}

/// Ranks `scenes` best first (ties keep the given order), with reasons and duplicates.
pub fn rank_scenes(scenes: &[Scene], opts: &ScoreOptions) -> Vec<SceneScore> {
    let mut scored: Vec<(usize, SceneScore)> = scenes
        .iter()
        .enumerate()
        .map(|(i, s)| (i, score_scene(s, opts)))
        .collect();
    scored.sort_by(|(ia, a), (ib, b)| b.score.cmp(&a.score).then(ia.cmp(ib)));
    let order: Vec<usize> = scored.iter().map(|(i, _)| *i).collect();
    let groups = duplicate_groups(scenes);
    let mut out: Vec<SceneScore> = scored.into_iter().map(|(_, s)| s).collect();
    for (pos, &i) in order.iter().enumerate() {
        out[pos].duplicates = order
            .iter()
            .filter(|&&j| j != i && groups[j] == groups[i])
            .map(|&j| scenes[j].id.clone())
            .collect();
    }
    out
}

/// Which words of `scene` are the target: `(found, covered word indexes)`.
fn find_target(scene: &Scene, opts: &ScoreOptions) -> (bool, Vec<usize>) {
    let parts: Vec<String> = opts
        .target
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let surface = scene
        .surface
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    let words = &scene.words;
    let matches =
        |w: &SceneWord, part: &str| w.lemma.to_lowercase() == part || w.text.to_lowercase() == part;
    // A one-word target (or the surface form as written in this scene).
    if parts.len() <= 1 {
        let hits: Vec<usize> = words
            .iter()
            .enumerate()
            .filter(|(_, w)| {
                parts.first().is_some_and(|p| matches(w, p))
                    || surface
                        .as_deref()
                        .is_some_and(|s| w.text.to_lowercase() == s)
            })
            .map(|(i, _)| i)
            .collect();
        if !hits.is_empty() {
            return (true, hits);
        }
    } else {
        // An expression: its parts in order, each at most EXPRESSION_GAP words after the last.
        for first in 0..words.len() {
            if !matches(&words[first], &parts[0]) {
                continue;
            }
            let mut covered = vec![first];
            let mut at = first;
            for part in &parts[1..] {
                let next = (at + 1..words.len().min(at + 2 + EXPRESSION_GAP))
                    .find(|&j| matches(&words[j], part));
                match next {
                    Some(j) => {
                        covered.push(j);
                        at = j;
                    }
                    None => break,
                }
            }
            if covered.len() == parts.len() {
                return (true, covered);
            }
        }
    }
    // The marked range, when the text there is the surface form as written.
    if let (Some((s, e)), Some(surface)) = (scene.target, surface.as_deref()) {
        if let Some(t) = scene.sentence.get(s..e) {
            if t.trim().to_lowercase() == surface {
                let covered = words
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| w.start < e && w.end > s)
                    .map(|(i, _)| i)
                    .collect();
                return (true, covered);
            }
        }
    }
    // The surface form as a phrase of the sentence (`picked it up`).
    if let Some(surface) = surface.as_deref() {
        let lower = scene.sentence.to_lowercase();
        if surface.contains(' ') && lower.contains(surface) {
            return (true, Vec::new());
        }
    }
    (false, Vec::new())
}

/// Whether the word at `i` starts a sentence (or a speaker's line): first word, or only
/// sentence punctuation, quotes, dashes or a music sign between it and the previous word.
fn starts_sentence(scene: &Scene, i: usize) -> bool {
    let gap_start = if i == 0 { 0 } else { scene.words[i - 1].end };
    let Some(gap) = scene.sentence.get(gap_start..scene.words[i].start) else {
        return true;
    };
    i == 0
        || gap.chars().any(|c| {
            matches!(
                c,
                '.' | '!' | '?' | '…' | ':' | '"' | '“' | '-' | '–' | '—' | '♪' | '(' | '[' | '\n'
            )
        })
}

fn is_name(scene: &Scene, i: usize, opts: &ScoreOptions) -> bool {
    let w = &scene.words[i];
    if matches!(w.status, WordStatus::Known | WordStatus::Learning) {
        return false; // the index knows it: a word, whatever its case
    }
    let pronoun_i = w.text == "I" || w.text.starts_with("I'") || w.text.starts_with("I’");
    let capital = w.text.chars().next().is_some_and(char::is_uppercase);
    (capital && !pronoun_i && !starts_sentence(scene, i))
        || (opts.frequency_pack && w.rank.is_none())
}

/// The sentence without leading/trailing quotes, music signs and brackets.
fn core_text(s: &str) -> &str {
    let strip = |c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '“' | '”' | '\'' | '‘' | '’' | '♪' | '(' | ')' | '[' | ']' | '<' | '>' | '*'
            )
    };
    s.trim_matches(strip)
}

fn two_speakers(text: &str) -> bool {
    let t = text.trim_start();
    if !(t.starts_with('-') || t.starts_with('–') || t.starts_with('—')) {
        return false;
    }
    let rest: Vec<char> = t.chars().skip(1).collect();
    rest.windows(3).any(|w| {
        matches!(w[0], '.' | '!' | '?' | '…')
            && w[1].is_whitespace()
            && matches!(w[2], '-' | '–' | '—')
    }) || t.contains("\n-")
}

fn median(mut v: Vec<u32>) -> Option<u32> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[v.len() / 2])
}

/// The score of one scene (duplicates are filled in by [`rank_scenes`]).
pub fn score_scene(scene: &Scene, opts: &ScoreOptions) -> SceneScore {
    use points as p;
    use SceneReasonCode as R;
    let mut reasons = Vec::new();
    let (found, covered) = find_target(scene, opts);
    reasons.push(if found {
        reason(R::TargetPresent, p::TARGET_PRESENT)
    } else {
        reason(R::TargetMissing, p::TARGET_MISSING)
    });
    let (mut unknown, mut learning, mut names, mut ranks) = (vec![], vec![], vec![], vec![]);
    let mut word_count = 0usize;
    for (i, w) in scene.words.iter().enumerate() {
        if !w.text.chars().any(char::is_alphabetic) {
            continue;
        }
        word_count += 1;
        if covered.contains(&i) || w.text.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }
        let apostrophe = w.text.contains(['\'', '’']);
        if apostrophe && w.rank.is_none() && w.status == WordStatus::New {
            continue; // a contraction the pack does not list (`I'll`, `didn't`): basic
        }
        if is_name(scene, i, opts) {
            names.push(w.text.clone());
            continue;
        }
        if let Some(r) = w.rank {
            ranks.push(r);
        }
        let lower = w.lemma.to_lowercase();
        match w.status {
            WordStatus::Known | WordStatus::Ignored => {}
            WordStatus::Learning => learning.push(w.text.clone()),
            WordStatus::New => {
                let basic = is_function_word(&opts.language, &lower)
                    || is_function_word(&opts.language, &w.text.to_lowercase())
                    || w.rank.is_some_and(|r| r <= opts.basic_rank);
                if !basic && !unknown.contains(&w.text) {
                    unknown.push(w.text.clone());
                }
            }
        }
    }
    if unknown.is_empty() {
        reasons.push(reason(R::NoOtherUnknown, p::NO_OTHER_UNKNOWN));
    } else {
        let n = i32::try_from(unknown.len()).unwrap_or(i32::MAX / 100);
        reasons.push(SceneReason {
            count: count_u32(unknown.len()),
            words: unknown.clone(),
            ..reason(R::OtherUnknown, p::PER_UNKNOWN * n)
        });
    }
    if !learning.is_empty() {
        let n = i32::try_from(learning.len()).unwrap_or(i32::MAX / 100);
        reasons.push(SceneReason {
            count: count_u32(learning.len()),
            words: learning,
            ..reason(R::LearningWords, p::PER_LEARNING * n)
        });
    }
    if !names.is_empty() {
        reasons.push(SceneReason {
            count: count_u32(names.len()),
            words: names,
            ..reason(R::NamesIgnored, 0)
        });
    }
    // Completeness.
    let text = core_text(&scene.sentence);
    let body = text.trim_start_matches(['-', '–', '—', ' ']);
    if body.starts_with("...")
        || body.starts_with('…')
        || body.chars().next().is_some_and(char::is_lowercase)
    {
        reasons.push(reason(R::CutStart, p::CUT_START));
    }
    let last = text.chars().last();
    if text.ends_with("--") || matches!(last, Some('-' | '–' | '—')) {
        reasons.push(reason(R::DanglingDash, p::DANGLING_DASH));
    } else if text.ends_with("...") || matches!(last, Some('…' | ',' | ';' | ':')) {
        reasons.push(reason(R::CutEnd, p::CUT_END));
    } else if last.is_some_and(char::is_alphanumeric) {
        reasons.push(reason(R::NoEndPunctuation, p::NO_END_PUNCTUATION));
    }
    if two_speakers(text) {
        reasons.push(reason(R::TwoSpeakers, p::TWO_SPEAKERS));
    }
    let length = |code, pts| SceneReason {
        count: count_u32(word_count),
        ..reason(code, pts)
    };
    if word_count < MIN_WORDS {
        reasons.push(length(R::TooShort, p::TOO_SHORT));
    } else if word_count > MAX_WORDS {
        reasons.push(length(R::TooLong, p::TOO_LONG));
    } else if GOOD_LENGTH.contains(&word_count) {
        reasons.push(length(R::GoodLength, p::GOOD_LENGTH));
    }
    if scene.has_audio {
        reasons.push(reason(R::HasAudio, p::HAS_AUDIO));
    }
    if scene.has_image {
        reasons.push(reason(R::HasImage, p::HAS_IMAGE));
    }
    if let Some(m) = median(ranks) {
        if m <= COMMON_RANK {
            reasons.push(SceneReason {
                count: Some(m),
                ..reason(R::CommonWords, p::COMMON_WORDS)
            });
        } else if m > RARE_RANK {
            reasons.push(SceneReason {
                count: Some(m),
                ..reason(R::RareWords, p::RARE_WORDS)
            });
        }
    }
    SceneScore {
        id: scene.id.clone(),
        score: reasons.iter().map(|r| r.points).sum(),
        reasons,
        target_found: found,
        unknown,
        duplicates: Vec::new(),
    }
}

// ------------------------------------------------------------------ duplicates

/// Lowercase words of a sentence (letters, digits and inner apostrophes), punctuation gone.
pub fn normalized_words(sentence: &str) -> Vec<String> {
    sentence
        .to_lowercase()
        .replace(['’', '‘'], "'")
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .map(|w| w.trim_matches('\''))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether two sentences are near-duplicates: the same words (case, punctuation and spacing
/// aside), one inside the other (overlapping subtitle lines, at least 3 words), or nearly the
/// same set of words (Jaccard ≥ 0.8, at least 3 words each).
pub fn near_duplicate(a: &str, b: &str) -> bool {
    let (wa, wb) = (normalized_words(a), normalized_words(b));
    if wa.is_empty() || wb.is_empty() {
        return false;
    }
    if wa == wb {
        return true;
    }
    let (short, long) = if wa.len() <= wb.len() {
        (&wa, &wb)
    } else {
        (&wb, &wa)
    };
    if short.len() >= 3 && long.windows(short.len()).any(|w| w == short.as_slice()) {
        return true;
    }
    if short.len() < 3 {
        return false;
    }
    let sa: std::collections::BTreeSet<&String> = wa.iter().collect();
    let sb: std::collections::BTreeSet<&String> = wb.iter().collect();
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    // inter / union >= 0.8, in integers.
    inter * 5 >= union * 4
}

/// Cluster number of each scene (near-duplicates share one, transitively).
fn duplicate_groups(scenes: &[Scene]) -> Vec<usize> {
    let mut group: Vec<usize> = (0..scenes.len()).collect();
    fn root(g: &mut [usize], mut i: usize) -> usize {
        while g[i] != i {
            g[i] = g[g[i]];
            i = g[i];
        }
        i
    }
    for i in 0..scenes.len() {
        for j in i + 1..scenes.len() {
            if near_duplicate(&scenes[i].sentence, &scenes[j].sentence) {
                let (a, b) = (root(&mut group, i), root(&mut group, j));
                group[b] = a;
            }
        }
    }
    (0..scenes.len()).map(|i| root(&mut group, i)).collect()
}

#[cfg(test)]
mod tests;
