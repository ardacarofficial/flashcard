//! v1.4.1 robustness round: seeded property tests ("fuzz") of the Subtitle capture surface
//! (`/subtitle/v1/*`, docs/06 §5.1) over real TCP.
//!
//! Properties, for every generated request:
//!
//! * the server answers: a well-formed HTTP response, never a closed connection or a hang;
//! * a client mistake is a `4xx`, never a `5xx`, with the contract's error body
//!   `{"error": {"code", "message"}}` (a request hyper refuses before routing — broken framing —
//!   gets hyper's bare `400`/`431`);
//! * a rejected request writes **nothing**: the whole collection (every table but the capture
//!   diagnostics, which record rejections by design, and the key's last-use note) and the media
//!   files on disk are the same before and after;
//! * duplicates are one note: replays, resends under another capture id of the same line,
//!   batches and concurrent resends (captures and uploads racing each other, captures before
//!   their media) end in one card per word and one upload id per picture/sound;
//! * after the run the server still takes a valid capture.
//!
//! Deterministic: a hand-rolled SplitMix64 from a fixed seed. `FC_FUZZ_SEED` (decimal or `0x`
//! hex) replaces the seed and `FC_FUZZ_CASES` the number of cases; every failure names the seed
//! and the case so it can be replayed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::NoteRef;
use fc_api::v1::search::SearchCards;
use fc_api::v1::subtitle::CreateSubtitleKey;
use fc_app::modules::subtitle::{MAX_AUDIO_BYTES, MAX_BATCH, MAX_SCREENSHOT_BYTES};
use fc_app::testing::{TempSqlite, TestApp};
use fc_http::{Server, ServerConfig, MAX_BODY_BYTES};
use fc_storage::repo::TableDump;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ------------------------------------------------------------------ seeds and cases

/// The seed when `FC_FUZZ_SEED` is not set.
const DEFAULT_SEED: u64 = 0x00f1_a5c0_de14_1001;

fn base_seed() -> u64 {
    match std::env::var("FC_FUZZ_SEED") {
        Ok(s) => {
            let s = s.trim();
            let parsed = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                Some(hex) => u64::from_str_radix(hex, 16),
                None => s.parse(),
            };
            parsed.unwrap_or_else(|_| panic!("FC_FUZZ_SEED={s:?} is not a u64"))
        }
        Err(_) => DEFAULT_SEED,
    }
}

/// Cases of a test: `FC_FUZZ_CASES` or the test's default (sized for ≤ ~10 s in debug).
fn cases(default: usize) -> usize {
    std::env::var("FC_FUZZ_CASES")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}

/// Where a failure came from: printed with every assertion.
#[derive(Clone)]
struct Case {
    test: &'static str,
    seed: u64,
    index: usize,
    what: String,
}

impl std::fmt::Display for Case {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{} seed={:#x} case={} — replay: FC_FUZZ_SEED={:#x} FC_FUZZ_CASES={} cargo test -p fc-http --test fuzz_capture {}] {}",
            self.test,
            self.seed,
            self.index,
            self.seed,
            self.index + 1,
            self.test,
            self.what
        )
    }
}

/// SplitMix64: small, fast, the same everywhere.
struct Rng(u64);

impl Rng {
    /// A generator for `test`: the base seed mixed with the test's name, so tests differ but
    /// one `FC_FUZZ_SEED` replays them all.
    fn for_test(seed: u64, test: &str) -> Self {
        let mut h = seed;
        for b in test.bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
        Self(h)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// `0..n` (`n > 0`).
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n as u64)).unwrap()
    }

    /// `lo..=hi`.
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }

    /// True `num` times in `den`.
    fn chance(&mut self, num: usize, den: usize) -> bool {
        self.below(den) < num
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }

    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next().to_le_bytes()[0]).collect()
    }

    fn shuffle<T>(&mut self, xs: &mut [T]) {
        for i in (1..xs.len()).rev() {
            let j = self.below(i + 1);
            xs.swap(i, j);
        }
    }
}

// ------------------------------------------------------------------ raw HTTP

/// The `Content-Length` a request sends.
#[derive(Clone)]
enum Length {
    /// The body's length (none for a bodiless GET).
    Auto,
    /// No `Content-Length` header (chunked bodies, or none).
    Omit,
    /// This raw value, whatever the body is.
    Raw(Vec<u8>),
}

#[derive(Clone)]
struct Req {
    method: String,
    path: String,
    headers: Vec<(String, Vec<u8>)>,
    body: Vec<u8>,
    length: Length,
    /// Close the sending side after the request (a client that sent less than it declared).
    half_close: bool,
}

impl Req {
    fn new(method: &str, path: &str) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            headers: vec![],
            body: vec![],
            length: Length::Auto,
            half_close: false,
        }
    }

    fn header(mut self, k: &str, v: impl AsRef<[u8]>) -> Self {
        self.headers.push((k.into(), v.as_ref().to_vec()));
        self
    }

    fn body(mut self, b: impl Into<Vec<u8>>) -> Self {
        self.body = b.into();
        self
    }

    fn has(&self, name: &str) -> bool {
        self.headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(name))
    }

    fn bytes(&self, addr: SocketAddr) -> Vec<u8> {
        let mut out = format!("{} {} HTTP/1.1\r\n", self.method, self.path).into_bytes();
        if !self.has("host") {
            out.extend_from_slice(format!("Host: 127.0.0.1:{}\r\n", addr.port()).as_bytes());
        }
        out.extend_from_slice(b"Connection: close\r\n");
        for (k, v) in &self.headers {
            out.extend_from_slice(k.as_bytes());
            out.extend_from_slice(b": ");
            out.extend_from_slice(v);
            out.extend_from_slice(b"\r\n");
        }
        match &self.length {
            Length::Auto if !self.body.is_empty() || self.method == "POST" => {
                out.extend_from_slice(format!("Content-Length: {}\r\n", self.body.len()).as_bytes())
            }
            Length::Raw(v) => {
                out.extend_from_slice(b"Content-Length: ");
                out.extend_from_slice(v);
                out.extend_from_slice(b"\r\n");
            }
            _ => {}
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&self.body);
        out
    }

    /// A short description for failure messages (no huge bodies).
    fn describe(&self) -> String {
        let headers: Vec<String> = self
            .headers
            .iter()
            .map(|(k, v)| {
                let v = String::from_utf8_lossy(v);
                let v: String = v.chars().take(60).collect();
                format!("{k}: {v}")
            })
            .collect();
        let body = String::from_utf8_lossy(&self.body);
        let body: String = body.chars().take(160).collect();
        let length = match &self.length {
            Length::Auto => "auto".to_owned(),
            Length::Omit => "omitted".to_owned(),
            Length::Raw(v) => format!("{:?}", String::from_utf8_lossy(v)),
        };
        format!(
            "{} {} headers={headers:?} content-length={length} half_close={} body({} bytes)={body:?}",
            self.method,
            self.path,
            self.half_close,
            self.body.len()
        )
    }
}

#[derive(Debug)]
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn json(&self) -> Option<Value> {
        serde_json::from_slice(&self.body).ok()
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body)
            .chars()
            .take(400)
            .collect()
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn dechunk(mut rest: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let eol = find(rest, b"\r\n")?;
        let size = std::str::from_utf8(&rest[..eol]).ok()?;
        let size = usize::from_str_radix(size.split(';').next()?.trim(), 16).ok()?;
        rest = &rest[eol + 2..];
        if size == 0 {
            return Some(out);
        }
        out.extend_from_slice(rest.get(..size)?);
        rest = rest.get(size + 2..)?;
    }
}

/// One complete response out of the bytes read, if they hold one.
fn parse_reply(raw: &[u8], head_request: bool) -> Option<Reply> {
    let end = find(raw, b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&raw[..end]).to_string();
    let rest = &raw[end + 4..];
    let mut lines = head.split("\r\n");
    let status_line = lines.next()?;
    if !status_line.starts_with("HTTP/1.") {
        return None;
    }
    let status: u16 = status_line.split(' ').nth(1)?.parse().ok()?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    let get = |n: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(n))
            .map(|(_, v)| v.clone())
    };
    let body = if head_request {
        vec![]
    } else if get("transfer-encoding").is_some_and(|v| v.contains("chunked")) {
        dechunk(rest)?
    } else if let Some(n) = get("content-length") {
        rest.get(..n.parse::<usize>().ok()?)?.to_vec()
    } else {
        rest.to_vec()
    };
    Some(Reply {
        status,
        headers,
        body,
    })
}

