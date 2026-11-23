//! v1.4.1 H2: kill matrix. A real process runs one command on a SQLite collection and is
//! killed (`abort`: no destructor, no rollback by us, no flush) at several points **inside an
//! open write transaction** (`fc_storage::crash_points`: the k-th row change, the k-th
//! `COMMIT` before it is durable) or while a backup's `.part` file is being written. The next
//! start must find a consistent collection:
//!
//! * SQLite `integrity_check` and `foreign_key_check` clean (`maintenance.integrity.check`);
//! * the app's own invariants (`browse.check_database` healthy, every card has its note,
//!   every `media:` reference of a note has its media row and file);
//! * no job left queued/running; every media row has its file, no file without a row, no
//!   `*.part` under `media/`;
//! * the oplog ends with the last acknowledged command — or with the killed one when it was
//!   committed as a whole — and nothing in between: the killed command is absent or complete;
//! * `undo` of the last acknowledged command works after the restart (and `redo` after it),
//!   leaving a consistent collection.
//!
//! Each scenario builds its collection once (parent), runs the command once uninterrupted in
//! a child to **measure** its row changes `R` and commits `C`, then kills a fresh copy at
//! row change 1, `R/2`, `R` and at commit 1, `C/2`, `C` (plus scenario extras), one child per
//! point. The migration scenario arms the kill from the environment (counted from process
//! start), so the open itself — the migrations — is what dies.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::modules::subtitle;
use crate::App;
use base64::Engine as _;
use fc_api::v1::oplog::{Actor, ListOplog};
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_storage::crash_points;
use fc_storage::repo::{CollectionDump, SqlValue};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Output;

/// Collection folder of a child.
const DIR_VAR: &str = "FC_KILL_MATRIX_DIR";
/// Scenario a child runs.
const SCENARIO_VAR: &str = "FC_KILL_MATRIX_SCENARIO";
/// Where a child dies (`Arm::encode`).
const ARM_VAR: &str = "FC_KILL_MATRIX_ARM";
const DB: &str = "collection.db";
/// What the parent acknowledged while building the collection.
const BASE: &str = "base.json";
/// Row changes and commits of the uninterrupted command.
const MEASURED: &str = "measured.json";
/// Written by a child that reached the end of its command although it was armed.
const SURVIVED: &str = "survived";
const MIGRATION: &str = "migration";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arm {
    /// Run uninterrupted, report the counts.
    Measure,
    /// Abort at the k-th row change of the command.
    Row(u64),
    /// Abort inside the k-th commit of the command.
    Commit(u64),
    /// Abort as soon as the backup's `.part` file appears.
    Part,
}

impl Arm {
    fn encode(self) -> String {
        match self {
            Arm::Measure => "measure".into(),
            Arm::Row(k) => format!("row:{k}"),
            Arm::Commit(k) => format!("commit:{k}"),
            Arm::Part => "part".into(),
        }
    }

    fn decode(s: &str) -> Self {
        let n = |v: &str| v.parse::<u64>().unwrap();
        match s.split_once(':') {
            Some(("row", k)) => Arm::Row(n(k)),
            Some(("commit", k)) => Arm::Commit(n(k)),
            _ if s == "part" => Arm::Part,
            _ => Arm::Measure,
        }
    }
}

// ------------------------------------------------------------------ small helpers

fn try_call(app: &App, method: &str, params: Value) -> Result<Value, fc_api::ApiError> {
    app.invoke(&format!("v1.{method}"), params, &Actor::User)
}

fn call(app: &App, method: &str, params: Value) -> Value {
    try_call(app, method, params).unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn write_json(path: &Path, v: &Value) {
    std::fs::write(path, v.to_string()).unwrap();
}

/// Copies a closed collection folder (the `-shm` index is rebuilt by SQLite).
fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let name = e.file_name();
        if name.to_string_lossy().ends_with("-shm") {
            continue;
        }
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &to.join(&name));
        } else {
            std::fs::copy(e.path(), to.join(&name)).unwrap();
        }
    }
}

fn note(app: &App, deck: &Value, front: &str) -> Value {
    call(
        app,
        "notes.create",
        json!({"notetype_id": CLASSIC_ID, "deck_id": deck, "fields": {"front": front, "back": "m"},
               "tags": [], "attributes": [], "allow_duplicate": true}),
    )
}

