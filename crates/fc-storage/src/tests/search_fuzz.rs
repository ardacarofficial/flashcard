//! v1.4.1 robustness round: the query language against the store, fuzzed.
//!
//! * random And/Or/Not combinations of every known atom ([`ATOMS`] ∪ [`LANG_ATOMS`]) over
//!   several small random worlds (with and without word data);
//! * generated atoms: every key alias, every operator, values full of SQL/LIKE/glob specials,
//!   Turkish letters, huge and malformed numbers, long and empty values, plus raw query text.
//!
//! Property: a query that parses and resolves never makes the store error and selects exactly
//! what the reference evaluator (`fc_core::query::eval`) selects — list, count and correlated
//! probe alike; a query that does not is a positioned error, never a panic.
//!
//! Deterministic: `FC_FUZZ_SEED` (u64) overrides the seed, `FC_FUZZ_CASES` the number of cases
//! per test (split over the worlds). A failure prints the seed, the world and the minimal query.

use super::search::{with_lang, world_with, World, ATOMS, LANG_ATOMS, NOW, WORDS};
use super::{cid, nid};
use crate::repo::{CardSearch, CardSort};
use fc_core::query::canonical::attr_key;
use fc_core::query::resolve::BUILTIN_KEYS;
use fc_core::query::{Filter, QueryErrorKind};
use fc_core::rng::{Rng, SplitMix64};
use fc_core::{AttributeDefId, CardId};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestError, TestRng, TestRunner};

const DEFAULT_SEED: u64 = 0x0001_0401_f022_5eed;

/// Note text with the characters a SQL/LIKE/glob/FTS translation could trip over.
const SPECIAL_WORDS: &[&str] = &[
    "50%_off",
    "a\\b",
    "it's",
    "say \"hi\"",
    "x*y",
    "why?",
    "[ab]",
    "a_b",
    "100%",
    "İĞŞ",
    "ığş",
    "<b>kalın</b>",
    "a&amp;b",
    "-5",
    "1e308",
    "NaN",
    "café",
    "",
    "  ",
];

/// Values: specials, Turkish, numbers of every shape, regexes, and the valid words of each key.
const VALUES: &[&str] = &[
    "%",
    "_",
    "\\",
    "'",
    "\"",
    "*",
    "?",
    "[",
    "]",
    "%%",
    "_%",
    "\\%",
    "\\_",
    "\\*",
    "**",
    "*%*",
    "*_*",
    "a%",
    "%a",
    "50%_off",
    "50%*",
    "a\\b",
    "a\\\\b",
    "it's",
    "it''s",
    "'; DROP TABLE notes; --",
    "\" OR 1=1 --",
    "x*y",
    "x\\*y",
    "why?",
    "[ab]",
    "[a-z]*",
    "a_b",
    "İ",
    "ı",
    "ş",
    "ğ",
    "I",
    "i",
    "İstanbul",
    "istanbul",
    "ISTANBUL",
    "IŞIK",
    "ışık",
    "kılıç",
    "KILIÇ",
    "kilic*",
    "ş*",
    "*ğ*",
    "",
    " ",
    "  ",
    "\t",
    "-",
    "--",
    "0",
    "-0",
    "1",
    "-1",
    "2",
    "3",
    "5.5",
    "1,5",
    "0.9",
    "-2",
    "1e308",
    "1e309",
    "-1e309",
    "1e-320",
    "NaN",
    "nan",
    "inf",
    "-inf",
    "Infinity",
    "0x10",
    "9223372036854775807",
    "-9223372036854775808",
    "9223372036854775808",
    "99999999999999999999",
    "4294967295",
    "4294967296",
    "-4294967297",
    "106751991167",
    "-106751991168",
    "re:^ç",
    "re:(",
    "re:[",
    "re:\\",
    "re:.*",
    "re:",
    "re:^(run|ran)$",
    "re:a{1000}{1000}",
    "none",
    "yok",
    "any",
    "evet",
    "no",
    "hayır",
    "true",
    "A1",
    "a2",
    "B1",
    "C2",
    "X9",
    "TYT",
    "ayt",
    "en",
    "de",
    "en-US",
    "tr_TR",
    "x",
    "toolongtag",
    "go",
    "pick up",
    "Pick  Up",
    "ho*",
    "The Night Train*",
    "*S1E2",
    "youtube",
    "dQw4w9WgXcQ",
    "altyazı",
    "okuma",
    "anki",
    "manual",
    "içe_aktarma",
    "video",
    "klip",
    "tts",
    "özgün",
    "dosya",
    "var",
    "red",
    "kırmızı",
    "mor",
    "8",
    "7",
    "Dil",
    "Dil::İngilizce",
    "dil::*",
    "fiil",
    "fiil::*",
    "fiil::düzensiz",
    "new",
    "learn",
    "due",
    "suspended",
    "kenarda",
    "yarına",
    "flagged",
    "classic",
    "forward",
    "Classic",
    "öğreniyorum",
    "biliyorum",
    "yok say",
    "2025-01-01",
    "2025-13-01",
];

