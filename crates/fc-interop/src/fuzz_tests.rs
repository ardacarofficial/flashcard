//! v1.4.1 (H3): random and broken input for every reader of this crate — Anki packages,
//! `.fcpack`, `.fcbackup`, delimited text, JSON, subtitles. Properties:
//!
//! * never a panic, whatever the bytes;
//! * a damaged archive is refused or read back *exactly* (never silently different);
//! * a file cut at any zip section boundary is refused;
//! * bounded time and memory (bombs are refused by the budgets, not by running out).
//!
//! Deterministic: the seed is fixed (`FC_FUZZ_SEED` overrides it, `FC_FUZZ_CASES` sets the
//! number of cases for a longer soak); a failure prints the seed and the minimal input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::anki::{self, fixtures, hostile};
use crate::fcbackup::{self, BackupHeader};
use crate::media::sha256_hex;
use crate::pack::{MediaFile, Pack};
use crate::text::{self, TextOptions};
use crate::{fcpack, json, subtitle};
use fc_core::Timestamp;
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use std::time::{Duration, Instant};

/// Default seed of every fuzz test in this crate.
const SEED: u64 = 0x4833_f0cc_2026_1123;

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

/// Runs `test` over `cases` inputs of `strategy` with a deterministic RNG; panics with the
/// seed and proptest's minimal failing input.
fn fuzz<S: Strategy>(
    name: &str,
    cases: u32,
    strategy: S,
    test: impl Fn(S::Value) -> Result<(), TestCaseError>,
) {
    let seed = env_u64("FC_FUZZ_SEED").unwrap_or(SEED);
    let cases = env_u64("FC_FUZZ_CASES").map_or(cases, |n| n as u32);
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&seed.to_le_bytes());
    let config = Config {
        cases,
        failure_persistence: None,
        ..Config::default()
    };
    let mut runner =
        TestRunner::new_with_rng(config, TestRng::from_seed(RngAlgorithm::ChaCha, &key));
    if let Err(e) = runner.run(&strategy, test) {
        panic!("{name} failed (FC_FUZZ_SEED={seed}, {cases} cases): {e}");
    }
}

// ------------------------------------------------------------------ byte mutations

/// One change to a file.
#[derive(Debug, Clone)]
enum Mutation {
    Flip(Index, u8),
    Truncate(Index),
    Overwrite(Index, Vec<u8>),
    Insert(Index, Vec<u8>),
    Delete(Index, usize),
}

fn mutation() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        4 => (any::<Index>(), 0u8..8).prop_map(|(i, b)| Mutation::Flip(i, b)),
        1 => any::<Index>().prop_map(Mutation::Truncate),
        2 => (any::<Index>(), prop::collection::vec(any::<u8>(), 1..16))
            .prop_map(|(i, b)| Mutation::Overwrite(i, b)),
        1 => (any::<Index>(), prop::collection::vec(any::<u8>(), 1..16))
            .prop_map(|(i, b)| Mutation::Insert(i, b)),
        1 => (any::<Index>(), 1usize..64).prop_map(|(i, n)| Mutation::Delete(i, n)),
    ]
}

fn mutations() -> impl Strategy<Value = Vec<Mutation>> {
    prop::collection::vec(mutation(), 1..4)
}

fn mutate(bytes: &[u8], muts: &[Mutation]) -> Vec<u8> {
    let mut b = bytes.to_vec();
    for m in muts {
        if b.is_empty() {
            break;
        }
        match m {
            Mutation::Flip(i, bit) => {
                let at = i.index(b.len());
                b[at] ^= 1 << bit;
            }
            Mutation::Truncate(i) => b.truncate(i.index(b.len())),
            Mutation::Overwrite(i, new) => {
                let at = i.index(b.len());
                for (k, x) in new.iter().enumerate() {
                    if let Some(slot) = b.get_mut(at + k) {
                        *slot = *x;
                    }
                }
            }
            Mutation::Insert(i, new) => {
                let at = i.index(b.len() + 1);
                b.splice(at..at, new.iter().copied());
            }
            Mutation::Delete(i, n) => {
                let at = i.index(b.len());
                let end = (at + n).min(b.len());
                b.drain(at..end);
            }
        }
    }
    b
}

/// Offsets where a zip section starts (local headers, data descriptors, central directory
/// entries, end records), each with its neighbours, plus the ends of the file.
fn zip_boundaries(bytes: &[u8]) -> Vec<usize> {
    let sigs: [&[u8]; 5] = [
        b"PK\x03\x04",
        b"PK\x07\x08",
        b"PK\x01\x02",
        b"PK\x06\x06",
        b"PK\x05\x06",
    ];
    let mut out = vec![0, 1, bytes.len().saturating_sub(1)];
    for i in 0..bytes.len().saturating_sub(3) {
        if sigs.iter().any(|s| bytes[i..].starts_with(s)) {
            out.extend([i.saturating_sub(1), i, i + 1, i + 4]);
        }
    }
    out.retain(|&i| i < bytes.len());
    out.sort_unstable();
    out.dedup();
    out
}

