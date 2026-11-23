//! The disaster path end to end over the real `fc` binary (v1.4.1 second backup location):
//! a collection with real data is backed up with `io.backup_mirror_dir` set, its whole folder
//! is deleted, and a brand new profile is restored from the mirror copy alone. Every step is
//! a separate process; nothing assumes Unix paths (this runs in the Windows CI job too).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

// ------------------------------------------------------------------ fc helpers

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

fn json_out(db: &Path, args: &[&str]) -> Value {
    let mut all = vec!["--json"];
    all.extend_from_slice(args);
    serde_json::from_str(&ok(db, &all)).unwrap()
}

/// `fc call <method> <params>`: a method answering with a job prints the finished job.
fn call(db: &Path, method: &str, params: &Value) -> Value {
    json_out(db, &["call", method, &params.to_string()])
}

fn app_setting(db: &Path, key: &str, value: Value) {
    call(
        db,
        "settings.set",
        &json!({"scope": {"kind": "app"}, "key": key, "value": value}),
    );
}

/// A finished, successful job's result.
fn job_result(job: &Value) -> &Value {
    assert_eq!(job["status"], "succeeded", "{job}");
    &job["result"]
}

// ------------------------------------------------------------------ data helpers

/// Deterministic pseudo-random bytes (xorshift), so byte identity means something.
fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn b64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| B64.iter().position(|&b| b == c).unwrap() as u32;
    let clean: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.chunks(4) {
        let data: Vec<u8> = chunk.iter().copied().filter(|&c| c != b'=').collect();
        let mut n = 0u32;
        for (i, &c) in data.iter().enumerate() {
            n |= val(c) << (18 - 6 * i);
        }
        for i in 0..data.len().saturating_sub(1) {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    out
}

/// The media store's file for `hash`: `<collection folder>/media/<ab>/<hash>`.
fn media_file(db: &Path, hash: &str) -> PathBuf {
    db.parent()
        .unwrap()
        .join("media")
        .join(&hash[..2])
        .join(hash)
}

/// Tables a restore deliberately does not bring back as they are (history starts afresh,
/// running work and access grants stay with the machine, derived data is rebuilt):
/// * `oplog`: cleared, a restore barrier instead; `jobs`: the restoring profile's own;
/// * `subtitle_keys` (revoked on restore), `subtitle_capture_log` (emptied),
///   `subtitle_uploads` (pending uploads of the extension), `api_clients`, `mcp_servers`;
/// * never dumped: `schema_migrations`, `unfinished_batch*`, the statistics cache, the word
///   index queues.
const NOT_COMPARED: &[&str] = &[
    "oplog",
    "jobs",
    "subtitle_keys",
    "subtitle_capture_log",
    "subtitle_uploads",
    "api_clients",
    "mcp_servers",
    "schema_migrations",
    "unfinished_batches",
    "unfinished_batch_parts",
    "stats_day_cache",
    "stats_cache_meta",
    "review_log_generation",
    "word_index_dirty",
    "word_index_dirty_keys",
];

/// Derived tables: the full-text index (the FTS5 table and its `_data/_idx/_docsize/
/// _config/_content` shadow tables, rebuilt by a restore with another segment layout; search
/// is checked through the API instead) and SQLite's own (`sqlite_stat1` planner statistics,
/// `sqlite_sequence`).
fn is_derived(table: &str) -> bool {
    table.contains("_fts") || table.starts_with("sqlite_")
}

/// Every row of every compared table, rendered and sorted (row order is not data).
fn snapshot(db: &Path) -> BTreeMap<String, Vec<String>> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let tables: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(Result::unwrap)
            .filter(|t| !NOT_COMPARED.contains(&t.as_str()) && !is_derived(t))
            .collect()
    };
    let mut out = BTreeMap::new();
    for t in tables {
        // The word data the app ships is reinstalled from the binary (its install time is the
        // restoring profile's), never taken from a backup: only imported packs are data.
        let filter = if t == "word_data_packs" {
            " WHERE origin = 'imported'"
        } else {
            ""
        };
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM \"{t}\"{filter}"))
            .unwrap();
        let n = stmt.column_count();
        let mut rows: Vec<String> = stmt
            .query_map([], |r| {
                let cells: Vec<rusqlite::types::Value> =
                    (0..n).map(|i| r.get(i)).collect::<Result<_, _>>()?;
                Ok(format!("{cells:?}"))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        rows.sort();
        out.insert(t, rows);
    }
    out
}

