//! Built-in [`Lemmatizer`]s (reading mode, v0.10; docs/02 §24): a word tokenizer shared by all
//! of them and three small, **rule-based** lemmatizers:
//!
//! * [`SimpleEnglish`] (`builtin.en`): irregular forms table (`ran` → `run`, `children` →
//!   `child`, `better` → `good`…) + suffix rules for `-s/-es/-ies`, `-ed/-ied`, `-ing` with
//!   consonant undoubling (`running` → `run`) and a few `e`-restoration patterns (`making` →
//!   `make`, `dancing` → `dance`). Possessive `'s` is dropped.
//! * [`SimpleTurkish`] (`builtin.tr`): Turkish-aware lowercasing (`I` → `ı`, `İ` → `i`),
//!   proper-noun apostrophes (`Ankara'da` → `ankara`) and iterative stripping of common
//!   inflectional suffixes (plural, case, possessive, tense, person) written as
//!   vowel-harmony templates, with safeguards: a stem keeps at least [`TR_MIN_STEM`] letters
//!   ([`TR_MIN_STEM_SHORT`] for two-letter suffixes) and a vowel, buffer consonants only
//!   after vowels (`kalemlerimizden` → `kalem`, `kitapları` → `kitap`, but `ayna`, `hasta`,
//!   `anne`, `kedi` stay whole). Two-letter roots therefore keep their suffixes (`evde`).
//! * [`Plain`] (`builtin.plain`): any language, lowercase only (Turkic-aware).
//!
//! They are deliberately simple (no lexicon): what matters for reading mode is that a text
//! word and a card word map to the **same key**, which holds because both go through the same
//! lemmatizer. Better analyzers (Hunspell, Zemberek, spaCy…) plug in through the same trait.
//! Pure: no I/O, no global state.

use crate::language::{Lemmatizer, Token};
use crate::provider::{Capability, ProviderInfo, ProviderResult};
use std::sync::Arc;

/// Language capability every language-agnostic lemmatizer declares.
pub const ANY_LANGUAGE: &str = "*";

/// Byte spans of the words of `text`: runs of letters (and combining marks); an inner
/// apostrophe (`'` or `’`) or hyphen (`-`, `‐`) followed by a letter continues the word
/// (`don't`, `Ankara'da`, `well-known`, `e-mail`).
/// Digits, punctuation and symbols are not words.
pub fn word_spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if is_word_char(c) {
            if start.is_none() {
                start = Some(i);
            }
            continue;
        }
        if let Some(s) = start {
            let joiner = matches!(c, '\'' | '\u{2019}' | '-' | '\u{2010}');
            if joiner && chars.peek().is_some_and(|&(_, n)| n.is_alphabetic()) {
                continue;
            }
            out.push((s, i));
            start = None;
        }
    }
    if let Some(s) = start {
        out.push((s, text.len()));
    }
    out
}

fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || is_combining_mark(c)
}

fn is_combining_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF)
}

/// Lowercases `word` for `language`: Turkish and Azerbaijani map `I` → `ı` and `İ` → `i`
/// (dotted/dotless pairs), every other language uses Unicode lowercase.
pub fn lowercase(word: &str, language: &str) -> String {
    if is_turkic(language) {
        let mut out = String::with_capacity(word.len());
        for c in word.chars() {
            match c {
                'I' => out.push('ı'),
                'İ' => out.push('i'),
                c => out.extend(c.to_lowercase()),
            }
        }
        out
    } else {
        word.to_lowercase()
    }
}

fn is_turkic(language: &str) -> bool {
    matches!(primary_language(language).as_str(), "tr" | "az")
}