/// Fails the case when one read takes longer than this (debug build, shared machine).
const SLOW: Duration = Duration::from_secs(5);

fn timed<T>(what: &str, f: impl FnOnce() -> T) -> Result<T, TestCaseError> {
    let t = Instant::now();
    let out = f();
    let took = t.elapsed();
    prop_assert!(took < SLOW, "{what} took {took:?}");
    Ok(out)
}

// ------------------------------------------------------------------ fixtures

fn backup_fixture() -> (Vec<u8>, fcbackup::Backup) {
    let media: Vec<(String, Vec<u8>)> = [b"ID3 audio".to_vec(), vec![7u8; 3000]]
        .into_iter()
        .map(|b| (sha256_hex(&b), b))
        .collect();
    let header = BackupHeader {
        created_at: Timestamp(1),
        schema_versions: vec![1, 1400],
        counts: [("notes".to_string(), 2)].into(),
    };
    let json = br#"{"tables":[{"name":"notes","rows":[["a","b"],["c","d"]]}]}"#;
    let (bytes, _) = fcbackup::write(header, json, &media).unwrap();
    let back = fcbackup::read(&bytes).unwrap();
    (bytes, back)
}

fn fcpack_fixture() -> (Vec<u8>, (fcpack::Manifest, Pack)) {
    let (pack, _) = anki::to_pack(&fixtures::sample(), fixtures::NOW);
    let mut pack = pack;
    pack.media.push(MediaFile {
        hash: sha256_hex(b"img"),
        mime: "image/png".into(),
        name: Some("a.png".into()),
        bytes: b"img".to_vec(),
    });
    let m = fcpack::Manifest::for_pack(&pack, "Deck", true, Timestamp(5));
    let bytes = fcpack::write(&pack, &m).unwrap();
    let back = fcpack::read(&bytes).unwrap();
    (bytes, back)
}

/// Reads an Anki package and converts it, as the import job does.
fn read_anki(bytes: &[u8]) -> Result<(), TestCaseError> {
    let col = timed("read_package", || anki::read_package(bytes))?;
    if let Ok(col) = col {
        timed("to_pack", || anki::to_pack(&col, fixtures::NOW))?;
    }
    Ok(())
}

// ------------------------------------------------------------------ packages

#[test]
fn damaged_anki_packages_never_panic() {
    let packages = [
        fixtures::legacy2_package(&fixtures::sample()),
        fixtures::latest_package(&fixtures::sample()),
    ];
    fuzz(
        "anki",
        160,
        (0..packages.len(), mutations()),
        |(which, muts)| read_anki(&mutate(&packages[which], &muts)),
    );
}

#[test]
fn anki_packages_cut_at_every_section_boundary_are_refused() {
    for pkg in [
        fixtures::legacy2_package(&fixtures::sample()),
        fixtures::latest_package(&fixtures::sample()),
    ] {
        for at in zip_boundaries(&pkg) {
            let r = anki::read_package(&pkg[..at]);
            assert!(r.is_err(), "cut at {at} of {} was read", pkg.len());
        }
    }
}

#[test]
fn hostile_anki_packages_are_refused_or_read_never_panic() {
    for case in hostile::cases() {
        let t = Instant::now();
        let r = std::panic::catch_unwind(|| {
            anki::read_package(&case.bytes).map(|c| anki::to_pack(&c, fixtures::NOW))
        });
        let took = t.elapsed();
        let r = r.unwrap_or_else(|_| panic!("{}: panicked", case.name));
        assert!(took < SLOW, "{}: took {took:?}", case.name);
        match r {
            Err(e) => {
                eprintln!("{}: refused: {e}", case.name);
                assert!(
                    !e.to_string().trim().is_empty(),
                    "{}: empty message",
                    case.name
                );
                assert!(case.read.is_none_or(|read| !read), "{}: refused", case.name);
            }
            Ok(_) => {
                eprintln!("{}: read", case.name);
                assert!(case.read.is_none_or(|read| read), "{}: was read", case.name);
            }
        }
    }
}

#[test]
fn damaged_fcpacks_are_refused_or_read_exactly() {
    let (bytes, original) = fcpack_fixture();
    fuzz("fcpack", 200, mutations(), |muts| {
        let m = mutate(&bytes, &muts);
        if let Ok(back) = timed("fcpack::read", || fcpack::read(&m))? {
            prop_assert_eq!(&back.1, &original.1, "a damaged pack read differently");
        }
        Ok(())
    });
    for at in zip_boundaries(&bytes) {
        assert!(fcpack::read(&bytes[..at]).is_err(), "cut at {at}");
    }
}

