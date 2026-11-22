//! Word data (v1.4, docs/03 §4.3): how often a word is heard and an estimated CEFR band, from
//! a **versioned, local word-data pack** per language. Pure: a pack arrives as bytes (the
//! app ships the English one compiled in, `fc_providers::word_data`, and
//! `words.data.import` reads others from a file); nothing here reads files or the network.
//!
//! * **Rank**: the place of a lemma in the pack's frequency list (1 = heard most). Inflected
//!   forms share their lemma's rank (`went` → `go`); a derived word keeps its own (`actor` is
//!   not `act`).
//! * **Band** ([`CefrBand`]) with its **origin** ([`BandOrigin`]): the lowest level a CEFR
//!   vocabulary profile teaches the word at (`profile`), else the band its rank falls in
//!   ([`band_of_rank`], `rank`, an estimate), else — for an expression — the documented rule of
//!   [`expression_band`] (`expression`, an estimate).
//! * **Family**: a derived lemma points to the head of its word family (`actor` → `act`). It is
//!   shown next to a word and **never** changes a status: knowing `act` does not make `actor`
//!   known.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// The pack format this build reads (`format` / `format_version` of a pack).
pub const PACK_FORMAT: &str = "fc-word-data";
/// Pack format version.
pub const PACK_FORMAT_VERSION: u32 = 1;
/// Most lemmas a pack may have.
pub const MAX_PACK_WORDS: usize = 200_000;
/// Most forms / profile headwords / family links a pack may have (each).
pub const MAX_PACK_MAP: usize = 400_000;
/// Longest word, form or headword (bytes).
pub const MAX_WORD_BYTES: usize = 100;

/// A CEFR level, as a band of vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum CefrBand {
    /// A1.
    A1,
    /// A2.
    A2,
    /// B1.
    B1,
    /// B2.
    B2,
    /// C1.
    C1,
    /// C2.
    C2,
}

impl CefrBand {
    /// Every band, easiest first.
    pub const ALL: [Self; 6] = [Self::A1, Self::A2, Self::B1, Self::B2, Self::C1, Self::C2];

    /// Storage / wire string (`A1`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::A1 => "A1",
            Self::A2 => "A2",
            Self::B1 => "B1",
            Self::B2 => "B2",
            Self::C1 => "C1",
            Self::C2 => "C2",
        }
    }

    /// Parses `A1`…`C2` (case-insensitive, trimmed).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        Self::ALL
            .into_iter()
            .find(|b| b.as_str().eq_ignore_ascii_case(s))
    }

    /// The highest rank of the band ([`RANK_BANDS`]).
    pub fn top_rank(self) -> u32 {
        RANK_BANDS
            .iter()
            .find(|(b, _)| *b == self)
            .map_or(u32::MAX, |(_, r)| *r)
    }
}

/// Where a band comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum BandOrigin {
    /// A CEFR vocabulary profile lists the word at this level.
    Profile,
    /// Estimated from the word's frequency rank ([`band_of_rank`]).
    Rank,
    /// Estimated for an expression from its words ([`expression_band`]).
    Expression,
}

impl BandOrigin {
    /// Storage / wire string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::Rank => "rank",
            Self::Expression => "expression",
        }
    }

    /// Parses a storage string.
    pub fn parse(s: &str) -> Option<Self> {
        [Self::Profile, Self::Rank, Self::Expression]
            .into_iter()
            .find(|o| o.as_str() == s)
    }

    /// The band is an estimate (not from a profile list).
    pub const fn estimated(self) -> bool {
        !matches!(self, Self::Profile)
    }
}

/// The CEFR bands by the rank of the words known at each: roughly the vocabulary of a level
/// counted in lemmas (the Subtitle extension's `CEFR_LEVELS`). A rank past the last band is
/// still C2 (the rare tail).
pub const RANK_BANDS: [(CefrBand, u32); 6] = [
    (CefrBand::A1, 1_000),
    (CefrBand::A2, 2_000),
    (CefrBand::B1, 3_500),
    (CefrBand::B2, 6_000),
    (CefrBand::C1, 10_000),
    (CefrBand::C2, 20_000),
];

