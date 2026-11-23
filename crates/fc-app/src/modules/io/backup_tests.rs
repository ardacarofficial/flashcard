//! v1.4 backups (`.orchestra/v14/briefs/a1-backup.md`): restore into an empty profile brings
//! notes, review history and media back byte-identical; an interrupted write leaves the
//! previous backup intact; missing media are reported; a corrupt backup fails verification
//! and restore without touching live data; automatic backups follow the injected clock and
//! keep the last N verified copies. Every check reads the persisted result back.

use super::backup_file::{part_path, write_backup_file, Check};
use crate::jobs::CancelToken;
use crate::testing::{TempSqlite, TestApp};
use fc_api::v1::common::Empty;
use fc_api::v1::io::*;
use fc_api::v1::jobs::{JobDto, JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::settings::SetSetting;
use fc_api::ErrorCode;
use fc_core::settings::SettingScope;
use fc_core::card::Rating;
use fc_core::{Clock, Timestamp};
use fc_interop::anki::fixtures as anki_fx;
use fc_interop::media::sha256_hex;
use fc_storage::repo::{CollectionDump, MediaRecord};
use fc_storage::Store;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

const HOUR: i64 = 60 * 60 * 1000;
const DAY: i64 = 24 * HOUR;

/// A ticking clock (1 ms per read, like `TickClock`) that tests can move forward.
struct MovableClock {
    tick: AtomicI64,
    offset: AtomicI64,
}

impl MovableClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            tick: AtomicI64::new(1_767_225_600_000),
            offset: AtomicI64::new(0),
        })
    }

    fn advance(&self, ms: i64) {
        self.offset.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for MovableClock {
    fn now(&self) -> Timestamp {
        Timestamp(self.tick.fetch_add(1, Ordering::SeqCst) + self.offset.load(Ordering::SeqCst))
    }
}

/// An app on a fresh SQLite file with a collection folder; also returns that folder.
struct Profile {
    t: TestApp,
    data: PathBuf,
    _dir: tempfile::TempDir,
}

fn profile_with(clock: Option<Arc<MovableClock>>) -> Profile {
    let db = TempSqlite::new();
    let data = db.path().parent().unwrap().to_path_buf();
    let store: Arc<dyn Store> = Arc::new(db);
    let t = match clock {
        Some(c) => TestApp::with_clock(store, c),
        None => TestApp::new(store),
    };
    t.app.set_data_dir(data.clone());
    Profile {
        t,
        data,
        _dir: tempfile::tempdir().unwrap(),
    }
}

fn profile() -> Profile {
    profile_with(None)
}

fn finish(t: &TestApp, job: &JobDto) -> JobDto {
    t.app.run_pending_jobs().unwrap();
    t.ok::<JobsGet>(JobRef { id: job.id })
}

fn result<T: DeserializeOwned>(job: &JobDto) -> T {
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    serde_json::from_value(job.result.clone().unwrap()).unwrap()
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Rich state: an Anki package with media (image + sound), every card state and review history.
fn fill(t: &TestApp, scratch: &Path) {
    let src = scratch.join("in.apkg");
    std::fs::write(
        &src,
        fc_interop::anki::write_package(&anki_fx::sample(), anki_fx::NOW).unwrap(),
    )
    .unwrap();
    let job = t.ok::<IoImportStart>(StartImport {
        source: ImportSource::File { path: s(&src) },
        format: None,
        text_options: None,
        deck_id: None,
        notetype_id: None,
        columns: None,
        skip_first_row: None,
        duplicates: None,
        create_missing_options: None,
        include_history: None,
    });
    result::<ImportReport>(&finish(t, &job));
}

fn table(d: &CollectionDump, name: &str) -> Rows {
    d.tables
        .iter()
        .find(|t| t.name == name)
        .map(|t| t.rows.clone())
        .unwrap_or_default()
}

fn dump(t: &TestApp) -> CollectionDump {
    t.store.read().unwrap().dump_collection().unwrap()
}

type Rows = Vec<Vec<fc_storage::repo::SqlValue>>;
type MediaFiles = Vec<(MediaRecord, Vec<u8>)>;

/// What must survive a backup → restore, byte for byte.
fn essentials(t: &TestApp) -> (Vec<Rows>, MediaFiles) {
    let d = dump(t);
    let rows = [
        "decks",
        "notetypes",
        "notes",
        "cards",
        "review_log",
        "media",
    ]
    .iter()
    .map(|n| table(&d, n))
    .collect();
    let tx = t.store.read().unwrap();
    let mut media: MediaFiles = tx
        .list_media()
        .unwrap()
        .into_iter()
        .filter_map(|m| {
            let bytes = tx.read_media_file(&m.hash).unwrap()?;
            Some((m, bytes))
        })
        .collect();
    media.sort_by(|a, b| a.0.hash.cmp(&b.0.hash));
    (rows, media)
}

fn create(t: &TestApp, path: &Path) -> BackupReport {
    let job = t.ok::<IoBackupCreate>(CreateBackup { path: s(path) });
    result(&finish(t, &job))
}

fn media_file(data: &Path, hash: &str) -> PathBuf {
    data.join("media").join(&hash[..2]).join(hash)
}

#[test]
fn restore_into_an_empty_profile_brings_notes_history_and_media_back_byte_identical() {
    let a = profile();
    fill(&a.t, a._dir.path());
    let before = essentials(&a.t);
    assert!(!before.0[4].is_empty(), "the fixture has review history");
    assert_eq!(before.1.len(), 2, "…and two media files");
    let path = a._dir.path().join("full.fcbackup");
    let rep = create(&a.t, &path);
    assert!(rep.verified);
    assert!(rep.missing_media.is_empty());
    assert_eq!((rep.notes, rep.media), (4, 2));
    assert!(rep.review_log > 0);
    assert!(!part_path(&path).exists());

    // A brand-new profile on another "computer".
    let b = profile();
    assert!(b.t.store.read().unwrap().list_media().unwrap().is_empty());
    let job = b.t.ok::<IoBackupRestore>(RestoreBackup {
        path: s(&path),
        safety_backup_path: None,
    });
    let rr: RestoreReport = result(&finish(&b.t, &job));
    assert_eq!(rr.media, 2);
    assert!(rr.missing_media.is_empty());
    let after = essentials(&b.t);
    assert_eq!(
        after.0, before.0,
        "decks, notes, cards, review history and media rows"
    );
    assert_eq!(after.1, before.1, "media files byte-identical");
    for (m, bytes) in &after.1 {
        assert_eq!(sha256_hex(bytes), m.hash);
        assert!(media_file(&b.data, &m.hash).exists());
    }
}

/// v1.4 (Z1): typed answers (`typed_answers`) are user history: a backup carries them.
#[test]
fn typed_answers_survive_backup_and_restore() {
    use fc_storage::repo::{CardFilter, TypedAnswerRecord};
    let a = profile();
    fill(&a.t, a._dir.path());
    let card = {
        let tx = a.t.store.read().unwrap();
        tx.list_cards(&CardFilter::default())
            .unwrap()
            .into_iter()
            .find(|c| !tx.review_logs_for_card(c.id).unwrap().is_empty())
            .expect("a card with review history")
    };
    {
        let mut tx = a.t.store.write().unwrap();
        let review = tx.review_logs_for_card(card.id).unwrap().pop().unwrap();
        tx.insert_typed_answer(&TypedAnswerRecord {
            id: 0,
            card_id: card.id,
            review_log_id: Some(review.id),
            mode: "normal".into(),
            typed: "yazdığım cevap".into(),
            correct: Some(false),
            created_at: review.created_at,
        })
        .unwrap();
        tx.commit().unwrap();
    }
    let kept =
        a.t.store
            .read()
            .unwrap()
            .typed_answers_of_card(card.id)
            .unwrap();
    assert_eq!(kept.len(), 1);
    let path = a._dir.path().join("typed.fcbackup");
    assert!(create(&a.t, &path).verified);

    let b = profile();
    let job = b.t.ok::<IoBackupRestore>(RestoreBackup {
        path: s(&path),
        safety_backup_path: None,
    });
    let _: RestoreReport = result(&finish(&b.t, &job));
    let restored =
        b.t.store
            .read()
            .unwrap()
            .typed_answers_of_card(card.id)
            .unwrap();
    assert_eq!(restored, kept);
}

#[test]
fn an_interrupted_write_leaves_the_previous_backup_intact() {
    let p = profile();
    fill(&p.t, p._dir.path());
    let path = p._dir.path().join("b.fcbackup");
    create(&p.t, &path);
    let good = std::fs::read(&path).unwrap();
    // A crash of an earlier run left a `.part`; this run is cancelled after the dump, at the
    // first media file.
    std::fs::write(part_path(&path), b"half a file").unwrap();
    p.t.ok::<DecksCreate>(fc_api::v1::decks::CreateDeck {
        path: "Changed".into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    });
    let cancel = CancelToken::cancelling_at_check(1);
    let store = p.t.store.clone();
    let e = write_backup_file(
        &|| Ok(store.read()?),
        Timestamp(5),
        &path,
        &cancel,
        Check::Full,
        &p.data.join("backups"),
    )
    .err()
    .expect("cancelled");
    assert_eq!(e.into_api().code, ErrorCode::ConflictJobCancelled);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        good,
        "the previous backup is untouched"
    );
    assert!(!part_path(&path).exists(), "no half file is left behind");
    // …and it still restores and verifies.
    let job = p.t.ok::<IoBackupVerify>(VerifyBackup { path: s(&path) });
    let v: BackupVerifyReport = result(&finish(&p.t, &job));
    assert!(v.ok, "{:?}", v.problems);
    // A cancelled job (cancelled before it runs) does not touch it either.
    let job = p.t.ok::<IoBackupCreate>(CreateBackup { path: s(&path) });
    p.t.ok::<JobsCancel>(JobRef { id: job.id });
    assert_eq!(finish(&p.t, &job).status, JobStatus::Cancelled);
    assert_eq!(std::fs::read(&path).unwrap(), good);
}

#[test]
fn missing_media_are_reported_and_the_backup_is_marked_incomplete() {
    let p = profile();
    fill(&p.t, p._dir.path());
    let media = p.t.store.read().unwrap().list_media().unwrap();
    let lost = media[0].hash.clone();
    std::fs::remove_file(media_file(&p.data, &lost)).unwrap();
    let path = p._dir.path().join("b.fcbackup");
    let rep = create(&p.t, &path);
    assert!(rep.verified, "what is there is complete and verified");
    assert_eq!(rep.missing_media, vec![lost.clone()]);
    assert_eq!(rep.media, 1);
    let info = p.t.ok::<IoBackupInspect>(InspectBackup { path: s(&path) });
    assert!(!info.complete);
    let st = p.t.ok::<IoBackupStatus>(Empty {});
    assert!(!st.last_verified.unwrap().complete);
    // Restoring it elsewhere brings the rest back and says what was missing.
    let b = profile();
    let job = b.t.ok::<IoBackupRestore>(RestoreBackup {
        path: s(&path),
        safety_backup_path: None,
    });
    let rr: RestoreReport = result(&finish(&b.t, &job));
    assert_eq!(rr.missing_media, vec![lost.clone()]);
    assert_eq!(rr.media, 1);
    let kept = &media[1].hash;
    assert!(b
        .t
        .store
        .read()
        .unwrap()
        .read_media_file(kept)
        .unwrap()
        .is_some());
}

/// Flips a byte in the middle of the first occurrence of `needle`.
fn flip(bytes: &mut [u8], needle: &[u8]) {
    let at = bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("needle");
    bytes[at + needle.len() / 2] ^= 0xff;
}

#[test]
fn a_corrupt_backup_fails_verification_and_restore_without_touching_live_data() {
    let p = profile();
    fill(&p.t, p._dir.path());
    let path = p._dir.path().join("b.fcbackup");
    create(&p.t, &path);
    let last = p.t.ok::<IoBackupStatus>(Empty {}).last_verified.unwrap();
    // Damage a media entry (stored as is inside the archive).
    let media = essentials(&p.t).1;
    let mut bytes = std::fs::read(&path).unwrap();
    let probe = &media[0].1[..media[0].1.len().min(64)];
    flip(&mut bytes, probe);
    let bad = p._dir.path().join("bad.fcbackup");
    std::fs::write(&bad, &bytes).unwrap();

    let live = essentials(&p.t);
    let job = p.t.ok::<IoBackupVerify>(VerifyBackup { path: s(&bad) });
    let v: BackupVerifyReport = result(&finish(&p.t, &job));
    assert!(!v.ok);
    assert!(!v.problems.is_empty());
    // A failed check is not "the last verified backup".
    assert_eq!(
        p.t.ok::<IoBackupStatus>(Empty {}).last_verified.unwrap(),
        last
    );

    let job = p.t.ok::<IoBackupRestore>(RestoreBackup {
        path: s(&bad),
        safety_backup_path: Some(s(&p._dir.path().join("safety.fcbackup"))),
    });
    let done = finish(&p.t, &job);
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error.unwrap().code, ErrorCode::ValidationImportFormat);
    assert_eq!(essentials(&p.t), live, "the live collection is untouched");
    // Verification restores into a scratch profile and cleans it up.
    let leftovers: Vec<_> = std::fs::read_dir(p.data.join("backups"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(".verify-"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn verify_reports_what_the_scratch_profile_holds() {
    let p = profile();
    fill(&p.t, p._dir.path());
    let path = p._dir.path().join("b.fcbackup");
    create(&p.t, &path);
    let live = essentials(&p.t);
    let job = p.t.ok::<IoBackupVerify>(VerifyBackup { path: s(&path) });
    let v: BackupVerifyReport = result(&finish(&p.t, &job));
    assert!(v.ok, "{:?}", v.problems);
    assert!(v.complete);
    assert_eq!(v.counts["notes"], 4);
    assert_eq!(v.counts["media"], 2);
    assert_eq!(v.counts["review_log"], live.0[4].len() as u64);
    assert_eq!(v.media_files, 2);
    assert_eq!(essentials(&p.t), live);
    let st = p.t.ok::<IoBackupStatus>(Empty {});
    let last = st.last_verified.unwrap();
    assert_eq!(last.path, s(&path));
    assert!(!last.automatic);
    // Not a backup at all.
    let junk = p._dir.path().join("junk.fcbackup");
    std::fs::write(&junk, b"junk").unwrap();
    let job = p.t.ok::<IoBackupVerify>(VerifyBackup { path: s(&junk) });
    let v: BackupVerifyReport = result(&finish(&p.t, &job));
    assert!(!v.ok && v.created_at.is_none());
}

fn set(t: &TestApp, key: &str, value: serde_json::Value) {
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: key.into(),
        value,
    });
}

fn auto_files(data: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(data.join("backups").join("auto"))
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

#[test]
fn automatic_backups_follow_the_clock_and_keep_the_last_n_verified_copies() {
    let clock = MovableClock::new();
    let p = profile_with(Some(clock.clone()));
    fill(&p.t, p._dir.path());
    let tick = || super::auto_backup_tick(&p.t.app, true).unwrap();

    // Default: daily, keep 7. No copy yet: due now.
    let st = p.t.ok::<IoBackupStatus>(Empty {});
    assert_eq!((st.schedule, st.keep), (AutoBackupSchedule::Daily, 7));
    assert!(st.last_verified.is_none());
    let job = tick().expect("due");
    assert!(tick().is_none(), "one at a time");
    let rep: BackupReport = result(&finish(&p.t, &job));
    assert!(rep.verified);
    assert_eq!(auto_files(&p.data).len(), 1);
    assert!(tick().is_none(), "not due again for a day");
    let st = p.t.ok::<IoBackupStatus>(Empty {});
    let first = st.last_verified.clone().unwrap();
    assert!(first.automatic && first.complete);
    assert_eq!(st.automatic.len(), 1);
    assert_eq!(st.next_due.unwrap().0, first.created_at.0 + DAY);

    // Keep 2: after three more days only the two newest remain, the newest is the last good.
    set(&p.t, super::backup_auto::KEEP_KEY, serde_json::json!(2));
    for _ in 0..3 {
        clock.advance(DAY + HOUR);
        let job = tick().expect("due");
        result::<BackupReport>(&finish(&p.t, &job));
    }
    let files = auto_files(&p.data);
    assert_eq!(files.len(), 2, "{files:?}");
    let st = p.t.ok::<IoBackupStatus>(Empty {});
    assert_eq!(st.automatic.len(), 2);
    let newest = st.automatic[0].clone();
    assert_eq!(st.last_verified.unwrap().path, newest.path);
    assert!(!files.iter().any(|f| first.path.ends_with(f.as_str())));

    // A failed run (the folder cannot be created) keeps the good copies and is reported; the
    // next try waits an hour.
    clock.advance(DAY + HOUR);
    let auto = p.data.join("backups").join("auto");
    std::fs::rename(&auto, p.data.join("backups").join("moved")).unwrap();
    std::fs::write(&auto, b"not a folder").unwrap();
    let job = tick().expect("due");
    assert_eq!(finish(&p.t, &job).status, JobStatus::Failed);
    let st = p.t.ok::<IoBackupStatus>(Empty {});
    assert!(st.last_failure.is_some());
    assert!(tick().is_none(), "backs off after a failure");
    std::fs::remove_file(&auto).unwrap();
    std::fs::rename(p.data.join("backups").join("moved"), &auto).unwrap();
    assert_eq!(auto_files(&p.data), files, "the good copies are untouched");
    clock.advance(2 * HOUR);
    let job = tick().expect("due again");
    result::<BackupReport>(&finish(&p.t, &job));
    let st = p.t.ok::<IoBackupStatus>(Empty {});
    assert!(st.last_failure.is_none());
    assert_eq!(st.automatic.len(), 2);
    assert_ne!(st.automatic[0].path, newest.path);

    // A cancelled run leaves nothing behind and records no failure.
    clock.advance(DAY + HOUR);
    let files = auto_files(&p.data);
    let job = tick().expect("due");
    p.t.ok::<JobsCancel>(JobRef { id: job.id });
    assert_eq!(finish(&p.t, &job).status, JobStatus::Cancelled);
    assert_eq!(auto_files(&p.data), files);
    assert!(p.t.ok::<IoBackupStatus>(Empty {}).last_failure.is_none());

    // Weekly, then off.
    set(
        &p.t,
        super::backup_auto::SCHEDULE_KEY,
        serde_json::json!("weekly"),
    );
    let st = p.t.ok::<IoBackupStatus>(Empty {});
    let newest = st.automatic[0].created_at.0;
    assert_eq!(st.next_due.unwrap().0, newest + 7 * DAY);
    assert!(tick().is_none());
    set(
        &p.t,
        super::backup_auto::SCHEDULE_KEY,
        serde_json::json!("off"),
    );
    clock.advance(30 * DAY);
    assert!(tick().is_none());
    assert!(p.t.ok::<IoBackupStatus>(Empty {}).next_due.is_none());
    // `io.backup.auto` still runs one on demand.
    let started = p.t.ok::<IoBackupAuto>(Empty {});
    result::<BackupReport>(&finish(&p.t, &started.job.unwrap()));
}

#[test]
fn status_says_when_the_backup_is_stale() {
    let clock = MovableClock::new();
    let p = profile_with(Some(clock.clone()));
    fill(&p.t, p._dir.path());
    let stale = || p.t.ok::<IoBackupStatus>(Empty {}).stale;
    let tick = || super::auto_backup_tick(&p.t.app, true).unwrap();

    // No backup yet.
    assert_eq!(stale(), Some(BackupStale::Never));
    result::<BackupReport>(&finish(&p.t, &tick().expect("due")));
    assert_eq!(stale(), None);
    // Daily: fine for two days, then old.
    clock.advance(2 * DAY - HOUR);
    assert_eq!(stale(), None);
    clock.advance(2 * HOUR);
    assert_eq!(stale(), Some(BackupStale::Old));
    // Weekly: two weeks.
    set(&p.t, super::backup_auto::SCHEDULE_KEY, serde_json::json!("weekly"));
    assert_eq!(stale(), None);
    clock.advance(13 * DAY);
    assert_eq!(stale(), Some(BackupStale::Old));
    // Off: a week.
    set(&p.t, super::backup_auto::SCHEDULE_KEY, serde_json::json!("off"));
    assert_eq!(stale(), Some(BackupStale::Old));
    result::<BackupReport>(&finish(&p.t, &p.t.ok::<IoBackupAuto>(Empty {}).job.unwrap()));
    assert_eq!(stale(), None);
    clock.advance(7 * DAY + HOUR);
    assert_eq!(stale(), Some(BackupStale::Old));

    // A failure wins over the rest.
    set(&p.t, super::backup_auto::SCHEDULE_KEY, serde_json::json!("daily"));
    let auto = p.data.join("backups").join("auto");
    std::fs::rename(&auto, p.data.join("backups").join("moved")).unwrap();
    std::fs::write(&auto, b"not a folder").unwrap();
    assert_eq!(finish(&p.t, &tick().expect("due")).status, JobStatus::Failed);
    assert_eq!(stale(), Some(BackupStale::Failed));
}

#[test]
fn staleness_rule() {
    use super::backup_auto::staleness;
    let b = |at: i64| VerifiedBackup {
        path: "x".into(),
        created_at: Timestamp(at),
        verified_at: Timestamp(at),
        bytes: 1,
        complete: true,
        automatic: true,
    };
    let now = Timestamp(100 * DAY);
    let daily = AutoBackupSchedule::Daily;
    assert_eq!(staleness(daily, None, false, now), Some(BackupStale::Never));
    assert_eq!(staleness(daily, Some(&b(now.0)), true, now), Some(BackupStale::Failed));
    assert_eq!(staleness(daily, Some(&b(now.0 - 2 * DAY)), false, now), None);
    assert_eq!(staleness(daily, Some(&b(now.0 - 2 * DAY - 1)), false, now), Some(BackupStale::Old));
    assert_eq!(staleness(AutoBackupSchedule::Weekly, Some(&b(now.0 - 14 * DAY)), false, now), None);
    assert_eq!(staleness(AutoBackupSchedule::Off, Some(&b(now.0 - 7 * DAY - 1)), false, now), Some(BackupStale::Old));
}

#[test]
fn without_a_collection_folder_there_are_no_automatic_backups() {
    let t = TestApp::mem();
    assert!(super::auto_backup_tick(&t.app, true).unwrap().is_none());
    let st = t.ok::<IoBackupStatus>(Empty {});
    assert!(st.folder.is_none() && st.next_due.is_none());
    // The module switch turns them off too.
    let p = profile();
    p.t.ok::<ModulesSetEnabled>(fc_api::v1::modules::SetModuleEnabled {
        id: "io".into(),
        enabled: false,
    });
    assert!(super::auto_backup_tick(&p.t.app, true).unwrap().is_none());
}

/// v1.4 Z2: the sanitizers run per chunk of a streamed restore; chunk by chunk they do what
/// they do to the whole dump.
#[test]
fn sanitizing_chunk_by_chunk_equals_sanitizing_the_whole_dump() {
    use super::backup::{sanitize, sanitize_rows};
    use fc_storage::repo::{SqlValue, TableDump};
    let text = |s: &str| SqlValue::Text(s.into());
    let table = |name: &str, columns: &[&str], rows: Vec<Vec<SqlValue>>| TableDump {
        name: name.into(),
        columns: columns.iter().map(|c| c.to_string()).collect(),
        rows,
    };
    let settings: Vec<Vec<SqlValue>> = (0..9)
        .map(|i| {
            let key = if i % 3 == 0 {
                crate::settings::EXEC_SETTINGS[0].to_owned()
            } else {
                format!("k{i}")
            };
            vec![text("app"), text(&key), text("\"/bin/sh\"")]
        })
        .collect();
    let whole = CollectionDump {
        schema_versions: vec![1],
        tables: vec![
            table("settings", &["scope", "key", "value"], settings),
            table(
                "mcp_servers",
                &["id", "approved"],
                vec![
                    vec![text("a"), text("yes")],
                    vec![text("b"), SqlValue::Null],
                ],
            ),
            table(
                "api_clients",
                &["id", "status", "scopes"],
                vec![vec![text("c"), text("approved"), text("[\"x\"]")]],
            ),
            table(
                "subtitle_capture_log",
                &["seq"],
                vec![vec![SqlValue::Integer(1)], vec![SqlValue::Integer(2)]],
            ),
            table(
                "subtitle_keys",
                &["id", "created_at", "revoked_at"],
                vec![vec![text("k"), SqlValue::Integer(5), SqlValue::Null]],
            ),
            table("notes", &["id"], vec![vec![text("n")]]),
        ],
    };
    let mut want = whole.clone();
    sanitize(&mut want);
    assert_ne!(want, whole, "the fixture has something to forget");
    // The same rows in chunks of two, as a streamed load hands them over.
    let mut got = whole.clone();
    for t in &mut got.tables {
        let mut kept = Vec::new();
        for chunk in t.rows.chunks(2) {
            let mut part = TableDump {
                name: t.name.clone(),
                columns: t.columns.clone(),
                rows: chunk.to_vec(),
            };
            sanitize_rows(&mut part).unwrap();
            assert_eq!((&part.name, &part.columns), (&t.name, &t.columns));
            kept.extend(part.rows);
        }
        t.rows = kept;
    }
    assert_eq!(got, want);
}

/// v1.4 Z2: a restore at this build's schema streams the dump through the sanitizers into
/// the tables (an exec setting in the backup does not come back) and reports the rows it
/// kept.
#[test]
fn a_streamed_restore_forgets_exec_settings_and_counts_what_it_kept() {
    let p = profile();
    fill(&p.t, p._dir.path());
    let path = p._dir.path().join("b.fcbackup");
    create(&p.t, &path);
    // The same backup with an exec setting slipped in.
    let b = fc_interop::fcbackup::read(&std::fs::read(&path).unwrap()).unwrap();
    let mut d: CollectionDump = serde_json::from_slice(&b.collection).unwrap();
    let settings = d.tables.iter_mut().find(|t| t.name == "settings").unwrap();
    let col = |n: &str| settings.columns.iter().position(|c| c == n).unwrap();
    let mut row = vec![fc_storage::repo::SqlValue::Null; settings.columns.len()];
    row[col("scope")] = fc_storage::repo::SqlValue::Text("app".into());
    row[col("key")] = fc_storage::repo::SqlValue::Text(crate::settings::EXEC_SETTINGS[0].into());
    row[col("value")] = fc_storage::repo::SqlValue::Text("\"/bin/sh\"".into());
    row[col("updated_at")] = fc_storage::repo::SqlValue::Integer(1);
    settings.rows.push(row);
    let header = fc_interop::fcbackup::BackupHeader {
        created_at: b.manifest.created_at,
        schema_versions: d.schema_versions.clone(),
        counts: d.row_counts(),
    };
    let (bytes, _) =
        fc_interop::fcbackup::write(header, &serde_json::to_vec(&d).unwrap(), &b.media).unwrap();
    let forged = p._dir.path().join("forged.fcbackup");
    std::fs::write(&forged, bytes).unwrap();

    let fresh = profile();
    let job = fresh.t.ok::<IoBackupRestore>(RestoreBackup {
        path: s(&forged),
        safety_backup_path: None,
    });
    let rr: RestoreReport = result(&finish(&fresh.t, &job));
    let after = dump(&fresh.t);
    let kept = table(&after, "settings");
    assert_eq!(
        kept.len() + 1,
        table(&d, "settings").len(),
        "the exec setting is gone"
    );
    assert_eq!(rr.counts["settings"], kept.len() as u64);
    assert_eq!(
        rr.counts["review_log"],
        table(&d, "review_log").len() as u64
    );
    assert_eq!(essentials(&fresh.t), essentials(&p.t));
}

// ------------------------------------------------------------------ mirror (v1.4.1, H1)

fn backup_files(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    let name = p.file_name().unwrap().to_string_lossy().into_owned();
                    name.starts_with(prefix) && name.ends_with(".fcbackup")
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

#[test]
fn every_backup_is_also_copied_to_the_mirror_verified_there_and_rotated() {
    let clock = MovableClock::new();
    let p = profile_with(Some(clock.clone()));
    fill(&p.t, p._dir.path());
    let mirror = tempfile::tempdir().unwrap();
    let m = mirror.path();
    assert!(p.t.ok::<IoBackupStatus>(Empty {}).mirror.is_none(), "off by default");
    set(&p.t, super::backup_mirror::MIRROR_KEY, serde_json::json!(s(m)));
    let st = p.t.ok::<IoBackupStatus>(Empty {}).mirror.unwrap();
    assert_eq!((st.state, st.copies.len()), (MirrorState::Ok, 0));
    assert_eq!(st.folder, s(m));
    let tick = || super::auto_backup_tick(&p.t.app, true).unwrap();

    // Automatic: the same file at home and in the mirror.
    let rep: BackupReport = result(&finish(&p.t, &tick().expect("due")));
    let copy = rep.mirror.clone().unwrap();
    assert_eq!(copy.error, None);
    let copied = PathBuf::from(copy.path.unwrap());
    assert_eq!(backup_files(m, "auto-"), vec![copied.clone()]);
    assert_eq!(std::fs::read(&copied).unwrap(), std::fs::read(&rep.path).unwrap());
    assert!(!part_path(&copied).exists());
    // …and it is a backup that restores and matches.
    let job = p.t.ok::<IoBackupVerify>(VerifyBackup { path: s(&copied) });
    assert!(result::<BackupVerifyReport>(&finish(&p.t, &job)).ok);

    // Manual: copied as `manual-…`.
    let rep = create(&p.t, &p._dir.path().join("hand.fcbackup"));
    let manual = PathBuf::from(rep.mirror.unwrap().path.unwrap());
    assert_eq!(backup_files(m, "manual-"), vec![manual.clone()]);
    assert_eq!(
        std::fs::read(&manual).unwrap(),
        std::fs::read(p._dir.path().join("hand.fcbackup")).unwrap()
    );

    // The same keep count as at home: keep 2 → two automatic copies in both places.
    set(&p.t, super::backup_auto::KEEP_KEY, serde_json::json!(2));
    for _ in 0..3 {
        clock.advance(DAY + HOUR);
        result::<BackupReport>(&finish(&p.t, &tick().expect("due")));
    }
    assert_eq!(auto_files(&p.data).len(), 2);
    let autos = backup_files(m, "auto-");
    assert_eq!(autos.len(), 2, "{autos:?}");
    assert!(!autos.contains(&copied), "the oldest went first");
    let st = p.t.ok::<IoBackupStatus>(Empty {}).mirror.unwrap();
    assert_eq!(st.state, MirrorState::Ok);
    assert_eq!(st.copies.len(), 3);
    assert!(st.last_failure.is_none());
    assert!(st.copies[0].automatic && !st.copies[2].automatic, "newest first");
    // The rescue of a damaged collection (H2) looks in the mirror too: the folder is
    // remembered beside the backups, not only in the (possibly damaged) collection.
    let candidates = crate::modules::maintenance::rescue::backup_candidates(&p.data);
    assert!(
        autos.iter().all(|a| candidates.contains(a)),
        "{candidates:?}"
    );
    assert!(!st.disk_full);
    // No probe file or `.part` is left behind.
    let names: Vec<String> = std::fs::read_dir(m)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
}

#[test]
fn a_missing_mirror_never_fails_the_backup_and_says_so() {
    let clock = MovableClock::new();
    let p = profile_with(Some(clock.clone()));
    fill(&p.t, p._dir.path());
    let root = tempfile::tempdir().unwrap();
    // A drive that is not plugged in: the folder is not created on the wrong disk.
    let gone = root.path().join("unplugged");
    set(&p.t, super::backup_mirror::MIRROR_KEY, serde_json::json!(s(&gone)));
    let tick = || super::auto_backup_tick(&p.t.app, true).unwrap();
    let rep: BackupReport = result(&finish(&p.t, &tick().expect("due")));
    assert!(rep.verified, "the backup itself succeeded");
    assert_eq!(auto_files(&p.data).len(), 1);
    let copy = rep.mirror.unwrap();
    assert!(copy.path.is_none() && copy.error.is_some());
    assert!(!gone.exists());
    let st = p.t.ok::<IoBackupStatus>(Empty {});
    assert!(st.last_failure.is_none(), "the backup did not fail");
    let mirror = st.mirror.unwrap();
    assert_eq!(mirror.state, MirrorState::Missing);
    assert!(mirror.last_failure.is_some());

    // Other folders that are no second place.
    for (dir, want) in [
        ("relative/folder".to_owned(), MirrorState::Missing),
        (s(&p.data.join("backups")), MirrorState::InsideCollection),
    ] {
        set(&p.t, super::backup_mirror::MIRROR_KEY, serde_json::json!(dir));
        let st = p.t.ok::<IoBackupStatus>(Empty {}).mirror.unwrap();
        assert_eq!(st.state, want, "{dir}");
    }
    // Inside the collection folder: nothing is copied there.
    let rep = create(&p.t, &p._dir.path().join("x.fcbackup"));
    assert!(rep.mirror.unwrap().error.is_some());
    assert!(backup_files(&p.data.join("backups"), "manual-").is_empty());

    // The drive is back: the next backup is copied and the failure is cleared.
    std::fs::create_dir_all(&gone).unwrap();
    set(&p.t, super::backup_mirror::MIRROR_KEY, serde_json::json!(s(&gone)));
    clock.advance(DAY + HOUR);
    let rep: BackupReport = result(&finish(&p.t, &tick().expect("due")));
    assert_eq!(rep.mirror.unwrap().error, None);
    assert_eq!(backup_files(&gone, "auto-").len(), 1);
    let mirror = p.t.ok::<IoBackupStatus>(Empty {}).mirror.unwrap();
    assert_eq!((mirror.state, mirror.copies.len()), (MirrorState::Ok, 1));
    assert!(mirror.last_failure.is_none());
}

/// v1.4.1: a preset row holding FSRS-4.5 weights (stored before v1.4.1, or restored from an
/// old backup) used to fail as soon as a card of its decks was graded ("FSRS-6 needs 21
/// weights"). It now reads as FSRS-6 and every card studies.
#[test]
fn a_stored_preset_with_old_fsrs_weights_studies_without_error() {
    use fc_api::v1::presets::{AssignPreset, PresetRef};
    use fc_api::v1::study::{AnswerCard, SessionRef, StartSession, StudyKind, StudyMode};
    let p = profile();
    fill(&p.t, p._dir.path());
    let id = fc_core::PresetId::from_uuid(uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0a11));
    let fsrs45 = vec![
        0.4872, 1.4003, 3.7145, 13.8206, 5.1618, 1.2298, 0.8975, 0.031, 1.6474, 0.1367, 1.0461,
        2.1072, 0.0793, 0.3246, 1.587, 0.2272, 2.8755,
    ];
    {
        let mut tx = p.t.store.write().unwrap();
        tx.upsert_preset(&fc_core::preset::Preset {
            id,
            name: "Old".into(),
            config: fc_core::preset::PresetConfig {
                fsrs_weights: fsrs45,
                ..Default::default()
            },
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
        })
        .unwrap();
        tx.commit().unwrap();
    }
    let decks: Vec<_> = p.t.store.read().unwrap().list_decks(false).unwrap();
    p.t.ok::<PresetsAssign>(AssignPreset {
        deck_ids: decks.iter().map(|d| d.id).collect(),
        preset_id: Some(id),
    });
    let got = p.t.ok::<PresetsGet>(PresetRef { id });
    assert_eq!(got.config.fsrs_weights.len(), 21);
    assert_eq!(got.config.validate(), Ok(()));
    let mut s = p.t.ok::<StudyStart>(StartSession {
        kind: StudyKind::Normal,
        mode: Some(StudyMode::Scheduled),
        deck_id: None,
        include_subdecks: None,
        filter: None,
        from_session: None,
        new_limit: Some(100),
        order: None,
        tz_offset_mins: Some(0),
        modes: None,
        scope: None,
    });
    let mut n = 0;
    while let Some(c) = s.current.clone() {
        n += 1;
        s = p.t.ok::<StudyAnswer>(AnswerCard {
            typed_answer: None,
            session_id: s.id,
            card_id: c.card_id,
            rating: if n % 2 == 0 { Rating::Again } else { Rating::Good },
            duration_ms: 3_000,
            reveal_ms: None,
        });
        assert!(n < 200);
    }
    assert!(n > 0, "the fixture has cards to study");
    p.t.ok::<StudyEnd>(SessionRef { session_id: s.id });
}
