//! Language-learning predicates of the query language (v1.4, docs/02 §9.1): the word's
//! status from the word index, its language, the lemma/expression it belongs to, how often it
//! was met, where the note came from (origin and source title / video), its media and — when a
//! word-data pack is installed — its frequency rank and CEFR band.
//!
//! One word per concept, a Turkish and an English key each:
//!
//! | Key | Meaning |
//! |---|---|
//! | `durum:` `status:` | word status from the index: `yeni`/`new`, `öğreniyorum`/`learning`, `biliyorum`/`known`, `yoksay`/`ignored` |
//! | `dil:` `lang:` | the word's language (primary subtag, `dil:en`); `dil:yok` = none known |
//! | `kelime:` `word:` | lemma or expression, inflection understood (`kelime:went` = `kelime:go`); `*` = glob over the lemma |
//! | `karşılaşma` `seen` | encounters of the same word (v1.4 W1: each note once, a Subtitle note once per captured scene), `karşılaşma>=2` |
//! | `nereden:` `from:` | `altyazı`/`subtitle`, `okuma`/`reading`, `elle`/`manual`, `anki`, `içe_aktarma`/`import` |
//! | `kaynak:` `source:` | source title (video/episode, reading text), site or video id; glob |
//! | `ses:` `audio:` | `var`/`yes`, `yok`/`none`, `video` (clip from the video), `dosya`/`file`, `tts` |
//! | `görsel:` `image:` | `var`/`yes`, `yok`/`none`, `video` (screenshot), `dosya`/`file` |
//! | `sıklık` `freq` | frequency rank of the word (`freq<=2000`), needs a word-data pack |
//! | `cefr:` (`level:`, `seviye:` when no classification has that name) | CEFR band, ordered (`cefr>=B1`) |
//!
//! A user classification with the same key always wins over these keys (queries saved before
//! v1.4 keep their meaning).

use super::ast::{Expr, Term};
use super::text::fold;
use crate::reading::WordStatus;
use crate::words::WordEvidence;
use serde::{Deserialize, Serialize};

/// Where a note came from (`note_origins.origin`; no row = [`NoteOrigin::Manual`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteOrigin {
    /// Captured with the Subtitle browser extension.
    Subtitle,
    /// Made in reading mode.
    Reading,
    /// Imported from an Anki package.
    Anki,
    /// Imported from another format (text, Quizlet, JSON, `.fcpack`).
    Import,
    /// Made by hand (or of unknown origin).
    #[default]
    Manual,
}

impl NoteOrigin {
    /// Every origin.
    pub const ALL: [Self; 5] = [
        Self::Subtitle,
        Self::Reading,
        Self::Anki,
        Self::Import,
        Self::Manual,
    ];

    /// Storage string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Subtitle => "subtitle",
            Self::Reading => "reading",
            Self::Anki => "anki",
            Self::Import => "import",
            Self::Manual => "manual",
        }
    }

    /// From the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.as_str() == s)
    }

    /// From a query value (folded; Turkish and English words).
    pub fn from_query(folded: &str) -> Option<Self> {
        Some(match folded {
            "subtitle" | "altyazi" | "video" => Self::Subtitle,
            "reading" | "okuma" => Self::Reading,
            "anki" => Self::Anki,
            "import" | "ice_aktarma" | "iceaktarma" => Self::Import,
            "manual" | "elle" => Self::Manual,
            _ => return None,
        })
    }
}

/// Kind of a stored media file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    /// `audio/*`.
    Audio,
    /// `image/*`.
    Image,
}

impl MediaKind {
    /// MIME prefix (`audio/`).
    pub const fn mime_prefix(self) -> &'static str {
        match self {
            Self::Audio => "audio/",
            Self::Image => "image/",
        }
    }
}

/// Which media files count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaFrom {
    /// Any stored file of the kind.
    Any,
    /// Taken from the video by the Subtitle extension (audio clip, screenshot).
    Video,
    /// Any other file (imported, added by hand).
    File,
}

/// File-name prefix of media the Subtitle extension uploaded (`subtitle-audio-<capture>.mp3`,
/// `subtitle-screenshot-<capture>.jpg`): what makes a file "from the video".
pub const VIDEO_MEDIA_PREFIX: &str = "subtitle-";

/// Whether a media file's original name marks it as taken from a video.
pub fn is_video_media(original_name: Option<&str>) -> bool {
    original_name.is_some_and(|n| {
        n.get(..VIDEO_MEDIA_PREFIX.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(VIDEO_MEDIA_PREFIX))
    })
}

/// CEFR bands in order (`A1` = 1 … `C2` = 6).
pub const CEFR_BANDS: [&str; 6] = ["a1", "a2", "b1", "b2", "c1", "c2"];

/// Ordinal (1–6) of a CEFR band (`B1` → 3).
pub fn cefr_ordinal(band: &str) -> Option<u8> {
    let b = fold(band.trim());
    CEFR_BANDS
        .iter()
        .position(|x| *x == b)
        .and_then(|i| u8::try_from(i + 1).ok())
}

