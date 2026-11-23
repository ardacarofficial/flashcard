//! v1.4.1 H2: a collection that does not open or fails its integrity check (deliberately
//! corrupted pages, a truncated file), the rescue (`rescue::rescue`), and failures at the file
//! layer ("disk full") on a real, tiny file system.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::rescue::{self, RescueMode};
use crate::App;
use fc_api::v1::oplog::Actor;
use fc_api::{ApiError, ErrorCode};
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::Timestamp;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn try_call(app: &App, method: &str, params: Value) -> Result<Value, ApiError> {
    app.invoke(&format!("v1.{method}"), params, &Actor::User)
}

fn call(app: &App, method: &str, params: Value) -> Value {
    try_call(app, method, params).unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

fn add_note(app: &App, deck: &Value, front: &str, back: &str) -> Result<Value, ApiError> {
    try_call(
        app,
        "notes.create",
        json!({"notetype_id": CLASSIC_ID, "deck_id": deck["id"],
               "fields": {"front": front, "back": back}, "tags": [], "attributes": []}),
    )
}

/// A 1×1 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

fn rows(app: &App, table: &str) -> u64 {
    let tx = app.store().read().unwrap();
    tx.dump_row_counts().unwrap().get(table).copied().unwrap_or(0)
}

/// Runs the queued jobs and returns the job's final state.
fn finish(app: &App, job: &Value) -> Value {
    app.run_pending_jobs().unwrap();
    call(app, "jobs.get", json!({"id": job["id"]}))
}

fn integrity_ok(app: &App) {
    let check = call(app, "maintenance.integrity.check", json!({"quick": false}));
    assert_eq!(check["ok"], true, "{check}");
}

/// A collection with `n` notes, one media file, and a verified backup in `backups/` (taken
/// after `backed_up` notes). Returns the db path and the hash of the media.
fn collection(dir: &Path, n: usize, backed_up: usize) -> (PathBuf, String) {
    let db = dir.join("collection.db");
    let app = App::open_sqlite(&db).unwrap();
    let deck = call(&app, "decks.create", json!({"path": "Kurtarma"}));
    let media = call(
        &app,
        "media.add",
        json!({"name": "a.png", "data_base64": PNG}),
    );
    for i in 0..n {
        if i == backed_up {
            let target = dir.join("backups").join("manual.fcbackup");
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            let job = call(&app, "io.backup.create", json!({"path": target.to_string_lossy()}));
            let done = finish(&app, &job);
            assert_eq!(done["status"], "succeeded", "{done}");
        }
        // Long enough that the notes fill many pages.
        add_note(&app, &deck, &format!("word {i} {}", "x".repeat(300)), &format!("meaning {i} <img src=\"{}\">", media["reference"].as_str().unwrap_or_default())).unwrap();
    }
    drop(app);
    (db, media["hash"].as_str().unwrap().to_owned())
}

/// Overwrites pages `from..to` (4 KiB) with garbage, leaving the header page alone.
fn corrupt_pages(db: &Path, from: u64, to: u64) {
    use std::io::{Seek, SeekFrom, Write};
    let mut f = std::fs::OpenOptions::new().write(true).open(db).unwrap();
    for page in from..to {
        f.seek(SeekFrom::Start(page * 4096 + 100)).unwrap();
        f.write_all(&[0xA5; 3000]).unwrap();
    }
    f.sync_all().unwrap();
}

fn file_bytes(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap()
}

#[test]
fn corrupted_pages_are_detected_kept_and_the_readable_rows_rescued() {
    let dir = tempfile::tempdir().unwrap();
    let (db, hash) = collection(dir.path(), 200, 0);
    assert!(!rescue::needs_rescue(&db), "a healthy file is not offered the rescue");
    let pages = std::fs::metadata(&db).unwrap().len() / 4096;
    corrupt_pages(&db, pages / 2, pages / 2 + 3);
    let damaged = file_bytes(&db);
    let health = rescue::check(&db).unwrap();
    assert!(health.opens && !health.ok, "{health:?}");
    assert!(rescue::needs_rescue(&db));

    let report = rescue::rescue(&db, RescueMode::Readable, Timestamp(1_700_000_000_000)).unwrap();
    // The original is kept byte for byte, beside its media.
    let kept = PathBuf::from(&report.kept_in);
    assert_eq!(file_bytes(&kept.join("collection.db")), damaged);
    assert!(kept.join("media").is_dir());
    assert!(kept.join("README.txt").is_file());
    assert!(report.copied_rows > 0, "{report:?}");

    // The new collection opens, is consistent and holds what could be read.
    let app = App::open_sqlite(&db).unwrap();
    integrity_ok(&app);
    let notes = rows(&app, "notes");
    assert!(notes > 0 && notes <= 200, "{notes}");
    assert_eq!(rows(&app, "cards"), notes, "every surviving note kept its card, no orphan card");
    let health = call(&app, "browse.check_database", json!({}));
    assert_eq!(health["orphan_cards"], json!([]), "{health}");
    // The media file the rows name came along.
    if rows(&app, "media") > 0 {
        assert!(fc_storage::media::media_file_path(&db, &hash).unwrap().is_file());
    }
    // And it is usable.
    let deck = call(&app, "decks.create", json!({"path": "Sonra"}));
    add_note(&app, &deck, "after", "rescue").unwrap();
}

#[test]
fn a_truncated_file_is_replaced_by_the_newest_verified_backup() {
    let dir = tempfile::tempdir().unwrap();
    let (db, hash) = collection(dir.path(), 60, 40);
    // A newer but broken "backup" must be skipped: only a copy that verifies now is used.
    std::fs::write(dir.path().join("backups").join("zz-newer.fcbackup"), b"PK\x03\x04 broken").unwrap();
    let len = std::fs::metadata(&db).unwrap().len();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&db)
        .unwrap()
        .set_len(len * 2 / 5)
        .unwrap();
    let damaged = file_bytes(&db);
    assert!(rescue::needs_rescue(&db));
    // The app itself does not open it silently into an empty collection.
    let opened = App::open_sqlite(&db);
    if let Ok(app) = &opened {
        let check = call(app, "maintenance.integrity.check", json!({"quick": false}));
        assert_eq!(check["ok"], false, "a truncated file must not pass: {check}");
    }
    drop(opened);

    let report = rescue::rescue(&db, RescueMode::Backup, Timestamp(1_700_000_000_000)).unwrap();
    assert!(report.backup.as_deref().unwrap().ends_with("manual.fcbackup"), "{report:?}");
    assert_eq!(file_bytes(&PathBuf::from(&report.kept_in).join("collection.db")), damaged);
    let app = App::open_sqlite(&db).unwrap();
    integrity_ok(&app);
    assert_eq!(rows(&app, "notes"), 40, "the notes of the backup are back");
    assert!(fc_storage::media::media_file_path(&db, &hash).unwrap().is_file());
}

