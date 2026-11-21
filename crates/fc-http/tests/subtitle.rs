//! The Subtitle extension's contract over real TCP (v1.2, docs/06 §5.1): `/subtitle/v1/*` with
//! the payloads of `docs/flashcards-api.md` (the extension's repository), next to the local
//! API's `/v1/*`, on one server.
//!
//! The client is a few lines of raw HTTP/1.1 (bodies may be bytes) so the test exercises
//! exactly what is on the wire.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fc_api::v1::common::Empty;
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::NoteRef;
use fc_api::v1::subtitle::{CreateSubtitleKey, SubtitleKeyRef};
use fc_app::testing::{TempSqlite, TestApp};
use fc_http::{Server, ServerConfig};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;

const EXT: &str = "chrome-extension://abcdefghijklmnopabcdefghijklmnop";
const JPEG: &[u8] = b"\xff\xd8\xff\xe0\x00\x10JFIF\x00screenshot";
const WAV: &[u8] = b"RIFF\x24\x00\x00\x00WAVEfmt clip";

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Value,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The contract's `error.code`.
    fn error(&self) -> &str {
        self.body["error"]["code"].as_str().unwrap_or_default()
    }
}

fn send(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Reply {
    let mut s = TcpStream::connect(addr).unwrap();
    let mut req = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        req.push_str(&format!("Host: 127.0.0.1:{}\r\n", addr.port()));
    }
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    let declared = headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("content-length"));
    if (!body.is_empty() || method == "POST") && !declared {
        req.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    req.push_str("\r\n");
    let mut bytes = req.into_bytes();
    bytes.extend_from_slice(body);
    s.write_all(&bytes).unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, rest) = text.split_once("\r\n\r\n").unwrap();
    let mut lines = head.lines();
    let status: u16 = lines
        .next()
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    let chunked = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.contains("chunked"));
    let payload = if chunked {
        dechunk(rest)
    } else {
        rest.to_owned()
    };
    Reply {
        status,
        headers,
        body: serde_json::from_str(&payload).unwrap_or(Value::Null),
    }
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

struct Fx {
    t: TestApp,
    server: Server,
    /// `Authorization: Bearer <key>`.
    auth: String,
}

impl Fx {
    /// A file-backed app with only the `subtitle` module on (the local API stays off), a key
    /// made in the app, served on a free loopback port.
    fn new() -> Self {
        let t = TestApp::new(Arc::new(TempSqlite::new()));
        t.ok::<ModulesSetEnabled>(SetModuleEnabled {
            id: "subtitle".into(),
            enabled: true,
        });
        let key = t.ok::<SubtitleKeysCreate>(CreateSubtitleKey {
            name: "Chrome".into(),
        });
        let app = Arc::new(TestApp::new(t.store.clone()).app);
        let server = Server::start(app, ServerConfig::localhost(0)).unwrap();
        Self {
            t,
            server,
            auth: format!("Bearer {}", key.secret),
        }
    }

    fn addr(&self) -> SocketAddr {
        self.server.local_addr()
    }

    fn get(&self, path: &str) -> Reply {
        send(
            self.addr(),
            "GET",
            path,
            &[
                ("Authorization", &self.auth),
                ("Accept", "application/json"),
            ],
            b"",
        )
    }

    fn post_json(&self, path: &str, body: &Value) -> Reply {
        send(
            self.addr(),
            "POST",
            path,
            &[
                ("Authorization", &self.auth),
                ("Accept", "application/json"),
                ("Content-Type", "application/json"),
            ],
            body.to_string().as_bytes(),
        )
    }

    fn upload(&self, capture_id: &str, kind: &str, mime: &str, bytes: &[u8]) -> Reply {
        send(
            self.addr(),
            "POST",
            "/subtitle/v1/media",
            &[
                ("Authorization", &self.auth),
                ("Content-Type", mime),
                ("X-Capture-Id", capture_id),
                ("X-Media-Kind", kind),
            ],
            bytes,
        )
    }
}