/// Characters of random values and raw queries.
const CHARS: &[char] = &[
    'a', 'b', 'e', 'i', 'n', 'o', 'r', 'O', 'R', 'A', 'N', 'D', 'T', '0', '1', '9', 'İ', 'ı', 'ş',
    'ğ', 'ç', 'ü', 'ö', 'Ş', 'I', '%', '_', '\\', '\'', '"', '*', '?', '[', ']', ' ', '-', ':',
    '<', '>', '=', '!', '(', ')', ',', '.', '/', '|', '^', '$', '&', ';', '\t',
];

const OPS: &[&str] = &[":", "=", "!=", "<", "<=", ">", ">="];

/// Every key alias the resolver knows (built-in, v1.4 language keys, the fixture's
/// classifications and fields), in several spellings, plus unknown ones.
fn keys() -> Vec<String> {
    let mut out: Vec<String> = BUILTIN_KEYS.iter().map(|k| (*k).to_owned()).collect();
    out.extend(
        [
            "durum",
            "status",
            "dil",
            "lang",
            "language",
            "kelime",
            "word",
            "karşılaşma",
            "karsilasma",
            "seen",
            "nereden",
            "from",
            "kaynak",
            "source",
            "ses",
            "audio",
            "görsel",
            "gorsel",
            "image",
            "sıklık",
            "siklik",
            "freq",
            "frequency",
            "cefr",
            "level",
            "seviye",
            "düzey",
            "duzey",
            "sınav",
            "sinav",
            "SINAV",
            "Sınav",
            "puan",
            "Puan",
            "zor",
            "ZOR",
            "Seviye",
            "LEVEL",
            "front",
            "back",
            "Front",
            "BACK",
            "DECK",
            "Deste",
            "TAG",
            "İS",
            "IS",
            "Is",
            "FLAG",
            "Re",
            "NID",
            "DURUM",
            "Dİl",
            "KELİME",
            "foo",
            "x_1",
            "123",
            "_",
            "ön",
            "arka",
        ]
        .map(str::to_owned),
    );
    // Canonical classification keys (`attr_<id>`) of the fixture's four classifications.
    for n in 0x9000u128..=0x9003 {
        out.push(attr_key(AttributeDefId(super::u(n))));
    }
    out
}

/// [`VALUES`] plus ids of the fixture's notes and cards.
fn values() -> Vec<String> {
    let mut out: Vec<String> = VALUES.iter().map(|v| (*v).to_owned()).collect();
    out.push(nid(1).to_string());
    out.push(format!("{},{}", nid(2), nid(3)));
    out.push(cid(4).to_string());
    out.push(format!("{}, ,{}", cid(5), cid(0xdead)));
    out.push(format!("{},x", nid(1)));
    out
}

// ------------------------------------------------------------------ seeds and worlds

fn env_u64(name: &str) -> Option<u64> {
    let v = std::env::var(name).ok()?;
    Some(
        v.trim()
            .parse()
            .unwrap_or_else(|_| panic!("{name}={v:?} is not a u64")),
    )
}

fn seed() -> u64 {
    env_u64("FC_FUZZ_SEED").unwrap_or(DEFAULT_SEED)
}

