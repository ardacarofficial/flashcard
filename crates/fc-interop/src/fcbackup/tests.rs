//! `.fcbackup` v2: round trip, v1 compatibility, missing/damaged/forged entries.

use super::*;

fn header() -> BackupHeader {
    BackupHeader {
        created_at: Timestamp(1),
        schema_versions: vec![1, 400],
        counts: [("notes".to_string(), 2)].into(),
    }
}

fn media(n: u8) -> (String, Vec<u8>) {
    let bytes = vec![n; 1000 + usize::from(n)];
    (sha256_hex(&bytes), bytes)
}

/// A zip from raw `(name, bytes, deflate)` entries (forged archives).
fn raw_zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut z = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes, deflate) in entries {
        let method = if *deflate {
            CompressionMethod::Deflated
        } else {
            CompressionMethod::Stored
        };
        z.start_file(
            *name,
            SimpleFileOptions::default().compression_method(method),
        )
        .unwrap();
        z.write_all(bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}

#[test]
fn roundtrips() {
    let (a, b) = (media(1), media(2));
    let (bytes, manifest) = write(
        header(),
        b"{\"tables\":[]}",
        &[b.clone(), a.clone(), a.clone()],
    )
    .unwrap();
    assert_eq!(manifest.version, VERSION);
    assert_eq!(manifest.media, 2, "a hash added twice is stored once");
    assert!(manifest.is_complete());
    assert_eq!(
        manifest.collection,
        Some(EntrySum {
            sha256: sha256_hex(b"{\"tables\":[]}"),
            size: 13
        })
    );
    let back = read(&bytes).unwrap();
    assert_eq!(back.manifest, manifest);
    assert_eq!(back.collection, b"{\"tables\":[]}");
    let mut want = vec![a, b];
    want.sort();
    assert_eq!(back.media, want);
    assert_eq!(read_manifest(&bytes).unwrap().media, 2);
    let mut r = BackupReader::open(Cursor::new(&bytes)).unwrap();
    r.check(|_, _| Ok(())).unwrap();
    let v: serde_json::Value = r.collection().unwrap();
    assert_eq!(v, serde_json::json!({"tables": []}));
}

#[test]
fn version_1_backups_still_read() {
    let (h, m) = media(7);
    let manifest = serde_json::json!({
        "format": "fcbackup", "version": 1, "created_at": 5,
        "schema_versions": [1], "counts": {"notes": 1}, "media": 2
    });
    let manifest = serde_json::to_vec(&manifest).unwrap();
    let name = format!("media/{h}");
    let bytes = raw_zip(&[
        ("manifest.json", &manifest, true),
        ("collection.json", b"{\"x\":1}", true),
        (&name, &m, true),
    ]);
    let b = read(&bytes).unwrap();
    assert_eq!(b.manifest.version, 1);
    assert_eq!(b.manifest.media, 2, "v1 counted rows, not files");
    assert!(b.manifest.media_files.is_empty());
    assert_eq!(b.media, vec![(h, m)]);
    assert_eq!(b.collection, b"{\"x\":1}");
}

#[test]
fn missing_media_is_listed_and_the_backup_is_incomplete() {
    let (a, b) = (media(1), media(2));
    let mut w = BackupWriter::new(Cursor::new(Vec::new()), header());
    w.write_collection(&serde_json::json!({})).unwrap();
    assert!(w.add_media(&a.0, &a.1).unwrap());
    // Bytes that no longer match their hash are not written either.
    assert!(!w.add_media(&b.0, b"changed on disk").unwrap());
    w.mark_missing(&"c".repeat(64));
    w.mark_missing(&a.0); // archived already: not missing
    let (out, manifest) = w.finish().unwrap();
    assert!(!manifest.is_complete());
    let mut missing = vec![b.0.clone(), "c".repeat(64)];
    missing.sort();
    assert_eq!(manifest.missing_media, missing);
    assert_eq!(manifest.media_files.len(), 1);
    // The rest restores.
    let back = read(&out.into_inner()).unwrap();
    assert_eq!(back.media, vec![a]);
    assert_eq!(back.manifest.missing_media, missing);
}

/// A v2 archive whose manifest is `manifest` and whose entries are `entries`.
fn forged(manifest: &Manifest, entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let m = serde_json::to_vec(manifest).unwrap();
    let mut all: Vec<(&str, &[u8], bool)> = vec![("manifest.json", &m, true)];
    all.extend_from_slice(entries);
    raw_zip(&all)
}

#[test]
fn a_listed_entry_missing_from_the_archive_is_refused() {
    let (a, b) = (media(1), media(2));
    let (bytes, manifest) = write(header(), b"{}", &[a.clone(), b]).unwrap();
    let name = format!("media/{}", a.0);
    // Same manifest, one media entry left out.
    let bad = forged(
        &manifest,
        &[("collection.json", b"{}", true), (&name, &a.1, false)],
    );
    let e = read(&bad).unwrap_err().to_string();
    assert!(e.contains("is missing"), "{e}");
    let mut r = BackupReader::open(Cursor::new(&bad)).unwrap();
    assert!(r.check(|_, _| Ok(())).is_err());
    // The collection is required as well.
    let bad = forged(&manifest, &[]);
    assert!(read(&bad)
        .unwrap_err()
        .to_string()
        .contains("collection.json is missing"));
    assert!(read(&bytes).is_ok());
}

/// Flips one byte of `needle`'s first occurrence.
fn flip(bytes: &mut [u8], needle: &[u8]) {
    let at = bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("needle");
    bytes[at + needle.len() / 2] ^= 0xff;
}

#[test]
fn a_corrupt_entry_is_refused() {
    let a = media(3);
    let (bytes, _) = write(header(), b"{}", std::slice::from_ref(&a)).unwrap();
    // Media are stored: their bytes are in the archive as is.
    let mut bad = bytes.clone();
    flip(&mut bad, &a.1[..200]);
    assert!(read(&bad).is_err());
    let mut r = BackupReader::open(Cursor::new(&bad)).unwrap();
    assert!(r.check(|_, _| Ok(())).is_err());
    // A media entry whose content does not match its name (CRC recomputed: a "valid" zip).
    let (_, manifest) = write(header(), b"{}", std::slice::from_ref(&a)).unwrap();
    let name = format!("media/{}", a.0);
    let other = vec![9u8; a.1.len()];
    let bad = forged(
        &manifest,
        &[("collection.json", b"{}", true), (&name, &other, false)],
    );
    assert!(read(&bad).unwrap_err().to_string().contains("damaged"));
    // A collection that does not match the manifest's hash.
    let bad = forged(
        &manifest,
        &[("collection.json", b"[]", true), (&name, &a.1, false)],
    );
    assert!(read(&bad).unwrap_err().to_string().contains("damaged"));
}

#[test]
fn sizes_must_match_the_manifest() {
    let a = media(4);
    let (_, mut manifest) = write(header(), b"{}", std::slice::from_ref(&a)).unwrap();
    manifest.media_files[0].size += 1;
    let name = format!("media/{}", a.0);
    let bad = forged(
        &manifest,
        &[("collection.json", b"{}", true), (&name, &a.1, false)],
    );
    assert!(read(&bad)
        .unwrap_err()
        .to_string()
        .contains("the manifest says"));
}

/// Overwrites the 32-bit uncompressed size the headers of entry `name` claim.
fn claim_size(bytes: &mut [u8], name: &str, size: u32) {
    let mut patched = 0;
    for i in 0..bytes.len().saturating_sub(46 + name.len()) {
        let at = match bytes[i..i + 4] {
            [0x50, 0x4b, 0x03, 0x04] if &bytes[i + 30..i + 30 + name.len()] == name.as_bytes() => {
                i + 22
            }
            [0x50, 0x4b, 0x01, 0x02] if &bytes[i + 46..i + 46 + name.len()] == name.as_bytes() => {
                i + 24
            }
            _ => continue,
        };
        bytes[at..at + 4].copy_from_slice(&size.to_le_bytes());
        patched += 1;
    }
    assert_eq!(patched, 2);
}

#[test]
fn a_header_claiming_an_impossible_ratio_is_refused_before_inflating() {
    // 8 MiB of zeros deflate to ~8 KiB (~1000:1: plausible, read fine).
    let manifest = serde_json::json!({
        "format": "fcbackup", "version": 1, "created_at": 5,
        "schema_versions": [1], "counts": {}, "media": 0
    });
    let manifest = serde_json::to_vec(&manifest).unwrap();
    let zeros = vec![0u8; 8 * 1024 * 1024];
    let ok = raw_zip(&[
        ("manifest.json", &manifest, true),
        ("collection.json", &zeros, true),
    ]);
    let mut r = BackupReader::open(Cursor::new(&ok)).unwrap();
    assert_eq!(r.collection_bytes().unwrap().len(), zeros.len());
    // The same entry claiming 400 MiB: refused from the header alone.
    let mut bad = ok.clone();
    claim_size(&mut bad, "collection.json", 400 * 1024 * 1024);
    let mut r = BackupReader::open(Cursor::new(&bad)).unwrap();
    let e = r.collection_bytes().unwrap_err().to_string();
    assert!(e.contains("implausible"), "{e}");
    // Claiming less than it holds: stops at the claim.
    let mut bad = ok;
    claim_size(&mut bad, "collection.json", 1024);
    let mut r = BackupReader::open(Cursor::new(&bad)).unwrap();
    assert!(r.collection_bytes().is_err());
}

#[test]
fn foreign_and_newer_files_are_refused() {
    assert!(read(b"not a zip")
        .unwrap_err()
        .to_string()
        .contains("not a zip"));
    let m = serde_json::json!({
        "format": "fcbackup", "version": 99, "created_at": 5,
        "schema_versions": [1], "counts": {}, "media": 0
    });
    let bad = raw_zip(&[("manifest.json", &serde_json::to_vec(&m).unwrap(), true)]);
    assert!(matches!(
        read_manifest(&bad),
        Err(InteropError::UnsupportedVersion { .. })
    ));
    let m = serde_json::json!({
        "format": "fcpack", "version": 1, "created_at": 5,
        "schema_versions": [1], "counts": {}, "media": 0
    });
    let bad = raw_zip(&[("manifest.json", &serde_json::to_vec(&m).unwrap(), true)]);
    assert!(read_manifest(&bad).is_err());
}