const ID: &str = "0192812a-6f3c-7a41-9c2e-5d1f0b8e4a77";

/// The contract's example, verbatim but for the upload ids.
fn example(shot: &str, audio: &str) -> Value {
    json!({
      "schema": 1,
      "capture_id": ID,
      "kind": "word",
      "lang": { "target": "en", "native": "tr" },
      "term": { "surface": "keys", "lemma": "key", "pos": "noun" },
      "definition": { "text": "anahtar", "source": "dict" },
      "context": {
        "sentence": "Almost. I just need to pick up my keys.",
        "translation": "Neredeyse. Sadece anahtarlarımı almam lazım.",
        "term_offset": [34, 38]
      },
      "source": {
        "type": "video",
        "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=83s",
        "title": "Never Gonna Give You Up",
        "site": "youtube",
        "video_id": "dQw4w9WgXcQ",
        "t_start_ms": 83450,
        "t_end_ms": 86120
      },
      "media": {
        "screenshot": { "upload_id": shot, "mime": "image/jpeg" },
        "audio": { "upload_id": audio, "mime": "audio/wav" }
      },
      "deck_id": null,
      "tags": ["subtitle"],
      "created_at": "2026-10-12T10:00:00.000Z",
      "client": { "name": "subtitle-extension", "ext_version": "3.1.3" }
    })
}

fn with_id(mut v: Value, n: u32, lemma: &str, sentence: &str) -> Value {
    v["capture_id"] = json!(format!("0192812a-6f3c-7a41-9c2e-{n:012x}"));
    v["term"]["lemma"] = json!(lemma);
    v["term"]["surface"] = json!(lemma);
    v["context"]["sentence"] = json!(sentence);
    v["context"]["term_offset"] = Value::Null;
    v["media"] = json!({});
    v
}

#[test]
fn requests_need_a_working_key() {
    let fx = Fx::new();
    for (method, path) in [
        ("GET", "/subtitle/v1/me"),
        ("GET", "/subtitle/v1/decks"),
        ("GET", "/subtitle/v1/known-words?lang=en"),
        ("POST", "/subtitle/v1/captures"),
        ("POST", "/subtitle/v1/captures:batch"),
        ("POST", "/subtitle/v1/media"),
    ] {
        let r = send(
            fx.addr(),
            method,
            path,
            &[("Content-Type", "application/json")],
            b"{}",
        );
        assert_eq!(r.status, 401, "{method} {path}");
        assert_eq!(r.error(), "unauthorized");
        assert!(!r.body["error"]["message"].as_str().unwrap().is_empty());
    }
    let wrong = send(
        fx.addr(),
        "GET",
        "/subtitle/v1/me",
        &[("Authorization", "Bearer sk_fc_not-a-key")],
        b"",
    );
    assert_eq!(wrong.status, 401);
    // A revoked key stops at once.
    assert_eq!(fx.get("/subtitle/v1/me").status, 200);
    let key = fx.t.ok::<SubtitleKeysList>(Empty {}).keys[0].id;
    fx.t.ok::<SubtitleKeysRevoke>(SubtitleKeyRef { id: key });
    let r = fx.get("/subtitle/v1/me");
    assert_eq!(r.status, 401);
    assert_eq!(r.error(), "unauthorized");
}

#[test]
fn me_and_decks() {
    let fx = Fx::new();
    let r = fx.get("/subtitle/v1/me");
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.body["user"]["name"], "Chrome");
    assert!(r.body["user"]["id"].as_str().is_some());
    assert_eq!(
        r.body["features"],
        json!({"media": true, "batch": true, "known_words": true})
    );
    let decks = r.body["decks"].as_u64().unwrap();
    let r = fx.get("/subtitle/v1/decks");
    assert_eq!(r.status, 200);
    let list = r.body["decks"].as_array().unwrap();
    assert_eq!(list.len() as u64, decks);
    assert!(list
        .iter()
        .all(|d| d["id"].is_string() && d["name"].is_string() && d["cards"].is_u64()));
}

