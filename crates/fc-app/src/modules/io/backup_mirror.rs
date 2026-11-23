//! Second backup location (v1.4.1): a dead disk must not take the backups with it.
//!
//! * Setting `io.backup_mirror_dir` (app scope, empty = off): a folder on another drive or a
//!   synced folder. It must already exist (an unplugged drive's path is not silently created
//!   on the system disk) and must not be inside the collection folder.
//! * Every verified backup — automatic (`auto-<millis>.fcbackup`) and `io.backup.create`
//!   (`manual-<millis>.fcbackup`) — is copied there after it is in place at home: written as
//!   `.part`, flushed to disk, **re-read there** (every entry's size and hash against the
//!   manifest, [`check_archive`]) and only then renamed. The mirror keeps the same number of
//!   copies of each kind as `io.auto_backup_keep`; rotation runs only after a good copy.
//! * Never blocking: the copy runs inside the backup job after the backup itself succeeded; a
//!   missing or unwritable mirror is recorded (`backups/status.json` `mirror_failure`) and
//!   reported in the job's result and `io.backup.status`, never an error of the job.
//!   Studying never waits on it (jobs run off the writer).
//! * Catch-up (v1.4.1): after a good copy, the verified automatic backups at home that the
//!   mirror lacks (it was missing or unwritable when they were taken; matched by the
//!   snapshot time in the manifest) are copied too — as many as the keep count leaves room
//!   for, each re-read there like the first — and `io.backup.status` says so
//!   (`mirror.last_catch_up`, the job's `mirror.caught_up`).

use super::backup_auto::{
    auto_dir, copy_info, fresh_copy_name, keep, list_auto, list_copies, load_status,
    rotate_copies, update_status,
};
use super::backup_file::{check_archive, part_path};
use crate::error::AppResult;
use crate::jobs::{CancelToken, JobContext};
use fc_api::v1::io::{
    BackupFailure, BackupMirrorStatus, MirrorCatchUp, MirrorCopy, MirrorState, VerifiedBackup,
};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::Timestamp;
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Setting: the mirror folder (empty = none).
pub const MIRROR_KEY: &str = "io.backup_mirror_dir";
/// Prefix of mirrored automatic backups (the same name as at home).
const AUTO: &str = "auto-";
/// Prefix of mirrored `io.backup.create` backups.
const MANUAL: &str = "manual-";

/// The setting.
pub(crate) fn setting() -> SettingDef {
    SettingDef {
        key: MIRROR_KEY.to_owned(),
        kind: SettingKind::Text {
            max_len: Some(1024),
        },
        default: Value::String(String::new()),
        label_key: format!("settings.{MIRROR_KEY}.label"),
        description_key: Some(format!("settings.{MIRROR_KEY}.description")),
        advanced: false,
        scopes: vec![SettingScopeKind::App],
    }
}

/// The mirror folder as set (`None`: not set).
pub(crate) fn mirror_dir(reads: &dyn fc_storage::Reads) -> AppResult<Option<PathBuf>> {
    let stored = crate::settings::resolve_stored(reads, &SettingScope::App, MIRROR_KEY)?;
    Ok(stored
        .as_ref()
        .and_then(|(v, _)| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from))
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// Whether a backup can be copied to `dir` now. Writes and removes a small probe file.
pub(crate) fn state(dir: &Path, data_dir: Option<&Path>) -> MirrorState {
    if !dir.is_absolute() || !dir.is_dir() {
        return MirrorState::Missing;
    }
    if let Some(home) = data_dir {
        if canonical(dir).starts_with(canonical(home)) {
            return MirrorState::InsideCollection;
        }
    }
    let probe = dir.join(".fc-mirror-probe");
    let wrote = std::fs::File::create(&probe)
        .and_then(|mut f| {
            f.write_all(b"fc")?;
            f.sync_all()
        })
        .is_ok();
    let _ = std::fs::remove_file(&probe);
    if wrote {
        MirrorState::Ok
    } else {
        MirrorState::Unwritable
    }
}

fn describe(state: MirrorState, dir: &Path) -> String {
    let shown = dir.to_string_lossy();
    match state {
        MirrorState::Ok => format!("{shown}: ok"),
        MirrorState::Missing => format!("{shown}: the folder is not there"),
        MirrorState::Unwritable => format!("{shown}: a file cannot be written there"),
        MirrorState::InsideCollection => {
            format!("{shown}: inside the collection folder (it would share its disk)")
        }
    }
}

/// Copies `src` to `part`, flushed to disk.
fn copy_synced(src: &Path, part: &Path) -> std::io::Result<u64> {
    let mut from = std::io::BufReader::with_capacity(1 << 20, std::fs::File::open(src)?);
    let file = std::fs::File::create(part)?;
    let mut to = std::io::BufWriter::with_capacity(1 << 20, file);
    let n = std::io::copy(&mut from, &mut to)?;
    let file = to.into_inner().map_err(std::io::IntoInnerError::into_error)?;
    file.sync_all()?;
    Ok(n)
}

/// Why a copy to the mirror failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MirrorError {
    /// In words, for the log and the status.
    pub message: String,
    /// The mirror's disk is full (`IO_DISK_FULL` at home; reported on its own here).
    pub disk_full: bool,
}

