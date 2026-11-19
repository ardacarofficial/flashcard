//! Small TR/EN message catalog for the CLI's human output (undo/redo summaries etc.).
//!
//! Files: `crates/fc-cli/locales/<lang>/<feature>.json`, flat `{"key": "text {param}"}`,
//! embedded at build time (`build.rs`). A feature adds its own file per language. Lookup
//! falls back to English, then to the key itself.

use fc_api::v1::common::I18nMessage;
use fc_core::i18n::Locale;
use std::collections::BTreeMap;

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

/// Language directory name of a locale.
pub fn lang_code(locale: Locale) -> &'static str {
    match locale {
        Locale::Tr => "tr",
        Locale::En => "en",
    }
}

/// All entries of one language: `key → text` (files merged; a malformed file is skipped,
/// which the catalog tests catch).
fn entries(lang: &str) -> BTreeMap<String, String> {
    LOCALE_FILES
        .iter()
        .filter(|(l, _, _)| *l == lang)
        .flat_map(|(_, _, src)| {
            serde_json::from_str::<BTreeMap<String, String>>(src).unwrap_or_default()
        })
        .collect()
}

/// A loaded catalog for one locale (with English fallback).
#[derive(Debug, Clone)]
pub struct Catalog {
    primary: BTreeMap<String, String>,
    fallback: BTreeMap<String, String>,
}

impl Catalog {
    /// Catalog for `locale`.
    pub fn new(locale: Locale) -> Self {
        Self {
            primary: entries(lang_code(locale)),
            fallback: entries("en"),
        }
    }

    /// Translates `key` with `{param}` interpolation.
    pub fn text(&self, key: &str, params: &BTreeMap<String, String>) -> String {
        let template = self
            .primary
            .get(key)
            .or_else(|| self.fallback.get(key))
            .map_or(key, String::as_str);
        let mut out = template.to_owned();
        for (name, value) in params {
            out = out.replace(&format!("{{{name}}}"), value);
        }
        out
    }

    /// Translates a backend message.
    pub fn message(&self, m: &I18nMessage) -> String {
        self.text(&m.key, &m.params)
    }

    /// Translates `key` with a single param.
    pub fn with(&self, key: &str, name: &str, value: impl Into<String>) -> String {
        self.text(key, &BTreeMap::from([(name.to_owned(), value.into())]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_api::MethodKind;
    use std::collections::BTreeSet;

    fn langs() -> BTreeSet<&'static str> {
        LOCALE_FILES.iter().map(|(l, _, _)| *l).collect()
    }

    #[test]
    fn every_file_parses_and_keys_are_not_duplicated() {
        for lang in langs() {
            let mut seen = BTreeSet::new();
            for (l, feature, src) in LOCALE_FILES.iter().filter(|(l, _, _)| *l == lang) {
                let map: BTreeMap<String, String> = serde_json::from_str(src)
                    .unwrap_or_else(|e| panic!("locales/{l}/{feature}.json: {e}"));
                for (k, v) in map {
                    assert!(!v.trim().is_empty(), "{l}/{feature}: empty text for {k}");
                    assert!(seen.insert(k.clone()), "{l}: key {k} defined twice");
                }
            }
        }
    }

    #[test]
    fn tr_and_en_have_the_same_files_and_keys() {
        assert_eq!(langs(), BTreeSet::from(["en", "tr"]));
        let files = |lang: &str| -> BTreeSet<&str> {
            LOCALE_FILES
                .iter()
                .filter(|(l, _, _)| *l == lang)
                .map(|(_, f, _)| *f)
                .collect()
        };
        assert_eq!(files("en"), files("tr"));
        let keys = |lang| entries(lang).into_keys().collect::<BTreeSet<_>>();
        assert_eq!(keys("en"), keys("tr"));
    }

    /// Every command that can show up in undo/redo has a summary text in both languages.
    #[test]
    fn every_command_summary_is_translated() {
        for lang in ["en", "tr"] {
            let have = entries(lang);
            for m in fc_api::v1::catalog() {
                if m.kind != MethodKind::Command || matches!(m.name, "oplog.undo" | "oplog.redo") {
                    continue;
                }
                let key = format!("oplog.summary.{}", m.name);
                assert!(have.contains_key(&key), "{lang}: missing {key}");
            }
        }
    }

    #[test]
    fn translates_with_params_and_falls_back() {
        let tr = Catalog::new(Locale::Tr);
        let m = I18nMessage::key("oplog.summary.decks.create").param("path", "Dil::İngilizce");
        assert_eq!(tr.message(&m), "Deste oluşturuldu: Dil::İngilizce");
        let en = Catalog::new(Locale::En);
        assert_eq!(en.message(&m), "Created deck Dil::İngilizce");
        assert_eq!(en.text("no.such.key", &BTreeMap::new()), "no.such.key");
        assert_eq!(tr.with("cli.undone", "summary", "x"), "Geri alındı: x");
    }
}
