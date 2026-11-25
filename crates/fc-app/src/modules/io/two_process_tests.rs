//! v1.4.2 K1 (review finding 9): the desktop app starts while another process — the `fc`
//! command line — is in the middle of a long import. Two real processes on one SQLite
//! collection: the child is the command line (`App::open_sqlite_shared`, a chunked import it
//! runs itself), the test is the desktop app (`App::open_sqlite`: the full startup).
//!
//! * The importer is alive: the startup must leave its undo group and its job alone; the
//!   import ends complete, as one undo step.
//! * The importer died between chunks: the startup removes what it committed and marks its
//!   job interrupted, as before.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::App;
use fc_api::v1::common::Empty;
use fc_api::v1::decks::CreateDeck;
use fc_api::v1::io::{DuplicatePolicy, ImportFormat, ImportSource, StartImport};
use fc_api::v1::jobs::{JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::oplog::Actor;
use fc_api::ApiMethod;
use fc_core::{DeckId, JobId};
use fc_storage::repo::NoteFilter;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Collection folder of the child.
const DIR_VAR: &str = "FC_TWO_PROCESS_DIR";
/// `live` (waits for the parent, then finishes) or `die` (aborts between chunks).
const ROLE_VAR: &str = "FC_TWO_PROCESS_ROLE";
const DB: &str = "collection.db";
/// Written by the child once chunks are committed: the job id.
const READY: &str = "ready";
/// Written by the parent: the child may go on.
const GO: &str = "go";
/// Written by the child at its end: the job's final status.
const DONE: &str = "done";
const ROWS: usize = 120;
const CHUNK: usize = 10;
/// Committed chunks when the app starts.
const PAUSE_AT: usize = 3;
/// Generous: the machine may be busy. Nothing below depends on how long a step takes.
const PATIENCE: Duration = Duration::from_secs(300);

fn call<M: ApiMethod>(app: &App, actor: &Actor, request: &M::Request) -> M::Response {
    let value = app
        .invoke(
            &format!("{}.{}", fc_api::API_VERSION, M::NAME),
            serde_json::to_value(request).unwrap(),
            actor,
        )
        .unwrap_or_else(|e| panic!("{}: {e:?}", M::NAME));
    serde_json::from_value(value).unwrap()
}

fn wait_for(path: &Path, mut alive: impl FnMut() -> bool) {
    let start = Instant::now();
    while !path.exists() {
        assert!(alive(), "the other process ended before {}", path.display());
        assert!(start.elapsed() < PATIENCE, "{} never appeared", path.display());
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn notes(app: &App) -> usize {
    let tx = app.store().read().unwrap();
    tx.list_notes(
        &NoteFilter::default(),
        &fc_core::paging::PageRequest {
            cursor: None,
            limit: Some(500),
        },
    )
    .unwrap()
    .items
    .len()
}

fn markers(app: &App) -> usize {
    app.store().read().unwrap().list_unfinished_batches().unwrap().len()
}

fn job_status(app: &App, id: JobId) -> JobStatus {
    call::<JobsGet>(app, &Actor::User, &JobRef { id }).status
}

fn rows() -> String {
    let rows: Vec<String> = (0..ROWS)
        .map(|i| format!(r#"{{"fields":{{"front":"q{i}","back":"a"}},"tags":["t{}"]}}"#, i % 5))
        .collect();
    format!("[{}]", rows.join(","))
}

/// The importing process: does nothing unless started by a test below.
#[test]
#[ignore = "two-process child: started by the tests of this file"]
fn importer_child() {
    let (Ok(dir), Ok(role)) = (std::env::var(DIR_VAR), std::env::var(ROLE_VAR)) else {
        return;
    };
    let dir = PathBuf::from(dir);
    // What `fc` does: attach to the collection the app owns.
    let app = App::open_sqlite_shared(&dir.join(DB), false).unwrap();
    let deck: DeckId = std::fs::read_to_string(dir.join("deck")).unwrap().parse().unwrap();
    let job = call::<IoImportStart>(
        &app,
        &Actor::Cli,
        &StartImport {
            source: ImportSource::Text { text: rows() },
            format: Some(ImportFormat::Json),
            text_options: None,
            deck_id: Some(deck),
            notetype_id: None,
            columns: None,
            skip_first_row: None,
            duplicates: Some(DuplicatePolicy::Update),
            create_missing_options: Some(true),
            include_history: None,
        },
    );
    let pause_dir = dir.clone();
    let id = job.id;
    super::import::knobs::set(
        job.id,
        super::import::knobs::Knobs {
            chunk_rows: CHUNK,
            after_chunk: Box::new(move |n, _| {
                if n != PAUSE_AT {
                    return;
                }
                std::fs::write(pause_dir.join(READY), id.to_string()).unwrap();
                if role == "die" {
                    // No destructor, no rollback, no flush: the process is gone.
                    std::process::abort();
                }
                wait_for(&pause_dir.join(GO), || true);
            }),
        },
    );
    app.run_pending_jobs().unwrap();
    let status = serde_json::to_string(&job_status(&app, job.id)).unwrap();
    std::fs::write(dir.join(DONE), status).unwrap();
}

/// A collection with one deck, closed; returns the notes it holds.
fn collection(dir: &Path) -> usize {
    let app = App::open_sqlite(&dir.join(DB)).unwrap();
    let deck = call::<DecksCreate>(
        &app,
        &Actor::User,
        &CreateDeck {
            path: "Base".into(),
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        },
    );
    std::fs::write(dir.join("deck"), deck.id.to_string()).unwrap();
    notes(&app)
}

/// The importing process; stopped when the test ends early (it would wait for `go`).
struct Importer(std::process::Child);

impl Drop for Importer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_importer(dir: &Path, role: &str) -> Importer {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "modules::io::two_process_tests::importer_child",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(DIR_VAR, dir)
        .env(ROLE_VAR, role)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    Importer(child)
}

#[test]
fn app_start_leaves_the_running_import_of_another_process_alone() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    let before = collection(dir);
    let mut child = spawn_importer(dir, "live");
    wait_for(&dir.join(READY), || child.0.try_wait().unwrap().is_none());
    let job: JobId = std::fs::read_to_string(dir.join(READY)).unwrap().parse().unwrap();

    // The desktop app starts: full startup on the collection the importer is writing.
    let app = App::open_sqlite(&dir.join(DB)).unwrap();
    assert_eq!(markers(&app), 1, "the importer's undo group is still open");
    assert_eq!(notes(&app), before + PAUSE_AT * CHUNK, "its committed chunks stay");
    assert_eq!(job_status(&app, job), JobStatus::Running, "its job is not interrupted");

    std::fs::write(dir.join(GO), "").unwrap();
    assert!(child.0.wait().unwrap().success(), "the importer failed");
    assert_eq!(std::fs::read_to_string(dir.join(DONE)).unwrap(), "\"succeeded\"");
    assert_eq!(notes(&app), before + ROWS, "the import is complete");
    assert_eq!(markers(&app), 0);
    assert_eq!(job_status(&app, job), JobStatus::Succeeded);
    // One undo step removes all of it.
    call::<OplogUndo>(&app, &Actor::User, &Empty {});
    assert_eq!(notes(&app), before);
    drop(app);
    // And the next start finds nothing to repair.
    let app = App::open_sqlite(&dir.join(DB)).unwrap();
    assert_eq!(notes(&app), before);
    assert_eq!(job_status(&app, job), JobStatus::Succeeded);
}

#[test]
fn app_start_removes_the_import_of_a_process_that_died() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    let before = collection(dir);
    let mut child = spawn_importer(dir, "die");
    assert!(!child.0.wait().unwrap().success(), "the importer was to die");
    let job: JobId = std::fs::read_to_string(dir.join(READY)).unwrap().parse().unwrap();
    {
        // Seen without the startup work: chunks and a marker were left behind.
        let peek = App::open_sqlite_shared(&dir.join(DB), false).unwrap();
        assert_eq!(markers(&peek), 1);
        assert_eq!(notes(&peek), before + PAUSE_AT * CHUNK);
    }
    let app = App::open_sqlite(&dir.join(DB)).unwrap();
    assert_eq!(markers(&app), 0);
    assert_eq!(notes(&app), before, "the dead importer's chunks are gone");
    assert_eq!(job_status(&app, job), JobStatus::Interrupted);
}