impl From<String> for MirrorError {
    fn from(message: String) -> Self {
        Self {
            message,
            disk_full: false,
        }
    }
}

fn io_failed(shown: &str, e: &std::io::Error) -> MirrorError {
    if fc_storage::is_disk_full(e) {
        MirrorError {
            message: format!("{shown}: the mirror disk is full ({e})"),
            disk_full: true,
        }
    } else {
        format!("{shown}: {e}").into()
    }
}

/// Copies `src` to `target` through `<target>.part`: flushed, re-read there (every entry's
/// size and hash), then renamed. Nothing is left behind on failure.
fn copy_verified(src: &Path, target: &Path, cancel: &CancelToken) -> Result<(), MirrorError> {
    let part = part_path(target);
    let shown = target.to_string_lossy().into_owned();
    let run = || -> Result<(), MirrorError> {
        let n = copy_synced(src, &part).map_err(|e| io_failed(&shown, &e))?;
        let want = std::fs::metadata(src).map(|m| m.len()).unwrap_or(n);
        let got = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if got != want {
            return Err(format!("{shown}: {got} bytes written, {want} expected").into());
        }
        // Re-read on the mirror's disk: every entry's size and hash.
        check_archive(&part, cancel)
            .map_err(|e| MirrorError::from(format!("{shown}: {}", e.into_api().message)))?;
        std::fs::rename(&part, target).map_err(|e| io_failed(&shown, &e))
    };
    let r = run();
    if r.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    r
}

/// Snapshot time (manifest `created_at`) of a backup file, `None` when unreadable.
fn snapshot_time(path: &Path) -> Option<i64> {
    super::backup_file::open_backup(path)
        .ok()
        .map(|r| r.manifest().created_at.0)
}

/// Copies the verified automatic backups in `home_auto` that `dir` lacks (matched by
/// snapshot time) and that the keep count leaves room for, oldest last; each is named after
/// its own time so rotation keeps the newest. Stops at the first failure. Returns how many.
pub(crate) fn catch_up(home_auto: &Path, dir: &Path, keep: u32, cancel: &CancelToken) -> u32 {
    let keep = usize::try_from(keep.max(1)).unwrap_or(usize::MAX);
    let present: Vec<(i64, Option<i64>)> = list_copies(dir, AUTO)
        .into_iter()
        .map(|(taken, p)| (taken, snapshot_time(&p)))
        .collect();
    let have: std::collections::HashSet<i64> = present.iter().filter_map(|(_, s)| *s).collect();
    let missing: Vec<(i64, PathBuf)> = list_auto(home_auto)
        .into_iter()
        .take(keep)
        .filter(|(_, p)| snapshot_time(p).is_some_and(|s| !have.contains(&s)))
        .collect();
    // Only what survives rotation: the newest `keep` of mirror ∪ missing.
    let mut times: Vec<i64> = present
        .iter()
        .map(|(t, _)| *t)
        .chain(missing.iter().map(|(t, _)| *t))
        .collect();
    times.sort_unstable_by(|a, b| b.cmp(a));
    let floor = times.get(keep - 1).copied().unwrap_or(i64::MIN);
    let mut copied = 0;
    for (taken, src) in missing.into_iter().filter(|(t, _)| *t >= floor) {
        if cancel.is_cancelled() {
            break;
        }
        let target = fresh_copy_name(dir, AUTO, Timestamp(taken));
        if copy_verified(&src, &target, cancel).is_err() {
            break;
        }
        copied += 1;
    }
    if copied > 0 {
        rotate_copies(dir, AUTO, u32::try_from(keep).unwrap_or(u32::MAX));
    }
    copied
}

