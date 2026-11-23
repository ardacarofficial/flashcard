//! v1.4.1 H2: the background WAL checkpoint thread (`store.rs`, `Checkpointer`) under a crash
//! and at close.
//!
//! * **Crash during a checkpoint:** a child process (this test binary re-run on one ignored
//!   test) commits large transactions so the thread checkpoints, and aborts itself
//!   (`std::process::abort`: no destructor, no close) while the thread is inside
//!   `PRAGMA wal_checkpoint`, at different moments of different checkpoints. The parent reopens
//!   the file: the integrity check passes and every commit the child saw acknowledged is there.
//! * **Close releases every handle** (the Windows concern: an open handle blocks rename and
//!   delete): after `drop(store)` the checkpoint thread has ended, the files can be renamed and
//!   deleted and their folder removed, and the database reopens. Also when the store is dropped
//!   while the thread is mid-checkpoint (the drop waits for it, bounded).

use crate::store::{CheckpointProbe, SqliteStore, Store};
use fc_core::Timestamp;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Folder the [`crash_child`] process writes in.
const DIR_VAR: &str = "FC_CKPT_CRASH_DIR";
/// When the child aborts: `<checkpoint number>:us:<µs>` or `<checkpoint number>:pct:<% of
/// the previous checkpoint's duration>` after the thread entered the checkpoint.
const PLAN_VAR: &str = "FC_CKPT_CRASH_PLAN";
/// Commits the child saw acknowledged (written after each `commit()` returned).
const ACKED: &str = "acked.txt";
/// Written by the child just before it aborts inside a checkpoint.
const MARKER: &str = "aborted_in_checkpoint.json";

/// Large rows rewrite a few slots (bounds the file); each commit also adds one small row.
const SLOTS: usize = 3;
/// 2 MiB per commit (~512 pages): a checkpoint every second commit or so.
const BLOB_LEN: usize = 2 * 1024 * 1024;
/// The child gives up (exits cleanly, failing the parent) after this many commits.
const MAX_COMMITS: usize = 200;

fn blob(seq: usize) -> String {
    let mut s = format!("{seq:08}:");
    let fill = char::from(b'a' + u8::try_from(seq % 26).unwrap());
    s.extend(std::iter::repeat_n(fill, BLOB_LEN - s.len()));
    s
}

/// Writes `contents` to `dir/name` so a reader never sees it torn (write, then rename).
fn write_atomic(dir: &Path, name: &str, contents: &str) {
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, contents).unwrap();
    std::fs::rename(&tmp, dir.join(name)).unwrap();
}

#[derive(Clone, Copy, Debug)]
enum Delay {
    Micros(u64),
    /// Percent of the previous completed checkpoint's duration.
    Percent(u32),
}

fn parse_plan(s: &str) -> (u64, Delay) {
    let mut it = s.split(':');
    let at = it.next().unwrap().parse().unwrap();
    let delay = match (
        it.next().unwrap(),
        it.next().unwrap().parse::<u64>().unwrap(),
    ) {
        ("us", n) => Delay::Micros(n),
        ("pct", n) => Delay::Percent(u32::try_from(n).unwrap()),
        (k, _) => panic!("bad plan kind {k}"),
    };
    (at, delay)
}

/// Spins until `cond` holds (or `limit` passes); returns whether it held.
fn spin_until(limit: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    while !cond() {
        if Instant::now() >= end {
            return false;
        }
        std::thread::yield_now();
    }
    true
}

/// Aborts the process inside the planned checkpoint (or a later one if that one ended before
/// the chosen moment); writes [`MARKER`] first.
fn watch_and_abort(dir: PathBuf, probe: CheckpointProbe, plan: (u64, Delay)) {
    let (mut target, delay) = plan;
    let mut last_duration = Duration::from_millis(1);
    loop {
        if !spin_until(Duration::from_secs(60), || probe.in_progress()) {
            return;
        }
        let started = Instant::now();
        // The checkpoint the thread is in (`runs` counts completed ones).
        let number = probe.runs() + 1;
        if number >= target {
            let wait = match delay {
                Delay::Micros(us) => Duration::from_micros(us),
                Delay::Percent(p) => last_duration * p / 100,
            };
            spin_until(wait, || false);
            if probe.in_progress() {
                let marker = json!({
                    "checkpoint": number,
                    "after_us": u64::try_from(started.elapsed().as_micros()).unwrap(),
                    "previous_checkpoint_us": u64::try_from(last_duration.as_micros()).unwrap(),
                });
                write_atomic(&dir, MARKER, &marker.to_string());
                if probe.in_progress() {
                    std::process::abort();
                }
                std::fs::remove_file(dir.join(MARKER)).unwrap();
            }
            // Ended before the chosen moment: the next checkpoint.
            target = number + 1;
        }
        spin_until(Duration::from_secs(60), || !probe.in_progress());
        last_duration = started.elapsed().max(Duration::from_micros(1));
    }
}