/// Key form of a word or expression as typed: lower case, single spaces (`Pick  Up` →
/// `pick up`). Lemma keys from the lemmatizer are compared with it too.
pub fn word_key(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Whether a (folded) key is one of the word keys (`kelime:`/`word:`).
pub fn is_word_key(folded_key: &str) -> bool {
    matches!(folded_key, "kelime" | "word")
}

/// Values of every `kelime:`/`word:` term without wildcards, as typed: the app lemmatizes them
/// before resolving ([`super::Catalog::word_keys`]); the resolver stays pure.
pub fn word_values(expr: &Expr) -> Vec<String> {
    fn walk(e: &Expr, out: &mut Vec<String>) {
        match e {
            Expr::And(v) | Expr::Or(v) => v.iter().for_each(|x| walk(x, out)),
            Expr::Not(x) => walk(x, out),
            Expr::Term(Term {
                key: Some(k),
                value,
                ..
            }) if is_word_key(&fold(k)) && !value.contains('*') => {
                if !out.contains(value) {
                    out.push(value.clone());
                }
            }
            Expr::Term(_) => {}
        }
    }
    let mut out = Vec::new();
    walk(expr, &mut out);
    out
}

/// Source title of a note's `source` field (HTML as the Subtitle extension or reading mode
/// wrote it, `<a href="…">Title · 1:23</a>`): the plain text without the trailing time.
pub fn source_title(field_html: &str) -> String {
    let text = crate::note::strip_html(field_html);
    let text = text.trim();
    let is_time = |t: &str| {
        let parts: Vec<&str> = t.split(':').collect();
        (2..=3).contains(&parts.len())
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.len() <= 2 && p.chars().all(|c| c.is_ascii_digit()))
    };
    match text.rsplit_once(" · ") {
        Some((title, time)) if is_time(time.trim()) => title.trim().to_owned(),
        _ if is_time(text) => String::new(),
        _ => text.to_owned(),
    }
}

/// What the language predicates look at for one note (the in-memory reference evaluator,
/// [`super::eval`]). Storage reads the same facts from `word_index_notes`/`word_index`,
/// `note_origins`, `note_media`/`media` and the optional `word_frequency` table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LangFacts {
    /// Index language of the note (`word_index_notes.language`; `""` = none).
    pub language: String,
    /// Its word key (`word_index_notes.key`), if it is in the index.
    pub key: Option<String>,
    /// Evidence of its word (`word_index` row of `(language, key)`); `None` without a language.
    pub word: Option<WordEvidence>,
    /// Origin.
    pub origin: NoteOrigin,
    /// Source title, site and video id (the non-empty ones).
    pub source: Vec<String>,
    /// Stored media files the note references: kind and whether taken from a video.
    pub media: Vec<(MediaKind, bool)>,
    /// Frequency rank of the word (word-data pack).
    pub rank: Option<u32>,
    /// CEFR band ordinal (1–6) of the word (word-data pack).
    pub band: Option<u8>,
    /// v1.4 W1: encounters of the word beyond one per note: for each of its notes, its
    /// Subtitle encounters minus one (`subtitle_encounter_counts`). `karşılaşma`/`seen` =
    /// the word's notes + this.
    pub extra_encounters: u32,
}

impl LangFacts {
    /// A note nothing is known about.
    pub const NONE: LangFacts = LangFacts {
        language: String::new(),
        key: None,
        word: None,
        origin: NoteOrigin::Manual,
        source: Vec::new(),
        media: Vec::new(),
        rank: None,
        band: None,
        extra_encounters: 0,
    };

    /// Status of the note's word with the "known by maturity" threshold.
    pub fn status(&self, known_days: u32) -> Option<WordStatus> {
        self.word.as_ref().map(|w| w.status(known_days))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_and_helpers() {
        assert_eq!(cefr_ordinal(" b1 "), Some(3));
        assert_eq!(cefr_ordinal("C2"), Some(6));
        assert_eq!(cefr_ordinal("D1"), None);
        assert_eq!(word_key("  Pick   UP "), "pick up");
        assert!(is_video_media(Some("Subtitle-audio-x.mp3")));
        assert!(!is_video_media(Some("sub.mp3")));
        assert!(!is_video_media(None));
        assert_eq!(
            NoteOrigin::from_query("altyazi"),
            Some(NoteOrigin::Subtitle)
        );
        assert_eq!(NoteOrigin::parse("anki"), Some(NoteOrigin::Anki));
        assert_eq!(
            source_title(
                "<a href=\"https://x.test/?v=1&amp;t=83s\">The Night Train · S1E2 · 1:23</a>"
            ),
            "The Night Train · S1E2"
        );
        assert_eq!(source_title("netflix"), "netflix");
        assert_eq!(source_title("Book · 1:02:03"), "Book");
        let e =
            super::super::parse_query("kelime:went (word:\"gave up\" OR -kelime:go*) kelime:went")
                .unwrap();
        assert_eq!(word_values(&e), ["went", "gave up"]);
    }
}