/// Sends `req` (from a writer thread, so a server that answers before reading everything is
/// still heard) and reads the whole answer. `Err` = no complete response (closed, reset or a
/// minute of silence).
fn exchange(addr: SocketAddr, req: &Req) -> Result<Reply, String> {
    let s = TcpStream::connect(addr).map_err(|e| format!("connect: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let mut w = s.try_clone().unwrap();
    let bytes = req.bytes(addr);
    let half = req.half_close;
    let writer = std::thread::spawn(move || {
        let r = w.write_all(&bytes);
        if half {
            let _ = w.shutdown(Shutdown::Write);
        }
        r.err().map(|e| e.to_string())
    });
    let mut s = s;
    let mut raw = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let read_error = loop {
        match s.read(&mut buf) {
            Ok(0) => break None,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => break Some(e.to_string()),
        }
    };
    let write_error = writer.join().unwrap();
    parse_reply(&raw, req.method == "HEAD").ok_or_else(|| {
        format!(
            "no complete HTTP response: {} bytes read {:?}, read error {read_error:?}, write error {write_error:?}",
            raw.len(),
            String::from_utf8_lossy(&raw[..raw.len().min(200)])
        )
    })
}

// ------------------------------------------------------------------ the app

/// Tables that change on a rejected request by design: the capture diagnostics (every attempt
/// that reached the capture command leaves a row, `rejected` included — not user data, never
/// in the oplog or a backup) and the key's last-use note (at most once a minute).
const BOOKKEEPING: &[&str] = &["subtitle_capture_log", "subtitle_keys"];

/// Everything a request could have written: every table (minus [`BOOKKEEPING`]) and the media
/// files on disk.
#[derive(PartialEq)]
struct Snapshot {
    tables: BTreeMap<String, TableDump>,
    files: BTreeSet<String>,
}

impl Snapshot {
    fn rows(&self, table: &str) -> usize {
        self.tables.get(table).map_or(0, |t| t.rows.len())
    }

    /// What differs, in a few words per table.
    fn diff(&self, after: &Self) -> String {
        let mut out = Vec::new();
        let names: BTreeSet<&String> = self.tables.keys().chain(after.tables.keys()).collect();
        for n in names {
            let (a, b) = (self.tables.get(n), after.tables.get(n));
            if a != b {
                out.push(format!(
                    "{n}: {} → {} rows",
                    a.map_or(0, |t| t.rows.len()),
                    b.map_or(0, |t| t.rows.len())
                ));
            }
        }
        if self.files != after.files {
            out.push(format!(
                "media files: {} → {}",
                self.files.len(),
                after.files.len()
            ));
        }
        out.join("; ")
    }
}

fn files_under(dir: &Path, root: &Path, out: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            files_under(&p, root, out);
        } else {
            out.insert(
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

struct Fx {
    db: Arc<TempSqlite>,
    t: TestApp,
    server: Server,
    /// `Bearer <key>`.
    auth: String,
}

impl Fx {
    /// A file-backed app with the `subtitle` module on and one key, served on a free loopback
    /// port; the key's first use (its last-use note) is already behind it.
    fn new() -> Self {
        let db = Arc::new(TempSqlite::new());
        let t = TestApp::new(db.clone());
        t.ok::<ModulesSetEnabled>(SetModuleEnabled {
            id: "subtitle".into(),
            enabled: true,
        });
        let key = t.ok::<SubtitleKeysCreate>(CreateSubtitleKey {
            name: "Chrome".into(),
        });
        let app = Arc::new(TestApp::new(db.clone()).app);
        let server = Server::start(app, ServerConfig::localhost(0)).unwrap();
        let fx = Self {
            db,
            t,
            server,
            auth: format!("Bearer {}", key.secret),
        };
        let me = exchange(fx.addr(), &fx.get("/subtitle/v1/me")).unwrap();
        assert_eq!(me.status, 200, "{}", me.text());
        fx
    }

    fn addr(&self) -> SocketAddr {
        self.server.local_addr()
    }

    fn get(&self, path: &str) -> Req {
        Req::new("GET", path).header("Authorization", &self.auth)
    }

    fn post_json(&self, path: &str, body: impl Into<Vec<u8>>) -> Req {
        Req::new("POST", path)
            .header("Authorization", &self.auth)
            .header("Content-Type", "application/json")
            .body(body)
    }

    fn upload(&self, capture_id: &str, kind: &str, mime: &str, bytes: &[u8]) -> Req {
        Req::new("POST", "/subtitle/v1/media")
            .header("Authorization", &self.auth)
            .header("Content-Type", mime)
            .header("X-Capture-Id", capture_id)
            .header("X-Media-Kind", kind)
            .body(bytes)
    }

    fn snapshot(&self) -> Snapshot {
        let dump = self.t.store.read().unwrap().dump_collection().unwrap();
        let tables = dump
            .tables
            .into_iter()
            .filter(|t| !BOOKKEEPING.contains(&t.name.as_str()))
            .map(|t| (t.name.clone(), t))
            .collect();
        let root = self.db.path().parent().unwrap().to_path_buf();
        let mut files = BTreeSet::new();
        files_under(&root.join("media"), &root, &mut files);
        Snapshot { tables, files }
    }

    /// Sends `req`, waiting out a `429` (only reachable with a very large `FC_FUZZ_CASES`).
    fn send(&self, req: &Req, case: &Case) -> Reply {
        for _ in 0..5 {
            let r = exchange(self.addr(), req)
                .unwrap_or_else(|e| panic!("{case}: {e}\nrequest: {}", req.describe()));
            if r.status != 429 {
                return r;
            }
            let secs: u64 = r
                .header("retry-after")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            std::thread::sleep(Duration::from_secs(secs.min(5)));
        }
        panic!("{case}: still rate limited after 5 tries");
    }

    /// A fresh valid capture is taken, and its resend is a duplicate of the same card.
    fn still_serves(&self, n: u64, case: &Case) {
        let before = self.snapshot();
        let c = capture(n, json!({}));
        let req = self.post_json("/subtitle/v1/captures", c.to_string());
        let first = self.send(&req, case);
        assert_eq!(first.status, 201, "{case}: {}", first.text());
        let first = first.json().unwrap();
        assert_eq!(first["status"], "created", "{case}");
        let again = self.send(&req, case);
        assert_eq!(again.status, 200, "{case}: {}", again.text());
        assert_eq!(
            again.json().unwrap(),
            json!({ "status": "duplicate", "card_id": first["card_id"] }),
            "{case}"
        );
        let after = self.snapshot();
        assert_eq!(after.rows("notes"), before.rows("notes") + 1, "{case}");
        assert_eq!(after.rows("cards"), before.rows("cards") + 1, "{case}");
    }
}

// ------------------------------------------------------------------ captures

/// `0192812a-6f3c-7a41-9c2e-<n>`: a lowercase UUIDv7.
fn id(n: u64) -> String {
    format!("0192812a-6f3c-7a41-9c2e-{n:012x}")
}

/// Letters only, distinct per `n` (a lemma no other capture of the run has).
fn word(n: u64) -> String {
    let mut s = String::from("fz");
    let mut n = n;
    loop {
        s.push(char::from(b'a' + u8::try_from(n % 26).unwrap()));
        n /= 26;
        if n == 0 {
            return s;
        }
    }
}

/// A valid capture of its own word and line.
fn capture(n: u64, media: Value) -> Value {
    let w = word(n);
    json!({
      "schema": 1,
      "capture_id": id(n),
      "kind": "word",
      "lang": { "target": "en", "native": "tr" },
      "term": { "surface": w, "lemma": w, "pos": "noun" },
      "definition": { "text": "anlam", "source": "dict" },
      "context": {
        "sentence": format!("The {w} was here."),
        "translation": "Çeviri burada.",
        "term_offset": [4, 4 + w.len()]
      },
      "source": { "type": "video", "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=83s",
                  "title": "Film", "site": "youtube", "video_id": "dQw4w9WgXcQ",
                  "t_start_ms": 83450, "t_end_ms": 86120 },
      "media": media,
      "deck_id": null,
      "tags": ["subtitle"],
      "created_at": "2026-11-01T10:00:00.000Z",
      "client": { "name": "subtitle-extension", "ext_version": "3.1.3" }
    })
}

/// Every member of the capture (objects included), as JSON pointers.
const POINTERS: &[&str] = &[
    "/schema",
    "/capture_id",
    "/kind",
    "/lang",
    "/lang/target",
    "/lang/native",
    "/term",
    "/term/surface",
    "/term/lemma",
    "/term/pos",
    "/definition",
    "/definition/text",
    "/definition/source",
    "/context",
    "/context/sentence",
    "/context/translation",
    "/context/term_offset",
    "/source",
    "/source/type",
    "/source/url",
    "/source/title",
    "/source/site",
    "/source/video_id",
    "/source/t_start_ms",
    "/source/t_end_ms",
    "/media",
    "/deck_id",
    "/tags",
    "/tags/0",
    "/created_at",
    "/client",
    "/client/name",
    "/client/ext_version",
];

/// Members holding text (where long and odd strings go).
const TEXT_POINTERS: &[&str] = &[
    "/capture_id",
    "/kind",
    "/lang/target",
    "/lang/native",
    "/term/surface",
    "/term/lemma",
    "/term/pos",
    "/definition/text",
    "/definition/source",
    "/context/sentence",
    "/context/translation",
    "/source/url",
    "/source/title",
    "/source/site",
    "/source/video_id",
    "/deck_id",
    "/tags/0",
    "/created_at",
    "/client/ext_version",
];

const ODD_CHARS: &[char] = &[
    'a',
    'Z',
    '0',
    ' ',
    '"',
    '\\',
    '/',
    '\n',
    '\t',
    '\r',
    '\0',
    '\u{7f}',
    'ş',
    'ğ',
    'İ',
    'ı',
    '漢',
    '😀',
    '\u{202e}',
    '\u{feff}',
    '\u{200b}',
    '\u{fffd}',
    '\u{10ffff}',
    '%',
    '<',
    '&',
    '{',
];

fn odd_string(rng: &mut Rng, len: usize) -> String {
    (0..len).map(|_| *rng.pick(ODD_CHARS)).collect()
}

fn random_value(rng: &mut Rng, depth: usize) -> Value {
    let pick = if depth == 0 {
        rng.below(5)
    } else {
        rng.below(7)
    };
    match pick {
        0 => Value::Null,
        1 => Value::Bool(rng.chance(1, 2)),
        2 => match rng.below(6) {
            0 => json!(0),
            1 => json!(-1),
            2 => json!(i64::MIN),
            3 => json!(u64::MAX),
            4 => json!(1),
            _ => json!(rng.next()),
        },
        3 => rng
            .pick(&[
                json!(1.5),
                json!(-0.0),
                json!(1e308),
                json!(9.0e15),
                json!(0.1),
                json!(1.0),
            ])
            .clone(),
        4 => {
            let len = rng.below(40);
            Value::String(odd_string(rng, len))
        }
        5 => {
            let len = rng.below(4);
            Value::Array((0..len).map(|_| random_value(rng, depth - 1)).collect())
        }
        _ => {
            let len = rng.below(4);
            Value::Object(
                (0..len)
                    .map(|_| {
                        let k = rng.below(8);
                        (odd_string(rng, k), random_value(rng, depth - 1))
                    })
                    .collect(),
            )
        }
    }
}

fn kind_of(v: &Value) -> u8 {
    match v {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(_) => 2,
        Value::String(_) => 3,
        Value::Array(_) => 4,
        Value::Object(_) => 5,
    }
}

/// A value of another JSON type than `old`.
fn wrong_type(rng: &mut Rng, old: &Value) -> Value {
    loop {
        let v = random_value(rng, 2);
        if kind_of(&v) != kind_of(old) {
            return v;
        }
    }
}

fn set(v: &mut Value, pointer: &str, new: Value) {
    if let Some(slot) = v.pointer_mut(pointer) {
        *slot = new;
    }
}

fn remove(v: &mut Value, pointer: &str) {
    let (parent, key) = pointer.rsplit_once('/').unwrap();
    match v.pointer_mut(parent) {
        Some(Value::Object(o)) => {
            o.remove(key);
        }
        Some(Value::Array(a)) => {
            if let Ok(i) = key.parse::<usize>() {
                if i < a.len() {
                    a.remove(i);
                }
            }
        }
        _ => {}
    }
}

/// One small change to a JSON capture that may or may not keep it valid.
fn mutate(rng: &mut Rng, v: &mut Value) -> String {
    match rng.below(5) {
        0 => {
            let p = *rng.pick(POINTERS);
            let old = v.pointer(p).cloned().unwrap_or(Value::Null);
            set(v, p, wrong_type(rng, &old));
            format!("wrong type at {p}")
        }
        1 => {
            let p = *rng.pick(POINTERS);
            remove(v, p);
            format!("removed {p}")
        }
        2 => {
            let p = *rng.pick(TEXT_POINTERS);
            let len = rng.below(30);
            set(v, p, Value::String(odd_string(rng, len)));
            format!("odd string at {p}")
        }
        3 => {
            let p = *rng.pick(TEXT_POINTERS);
            let len = *rng.pick(&[64, 65, 200, 201, 2_000, 2_001, 5_000, 5_001]);
            let c = *rng.pick(&['x', 'ş', '😀']);
            set(v, p, Value::String(c.to_string().repeat(len)));
            format!("{len} × {c:?} at {p}")
        }
        _ => {
            let k = rng.range(1, 12);
            let key = odd_string(rng, k);
            let parent = *rng.pick(&["", "/term", "/source", "/media", "/context", "/client"]);
            let val = random_value(rng, 3);
            if let Some(Value::Object(o)) = v.pointer_mut(parent) {
                o.insert(key.clone(), val);
            }
            format!("unknown field {key:?} under {parent:?}")
        }
    }
}

/// A placeholder string the raw generators replace with bytes `serde_json` would never write.
const HOLE: &str = "\u{1}fuzz-hole\u{1}";

fn fill_hole(v: &Value, raw: &[u8]) -> Vec<u8> {
    let text = v.to_string().into_bytes();
    let needle = Value::String(HOLE.into()).to_string().into_bytes();
    let at = find(&text, &needle).unwrap();
    let mut out = text[..at].to_vec();
    out.extend_from_slice(raw);
    out.extend_from_slice(&text[at + needle.len()..]);
    out
}

fn deep(depth: usize, objects: bool) -> Vec<u8> {
    if objects {
        let mut s = "{\"a\":".repeat(depth).into_bytes();
        s.push(b'1');
        s.extend(std::iter::repeat_n(b'}', depth));
        s
    } else {
        let mut s = "[".repeat(depth).into_bytes();
        s.extend(std::iter::repeat_n(b']', depth));
        s
    }
}

/// Strings that are not valid UTF-8 / valid JSON text.
const BAD_STRINGS: &[&[u8]] = &[
    b"\"ab\xffcd\"",
    b"\"\xc3\x28\"",
    b"\"\xed\xa0\x80\"",
    b"\"\\ud800\"",
    b"\"\\udfff tail\"",
    b"\"\xf8\x88\x80\x80\x80\"",
    b"\"\xc0\xaf\"",
    b"\"unterminated",
    b"\"\\x41\"",
    b"\"a\x01b\"",
];

/// What a request may be answered.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Expect {
    /// A rejection with this status.
    Status(u16),
    /// Taken (`2xx`) or rejected (`4xx`).
    Either,
}

/// A capture body (for `/captures` or, wrapped, one item of a batch) and what it deserves on
/// its own.
fn capture_body(rng: &mut Rng, n: u64) -> (Vec<u8>, Expect, String) {
    let mut v = capture(n, json!({}));
    match rng.below(12) {
        0..=3 => {
            let changes = rng.range(1, 3);
            let what: Vec<String> = (0..changes).map(|_| mutate(rng, &mut v)).collect();
            (v.to_string().into_bytes(), Expect::Either, what.join(", "))
        }
        4 => {
            let text = v.to_string().into_bytes();
            let cut = rng.below(text.len());
            (
                text[..cut].to_vec(),
                Expect::Status(422),
                format!("truncated at byte {cut}"),
            )
        }
        5 => {
            let depth = *rng.pick(&[100, 129, 1_000, 100_000]);
            let objects = rng.chance(1, 2);
            let p = *rng.pick(POINTERS);
            let unknown = rng.chance(1, 3);
            if unknown {
                v["x_unknown"] = Value::String(HOLE.into());
            } else {
                set(&mut v, p, Value::String(HOLE.into()));
            }
            let expect = if depth > 120 {
                Expect::Status(422)
            } else {
                Expect::Either
            };
            (
                fill_hole(&v, &deep(depth, objects)),
                expect,
                format!(
                    "nesting {depth} deep ({}) at {}",
                    if objects { "objects" } else { "arrays" },
                    if unknown { "/x_unknown" } else { p }
                ),
            )
        }
        6 => {
            let p = *rng.pick(TEXT_POINTERS);
            let bad = *rng.pick(BAD_STRINGS);
            set(&mut v, p, Value::String(HOLE.into()));
            (
                fill_hole(&v, bad),
                Expect::Status(422),
                format!("bad string {:?} at {p}", String::from_utf8_lossy(bad)),
            )
        }
        7 => (vec![], Expect::Status(422), "empty body".into()),
        8 => {
            let len = rng.below(2_000);
            (
                rng.bytes(len),
                Expect::Status(422),
                format!("{len} random bytes"),
            )
        }
        9 => {
            let r = random_value(rng, 5);
            (
                r.to_string().into_bytes(),
                Expect::Status(422),
                "random JSON".into(),
            )
        }
        10 => {
            let text = v.to_string();
            let (body, what) = match rng.below(4) {
                0 => (format!("{text} trailing"), "trailing garbage"),
                1 => (format!("{text}{text}"), "two objects"),
                2 => (format!("\u{feff}{text}"), "byte order mark"),
                _ => (format!("[{text}]"), "capture in an array"),
            };
            (body.into_bytes(), Expect::Status(422), what.into())
        }
        _ => {
            // Duplicate keys: serde_json keeps the last.
            let text = v.to_string();
            let body = format!("{{\"schema\":2,\"kind\":5,{}", &text[1..]);
            (body.into_bytes(), Expect::Either, "duplicate keys".into())
        }
    }
}

/// The error codes of the contract (docs/06 §5.1) and the statuses that carry them.
fn code_fits_status(code: &str, status: u16) -> bool {
    matches!(
        (code, status),
        ("invalid_capture" | "invalid_request", 422)
            | ("invalid_media", 415 | 422)
            | ("too_large", 413)
            | ("unauthorized", 401)
            | ("forbidden", 403)
            | ("not_found", 404 | 405)
            | ("rate_limited", 429)
    )
}

/// A rejection in the contract's shape: `4xx`, JSON, `{"error": {"code", "message"}}`.
fn check_rejection(r: &Reply, case: &Case) {
    assert!(
        (400..500).contains(&r.status),
        "{case}: a client mistake answered {}: {}",
        r.status,
        r.text()
    );
    assert!(
        r.header("content-type")
            .is_some_and(|v| v.starts_with("application/json")),
        "{case}: {} without a JSON content type: {:?}",
        r.status,
        r.headers
    );
    let body = r
        .json()
        .unwrap_or_else(|| panic!("{case}: {} body is not JSON: {}", r.status, r.text()));
    let err = &body["error"];
    let code = err["code"].as_str().unwrap_or_default();
    assert!(
        body.as_object().is_some_and(|o| o.len() == 1)
            && err["message"].as_str().is_some_and(|m| !m.is_empty())
            && code_fits_status(code, r.status),
        "{case}: error body {body} with status {}",
        r.status
    );
}

/// A taken single capture: `201 created` / `200 duplicate` with a card id.
fn check_taken(r: &Reply, case: &Case) -> (String, String) {
    let body = r
        .json()
        .unwrap_or_else(|| panic!("{case}: {} body is not JSON: {}", r.status, r.text()));
    let status = body["status"].as_str().unwrap_or_default().to_owned();
    let card = body["card_id"].as_str().unwrap_or_default().to_owned();
    let expected = if r.status == 201 {
        "created"
    } else {
        "duplicate"
    };
    assert!(
        matches!(r.status, 200 | 201) && status == expected && !card.is_empty(),
        "{case}: {} {body}",
        r.status
    );
    (status, card)
}

/// A batch answer: `200` and one well-formed result per item: `(status, card id)` of each.
fn check_batch(r: &Reply, items: usize, case: &Case) -> Vec<(String, Option<String>)> {
    assert_eq!(r.status, 200, "{case}: {}", r.text());
    let body = r.json().unwrap();
    let results = body["results"]
        .as_array()
        .unwrap_or_else(|| panic!("{case}: {body}"));
    assert_eq!(results.len(), items, "{case}: {body}");
    results
        .iter()
        .map(|item| {
            let status = item["status"].as_str().unwrap_or_default().to_owned();
            match status.as_str() {
                "created" | "duplicate" => assert!(item["card_id"].is_string(), "{case}: {item}"),
                "error" => {
                    let code = item["error"]["code"].as_str().unwrap_or_default();
                    assert!(
                        matches!(code, "invalid_capture" | "invalid_request")
                            && item["error"]["retryable"] == false
                            && item["error"]["message"].is_string(),
                        "{case}: batch item {item}"
                    );
                }
                _ => panic!("{case}: batch item {item}"),
            }
            (status, item["card_id"].as_str().map(str::to_owned))
        })
        .collect()
}

// ------------------------------------------------------------------ the tests

/// Random and malformed capture bodies, single and batched, with right and wrong headers.
#[test]
fn malformed_captures_are_answered_and_write_nothing() {
    const TEST: &str = "malformed_captures_are_answered_and_write_nothing";
    let seed = base_seed();
    let mut rng = Rng::for_test(seed, TEST);
    let fx = Fx::new();
    let started = Instant::now();
    let mut before = fx.snapshot();
    let n_cases = cases(500);
    let mut taken = 0;
    for index in 0..n_cases {
        let n = 1_000 + index as u64 * 100;
        let mut case = Case {
            test: TEST,
            seed,
            index,
            what: String::new(),
        };
        let batch = rng.chance(1, 4);
        let (path, body, mut expect, mut items) = if batch {
            let shape = rng.below(6);
            match shape {
                0..=2 => {
                    let k = rng.range(1, 5);
                    let mut whats = Vec::new();
                    let mut parts = Vec::new();
                    for j in 0..k {
                        let mut v = capture(n + j as u64, json!({}));
                        if rng.chance(2, 3) {
                            whats.push(mutate(&mut rng, &mut v));
                        }
                        parts.push(v);
                    }
                    case.what = format!("batch of {k}: {}", whats.join(", "));
                    let body = json!({ "captures": parts }).to_string().into_bytes();
                    (Some(k), body, Expect::Status(200))
                }
                3 => {
                    let v = random_value(&mut rng, 3);
                    case.what = "batch: captures is random JSON".into();
                    let body = json!({ "captures": v }).to_string().into_bytes();
                    let expect = if v.is_array() {
                        Expect::Either
                    } else {
                        Expect::Status(422)
                    };
                    (None, body, expect)
                }
                4 => {
                    case.what = format!("batch of {} (too many)", MAX_BATCH + 1);
                    let body = json!({ "captures": vec![json!({}); MAX_BATCH + 1] })
                        .to_string()
                        .into_bytes();
                    (None, body, Expect::Status(422))
                }
                _ => {
                    let (raw, _, what) = capture_body(&mut rng, n);
                    case.what = format!("batch body: {what}");
                    // As a whole body: only a JSON object with a `captures` array is taken.
                    (None, raw, Expect::Either)
                }
            }
            .pipe(|(k, body, expect)| ("/subtitle/v1/captures:batch", body, expect, k))
        } else {
            let (body, expect, what) = capture_body(&mut rng, n);
            case.what = what;
            ("/subtitle/v1/captures", body, expect, None)
        };
        let mut req = fx.post_json(path, body);
        // Too big for one capture: refused before it is read.
        if !batch && req.body.len() > MAX_BODY_BYTES {
            expect = Expect::Status(413);
        }
        if rng.chance(1, 6) {
            let ct: &[u8] = rng.pick(&[
                &b""[..],
                b"text/plain",
                b"application/x-www-form-urlencoded",
                b"application/jsonp",
                b"json",
                b"multipart/form-data; boundary=x",
                b"application/\xffjson",
            ]);
            req.headers.retain(|(k, _)| k != "Content-Type");
            if rng.chance(3, 4) {
                req = req.header("Content-Type", ct);
            }
            expect = Expect::Status(415);
            items = None;
            case.what.push_str(" + bad content type");
        } else if rng.chance(1, 8) {
            let ct = *rng.pick(&[
                "APPLICATION/JSON",
                "application/json; charset=utf-8",
                " application/json ;charset=UTF-8",
            ]);
            req.headers.retain(|(k, _)| k != "Content-Type");
            req = req.header("Content-Type", ct);
        }
        if rng.chance(1, 10) {
            let auth: &[u8] = rng.pick(&[
                &b"Bearer sk_fc_wrong"[..],
                b"Basic dXNlcjpwYXNz",
                b"Bearer ",
                b"bearer",
                b"Bearer \xff\xfe",
            ]);
            let long = format!("Bearer {}", "k".repeat(300));
            req.headers.retain(|(k, _)| k != "Authorization");
            match rng.below(3) {
                0 => {}
                1 => req = req.header("Authorization", long),
                _ => req = req.header("Authorization", auth),
            }
            expect = Expect::Status(401);
            items = None;
            case.what.push_str(" + bad key");
        }
        let r = fx.send(&req, &case);
        case.what = format!("{} — {}", case.what, req.describe());
        assert!(r.status < 500, "{case}: {} {}", r.status, r.text());
        match expect {
            Expect::Status(s) => assert_eq!(r.status, s, "{case}: {}", r.text()),
            Expect::Either => {}
        }
        let wrote = if r.status >= 400 {
            check_rejection(&r, &case);
            false
        } else if batch {
            let results = match items {
                Some(k) => check_batch(&r, k, &case),
                None => {
                    let k = r.json().unwrap()["results"].as_array().map_or(0, Vec::len);
                    check_batch(&r, k, &case)
                }
            };
            results.iter().any(|(s, _)| s == "created")
        } else {
            check_taken(&r, &case).0 == "created"
        };
        let after = fx.snapshot();
        if wrote {
            taken += 1;
        } else {
            assert!(
                after == before,
                "{case}: answered {} but wrote: {}",
                r.status,
                before.diff(&after)
            );
        }
        before = after;
    }
    let case = Case {
        test: TEST,
        seed,
        index: n_cases,
        what: "after the run".into(),
    };
    fx.still_serves(1, &case);
    eprintln!(
        "{TEST}: seed={seed:#x} cases={n_cases} taken={taken} in {:?}",
        started.elapsed()
    );
}

/// `x.pipe(f)` = `f(x)`: keeps the batch generator readable.
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}

/// Wrong methods, odd paths, broken framing (lengths, encodings), odd headers.
#[test]
fn wrong_methods_paths_and_framing_are_answered_and_write_nothing() {
    const TEST: &str = "wrong_methods_paths_and_framing_are_answered_and_write_nothing";
    let seed = base_seed();
    let mut rng = Rng::for_test(seed, TEST);
    let fx = Fx::new();
    let started = Instant::now();
    let mut before = fx.snapshot();
    let n_cases = cases(600);
    const METHODS: &[&str] = &[
        "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "TRACE", "OPTIONS", "FOO", "post",
    ];
    const PATHS: &[&str] = &[
        "/subtitle/v1/captures",
        "/subtitle/v1/captures:batch",
        "/subtitle/v1/media",
        "/subtitle/v1/me",
        "/subtitle/v1/decks",
        "/subtitle/v1/known-words",
        "/subtitle/v1/known-words?lang=%ff%fe&cursor=%%%zz",
        "/subtitle/v1/known-words?cursor=not-a-cursor",
        "/subtitle",
        "/subtitle/",
        "/subtitle/v1",
        "/subtitle/v1/",
        "/subtitle/v2/captures",
        "/subtitle/v1/captures/",
        "/subtitle/v1/captures%3Abatch",
        "/subtitle/v1/../v1/captures",
        "/subtitle//v1/captures",
        "/subtitle/v1/captures?x=1&&=&%",
        "/subtitle/v1/CAPTURES",
        "/subtitle/v1/captures:batch:batch",
    ];
    for index in 0..n_cases {
        let mut case = Case {
            test: TEST,
            seed,
            index,
            what: String::new(),
        };
        let method = *rng.pick(METHODS);
        let path = if rng.chance(1, 8) {
            let len = rng.range(1, 30);
            let seg: String = (0..len)
                .map(|_| *rng.pick(&['a', 'Z', '9', '-', '.', '_', '~', '%', ':', '@', '!']))
                .collect();
            format!("/subtitle/v1/{seg}")
        } else {
            (*rng.pick(PATHS)).to_owned()
        };
        let body: Vec<u8> = match rng.below(4) {
            0 => vec![],
            1 => capture(2_000_000 + index as u64, json!({}))
                .to_string()
                .into_bytes(),
            2 => {
                let len = rng.below(300);
                rng.bytes(len)
            }
            _ => b"{\"captures\":[]}".to_vec(),
        };
        let mut req = Req::new(method, &path)
            .header("Authorization", &fx.auth)
            .header("Content-Type", "application/json")
            .body(body);
        // Framing hyper refuses before routing: its own bare 4xx is the answer.
        let mut framing = false;
        // A short body cut or badly encoded: never "too large".
        let mut broken_body = false;
        match rng.below(14) {
            0 => {
                let v: &[u8] = rng.pick(&[
                    &b"abc"[..],
                    b"-1",
                    b"1e3",
                    b"+5",
                    b"18446744073709551616",
                    b"0x10",
                    b"",
                ]);
                req.length = Length::Raw(v.to_vec());
                framing = true;
                case.what = format!("content-length {:?}", String::from_utf8_lossy(v));
            }
            1 => {
                // Two different lengths.
                let n = req.body.len();
                req.length = Length::Raw(n.to_string().into_bytes());
                req = req.header("Content-Length", (n + 1).to_string());
                framing = true;
                case.what = "two content-lengths".into();
            }
            2 => {
                // Length and chunked at once (request smuggling).
                req.length = Length::Raw(req.body.len().to_string().into_bytes());
                req.half_close = true;
                req = req.header("Transfer-Encoding", "chunked");
                framing = true;
                case.what = "content-length + chunked".into();
            }
            3 if !req.body.is_empty() => {
                // Declared shorter than sent: the rest is never read as this request's body.
                let n = rng.below(req.body.len());
                req.length = Length::Raw(n.to_string().into_bytes());
                case.what = format!("declared {n} of {} bytes", req.body.len());
            }
            4 => {
                // Declared longer than sent, then the client stops sending.
                let n = req.body.len() + rng.range(1, 1_000);
                req.length = Length::Raw(n.to_string().into_bytes());
                req.half_close = true;
                broken_body = true;
                case.what = format!("declared {n}, sent {} and closed", req.body.len());
            }
            5 => {
                // A proper chunked body.
                let mut chunked = Vec::new();
                for part in req.body.chunks(rng.range(1, 64)) {
                    chunked.extend_from_slice(format!("{:x}\r\n", part.len()).as_bytes());
                    chunked.extend_from_slice(part);
                    chunked.extend_from_slice(b"\r\n");
                }
                chunked.extend_from_slice(b"0\r\n\r\n");
                req.body = chunked;
                req.length = Length::Omit;
                req = req.header("Transfer-Encoding", "chunked");
                case.what = "chunked".into();
            }
            6 => {
                // A broken chunked body.
                let bad: &[u8] = rng.pick(&[
                    &b"zz\r\nabc\r\n0\r\n\r\n"[..],
                    b"5\r\nab",
                    b"ffffffffffffffffff\r\n",
                    b"3\r\nabcdef\r\n0\r\n\r\n",
                ]);
                req.body = bad.to_vec();
                req.length = Length::Omit;
                req.half_close = true;
                req = req.header("Transfer-Encoding", "chunked");
                broken_body = true;
                case.what = format!("broken chunked {:?}", String::from_utf8_lossy(bad));
            }
            7 => {
                req.half_close = true;
                req = req.header("Transfer-Encoding", "gzip");
                framing = true;
                case.what = "transfer-encoding gzip".into();
            }
            8 => {
                let host = *rng.pick(&["evil.example", "127.0.0.1", "localhost:1", "[::1]"]);
                req = req.header("Host", host);
                case.what = format!("host {host}");
            }
            9 => {
                let origin = *rng.pick(&[
                    "https://evil.example",
                    "null",
                    "chrome-extension://abcdefghijklmnopabcdefghijklmnop",
                    "moz-extension://1234",
                ]);
                req = req.header("Origin", origin);
                case.what = format!("origin {origin}");
            }
            10 => {
                let len = rng.range(1, 100);
                let v = rng.bytes(len);
                let v: Vec<u8> = v.into_iter().map(|b| b | 0x80).collect();
                let name =
                    *rng.pick(&["X-Capture-Id", "X-Media-Kind", "Accept", "Idempotency-Key"]);
                req = req.header(name, v);
                case.what = format!("high bytes in {name}");
            }
            11 => {
                for i in 0..rng.range(90, 140) {
                    req = req.header(&format!("X-Pad-{i}"), "x");
                }
                framing = true;
                case.what = "many headers".into();
            }
            12 => {
                req = req.header("X-Long", "v".repeat(rng.range(9_000, 70_000)));
                framing = true;
                case.what = "one long header".into();
            }
            _ => case.what = "plain".into(),
        }
        let r = fx.send(&req, &case);
        case.what = format!("{} — {}", case.what, req.describe());
        assert!(r.status < 500, "{case}: {} {}", r.status, r.text());
        assert!(
            !(broken_body && r.status == 413),
            "{case}: a short broken body answered 413: {}",
            r.text()
        );
        let wrote = if r.status >= 400 {
            if method == "HEAD" {
                assert!(r.status < 500, "{case}: {}", r.status);
            } else if !(framing && r.body.is_empty()) {
                check_rejection(&r, &case);
            }
            false
        } else {
            // Taken: only a POST of a capture or a batch, or something that reads.
            match (r.status, method) {
                (204, "OPTIONS") => false,
                (200, "GET" | "HEAD") => false,
                (200 | 201, "POST") => r.json().is_some_and(|b| {
                    b["status"] == "created"
                        || b["results"]
                            .as_array()
                            .is_some_and(|a| a.iter().any(|i| i["status"] == "created"))
                }),
                _ => panic!("{case}: unexpected {} {}", r.status, r.text()),
            }
        };
        let after = fx.snapshot();
        if !wrote {
            assert!(
                after == before,
                "{case}: answered {} but wrote: {}",
                r.status,
                before.diff(&after)
            );
        }
        before = after;
    }
    let case = Case {
        test: TEST,
        seed,
        index: n_cases,
        what: "after the run".into(),
    };
    fx.still_serves(2, &case);
    eprintln!(
        "{TEST}: seed={seed:#x} cases={n_cases} in {:?}",
        started.elapsed()
    );
}

const JPEG_MAGIC: &[u8] = b"\xff\xd8\xff\xe0\x00\x10JFIF\x00";
const WAV_MAGIC: &[u8] = b"RIFF\x24\x00\x00\x00WAVEfmt ";

/// Media uploads with every header and body wrong in turn, judged against an oracle of the
/// contract's rules; a taken upload's resend keeps its upload id.
#[test]
fn media_uploads_follow_the_rules_and_rejections_write_nothing() {
    const TEST: &str = "media_uploads_follow_the_rules_and_rejections_write_nothing";
    let seed = base_seed();
    let mut rng = Rng::for_test(seed, TEST);
    let fx = Fx::new();
    let started = Instant::now();
    let mut before = fx.snapshot();
    let n_cases = cases(600);
    let mut taken = 0;
    for index in 0..n_cases {
        let mut case = Case {
            test: TEST,
            seed,
            index,
            what: String::new(),
        };
        let n = 3_000_000 + index as u64;
        // (header value, valid?) — `None` = header left out.
        let cid: (Option<Vec<u8>>, bool) = match rng.below(8) {
            0 => (None, false),
            1 => (Some(b"not-a-uuid".to_vec()), false),
            2 => (Some(id(n).to_uppercase().into_bytes()), false),
            3 => (Some(format!("  {}  ", id(n)).into_bytes()), true),
            4 => (Some(id(n).replace("-7a41-", "-4a41-").into_bytes()), false),
            5 => (Some(b"\xff\xfe".to_vec()), false),
            _ => (Some(id(n).into_bytes()), true),
        };
        // Kind: (value, Some(is screenshot) if valid).
        let kind: (Option<&[u8]>, Option<bool>) = match rng.below(9) {
            0 => (None, None),
            1 => (Some(b"video".as_slice()), None),
            2 => (Some(b"".as_slice()), None),
            3 => (Some(b"Audio".as_slice()), Some(false)),
            4 => (Some(b" screenshot ".as_slice()), Some(true)),
            5 => (Some(b"scr\xffeenshot".as_slice()), None),
            6 | 7 => (Some(b"screenshot".as_slice()), Some(true)),
            _ => (Some(b"audio".as_slice()), Some(false)),
        };
        // Content type: (value, Some(is jpeg) if an accepted type).
        let mime: (Option<&[u8]>, Option<bool>) = match rng.below(12) {
            0 => (None, None),
            1 => (Some(b"image/png".as_slice()), None),
            2 => (Some(b"application/json".as_slice()), None),
            3 => (Some(b"audio/mpeg".as_slice()), None),
            4 => (Some(b"image/jpg".as_slice()), Some(true)),
            5 => (Some(b"audio/x-wav".as_slice()), Some(false)),
            6 => (Some(b"Audio/WAV; codecs=1".as_slice()), Some(false)),
            7 => (Some(b"\xffimage/jpeg".as_slice()), None),
            8 | 9 => (Some(b"image/jpeg".as_slice()), Some(true)),
            _ => (Some(b"audio/wav".as_slice()), Some(false)),
        };
        // Body: (bytes, Some(is jpeg) if its magic is one of ours).
        let tail_len = rng.below(500);
        let mut tail = rng.bytes(tail_len);
        tail.extend_from_slice(format!("#{n}").as_bytes());
        let body: (Vec<u8>, Option<bool>) = match rng.below(10) {
            0 => (vec![], None),
            1 => (rng.bytes(tail_len), None),
            2 => ([b"\x89PNG\r\n\x1a\n".as_slice(), &tail].concat(), None),
            3 => (
                [b"RIFF\x24\x00\x00\x00WEBP".as_slice(), &tail].concat(),
                None,
            ),
            4 => ([b"ID3".as_slice(), &tail].concat(), None),
            5 => (b"\xff\xd8".to_vec(), None),
            6 | 7 => ([JPEG_MAGIC, &tail].concat(), Some(true)),
            _ => ([WAV_MAGIC, &tail].concat(), Some(false)),
        };
        // Half the time the type and the body agree with a valid kind (else too few are taken).
        let (mime, body) = match kind.1 {
            Some(true) if rng.chance(1, 2) => (
                (Some(b"image/jpeg".as_slice()), Some(true)),
                ([JPEG_MAGIC, &tail].concat(), Some(true)),
            ),
            Some(false) if rng.chance(1, 2) => (
                (Some(b"audio/wav".as_slice()), Some(false)),
                ([WAV_MAGIC, &tail].concat(), Some(false)),
            ),
            _ => (mime, body),
        };
        let mut req = Req::new("POST", "/subtitle/v1/media").header("Authorization", &fx.auth);
        if let Some(v) = &mime.0 {
            req = req.header("Content-Type", v);
        }
        if let Some(v) = &cid.0 {
            req = req.header("X-Capture-Id", v);
        }
        if let Some(v) = &kind.0 {
            req = req.header("X-Media-Kind", v);
        }
        req = req.body(body.0.clone());
        // The oracle.
        let ok =
            cid.1 && kind.1.is_some() && mime.1.is_some() && kind.1 == mime.1 && body.1 == kind.1;
        case.what = req.describe();
        let r = fx.send(&req, &case);
        assert!(r.status < 500, "{case}: {} {}", r.status, r.text());
        if ok {
            assert_eq!(r.status, 201, "{case}: {}", r.text());
            let up = r.json().unwrap()["upload_id"]
                .as_str()
                .unwrap_or_else(|| panic!("{case}: {}", r.text()))
                .to_owned();
            assert!(up.starts_with("up_"), "{case}: {up}");
            taken += 1;
            if rng.chance(1, 3) {
                // The outbox resends it: the same upload, nothing new.
                let mid = fx.snapshot();
                let again = fx.send(&req, &case);
                assert_eq!(again.status, 201, "{case}: {}", again.text());
                assert_eq!(again.json().unwrap()["upload_id"], up.as_str(), "{case}");
                let after = fx.snapshot();
                assert_eq!(
                    after.rows("subtitle_uploads"),
                    mid.rows("subtitle_uploads"),
                    "{case}"
                );
                assert_eq!(after.files, mid.files, "{case}");
            }
            before = fx.snapshot();
        } else {
            check_rejection(&r, &case);
            let missing_type = mime.0.is_none() || mime.0.is_some_and(|m| m.first() == Some(&0xff));
            if missing_type || (cid.1 && kind.1.is_some() && mime.1.is_none()) {
                assert_eq!(r.status, 415, "{case}: {}", r.text());
            }
            let after = fx.snapshot();
            assert!(
                after == before,
                "{case}: answered {} but wrote: {}",
                r.status,
                before.diff(&after)
            );
        }
    }
    let case = Case {
        test: TEST,
        seed,
        index: n_cases,
        what: "after the run".into(),
    };
    fx.still_serves(3, &case);
    eprintln!(
        "{TEST}: seed={seed:#x} cases={n_cases} taken={taken} in {:?}",
        started.elapsed()
    );
}

/// The size limits, at and just past each one, with the body really sent.
#[test]
fn bodies_past_the_limits_are_refused_and_write_nothing() {
    const TEST: &str = "bodies_past_the_limits_are_refused_and_write_nothing";
    let fx = Fx::new();
    let case = |what: &str| Case {
        test: TEST,
        seed: 0,
        index: 0,
        what: what.into(),
    };
    let cid = id(4_000_001);
    let padded = |len: usize, magic: &[u8]| {
        let mut b = magic.to_vec();
        b.resize(len, 0x5a);
        b
    };
    let refusals: Vec<(&str, Req, u16)> = vec![
        (
            "audio declared and sent past the server's limit",
            fx.upload(
                &cid,
                "audio",
                "audio/wav",
                &padded(MAX_AUDIO_BYTES + 1, WAV_MAGIC),
            ),
            413,
        ),
        (
            "audio past the limit, chunked (no length to refuse it by)",
            {
                let body = padded(MAX_AUDIO_BYTES + 1, WAV_MAGIC);
                let mut chunked = Vec::new();
                for part in body.chunks(256 * 1024) {
                    chunked.extend_from_slice(format!("{:x}\r\n", part.len()).as_bytes());
                    chunked.extend_from_slice(part);
                    chunked.extend_from_slice(b"\r\n");
                }
                chunked.extend_from_slice(b"0\r\n\r\n");
                let mut r = fx.upload(&cid, "audio", "audio/wav", b"");
                r.body = chunked;
                r.length = Length::Omit;
                r.header("Transfer-Encoding", "chunked")
            },
            413,
        ),
        (
            "a screenshot one byte past its limit",
            fx.upload(
                &cid,
                "screenshot",
                "image/jpeg",
                &padded(MAX_SCREENSHOT_BYTES + 1, JPEG_MAGIC),
            ),
            413,
        ),
        (
            "a capture body one byte past the limit",
            {
                let mut body = capture(4_000_002, json!({})).to_string().into_bytes();
                body.resize(MAX_BODY_BYTES + 1, b' ');
                fx.post_json("/subtitle/v1/captures", body)
            },
            413,
        ),
        (
            "a capture declared huge, nothing sent",
            {
                let mut r = fx.post_json("/subtitle/v1/captures", "");
                r.length = Length::Raw(b"99999999999".to_vec());
                r
            },
            413,
        ),
    ];
    for (what, req, status) in refusals {
        let case = case(what);
        let before = fx.snapshot();
        let r = fx.send(&req, &case);
        assert_eq!(r.status, status, "{case}: {}", r.text());
        check_rejection(&r, &case);
        assert_eq!(r.json().unwrap()["error"]["code"], "too_large", "{case}");
        let after = fx.snapshot();
        assert!(after == before, "{case}: wrote {}", before.diff(&after));
    }
    // Exactly at each limit: taken.
    let at = [
        fx.upload(
            &cid,
            "screenshot",
            "image/jpeg",
            &padded(MAX_SCREENSHOT_BYTES, JPEG_MAGIC),
        ),
        fx.upload(
            &cid,
            "audio",
            "audio/wav",
            &padded(MAX_AUDIO_BYTES, WAV_MAGIC),
        ),
    ];
    for req in &at {
        let r = fx.send(req, &case("at the limit"));
        assert_eq!(r.status, 201, "{}", r.text());
    }
    let mut body = capture(4_000_003, json!({})).to_string().into_bytes();
    body.resize(MAX_BODY_BYTES, b' ');
    let r = fx.send(
        &fx.post_json("/subtitle/v1/captures", body),
        &case("capture at the limit"),
    );
    assert_eq!(r.status, 201, "{}", r.text());
    fx.still_serves(4, &case("after the limits"));
}

/// One job of the replay run.
#[derive(Clone, Debug)]
enum Job {
    /// Upload `kind` (`true` = screenshot) of capture `i`.
    Upload(usize, bool),
    /// Capture `i` on its own; `true` = its twin (another capture id, the same line).
    Capture(usize, bool),
    /// Several in one batch.
    Batch(Vec<(usize, bool)>),
}

/// Runs `jobs` on `threads` threads at once; answers in job order.
fn run_parallel(jobs: &[Job], threads: usize, f: impl Fn(&Job) -> Reply + Sync) -> Vec<Reply> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<Reply>>> = Mutex::new((0..jobs.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(job) = jobs.get(i) else {
                    return;
                };
                let r = f(job);
                out.lock().unwrap()[i] = Some(r);
            });
        }
    });
    out.into_inner()
        .unwrap()
        .into_iter()
        .map(Option::unwrap)
        .collect()
}