fn newest_oplog(app: &App) -> Value {
    let page = call(
        app,
        "oplog.list",
        serde_json::to_value(ListOplog::default()).unwrap(),
    );
    page["items"][0].clone()
}

fn dump(app: &App) -> CollectionDump {
    app.store().read().unwrap().dump_collection().unwrap()
}

fn table<'d>(d: &'d CollectionDump, name: &str) -> Option<&'d fc_storage::repo::TableDump> {
    d.tables.iter().find(|t| t.name == name)
}

fn column(t: &fc_storage::repo::TableDump, name: &str) -> Option<usize> {
    t.columns.iter().position(|c| c == name)
}

fn text(v: &SqlValue) -> Option<&str> {
    match v {
        SqlValue::Text(s) => Some(s),
        _ => None,
    }
}

/// Rows of `name` (not deleted ones when the table has `deleted_at`).
fn live_rows(app: &App, name: &str) -> usize {
    let d = dump(app);
    let Some(t) = table(&d, name) else {
        return 0;
    };
    match column(t, "deleted_at") {
        Some(c) => t.rows.iter().filter(|r| r[c] == SqlValue::Null).count(),
        None => t.rows.len(),
    }
}

/// Runs queued jobs and returns the newest job.
fn run_jobs(app: &App) -> Value {
    app.run_pending_jobs().unwrap();
    call(app, "jobs.list", json!({}))["jobs"][0].clone()
}

// ------------------------------------------------------------------ checks after a restart

/// Every invariant a restart must find, whatever was killed. `started`: right after a start
/// (an undone `media.add` keeps its file until the next start, so redo can bring it back).
fn common_checks(app: &App, dir: &Path, label: &str) {
    checks(app, dir, label, true);
}

fn checks(app: &App, dir: &Path, label: &str, started: bool) {
    let integrity = call(app, "maintenance.integrity.check", json!({"quick": false}));
    assert_eq!(integrity["ok"], true, "{label}: integrity {integrity}");
    let report = call(app, "browse.check_database", json!({}));
    assert_eq!(report["healthy"], true, "{label}: check_database {report}");
    for job in call(app, "jobs.list", json!({}))["jobs"]
        .as_array()
        .unwrap()
    {
        assert!(
            job["status"] != "queued" && job["status"] != "running",
            "{label}: a job is still {}: {job}",
            job["status"]
        );
    }

    let d = dump(app);
    // Every card has its note (live or trashed).
    let notes = table(&d, "notes").unwrap();
    let id = column(notes, "id").unwrap();
    let note_ids: HashSet<&str> = notes.rows.iter().filter_map(|r| text(&r[id])).collect();
    let cards = table(&d, "cards").unwrap();
    let card_note = column(cards, "note_id").unwrap();
    for r in &cards.rows {
        let n = text(&r[card_note]).unwrap();
        assert!(note_ids.contains(n), "{label}: card without note {n}");
    }

    // Media: rows ↔ files, no partial file.
    let db = dir.join(DB);
    let media: HashSet<String> = app
        .store()
        .read()
        .unwrap()
        .list_media()
        .unwrap()
        .into_iter()
        .map(|m| m.hash)
        .collect();
    for h in &media {
        let path = fc_storage::media::media_file_path(&db, h).unwrap();
        assert!(path.is_file(), "{label}: media row {h} has no file");
    }
    if let Ok(shards) = std::fs::read_dir(dir.join("media")) {
        for shard in shards.flatten() {
            for f in std::fs::read_dir(shard.path()).unwrap().flatten() {
                let name = f.file_name().to_string_lossy().into_owned();
                assert!(
                    !name.ends_with(".part"),
                    "{label}: partial media file {name}"
                );
                assert!(
                    !started || media.contains(&name),
                    "{label}: media file without row {name}"
                );
            }
        }
    }
    // Every `media:<hash>` a note field names has its row (and so its file).
    let fields = column(notes, "fields").unwrap();
    for r in &notes.rows {
        let f = text(&r[fields]).unwrap_or_default();
        for (i, _) in f.match_indices("media:") {
            let hash: String = f[i + 6..]
                .chars()
                .take_while(char::is_ascii_hexdigit)
                .collect();
            if hash.len() == 64 {
                assert!(
                    media.contains(&hash),
                    "{label}: note names missing media {hash}"
                );
            }
        }
    }
}