fn cases(default: u32) -> u32 {
    env_u64("FC_FUZZ_CASES").map_or(default, |n| u32::try_from(n).unwrap_or(u32::MAX))
}

/// One small random collection.
#[derive(Debug, Clone, Copy)]
struct Spec {
    seed: u64,
    notes: u128,
    /// `None`: no language data; `Some(with_freq)`: [`with_lang`].
    lang: Option<bool>,
}

fn specs(seed: u64, n: usize) -> Vec<Spec> {
    let mut rng = SplitMix64::new(seed);
    (0..n)
        .map(|i| Spec {
            seed: rng.next_u64(),
            notes: 20 + u128::from(rng.next_u64() % 61),
            lang: [None, Some(true), Some(false)][i % 3],
        })
        .collect()
}

fn build(s: Spec) -> World {
    let words: Vec<&str> = WORDS.iter().chain(SPECIAL_WORDS).copied().collect();
    let w = world_with(s.notes, s.seed, &words);
    match s.lang {
        None => w,
        Some(with_freq) => with_lang(w, s.seed, with_freq),
    }
}

fn runner(seed: u64, world: usize, cases: u32) -> TestRunner {
    let mut rng = SplitMix64::new(seed ^ (world as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    let mut key = [0u8; 32];
    for chunk in key.chunks_mut(8) {
        chunk.copy_from_slice(&rng.next_u64().to_le_bytes());
    }
    TestRunner::new_with_rng(
        Config {
            cases,
            failure_persistence: None,
            max_shrink_iters: 4096,
            ..Config::default()
        },
        TestRng::from_seed(RngAlgorithm::ChaCha, &key),
    )
}

/// Runs `strategy` against [`check`] over `n_worlds` random worlds, `total` cases in all.
fn fuzz<S>(name: &str, n_worlds: usize, total: u32, strategy: S)
where
    S: Strategy<Value = String>,
{
    let seed = seed();
    let per_world = total.div_ceil(n_worlds as u32).max(1);
    for (i, spec) in specs(seed, n_worlds).into_iter().enumerate() {
        let w = build(spec);
        let all = w.actual(&Filter::All);
        match runner(seed, i, per_world).run(&strategy, |q| check(&w, &all, &q)) {
            Ok(()) => {}
            Err(TestError::Fail(why, q)) => panic!(
                "{name}: FC_FUZZ_SEED={seed} world #{i} {spec:?}\n  minimal query: {q:?}\n  {why}"
            ),
            Err(TestError::Abort(why)) => {
                panic!("{name}: FC_FUZZ_SEED={seed} world #{i} {spec:?}: aborted: {why}")
            }
        }
    }
}

// ------------------------------------------------------------------ the property

/// A query that compiles agrees with the reference everywhere the store answers it; one that
/// does not is a positioned error.
fn check(w: &World, all: &[CardId], q: &str) -> Result<(), TestCaseError> {
    let f = match w.try_filter(q) {
        Ok(f) => f,
        Err(e) => {
            let len = u32::try_from(q.chars().count()).unwrap_or(u32::MAX);
            prop_assert!(
                e.span.start <= e.span.end && e.span.end <= len,
                "error {e} outside the query ({len} chars)"
            );
            // A syntax error is reported as such, by the parser.
            if let Err(p) = fc_core::query::parse_query(q) {
                prop_assert_eq!(&e, &p);
            } else {
                prop_assert!(
                    !matches!(
                        e.kind,
                        QueryErrorKind::UnclosedQuote
                            | QueryErrorKind::UnclosedParen
                            | QueryErrorKind::UnmatchedParen
                            | QueryErrorKind::ExpectedTerm
                    ),
                    "syntax error {e} from the resolver"
                );
            }
            return Ok(());
        }
    };
    let expected = w.expected(&f);
    let fail = |what: &str, e: crate::error::StorageError| {
        TestCaseError::fail(format!("{what} errored: {e} (filter {f:?})"))
    };
    let actual =
        w.fx.read(|r| r.search_card_ids(&f, NOW))
            .map_err(|e| fail("search_card_ids", e))?;
    prop_assert_eq!(&actual, &expected, "list (filter {:?})", f);
    let probed =
        w.fx.read(|r| r.search_card_ids_among(&f, NOW, all))
            .map_err(|e| fail("search_card_ids_among", e))?;
    prop_assert_eq!(&probed, &expected, "correlated probe (filter {:?})", f);
    let page =
        w.fx.read(|r| {
            r.search_cards(&CardSearch {
                filter: f.clone(),
                now: NOW,
                sort: CardSort::SortField,
                descending: false,
                notes_only: false,
                offset: 0,
                limit: 10_000,
                after: None,
            })
        })
        .map_err(|e| fail("search_cards", e))?;
    prop_assert_eq!(
        page.total as usize,
        expected.len(),
        "count (filter {:?})",
        f
    );
    let mut listed: Vec<CardId> = page.hits.iter().map(|h| h.card.id).collect();
    listed.sort();
    let mut want = expected.clone();
    want.sort();
    prop_assert_eq!(listed, want, "page (filter {:?})", f);
    Ok(())
}

// ------------------------------------------------------------------ strategies

fn combine(atom: BoxedStrategy<String>) -> impl Strategy<Value = String> {
    atom.prop_recursive(3, 12, 3, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 2..4).prop_map(|v| format!("({})", v.join(" "))),
            prop::collection::vec(inner.clone(), 2..4)
                .prop_map(|v| format!("({})", v.join(" OR "))),
            inner.clone().prop_map(|q| format!("-{q}")),
            inner.prop_map(|q| format!("NOT {q}")),
        ]
    })
}

