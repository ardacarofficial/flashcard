//! Built-in [`Lemmatizer`]s (reading mode, v0.10; docs/02 §24): a word tokenizer shared by all
//! of them and three small, **rule-based** lemmatizers:
//!
//! * [`SimpleEnglish`] (`builtin.en`): irregular forms table (`ran` → `run`, `children` →
//!   `child`, `better` → `good`…) + suffix rules for `-s/-es/-ies`, `-ed/-ied`, `-ing` with
//!   consonant undoubling (`running` → `run`) and a few `e`-restoration patterns (`making` →
//!   `make`, `dancing` → `dance`). Possessive `'s` is dropped.
//! * [`SimpleTurkish`] (`builtin.tr`): Turkish-aware lowercasing (`I` → `ı`, `İ` → `i`),
//!   proper-noun apostrophes (`Ankara'da` → `ankara`) and iterative stripping of common
//!   inflectional suffixes (plural, case, possessive, tense, person) while a root of at least
//!   two letters with a vowel remains (`evlerimizden` → `ev`, `kitapları` → `kitap`).
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

/// Byte spans of the words of `text`: runs of letters (and combining marks), an inner
/// apostrophe (`'` or `’`) followed by a letter continues the word (`don't`, `Ankara'da`).
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
            let apostrophe = c == '\'' || c == '\u{2019}';
            if apostrophe && chars.peek().is_some_and(|&(_, n)| n.is_alphabetic()) {
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
    ("bit", "bite"),
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
    ("ground", "grind"),
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
    ("wound", "wind"),
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
    ("me", "i"),
    ("us", "we"),
    ("him", "he"),
    ("them", "they"),
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

/// Lemma of one English word (any case).
pub fn english_lemma(word: &str) -> String {
    let lower = word.to_lowercase().replace('\u{2019}', "'");
    let w = lower
        .strip_suffix("'s")
        .or_else(|| lower.strip_suffix('\''))
        .unwrap_or(&lower);
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

/// Turkish inflectional suffixes, longest first within each group; stripped repeatedly from
/// the end (first match in this order) while the remainder is a plausible root.
const TR_SUFFIXES: &[&str] = &[
    // person / copula
    "sınız", "siniz", "sunuz", "sünüz", "dır", "dir", "dur", "dür", "tır", "tir", "tur", "tür",
    "sın", "sin", "sun", "sün", "ız", "iz", "uz", "üz", "ım", "im", "um", "üm",
    // tense / mood
    "acak", "ecek", "yacak", "yecek", "ıyor", "iyor", "uyor", "üyor", "yor", "mış", "miş", "muş",
    "müş", "dı", "di", "du", "dü", "tı", "ti", "tu", "tü", "mak", "mek", // case
    "ndan", "nden", "ndaki", "ndeki", "daki", "deki", "taki", "teki", "dan", "den", "tan", "ten",
    "nda", "nde", "da", "de", "ta", "te", "nın", "nin", "nun", "nün", "yla", "yle", "la", "le",
    "ya", "ye", "yı", "yi", "yu", "yü", "na", "ne", // possessive
    "ımız", "imiz", "umuz", "ümüz", "ınız", "iniz", "unuz", "ünüz", "ları", "leri", "sı", "si",
    "su", "sü", // plural
    "lar", "ler",
];

const TR_VOWELS: &[char] = &['a', 'e', 'ı', 'i', 'o', 'ö', 'u', 'ü'];

/// Lemma (approximate root) of one Turkish word (any case).
pub fn turkish_lemma(word: &str) -> String {
    let lower = lowercase(word, "tr");
    let root = strip_apostrophe_tail(&lower);
    if root.len() != lower.len() {
        // Proper noun + suffixes ("Ankara'da"): the part before the apostrophe is the root.
        return root.to_owned();
    }
    let mut w: &str = &lower;
    while let Some(next) = TR_SUFFIXES
        .iter()
        .filter_map(|s| w.strip_suffix(s))
        .find(|rest| plausible_root(rest))
    {
        w = next;
    }
    w.to_owned()
}

fn plausible_root(rest: &str) -> bool {
    rest.chars().count() >= 2 && rest.chars().any(|c| TR_VOWELS.contains(&c))
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
        let text = "Don't stop—Ankara'da 3 kedi; ‘quoted’ rock'n'roll.";
        let words: Vec<&str> = word_spans(text).iter().map(|&(s, e)| &text[s..e]).collect();
        assert_eq!(
            words,
            [
                "Don't",
                "stop",
                "Ankara'da",
                "kedi",
                "quoted",
                "rock'n'roll"
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
            ("evlerimizden", "ev"),
            ("evde", "ev"),
            ("geliyorum", "gel"),
            ("okulda", "okul"),
            ("Ankara'da", "ankara"),
            ("İstanbul'u", "istanbul"),
            ("IŞIK", "ışık"),
            ("ev", "ev"),
            ("masaya", "masa"),
            ("arkadaşlarımızla", "arkadaş"),
        ];
        for (word, lemma) in cases {
            assert_eq!(turkish_lemma(word), lemma, "{word}");
        }
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
