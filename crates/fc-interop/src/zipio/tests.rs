//! Crafted zip/zstd bombs: every reader stops at the decompression budget instead of
//! inflating (or pre-allocating) what the archive claims.

use super::*;
use crate::{anki, fcbackup, fcpack};

const MIB: usize = 1024 * 1024;

/// Streams `mib` MiB of zeros into a writer, 1 MiB at a time (never held whole).
fn zeros(w: &mut impl std::io::Write, mib: usize) {
    let chunk = vec![0u8; MIB];
    for _ in 0..mib {
        w.write_all(&chunk).unwrap();
    }
}

/// A zip whose entries are `(name, small bytes)` or `(name, MiB of zeros, deflated)`.
enum Entry<'a> {
    Bytes(&'a str, &'a [u8]),
    Zeros(&'a str, usize),
}

fn zip(entries: &[Entry<'_>]) -> Vec<u8> {
    let mut z = ZipWriter::new(Cursor::new(Vec::new()));
    let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for e in entries {
        match e {
            Entry::Bytes(name, b) => {
                z.start_file(*name, deflate).unwrap();
                z.write_all(b).unwrap();
            }
            Entry::Zeros(name, mib) => {
                z.start_file(*name, deflate).unwrap();
                zeros(&mut z, *mib);
            }
        }
    }
    z.finish().unwrap().into_inner()
}

fn is_limit(e: &InteropError) -> bool {
    matches!(e, InteropError::Invalid { reason, .. } if reason == TOO_LARGE)
}

#[test]
fn deflate_bomb_in_an_apkg_stops_at_the_budget() {
    let bomb = zip(&[Entry::Zeros("collection.anki2", 100)]);
    assert!(bomb.len() < MIB, "the bomb is small: {}", bomb.len());
    let e = anki::read_package(&bomb).unwrap_err();
    assert!(is_limit(&e), "{e}");
    // A smaller explicit budget stops even earlier.
    let mut z = ZipIn::open_with_budget(&bomb, "apkg", MIB as u64).unwrap();
    assert!(is_limit(&z.read("collection.anki2").unwrap_err()));
}

#[test]
fn zstd_bomb_in_a_latest_apkg_stops_at_the_budget() {
    let mut enc = zstd::stream::write::Encoder::new(Vec::new(), 3).unwrap();
    zeros(&mut enc, 160);
    let payload = enc.finish().unwrap();
    assert!(payload.len() < MIB);
    let bomb = zip(&[Entry::Bytes("collection.anki21b", &payload)]);
    let e = anki::read_package(&bomb).unwrap_err();
    assert!(is_limit(&e), "{e}");
}

#[test]
fn many_entries_share_one_total_budget() {
    // Each entry is under the per-entry cap; together they exceed the 64 MiB minimum budget.
    // Valid names (sha256 of the content), so only the budget can stop the read.
    let names: Vec<String> = [30, 31, 32]
        .iter()
        .map(|mib| format!("media/{}", crate::media::sha256_hex(&vec![0u8; mib * MIB])))
        .collect();
    let bomb = zip(&[
        Entry::Bytes("collection.anki2", b"x"),
        Entry::Zeros(&names[0], 30),
        Entry::Zeros(&names[1], 31),
        Entry::Zeros(&names[2], 32),
    ]);
    // Each entry alone fits.
    let mut z = ZipIn::open(&bomb, "apkg").unwrap();
    assert_eq!(z.read(&names[2]).unwrap().unwrap().len(), 32 * MIB);
    let mut z = ZipIn::open(&bomb, "apkg").unwrap();
    z.read(&names[0]).unwrap();
    z.read(&names[1]).unwrap();
    let e = z.read(&names[2]).unwrap_err();
    assert!(is_limit(&e), "{e}");
}

#[test]
fn own_backups_have_no_total_ceiling() {
    // v1.4 (deliberate change): the same 93 MiB of media in an `.fcbackup` — over the shared
    // budget above — reads back, entry by entry: what the app can write it can read. Its
    // guards are per entry (ratio, declared size, CRC, hash; see `fcbackup`).
    let manifest = serde_json::json!({
        "format": "fcbackup", "version": 1, "created_at": 1,
        "schema_versions": [1], "counts": {}, "media": 3
    });
    let manifest = serde_json::to_vec(&manifest).unwrap();
    let names: Vec<String> = [30, 31, 32]
        .iter()
        .map(|mib| format!("media/{}", crate::media::sha256_hex(&vec![0u8; mib * MIB])))
        .collect();
    let archive = zip(&[
        Entry::Bytes("manifest.json", &manifest),
        Entry::Bytes("collection.json", b"{}"),
        Entry::Zeros(&names[0], 30),
        Entry::Zeros(&names[1], 31),
        Entry::Zeros(&names[2], 32),
    ]);
    let mut r = fcbackup::BackupReader::open(Cursor::new(&archive)).unwrap();
    r.check(|_, _| Ok(())).unwrap();
    assert_eq!(r.media_list().len(), 3);
}

#[test]
fn fcpack_data_bomb_stops_at_the_budget() {
    let pack = crate::pack::Pack::default();
    let manifest = fcpack::Manifest::for_pack(&pack, "x", false, fc_core::Timestamp(1));
    let manifest = serde_json::to_vec(&manifest).unwrap();
    let bomb = zip(&[
        Entry::Bytes("manifest.json", &manifest),
        Entry::Zeros("pack.json", 100),
    ]);
    let e = fcpack::read(&bomb).unwrap_err();
    assert!(is_limit(&e), "{e}");
}

/// Overwrites the little-endian u32 at `at`.
fn put_u32(bytes: &mut [u8], at: usize, v: u32) {
    bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// Patches the uncompressed size every header of `bytes` claims.
fn claim_size(bytes: &mut [u8], size: u32) {
    let mut patched = 0;
    for i in 0..bytes.len().saturating_sub(30) {
        match bytes[i..i + 4] {
            [0x50, 0x4b, 0x03, 0x04] => put_u32(bytes, i + 22, size), // local header
            [0x50, 0x4b, 0x01, 0x02] => put_u32(bytes, i + 24, size), // central directory
            _ => continue,
        }
        patched += 1;
    }
    assert_eq!(patched, 2);
}

#[test]
fn lying_size_headers_neither_preallocate_nor_bypass_the_budget() {
    // Claims ~4 GiB for 5 bytes: read without allocating what is claimed.
    let mut z = ZipOut::new("t");
    z.add("a", b"hello", false).unwrap();
    let mut lying = z.finish().unwrap();
    claim_size(&mut lying, u32::MAX - 1);
    let mut zin = ZipIn::open(&lying, "t").unwrap();
    match zin.read("a") {
        Ok(Some(b)) => {
            assert_eq!(b, b"hello");
            assert!(b.capacity() <= MAX_PREALLOC as usize);
        }
        Ok(None) => panic!("entry vanished"),
        Err(e) => assert!(matches!(e, InteropError::Invalid { .. })),
    }

    // Claims 1 byte for 100 MiB: the budget counts what is produced, not what is claimed.
    let mut bomb = zip(&[Entry::Zeros("collection.anki2", 100)]);
    claim_size(&mut bomb, 1);
    let e = anki::read_package(&bomb).unwrap_err();
    assert!(matches!(e, InteropError::Invalid { .. }), "{e}");
    let mut zin = ZipIn::open_with_budget(&bomb, "apkg", MIB as u64).unwrap();
    assert!(zin.read("collection.anki2").is_err());
}

#[test]
fn budget_scales_with_input_and_is_clamped() {
    assert_eq!(default_budget(0), MIN_BUDGET);
    assert_eq!(default_budget(MIB), 200 * MIB as u64);
    assert_eq!(default_budget(usize::MAX), MAX_BUDGET);
    // The ceiling is 1 GiB (docs/10 §14), reached from ~5 MiB of input on; no entry may
    // exceed it either.
    assert_eq!(MAX_BUDGET, 1024 * 1024 * 1024);
    assert_eq!(default_budget(64 * MIB), MAX_BUDGET);
    assert_eq!(default_budget(1024 * MIB), MAX_BUDGET);
    assert_eq!(MAX_ENTRY, MAX_BUDGET);
    // Legitimate packages are untouched.
    let col = anki::fixtures::sample();
    for bytes in [
        anki::fixtures::legacy2_package(&col),
        anki::fixtures::latest_package(&col),
    ] {
        anki::read_package(&bytes).unwrap();
    }
}
