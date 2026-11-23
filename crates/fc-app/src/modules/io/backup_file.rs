//! Writing and verifying `.fcbackup` files on disk (v1.4), shared by `io.backup.create`,
//! automatic backups, the safety backup of a restore and `io.backup.verify`.
//!
//! * **Streaming**: the storage writes the dump's JSON row by row straight into the archive
//!   (`dump_collection_json`, v1.4 Z2), media files are read and written one at a time
//!   ([`fc_interop::fcbackup`]); memory is bounded by the largest media file, not the
//!   collection or its review history.
//! * **Never half a file**: the archive goes to `<name>.part` beside the target, is flushed to
//!   disk, checked, and only then renamed over the target. A failure, a cancellation or a
//!   crash leaves the previous file at the target untouched (a stale `.part` is removed by
//!   the next write).
//! * **Missing media** (a row whose file is gone or no longer matches its hash) do not stop
//!   the backup: they are listed in the manifest and the report (`missing_media`), and the
//!   backup is marked incomplete. Refusing would leave a collection with one lost file
//!   without any later backup.
//! * **Verification** ([`verify_file`]): the backup restored into a scratch profile (a
//!   temporary SQLite collection, deleted afterwards) — the dump loaded as it streams out of
//!   the archive (a dump at an older schema is read whole and brought to this build's schema
//!   first), every media file put into the scratch media store — then read back: row counts
//!   of the main tables and every media file's hash compared with the manifest. The live
//!   collection is not touched.

use super::backup::{compatible, row_counts};
use super::media::media_store;
use super::{file_err, interop_err};
use crate::error::{AppError, AppResult};
use crate::jobs::CancelToken;
use fc_api::v1::io::BackupVerifyReport;
use fc_api::ErrorCode;
use fc_interop::fcbackup::{self, BackupHeader, BackupReader, BackupWriter, Manifest};
use fc_storage::repo::{DumpStream, DumpStreamError, MediaRecord};
use fc_storage::{ReadTx, Reads, SqliteStore, StorageError, Store};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// Tables whose row counts a verification compares.
pub(crate) const CHECKED_TABLES: [&str; 6] = [
    "decks",
    "notetypes",
    "notes",
    "cards",
    "review_log",
    "media",
];

/// I/O buffer of backup files.
const BUF: usize = 1 << 20;

/// How a freshly written backup is checked before it replaces the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Check {
    /// Every entry re-read from the file and hashed ([`BackupReader::check`]).
    Archive,
    /// [`Check::Archive`] plus a restore into a scratch profile ([`verify_file`]).
    Full,
}

/// A backup written to disk.
pub(crate) struct Written {
    pub manifest: Manifest,
    pub bytes: u64,
    /// The verification report ([`Check::Full`] only).
    pub verified: Option<BackupVerifyReport>,
}

/// `<file name>.part` beside `path`.
pub(crate) fn part_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    path.with_file_name(name)
}

fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// A writer error on `path` (disk full, removed folder…) as `IO_FILE`.
fn write_failed(path: &Path, e: impl ToString) -> AppError {
    file_err(&shown(path), std::io::Error::other(e.to_string()))
}

/// Writes a backup of what a `snapshot` sees to `path` (see the module docs). `scratch` holds the
/// scratch profile of a [`Check::Full`] verification.
pub(crate) fn write_backup_file<'a>(
    snapshot: &dyn Fn() -> AppResult<Box<dyn ReadTx + 'a>>,
    now: fc_core::Timestamp,
    path: &Path,
    cancel: &CancelToken,
    check: Check,
    scratch: &Path,
) -> AppResult<Written> {
    let part = part_path(path);
    let _ = std::fs::remove_file(&part);
    // The read snapshot is held while writing only, not while verifying.
    let written = snapshot().and_then(|tx| write_part(&*tx, now, &part, cancel));
    let out = written.and_then(|(manifest, bytes)| {
        cancel.check()?;
        let verified = match check {
            Check::Archive => {
                check_archive(&part, cancel)?;
                None
            }
            Check::Full => {
                let report = verify_file(&part, scratch, cancel)?;
                if !report.ok {
                    return Err(AppError::api(
                        ErrorCode::Internal,
                        format!("the backup did not verify: {}", report.problems.join("; ")),
                    ));
                }
                Some(report)
            }
        };
        cancel.check()?;
        std::fs::rename(&part, path).map_err(|e| file_err(&shown(path), e))?;
        Ok(Written {
            manifest,
            bytes,
            verified,
        })
    });
    if out.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    out
}

