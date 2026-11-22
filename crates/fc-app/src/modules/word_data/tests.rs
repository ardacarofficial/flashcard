//! Word data tests (v1.4 B2), against SQLite and the in-memory store.
//!
//! * The shipped English pack is installed at start-up once, with its sources and licences
//!   readable — `the_shipped_pack_is_installed_once_with_its_sources`.
//! * Rank and band on `words.lookup`; inflection joins the lemma, derivation keeps its own
//!   entry and its family never makes it known — `lookup_has_rank_band_and_family`.
//! * Expressions get an estimated band by the documented rule — same test.
//! * `words.list` sorts/filters by rank and band, paging included — `list_by_frequency_*`.
//! * The module switch — `switched_off_*`; import / reset — `import_and_reset`.
//! * Performance: a 50 000-word pack loads and 500 forms are looked up —
//!   `a_50k_pack_loads_and_500_forms_look_up_fast`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::common::Empty;
use fc_api::v1::decks::CreateDeck;
use fc_api::v1::io::ImportSource;
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::CreateNote;
use fc_api::v1::reading::MarkWords;
use fc_api::v1::settings::SetSetting;
use fc_api::v1::words::*;
use fc_api::ErrorCode;
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::reading::{WordMark, WordStatus};
use fc_core::settings::SettingScope;
use fc_core::word_data::{BandOrigin, CefrBand};
use fc_storage::Store;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

/// A small pack: ranks by position, a profile, forms and a family.
const FIXTURE: &str = r#"{"format":"fc-word-data","format_version":1,"language":"en",
 "version":"0.0.1","name":"Fixture","licence":"CC0",
 "sources":[{"name":"Test fixture","url":"","licence":"CC0","attribution":"none"}],
 "words":["go","give","up","the","house","act","door","bucket","kick","january","actor","action","fend","off"],
 "profile":{"january":"A1","according to":"A2","fend":"C1"},
 "forms":{"went":"go","gone":"go","houses":"house","gave":"give"},
 "family":{"actor":"act","action":"act"}}"#;

fn import_fixture(t: &TestApp) -> WordDataPackDto {
    t.ok::<WordsDataImport>(ImportWordData {
        source: ImportSource::Text {
            text: FIXTURE.into(),
        },
    })
}

/// An app over `store` with an English deck holding `words` (one note each).
fn app_with(store: Arc<dyn Store>, words: &[&str]) -> TestApp {
    let t = TestApp::new(store);
    let d = t
        .ok::<DecksCreate>(CreateDeck {
            path: "English".into(),
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        })
        .id;
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::Deck(d),
        key: crate::modules::words::LANGUAGE.into(),
        value: serde_json::json!("en"),
    });
    for w in words {
        let mut fields = BTreeMap::new();
        fields.insert("front".to_owned(), (*w).to_owned());
        fields.insert("back".to_owned(), "x".to_owned());
        t.ok::<NotesCreate>(CreateNote {
            notetype_id: CLASSIC_ID,
            deck_id: d,
            fields,
            tags: vec![],
            attributes: vec![],
            allow_duplicate: Some(true),
        });
    }
    t
}

fn lookup(t: &TestApp, forms: &[&str]) -> Vec<WordFormResult> {
    t.ok::<WordsLookup>(LookupWords {
        language: "en".into(),
        forms: forms.iter().map(|s| (*s).to_owned()).collect(),
    })
    .forms
}

fn mark(t: &TestApp, lemmas: &[&str], m: Option<WordMark>) {
    t.ok::<ReadingWordsMark>(MarkWords {
        language: "en".into(),
        lemmas: lemmas.iter().map(|s| (*s).to_owned()).collect(),
        mark: m,
    });
}

fn switch(t: &TestApp, on: bool) {
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: super::ID.into(),
        enabled: on,
    });
}