/// The band a frequency rank falls in (rank 1 = heard most); C2 for the tail past 20 000.
pub fn band_of_rank(rank: u32) -> CefrBand {
    RANK_BANDS
        .iter()
        .find(|(_, top)| rank <= *top)
        .map_or(CefrBand::C2, |(b, _)| *b)
}

/// A word's band: the profile's level when a profile lists it, else the band of its rank.
pub fn word_band(rank: Option<u32>, profile: Option<CefrBand>) -> Option<(CefrBand, BandOrigin)> {
    match (profile, rank) {
        (Some(b), _) => Some((b, BandOrigin::Profile)),
        (None, Some(r)) => Some((band_of_rank(r), BandOrigin::Rank)),
        (None, None) => None,
    }
}

/// English words that do not carry an expression's level: articles, pronouns, auxiliaries,
/// conjunctions and the particles / prepositions of phrasal verbs (the Subtitle extension's
/// `ENGLISH_FUNCTION_WORDS`).
const ENGLISH_FUNCTION_WORDS: &str = "a an the and or but nor if as than so not no \
    i me my mine you your yours he him his she her hers it its we us our ours they them their theirs one oneself \
    myself yourself himself herself itself ourselves yourselves themselves this that these those there here \
    someone somebody something anyone anybody anything everyone everything what who whom whose which \
    be am is are was were been being have has had having do does did done will would shall should can could may \
    might must about above across after against ahead along among apart around aside at away back behind below \
    beneath beside between beyond by down for forth forward from in inside into near of off on onto out outside \
    over past round through throughout to together toward towards under up upon with within without";

/// Particles that may follow the verb of an English phrasal verb (`give up`, `look after`,
/// `get out of`).
const ENGLISH_PARTICLES: &str = "about above across after against ahead along among apart around aside at \
    away back behind below beneath beside between beyond by down for forth forward from in inside into near of \
    off on onto out outside over past round through throughout to together toward towards under up upon with \
    within without";

fn listed(list: &str, word: &str) -> bool {
    list.split_whitespace().any(|w| w == word)
}

/// Whether `word` is an English function word (does not carry an expression's level).
pub fn is_function_word(language: &str, word: &str) -> bool {
    language == "en" && listed(ENGLISH_FUNCTION_WORDS, word)
}

/// The rule an expression's band was estimated by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpressionRule {
    /// An English phrasal verb (a verb, then only particles): as hard as its verb, at least A2.
    PhrasalVerb,
    /// Any other expression: as hard as its rarest content word.
    RarestWord,
}

/// The estimated band of an expression from its words' lemmas and their bands (`None` = the
/// word has no band), the Subtitle extension's `expressionLevel` rule:
///
/// * an English **phrasal verb** — a first word that is not a function word followed only by
///   particles (`give up`, `look after`, `get out of`) — is as hard as its verb and at least
///   A2 (a verb with a particle is a step past the verb alone): `give up` A2, `fend off` like
///   `fend`;
/// * any other expression is as hard as its **rarest content word**, function words left out
///   (`in front of` = `front`); a word without a band does not count.
///
/// `None` when no word tells a level. (The extension also lifts idioms one level; the index
/// cannot tell an idiom from a set phrase, so it does not.)
pub fn expression_band(
    language: &str,
    words: &[(&str, Option<CefrBand>)],
) -> Option<(CefrBand, ExpressionRule)> {
    if words.len() < 2 {
        return None;
    }
    let phrasal = language == "en"
        && !listed(ENGLISH_FUNCTION_WORDS, words[0].0)
        && words[1..].iter().all(|(w, _)| listed(ENGLISH_PARTICLES, w));
    if phrasal {
        return words[0]
            .1
            .map(|b| (b.max(CefrBand::A2), ExpressionRule::PhrasalVerb));
    }
    words
        .iter()
        .filter(|(w, _)| !is_function_word(language, w))
        .filter_map(|(_, b)| *b)
        .max()
        .map(|b| (b, ExpressionRule::RarestWord))
}

/// One source of a pack's data, with its licence and the attribution it requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PackSource {
    /// Name of the data set.
    pub name: String,
    /// Where it comes from.
    #[serde(default)]
    pub url: String,
    /// Its licence.
    pub licence: String,
    /// The credit its licence asks for.
    #[serde(default)]
    pub attribution: String,
}