/// Primary subtag of a BCP-47 tag, lowercase (`en-GB` → `en`).
pub fn primary_language(language: &str) -> String {
    language
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

fn analyze_with(text: &str, lemma: impl Fn(&str) -> String) -> Vec<Token> {
    word_spans(text)
        .into_iter()
        .map(|(start, end)| {
            let surface = &text[start..end];
            Token {
                start,
                end,
                text: surface.to_owned(),
                lemma: lemma(surface),
            }
        })
        .collect()
}

fn info(id: &str, languages: &[&str]) -> ProviderInfo {
    ProviderInfo {
        id: id.to_owned(),
        name_key: format!("providers.lemmatizer.{id}"),
        capabilities: languages
            .iter()
            .map(|l| Capability::Language((*l).to_owned()))
            .collect(),
        settings: vec![],
        secret_keys: vec![],
        local: true,
    }
}

/// Every built-in lemmatizer, most specific first.
pub fn builtin_lemmatizers() -> Vec<Arc<dyn Lemmatizer>> {
    vec![
        Arc::new(SimpleEnglish),
        Arc::new(SimpleTurkish),
        Arc::new(Plain),
    ]
}

// ------------------------------------------------------------------ plain

/// Any language: lowercase only (`builtin.plain`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Plain;

impl Lemmatizer for Plain {
    fn info(&self) -> ProviderInfo {
        info("builtin.plain", &[ANY_LANGUAGE])
    }

    fn analyze(&self, text: &str, language: &str) -> ProviderResult<Vec<Token>> {
        Ok(analyze_with(text, |w| lowercase(w, language)))
    }
}

fn strip_apostrophe_tail(w: &str) -> &str {
    match w.find(['\'', '\u{2019}']) {
        Some(i) if i > 0 => &w[..i],
        _ => w,
    }
}

// ------------------------------------------------------------------ English

/// Rule-based English lemmatizer (`builtin.en`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SimpleEnglish;

impl Lemmatizer for SimpleEnglish {
    fn info(&self) -> ProviderInfo {
        info("builtin.en", &["en"])
    }

    fn analyze(&self, text: &str, _language: &str) -> ProviderResult<Vec<Token>> {
        Ok(analyze_with(text, english_lemma))
    }
}

/// Irregular English forms → lemma.
const EN_IRREGULAR: &[(&str, &str)] = &[
    ("am", "be"),
    ("is", "be"),
    ("are", "be"),
    ("was", "be"),
    ("were", "be"),
    ("been", "be"),
    ("being", "be"),
    ("has", "have"),
    ("had", "have"),
    ("having", "have"),
    ("does", "do"),
    ("did", "do"),
    ("done", "do"),
    ("doing", "do"),
    ("goes", "go"),
    ("went", "go"),
    ("gone", "go"),
    ("going", "go"),
    ("said", "say"),
    ("says", "say"),
    ("got", "get"),
    ("gotten", "get"),
    ("made", "make"),
    ("knew", "know"),
    ("known", "know"),
    ("thought", "think"),
    ("took", "take"),
    ("taken", "take"),
    ("saw", "see"),
    ("seen", "see"),
    ("came", "come"),
    ("gave", "give"),
    ("given", "give"),
    ("found", "find"),
    ("told", "tell"),
    ("became", "become"),
    ("left", "leave"),
    ("felt", "feel"),
    ("brought", "bring"),
    ("began", "begin"),
    ("begun", "begin"),
    ("kept", "keep"),
    ("held", "hold"),
    ("wrote", "write"),
    ("written", "write"),
    ("stood", "stand"),
    ("heard", "hear"),
    ("meant", "mean"),
    ("met", "meet"),
    ("ran", "run"),
    ("paid", "pay"),
    ("sat", "sit"),
    ("spoke", "speak"),
    ("spoken", "speak"),
    ("led", "lead"),
    ("grew", "grow"),
    ("grown", "grow"),
    ("lost", "lose"),
    ("fell", "fall"),
    ("fallen", "fall"),
    ("sent", "send"),
    ("built", "build"),
    ("understood", "understand"),
    ("drew", "draw"),
    ("drawn", "draw"),
    ("broke", "break"),
    ("broken", "break"),
    ("spent", "spend"),
    ("rose", "rise"),
    ("risen", "rise"),
    ("drove", "drive"),
    ("driven", "drive"),
    ("bought", "buy"),
    ("wore", "wear"),
    ("worn", "wear"),
    ("chose", "choose"),
    ("chosen", "choose"),
    ("ate", "eat"),
    ("eaten", "eat"),
    ("drank", "drink"),
    ("drunk", "drink"),
    ("sang", "sing"),
    ("sung", "sing"),
    ("swam", "swim"),
    ("swum", "swim"),
    ("flew", "fly"),
    ("flown", "fly"),
    ("forgot", "forget"),
    ("forgotten", "forget"),
    ("slept", "sleep"),
    ("taught", "teach"),
    ("caught", "catch"),
    ("fought", "fight"),
    ("sought", "seek"),
    ("sold", "sell"),
    ("won", "win"),
    ("threw", "throw"),
    ("thrown", "throw"),
    ("shook", "shake"),
    ("shaken", "shake"),
    ("hid", "hide"),
    ("hidden", "hide"),
    ("bitten", "bite"),
    ("rode", "ride"),
    ("ridden", "ride"),
    ("stole", "steal"),
    ("stolen", "steal"),
    ("woke", "wake"),
    ("woken", "wake"),
    ("fed", "feed"),
    ("hung", "hang"),
    ("shot", "shoot"),
    ("dug", "dig"),
    ("stuck", "stick"),
    ("struck", "strike"),
    ("lent", "lend"),
    ("bent", "bend"),
    ("lit", "light"),
    ("slid", "slide"),
    ("spun", "spin"),
    ("froze", "freeze"),
    ("frozen", "freeze"),
    ("dealt", "deal"),
    ("dreamt", "dream"),
    ("fled", "flee"),
    ("forgave", "forgive"),
    ("forgiven", "forgive"),
    ("laid", "lay"),
    ("lain", "lie"),
    ("lying", "lie"),
    ("dying", "die"),
    ("tying", "tie"),
    ("lied", "lie"),
    ("died", "die"),
    ("tied", "tie"),
    ("overcame", "overcome"),
    ("sank", "sink"),
    ("sunk", "sink"),
    ("shone", "shine"),
    ("shrank", "shrink"),
    ("sprang", "spring"),
    ("swore", "swear"),
    ("sworn", "swear"),
    ("tore", "tear"),
    ("torn", "tear"),
    ("wept", "weep"),
    ("withdrew", "withdraw"),
    ("children", "child"),
    ("men", "man"),
    ("women", "woman"),
    ("people", "person"),
    ("mice", "mouse"),
    ("feet", "foot"),
    ("teeth", "tooth"),
    ("geese", "goose"),
    ("oxen", "ox"),
    ("lives", "life"),
    ("wives", "wife"),
    ("knives", "knife"),
    ("leaves", "leaf"),
    ("wolves", "wolf"),
    ("halves", "half"),
    ("shelves", "shelf"),
    ("thieves", "thief"),
    ("loaves", "loaf"),
    ("calves", "calf"),
    ("selves", "self"),
    ("better", "good"),
    ("best", "good"),
    ("worse", "bad"),
    ("worst", "bad"),
    ("further", "far"),
    ("farther", "far"),
    ("less", "little"),
    ("least", "little"),
    ("an", "a"),
];

/// Words ending in `s` that are not plurals / third persons.
const EN_S_WORDS: &[&str] = &[
    "this",
    "his",
    "hers",
    "its",
    "ours",
    "yours",
    "theirs",
    "yes",
    "news",
    "series",
    "species",
    "always",
    "perhaps",
    "various",
    "famous",
    "serious",
    "nervous",
    "previous",
    "obvious",
    "jealous",
    "dangerous",
    "bus",
    "gas",
    "plus",
    "thus",
    "lens",
    "chaos",
    "bonus",
    "status",
    "virus",
    "campus",
    "census",
    "focus",
    "minus",
    "christmas",
    "physics",
    "maths",
    "mathematics",
    "politics",
    "economics",
    "athletics",
    "ethics",
    "whereas",
    "unless",
    "towards",
    "afterwards",
    "besides",
    "sometimes",
    "nevertheless",
    "pants",
    "glasses",
    "scissors",
    "clothes",
    "thanks",
];

/// Words ending in `ing`/`ed` that are not inflections.
const EN_KEEP: &[&str] = &[
    "thing",
    "nothing",
    "something",
    "anything",
    "everything",
    "king",
    "ring",
    "sing",
    "bring",
    "spring",
    "string",
    "wing",
    "swing",
    "sting",
    "during",
    "morning",
    "evening",
    "ceiling",
    "building",
    "wedding",
    "pudding",
    "feeling",
    "meeting",
    "painting",
    "red",
    "bed",
    "need",
    "seed",
    "speed",
    "feed",
    "bleed",
    "breed",
    "shed",
    "wed",
    "hundred",
    "naked",
    "wicked",
    "sacred",
    "kindred",
    "bread",
    "head",
    "dead",
    "lead",
    "read",
    "spread",
    "thread",
    "ahead",
    "instead",
    "bored",
    "tired",
    "interested",
];

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

fn has_vowel(s: &str) -> bool {
    s.chars().any(|c| is_vowel(c) || c == 'y')
}

/// Contracted negations whose base is not the part before `n't`.
const EN_NEGATIONS: &[(&str, &str)] = &[
    ("can't", "can"),
    ("won't", "will"),
    ("shan't", "shall"),
    ("ain't", "be"),
];

/// Clitics dropped from the end of a word (`don't` → `do`, `I'm` → `i`, `they've` → `they`,
/// `John's` → `john`).
const EN_CLITICS: &[&str] = &["n't", "'s", "'m", "'re", "'ve", "'ll", "'d", "'"];

/// Lemma of one English word (any case). Contractions lose their clitic (`don't` → `do`,
/// `isn't` → `be`, `can't` → `can`, `won't` → `will`); a hyphenated word keeps its first
/// parts and lemmatizes its last (`e-mails` → `e-mail`).
pub fn english_lemma(word: &str) -> String {
    let lower = word.to_lowercase().replace('\u{2019}', "'");
    if let Some((_, base)) = EN_NEGATIONS.iter().find(|(f, _)| *f == lower) {
        return (*base).to_owned();
    }
    if let Some(i) = lower.rfind('-') {
        let (head, last) = lower.split_at(i + 1);
        return format!("{head}{}", english_lemma(last));
    }
    if let Some(base) = EN_CLITICS
        .iter()
        .find_map(|c| lower.strip_suffix(c))
        .filter(|b| !b.is_empty() && !b.contains('\''))
    {
        return english_lemma(base);
    }
    let w = lower.as_str();
    if let Some((_, lemma)) = EN_IRREGULAR.iter().find(|(f, _)| *f == w) {
        return (*lemma).to_owned();
    }
    if w.contains('\'') || !w.is_ascii() || w.len() <= 3 || EN_KEEP.contains(&w) {
        return w.to_owned();
    }
    if let Some(stem) = w.strip_suffix("ing") {
        if stem.len() >= 2 && has_vowel(stem) {
            return restore_stem(stem);
        }
    }
    if let Some(stem) = w.strip_suffix("ied") {
        if stem.len() >= 2 {
            return format!("{stem}y");
        }
    }
    if let Some(stem) = w.strip_suffix("ed") {
        if stem.len() >= 2 && has_vowel(stem) {
            if stem.ends_with('e') {
                // agreed, freed
                return format!("{stem}e");
            }
            return restore_stem(stem);
        }
    }
    if EN_S_WORDS.contains(&w) {
        return w.to_owned();
    }
    if let Some(stem) = w.strip_suffix("ies") {
        if stem.len() >= 2 {
            return format!("{stem}y");
        }
    }
    if let Some(stem) = w.strip_suffix("es") {
        if ["ss", "sh", "ch", "x", "z", "o"]
            .iter()
            .any(|e| stem.ends_with(e))
        {
            return stem.to_owned();
        }
    }
    if let Some(stem) = w.strip_suffix('s') {
        if !(stem.ends_with('s') || stem.ends_with('u') || stem.ends_with('i')) {
            return stem.to_owned();
        }
    }
    w.to_owned()
}

/// `-ing`/`-ed` stem → lemma: undouble a doubled final consonant (`runn` → `run`, but `fall`,
/// `miss`, `buzz` keep theirs) or restore a silent `e` where the stem cannot end a word
/// (`mak` → `make`, `danc` → `dance`, `continu` → `continue`, `troubl` → `trouble`).
fn restore_stem(stem: &str) -> String {
    let chars: Vec<char> = stem.chars().collect();
    let n = chars.len();
    let (Some(&last), Some(&before)) = (chars.last(), chars.get(n.wrapping_sub(2))) else {
        return stem.to_owned();
    };
    if last == before && !is_vowel(last) && !matches!(last, 'l' | 's' | 'z') {
        return chars[..n - 1].iter().collect();
    }
    let needs_e = matches!(last, 'v' | 'c' | 'u')
        || stem.ends_with("dg")
        || stem.ends_with("iz")
        || (last == 'l' && matches!(before, 'b' | 'p' | 't' | 'g' | 'd' | 'k'))
        || (n == 2 && is_vowel(before) && !is_vowel(last))
        || (n == 3
            && !is_vowel(chars[0])
            && is_vowel(before)
            && !is_vowel(last)
            && !matches!(last, 'w' | 'x' | 'y'));
    if needs_e {
        format!("{stem}e")
    } else {
        stem.to_owned()
    }
}

// ------------------------------------------------------------------ Turkish

/// Rule-based Turkish lemmatizer (`builtin.tr`): a light suffix stripper.
#[derive(Debug, Clone, Copy, Default)]
pub struct SimpleTurkish;

impl Lemmatizer for SimpleTurkish {
    fn info(&self) -> ProviderInfo {
        info("builtin.tr", &["tr"])
    }

    fn analyze(&self, text: &str, _language: &str) -> ProviderResult<Vec<Token>> {
        Ok(analyze_with(text, turkish_lemma))
    }
}

/// What a suffix may follow: its buffer consonant (`y`, `n`, `s`) only comes after a vowel,
/// a vowel-initial suffix only after a consonant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum After {
    Vowel,
    Consonant,
    Any,
}