/// `undo` of the newest step works after the restart (and is the last acknowledged command
/// when that was a plain command), then `redo`; consistent after each.
fn undo_round_trip(app: &App, dir: &Path, label: &str) {
    let newest = newest_oplog(app);
    let status = call(app, "oplog.status", json!({}));
    assert_eq!(
        status["can_undo"], true,
        "{label}: nothing to undo: {status}"
    );
    let undone = call(app, "oplog.undo", json!({}));
    if newest["kind"] == "do" {
        assert_eq!(
            undone["reverted_batch_id"], newest["batch_id"],
            "{label}: undo reverted another step than the last one"
        );
    }
    checks(app, dir, &format!("{label} after undo"), false);
    call(app, "oplog.redo", json!({}));
    checks(app, dir, &format!("{label} after redo"), false);
}

// ------------------------------------------------------------------ scenarios

/// One scenario: what the parent acknowledges, the command a child runs (and may die in),
/// and what a restart must find (returns whether the killed command left its oplog entry).
struct Scenario {
    name: &'static str,
    setup: fn(&App, &Path) -> Value,
    run: fn(&App, &Path, &Value),
    verify: fn(&App, &Path, &Value, bool) -> bool,
    /// Also kill when the backup's `.part` appears.
    part: bool,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "study",
        setup: study_setup,
        run: study_run,
        verify: study_verify,
        part: false,
    },
    Scenario {
        name: "subtitle",
        setup: subtitle_setup,
        run: subtitle_run,
        verify: subtitle_verify,
        part: false,
    },
    Scenario {
        name: "import",
        setup: import_setup,
        run: import_run,
        verify: import_verify,
        part: false,
    },
    Scenario {
        name: "undo",
        setup: undo_setup,
        run: undo_run,
        verify: undo_verify,
        part: false,
    },
    Scenario {
        name: "redo",
        setup: redo_setup,
        run: redo_run,
        verify: redo_verify,
        part: false,
    },
    Scenario {
        name: "backup",
        setup: backup_setup,
        run: backup_run,
        verify: backup_verify,
        part: true,
    },
    Scenario {
        name: "words",
        setup: words_setup,
        run: words_run,
        verify: words_verify,
        part: false,
    },
];

fn scenario(name: &str) -> &'static Scenario {
    SCENARIOS.iter().find(|s| s.name == name).unwrap()
}

// --- 1. study.answer

fn study_setup(app: &App, _dir: &Path) -> Value {
    let deck = call(app, "decks.create", json!({"path": "Kill"}))["id"].clone();
    for i in 0..8 {
        note(app, &deck, &format!("ön {i}"));
    }
    let mut s = call(
        app,
        "study.start",
        json!({"kind": "normal", "deck_id": deck, "tz_offset_mins": 0}),
    );
    for rating in ["good", "again"] {
        let card = s["current"]["card_id"].clone();
        s = call(
            app,
            "study.answer",
            json!({"session_id": s["id"], "card_id": card, "rating": rating, "duration_ms": 900}),
        );
    }
    json!({"session": s["id"], "card": s["current"]["card_id"], "answered": s["answered"],
           "reviews": live_rows(app, "review_log")})
}

fn study_run(app: &App, _dir: &Path, base: &Value) {
    call(
        app,
        "study.answer",
        json!({"session_id": base["session"], "card_id": base["card"], "rating": "good",
               "duration_ms": 900}),
    );
}

fn study_verify(app: &App, _dir: &Path, base: &Value, completed: bool) -> bool {
    let s = call(app, "study.session", json!({"session_id": base["session"]}));
    let answered = base["answered"].as_u64().unwrap();
    let reviews = base["reviews"].as_u64().unwrap() as usize;
    if completed {
        assert_eq!(s["answered"], answered + 1);
        assert_eq!(live_rows(app, "review_log"), reviews + 1);
    } else {
        // One transaction, killed before it was durable: nothing of it.
        assert_eq!(s["answered"], answered, "{s}");
        assert_eq!(s["current"]["card_id"], base["card"], "{s}");
        assert_eq!(live_rows(app, "review_log"), reviews);
    }
    completed
}

