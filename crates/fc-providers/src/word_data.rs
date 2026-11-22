//! Word-data packs shipped with the app (v1.4, docs/03 §4.3): compiled into the binary, so
//! frequency ranks and CEFR bands need no network and no file at runtime. The pack format and
//! how a pack is built: `tools/word-data/README.md`; parsing and the rules:
//! `fc_core::word_data`.
//!
//! The English pack is CC BY-SA 4.0 (FrequencyWords / OpenSubtitles 2018 counts, Wiktionary
//! forms, the Octanove C1/C2 profile) and cites the CEFR-J Wordlist 1.5, as its terms ask;
//! its `sources` carry the attributions, shown by `words.data.sources`.

/// `(language, pack bytes)` of every shipped pack.
pub const BUNDLED: &[(&str, &[u8])] = &[("en", include_bytes!("../data/word-data/en.json"))];

/// The shipped pack of `language` (primary subtag), if any.
pub fn bundled(language: &str) -> Option<&'static [u8]> {
    BUNDLED
        .iter()
        .find(|(l, _)| *l == language)
        .map(|(_, b)| *b)
}

#[cfg(test)]
mod tests {
    use fc_core::word_data::WordPack;

    #[test]
    fn every_shipped_pack_is_valid_and_attributed() {
        for (language, bytes) in super::BUNDLED {
            let pack = WordPack::parse(bytes).unwrap();
            assert_eq!(pack.language, *language);
            assert!(pack.words.len() > 30_000, "{}", pack.words.len());
            assert!(pack.sources.len() >= 3);
            assert!(pack
                .sources
                .iter()
                .all(|s| !s.licence.is_empty() && !s.name.is_empty()));
        }
        let en = WordPack::parse(super::bundled("en").unwrap()).unwrap();
        let rank = |w: &str| en.words.iter().position(|x| x == w).map(|i| i + 1);
        assert!(rank("go").unwrap() < 100);
        assert_eq!(en.forms.get("went").map(String::as_str), Some("go"));
        assert_eq!(en.family.get("actor").map(String::as_str), Some("act"));
        assert!(en.profile.contains_key("january"));
    }
}