/// A pack as written on disk (`tools/word-data/README.md`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPack {
    format: String,
    format_version: u32,
    language: String,
    version: String,
    name: String,
    licence: String,
    sources: Vec<PackSource>,
    #[serde(default)]
    notes: String,
    words: Vec<String>,
    #[serde(default)]
    profile: BTreeMap<String, String>,
    #[serde(default)]
    forms: BTreeMap<String, String>,
    #[serde(default)]
    family: BTreeMap<String, String>,
}

/// A validated word-data pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordPack {
    /// Language (primary subtag).
    pub language: String,
    /// Pack version.
    pub version: String,
    /// Display name.
    pub name: String,
    /// Licence of the pack as a whole.
    pub licence: String,
    /// Sources, licences and attributions.
    pub sources: Vec<PackSource>,
    /// How it was made.
    pub notes: String,
    /// Lemmas, heard most first (rank = index + 1).
    pub words: Vec<String>,
    /// Headword → lowest level a vocabulary profile teaches it at.
    pub profile: BTreeMap<String, CefrBand>,
    /// Inflected form → lemma.
    pub forms: BTreeMap<String, String>,
    /// Derived lemma → head of its word family.
    pub family: BTreeMap<String, String>,
}

/// One row of a pack as the store keeps it: a form (a lemma is its own form) with what its
/// lemma has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordDataRow {
    /// Form (lower case).
    pub form: String,
    /// Its lemma.
    pub lemma: String,
    /// The lemma's rank (`None` = a profile headword the frequency list lacks).
    pub rank: Option<u32>,
    /// Band.
    pub band: Option<CefrBand>,
    /// Origin of the band.
    pub origin: Option<BandOrigin>,
}

/// Why a pack was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PackError(pub String);

fn bad(msg: impl Into<String>) -> PackError {
    PackError(msg.into())
}

fn check_word(what: &str, w: &str) -> Result<(), PackError> {
    if w.trim().is_empty() || w.len() > MAX_WORD_BYTES || w != w.trim() {
        return Err(bad(format!("{what} {w:?} is empty, padded or too long")));
    }
    if w.chars().any(char::is_uppercase) {
        return Err(bad(format!("{what} {w:?} is not lower case")));
    }
    Ok(())
}

impl WordPack {
    /// Parses and validates a pack (JSON bytes).
    pub fn parse(bytes: &[u8]) -> Result<Self, PackError> {
        let raw: RawPack =
            serde_json::from_slice(bytes).map_err(|e| bad(format!("not a word-data pack: {e}")))?;
        if raw.format != PACK_FORMAT || raw.format_version != PACK_FORMAT_VERSION {
            return Err(bad(format!(
                "format {:?} v{} is not {PACK_FORMAT} v{PACK_FORMAT_VERSION}",
                raw.format, raw.format_version
            )));
        }
        let language = crate::words::index_language(&raw.language);
        if language.is_empty() || language != raw.language {
            return Err(bad(format!(
                "language {:?} is not a primary subtag",
                raw.language
            )));
        }
        for (what, v, max) in [
            ("version", &raw.version, 40),
            ("name", &raw.name, 200),
            ("licence", &raw.licence, 200),
        ] {
            if v.trim().is_empty() || v.len() > max {
                return Err(bad(format!("{what} is empty or longer than {max} bytes")));
            }
        }
        if raw.sources.is_empty()
            || raw
                .sources
                .iter()
                .any(|s| s.name.trim().is_empty() || s.licence.trim().is_empty())
        {
            return Err(bad("every pack names its sources and their licences"));
        }
        if raw.words.is_empty() || raw.words.len() > MAX_PACK_WORDS {
            return Err(bad(format!("1–{MAX_PACK_WORDS} words")));
        }
        if [raw.profile.len(), raw.forms.len(), raw.family.len()]
            .iter()
            .any(|n| *n > MAX_PACK_MAP)
        {
            return Err(bad(format!(
                "at most {MAX_PACK_MAP} profile/forms/family entries"
            )));
        }
        let mut seen = HashSet::with_capacity(raw.words.len());
        for w in &raw.words {
            check_word("word", w)?;
            if !seen.insert(w.as_str()) {
                return Err(bad(format!("word {w:?} twice")));
            }
        }
        let mut profile = BTreeMap::new();
        for (w, level) in &raw.profile {
            check_word("profile headword", w)?;
            let band = CefrBand::parse(level)
                .ok_or_else(|| bad(format!("profile level {level:?} of {w:?} is not A1–C2")))?;
            profile.insert(w.clone(), band);
        }
        for (f, l) in &raw.forms {
            check_word("form", f)?;
            if !seen.contains(l.as_str()) || f == l {
                return Err(bad(format!("form {f:?} leads to {l:?}, not another word")));
            }
        }
        for (m, h) in &raw.family {
            if !seen.contains(m.as_str()) || !seen.contains(h.as_str()) || m == h {
                return Err(bad(format!("family {m:?} → {h:?} is not two words")));
            }
        }
        Ok(Self {
            language,
            version: raw.version,
            name: raw.name,
            licence: raw.licence,
            sources: raw.sources,
            notes: raw.notes,
            words: raw.words,
            profile,
            forms: raw.forms,
            family: raw.family,
        })
    }

