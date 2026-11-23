//! A collection that does not open or fails its integrity check (v1.4.1 H2).
//!
//! Rules: **nothing is deleted and nothing is repaired in place.** The damaged files — the
//! database, its `-wal`/`-shm` companions and the `media/` folder — are moved, as they are,
//! into `<data dir>/damaged/<time>/` (same layout as a collection folder, so the copy still
//! opens as a collection: its WAL and media keep their names), with a short bilingual
//! `README.txt`. Then a fresh collection is put in their place, one of two ways:
//!
//! * [`RescueMode::Backup`]: the newest backup that **verifies now** (restored into a scratch
//!   profile and read back: `io.backup.verify`'s check) among the automatic copies
//!   (`backups/auto/`), the other backups in `backups/` (safety copies of restores) and the
//!   last verified backup recorded in `backups/status.json` (wherever the user saved it);
//!   restored at the collection's place and read back once more.
//! * [`RescueMode::Readable`]: every row still readable from the damaged file is copied
//!   (`fc_storage::rescue::salvage_into`), then the media files those rows name are copied
//!   from the damaged folder.
//!
//! If the second step fails, the fresh files are removed and the damaged ones moved back:
//! the user is never left with less than before. Run while no process has the collection
//! open (the desktop app runs it before opening; `fc rescue` likewise).

use crate::error::{AppError, AppResult};
use crate::jobs::CancelToken;
use crate::modules::io::rescue_support as io;
use fc_api::v1::io::BackupVerifyReport;
use fc_api::{ApiError, ErrorCode};
use fc_core::Timestamp;
use fc_storage::rescue::{inspect_file, FileHealth, SalvageReport};
use fc_storage::{SqliteStore, Store};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Folder (in the data dir) the damaged files are moved into.
pub const DAMAGED_DIR: &str = "damaged";
/// The companions SQLite keeps next to a database file.
const COMPANIONS: [&str; 2] = ["-wal", "-shm"];

/// What to put in place of a damaged collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RescueMode {
    /// The newest backup that verifies.
    Backup,
    /// What is still readable in the damaged file.
    Readable,
}

/// Outcome of [`rescue`].
#[derive(Debug, Clone, Serialize)]
pub struct RescueReport {
    /// What was done.
    pub mode: RescueMode,
    /// Where the damaged files are now (kept).
    pub kept_in: String,
    /// The backup restored ([`RescueMode::Backup`]).
    pub backup: Option<String>,
    /// Its verification read back from the new collection.
    pub verify: Option<BackupVerifyReport>,
    /// Rows copied / lost ([`RescueMode::Readable`]).
    pub copied_rows: u64,
    /// Rows that could not be read.
    pub lost_rows: u64,
    /// Media files copied from the damaged folder.
    pub media_files: u64,
    /// Media the copied rows name whose file was not in the damaged folder.
    pub missing_media: Vec<String>,
}

/// The health of the collection file at `db` (read-only; see
/// [`fc_storage::rescue::inspect_file`]). A missing file is "not damaged" (a first start).
pub fn check(db: &Path) -> Option<FileHealth> {
    if !db.exists() {
        return None;
    }
    Some(inspect_file(db))
}

/// Whether a collection that failed to open should be offered the rescue: the file exists and
/// SQLite says it is damaged (a newer schema or a busy file is not damage).
pub fn needs_rescue(db: &Path) -> bool {
    check(db).is_some_and(|h| !h.ok)
}

/// Backups a [`RescueMode::Backup`] rescue may use, newest first: the automatic ones, the
/// other `.fcbackup` files in `backups/`, and the last verified one recorded in `status.json`.
pub fn backup_candidates(data_dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<(i64, PathBuf)> = Vec::new();
    for dir in [io::auto_dir(data_dir), io::backups_dir(data_dir)] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let path = e.path();
            if path.extension().is_some_and(|x| x == "fcbackup") && path.is_file() {
                found.push((modified_ms(&path), path));
            }
        }
    }
    if let Some(last) = io::last_verified_path(data_dir) {
        let path = PathBuf::from(last);
        if path.is_file() && !found.iter().any(|(_, p)| *p == path) {
            found.push((modified_ms(&path), path));
        }
    }
    found.sort_by(|a, b| b.cmp(a));
    found.into_iter().map(|(_, p)| p).collect()
}

fn modified_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The newest candidate that verifies now (restored into a scratch profile and read back).
pub fn newest_verified_backup(data_dir: &Path) -> Option<(PathBuf, BackupVerifyReport)> {
    let scratch = io::backups_dir(data_dir);
    let cancel = CancelToken::new();
    backup_candidates(data_dir).into_iter().find_map(|p| {
        let report = io::verify_file(&p, &scratch, &cancel).ok()?;
        report.ok.then_some((p, report))
    })
}

fn file_err(path: &Path, e: &std::io::Error) -> AppError {
    let shown = path.display().to_string();
    if fc_storage::is_disk_full(e) {
        return io::disk_full(&shown, &e.to_string());
    }
    AppError::Api(
        ApiError::new(ErrorCode::IoFile, format!("{shown}: {e}")).with_detail("path", shown),
    )
}