// --- 2. Subtitle capture with media (files written before their rows)

fn jpeg(n: u32) -> Vec<u8> {
    let mut v = b"\xff\xd8\xff\xe0\x00\x10JFIF\x00".to_vec();
    v.extend(format!("kill-matrix-jpeg-{n}").bytes());
    v
}

fn wav(n: u32) -> Vec<u8> {
    let mut v = b"RIFF\x24\x00\x00\x00WAVEfmt ".to_vec();
    v.extend(format!("kill-matrix-wav-{n}").bytes());
    v
}

fn capture_id(n: u32) -> String {
    format!("0192812a-6f3c-7a41-9c2e-{n:012x}")
}

/// Uploads a screenshot and a sound, then sends the capture naming them (what the extension
/// does): three commands.
fn subtitle_capture(app: &App, secret: &str, n: u32, lemma: &str, sentence: &str) {
    let key = subtitle::authenticate(app, &subtitle::key_hash(secret)).unwrap();
    let id = capture_id(n);
    let shot = subtitle::upload(app, &key, &id, "screenshot", "image/jpeg", &jpeg(n))
        .unwrap()
        .upload_id;
    let audio = subtitle::upload(app, &key, &id, "audio", "audio/wav", &wav(n))
        .unwrap()
        .upload_id;
    let v = json!({
      "schema": 1,
      "capture_id": id,
      "kind": "word",
      "lang": { "target": "en", "native": "tr" },
      "term": { "surface": lemma, "lemma": lemma, "pos": "noun" },
      "definition": { "text": "anlam", "source": "dict" },
      "context": { "sentence": sentence, "translation": "çeviri", "term_offset": null },
      "source": {
        "type": "video", "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=83s",
        "title": "Film", "site": "youtube", "video_id": "dQw4w9WgXcQ",
        "t_start_ms": 83450, "t_end_ms": 86120
      },
      "media": {
        "screenshot": {"upload_id": shot, "mime": "image/jpeg"},
        "audio": {"upload_id": audio, "mime": "audio/wav"}
      },
      "deck_id": null,
      "tags": ["subtitle"],
      "created_at": "2026-10-12T10:00:00.000Z",
      "client": { "name": "subtitle-extension", "ext_version": "3.1.3" }
    });
    subtitle::capture(app, &key, &v).unwrap();
}

fn captures(app: &App) -> u64 {
    app.store()
        .read()
        .unwrap()
        .subtitle_capture_stats()
        .unwrap()
        .0
}

fn subtitle_setup(app: &App, _dir: &Path) -> Value {
    call(
        app,
        "modules.set_enabled",
        json!({"id": subtitle::ID, "enabled": true}),
    );
    let secret = call(app, "subtitle.keys.create", json!({"name": "Kill"}))["secret"].clone();
    subtitle_capture(
        app,
        secret.as_str().unwrap(),
        1,
        "key",
        "Almost. I just need to pick up my keys.",
    );
    json!({"secret": secret, "captures": captures(app), "notes": live_rows(app, "notes")})
}

fn subtitle_run(app: &App, _dir: &Path, base: &Value) {
    subtitle_capture(
        app,
        base["secret"].as_str().unwrap(),
        2,
        "door",
        "Close the door, please.",
    );
}

fn subtitle_verify(app: &App, _dir: &Path, base: &Value, completed: bool) -> bool {
    let before = base["captures"].as_u64().unwrap();
    let notes = base["notes"].as_u64().unwrap() as usize;
    let now = captures(app);
    let present = now == before + 1;
    assert!(now == before || present, "captures {before} → {now}");
    assert!(!completed || present);
    // The capture and its note come together (media rows/files: `common_checks`).
    assert_eq!(live_rows(app, "notes"), notes + usize::from(present));
    present
}

// --- 3. Import (a job: chunks in their own transactions, one undo group)

const IMPORT_ROWS: usize = 400;

fn import_setup(app: &App, _dir: &Path) -> Value {
    let deck = call(app, "decks.create", json!({"path": "İçe aktarılan"}))["id"].clone();
    note(app, &deck, "elle");
    json!({"deck": deck, "notes": live_rows(app, "notes")})
}