fn count(db: &Path, sql: &str) -> i64 {
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn ids(db: &Path, sql: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn.prepare(sql).unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn named(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    files_in(dir)
        .into_iter()
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            name.starts_with(prefix) && name.ends_with(".fcbackup")
        })
        .collect()
}

// ------------------------------------------------------------------ Subtitle over HTTP

/// `fc api serve` on a free loopback port (the server the desktop runs), killed on drop.
struct Serve(Child, SocketAddr);

impl Drop for Serve {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn serve(db: &Path) -> Serve {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fc"))
        .arg("--db")
        .arg(db)
        .args(["api", "serve", "--port", "0"])
        .env_remove("FC_DB")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stderr.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let addr = line
        .trim()
        .rsplit("http://")
        .next()
        .unwrap()
        .parse()
        .unwrap_or_else(|_| panic!("no address in {line:?}"));
    Serve(child, addr)
}

/// One raw HTTP/1.1 request (`Connection: close`): (status, JSON body).
fn send(addr: SocketAddr, path: &str, headers: &[(&str, &str)], body: &[u8]) -> (u16, Value) {
    let mut s = TcpStream::connect(addr).unwrap();
    let mut req = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nContent-Length: {}\r\n",
        addr.port(),
        body.len()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    let mut bytes = req.into_bytes();
    bytes.extend_from_slice(body);
    s.write_all(&bytes).unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, rest) = text.split_once("\r\n\r\n").unwrap();
    let status: u16 = head.split(' ').nth(1).unwrap().parse().unwrap();
    let chunked = head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked");
    let payload = if chunked {
        dechunk(rest)
    } else {
        rest.to_owned()
    };
    (
        status,
        serde_json::from_str(&payload).unwrap_or(Value::Null),
    )
}

fn dechunk(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some((len, tail)) = rest.split_once("\r\n") {
        let n = usize::from_str_radix(len.trim(), 16).unwrap_or(0);
        if n == 0 {
            break;
        }
        out.push_str(&tail[..n]);
        rest = &tail[n + 2..];
    }
    out
}

/// A capture as the Subtitle extension sends it (`docs/flashcards-api.md`).
fn capture(n: u32, lemma: &str, definition: &str, sentence: &str, shot: Option<&str>) -> Value {
    let media = match shot {
        Some(id) => json!({"screenshot": {"upload_id": id, "mime": "image/jpeg"}}),
        None => json!({}),
    };
    json!({
        "schema": 1,
        "capture_id": format!("0192812a-6f3c-7a41-9c2e-{n:012x}"),
        "kind": "word",
        "lang": {"target": "en", "native": "tr"},
        "term": {"surface": lemma, "lemma": lemma, "pos": "noun"},
        "definition": {"text": definition, "source": "dict"},
        "context": {"sentence": sentence, "translation": null, "term_offset": null},
        "source": {
            "type": "video",
            "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "title": "Disaster drill",
            "site": "youtube",
            "video_id": "dQw4w9WgXcQ",
            "t_start_ms": 1000 * u64::from(n),
            "t_end_ms": 1000 * u64::from(n) + 1500
        },
        "media": media,
        "deck_id": null,
        "tags": ["subtitle"],
        "created_at": "2026-10-12T10:00:00.000Z",
        "client": {"name": "subtitle-extension", "ext_version": "3.1.3"}
    })
}

/// Sends three captures (two scenes of "key", one "door") and a screenshot through the real
/// local server, as the extension would. Returns the screenshot's bytes.
fn capture_from_subtitle(db: &Path) -> Vec<u8> {
    call(
        db,
        "modules.set_enabled",
        &json!({"id": "subtitle", "enabled": true}),
    );
    let key = call(db, "subtitle.keys.create", &json!({"name": "Chrome"}));
    let auth = format!("Bearer {}", key["secret"].as_str().unwrap());
    let server = serve(db);
    let mut shot = b"\xff\xd8\xff\xe0\x00\x10JFIF\x00".to_vec();
    shot.extend(noise(7, 3000));
    let first = capture(1, "key", "anahtar", "I just need to pick up my key.", None);
    let capture_id = first["capture_id"].as_str().unwrap().to_owned();
    let (status, up) = send(
        server.1,
        "/subtitle/v1/media",
        &[
            ("Authorization", &auth),
            ("Content-Type", "image/jpeg"),
            ("X-Capture-Id", &capture_id),
            ("X-Media-Kind", "screenshot"),
        ],
        &shot,
    );
    assert_eq!(status, 201, "{up}");
    let upload = up["upload_id"].as_str().unwrap().to_owned();
    let captures = [
        capture(
            1,
            "key",
            "anahtar",
            "I just need to pick up my key.",
            Some(&upload),
        ),
        capture(2, "key", "anahtar", "Where did you leave the key?", None),
        capture(3, "door", "kapı", "Close the door behind you.", None),
    ];
    for c in &captures {
        let (status, r) = send(
            server.1,
            "/subtitle/v1/captures",
            &[
                ("Authorization", &auth),
                ("Content-Type", "application/json"),
            ],
            c.to_string().as_bytes(),
        );
        assert!(status == 201 || status == 200, "{status} {r}");
    }
    drop(server); // no process holds the collection afterwards
    shot
}