/// Turkish inflectional suffixes as **harmony templates**, longest first: `A` = `a/e`
/// (two-way vowel harmony), `I` = `ı/i/u/ü` (four-way), `D` = `d/t` (`t` after a voiceless
/// consonant); other letters are literal (`yor`, `ki`). A template matches only in the form
/// harmony gives it after the stem (`evler` yes, `evlar` no; `kitapta` yes, `kitapda` no).
/// Deliberately conservative — no single-letter suffixes (accusative/dative `ı/a`,
/// possessive `m`), no copula `-Iz` (`temiz`, `deniz`), no negative `-mA` — because without a
/// lexicon they mostly cut real roots.
const TR_SUFFIXES: &[(&str, After)] = &[
    // 5
    ("nDAki", After::Vowel),
    ("sInIz", After::Any),
    // 4
    ("yAcAk", After::Vowel),
    ("nDAn", After::Vowel),
    ("DAki", After::Any),
    ("AcAk", After::Consonant),
    ("Iyor", After::Consonant),
    ("ImIz", After::Consonant),
    ("InIz", After::Consonant),
    ("lArI", After::Any),
    // 3
    ("DAn", After::Any),
    ("nDA", After::Vowel),
    ("nIn", After::Vowel),
    ("ylA", After::Vowel),
    ("yIm", After::Vowel),
    ("yIz", After::Vowel),
    ("DIr", After::Any),
    ("sIn", After::Any),
    ("mIş", After::Any),
    ("mAk", After::Any),
    ("yor", After::Vowel),
    ("lAr", After::Any),
    // 2 (the stem must keep at least TR_MIN_STEM_SHORT letters)
    ("DA", After::Any),
    ("DI", After::Any),
    ("lA", After::Any),
    ("yA", After::Vowel),
    ("yI", After::Vowel),
    ("nA", After::Vowel),
    ("nI", After::Vowel),
    ("sI", After::Vowel),
    ("Im", After::Consonant),
    ("In", After::Consonant),
];