    /// The rows the store keeps: every lemma (its rank and band), every profile headword the
    /// list lacks (band only), every inflected form (its lemma's rank and band). A form that is
    /// also a lemma or headword keeps the lemma's own row.
    pub fn rows(&self) -> Vec<WordDataRow> {
        let mut rows = Vec::with_capacity(self.words.len() + self.forms.len());
        let mut of_lemma: BTreeMap<&str, (u32, Option<(CefrBand, BandOrigin)>)> = BTreeMap::new();
        for (i, w) in self.words.iter().enumerate() {
            let rank = u32::try_from(i + 1).unwrap_or(u32::MAX);
            let band = word_band(Some(rank), self.profile.get(w).copied());
            of_lemma.insert(w, (rank, band));
            rows.push(WordDataRow {
                form: w.clone(),
                lemma: w.clone(),
                rank: Some(rank),
                band: band.map(|b| b.0),
                origin: band.map(|b| b.1),
            });
        }
        for (w, b) in &self.profile {
            if !of_lemma.contains_key(w.as_str()) {
                rows.push(WordDataRow {
                    form: w.clone(),
                    lemma: w.clone(),
                    rank: None,
                    band: Some(*b),
                    origin: Some(BandOrigin::Profile),
                });
            }
        }
        for (f, l) in &self.forms {
            if of_lemma.contains_key(f.as_str()) || self.profile.contains_key(f) {
                continue;
            }
            let Some((rank, band)) = of_lemma.get(l.as_str()) else {
                continue;
            };
            rows.push(WordDataRow {
                form: f.clone(),
                lemma: l.clone(),
                rank: Some(*rank),
                band: band.map(|b| b.0),
                origin: band.map(|b| b.1),
            });
        }
        rows
    }