// ------------------------------------------------------------------ the facts compared

/// What the user would miss, as the API shows it (wall-clock dependent parts left out).
fn api_facts(db: &Path, card_ids: &[String], note_ids: &[String]) -> Value {
    let notes: Vec<Value> = note_ids
        .iter()
        .map(|id| call(db, "notes.get", &json!({"id": id})))
        .collect();
    let cards: Vec<Value> = card_ids
        .iter()
        .map(|id| {
            let h = call(db, "study.card_history", &json!({"card_id": id}));
            json!({"card": h["card"], "reviews": h["reviews"]})
        })
        .collect();
    let encounters: Vec<Value> = note_ids
        .iter()
        .map(|id| call(db, "subtitle.encounters.list", &json!({"note_id": id})))
        .collect();
    let settings = call(
        db,
        "settings.get",
        &json!({"scope": {"kind": "app"}, "keys": [
            "io.auto_backup", "io.auto_backup_keep", "io.backup_mirror_dir",
            "words.known_interval_days"
        ]}),
    );
    // Search (the full-text index is rebuilt by a restore): the same notes for each word.
    let search: Vec<Value> = ["run", "koşmak", "house", "key", "kapı", "tag:temel"]
        .iter()
        .map(|q| {
            let page = call(
                db,
                "search.cards",
                &json!({"query": q, "notes_only": true, "limit": 50}),
            );
            let mut found: Vec<String> = page["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["note_id"].as_str().unwrap().to_owned())
                .collect();
            found.sort();
            json!({"query": q, "notes": found})
        })
        .collect();
    json!({
        "search": search,
        "notes": notes,
        "cards": cards,
        "encounters": encounters,
        "words": call(db, "words.list", &json!({"language": "en"})),
        "media": call(db, "media.list", &json!({})),
        "settings": settings,
    })
}

// ------------------------------------------------------------------ tests

/// Profile A is built with real data and backed up with a mirror folder; A's whole folder
/// is then deleted. A brand new profile B restored from the mirror copy alone has every note,
/// card (with its schedule), review, media file (byte for byte), Subtitle encounter, word
/// mark and ordinary setting back.
#[test]
fn a_new_profile_restored_from_the_mirror_alone_has_everything_back() {
    let tmp_a = tempfile::tempdir().unwrap();
    let mirror = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();
    let a = tmp_a.path().join("collection.db");
    let m = mirror.path().to_path_buf();
    assert!(m.is_absolute());

    // ---- 1. Profile A: real data through the API.
    ok(&a, &["deck", "create", "Dil"]);
    let mut pic = b"\x89PNG\r\n\x1a\n".to_vec();
    pic.extend(noise(42, 6000));
    let media = call(
        &a,
        "media.add",
        &json!({"name": "resim.png", "data_base64": b64_encode(&pic)}),
    );
    let hash = media["hash"].as_str().unwrap().to_owned();
    let reference = media["reference"].as_str().unwrap().to_owned();
    for (front, back) in [
        ("run", "koşmak".to_owned()),
        ("walk", "yürümek".to_owned()),
        ("house", format!("ev <img src=\"{reference}\">")),
        ("book", "kitap".to_owned()),
    ] {
        ok(
            &a,
            &[
                "note", "add", "--deck", "Dil", "--front", front, "--back", &back, "--tags",
                "temel",
            ],
        );
    }
    assert_eq!(
        call(&a, "media.get", &json!({"hash": hash}))["ref_count"],
        1,
        "the note references the picture"
    );

    // Study: every answer a separate process; reviews and new schedules land in the file.
    let deck_id = ids(&a, "SELECT id FROM decks WHERE name = 'Dil'")[0].clone();
    let mut session = call(
        &a,
        "study.start",
        &json!({"kind": "normal", "deck_id": deck_id}),
    );
    let sid = session["id"].as_str().unwrap().to_owned();
    let ratings = ["good", "again", "easy", "hard", "good", "good"];
    let mut answered = 0;
    while let Some(card) = session["current"].as_object() {
        if answered >= 8 {
            break;
        }
        let card_id = card["card_id"].as_str().unwrap().to_owned();
        session = call(
            &a,
            "study.answer",
            &json!({
                "session_id": sid,
                "card_id": card_id,
                "rating": ratings[answered % ratings.len()],
                "duration_ms": 1200 + 100 * answered,
            }),
        );
        answered += 1;
    }
    assert!(answered >= 4, "every new card was shown once: {answered}");
    call(&a, "study.end", &json!({"session_id": sid}));
    let reviews = count(&a, "SELECT count(*) FROM review_log");
    assert!(reviews >= 4, "{reviews}");
    assert!(
        count(&a, "SELECT count(*) FROM cards WHERE stability IS NOT NULL") >= 4,
        "the answers scheduled the cards"
    );

    // Subtitle: captures through the real local server (encounters + a screenshot).
    let shot = capture_from_subtitle(&a);
    assert_eq!(count(&a, "SELECT count(*) FROM subtitle_captures"), 3);

    // Word marks and settings (non-default values).
    call(
        &a,
        "words.mark",
        &json!({"language": "en", "lemmas": ["hello", "world"], "mark": "known"}),
    );
    call(
        &a,
        "words.mark",
        &json!({"language": "en", "lemmas": ["bob"], "mark": "ignored"}),
    );
    app_setting(&a, "io.auto_backup_keep", json!(4));
    app_setting(&a, "io.auto_backup", json!("weekly"));
    app_setting(&a, "words.known_interval_days", json!(30));
    app_setting(&a, "io.backup_mirror_dir", json!(m.to_string_lossy()));

    // Backups: a manual one to a file in A's folder, then the automatic one; each is also
    // copied to the mirror and re-read there.
    let manual_path = tmp_a.path().join("elle.fcbackup");
    let manual = call(
        &a,
        "io.backup.create",
        &json!({"path": manual_path.to_string_lossy()}),
    );
    let manual = job_result(&manual);
    assert_eq!(manual["mirror"]["error"], Value::Null, "{manual}");
    let manual_copy = PathBuf::from(manual["mirror"]["path"].as_str().unwrap());
    assert!(manual_copy.is_file(), "{manual}");
    assert_eq!(manual_copy.parent().unwrap(), m.as_path());

    let auto = call(&a, "io.backup.auto", &json!({}));
    assert_eq!(auto["kind"], "io.auto_backup", "{auto}");
    let auto = job_result(&auto).clone();
    assert_eq!(auto["mirror"]["error"], Value::Null, "{auto}");
    assert_eq!(auto["verified"], true);
    assert_eq!(auto["media"], 2, "the picture and the screenshot: {auto}");
    assert!(auto["path"]
        .as_str()
        .unwrap()
        .starts_with(&*tmp_a.path().join("backups").join("auto").to_string_lossy()));
    let copy = PathBuf::from(auto["mirror"]["path"].as_str().unwrap());
    assert!(copy.is_file(), "{auto}");
    assert_eq!(named(&m, "auto-"), vec![copy.clone()]);
    assert_eq!(named(&m, "manual-"), vec![manual_copy]);
    // Byte-identical to the backup at home.
    assert_eq!(
        std::fs::read(&copy).unwrap(),
        std::fs::read(auto["path"].as_str().unwrap()).unwrap()
    );

    let status = call(&a, "io.backup.status", &json!({}));
    let ms = &status["mirror"];
    assert_eq!(ms["state"], "ok", "{status}");
    assert_eq!(ms["folder"], json!(m.to_string_lossy()));
    assert_eq!(ms["copies"].as_array().unwrap().len(), 2, "{status}");
    assert_eq!(ms["last_failure"], Value::Null);
    assert_eq!(status["keep"], 4);

    // ---- 2. "Before" facts.
    let note_ids = ids(&a, "SELECT id FROM notes ORDER BY id");
    let card_ids = ids(&a, "SELECT id FROM cards ORDER BY id");
    assert_eq!(note_ids.len(), 6, "4 typed + 2 Subtitle notes");
    let before_rows = snapshot(&a);
    let before_api = api_facts(&a, &card_ids, &note_ids);
    let key_encounters = call(
        &a,
        "subtitle.encounters.list",
        &json!({"lang": "en", "lemma": "key"}),
    );
    assert_eq!(
        key_encounters["encounters"].as_array().unwrap().len(),
        2,
        "two scenes of one word: {key_encounters}"
    );
    let marks = before_rows["word_marks"].clone();
    assert_eq!(marks.len(), 3, "{marks:?}");
    assert_eq!(before_rows["review_log"].len() as i64, reviews);

    // ---- 3. Disaster: A's whole folder is gone (no fc process is running).
    let a_root = tmp_a.path().to_path_buf();
    tmp_a.close().unwrap();
    assert!(!a_root.exists());
    assert!(copy.is_file(), "only the mirror survives");

    // ---- 4. Profile B: a brand new collection, restored from the mirror copy only.
    let b = tmp_b.path().join("collection.db");
    ok(&b, &["deck", "list"]);
    assert_eq!(count(&b, "SELECT count(*) FROM notes"), 0);
    let copy_arg = copy.to_string_lossy().into_owned();
    let info = call(&b, "io.backup.inspect", &json!({"path": copy_arg}));
    assert_eq!(info["compatible"], true, "{info}");
    assert_eq!(info["complete"], true, "{info}");
    assert_eq!(info["media"], 2);
    assert_eq!(info["counts"]["notes"], 6, "{info}");
    assert_eq!(info["counts"]["review_log"], reviews, "{info}");
    let restored = call(
        &b,
        "io.backup.restore",
        &json!({"path": copy_arg, "safety_backup_path": null}),
    );
    let report = job_result(&restored);
    assert_eq!(report["media"], 2, "{report}");
    assert_eq!(report["missing_media"], json!([]));
    // The safety backup of B's empty state went into B's folder, not next to the mirror.
    let safety = PathBuf::from(report["safety_backup_path"].as_str().unwrap());
    assert!(safety.starts_with(tmp_b.path()), "{report}");
    // Nothing else was written into the mirror (no safety backup, no leftover `.part`).
    assert_eq!(files_in(&m).len(), 2, "{:?}", files_in(&m));

    // ---- 5. Everything is back (each read a fresh process on B's file).
    let after_rows = snapshot(&b);
    let tables: Vec<&String> = before_rows.keys().collect();
    assert_eq!(tables, after_rows.keys().collect::<Vec<_>>());
    let differing: Vec<String> = before_rows
        .iter()
        .filter(|(table, rows)| *rows != &after_rows[*table])
        .map(|(table, rows)| {
            let gone: Vec<&String> = rows
                .iter()
                .filter(|r| !after_rows[table].contains(r))
                .collect();
            let new: Vec<&String> = after_rows[table]
                .iter()
                .filter(|r| !rows.contains(r))
                .collect();
            let cut = |v: &[&String]| {
                v.iter()
                    .map(|r| r.chars().take(300).collect::<String>())
                    .collect::<Vec<_>>()
            };
            format!(
                "{table}: before only {:?}\n  after only {:?}",
                cut(&gone),
                cut(&new)
            )
        })
        .collect();
    assert!(
        differing.is_empty(),
        "differ after the restore:\n{}",
        differing.join("\n")
    );
    let after_api = api_facts(&b, &card_ids, &note_ids);
    for part in [
        "notes",
        "cards",
        "encounters",
        "words",
        "media",
        "settings",
        "search",
    ] {
        assert_eq!(before_api[part], after_api[part], "{part}");
    }
    // Schedules really were restored (not just two empty sets compared).
    let hits = |q: &str| {
        after_api["search"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["query"] == q)
            .unwrap()["notes"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(hits("run"), 1, "{}", after_api["search"]);
    assert_eq!(hits("tag:temel"), 4, "{}", after_api["search"]);
    assert!(hits("key") >= 1, "{}", after_api["search"]);
    let studied = after_api["cards"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["card"]["stability"].is_number() && !c["reviews"].as_array().unwrap().is_empty()
        })
        .count();
    assert!(studied >= 4, "{after_api}");
    // Settings: the values set in A, resolved from the app scope.
    let settings: BTreeMap<String, Value> = after_api["settings"]["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["key"].as_str().unwrap().to_owned(), v["value"].clone()))
        .collect();
    assert_eq!(settings["io.auto_backup_keep"], 4);
    assert_eq!(settings["io.auto_backup"], "weekly");
    assert_eq!(settings["words.known_interval_days"], 30);
    assert_eq!(settings["io.backup_mirror_dir"], json!(m.to_string_lossy()));
    // Word marks: still known / ignored.
    let words = &after_api["words"]["words"];
    let status_of = |lemma: &str| {
        words
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["key"] == lemma)
            .map(|w| (w["status"].clone(), w["mark"].clone()))
            .unwrap_or_else(|| panic!("{lemma} not in {words}"))
    };
    assert_eq!(status_of("hello"), (json!("known"), json!("known")));
    assert_eq!(status_of("bob"), (json!("ignored"), json!("ignored")));

    // Media byte for byte: through the API and in B's media store.
    for (h, bytes) in [
        (hash.clone(), pic.clone()),
        (media_hash_of(&b, &shot), shot),
    ] {
        let read = call(&b, "media.read", &json!({"hash": h}));
        assert_eq!(b64_decode(read["data_base64"].as_str().unwrap()), bytes);
        assert_eq!(std::fs::read(media_file(&b, &h)).unwrap(), bytes);
    }
    assert_eq!(
        call(&b, "media.get", &json!({"hash": hash}))["ref_count"],
        1
    );

    // B goes on: its next automatic backup lands in the same mirror.
    let next = call(&b, "io.backup.auto", &json!({}));
    let next = job_result(&next);
    assert_eq!(next["mirror"]["error"], Value::Null, "{next}");
    assert_eq!(next["notes"], 6);
    assert_eq!(named(&m, "auto-").len(), 2);
}

/// The media-store hash of `bytes` as B lists it (the screenshot's hash is the app's).
fn media_hash_of(db: &Path, bytes: &[u8]) -> String {
    let list = call(db, "media.list", &json!({}));
    let size = bytes.len() as u64;
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["size"] == size && m["mime"] == "image/jpeg")
        .unwrap_or_else(|| panic!("no {size}-byte JPEG in {list}"))["hash"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// A mirror folder that is not there (an unplugged drive) never fails the backup: the local
/// copy is written, the job's result and `io.backup.status` say what went wrong; once the
/// folder is back the next backup copies there and the failure is cleared.
#[test]
fn a_missing_mirror_is_reported_and_recovers() {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let db = home.path().join("collection.db");
    let gone = outside.path().join("unplugged-drive");
    ok(&db, &["deck", "create", "Dil"]);
    ok(
        &db,
        &[
            "note", "add", "--deck", "Dil", "--front", "x", "--back", "y",
        ],
    );
    app_setting(&db, "io.backup_mirror_dir", json!(gone.to_string_lossy()));

    let job = call(&db, "io.backup.auto", &json!({}));
    let r = job_result(&job).clone();
    assert!(PathBuf::from(r["path"].as_str().unwrap()).is_file(), "{r}");
    assert_eq!(r["mirror"]["path"], Value::Null, "{r}");
    assert!(r["mirror"]["error"].is_string(), "{r}");
    assert!(!gone.exists(), "the mirror folder is never created");

    let status = call(&db, "io.backup.status", &json!({}));
    let ms = &status["mirror"];
    assert_eq!(ms["state"], "missing", "{status}");
    assert_eq!(ms["copies"], json!([]));
    assert!(ms["last_failure"]["message"].is_string(), "{status}");
    // The automatic backup itself is fine.
    assert_eq!(status["last_failure"], Value::Null, "{status}");
    assert_eq!(status["automatic"].as_array().unwrap().len(), 1);

    // The drive is back.
    std::fs::create_dir(&gone).unwrap();
    let job = call(&db, "io.backup.auto", &json!({}));
    let r = job_result(&job).clone();
    assert_eq!(r["mirror"]["error"], Value::Null, "{r}");
    let copy = PathBuf::from(r["mirror"]["path"].as_str().unwrap());
    assert!(copy.is_file());
    assert_eq!(named(&gone, "auto-"), vec![copy]);
    let status = call(&db, "io.backup.status", &json!({}));
    let ms = &status["mirror"];
    assert_eq!(ms["state"], "ok", "{status}");
    assert_eq!(ms["copies"].as_array().unwrap().len(), 1);
    assert_eq!(ms["last_failure"], Value::Null, "{status}");
}