/// Replays, resends under another capture id, batches repeating a capture, captures before
/// their media, all racing on several threads: one note per word, one upload id per picture
/// or sound, nothing lost, nothing doubled.
#[test]
fn replayed_reordered_and_concurrent_captures_make_one_note_each() {
    const TEST: &str = "replayed_reordered_and_concurrent_captures_make_one_note_each";
    let seed = base_seed();
    let mut rng = Rng::for_test(seed, TEST);
    let fx = Fx::new();
    let started = Instant::now();
    // `FC_FUZZ_CASES` counts requests elsewhere; a round here is ~75 of them.
    let rounds = cases(600).div_ceil(75);
    for round in 0..rounds {
        let case = |what: String| Case {
            test: TEST,
            seed,
            index: round,
            what,
        };
        let base = 5_000_000 + round as u64 * 1_000;
        let k = rng.range(6, 14);
        let twin_id = |i: usize| id(base + 500 + i as u64);
        // Per capture: media sent before it (`Some(true)`), after it (`Some(false)`), or none.
        let shot: Vec<Option<bool>> = (0..k)
            .map(|_| [None, Some(true), Some(false)][rng.below(3)])
            .collect();
        let audio: Vec<Option<bool>> = (0..k)
            .map(|_| [None, Some(true), Some(false)][rng.below(3)])
            .collect();
        let bytes = |i: usize, screenshot: bool| {
            let tag = format!("#{}-{i}", base);
            if screenshot {
                [JPEG_MAGIC, tag.as_bytes()].concat()
            } else {
                [WAV_MAGIC, tag.as_bytes()].concat()
            }
        };
        let before = fx.snapshot();
        let upload_req = |i: usize, screenshot: bool| {
            if screenshot {
                fx.upload(
                    &id(base + i as u64),
                    "screenshot",
                    "image/jpeg",
                    &bytes(i, true),
                )
            } else {
                fx.upload(&id(base + i as u64), "audio", "audio/wav", &bytes(i, false))
            }
        };
        let mut upload_ids: BTreeMap<(usize, bool), BTreeSet<String>> = BTreeMap::new();
        let upload_round = |rng: &mut Rng,
                            upload_ids: &mut BTreeMap<(usize, bool), BTreeSet<String>>,
                            early: bool,
                            what: &str| {
            let mut jobs = Vec::new();
            for i in 0..k {
                for (screenshot, when) in [(true, shot[i]), (false, audio[i])] {
                    if when == Some(early) {
                        for _ in 0..rng.range(1, 3) {
                            jobs.push(Job::Upload(i, screenshot));
                        }
                    }
                }
            }
            rng.shuffle(&mut jobs);
            let replies = run_parallel(&jobs, 4, |job| {
                let Job::Upload(i, screenshot) = job else {
                    unreachable!()
                };
                fx.send(
                    &upload_req(*i, *screenshot),
                    &case(format!("{what} {job:?}")),
                )
            });
            for (job, r) in jobs.iter().zip(&replies) {
                let Job::Upload(i, screenshot) = job else {
                    unreachable!()
                };
                let c = case(format!("{what} {job:?}"));
                assert_eq!(r.status, 201, "{c}: {}", r.text());
                let up = r.json().unwrap()["upload_id"].as_str().unwrap().to_owned();
                upload_ids.entry((*i, *screenshot)).or_default().insert(up);
            }
        };
        upload_round(&mut rng, &mut upload_ids, true, "early upload");
        // The captures: the early ones name their uploads; the late ones name uploads that
        // do not exist yet (the card simply goes without).
        let body_of = |i: usize, twin: bool, ids: &BTreeMap<(usize, bool), BTreeSet<String>>| {
            let mut media = serde_json::Map::new();
            for (screenshot, when, key, mime) in [
                (true, shot[i], "screenshot", "image/jpeg"),
                (false, audio[i], "audio", "audio/wav"),
            ] {
                let up = match when {
                    Some(true) => ids[&(i, screenshot)].iter().next().unwrap().clone(),
                    Some(false) => format!("up_not_yet_{i}"),
                    None => continue,
                };
                media.insert(key.into(), json!({ "upload_id": up, "mime": mime }));
            }
            if twin {
                // The same line captured again: its own capture id, no uploads of its own.
                let mut c = capture(base + i as u64, json!({}));
                c["capture_id"] = json!(twin_id(i));
                return c;
            }
            capture(base + i as u64, Value::Object(media))
        };
        let mut jobs = Vec::new();
        for i in 0..k {
            for _ in 0..rng.range(1, 4) {
                jobs.push(Job::Capture(i, false));
            }
            if rng.chance(1, 3) {
                for _ in 0..rng.range(1, 2) {
                    jobs.push(Job::Capture(i, true));
                }
            }
        }
        for _ in 0..rng.range(1, 4) {
            let items = (0..rng.range(1, 6))
                .map(|_| (rng.below(k), rng.chance(1, 4)))
                .collect();
            jobs.push(Job::Batch(items));
        }
        rng.shuffle(&mut jobs);
        let ids_now = upload_ids.clone();
        let replies = run_parallel(&jobs, 4, |job| {
            let req = match job {
                Job::Capture(i, twin) => fx.post_json(
                    "/subtitle/v1/captures",
                    body_of(*i, *twin, &ids_now).to_string(),
                ),
                Job::Batch(items) => {
                    let captures: Vec<Value> = items
                        .iter()
                        .map(|(i, twin)| body_of(*i, *twin, &ids_now))
                        .collect();
                    fx.post_json(
                        "/subtitle/v1/captures:batch",
                        json!({ "captures": captures }).to_string(),
                    )
                }
                Job::Upload(..) => unreachable!(),
            };
            fx.send(&req, &case(format!("{job:?}")))
        });
        // Per word: every answer names one card, and exactly one answer created it.
        let mut cards: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
        let mut created: BTreeMap<usize, usize> = BTreeMap::new();
        // Words whose card the twin made (it names no uploads: that card has no media).
        let mut by_twin: BTreeSet<usize> = BTreeSet::new();
        for (job, r) in jobs.iter().zip(&replies) {
            let c = case(format!("{job:?}"));
            assert!(r.status < 500, "{c}: {} {}", r.status, r.text());
            match job {
                Job::Capture(i, twin) => {
                    let (status, card) = check_taken(r, &c);
                    if *twin && status == "created" {
                        by_twin.insert(*i);
                    }
                    cards.entry(*i).or_default().insert(card);
                    *created.entry(*i).or_default() += usize::from(status == "created");
                }
                Job::Batch(items) => {
                    let results = check_batch(r, items.len(), &c);
                    for ((i, twin), (status, card)) in items.iter().zip(results) {
                        assert_ne!(status, "error", "{c}: {}", r.text());
                        if *twin && status == "created" {
                            by_twin.insert(*i);
                        }
                        cards.entry(*i).or_default().insert(card.unwrap());
                        *created.entry(*i).or_default() += usize::from(status == "created");
                    }
                }
                Job::Upload(..) => unreachable!(),
            }
        }
        let c = case(format!("{k} words, jobs {jobs:?}"));
        for i in 0..k {
            assert_eq!(cards[&i].len(), 1, "{c}: word {i} cards {:?}", cards[&i]);
            assert_eq!(
                created[&i], 1,
                "{c}: word {i} created {} times",
                created[&i]
            );
        }
        upload_round(&mut rng, &mut upload_ids, false, "late upload");
        for (key, ids) in &upload_ids {
            assert_eq!(ids.len(), 1, "{c}: upload {key:?} got ids {ids:?}");
        }
        let after = fx.snapshot();
        let uploads = upload_ids.len();
        assert_eq!(after.rows("notes"), before.rows("notes") + k, "{c}");
        assert_eq!(after.rows("cards"), before.rows("cards") + k, "{c}");
        assert_eq!(
            after.rows("subtitle_captures"),
            before.rows("subtitle_captures") + k,
            "{c}"
        );
        assert_eq!(
            after.rows("subtitle_uploads"),
            before.rows("subtitle_uploads") + uploads,
            "{c}"
        );
        assert_eq!(after.rows("media"), before.rows("media") + uploads, "{c}");
        assert_eq!(after.files.len(), before.files.len() + uploads, "{c}");
        // Media sent before the capture reached its card; media sent after did not.
        let page = fx.t.ok::<SearchCardsMethod>(SearchCards {
            query: String::new(),
            sort: None,
            descending: None,
            notes_only: None,
            offset: None,
            limit: Some(500),
            cursor: None,
        });
        for i in 0..k {
            let card = cards[&i].iter().next().unwrap();
            let row = page
                .items
                .iter()
                .find(|r| r.card_id.to_string() == *card)
                .unwrap_or_else(|| panic!("{c}: card {card} of word {i} not found"));
            let note = fx.t.ok::<NotesGet>(NoteRef { id: row.note_id });
            for (field, when) in [("image", shot[i]), ("audio", audio[i])] {
                let has = note
                    .fields
                    .get(field)
                    .is_some_and(|v| v.starts_with("media:"));
                assert_eq!(
                    has,
                    when == Some(true) && !by_twin.contains(&i),
                    "{c}: word {i} {field} = {:?} (sent {:?})",
                    note.fields.get(field),
                    when
                );
            }
        }
    }
    let case = Case {
        test: TEST,
        seed,
        index: rounds,
        what: "after the run".into(),
    };
    fx.still_serves(5, &case);
    eprintln!(
        "{TEST}: seed={seed:#x} rounds={rounds} in {:?}",
        started.elapsed()
    );
}