#[test]
fn the_shipped_pack_is_installed_once_with_its_sources() {
    let store: Arc<dyn Store> = Arc::new(TempSqlite::new());
    let t = TestApp::new(store.clone());
    let s = t.ok::<WordsDataSources>(Empty {});
    assert_eq!(s.packs.len(), 1);
    let en = &s.packs[0];
    assert_eq!((en.language.as_str(), en.origin.as_str()), ("en", "bundled"));
    assert!(en.words > 30_000, "{}", en.words);
    assert!(en.licence.contains("CC BY-SA"));
    assert!(en.sources.iter().any(|s| s.name.contains("CEFR-J")));
    assert!(en.sources.iter().any(|s| s.name.contains("FrequencyWords")));
    assert_eq!(s.bundled.len(), 1);
    assert_eq!(s.bundled[0].version, en.version);
    // A second start leaves the installed pack alone (same bytes).
    let again = TestApp::new(store);
    let s2 = again.ok::<WordsDataSources>(Empty {});
    assert_eq!(s2.packs[0].installed_at, en.installed_at);
    // And the real pack answers: `went` has `go`'s rank, `actor` its own.
    let f = lookup(&again, &["went", "go", "actor", "act", "january"]);
    let level = |i: usize| f[i].level.clone().unwrap();
    assert_eq!(level(0).rank, level(1).rank);
    assert_eq!(level(0).lemma.as_deref(), Some("go"));
    assert_ne!(level(2).rank, level(3).rank);
    assert_eq!(level(4).band, Some(CefrBand::A1));
    assert_eq!(level(4).band_origin, Some(BandOrigin::Profile));
    assert!(f[2].family.iter().any(|m| m.key == "act" && m.head));
}

fn lookup_has_rank_band_and_family(store: Arc<dyn Store>) {
    let t = app_with(store, &["act", "go"]);
    let pack = import_fixture(&t);
    assert_eq!((pack.origin.as_str(), pack.words, pack.families), ("imported", 14, 2));
    mark(&t, &["act"], Some(WordMark::Known));
    let f = lookup(
        &t,
        &["went", "Go", "actor", "act", "give up", "kick the bucket", "according to", "fend off", "zzz"],
    );
    let level = |i: usize| f[i].level.clone();
    // Inflection joins the lemma: `went` is `go`, rank 1, band by rank (estimated).
    let went = level(0).unwrap();
    assert_eq!((went.lemma.as_deref(), went.rank), (Some("go"), Some(1)));
    assert_eq!((went.band, went.band_origin, went.estimated), (Some(CefrBand::A1), Some(BandOrigin::Rank), true));
    assert_eq!(level(1), level(0));
    assert_eq!(f[1].word.as_ref().unwrap().level, level(1), "index row carries it too");
    // Derivation keeps its own entry; the family is shown and makes nothing known.
    assert_eq!(level(2).unwrap().rank, Some(11));
    assert_eq!(f[2].status, WordStatus::New, "knowing act does not make actor known");
    let fam: Vec<(&str, WordStatus, bool)> =
        f[2].family.iter().map(|m| (m.key.as_str(), m.status, m.head)).collect();
    assert_eq!(fam, [("act", WordStatus::Known, true), ("action", WordStatus::New, false)]);
    let fam: Vec<&str> = f[3].family.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(fam, ["action", "actor"]);
    // Expressions: the documented rule, estimated.
    let give_up = level(4).unwrap();
    assert_eq!(
        (give_up.band, give_up.band_origin, give_up.estimated, give_up.rank),
        (Some(CefrBand::A2), Some(BandOrigin::Expression), true, None),
        "a phrasal verb is at least A2"
    );
    assert_eq!(level(5).unwrap().band, Some(CefrBand::A1), "rarest content word (bucket, rank 8)");
    let according = level(6).unwrap();
    assert_eq!((according.band, according.band_origin), (Some(CefrBand::A2), Some(BandOrigin::Profile)));
    assert_eq!(level(7).unwrap().band, Some(CefrBand::C1), "fend off is as hard as fend");
    assert_eq!(level(8), None);
}

#[test]
fn lookup_has_rank_band_and_family_sqlite() {
    lookup_has_rank_band_and_family(Arc::new(TempSqlite::new()));
}

#[test]
fn lookup_has_rank_band_and_family_mem() {
    lookup_has_rank_band_and_family(Arc::new(MemStore::new()));
}

fn keys(l: &WordList) -> Vec<&str> {
    l.words.iter().map(|w| w.key.as_str()).collect()
}