#[test]
fn damaged_backups_are_refused_or_read_exactly() {
    let (bytes, original) = backup_fixture();
    fuzz("fcbackup", 300, mutations(), |muts| {
        let m = mutate(&bytes, &muts);
        let whole = timed("fcbackup::read", || fcbackup::read(&m))?;
        // The streaming check the app runs agrees: it passes only on an intact backup.
        let checked = timed("fcbackup check", || {
            fcbackup::BackupReader::open(std::io::Cursor::new(&m)).and_then(|mut r| {
                r.check(|_, _| Ok(()))?;
                Ok(r.manifest().clone())
            })
        })?;
        if let Ok(back) = &whole {
            prop_assert_eq!(&back.collection, &original.collection);
            prop_assert_eq!(&back.media, &original.media);
            prop_assert_eq!(&back.manifest, &original.manifest);
        }
        if let Ok(manifest) = checked {
            prop_assert_eq!(&manifest, &original.manifest);
            prop_assert!(
                whole.is_ok(),
                "check passed, read failed: {:?}",
                whole.err()
            );
        }
        Ok(())
    });
}

#[test]
fn backups_cut_at_every_section_boundary_are_refused() {
    let (bytes, _) = backup_fixture();
    let cuts = zip_boundaries(&bytes);
    assert!(cuts.len() > 10, "{cuts:?}");
    for at in cuts {
        let cut = &bytes[..at];
        assert!(fcbackup::read(cut).is_err(), "cut at {at} was read");
        let checked = fcbackup::BackupReader::open(std::io::Cursor::new(cut))
            .and_then(|mut r| r.check(|_, _| Ok(())));
        assert!(checked.is_err(), "cut at {at} passed the check");
    }
}

/// A real deflate bomb (1 GiB of zeros in ~1 MiB) inside each package format: refused by
/// the budgets, quickly, without inflating it.
#[test]
fn zip_bombs_are_refused_quickly() {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    let bomb = |entries: &[(&str, &[u8])], big: &str, size: u64| {
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(size > u32::MAX as u64);
        for (name, bytes) in entries {
            z.start_file(*name, opts).unwrap();
            z.write_all(bytes).unwrap();
        }
        z.start_file(big, opts).unwrap();
        let chunk = vec![0u8; 1 << 20];
        let mut left = size;
        while left > 0 {
            let n = left.min(chunk.len() as u64) as usize;
            z.write_all(&chunk[..n]).unwrap();
            left -= n as u64;
        }
        z.finish().unwrap().into_inner()
    };
    // 300 MiB of zeros: over the budget of a ~300 KiB archive (64 MiB floor, ×200).
    let size = 300 << 20;
    let apkg = bomb(&[], "collection.anki21", size);
    let t = Instant::now();
    let e = anki::read_package(&apkg).unwrap_err();
    assert!(e.to_string().contains("size limit"), "{e}");
    let fcp = bomb(&[("manifest.json", b"{}")], "pack.json", size);
    assert!(fcpack::read(&fcp).is_err());
    // A backup entry honestly declaring 300 MiB of zeros is within deflate's ratio; the
    // manifest does not list it, so it is never inflated; the collection's declared size
    // must match the manifest.
    let (good, original) = backup_fixture();
    let _ = good;
    let manifest = serde_json::to_vec(&original.manifest).unwrap();
    let fcb = bomb(&[("manifest.json", &manifest)], "collection.json", size);
    let e = fcbackup::read(&fcb).unwrap_err();
    assert!(e.to_string().contains("manifest says"), "{e}");
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
}

// ------------------------------------------------------------------ text, JSON, subtitles

/// Text made of the characters that break delimited parsers.
fn nasty_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("\"".to_owned()),
        Just("\"\"".to_owned()),
        Just("\u{feff}".to_owned()),
        Just("\r\n".to_owned()),
        Just("\r".to_owned()),
        Just("\n".to_owned()),
        Just("\n\n".to_owned()),
        Just("\t".to_owned()),
        Just(",".to_owned()),
        Just(";".to_owned()),
        Just(" - ".to_owned()),
        Just("\u{0}".to_owned()),
        Just("İı".to_owned()),
        "[a-zçğöşü ]{0,6}",
        any::<String>().prop_map(|s| s.chars().take(4).collect()),
    ];
    prop::collection::vec(piece, 0..40).prop_map(|v| v.concat())
}

fn options() -> impl Strategy<Value = TextOptions> {
    (
        prop::sample::select(vec![",", "\t", ";", " - ", ">>", "|"]),
        prop::sample::select(vec![text::NEWLINE, text::BLANK_LINE, ";", "@@"]),
        any::<bool>(),
        prop::option::of(1usize..5),
    )
        .prop_map(|(f, r, quotes, max_fields)| TextOptions {
            field_separator: f.into(),
            record_separator: r.into(),
            quotes,
            max_fields,
        })
}