#[test]
fn without_a_verified_backup_nothing_changes() {
    let dir = tempfile::tempdir().unwrap();
    let (db, _) = collection(dir.path(), 30, usize::MAX);
    let pages = std::fs::metadata(&db).unwrap().len() / 4096;
    corrupt_pages(&db, 2, pages.min(6));
    let before = file_bytes(&db);
    let e = rescue::rescue(&db, RescueMode::Backup, Timestamp(1)).unwrap_err().into_api();
    assert_eq!(e.code, ErrorCode::ConflictNoVerifiedBackup);
    assert_eq!(file_bytes(&db), before, "the damaged file is where it was, untouched");
    assert!(!dir.path().join(rescue::DAMAGED_DIR).exists());
    assert!(dir.path().join("media").is_dir());
}

// ------------------------------------------------------------------ disk full

#[test]
fn disk_full_errors_are_recognised_on_every_platform() {
    assert!(fc_storage::is_disk_full(&std::io::Error::from(std::io::ErrorKind::StorageFull)));
    let raw = if cfg!(windows) { 112 } else { 28 };
    assert!(fc_storage::is_disk_full(&std::io::Error::from_raw_os_error(raw)));
    assert!(!fc_storage::is_disk_full(&std::io::Error::from(std::io::ErrorKind::NotFound)));
    let e = crate::error::storage_to_api(fc_storage::StorageError::DiskFull { path: None });
    assert_eq!(e.code, ErrorCode::IoDiskFull);
    assert_eq!(e.message_key, "errors.io.disk_full");
}

/// A tiny real file system (Linux tmpfs; needs the right to mount, so it is skipped on
/// Windows/macOS and for unprivileged runs). Unmounted on drop.
struct Tmpfs(PathBuf);

impl Tmpfs {
    fn mount(at: &Path, size: &str) -> Option<Self> {
        if !cfg!(target_os = "linux") {
            return None;
        }
        std::fs::create_dir_all(at).ok()?;
        let ok = std::process::Command::new("mount")
            .args(["-t", "tmpfs", "-o", &format!("size={size}"), "tmpfs"])
            .arg(at)
            .output()
            .ok()?
            .status
            .success();
        ok.then(|| Self(at.to_path_buf()))
    }
}

impl Drop for Tmpfs {
    fn drop(&mut self) {
        let _ = std::process::Command::new("umount").arg(&self.0).output();
    }
}