#[test]
fn the_contract_example_becomes_a_card_once() {
    let fx = Fx::new();
    // Media first, raw bytes; a retry of the same capture + kind gets the same id.
    let shot = fx.upload(ID, "screenshot", "image/jpeg", JPEG);
    assert_eq!(shot.status, 201, "{}", shot.body);
    let shot = shot.body["upload_id"].as_str().unwrap().to_owned();
    assert_eq!(
        fx.upload(ID, "screenshot", "image/jpeg", JPEG).body["upload_id"],
        json!(shot)
    );
    let audio = fx.upload(ID, "audio", "audio/wav", WAV);
    assert_eq!(audio.status, 201);
    let audio = audio.body["upload_id"].as_str().unwrap().to_owned();

    let created = fx.post_json("/subtitle/v1/captures", &example(&shot, &audio));
    assert_eq!(created.status, 201, "{}", created.body);
    assert_eq!(created.body["status"], "created");
    let card = created.body["card_id"].as_str().unwrap().to_owned();
    // The same capture again (the outbox retrying): 200 duplicate, the same card.
    let again = fx.post_json("/subtitle/v1/captures", &example(&shot, &audio));
    assert_eq!(again.status, 200);
    assert_eq!(again.body, json!({"status": "duplicate", "card_id": card}));
    // Another id, the same word from the same line: a duplicate too.
    let mut same = example(&shot, &audio);
    same["capture_id"] = json!("0192812a-6f3c-7a41-9c2e-000000000099");
    let r = fx.post_json("/subtitle/v1/captures", &same);
    assert_eq!(r.status, 200);
    assert_eq!(r.body["card_id"], json!(card));

    // The note: in the Subtitle deck, with the media and the term in bold.
    let found =
        fx.t.ok::<SearchCardsMethod>(fc_api::v1::search::SearchCards {
            query: "deck:Subtitle".into(),
            sort: None,
            descending: None,
            notes_only: None,
            offset: None,
            limit: None,
        });
    assert_eq!(found.total, 1);
    let note = fx.t.ok::<NotesGet>(NoteRef {
        id: found.items[0].note_id,
    });
    assert_eq!(note.cards[0].id.to_string(), card);
    assert!(note.fields["image"].starts_with("media:"));
    assert!(note.fields["audio"].starts_with("media:"));
    assert!(note.fields["sentence"].contains("<b>keys</b>"));

    // Undo in the app takes the card back; the extension's resend then makes it again.
    fx.t.ok::<OplogUndo>(Empty {});
    let r = fx.post_json("/subtitle/v1/captures", &example(&shot, &audio));
    assert_eq!(r.status, 201, "{}", r.body);
}

#[test]
fn invalid_captures_and_media_are_refused() {
    let fx = Fx::new();
    let mut bad = example("up_x", "up_y");
    bad["term"].as_object_mut().unwrap().remove("lemma");
    let r = fx.post_json("/subtitle/v1/captures", &bad);
    assert_eq!(r.status, 422);
    assert_eq!(
        r.body,
        json!({"error": {"code": "invalid_capture", "message": "term.lemma is required"}})
    );
    let r = send(
        fx.addr(),
        "POST",
        "/subtitle/v1/captures",
        &[
            ("Authorization", &fx.auth),
            ("Content-Type", "application/json"),
        ],
        b"{not json",
    );
    assert_eq!(r.status, 422);
    assert_eq!(r.error(), "invalid_capture");
    // Media: only JPEG screenshots and WAV clips, as declared, not too big.
    let r = fx.upload(ID, "screenshot", "image/png", JPEG);
    assert_eq!(r.status, 415);
    assert_eq!(r.error(), "invalid_media");
    assert_eq!(fx.upload(ID, "audio", "audio/wav", JPEG).status, 422);
    assert_eq!(fx.upload(ID, "video", "audio/wav", WAV).status, 422);
    assert_eq!(fx.upload("nope", "audio", "audio/wav", WAV).status, 422);
    // A body declared bigger than any clip is refused before it is read.
    let too_big = (fc_app::modules::subtitle::MAX_AUDIO_BYTES + 1).to_string();
    let r = send(
        fx.addr(),
        "POST",
        "/subtitle/v1/media",
        &[
            ("Authorization", &fx.auth),
            ("Content-Type", "audio/wav"),
            ("X-Capture-Id", ID),
            ("X-Media-Kind", "audio"),
            ("Content-Length", &too_big),
        ],
        b"",
    );
    assert_eq!(r.status, 413);
    let mut big = JPEG.to_vec();
    big.resize(fc_app::modules::subtitle::MAX_SCREENSHOT_BYTES + 1, 0);
    let r = fx.upload(ID, "screenshot", "image/jpeg", &big);
    assert_eq!(r.status, 413);
    assert_eq!(r.error(), "too_large");
    // Unknown paths and wrong methods.
    assert_eq!(fx.get("/subtitle/v1/nothing").status, 404);
    assert_eq!(fx.get("/subtitle/v1/captures").status, 405);
}