/// A suffix is never stripped when fewer letters than this would remain (`ayna` ↛ `ay`,
/// `anne` ↛ `an`, `kedi` ↛ `ke`).
pub const TR_MIN_STEM: usize = 3;
/// …nor, for a two-letter suffix, fewer than this (`hasta` ↛ `has`, `şimdi` ↛ `şim`,
/// `resim` ↛ `res`, `altın` ↛ `alt`).
pub const TR_MIN_STEM_SHORT: usize = 4;

fn tr_vowel(c: char) -> bool {
    matches!(
        c,
        'a' | 'e' | 'ı' | 'i' | 'o' | 'ö' | 'u' | 'ü' | 'â' | 'î' | 'û'
    )
}

fn tr_back(v: char) -> bool {
    matches!(v, 'a' | 'ı' | 'o' | 'u' | 'â' | 'û')
}

fn tr_voiceless(c: char) -> bool {
    matches!(c, 'ç' | 'f' | 'h' | 'k' | 'p' | 's' | 'ş' | 't')
}

/// The form `template` takes after `stem` (vowel and consonant harmony), `None` when the
/// stem has no vowel to harmonize with.
fn tr_surface(stem: &[char], template: &str) -> Option<Vec<char>> {
    let mut vowel = stem.iter().rev().copied().find(|&c| tr_vowel(c))?;
    let mut prev = *stem.last()?;
    let mut out = Vec::with_capacity(template.len());
    for t in template.chars() {
        let c = match t {
            'A' if tr_back(vowel) => 'a',
            'A' => 'e',
            'I' => match vowel {
                'a' | 'ı' | 'â' => 'ı',
                'e' | 'i' | 'î' => 'i',
                'o' | 'u' | 'û' => 'u',
                _ => 'ü',
            },
            'D' if tr_voiceless(prev) => 't',
            'D' => 'd',
            c => c,
        };
        if tr_vowel(c) {
            vowel = c;
        }
        prev = c;
        out.push(c);
    }
    Some(out)
}