    /// `(derived lemma, family head)` pairs.
    pub fn families(&self) -> Vec<(String, String)> {
        self.family
            .iter()
            .map(|(m, h)| (m.clone(), h.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_json(extra: &str) -> String {
        format!(
            r#"{{"format":"fc-word-data","format_version":1,"language":"en","version":"1.0.0",
            "name":"Test","licence":"CC BY-SA 4.0",
            "sources":[{{"name":"Fixture","url":"","licence":"CC0","attribution":"nobody"}}],
            "words":["you","go","act","give","january","actor"]{extra}}}"#
        )
    }

    #[test]
    fn bands_by_rank_and_profile() {
        assert_eq!(band_of_rank(1), CefrBand::A1);
        assert_eq!(band_of_rank(1000), CefrBand::A1);
        assert_eq!(band_of_rank(1001), CefrBand::A2);
        assert_eq!(band_of_rank(5999), CefrBand::B2);
        assert_eq!(band_of_rank(19_999), CefrBand::C2);
        assert_eq!(band_of_rank(45_000), CefrBand::C2);
        assert_eq!(
            word_band(Some(4400), Some(CefrBand::A1)),
            Some((CefrBand::A1, BandOrigin::Profile)),
            "a profile level wins over the rank"
        );
        assert_eq!(
            word_band(Some(4400), None),
            Some((CefrBand::B2, BandOrigin::Rank))
        );
        assert_eq!(word_band(None, None), None);
        assert!(BandOrigin::Rank.estimated() && !BandOrigin::Profile.estimated());
        assert_eq!(CefrBand::parse(" b2 "), Some(CefrBand::B2));
        assert_eq!(CefrBand::B1.top_rank(), 3500);
    }

    #[test]
    fn expression_rule() {
        use CefrBand::*;
        // A phrasal verb is as hard as its verb, at least A2.
        assert_eq!(
            expression_band("en", &[("give", Some(A1)), ("up", Some(A1))]),
            Some((A2, ExpressionRule::PhrasalVerb))
        );
        assert_eq!(
            expression_band("en", &[("fend", Some(C1)), ("off", Some(A1))]),
            Some((C1, ExpressionRule::PhrasalVerb))
        );
        assert_eq!(
            expression_band(
                "en",
                &[("get", Some(A1)), ("out", Some(A1)), ("of", Some(A1))]
            ),
            Some((A2, ExpressionRule::PhrasalVerb))
        );
        // Otherwise the rarest content word; function words do not count.
        assert_eq!(
            expression_band(
                "en",
                &[("in", Some(A1)), ("front", Some(A2)), ("of", Some(A1))]
            ),
            Some((A2, ExpressionRule::RarestWord))
        );
        assert_eq!(
            expression_band(
                "en",
                &[("kick", Some(A2)), ("the", Some(A1)), ("bucket", Some(B1))]
            ),
            Some((B1, ExpressionRule::RarestWord))
        );
        assert_eq!(
            expression_band("en", &[("give", None), ("up", Some(A1))]),
            None
        );
        assert_eq!(expression_band("en", &[("go", Some(A1))]), None, "one word");
        // Other languages: every word counts.
        assert_eq!(
            expression_band("de", &[("auf", Some(A1)), ("geben", Some(A2))]),
            Some((A2, ExpressionRule::RarestWord))
        );
    }

    #[test]
    fn a_pack_parses_into_rows() {
        let json = pack_json(
            r#","profile":{"january":"A1","look after":"A2"},
               "forms":{"went":"go","gone":"go","acts":"act"},
               "family":{"actor":"act"}"#,
        );
        let p = WordPack::parse(json.as_bytes()).unwrap();
        assert_eq!(p.language, "en");
        let rows = p.rows();
        let row = |f: &str| rows.iter().find(|r| r.form == f).unwrap().clone();
        assert_eq!(row("you").rank, Some(1));
        assert_eq!(row("went").lemma, "go");
        assert_eq!(row("went").rank, Some(2), "a form shares its lemma's rank");
        assert_eq!(row("january").band, Some(CefrBand::A1));
        assert_eq!(row("january").origin, Some(BandOrigin::Profile));
        assert_eq!(row("act").origin, Some(BandOrigin::Rank));
        assert_eq!(row("look after").rank, None);
        assert_eq!(row("look after").band, Some(CefrBand::A2));
        assert_eq!(row("actor").rank, Some(6), "derivation keeps its own rank");
        assert_eq!(rows.len(), 6 + 1 + 3);
        assert_eq!(p.families(), vec![("actor".into(), "act".into())]);
    }

    #[test]
    fn bad_packs_are_refused() {
        let cases = [
            pack_json(r#","forms":{"went":"goes"}"#),
            pack_json(r#","family":{"act":"act"}"#),
            pack_json(r#","profile":{"go":"D1"}"#),
            pack_json(r#","forms":{"Went":"go"}"#),
            pack_json(r#","surprise":1"#),
            pack_json("").replace("\"you\",", "\"go\","),
            pack_json("").replace("fc-word-data", "other"),
            pack_json("").replace("\"en\"", "\"en-GB\""),
            pack_json("").replace("\"licence\":\"CC0\"", "\"licence\":\"\""),
            "not json".to_owned(),
        ];
        for c in cases {
            assert!(WordPack::parse(c.as_bytes()).is_err(), "{c}");
        }
    }
}