/// A streamed dump's outcome as [`fcbackup`] wants it: a JSON-side failure is the stream's
/// (reading: not a dump; writing: the file), a storage one stays the caller's.
pub(crate) fn split_stream<T>(
    r: Result<T, DumpStreamError>,
) -> std::io::Result<Result<T, StorageError>> {
    match r {
        Ok(v) => Ok(Ok(v)),
        Err(DumpStreamError::Json(e)) => Err(std::io::Error::other(e)),
        Err(DumpStreamError::Storage(e)) => Ok(Err(e)),
    }
}

/// Streams the archive into `part` and flushes it to disk.
fn write_part(
    reads: &dyn Reads,
    now: fc_core::Timestamp,
    part: &Path,
    cancel: &CancelToken,
) -> AppResult<(Manifest, u64)> {
    let store = media_store();
    let media = store.list(reads)?;
    let file = File::create(part).map_err(|e| file_err(&shown(part), e))?;
    let mut w = BackupWriter::new(
        BufWriter::with_capacity(BUF, file),
        BackupHeader {
            created_at: now,
            schema_versions: Vec::new(),
            counts: BTreeMap::new(),
        },
    );
    // Row by row from the snapshot into the archive; the manifest (written last) gets the
    // schema and the rows per table actually written.
    let written = w
        .write_collection_with(|out| split_stream(reads.dump_collection_json(out)))
        .map_err(|e| write_failed(part, e))?
        .map_err(|e| write_failed(part, e))??;
    let header = w.header_mut();
    header.schema_versions = written.schema_versions;
    header.counts = written.counts;
    for m in &media {
        cancel.check()?;
        match store.bytes(reads, &m.hash)? {
            Some(bytes) => {
                w.add_media(&m.hash, &bytes)
                    .map_err(|e| write_failed(part, e))?;
            }
            None => w.mark_missing(&m.hash),
        }
    }
    let (buf, manifest) = w.finish().map_err(|e| write_failed(part, e))?;
    let file = buf.into_inner().map_err(|e| write_failed(part, e))?;
    file.sync_all().map_err(|e| file_err(&shown(part), e))?;
    let bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
    Ok((manifest, bytes))
}

/// Opens a backup file for streaming reads.
pub(crate) fn open_backup(path: &Path) -> AppResult<BackupReader<BufReader<File>>> {
    let file = File::open(path).map_err(|e| file_err(&shown(path), e))?;
    BackupReader::open(BufReader::with_capacity(BUF, file)).map_err(interop_err)
}

/// Re-reads every entry of the backup at `path` and checks sizes and hashes.
pub(crate) fn check_archive(path: &Path, cancel: &CancelToken) -> AppResult<()> {
    let mut r = open_backup(path)?;
    r.check(|_, _| {
        cancel
            .check()
            .map_err(|_| fc_interop::InteropError::invalid(fcbackup::FORMAT, "cancelled"))
    })
    .map_err(|e| {
        if cancel.is_cancelled() {
            crate::jobs::cancelled()
        } else {
            interop_err(e)
        }
    })
}

/// Restores the backup at `path` into a scratch profile under `scratch` and compares (see the
/// module docs). Damage is reported in `problems` (`ok: false`); only cancellation and an
/// unreadable scratch folder are errors.
pub(crate) fn verify_file(
    path: &Path,
    scratch: &Path,
    cancel: &CancelToken,
) -> AppResult<BackupVerifyReport> {
    let mut report = BackupVerifyReport {
        path: shown(path),
        ..BackupVerifyReport::default()
    };
    match verify_into(path, scratch, cancel, &mut report) {
        Ok(()) => {}
        Err(_) if cancel.is_cancelled() => return Err(crate::jobs::cancelled()),
        Err(e) => report.problems.push(e.into_api().message),
    }
    report.ok = report.problems.is_empty();
    Ok(report)
}

