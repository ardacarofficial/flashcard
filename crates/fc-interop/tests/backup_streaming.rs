//! `.fcbackup` streams both ways (v1.4): writing and reading a backup far larger than any
//! single file keeps memory bounded by the largest file, not by the archive.
//!
//! The "1 GiB" of the brief is scaled down: the archive here is ~96 MiB — above the 64 MiB
//! floor of the budget foreign packages get (`zipio`), which the pre-v1.4 in-memory reader
//! applied to backups too — while the heap never holds more than a few files' worth. A real
//! 1 GiB+ archive differs only in the loop count. Heap use is measured with a counting global
//! peak resident size (Linux `VmHWM`, reset through `/proc/self/clear_refs`; its own test
//! binary, so nothing else runs beside it). Elsewhere the round trip still runs, unmeasured.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use fc_core::Timestamp;
use fc_interop::fcbackup::{BackupHeader, BackupReader, BackupWriter};
use fc_interop::media::sha256_hex;
use std::io::{BufReader, BufWriter};

const MIB: usize = 1024 * 1024;
/// Media files written (FILE_MIB each).
const FILES: usize = 64;
const FILE_MIB: usize = 2;

/// A `/proc/self/status` field in bytes (`VmRSS`, `VmHWM`).
fn status(field: &str) -> Option<usize> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with(field))?;
    let kib: usize = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

/// Peak resident growth while `f` runs (`None` where it cannot be measured).
fn peak_during<T>(f: impl FnOnce() -> T) -> (T, Option<usize>) {
    let reset = std::fs::write("/proc/self/clear_refs", "5").is_ok();
    let base = status("VmRSS");
    let out = f();
    let peak = status("VmHWM");
    let growth = match (reset, base, peak) {
        (true, Some(b), Some(p)) => Some(p.saturating_sub(b)),
        _ => None,
    };
    (out, growth)
}

/// Distinct, incompressible-ish content for file `i`.
fn content(i: usize) -> Vec<u8> {
    let mut x = (i as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    (0..FILE_MIB * MIB)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

#[test]
fn write_and_read_keep_memory_bounded_by_one_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.fcbackup");
    let dump = serde_json::json!({"schema_versions": [1], "tables": []});

    let (hashes, write_peak) = peak_during(|| {
        let file = BufWriter::new(std::fs::File::create(&path).unwrap());
        let mut w = BackupWriter::new(
            file,
            BackupHeader {
                created_at: Timestamp(1),
                schema_versions: vec![1],
                counts: Default::default(),
            },
        );
        w.write_collection(&dump).unwrap();
        let mut hashes = Vec::new();
        for i in 0..FILES {
            let bytes = content(i);
            let hash = sha256_hex(&bytes);
            assert!(w.add_media(&hash, &bytes).unwrap());
            hashes.push(hash);
        }
        let (file, manifest) = w.finish().unwrap();
        file.into_inner().unwrap().sync_all().unwrap();
        assert_eq!(manifest.media, FILES);
        hashes
    });
    let size = std::fs::metadata(&path).unwrap().len() as usize;
    let total = FILES * FILE_MIB * MIB;
    assert!(size > total, "archive {size} holds every byte");
    assert!(size > 64 * MIB, "above the old reader's floor budget");
    // One file in flight (+ the hasher's and zip's buffers): far below the archive.
    if let Some(peak) = write_peak {
        assert!(
            peak < 32 * MIB,
            "write peak {} MiB for a {} MiB archive",
            peak / MIB,
            size / MIB
        );
    }

    let (_, read_peak) = peak_during(|| {
        let file = BufReader::new(std::fs::File::open(&path).unwrap());
        let mut r = BackupReader::open(file).unwrap();
        let mut steps = 0;
        r.check(|_, _| {
            steps += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(steps, FILES);
        let back: serde_json::Value = r.collection().unwrap();
        assert_eq!(back, dump);
        let list = r.media_list();
        assert_eq!(list.len(), FILES);
        for m in &list {
            let bytes = r.read_media(m).unwrap();
            assert_eq!(bytes.len(), FILE_MIB * MIB);
            assert!(hashes.contains(&m.hash));
        }
    });
    if let Some(peak) = read_peak {
        assert!(
            peak < 32 * MIB,
            "read peak {} MiB for a {} MiB archive",
            peak / MIB,
            size / MIB
        );
    }
    eprintln!(
        "archive {} MiB, peaks: write {write_peak:?}, read {read_peak:?}",
        size / MIB
    );
}