fn list_by_frequency(store: Arc<dyn Store>) {
    let t = app_with(store, &["door", "house", "went", "actor", "kick the bucket", "zebra", "january"]);
    import_fixture(&t);
    mark(&t, &["house"], Some(WordMark::Known));
    let list = |sort, bands: Vec<CefrBand>, max_rank, status, cursor, limit| {
        t.ok::<WordsList>(ListWords {
            language: Some("en".into()),
            status,
            cursor,
            limit,
            sort: Some(sort),
            bands,
            max_rank,
        })
    };
    // Most frequent words I don't know yet: by rank, `new` and `learning` only… here the
    // notes have no review yet, so every carded word is `learning`.
    let by_rank = list(WordSort::Rank, vec![], None, Some(WordStatus::Learning), None, None);
    assert_eq!(keys(&by_rank), ["go", "door", "january", "actor", "kick the bucket", "zebra"]);
    let lvl = |w: &WordEntryDto| w.level.clone().map(|l| (l.rank, l.band));
    assert_eq!(lvl(&by_rank.words[0]), Some((Some(1), Some(CefrBand::A1))));
    assert_eq!(
        lvl(&by_rank.words[4]),
        Some((None, Some(CefrBand::A1))),
        "expression band by the rule, no rank"
    );
    assert_eq!(by_rank.words[5].level, None, "unknown to the pack");
    // Paging through the sorted list gives the same order.
    let mut paged = Vec::new();
    let mut cursor = None;
    loop {
        let p = list(WordSort::Rank, vec![], None, Some(WordStatus::Learning), cursor, Some(2));
        paged.extend(p.words.iter().map(|w| w.key.clone()));
        cursor = p.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(paged, keys(&by_rank));
    // Filters: rank ≤ 7; band A1 sorted by band (all A1 in the fixture except none).
    let top = list(WordSort::Rank, vec![], Some(7), None, None, None);
    assert_eq!(keys(&top), ["go", "house", "door"]);
    let a1 = list(WordSort::Band, vec![CefrBand::A1], None, None, None, None);
    assert_eq!(keys(&a1), ["go", "house", "door", "january", "actor"]);
    let none = list(WordSort::Band, vec![CefrBand::C2], None, None, None, None);
    assert!(none.words.is_empty());
    // A key-sorted list keeps the index's order and carries levels.
    let by_key = list(WordSort::Key, vec![], None, None, None, None);
    assert_eq!(keys(&by_key)[0], "actor");
    assert!(by_key.words[0].level.is_some());
    // Stats per band.
    let s = t.ok::<WordsStats>(Empty {});
    let en = &s.languages[0];
    let a1 = en.bands.iter().find(|b| b.band == Some(CefrBand::A1)).unwrap();
    assert_eq!((a1.counts.known, a1.counts.learning), (1, 4));
    let unknown = en.bands.iter().find(|b| b.band.is_none()).unwrap();
    assert_eq!(unknown.counts.learning, 2, "zebra and the expression (no pack row)");
}

#[test]
fn list_by_frequency_sqlite() {
    list_by_frequency(Arc::new(TempSqlite::new()));
}

#[test]
fn list_by_frequency_mem() {
    list_by_frequency(Arc::new(MemStore::new()));
}

#[test]
fn switched_off_words_carry_no_level_and_lists_refuse_frequency_order() {
    let t = app_with(Arc::new(TempSqlite::new()), &["go"]);
    import_fixture(&t);
    switch(&t, false);
    let f = lookup(&t, &["went", "actor"]);
    assert!(f.iter().all(|f| f.level.is_none() && f.family.is_empty()));
    assert_eq!(f[0].status, WordStatus::Learning, "the index itself still answers");
    let e = t.err::<WordsList>(ListWords {
        sort: Some(WordSort::Rank),
        ..ListWords::default()
    });
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
    let plain = t.ok::<WordsList>(ListWords::default());
    assert_eq!(keys(&plain), ["go"]);
    assert!(plain.words[0].level.is_none());
    assert!(t.ok::<WordsStats>(Empty {}).languages[0].bands.is_empty());
    switch(&t, true);
    assert_eq!(lookup(&t, &["went"])[0].level.as_ref().unwrap().rank, Some(1));
}

#[test]
fn import_and_reset() {
    let store: Arc<dyn Store> = Arc::new(TempSqlite::new());
    let t = app_with(store.clone(), &[]);
    let bad = t.err::<WordsDataImport>(ImportWordData {
        source: ImportSource::Text {
            text: FIXTURE.replace("\"went\":\"go\"", "\"went\":\"nowhere\""),
        },
    });
    assert_eq!(bad.code, ErrorCode::ValidationInvalidParams);
    // A failed import leaves the shipped pack in place.
    assert_eq!(t.ok::<WordsDataSources>(Empty {}).packs[0].origin, "bundled");
    // From a file; a restart does not put the shipped pack over it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("en.json");
    std::fs::write(&path, FIXTURE).unwrap();
    t.ok::<WordsDataImport>(ImportWordData {
        source: ImportSource::File {
            path: path.to_string_lossy().into_owned(),
        },
    });
    let again = TestApp::new(store);
    let s = again.ok::<WordsDataSources>(Empty {});
    assert_eq!((s.packs[0].origin.as_str(), s.packs[0].version.as_str()), ("imported", "0.0.1"));
    assert_eq!(lookup(&again, &["door"])[0].level.as_ref().unwrap().rank, Some(7));
    // Reset puts the shipped one back; a language without one loses its pack.
    let r = again.ok::<WordsDataReset>(ResetWordData { language: "en-US".into() });
    assert_eq!(r.packs[0].origin, "bundled");
    let r = again.ok::<WordsDataReset>(ResetWordData { language: "xx".into() });
    assert_eq!(r.packs.len(), 1);
}

/// Synthetic word `i` (letters only: the tokenizer drops digits).
fn word(i: usize) -> String {
    let mut s = String::from("q");
    let mut n = i;
    loop {
        s.push(char::from(b'a' + u8::try_from(n % 26).unwrap()));
        n /= 26;
        if n == 0 {
            return s;
        }
    }
}

/// A synthetic pack of `n` words (`word(0)` most heard) with a form (`x…`) per word and a
/// family per tenth word.
fn big_pack(n: usize) -> String {
    let words: Vec<String> = (0..n).map(word).collect();
    let forms: BTreeMap<String, String> = (0..n).map(|i| (format!("x{}", word(i)), word(i))).collect();
    let family: BTreeMap<String, String> = (1..n).step_by(10).map(|i| (word(i), word(i - 1))).collect();
    serde_json::json!({
        "format": "fc-word-data", "format_version": 1, "language": "en", "version": "9.9.9",
        "name": "Big", "licence": "CC0",
        "sources": [{"name": "Synthetic", "url": "", "licence": "CC0", "attribution": ""}],
        "words": words, "forms": forms, "family": family,
    })
    .to_string()
}

#[test]
fn a_50k_pack_loads_and_500_forms_look_up_fast() {
    let (w10, w20k) = (word(10), format!("x{}", word(20_000)));
    let t = app_with(Arc::new(TempSqlite::new()), &[&w10, &w20k]);
    let text = big_pack(50_000);
    let start = Instant::now();
    let pack = t.ok::<WordsDataImport>(ImportWordData {
        source: ImportSource::Text { text },
    });
    let load = start.elapsed();
    assert_eq!((pack.words, pack.forms, pack.families), (50_000, 100_000, 5_000));
    let forms: Vec<String> = (0..500).map(|i| format!("x{}", word(i * 97))).collect();
    let start = Instant::now();
    let found = t
        .ok::<WordsLookup>(LookupWords {
            language: "en".into(),
            forms: forms.clone(),
        })
        .forms;
    let look = start.elapsed();
    println!("50k-word pack: load {load:?}; 500-form lookup {look:?}");
    assert!(found.iter().all(|f| f.level.is_some()));
    assert_eq!(found[1].level.as_ref().unwrap().rank, Some(98));
    let start = Instant::now();
    let l = t.ok::<WordsList>(ListWords {
        sort: Some(WordSort::Rank),
        ..ListWords::default()
    });
    println!("rank-sorted list: {:?}", start.elapsed());
    assert_eq!(keys(&l), [w10.as_str(), w20k.as_str()]);
    // Generous bounds (debug build, shared machine).
    assert!(load.as_secs() < 30, "load took {load:?}");
    assert!(look.as_secs() < 5, "lookup took {look:?}");
}