fn verify_into(
    path: &Path,
    scratch: &Path,
    cancel: &CancelToken,
    report: &mut BackupVerifyReport,
) -> AppResult<()> {
    let mut r = open_backup(path)?;
    let manifest = r.manifest().clone();
    report.created_at = Some(manifest.created_at);
    report.complete = manifest.is_complete();
    report.missing_media = manifest.missing_media.clone();
    if !compatible(&manifest.schema_versions) {
        report
            .problems
            .push("the backup comes from a newer version".into());
        return Ok(());
    }
    std::fs::create_dir_all(scratch).map_err(|e| file_err(&shown(scratch), e))?;
    let dir = tempfile::Builder::new()
        .prefix(".verify-")
        .tempdir_in(scratch)
        .map_err(|e| file_err(&shown(scratch), e))?;
    let store = SqliteStore::open(&dir.path().join("collection.db"))?;
    let media = r.media_list();
    // The dump is what the manifest says it is.
    let mut compare = |counts: &BTreeMap<String, u64>| {
        for (table, n) in counts {
            let want = manifest.counts.get(table).copied().unwrap_or(0);
            if want != *n {
                report
                    .problems
                    .push(format!("{table}: {n} rows, the manifest says {want}"));
            }
        }
    };
    let expected = {
        let mut tx = store.write()?;
        // Loaded as it streams out of the archive (checked for cancellation every chunk).
        let mut hook = |_: &mut fc_storage::repo::TableDump| {
            if cancel.is_cancelled() {
                return Err(StorageError::Corrupt {
                    table: "backup",
                    message: "cancelled".into(),
                });
            }
            Ok(())
        };
        let read = r
            .collection_with(|json| split_stream(tx.load_collection_json(json, &mut hook)))
            .map_err(interop_err)??;
        let expected = match read {
            DumpStream::Streamed(summary) => {
                compare(&summary.counts);
                summary.counts
            }
            // An older schema: brought forward whole, then loaded.
            DumpStream::Whole(dump) => {
                compare(&row_counts(&dump));
                cancel.check()?;
                let dump = fc_storage::upgrade_dump(dump)?;
                let expected = row_counts(&dump);
                tx.load_collection(&dump)?;
                expected
            }
        };
        cancel.check()?;
        let files = media_store();
        for m in &media {
            cancel.check()?;
            let bytes = r.read_media(m).map_err(interop_err)?;
            let record = tx.get_media(&m.hash)?.unwrap_or(MediaRecord {
                hash: m.hash.clone(),
                mime: "application/octet-stream".into(),
                size: bytes.len() as u64,
                original_name: None,
                ref_count: 0,
                created_at: manifest.created_at,
            });
            files.put(&mut *tx, &record, &bytes)?;
        }
        tx.commit()?;
        expected
    };
    // Read back what the scratch profile holds.
    let tx = store.read()?;
    let back = tx.dump_row_counts()?;
    let mut counts = BTreeMap::new();
    for table in CHECKED_TABLES {
        let want = expected.get(table).copied().unwrap_or(0);
        let got = back.get(table).copied().unwrap_or(0);
        if want != got {
            report
                .problems
                .push(format!("{table}: {got} rows restored, {want} expected"));
        }
        counts.insert(table.to_owned(), got);
    }
    report.counts = counts;
    let mut restored = 0u32;
    for m in &media {
        cancel.check()?;
        match tx.read_media_file(&m.hash)? {
            Some(bytes) if fc_interop::media::sha256_hex(&bytes) == m.hash => restored += 1,
            _ => report
                .problems
                .push(format!("media {} did not restore", m.hash)),
        }
    }
    report.media_files = restored;
    drop(tx);
    drop(store);
    // Best effort: the folder is removed on drop too.
    let _ = dir.close();
    Ok(())
}