/// Copies the verified backup `src` into `dir` as `<prefix><now>.fcbackup`, re-reads it there
/// and rotates (see the module docs). `Err`: why not.
pub(crate) fn copy_to_mirror(
    src: &Path,
    dir: &Path,
    data_dir: Option<&Path>,
    automatic: bool,
    now: Timestamp,
    keep: u32,
    cancel: &CancelToken,
) -> Result<PathBuf, MirrorError> {
    let s = state(dir, data_dir);
    if s != MirrorState::Ok {
        return Err(describe(s, dir).into());
    }
    // A backup written straight into the mirror folder is its own copy.
    if src.parent().map(canonical) == Some(canonical(dir)) {
        return Ok(src.to_path_buf());
    }
    let prefix = if automatic { AUTO } else { MANUAL };
    let target = fresh_copy_name(dir, prefix, now);
    copy_verified(src, &target, cancel)?;
    rotate_copies(dir, prefix, keep);
    Ok(target)
}

/// After a verified backup at `src`: copies it to the mirror when one is set, records the
/// outcome in `status.json` and returns it for the job's report (`None`: no mirror set).
/// Never fails the job.
pub(crate) fn mirror_backup(
    ctx: &JobContext<'_>,
    data_dir: Option<&Path>,
    src: &Path,
    automatic: bool,
) -> Option<MirrorCopy> {
    let (dir, keep) = {
        let tx = ctx.read().ok()?;
        (mirror_dir(&*tx).ok()??, keep(&*tx).ok()?)
    };
    ctx.report(Some(0.95), "io.progress.mirror");
    let outcome = copy_to_mirror(src, &dir, data_dir, automatic, ctx.now(), keep, &ctx.cancel);
    let caught_up = match (&outcome, data_dir) {
        (Ok(_), Some(home)) => catch_up(&auto_dir(home), &dir, keep, &ctx.cancel),
        _ => 0,
    };
    if let Some(home) = data_dir {
        let at = ctx.now();
        let shown = dir.to_string_lossy().into_owned();
        match &outcome {
            Ok(_) => update_status(home, |s| {
                s.mirror_failure = None;
                s.mirror_disk_full = false;
                s.mirror_dir = Some(shown);
                if caught_up > 0 {
                    s.mirror_catch_up = Some(MirrorCatchUp {
                        at,
                        copies: caught_up,
                    });
                }
            }),
            Err(e) => update_status(home, |s| {
                s.mirror_failure = Some(BackupFailure {
                    at,
                    message: e.message.clone(),
                });
                s.mirror_disk_full = e.disk_full;
                s.mirror_dir = Some(shown);
            }),
        }
    }
    Some(match outcome {
        Ok(path) => MirrorCopy {
            path: Some(path.to_string_lossy().into_owned()),
            error: None,
            disk_full: false,
            caught_up,
        },
        Err(e) => MirrorCopy {
            path: None,
            error: Some(e.message),
            disk_full: e.disk_full,
            caught_up: 0,
        },
    })
}

/// The mirror part of `io.backup.status` (`None`: not set).
pub(crate) fn status(
    reads: &dyn fc_storage::Reads,
    data_dir: Option<&Path>,
) -> AppResult<Option<BackupMirrorStatus>> {
    let Some(dir) = mirror_dir(reads)? else {
        return Ok(None);
    };
    let state = state(&dir, data_dir);
    let mut copies: Vec<VerifiedBackup> = Vec::new();
    if dir.is_dir() {
        for (prefix, automatic) in [(AUTO, true), (MANUAL, false)] {
            copies.extend(list_copies(&dir, prefix).into_iter().map(|(taken, path)| {
                VerifiedBackup {
                    automatic,
                    ..copy_info(&path, taken)
                }
            }));
        }
    }
    copies.sort_by_key(|c| std::cmp::Reverse(c.verified_at.0));
    let file = data_dir.map(load_status).unwrap_or_default();
    Ok(Some(BackupMirrorStatus {
        folder: dir.to_string_lossy().into_owned(),
        state,
        copies,
        disk_full: file.mirror_failure.is_some() && file.mirror_disk_full,
        last_failure: file.mirror_failure,
        last_catch_up: file.mirror_catch_up,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_mirror_disk_is_said_plainly() {
        let full = std::io::Error::from(std::io::ErrorKind::StorageFull);
        let e = io_failed("E:\\yedek\\auto-1.fcbackup", &full);
        assert!(e.disk_full);
        assert!(e.message.contains("mirror disk is full"), "{}", e.message);
        let other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert!(!io_failed("x", &other).disk_full);
    }
}