/// Strips [`TR_SUFFIXES`] from the end of a lowercase word, repeatedly, while the
/// safeguards hold; a stem softened before a vowel gets its consonant back
/// (`çocuğ-um` → `çocuk`, `kitab-ın` → `kitap`, `ağac-ın` → `ağaç`).
fn tr_strip(word: &str) -> String {
    let mut w: Vec<char> = word.chars().collect();
    'strip: loop {
        for &(template, after) in TR_SUFFIXES {
            let n = template.chars().count();
            let min = if n <= 2 {
                TR_MIN_STEM_SHORT
            } else {
                TR_MIN_STEM
            };
            if w.len() < n + min {
                continue;
            }
            let (stem, tail) = w.split_at(w.len() - n);
            let ends_in_vowel = stem.last().is_some_and(|&c| tr_vowel(c));
            let context_ok = match after {
                After::Vowel => ends_in_vowel,
                After::Consonant => !ends_in_vowel,
                After::Any => true,
            };
            if !context_ok || tr_surface(stem, template).as_deref() != Some(tail) {
                continue;
            }
            let vowel_initial = tail.first().is_some_and(|&c| tr_vowel(c));
            let mut stem = stem.to_vec();
            if vowel_initial {
                if let Some(last) = stem.last_mut() {
                    *last = match *last {
                        'ğ' => 'k',
                        'b' => 'p',
                        'c' => 'ç',
                        c => c,
                    };
                }
            }
            w = stem;
            continue 'strip;
        }
        break;
    }
    w.into_iter().collect()
}