/// The process that gets killed: does nothing unless started by
/// [`a_crash_during_a_checkpoint_loses_no_acknowledged_commit`].
#[test]
#[ignore = "crash child: started by `a_crash_during_a_checkpoint_loses_no_acknowledged_commit`"]
fn crash_child() {
    let (Ok(dir), Ok(plan)) = (std::env::var(DIR_VAR), std::env::var(PLAN_VAR)) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let store = SqliteStore::open(&dir.join("collection.db")).unwrap();
    let probe = store.checkpoint_probe().expect("file store has the thread");
    let watcher_dir = dir.clone();
    let plan = parse_plan(&plan);
    std::thread::spawn(move || watch_and_abort(watcher_dir, probe, plan));
    for i in 0..MAX_COMMITS {
        let at = Timestamp(i64::try_from(i).unwrap());
        let mut tx = store.write().unwrap();
        tx.set_ux_state(&format!("c{i}"), &json!(i), at).unwrap();
        tx.set_ux_state(&format!("blob{}", i % SLOTS), &json!(blob(i)), at)
            .unwrap();
        tx.commit().unwrap();
        write_atomic(&dir, ACKED, &(i + 1).to_string());
    }
    // Not killed: the parent fails on the clean exit.
}

fn run_child(dir: &Path, plan: &str) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "tests::checkpoint_crash::crash_child",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(DIR_VAR, dir)
        .env(PLAN_VAR, plan)
        .output()
        .unwrap()
}

/// Every acknowledged commit survives an abort inside a checkpoint: the 1st checkpoint right
/// as it starts and a little later, the 2nd at a quarter and three quarters of the 1st's
/// duration, the 3rd at half of the 2nd's.
#[test]
fn a_crash_during_a_checkpoint_loses_no_acknowledged_commit() {
    for plan in ["1:us:0", "1:us:300", "2:pct:25", "2:pct:75", "3:pct:50"] {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let out = run_child(dir, plan);
        let log = || {
            format!(
                "plan {plan}\n{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        };
        assert!(!out.status.success(), "the child must be killed: {}", log());
        let marker: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join(MARKER))
                .unwrap_or_else(|e| panic!("not killed inside a checkpoint ({e}): {}", log())),
        )
        .unwrap();
        let acked: usize = std::fs::read_to_string(dir.join(ACKED))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(acked >= 1, "{}", log());
        // The crash left a WAL behind for recovery to deal with.
        let wal = dir.join("collection.db-wal");
        assert!(
            std::fs::metadata(&wal).is_ok_and(|m| m.len() > 0),
            "{marker}: no WAL left"
        );

        let store = SqliteStore::open(&dir.join("collection.db")).unwrap();
        {
            let r = store.read().unwrap();
            let check = r.integrity_check(false, 100).unwrap();
            assert_eq!(check.rows, ["ok"], "{plan} {marker}");
            assert_eq!(check.foreign_key_violations, 0);
            for i in 0..acked {
                assert_eq!(
                    r.ux_state(&format!("c{i}")).unwrap(),
                    Some(json!(i)),
                    "{plan} {marker}: acknowledged commit {i} of {acked} lost"
                );
            }
            // At most the one commit in flight when the ack was not yet written.
            assert_eq!(r.ux_state(&format!("c{}", acked + 1)).unwrap(), None);
            let landed = if r.ux_state(&format!("c{acked}")).unwrap().is_some() {
                acked + 1
            } else {
                acked
            };
            for slot in 0..SLOTS.min(landed) {
                let last = (0..landed).rev().find(|i| i % SLOTS == slot).unwrap();
                assert_eq!(
                    r.ux_state(&format!("blob{slot}")).unwrap(),
                    Some(json!(blob(last))),
                    "{plan} {marker}: slot {slot} is not commit {last}'s row"
                );
            }
        }
        drop(store);
        eprintln!("checkpoint crash {plan}: {marker}, {acked} commits acknowledged");
    }
}