fn import_run(app: &App, _dir: &Path, base: &Value) {
    let text: String = (0..IMPORT_ROWS)
        .map(|i| format!("yeni kelime {i}\tnew word {i}\n"))
        .collect();
    call(
        app,
        "io.import.start",
        json!({"source": {"kind": "text", "text": text}, "deck_id": base["deck"]}),
    );
    let job = run_jobs(app);
    assert_eq!(job["status"], "succeeded", "{job}");
}

fn import_verify(app: &App, _dir: &Path, base: &Value, completed: bool) -> bool {
    let before = base["notes"].as_u64().unwrap() as usize;
    let now = live_rows(app, "notes");
    let present = now == before + IMPORT_ROWS;
    assert!(
        now == before || present,
        "notes {before} → {now}: a partial import"
    );
    assert!(!completed || present);
    present
}

// --- 4. undo / redo

fn two_notes(app: &App) -> Value {
    let deck = call(app, "decks.create", json!({"path": "Geri al"}))["id"].clone();
    note(app, &deck, "A");
    note(app, &deck, "B");
    deck
}

fn undo_setup(app: &App, _dir: &Path) -> Value {
    two_notes(app);
    json!({"notes": live_rows(app, "notes")})
}

fn undo_run(app: &App, _dir: &Path, _base: &Value) {
    call(app, "oplog.undo", json!({}));
}

fn undo_verify(app: &App, _dir: &Path, base: &Value, completed: bool) -> bool {
    let before = base["notes"].as_u64().unwrap() as usize;
    assert_eq!(live_rows(app, "notes"), before - usize::from(completed));
    completed
}

fn redo_setup(app: &App, _dir: &Path) -> Value {
    two_notes(app);
    call(app, "oplog.undo", json!({}));
    json!({"notes": live_rows(app, "notes")})
}

fn redo_run(app: &App, _dir: &Path, _base: &Value) {
    call(app, "oplog.redo", json!({}));
}

fn redo_verify(app: &App, _dir: &Path, base: &Value, completed: bool) -> bool {
    let before = base["notes"].as_u64().unwrap() as usize;
    assert_eq!(live_rows(app, "notes"), before + usize::from(completed));
    if !completed {
        let status = call(app, "oplog.status", json!({}));
        assert_eq!(
            status["can_redo"], true,
            "the undone step can still be redone: {status}"
        );
    }
    completed
}

// --- 5. Backup (`.part` beside the target, verified by a scratch restore, renamed)

fn target(dir: &Path) -> PathBuf {
    dir.join("out").join("kill.fcbackup")
}

fn backup(app: &App, dir: &Path) -> Value {
    call(
        app,
        "io.backup.create",
        json!({"path": target(dir).to_string_lossy()}),
    );
    run_jobs(app)
}

fn backup_setup(app: &App, dir: &Path) -> Value {
    std::fs::create_dir_all(dir.join("out")).unwrap();
    let deck = call(app, "decks.create", json!({"path": "Yedek"}))["id"].clone();
    for i in 0..3 {
        note(app, &deck, &format!("yedek {i}"));
    }
    let data = base64::engine::general_purpose::STANDARD.encode(jpeg(99));
    call(
        app,
        "media.add",
        json!({"name": "resim.jpg", "data_base64": data}),
    );
    let job = backup(app, dir);
    assert_eq!(job["status"], "succeeded", "{job}");
    let at_backup = live_rows(app, "notes");
    note(app, &deck, "yedekten sonra");
    json!({"at_backup": at_backup, "notes": live_rows(app, "notes")})
}

fn backup_run(app: &App, dir: &Path, _base: &Value) {
    let job = backup(app, dir);
    assert_eq!(job["status"], "succeeded", "{job}");
}

fn inspect_notes(app: &App, dir: &Path) -> u64 {
    let info = call(
        app,
        "io.backup.inspect",
        json!({"path": target(dir).to_string_lossy()}),
    );
    info["counts"]["notes"].as_u64().unwrap()
}