/// Moves the damaged collection at `db` (database, companions, `media/`) into a new folder
/// `<data dir>/damaged/<time>/`. Returns `(folder, moved)`: `moved` lists `(from, to)` so a
/// failed rescue can move them back.
fn quarantine(db: &Path, now: Timestamp) -> AppResult<(PathBuf, Vec<(PathBuf, PathBuf)>)> {
    let data_dir = db.parent().unwrap_or_else(|| Path::new("."));
    let name = db
        .file_name()
        .map_or_else(|| "collection.db".into(), |n| n.to_os_string());
    let stamp = fc_core::maintenance::file_stamp(now);
    let mut folder = data_dir.join(DAMAGED_DIR).join(&stamp);
    let mut n = 1;
    while folder.exists() {
        folder = data_dir.join(DAMAGED_DIR).join(format!("{stamp}-{n}"));
        n += 1;
    }
    std::fs::create_dir_all(&folder).map_err(|e| file_err(&folder, &e))?;
    let mut items = vec![(db.to_path_buf(), folder.join(&name))];
    for c in COMPANIONS {
        let mut from = name.clone();
        from.push(c);
        items.push((data_dir.join(&from), folder.join(&from)));
    }
    items.push((data_dir.join("media"), folder.join("media")));
    let mut moved = Vec::new();
    for (from, to) in items {
        if !from.exists() {
            continue;
        }
        if let Err(e) = std::fs::rename(&from, &to) {
            undo_moves(&moved);
            return Err(file_err(&from, &e));
        }
        moved.push((from, to));
    }
    let _ = std::fs::write(folder.join("README.txt"), README);
    Ok((folder, moved))
}

const README: &str = "\
Bu klasör, açılamayan ya da bütünlük denetiminden geçemeyen koleksiyonun dokunulmamış \
kopyasıdır. Flashcard onu silmez; ihtiyacın kalmadığından eminsen kendin silebilirsin.\n\n\
This folder holds the untouched copy of a collection that did not open or failed its \
integrity check. Flashcard never deletes it; remove it yourself once you are sure you do not \
need it.\n";

fn undo_moves(moved: &[(PathBuf, PathBuf)]) {
    for (from, to) in moved.iter().rev() {
        let _ = std::fs::rename(to, from);
    }
}

/// Removes a fresh collection written at `db` (database, companions, `media/`).
fn remove_fresh(db: &Path) {
    let data_dir = db.parent().unwrap_or_else(|| Path::new("."));
    let _ = std::fs::remove_file(db);
    if let Some(name) = db.file_name() {
        for c in COMPANIONS {
            let mut n = name.to_os_string();
            n.push(c);
            let _ = std::fs::remove_file(data_dir.join(n));
        }
    }
    let _ = std::fs::remove_dir_all(data_dir.join("media"));
}

/// Rescues the damaged collection at `db` (see the module docs). Run it while nothing has the
/// collection open.
pub fn rescue(db: &Path, mode: RescueMode, now: Timestamp) -> AppResult<RescueReport> {
    let data_dir = db.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    // Chosen before anything moves: no verified backup = nothing changes.
    let backup = match mode {
        RescueMode::Backup => Some(newest_verified_backup(&data_dir).ok_or_else(|| {
            AppError::Api(ApiError::new(
                ErrorCode::ConflictNoVerifiedBackup,
                "no backup verifies: nothing was changed",
            ))
        })?),
        RescueMode::Readable => None,
    };
    let (folder, moved) = quarantine(db, now)?;
    let mut report = RescueReport {
        mode,
        kept_in: folder.display().to_string(),
        backup: None,
        verify: None,
        copied_rows: 0,
        lost_rows: 0,
        media_files: 0,
        missing_media: Vec::new(),
    };
    let done = match backup {
        Some((path, _)) => restore_backup(&path, db, &mut report),
        None => salvage(&folder, db, &mut report),
    };
    match done {
        Ok(()) => {
            log::warn!(target: "fc_app::maintenance",
                "damaged collection rescued ({mode:?}); the damaged files are kept in {}",
                report.kept_in);
            Ok(report)
        }
        Err(e) => {
            remove_fresh(db);
            undo_moves(&moved);
            let _ = std::fs::remove_file(folder.join("README.txt"));
            let _ = std::fs::remove_dir(&folder);
            Err(e)
        }
    }
}

fn restore_backup(path: &Path, db: &Path, report: &mut RescueReport) -> AppResult<()> {
    let reader = io::open_backup(path)?;
    let mut verify = BackupVerifyReport {
        path: path.display().to_string(),
        ..BackupVerifyReport::default()
    };
    io::restore_into(reader, db, &CancelToken::new(), &mut verify)?;
    verify.ok = verify.problems.is_empty();
    if !verify.ok {
        return Err(AppError::api(
            ErrorCode::Internal,
            format!("the restored backup did not read back: {}", verify.problems.join("; ")),
        ));
    }
    report.media_files = u64::from(verify.media_files);
    report.missing_media = verify.missing_media.clone();
    report.backup = Some(path.display().to_string());
    report.verify = Some(verify);
    Ok(())
}

fn salvage(folder: &Path, db: &Path, report: &mut RescueReport) -> AppResult<()> {
    let name = db
        .file_name()
        .map_or_else(|| "collection.db".into(), |n| n.to_os_string());
    let damaged = folder.join(&name);
    let SalvageReport { tables, .. } = fc_storage::rescue::salvage_into(&damaged, db)?;
    report.copied_rows = tables.values().map(|t| t.0).sum();
    report.lost_rows = tables.values().map(|t| t.1).sum();
    // The media files the copied rows name, from the damaged folder.
    let store = SqliteStore::open(db)?;
    let media = store.read()?.list_media()?;
    drop(store);
    for m in media {
        let (Some(from), Some(to)) = (
            fc_storage::media::media_file_path(&damaged, &m.hash),
            fc_storage::media::media_file_path(db, &m.hash),
        ) else {
            continue;
        };
        if !from.is_file() {
            report.missing_media.push(m.hash);
            continue;
        }
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|e| file_err(dir, &e))?;
        }
        std::fs::copy(&from, &to).map_err(|e| file_err(&to, &e))?;
        report.media_files += 1;
    }
    Ok(())
}
