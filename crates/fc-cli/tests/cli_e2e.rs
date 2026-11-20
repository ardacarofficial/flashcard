//! End-to-end: runs the real `fc` binary against a temporary SQLite file. Every step is a
//! separate process, so this also proves the state (and the oplog for undo) is persisted.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

fn fc(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fc"))
        .arg("--db")
        .arg(db)
        .args(args)
        .env_remove("FC_DB")
        .output()
        .expect("run fc")
}

/// Runs `fc` and returns stdout, failing with stderr on a non-zero exit.
fn ok(db: &Path, args: &[&str]) -> String {
    let o = fc(db, args);
    assert!(
        o.status.success(),
        "fc {args:?} failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).unwrap()
}

fn json(db: &Path, args: &[&str]) -> Value {
    let mut all = vec!["--json"];
    all.extend_from_slice(args);
    serde_json::from_str(&ok(db, &all)).unwrap()
}

fn note_fronts(db: &Path) -> Vec<String> {
    json(db, &["note", "list"])["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["sort_field"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn deck_note_attr_undo_roundtrip_on_a_temp_db() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("collection.db");

    // Decks: create (missing parent built) and list.
    ok(&db, &["deck", "create", "Dil::İngilizce"]);
    ok(&db, &["deck", "create", "Matematik"]);
    let tree = ok(&db, &["deck", "list"]);
    assert!(tree.contains("Dil"), "{tree}");
    assert!(tree.contains("  İngilizce"), "child is indented: {tree}");
    assert!(tree.contains("Matematik"), "{tree}");

    // Classification on the parent deck, used on a note in the child deck.
    let mut define = vec![
        "attribute",
        "define",
        "--name",
        "Seviye",
        "--kind",
        "ordered-enum",
        "--deck",
        "Dil",
    ];
    for l in ["A1", "A2", "B1", "B2", "C1", "C2"] {
        define.extend_from_slice(&["--option", l]);
    }
    ok(&db, &define);

    let added = json(
        &db,
        &[
            "note",
            "add",
            "--deck",
            "Dil::İngilizce",
            "--front",
            "run",
            "--back",
            "koşmak",
            "--tags",
            "fiil",
            "--attr",
            "seviye=B1",
        ],
    );
    let b1 = added["attributes"][0]["value"]["value"].as_str().unwrap();
    let level = json(&db, &["attribute", "list", "--deck", "Dil::İngilizce"]);
    let opts = level[0]["options"].as_array().unwrap();
    let b1_label = opts.iter().find(|o| o["id"] == b1).unwrap()["label"].clone();
    assert_eq!(b1_label, "B1");
    assert_eq!(note_fronts(&db), ["run"]);
    // The classification does not apply outside its deck group.
    assert!(!fc(
        &db,
        &[
            "note",
            "add",
            "--deck",
            "Matematik",
            "--front",
            "x",
            "--attr",
            "seviye=B1"
        ]
    )
    .status
    .success());
    // Unknown option label → error exit, nothing written.
    let bad = fc(
        &db,
        &[
            "note",
            "add",
            "--deck",
            "Dil::İngilizce",
            "--front",
            "walk",
            "--attr",
            "seviye=Z9",
        ],
    );
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("NOT_FOUND_ATTRIBUTE_OPTION"));
    assert_eq!(note_fronts(&db), ["run"]);

    // Undo (in a new process) removes the note; redo brings it back; undo twice more removes
    // the note and the classification.
    ok(&db, &["undo"]);
    assert!(note_fronts(&db).is_empty());
    ok(&db, &["redo"]);
    assert_eq!(note_fronts(&db), ["run"]);
    ok(&db, &["undo"]);
    ok(&db, &["undo"]);
    let attrs = json(&db, &["attribute", "list", "--deck", "Dil::İngilizce"]);
    assert!(attrs.as_array().unwrap().is_empty());
    // Decks are still there.
    assert!(ok(&db, &["deck", "list"]).contains("İngilizce"));
}

/// Reviewer scenario: nested decks, CEFR on the parent, a note in the child classified by key
/// *and* by localised name, option rename, undo ×2, redo — every step a separate process.
#[test]
fn cefr_on_parent_option_rename_undo_redo() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("collection.db");

    ok(&db, &["deck", "create", "Dil"]);
    ok(&db, &["deck", "create", "İngilizce", "--parent", "Dil"]);
    ok(
        &db,
        &["deck", "create", "Fiiller", "--parent", "Dil::İngilizce"],
    );
    assert!(ok(&db, &["deck", "list"]).contains("    Fiiller"));

    let cefr = json(
        &db,
        &[
            "attribute",
            "apply-template",
            "cefr",
            "--deck",
            "Dil",
            "--locale",
            "tr",
        ],
    );
    assert_eq!(cefr["key"], "level");
    assert_eq!(cefr["name"], "Seviye");

    // `--attr` takes the key or the localised name, case-insensitively.
    let by_name = json(
        &db,
        &[
            "note",
            "add",
            "--deck",
            "Dil::İngilizce::Fiiller",
            "--front",
            "run",
            "--attr",
            "seviye=B1",
        ],
    );
    let by_key = json(
        &db,
        &[
            "note",
            "add",
            "--deck",
            "Dil::İngilizce::Fiiller",
            "--front",
            "go",
            "--attr",
            "LEVEL=a2",
        ],
    );
    assert_eq!(by_name["attributes"][0]["attr_id"], cefr["id"]);
    assert_eq!(by_key["attributes"][0]["attr_id"], cefr["id"]);
    let b1 = by_name["attributes"][0]["value"]["value"].clone();

    // Rename B1 → B1+ (id unchanged, the note still points at it).
    let renamed = ok(
        &db,
        &["attribute", "option", "rename", "level", "B1", "B1+"],
    );
    assert!(renamed.contains("B1+"), "{renamed}");
    let level = json(
        &db,
        &["attribute", "list", "--deck", "Dil::İngilizce::Fiiller"],
    );
    let label_of_b1 = level[0]["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == b1)
        .unwrap()["label"]
        .clone();
    assert_eq!(label_of_b1, "B1+");

    // Undo ×2 (rename, then the second note), translated output; redo brings the note back.
    assert_eq!(
        ok(&db, &["--lang", "en", "undo"]).trim(),
        "Undone: Seviye: renamed an option"
    );
    assert_eq!(
        ok(&db, &["--lang", "tr", "undo"]).trim(),
        "Geri alındı: Not eklendi: go"
    );
    assert_eq!(note_fronts(&db).len(), 1);
    let level = json(&db, &["attribute", "list", "--deck", "Dil"]);
    assert!(level[0]["options"]
        .as_array()
        .unwrap()
        .iter()
        .any(|o| o["label"] == "B1"));
    assert_eq!(
        ok(&db, &["--lang", "tr", "redo"]).trim(),
        "Yinelendi: Not eklendi: go"
    );
    let mut fronts = note_fronts(&db);
    fronts.sort();
    assert_eq!(fronts, ["go", "run"]);

    // The generic escape hatch: any public method, internal ones refused like over IPC.
    let tags = json(&db, &["call", "tags.list"]);
    assert!(tags["tags"].is_array());
    let denied = fc(
        &db,
        &[
            "call",
            "decks.restore_affected",
            r#"{"decks":[],"notes":[],"cards":[]}"#,
        ],
    );
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("PERMISSION_DENIED"));
}