fn backup_verify(app: &App, dir: &Path, base: &Value, completed: bool) -> bool {
    let old = base["at_backup"].as_u64().unwrap();
    let new = base["notes"].as_u64().unwrap();
    // The target is a whole, verified backup: the previous one or the new one.
    call(
        app,
        "io.backup.verify",
        json!({"path": target(dir).to_string_lossy()}),
    );
    let job = run_jobs(app);
    assert_eq!(job["status"], "succeeded", "{job}");
    assert_eq!(job["result"]["ok"], true, "{job}");
    let notes = inspect_notes(app, dir);
    assert!(notes == old || notes == new, "backup holds {notes} notes");
    assert!(!completed || notes == new);
    // The next backup replaces it (and any stale `.part`).
    let job = backup(app, dir);
    assert_eq!(job["status"], "succeeded", "{job}");
    assert_eq!(inspect_notes(app, dir), new);
    let parts: Vec<_> = std::fs::read_dir(dir.join("out"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".part"))
        .collect();
    assert!(parts.is_empty(), "stale {parts:?}");
    false
}

/// Child side of [`Arm::Part`]: aborts as soon as the `.part` of the target exists.
fn abort_when_part_appears(dir: &Path) {
    let mut name = target(dir).file_name().unwrap().to_os_string();
    name.push(".part");
    let part = target(dir).with_file_name(name);
    std::thread::spawn(move || loop {
        if part.exists() {
            std::process::abort();
        }
        std::thread::yield_now();
    });
}

// --- 6. Word index rebuild (a job running `words.index.rebuild_now`)

fn index_dump(app: &App) -> String {
    format!(
        "{:?}",
        app.store().read().unwrap().word_index_dump().unwrap()
    )
}

fn words_setup(app: &App, _dir: &Path) -> Value {
    let deck = call(app, "decks.create", json!({"path": "Kelimeler"}))["id"].clone();
    call(
        app,
        "settings.set",
        json!({"scope": {"kind": "deck", "id": deck}, "key": crate::modules::words::LANGUAGE,
               "value": "en"}),
    );
    for w in ["go", "went", "actor", "keys", "houses", "running", "acting"] {
        note(app, &deck, w);
    }
    let index = index_dump(app);
    assert!(index.len() > 100, "the index has rows: {index}");
    json!({"index": index})
}

fn words_run(app: &App, _dir: &Path, _base: &Value) {
    call(app, "words.index.rebuild", json!({}));
    let job = run_jobs(app);
    assert_eq!(job["status"], "succeeded", "{job}");
}

fn words_verify(app: &App, _dir: &Path, base: &Value, _completed: bool) -> bool {
    // Rebuilt or not, the index is the full rebuild's.
    assert_eq!(index_dump(app), base["index"].as_str().unwrap());
    call(app, "words.index.rebuild_now", json!({}));
    assert_eq!(index_dump(app), base["index"].as_str().unwrap());
    false
}

// ------------------------------------------------------------------ the child

/// The process that gets killed: does nothing unless started by a test below.
#[test]
#[ignore = "kill-matrix child: started by the kill_matrix tests"]
fn matrix_child() {
    let (Ok(dir), Ok(name), Ok(arm)) = (
        std::env::var(DIR_VAR),
        std::env::var(SCENARIO_VAR),
        std::env::var(ARM_VAR),
    ) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let arm = Arm::decode(&arm);
    let db = dir.join(DB);
    if name == MIGRATION {
        // The open is the command: armed from the environment, counted from process start.
        if arm == Arm::Measure {
            let store = fc_storage::SqliteStore::open(&db).unwrap();
            let measured = json!({"rows": crash_points::row_changes(),
                                  "commits": crash_points::commits()});
            drop(store);
            write_json(&dir.join(MEASURED), &measured);
        } else {
            let _app = App::open_sqlite(&db);
            std::fs::write(dir.join(SURVIVED), "").unwrap();
            panic!("the migration was not killed at {arm:?}");
        }
        return;
    }
    let s = scenario(&name);
    let app = App::open_sqlite(&db).unwrap();
    let base = read_json(&dir.join(BASE));
    let (rows, commits) = (crash_points::row_changes(), crash_points::commits());
    match arm {
        Arm::Measure => {}
        Arm::Row(k) => crash_points::crash_after_row_changes(k),
        Arm::Commit(k) => crash_points::crash_at_next_commits(k),
        Arm::Part => abort_when_part_appears(&dir),
    }
    (s.run)(&app, &dir, &base);
    if arm == Arm::Measure {
        let measured = json!({"rows": crash_points::row_changes() - rows,
                              "commits": crash_points::commits() - commits});
        write_json(&dir.join(MEASURED), &measured);
    } else {
        std::fs::write(dir.join(SURVIVED), "").unwrap();
        panic!("{name} was not killed at {arm:?}");
    }
}