/// Regression (found by the framing fuzz above): a body that ended before its
/// `Content-Length`, or whose chunked encoding was broken, was answered `413 too_large` — "too
/// big", which the extension takes as final — although it was a few bytes long. It is a
/// malformed request: `422 invalid_request`, and nothing is written.
#[test]
fn a_body_cut_short_or_badly_chunked_is_invalid_not_too_large() {
    const TEST: &str = "a_body_cut_short_or_badly_chunked_is_invalid_not_too_large";
    let fx = Fx::new();
    let cut = |mut req: Req, declared: usize| {
        req.length = Length::Raw(declared.to_string().into_bytes());
        req.half_close = true;
        req
    };
    let chunked = |mut req: Req, raw: &[u8]| {
        req.body = raw.to_vec();
        req.length = Length::Omit;
        req.half_close = true;
        req.header("Transfer-Encoding", "chunked")
    };
    let c = capture(7_000_001, json!({})).to_string();
    let wav = [WAV_MAGIC, b"cut"].concat();
    let reqs = [
        (
            "capture cut short",
            cut(
                fx.post_json("/subtitle/v1/captures", c.clone()),
                c.len() + 10,
            ),
        ),
        (
            "batch cut short",
            cut(
                fx.post_json("/subtitle/v1/captures:batch", "{\"captures\":[]}"),
                100,
            ),
        ),
        (
            "upload cut short",
            cut(fx.upload(&id(7_000_001), "audio", "audio/wav", &wav), 100),
        ),
        (
            "capture with a broken chunk size",
            chunked(
                fx.post_json("/subtitle/v1/captures", ""),
                b"zz\r\nabc\r\n0\r\n\r\n",
            ),
        ),
        (
            "upload with a chunk longer than said",
            chunked(
                fx.upload(&id(7_000_001), "audio", "audio/wav", b""),
                b"3\r\nabcdef\r\n0\r\n\r\n",
            ),
        ),
    ];
    for (what, req) in reqs {
        let case = Case {
            test: TEST,
            seed: 0,
            index: 0,
            what: what.into(),
        };
        let before = fx.snapshot();
        let r = fx.send(&req, &case);
        assert_eq!(r.status, 422, "{case}: {}", r.text());
        check_rejection(&r, &case);
        assert_eq!(
            r.json().unwrap()["error"]["code"],
            "invalid_request",
            "{case}"
        );
        let after = fx.snapshot();
        assert!(after == before, "{case}: wrote {}", before.diff(&after));
    }
    let case = Case {
        test: TEST,
        seed: 0,
        index: 1,
        what: "after".into(),
    };
    fx.still_serves(7, &case);
}