/// Review fix: emptying the trash writes a history barrier. Undo then stops there with
/// `CONFLICT_HISTORY_BARRIER` (not `NOT_FOUND_NOTE` on the purged note), the older history
/// stays listed, and actions after the barrier undo normally.
#[test]
fn purge_writes_a_history_barrier_that_undo_stops_at() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("collection.db");
    ok(&db, &["deck", "create", "Dil"]);
    ok(
        &db,
        &[
            "note", "add", "--deck", "Dil", "--front", "dog", "--back", "köpek",
        ],
    );
    ok(
        &db,
        &[
            "note", "add", "--deck", "Dil", "--front", "cat", "--back", "kedi",
        ],
    );
    let dog = json(&db, &["note", "list"])["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["sort_field"] == "dog")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(&db, &["note", "delete", &dog]);
    let purged = json(
        &db,
        &["call", "browse.purge_trash", r#"{"older_than_days":0}"#],
    );
    assert_eq!(purged["notes"], 1, "{purged}");

    // Undo would have restored the purged note: it stops at the barrier instead.
    let o = fc(&db, &["--json", "undo"]);
    assert!(!o.status.success());
    let err: Value = serde_json::from_slice(&o.stderr).unwrap();
    assert_eq!(err["code"], "CONFLICT_HISTORY_BARRIER", "{err}");
    assert_eq!(err["message_key"], "errors.conflict.history_barrier");
    assert_eq!(err["details"]["reason"], "purge");
    let status = json(&db, &["call", "oplog.status"]);
    assert_eq!(status["can_undo"], false, "{status}");

    // History before the barrier is still listed.
    let log = json(&db, &["call", "oplog.list"]);
    let commands: Vec<&str> = log["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["command"].as_str().unwrap())
        .collect();
    assert_eq!(commands[0], "v1.oplog.barrier", "{commands:?}");
    assert!(commands.contains(&"v1.notes.delete"), "{commands:?}");
    assert_eq!(
        commands.iter().filter(|c| **c == "v1.notes.create").count(),
        2
    );

    // A new action after the barrier is an ordinary undo step; the barrier holds again.
    ok(
        &db,
        &[
            "note", "add", "--deck", "Dil", "--front", "bird", "--back", "kuş",
        ],
    );
    assert!(ok(&db, &["undo"]).contains("bird"));
    assert_eq!(note_fronts(&db), vec!["cat".to_owned()]);
    let o = fc(&db, &["--json", "undo"]);
    assert!(!o.status.success());
    let err: Value = serde_json::from_slice(&o.stderr).unwrap();
    assert_eq!(err["code"], "CONFLICT_HISTORY_BARRIER");
    // Purging the undone "bird" (now in the trash) writes a second barrier; an empty trash
    // purges nothing and writes none.
    let barriers = |db: &Path| {
        json(db, &["call", "oplog.list"])["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["command"] == "v1.oplog.barrier")
            .count()
    };
    let purge = |db: &Path| {
        json(
            db,
            &["call", "browse.purge_trash", r#"{"older_than_days":0}"#],
        )
    };
    assert_eq!(purge(&db)["notes"], 1);
    assert_eq!(barriers(&db), 2);
    assert_eq!(purge(&db)["notes"], 0);
    assert_eq!(barriers(&db), 2);
}

fn default_preset(db: &Path) -> String {
    json(
        db,
        &["call", "presets.create", r#"{"name":"P","config":null}"#],
    )["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// `optimizer.run` answers `{"job": …}`: the CLI must still run the queued job now and print
/// the finished one — not leave it queued for the next start to mark `interrupted`.
#[test]
fn a_wrapped_job_response_is_run_to_completion() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("collection.db");
    ok(&db, &["deck", "list"]);
    let params = serde_json::json!({ "preset_id": default_preset(&db) }).to_string();
    let job = json(&db, &["call", "optimizer.run", &params]);
    assert_eq!(job["kind"], "optimizer.run", "{job}");
    assert_ne!(job["status"], "queued", "{job}");
    // …and nothing is left behind to be interrupted by the next process.
    let conn = rusqlite::Connection::open(&db).unwrap();
    let open: i64 = conn
        .query_row(
            "SELECT count(*) FROM jobs WHERE status IN ('queued','running','interrupted')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(open, 0);
}

/// A second process must not touch what the app that owns the collection is doing: a job
/// that is running there stays running when `fc` starts.
#[test]
fn fc_does_not_interrupt_jobs_another_process_runs() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("collection.db");
    ok(&db, &["deck", "list"]);
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "INSERT INTO jobs (id, kind, status, created_at, updated_at)
         VALUES ('01J00000000000000000000000', 'import', 'running', 1, 1)",
        [],
    )
    .unwrap();
    ok(&db, &["deck", "list"]);
    let status: String = conn
        .query_row("SELECT status FROM jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "running");
}

/// An old schema is refused (never upgraded behind the app's back) unless `--migrate`.
#[test]
fn an_older_schema_needs_an_explicit_migrate() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("collection.db");
    ok(&db, &["deck", "list"]);
    let conn = rusqlite::Connection::open(&db).unwrap();
    // Forget the newest migration: the file now looks one version behind.
    let (version, table) = ("MAX(version)", "schema_migrations");
    let newest: i64 = conn
        .query_row(&format!("SELECT {version} FROM {table}"), [], |r| r.get(0))
        .unwrap();
    conn.execute(&format!("DELETE FROM {table} WHERE version = {newest}"), [])
        .unwrap();
    drop(conn);
    let refused = fc(&db, &["deck", "list"]);
    assert!(!refused.status.success());
    let err = String::from_utf8_lossy(&refused.stderr);
    assert!(
        err.contains("IO_DATABASE") && err.contains("migrated"),
        "{err}"
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    let still: i64 = conn
        .query_row(
            &format!("SELECT count(*) FROM {table} WHERE version = {newest}"),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(still, 0, "refusing must not migrate");
}

/// `fc … | head` closes the pipe early: no panic, no noise.
#[test]
fn a_closed_stdout_pipe_is_not_a_panic() {
    use std::process::Stdio;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("collection.db");
    for i in 0..40 {
        ok(
            &db,
            &["deck", "create", &format!("Deck {i} {}", "x".repeat(50))],
        );
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_fc"))
        .arg("--db")
        .arg(&db)
        .args(["deck", "list"])
        .env_remove("FC_DB")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take()); // the reader goes away before fc writes
    let out = child.wait_with_output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("panicked"), "{err}");
    assert_eq!(out.status.code(), Some(0), "{err}");
}