/// Lemma (approximate root) of one Turkish word (any case). A proper noun's suffixes come
/// after an apostrophe (`Ankara'da`); the part before it goes through the same stripping as
/// any other word, so `Türkiye'de` and `Türkiye` always share a key.
pub fn turkish_lemma(word: &str) -> String {
    let lower = lowercase(word, "tr");
    tr_strip(strip_apostrophe_tail(&lower))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lemmas(l: &dyn Lemmatizer, text: &str, lang: &str) -> Vec<String> {
        l.analyze(text, lang)
            .unwrap()
            .into_iter()
            .map(|t| t.lemma)
            .collect()
    }

    #[test]
    fn spans_cover_words_with_inner_apostrophes_only() {
        let text = "Don't stop—Ankara'da 3 kedi; ‘quoted’ rock'n'roll well-known -x y- e‐mail.";
        let words: Vec<&str> = word_spans(text).iter().map(|&(s, e)| &text[s..e]).collect();
        assert_eq!(
            words,
            [
                "Don't",
                "stop",
                "Ankara'da",
                "kedi",
                "quoted",
                "rock'n'roll",
                "well-known",
                "x",
                "y",
                "e‐mail"
            ]
        );
    }

    #[test]
    fn english_inflections_share_a_lemma() {
        let cases = [
            ("running", "run"),
            ("ran", "run"),
            ("runs", "run"),
            ("run", "run"),
            ("making", "make"),
            ("made", "make"),
            ("cities", "city"),
            ("tried", "try"),
            ("stopped", "stop"),
            ("played", "play"),
            ("loved", "love"),
            ("dancing", "dance"),
            ("boxes", "box"),
            ("watches", "watch"),
            ("children", "child"),
            ("better", "good"),
            ("falling", "fall"),
            ("missing", "miss"),
            ("thing", "thing"),
            ("this", "this"),
            ("John's", "john"),
            ("Was", "be"),
            ("using", "use"),
            ("used", "use"),
            ("continuing", "continue"),
            ("troubled", "trouble"),
            ("hoping", "hope"),
            ("hopping", "hop"),
            ("reading", "read"),
            ("wanted", "want"),
            ("agreed", "agree"),
            ("died", "die"),
            // contractions
            ("don't", "do"),
            ("Don’t", "do"),
            ("doesn't", "do"),
            ("didn't", "do"),
            ("can't", "can"),
            ("won't", "will"),
            ("isn't", "be"),
            ("wasn't", "be"),
            ("haven't", "have"),
            ("couldn't", "could"),
            ("I'm", "i"),
            ("you're", "you"),
            ("they've", "they"),
            ("we'll", "we"),
            ("he'd", "he"),
            ("let's", "let"),
            ("o'clock", "o'clock"),
            // forms that are words of their own stay
            ("bit", "bit"),
            ("US", "us"),
            ("us", "us"),
            ("ground", "ground"),
            ("wound", "wound"),
            ("them", "them"),
            // hyphenated words: one word, last part lemmatized
            ("e-mails", "e-mail"),
            ("mothers-in-law", "mothers-in-law"),
            ("self-made", "self-make"),
        ];
        for (word, lemma) in cases {
            assert_eq!(english_lemma(word), lemma, "{word}");
        }
    }

    #[test]
    fn turkish_suffixes_are_stripped_to_the_root() {
        let cases = [
            ("kitaplar", "kitap"),
            ("kitapları", "kitap"),
            ("kalemlerimizden", "kalem"),
            ("geliyorum", "gel"),
            ("okulda", "okul"),
            ("Ankara'da", "ankara"),
            ("İstanbul'u", "istanbul"),
            ("Ali'ye", "ali"),
            ("Ayşe'nin", "ayşe"),
            ("IŞIK", "ışık"),
            ("masaya", "masa"),
            ("masalar", "masa"),
            ("arkadaşlarımızla", "arkadaş"),
            // softened stems get their consonant back
            ("çocuğum", "çocuk"),
            ("çocuklar", "çocuk"),
            ("kitabın", "kitap"),
            ("ağacın", "ağaç"),
            // tense, person, case
            ("gelmiş", "gel"),
            ("gelecek", "gel"),
            ("gelmek", "gel"),
            ("okuyacak", "oku"),
            ("okuyor", "oku"),
            ("hastayım", "hasta"),
            ("arabanın", "araba"),
            ("annenin", "anne"),
            ("arabasını", "araba"),
            ("arabasına", "araba"),
            ("kalemle", "kalem"),
            ("yollar", "yol"),
            ("yoldan", "yol"),
            ("kitapta", "kitap"),
            ("gözlükler", "gözlük"),
            ("öğrencilerin", "öğrenci"),
            ("üzümler", "üzüm"),
            ("güller", "gül"),
            ("sözlükte", "sözlük"),
        ];
        for (word, lemma) in cases {
            assert_eq!(turkish_lemma(word), lemma, "{word}");
        }
    }

    /// Lexicon-free safeguards: common words that merely *end* like a suffix stay whole.
    #[test]
    fn turkish_roots_are_not_overstripped() {
        let whole = "ayna hasta anne kedi araba masa kapı pencere kalem defter okul öğretmen \
            bilgisayar telefon doktor deniz temiz güzel büyük küçük sabah akşam gece insan \
            zaman kadın erkek çocuk baba kardeş arkadaş kitap köpek elma armut ekmek peynir su \
            çay kahve şeker dünya hava yağmur kar güneş altın resim durum sonra önce şimdi \
            bugün yarın dün mavi beyaz siyah yeşil kırmızı sarı meydan vatan beden maden dolar \
            orta bana sana yeni eski iyi kötü yemek çiçek bebek kulak toprak ırmak kendi hindi \
            nokta bile şelale sauna kuzen saat liman tavan çoban kaplan aslan bütün derin kumaş";
        let words: Vec<&str> = whole.split_whitespace().collect();
        assert!(words.len() >= 95, "{}", words.len());
        for w in words {
            assert_eq!(turkish_lemma(w), w, "{w}");
        }
        // Never a stem shorter than TR_MIN_STEM: two-letter roots keep their suffixes rather
        // than risk `ayna` → `ay`.
        assert_eq!(turkish_lemma("evde"), "evde");
        assert_eq!(turkish_lemma("evlerimizden"), "evler");
        // Harmony: a suffix only matches in the form the stem's last vowel gives it.
        assert_eq!(turkish_lemma("kitapler"), "kitapler");
        assert_eq!(turkish_lemma("kitapda"), "kitapda");
        // A buffer consonant only after a vowel.
        assert_eq!(turkish_lemma("kalemya"), "kalemya");
    }

    /// The part before an apostrophe is stripped like any word: one key per word, with or
    /// without the proper-noun apostrophe.
    #[test]
    fn turkish_apostrophe_forms_share_the_bare_words_key() {
        let key = turkish_lemma("Türkiye");
        for w in ["Türkiye'de", "Türkiye’nin", "Türkiye'ye", "TÜRKİYE'DEN"] {
            assert_eq!(turkish_lemma(w), key, "{w}");
        }
        for (bare, with) in [
            ("Ankara", "Ankara'ya"),
            ("Ayşe", "Ayşe'nin"),
            ("Kemal", "Kemal'in"),
        ] {
            assert_eq!(turkish_lemma(bare), turkish_lemma(with), "{with}");
        }
    }

    #[test]
    fn turkish_suffix_table_is_longest_first() {
        let lens: Vec<usize> = TR_SUFFIXES.iter().map(|(t, _)| t.chars().count()).collect();
        assert!(lens.windows(2).all(|w| w[0] >= w[1]), "{lens:?}");
        assert!(lens.iter().all(|&n| n >= 2), "no single-letter suffixes");
    }

    #[test]
    fn analyzers_report_byte_spans_and_languages() {
        let en = SimpleEnglish;
        let toks = en.analyze("The cats ran.", "en").unwrap();
        assert_eq!(toks[1].start, 4);
        assert_eq!(toks[1].end, 8);
        assert_eq!(toks[1].text, "cats");
        assert_eq!(lemmas(&en, "The cats ran.", "en"), ["the", "cat", "run"]);
        assert_eq!(
            lemmas(&Plain, "Größe GRÖSSE l'eau", "de"),
            ["größe", "grösse", "l'eau"]
        );
        assert_eq!(lemmas(&Plain, "IRMAK", "tr"), ["ırmak"]);
        assert!(en
            .info()
            .capabilities
            .contains(&Capability::Language("en".into())));
        assert_eq!(builtin_lemmatizers().len(), 3);
        assert_eq!(primary_language("en-GB"), "en");
    }
}