#[test]
fn batches_answer_each_capture() {
    let fx = Fx::new();
    let base = example("up_x", "up_y");
    let first = fx.post_json(
        "/subtitle/v1/captures",
        &with_id(base.clone(), 1, "key", "A line."),
    );
    assert_eq!(first.status, 201);
    let mut invalid = with_id(base.clone(), 3, "x", "A line.");
    invalid["kind"] = json!("idiom");
    let r = fx.post_json(
        "/subtitle/v1/captures:batch",
        &json!({"captures": [
            with_id(base.clone(), 2, "pick up", "A line."),
            with_id(base.clone(), 1, "key", "A line."),
            invalid,
        ]}),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let results = r.body["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0]["status"], "created");
    assert_eq!(results[1]["status"], "duplicate");
    assert_eq!(results[1]["card_id"], first.body["card_id"]);
    assert_eq!(results[2]["status"], "error");
    assert_eq!(
        results[2]["capture_id"],
        "0192812a-6f3c-7a41-9c2e-000000000003"
    );
    assert_eq!(results[2]["error"]["code"], "invalid_capture");
    assert_eq!(results[2]["error"]["retryable"], false);
    assert!(results[0].get("error").is_none());
    // Review fix: a full batch of long (valid) captures is larger than one request body may
    // be; it is still taken (a 413 would fail all of them for good).
    let long: Vec<Value> = (0..20)
        .map(|n| {
            let mut c = with_id(
                base.clone(),
                200 + n,
                &format!("word{n}"),
                &"ş".repeat(4_000),
            );
            c["context"]["translation"] = json!("ğ".repeat(4_000));
            c
        })
        .collect();
    let body = json!({ "captures": long });
    assert!(body.to_string().len() > fc_http::MAX_BODY_BYTES);
    let r = fx.post_json("/subtitle/v1/captures:batch", &body);
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.body["results"]
        .as_array()
        .unwrap()
        .iter()
        .all(|x| x["status"] == "created"));
    let many: Vec<Value> = (0..51)
        .map(|n| with_id(base.clone(), 100 + n, "w", "x"))
        .collect();
    assert_eq!(
        fx.post_json("/subtitle/v1/captures:batch", &json!({ "captures": many }))
            .status,
        422
    );
}

#[test]
fn known_words_come_from_the_reviews() {
    let fx = Fx::new();
    let base = example("up_x", "up_y");
    for (n, lemma) in [(1, "key"), (2, "pick up")] {
        let r = fx.post_json(
            "/subtitle/v1/captures",
            &with_id(base.clone(), n, lemma, "A line."),
        );
        assert_eq!(r.status, 201);
    }
    let r = fx.get("/subtitle/v1/known-words?lang=en");
    assert_eq!(r.status, 200, "{}", r.body);
    let words = r.body["words"].as_array().unwrap();
    assert_eq!(words.len(), 2);
    assert_eq!(words[0]["lemma"], "key");
    assert_eq!(words[0]["lang"], "en");
    assert_eq!(words[0]["status"], "learning");
    assert_eq!(words[0]["interval_days"], 0);
    assert!(words[0]["updated_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(r.body["next_cursor"], Value::Null);
    assert_eq!(
        fx.get("/subtitle/v1/known-words?lang=de").body["words"],
        json!([])
    );
    assert_eq!(fx.get("/subtitle/v1/known-words?cursor=%25%25").status, 422);
}

#[test]
fn origins_and_cors() {
    let fx = Fx::new();
    // A web page gets nothing, in the contract's shape.
    let r = send(
        fx.addr(),
        "GET",
        "/subtitle/v1/me",
        &[
            ("Authorization", &fx.auth),
            ("Origin", "https://evil.example"),
        ],
        b"",
    );
    assert_eq!(r.status, 403);
    assert_eq!(r.error(), "forbidden");
    let r = send(
        fx.addr(),
        "OPTIONS",
        "/subtitle/v1/captures",
        &[
            ("Origin", "https://evil.example"),
            ("Access-Control-Request-Method", "POST"),
        ],
        b"",
    );
    assert_eq!(r.status, 403);
    // Foreign Host headers (DNS rebinding) neither.
    let r = send(
        fx.addr(),
        "GET",
        "/subtitle/v1/me",
        &[("Authorization", &fx.auth), ("Host", "evil.example")],
        b"",
    );
    assert_eq!(r.status, 403);
    // The extension's origin: preflight with the contract's headers, answers with CORS.
    let r = send(
        fx.addr(),
        "OPTIONS",
        "/subtitle/v1/media",
        &[("Origin", EXT), ("Access-Control-Request-Method", "POST")],
        b"",
    );
    assert_eq!(r.status, 204);
    assert_eq!(r.header("access-control-allow-origin"), Some(EXT));
    let allowed = r.header("access-control-allow-headers").unwrap();
    for h in [
        "authorization",
        "content-type",
        "idempotency-key",
        "x-capture-id",
        "x-media-kind",
    ] {
        assert!(allowed.contains(h), "{h}");
    }
    let r = send(
        fx.addr(),
        "GET",
        "/subtitle/v1/me",
        &[("Authorization", &fx.auth), ("Origin", EXT)],
        b"",
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.header("access-control-allow-origin"), Some(EXT));
    // No X-Flashcard-Client needed here (the extension cannot send it); the key is.
    assert_eq!(fx.get("/subtitle/v1/me").status, 200);
}

#[test]
fn each_module_answers_only_its_own_routes() {
    let fx = Fx::new();
    // `subtitle` on, `local_api` off: the server runs, `/v1/*` says nothing (ping included).
    let r = send(fx.addr(), "GET", "/v1/ping", &[], b"");
    assert_eq!(r.status, 503);
    assert_eq!(r.body["code"], "CONFLICT_MODULE_DISABLED");
    let r = send(
        fx.addr(),
        "GET",
        "/v1/decks",
        &[("X-Flashcard-Client", "t"), ("Authorization", &fx.auth)],
        b"",
    );
    assert_eq!(r.status, 503);
    // And with `subtitle` off its routes are gone, key or not.
    fx.t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "local_api".into(),
        enabled: true,
    });
    fx.t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "subtitle".into(),
        enabled: false,
    });
    // The served app reads the switches from the shared file.
    std::thread::sleep(std::time::Duration::from_millis(10));
    let served = |path: &str| {
        // Another app over the same store, as the desktop sees it after a reload.
        let app = Arc::new(TestApp::new(fx.t.store.clone()).app);
        let server = Server::start(app, ServerConfig::localhost(0)).unwrap();
        let r = send(
            server.local_addr(),
            "GET",
            path,
            &[("Authorization", &fx.auth), ("X-Flashcard-Client", "t")],
            b"",
        );
        drop(server);
        r
    };
    let r = served("/subtitle/v1/me");
    assert_eq!(r.status, 404);
    assert_eq!(r.error(), "not_found");
    assert_eq!(served("/v1/ping").status, 200);
}