/// Fills the file system at `dir` with a filler file until it is full; returns the filler.
fn fill_up(dir: &Path) -> PathBuf {
    use std::io::Write;
    let path = dir.join("filler.bin");
    let mut f = std::fs::File::create(&path).unwrap();
    let chunk = vec![0u8; 16 * 1024];
    while f.write_all(&chunk).is_ok() {}
    let _ = f.sync_all();
    path
}

fn code(e: &ApiError) -> ErrorCode {
    e.code
}

#[test]
fn disk_full_fails_the_command_cleanly_and_the_app_stays_usable() {
    let root = tempfile::tempdir().unwrap();
    let Some(data) = Tmpfs::mount(&root.path().join("data"), "64m") else {
        eprintln!("skipped: cannot mount a tmpfs here (needs Linux + the right to mount)");
        return;
    };
    let Some(out) = Tmpfs::mount(&root.path().join("out"), "64m") else {
        return;
    };
    let db = data.0.join("collection.db");
    let app = App::open_sqlite(&db).unwrap();
    let deck = call(&app, "decks.create", json!({"path": "Dolu"}));
    let mut acked = Vec::new();
    for i in 0..20 {
        acked.push(add_note(&app, &deck, &format!("w{i}"), "m").unwrap()["id"].clone());
    }

    // 1. A backup that does not fit: fails as "disk full", the previous good copy stays, no
    //    half file is left that could look like a backup.
    let target = out.0.join("copy.fcbackup");
    let first = finish(&app, &call(&app, "io.backup.create", json!({"path": target.to_string_lossy()})));
    assert_eq!(first["status"], "succeeded", "{first}");
    let good = file_bytes(&target);
    let out_filler = fill_up(&out.0);
    for i in 0..40 {
        acked.push(add_note(&app, &deck, &format!("more {i} {}", "y".repeat(2000)), "m").unwrap()["id"].clone());
    }
    let second = finish(&app, &call(&app, "io.backup.create", json!({"path": target.to_string_lossy()})));
    assert_eq!(second["status"], "failed", "{second}");
    assert_eq!(second["error"]["code"], "IO_DISK_FULL", "{second}");
    assert_eq!(file_bytes(&target), good, "the previous backup is untouched");
    let leftovers: Vec<_> = std::fs::read_dir(&out.0)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".part"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    drop(std::fs::remove_file(&out_filler));
    let verify = finish(&app, &call(&app, "io.backup.verify", json!({"path": target.to_string_lossy()})));
    assert_eq!(verify["status"], "succeeded", "{verify}");

    // 2. The collection's own disk fills up: database (WAL) and media writes fail cleanly.
    let filler = fill_up(&data.0);
    let mut db_error = None;
    for i in 0..400 {
        match add_note(&app, &deck, &format!("full {i} {}", "z".repeat(4000)), "m") {
            Ok(n) => acked.push(n["id"].clone()),
            Err(e) => {
                db_error = Some(e);
                break;
            }
        }
    }
    let db_error = db_error.expect("the disk filled up");
    assert_eq!(code(&db_error), ErrorCode::IoDiskFull, "{db_error:?}");
    assert_eq!(db_error.message_key, "errors.io.disk_full");
    let media = try_call(
        &app,
        "media.add",
        json!({"name": "big.png", "data_base64": base64_of(&big_png())}),
    )
    .unwrap_err();
    assert_eq!(code(&media), ErrorCode::IoDiskFull, "{media:?}");
    assert_eq!(rows(&app, "media"), 0, "no row for a file that was not written");
    let parts = walk(&data.0.join("media"))
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == "part"))
        .count();
    assert_eq!(parts, 0, "the partial media file is removed at once");
    // Still usable: reads work, and after freeing space writes work again.
    call(&app, "notes.get", json!({"id": acked.last().unwrap()}));
    std::fs::remove_file(&filler).unwrap();
    acked.push(add_note(&app, &deck, "after", "space").unwrap()["id"].clone());
    call(&app, "media.add", json!({"name": "ok.png", "data_base64": PNG}));
    drop(app);

    // 3. After a restart: consistent, every acknowledged note there, no partial media file.
    let app = App::open_sqlite(&db).unwrap();
    integrity_ok(&app);
    for id in &acked {
        call(&app, "notes.get", json!({"id": id}));
    }
    assert_eq!(rows(&app, "notes"), acked.len() as u64);
    let parts = walk(&data.0.join("media"))
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == "part"))
        .count();
    assert_eq!(parts, 0);
    drop(app);
    drop(out);
    drop(data);
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// A PNG-typed blob of 512 KiB (the 1×1 image followed by padding).
fn big_png() -> Vec<u8> {
    use base64::Engine;
    let mut b = base64::engine::general_purpose::STANDARD.decode(PNG).unwrap();
    b.resize(512 * 1024, 0);
    b
}