/// Large commits until the thread has checkpointed at least `runs` times.
fn fill_until_checkpointed(store: &SqliteStore, from: usize, runs: u64) -> usize {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut i = from;
    while store.checkpoints_run().unwrap() < runs {
        assert!(Instant::now() < deadline, "no checkpoint ran");
        if i < from + 8 {
            let mut tx = store.write().unwrap();
            tx.set_ux_state(&format!("blob{i}"), &json!(blob(i)), Timestamp(1))
                .unwrap();
            tx.commit().unwrap();
            i += 1;
        } else {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    i
}

/// Files of the database at `path` (the WAL and shared-memory files when present).
fn db_files(path: &Path) -> Vec<PathBuf> {
    ["db", "db-wal", "db-shm"]
        .into_iter()
        .map(|ext| path.with_extension(ext))
        .filter(|p| p.exists())
        .collect()
}

/// No descriptor of this process points into `dir` (Linux can tell; elsewhere the renames
/// and deletes below are the check, as Windows refuses them on an open file).
fn assert_no_open_files_in(dir: &Path) {
    #[cfg(target_os = "linux")]
    {
        let dir = dir.canonicalize().unwrap();
        for fd in std::fs::read_dir("/proc/self/fd").unwrap().flatten() {
            if let Ok(target) = std::fs::read_link(fd.path()) {
                assert!(
                    !target.starts_with(&dir),
                    "still open after close: {}",
                    target.display()
                );
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = dir;
}

/// Renames every database file away and back (fails on Windows while a handle is open).
fn rename_round_trip(path: &Path) {
    for f in db_files(path) {
        let moved = f.with_file_name(format!(
            "moved-{}",
            f.file_name().unwrap().to_string_lossy()
        ));
        std::fs::rename(&f, &moved).unwrap_or_else(|e| panic!("rename {}: {e}", f.display()));
        std::fs::rename(&moved, &f).unwrap();
    }
}

/// Deletes the database files, then the (now empty) folder.
fn delete_all(tmp: tempfile::TempDir, path: &Path) {
    for f in db_files(path) {
        std::fs::remove_file(&f).unwrap_or_else(|e| panic!("delete {}: {e}", f.display()));
    }
    let dir = tmp.keep();
    std::fs::remove_dir(&dir).unwrap_or_else(|e| panic!("remove {}: {e}", dir.display()));
}

/// After `drop(store)` nothing of the store is left: the checkpoint thread ended, the
/// writer's WAL hook is gone, the last close checkpointed and removed the WAL, no handle is
/// open (the files rename and delete, the folder goes), and the file reopens with its data.
#[test]
fn closing_the_store_releases_every_file_handle() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("collection.db");
    let store = SqliteStore::open(&path).unwrap();
    let probe = store.checkpoint_probe().unwrap();
    let written = fill_until_checkpointed(&store, 0, 1);
    // Every reader of the pool used, two at once.
    for _ in 0..crate::store::READ_POOL_SIZE {
        let a = store.read().unwrap();
        let b = store.read().unwrap();
        assert!(a.ux_state("blob0").unwrap().is_some());
        assert!(b.integrity_check(true, 1).unwrap().rows == ["ok"]);
    }
    drop(store);
    assert!(
        probe.released(),
        "the checkpoint thread or a connection survived the drop"
    );
    assert!(
        !path.with_extension("db-wal").exists(),
        "the last close checkpoints and removes the WAL"
    );
    assert_no_open_files_in(tmp.path());
    rename_round_trip(&path);

    let store = SqliteStore::open(&path).unwrap();
    {
        let r = store.read().unwrap();
        assert_eq!(r.integrity_check(false, 10).unwrap().rows, ["ok"]);
        for i in 0..written {
            assert_eq!(
                r.ux_state(&format!("blob{i}")).unwrap(),
                Some(json!(blob(i)))
            );
        }
    }
    let probe = store.checkpoint_probe().unwrap();
    drop(store);
    assert!(probe.released());
    assert_no_open_files_in(tmp.path());
    rename_round_trip(&path);
    delete_all(tmp, &path);
}

/// Dropping the store while the thread is inside a checkpoint waits for that checkpoint
/// (bounded) and then releases everything, as a clean close does.
#[test]
fn dropping_the_store_mid_checkpoint_joins_the_thread() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("collection.db");
    let store = SqliteStore::open(&path).unwrap();
    let probe = store.checkpoint_probe().unwrap();
    let mut written = 0;
    let mut caught = false;
    // A checkpoint can end before this thread sees it: try a few.
    for _ in 0..10 {
        let before = probe.runs();
        for _ in 0..3 {
            let mut tx = store.write().unwrap();
            tx.set_ux_state(
                &format!("blob{written}"),
                &json!(blob(written)),
                Timestamp(1),
            )
            .unwrap();
            tx.commit().unwrap();
            written += 1;
        }
        if spin_until(Duration::from_secs(5), || {
            probe.in_progress() || probe.runs() > before
        }) && probe.in_progress()
        {
            caught = true;
            break;
        }
    }
    assert!(caught, "never saw the thread inside a checkpoint");
    let started = Instant::now();
    drop(store);
    let took = started.elapsed();
    assert!(took < Duration::from_secs(10), "drop took {took:?}");
    assert!(probe.released(), "the checkpoint thread survived the drop");
    assert!(!probe.in_progress());
    assert!(
        !path.with_extension("db-wal").exists(),
        "WAL left after close"
    );
    assert_no_open_files_in(tmp.path());
    rename_round_trip(&path);

    let store = SqliteStore::open(&path).unwrap();
    {
        let r = store.read().unwrap();
        assert_eq!(r.integrity_check(false, 10).unwrap().rows, ["ok"]);
        for i in 0..written {
            assert_eq!(
                r.ux_state(&format!("blob{i}")).unwrap(),
                Some(json!(blob(i)))
            );
        }
    }
    drop(store);
    delete_all(tmp, &path);
}