#[test]
fn delimited_text_never_panics_and_detection_parses() {
    fuzz("text", 400, (nasty_text(), options()), |(t, o)| {
        if o.validate().is_ok() {
            let _ = text::parse_delimited(&t, &o);
        }
        let d = text::detect(&t);
        let _ = text::parse_delimited(&t, &d.options);
        let _ = text::normalize_newlines(&t);
        Ok(())
    });
}

/// What the writer writes, the reader reads back (newlines compared normalized: the reader
/// turns `\r\n` and `\r` into `\n`).
#[test]
fn written_rows_parse_back() {
    let cell = prop_oneof![
        "[a-z\"\t,;|@\n\r ğİ-]{0,8}",
        Just("a\r\n\r\nb".to_owned()),
        Just(" -".to_owned()),
    ];
    let rows = prop::collection::vec(prop::collection::vec(cell, 1..4), 1..6);
    let opts = (
        prop::sample::select(vec![",", "\t", ";", " - ", "|"]),
        prop::sample::select(vec![text::NEWLINE, text::BLANK_LINE, "@@"]),
    )
        .prop_map(|(f, r)| TextOptions::new(f, r))
        .prop_filter("valid", |o| o.validate().is_ok());
    fuzz("text roundtrip", 400, (rows, opts), |(rows, o)| {
        let norm = |rows: &[Vec<String>]| -> Vec<Vec<String>> {
            rows.iter()
                .map(|r| r.iter().map(|c| text::normalize_newlines(c)).collect())
                .filter(|r: &Vec<String>| r.iter().any(|c| !c.trim().is_empty()))
                .collect()
        };
        let written = text::write_delimited(&rows, &o);
        let back = text::parse_delimited(&written, &o).unwrap();
        prop_assert_eq!(norm(&back), norm(&rows), "written: {:?}", written);
        Ok(())
    });
}

/// Regression (found by `written_rows_parse_back`): a lone `\r` inside a value was written
/// unquoted and read back as a record break; a quoted value after a custom record separator
/// kept its quotes.
#[test]
fn a_carriage_return_inside_a_value_survives_export_and_import() {
    for (rec, value) in [
        (text::NEWLINE, "ğ\rğ"),
        (text::BLANK_LINE, "a\r\rb"),
        (text::BLANK_LINE, "a\r\n\r\nb"),
        // Second regression: after a custom record separator the writer adds a newline, and
        // the quote that followed it was not seen.
        ("@@", " -"),
        // Third: a value ending in the start of the separator (`a@` + `@@`).
        ("@@", "a@"),
    ] {
        let o = TextOptions::new(" - ", text::BLANK_LINE);
        let rows = vec![vec!["a".to_owned()], vec![String::new(), "b".to_owned()]];
        let back = text::parse_delimited(&text::write_delimited(&rows, &o), &o).unwrap();
        assert_eq!(back, rows, "an empty first value before a blank separator");
        let o = TextOptions::new("\t", rec);
        let rows = vec![
            vec!["first".to_owned()],
            vec![value.to_owned(), "x".to_owned()],
        ];
        let back = text::parse_delimited(&text::write_delimited(&rows, &o), &o).unwrap();
        assert_eq!(back.len(), 2, "{value:?}: {back:?}");
        assert_eq!(back[1][0], text::normalize_newlines(value));
    }
}

#[test]
fn any_bytes_decode_to_text() {
    let bytes = prop_oneof![
        prop::collection::vec(any::<u8>(), 0..300),
        (
            prop::sample::select(vec![
                &b"\xef\xbb\xbf"[..],
                &b"\xff\xfe"[..],
                &b"\xfe\xff"[..]
            ]),
            prop::collection::vec(any::<u8>(), 0..300)
        )
            .prop_map(|(bom, rest)| [bom, &rest[..]].concat()),
    ];
    fuzz("decode", 400, bytes, |b| {
        let d = text::decode(&b);
        let _ = text::detect(&d.text);
        let _ = json::parse(&d.text);
        let _ = subtitle::parse_subtitles(&b);
        Ok(())
    });
}

/// A million columns on one line, and a million short lines: parsed in bounded time.
#[test]
fn a_million_columns_parse_in_bounded_time() {
    let wide = "a\t".repeat(1_000_000);
    let t = Instant::now();
    let rows = text::parse_delimited(&wide, &TextOptions::new("\t", text::NEWLINE)).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].len() >= 1_000_000);
    let d = text::detect(&wide[..200_000]);
    let _ = d;
    let tall = "a,b\r\n".repeat(1_000_000);
    let rows = text::parse_delimited(&tall, &TextOptions::new(",", text::NEWLINE)).unwrap();
    assert_eq!(rows.len(), 1_000_000);
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
}