fn known_atom() -> BoxedStrategy<String> {
    let all: Vec<&'static str> = ATOMS.iter().chain(LANG_ATOMS).copied().collect();
    prop::sample::select(all).prop_map(str::to_owned).boxed()
}

fn value() -> BoxedStrategy<String> {
    prop_oneof![
        6 => prop::sample::select(values()),
        3 => prop::collection::vec(prop::sample::select(CHARS), 0..10)
            .prop_map(|cs| cs.into_iter().collect::<String>()),
        2 => (prop::sample::select(values()), prop::sample::select(values()))
            .prop_map(|(a, b)| format!("{a}{b}")),
        1 => (prop::sample::select(&["a", "ş", "*", "%_", "kılıç ", "9"][..]), 50..2000usize)
            .prop_map(|(s, n)| s.repeat(n)),
    ]
    .boxed()
}

/// `value` as query text: bare (as typed, may split or fail) or quoted (escaped).
fn render(value: &str, quoted: bool) -> String {
    if quoted {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_owned()
    }
}

fn generated_atom() -> BoxedStrategy<String> {
    prop_oneof![
        6 => (prop::sample::select(keys()), prop::sample::select(OPS), value(), any::<bool>())
            .prop_map(|(k, op, v, q)| format!("{k}{op}{}", render(&v, q))),
        2 => (value(), any::<bool>()).prop_map(|(v, q)| render(&v, q)),
        1 => known_atom(),
    ]
    .boxed()
}

fn raw_query() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(CHARS), 0..40)
        .prop_map(|cs| cs.into_iter().collect::<String>())
}

// ------------------------------------------------------------------ tests

/// Brief item 4.1: combinations of every known atom, v1.4 language atoms included, over six
/// random worlds (two each: no word data, word data with and without the frequency pack).
#[test]
fn known_atom_combinations_agree_over_random_worlds() {
    fuzz(
        "known_atom_combinations",
        6,
        cases(1500),
        combine(known_atom()),
    );
}

/// Brief item 4.2: generated atoms (every key alias × every operator × hostile values) and
/// raw query text: compiles → agrees, else a positioned error.
#[test]
fn generated_queries_never_error_and_agree() {
    fuzz(
        "generated_queries",
        6,
        cases(3000),
        prop_oneof![4 => combine(generated_atom()), 1 => raw_query()],
    );
}

// ------------------------------------------------------------------ regressions