fn spawn_child(dir: &Path, name: &str, arm: Arm, env: Option<(&str, u64)>) -> Output {
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "modules::maintenance::kill_matrix_tests::matrix_child",
        "--exact",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ])
    .env(DIR_VAR, dir)
    .env(SCENARIO_VAR, name)
    .env(ARM_VAR, arm.encode())
    .env_remove(crash_points::AT_ROW_CHANGE)
    .env_remove(crash_points::AT_COMMIT);
    if let Some((var, k)) = env {
        cmd.env(var, k.to_string());
    }
    cmd.output().unwrap()
}

fn output_text(out: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Died by `abort` (not a test failure, exit code 101, and not by reaching the end).
fn assert_killed(out: &Output, dir: &Path, label: &str) {
    assert!(
        !out.status.success() && out.status.code() != Some(101) && !dir.join(SURVIVED).exists(),
        "{label}: the child must be killed ({:?}): {}",
        out.status,
        output_text(out)
    );
}

fn measure(dir: &Path, name: &str) -> (u64, u64) {
    let out = spawn_child(dir, name, Arm::Measure, None);
    assert!(
        out.status.success(),
        "{name} measure: {}",
        output_text(&out)
    );
    let m = read_json(&dir.join(MEASURED));
    let (rows, commits) = (m["rows"].as_u64().unwrap(), m["commits"].as_u64().unwrap());
    assert!(rows > 0 && commits > 0, "{name}: {m}");
    (rows, commits)
}

/// 1, half and all of `n`, without repeats.
fn points(n: u64) -> Vec<u64> {
    let mut v = vec![1, n.div_ceil(2), n];
    v.dedup();
    v
}

fn kill_points(rows: u64, commits: u64) -> Vec<Arm> {
    points(rows)
        .into_iter()
        .map(Arm::Row)
        .chain(points(commits).into_iter().map(Arm::Commit))
        .collect()
}

// ------------------------------------------------------------------ the parents

fn restart_and_verify(s: &Scenario, dir: &Path, base: &Value, arm: Option<Arm>) {
    let label = format!("{} {arm:?}", s.name);
    let app = App::open_sqlite(&dir.join(DB)).unwrap_or_else(|e| panic!("{label}: {e:?}"));
    common_checks(&app, dir, &label);
    let present = (s.verify)(&app, dir, base, arm.is_none());
    common_checks(&app, dir, &format!("{label} verified"));
    let newest = newest_oplog(&app);
    if present {
        assert_ne!(newest["id"], base["oplog"]["id"], "{label}: {newest}");
    } else {
        assert_eq!(
            newest["id"], base["oplog"]["id"],
            "{label}: the last step must be the last acknowledged command: {newest}"
        );
    }
    undo_round_trip(&app, dir, &label);
}

fn run_matrix(name: &str) {
    let s = scenario(name);
    let root = tempfile::tempdir().unwrap();
    let base_dir = root.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();
    {
        let app = App::open_sqlite(&base_dir.join(DB)).unwrap();
        let mut base = (s.setup)(&app, &base_dir);
        base["oplog"] = newest_oplog(&app);
        assert_ne!(base["oplog"], Value::Null, "{name}: something to undo");
        common_checks(&app, &base_dir, &format!("{name} base"));
        write_json(&base_dir.join(BASE), &base);
    }
    let base = read_json(&base_dir.join(BASE));

    let measured = root.path().join("measure");
    copy_dir(&base_dir, &measured);
    let (rows, commits) = measure(&measured, name);
    restart_and_verify(s, &measured, &base, None);

    let mut arms = kill_points(rows, commits);
    if s.part {
        arms.push(Arm::Part);
    }
    for (i, arm) in arms.iter().enumerate() {
        let dir = root.path().join(format!("kill{i}"));
        copy_dir(&base_dir, &dir);
        let out = spawn_child(&dir, name, *arm, None);
        assert_killed(&out, &dir, &format!("{name} {arm:?}"));
        restart_and_verify(s, &dir, &base, Some(*arm));
    }
    eprintln!("kill matrix {name}: {rows} row changes, {commits} commits, killed at {arms:?}");
}

#[test]
fn kill_matrix_study_answer() {
    run_matrix("study");
}

#[test]
fn kill_matrix_subtitle_capture_with_media() {
    run_matrix("subtitle");
}

#[test]
fn kill_matrix_import() {
    run_matrix("import");
}

#[test]
fn kill_matrix_undo() {
    run_matrix("undo");
}

#[test]
fn kill_matrix_redo() {
    run_matrix("redo");
}

#[test]
fn kill_matrix_backup_create() {
    run_matrix("backup");
}

#[test]
fn kill_matrix_word_index_rebuild() {
    run_matrix("words");
}

// --- 7. Migration on open

/// A collection written by an older build (before the v1.4 migrations 14xx, which rebuild
/// derived data: the word index, …) is opened by this one; the open is killed during the
/// migrations; the next open completes them with every row intact.
#[test]
fn kill_matrix_migration() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    let (dump, expected) = {
        let app = App::open_sqlite(&src.join(DB)).unwrap();
        study_setup(&app, &src);
        let data = base64::engine::general_purpose::STANDARD.encode(jpeg(7));
        call(
            &app,
            "media.add",
            json!({"name": "resim.jpg", "data_base64": data}),
        );
        let expected = json!({
            "notes": live_rows(&app, "notes"),
            "cards": live_rows(&app, "cards"),
            "reviews": live_rows(&app, "review_log"),
            "media": live_rows(&app, "media"),
            "oplog": newest_oplog(&app),
            "index": index_dump(&app),
        });
        (dump(&app), expected)
    };
    let older: Vec<u32> = dump
        .schema_versions
        .iter()
        .copied()
        .filter(|v| *v < 1410)
        .collect();
    assert!(older.len() < dump.schema_versions.len());
    let fitted = fc_storage::fit_dump_to_schema(dump, &older).unwrap();
    let base_dir = root.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();
    crash_points::write_older_collection(&base_dir.join(DB), &fitted).unwrap();
    copy_dir(&src.join("media"), &base_dir.join("media"));

    let check = |dir: &Path, label: &str| {
        let app = App::open_sqlite(&dir.join(DB)).unwrap_or_else(|e| panic!("{label}: {e:?}"));
        assert_eq!(
            app.store().schema_version().unwrap(),
            fc_storage::migrate::latest_version(),
            "{label}"
        );
        common_checks(&app, dir, label);
        for t in ["notes", "cards", "reviews", "media"] {
            let table = if t == "reviews" { "review_log" } else { t };
            assert_eq!(
                live_rows(&app, table) as u64,
                expected[t].as_u64().unwrap(),
                "{label}: {t}"
            );
        }
        assert_eq!(newest_oplog(&app)["id"], expected["oplog"]["id"], "{label}");
        // The word index the migration queued is built, equal to a full rebuild.
        assert_eq!(
            index_dump(&app),
            expected["index"].as_str().unwrap(),
            "{label}"
        );
        undo_round_trip(&app, dir, label);
    };

    let measured = root.path().join("measure");
    copy_dir(&base_dir, &measured);
    let (rows, commits) = measure(&measured, MIGRATION);
    check(&measured, "migration measure");

    let mut done = Vec::new();
    for (i, (var, k)) in points(rows)
        .into_iter()
        .map(|k| (crash_points::AT_ROW_CHANGE, k))
        .chain(
            points(commits)
                .into_iter()
                .map(|k| (crash_points::AT_COMMIT, k)),
        )
        .enumerate()
    {
        let label = format!("migration {var}={k}");
        let dir = root.path().join(format!("kill{i}"));
        copy_dir(&base_dir, &dir);
        let out = spawn_child(&dir, MIGRATION, Arm::Row(k), Some((var, k)));
        assert_killed(&out, &dir, &label);
        check(&dir, &label);
        done.push(label);
    }
    eprintln!("kill matrix migration: {rows} row changes, {commits} commits, killed at {done:?}");
}