/// Found by `generated_queries_never_error_and_agree` (v1.4.1): a long pasted text is one
/// `AND` of as many terms, and SQL chained them flat — `Expression tree is too large (maximum
/// depth 1000)` from about a thousand words (the correlated probe of automations first). The
/// compiler now nests `AND`/`OR` lists as balanced trees.
#[test]
fn regression_a_thousand_word_query_is_not_too_deep() {
    let w = build(specs(DEFAULT_SEED, 1)[0]);
    let all = w.actual(&Filter::All);
    for q in [
        format!("NOT {}", "kılıç ".repeat(992)),
        "kılıç ".repeat(3000),
        ["apple"; 3000].join(" OR "),
        format!("-({})", ["tag:fiil -is:new"; 1500].join(" OR ")),
    ] {
        if let Err(e) = check(&w, &all, &q) {
            panic!("{}…: {e}", &q[..40]);
        }
    }
}

/// Found with the fuzzer's probes (v1.4.1): 1000 nested `(` overflowed the stack of a 2 MB
/// thread in the parser (a process abort, not an error). Nesting is capped
/// (`fc_core::query::parse::MAX_DEPTH`); the deepest allowed query runs in SQL.
#[test]
fn regression_deep_nesting_is_an_error_and_the_deepest_allowed_runs() {
    use fc_core::query::parse::MAX_DEPTH;
    let w = build(specs(DEFAULT_SEED, 1)[0]);
    let all = w.actual(&Filter::All);
    let too_deep = format!("{}x{}", "(".repeat(100_000), ")".repeat(100_000));
    let e = std::thread::spawn(move || fc_core::query::parse_query(&too_deep).unwrap_err())
        .join()
        .unwrap();
    assert_eq!(e.kind, QueryErrorKind::TooDeep);
    // Alternating AND/OR/NOT at the limit, wide at every level.
    let mut q = "kilic".to_owned();
    for i in 0..MAX_DEPTH / 2 {
        let wide = ["tag:fiil", "-is:new", "apple"][..1 + i % 3].join(" ");
        q = if i % 2 == 0 {
            format!("-({q} OR {wide} OR {})", ["flag:1"; 40].join(" OR "))
        } else {
            format!("({q} {wide} {})", ["-is:due"; 40].join(" "))
        };
    }
    if let Err(e) = check(&w, &all, &q) {
        panic!("{q}: {e}");
    }
}

/// Found by `generated_queries_never_error_and_agree` (seed 3, v1.4.1): `kaynak:""` and
/// `kaynak:*` matched notes whose origin row has an empty source, site or video id (an Anki
/// import, a reading note) in SQL; a note's sources are its non-empty ones (`LangFacts::source`).
#[test]
fn regression_a_source_glob_never_matches_an_empty_source() {
    let w = build(Spec {
        seed: 11_307_387_092_600_937_729,
        notes: 43,
        lang: Some(true),
    });
    let all = w.actual(&Filter::All);
    for q in [
        "NOT -kaynak:\"\"",
        "kaynak:*",
        "source:**",
        "-kaynak:*",
        "kaynak:* nereden:anki",
    ] {
        if let Err(e) = check(&w, &all, q) {
            panic!("{q}: {e}");
        }
    }
    assert!(w
        .actual(&w.try_filter("kaynak:* nereden:anki").unwrap())
        .is_empty());
}

/// Found by `generated_queries_never_error_and_agree` (seeds 1, 2, 4, v1.4.1): a day count
/// whose milliseconds saturate to `i64::MIN` (`added:-9223372036854775808`,
/// `added:-106751991168`) panicked negating it ("attempt to negate with overflow").
#[test]
fn regression_huge_day_counts_do_not_overflow() {
    let w = build(specs(DEFAULT_SEED, 1)[0]);
    let all = w.actual(&Filter::All);
    for key in ["added", "edited", "reviewed", "due"] {
        for n in [
            "-9223372036854775808",
            "-106751991168",
            "9223372036854775807",
            "106751991167",
        ] {
            let q = format!("({key}:{n} OR deck:%)");
            if let Err(e) = check(&w, &all, &q) {
                panic!("{q}: {e}");
            }
        }
    }
}
