//! v1.4 A6: the Subtitle surface when delivery is interrupted (docs/06 §5.1), over real TCP.
//! The same scenarios run end to end with the extension's own outbox in
//! `tools/subtitle-contract/interrupted.mjs` (including a killed and restarted server); here
//! the server side is pinned down in Rust:
//!
//! * an answer the client never reads (the line dropped after the server committed): the
//!   resend is a `duplicate` of the same card, the media stay stored once;
//! * a request cut before its body arrived commits nothing (and logs nothing);
//! * the outbox retrying while the first try is still being worked on (resends of one capture
//!   and of one upload racing each other): one card, one upload id, never a `5xx`;
//! * undo in the app, then the extension's resend: a new card, the old one stays gone;
//! * `subtitle.captures.recent` names every attempt that reached the capture command, with
//!   the card it ended in.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fc_api::v1::common::Empty;
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::NoteRef;
use fc_api::v1::search::SearchCards;
use fc_api::v1::subtitle::{
    CreateSubtitleKey, RecentSubtitleCaptures, SubtitleAttemptResult, SubtitleCaptureAttempt,
};
use fc_app::testing::{TempSqlite, TestApp};
use fc_core::CardId;
use fc_http::{Server, ServerConfig};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

const JPEG: &[u8] = b"\xff\xd8\xff\xe0\x00\x10JFIF\x00interrupted frame";
const WAV: &[u8] = b"RIFF\x24\x00\x00\x00WAVEfmt interrupted clip";

struct Fx {
    t: TestApp,
    server: Server,
    auth: String,
}

/// The raw request bytes of one exchange.
fn request(
    addr: SocketAddr,
    auth: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Vec<u8> {
    let mut req = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nAuthorization: {auth}\r\nContent-Length: {}\r\n",
        addr.port(),
        body.len()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    let mut bytes = req.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

/// Status and JSON body of a full exchange.
fn exchange(addr: SocketAddr, bytes: &[u8]) -> (u16, Value) {
    let mut s = TcpStream::connect(addr).unwrap();
    s.write_all(bytes).unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    let chunked = head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked");
    let body = if chunked {
        // One chunk is all these small answers use.
        body.split_once("\r\n")
            .map_or("", |(_, rest)| rest)
            .rsplit_once("\r\n0")
            .map_or("", |(b, _)| b)
            .trim_end_matches("\r\n")
            .to_owned()
    } else {
        body.to_owned()
    };
    (status, serde_json::from_str(&body).unwrap_or(Value::Null))
}

impl Fx {
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

    fn capture_bytes(&self, capture: &Value) -> Vec<u8> {
        request(
            self.addr(),
            &self.auth,
            "/subtitle/v1/captures",
            &[("Content-Type", "application/json")],
            capture.to_string().as_bytes(),
        )
    }

    fn capture(&self, capture: &Value) -> (u16, Value) {
        exchange(self.addr(), &self.capture_bytes(capture))
    }

    fn upload_bytes(&self, capture_id: &str, kind: &str, mime: &str, bytes: &[u8]) -> Vec<u8> {
        request(
            self.addr(),
            &self.auth,
            "/subtitle/v1/media",
            &[
                ("Content-Type", mime),
                ("X-Capture-Id", capture_id),
                ("X-Media-Kind", kind),
            ],
            bytes,
        )
    }

    fn upload(&self, capture_id: &str, kind: &str, mime: &str, bytes: &[u8]) -> String {
        let (status, body) = exchange(
            self.addr(),
            &self.upload_bytes(capture_id, kind, mime, bytes),
        );
        assert_eq!(status, 201, "{body}");
        body["upload_id"].as_str().unwrap().to_owned()
    }

    /// Attempts of one capture, oldest first.
    fn attempts(&self, capture_id: &str) -> Vec<SubtitleCaptureAttempt> {
        let mut rows: Vec<_> = self
            .t
            .ok::<SubtitleCapturesRecent>(RecentSubtitleCaptures { limit: Some(500) })
            .attempts
            .into_iter()
            .filter(|a| a.capture_id == capture_id)
            .collect();
        rows.reverse();
        rows
    }

    fn search(&self) -> fc_api::v1::search::CardSearchPage {
        self.t.ok::<SearchCardsMethod>(SearchCards {
            query: "deck:Subtitle".into(),
            sort: None,
            descending: None,
            notes_only: None,
            offset: None,
            limit: None,
            cursor: None,
        })
    }

    fn cards(&self) -> u32 {
        self.search().total
    }

    /// The note of a card in the Subtitle deck.
    fn note_of(&self, card: CardId) -> fc_api::v1::notes::NoteDto {
        let row = self.search().items.into_iter().find(|r| r.card_id == card);
        let id = row.expect("the card is in deck:Subtitle").note_id;
        self.t.ok::<NotesGet>(NoteRef { id })
    }

    fn media(&self) -> usize {
        self.t.ok::<MediaList>(Empty {}).items.len()
    }

    /// Waits until `capture_id` has `n` logged attempts (the server works on a request the
    /// client has already given up on).
    fn wait_for_attempts(&self, capture_id: &str, n: usize) -> Vec<SubtitleCaptureAttempt> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let rows = self.attempts(capture_id);
            if rows.len() >= n || Instant::now() > deadline {
                return rows;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn id(n: u32) -> String {
    format!("0192812a-6f3c-7a41-9c2e-{n:012x}")
}

fn capture(n: u32, lemma: &str, sentence: &str, media: Value) -> Value {
    json!({
      "schema": 1,
      "capture_id": id(n),
      "kind": "word",
      "lang": { "target": "en", "native": "tr" },
      "term": { "surface": lemma, "lemma": lemma, "pos": "noun" },
      "definition": { "text": "anlam", "source": "dict" },
      "context": { "sentence": sentence, "translation": "çeviri", "term_offset": null },
      "source": { "type": "video", "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ", "title": "T", "site": "youtube",
                  "video_id": "dQw4w9WgXcQ", "t_start_ms": 1000, "t_end_ms": 2000 },
      "media": media,
      "deck_id": null,
      "tags": ["subtitle"],
      "created_at": "2026-11-01T10:00:00.000Z",
      "client": { "name": "subtitle-extension", "ext_version": "3.1.3" }
    })
}

fn results(rows: &[SubtitleCaptureAttempt]) -> Vec<SubtitleAttemptResult> {
    rows.iter().map(|r| r.result).collect()
}

#[test]
fn an_answer_never_read_makes_the_resend_a_duplicate_of_the_same_card() {
    let fx = Fx::new();
    let cid = id(1);
    let shot = fx.upload(&cid, "screenshot", "image/jpeg", JPEG);
    let audio = fx.upload(&cid, "audio", "audio/wav", WAV);
    let c = capture(
        1,
        "lantern",
        "He lifted the lantern slowly.",
        json!({
            "screenshot": { "upload_id": shot, "mime": "image/jpeg" },
            "audio": { "upload_id": audio, "mime": "audio/wav" }
        }),
    );
    // The request goes out, then the line drops: the client never reads the answer.
    {
        let mut s = TcpStream::connect(fx.addr()).unwrap();
        s.write_all(&fx.capture_bytes(&c)).unwrap();
        let rows = fx.wait_for_attempts(&cid, 1);
        assert_eq!(results(&rows), [SubtitleAttemptResult::Created], "{rows:?}");
        // Dropped unread.
    }
    let card = fx.attempts(&cid)[0].card_id.unwrap();
    // The outbox retries: media again (the same ids), then the capture: a duplicate.
    assert_eq!(fx.upload(&cid, "screenshot", "image/jpeg", JPEG), shot);
    assert_eq!(fx.upload(&cid, "audio", "audio/wav", WAV), audio);
    let (status, body) = fx.capture(&c);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        json!({ "status": "duplicate", "card_id": card.to_string() })
    );
    assert_eq!(fx.cards(), 1);
    assert_eq!(fx.media(), 2, "each picture and sound stored once");
    let rows = fx.attempts(&cid);
    assert_eq!(
        results(&rows),
        [
            SubtitleAttemptResult::Created,
            SubtitleAttemptResult::Duplicate
        ]
    );
    assert!(rows.iter().all(|r| r.card_id == Some(card)));
    let note = fx.note_of(card);
    assert!(note.fields["image"].starts_with("media:"));
    assert!(note.fields["audio"].starts_with("media:"));
}

#[test]
fn a_request_cut_before_its_body_arrived_commits_nothing() {
    let fx = Fx::new();
    let c = capture(
        2,
        "harbour",
        "The ship left the harbour at dawn.",
        json!({}),
    );
    let bytes = fx.capture_bytes(&c);
    {
        let mut s = TcpStream::connect(fx.addr()).unwrap();
        s.write_all(&bytes[..bytes.len() - 40]).unwrap();
        s.shutdown(std::net::Shutdown::Both).unwrap();
    }
    // A media upload cut the same way.
    let up = fx.upload_bytes(&id(2), "audio", "audio/wav", WAV);
    {
        let mut s = TcpStream::connect(fx.addr()).unwrap();
        s.write_all(&up[..up.len() - 8]).unwrap();
        s.shutdown(std::net::Shutdown::Both).unwrap();
    }
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(fx.cards(), 0);
    assert_eq!(fx.media(), 0);
    assert!(
        fx.attempts(&id(2)).is_empty(),
        "nothing reached the capture command"
    );
    // The retry is the first capture the app sees.
    let (status, body) = fx.capture(&c);
    assert_eq!(status, 201, "{body}");
    assert_eq!(fx.cards(), 1);
    assert_eq!(
        results(&fx.attempts(&id(2))),
        [SubtitleAttemptResult::Created]
    );
}

#[test]
fn resends_racing_each_other_make_one_card_and_one_upload() {
    let fx = Fx::new();
    let cid = id(3);
    const N: usize = 8;
    // The same upload N times at once: one upload id, one media file.
    let barrier = Arc::new(Barrier::new(N));
    let addr = fx.addr();
    let up = fx.upload_bytes(&cid, "audio", "audio/wav", WAV);
    let ids: Vec<(u16, Value)> = (0..N)
        .map(|_| {
            let (b, up) = (barrier.clone(), up.clone());
            std::thread::spawn(move || {
                b.wait();
                exchange(addr, &up)
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect();
    assert!(ids.iter().all(|(s, _)| *s == 201), "{ids:?}");
    let upload = ids[0].1["upload_id"].as_str().unwrap().to_owned();
    assert!(
        ids.iter().all(|(_, b)| b["upload_id"] == json!(upload)),
        "{ids:?}"
    );
    assert_eq!(fx.media(), 1);

    // The same capture N times at once.
    let c = capture(
        3,
        "meadow",
        "Cows grazed in the meadow.",
        json!({
            "audio": { "upload_id": upload, "mime": "audio/wav" }
        }),
    );
    let bytes = fx.capture_bytes(&c);
    let barrier = Arc::new(Barrier::new(N));
    let answers: Vec<(u16, Value)> = (0..N)
        .map(|_| {
            let (b, bytes) = (barrier.clone(), bytes.clone());
            std::thread::spawn(move || {
                b.wait();
                exchange(addr, &bytes)
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect();
    let created: Vec<_> = answers.iter().filter(|(s, _)| *s == 201).collect();
    assert_eq!(created.len(), 1, "{answers:?}");
    let card = created[0].1["card_id"].clone();
    assert!(
        answers
            .iter()
            .all(|(s, b)| (*s == 201 || *s == 200) && b["card_id"] == card),
        "{answers:?}"
    );
    assert_eq!(fx.cards(), 1);
    assert_eq!(fx.media(), 1);
    let rows = fx.attempts(&cid);
    assert_eq!(rows.len(), N);
    assert_eq!(
        rows.iter()
            .filter(|r| r.result == SubtitleAttemptResult::Created)
            .count(),
        1
    );
    assert!(rows
        .iter()
        .all(|r| r.card_id.map(|c| c.to_string()) == card.as_str().map(str::to_owned)));
}

#[test]
fn undo_then_the_resend_is_a_new_card_and_the_log_shows_both() {
    let fx = Fx::new();
    let cid = id(4);
    let shot = fx.upload(&cid, "screenshot", "image/jpeg", JPEG);
    let c = capture(
        4,
        "orchard",
        "They walked through the orchard.",
        json!({
            "screenshot": { "upload_id": shot, "mime": "image/jpeg" }
        }),
    );
    let (status, first) = fx.capture(&c);
    assert_eq!(status, 201);
    fx.t.ok::<OplogUndo>(Empty {});
    assert_eq!(fx.cards(), 0, "undo took the card back");
    // The extension resends (its answer was lost, or the user sends it again): documented
    // result — undo forgot the capture, so it is a new capture and a new card.
    let (status, second) = fx.capture(&c);
    assert_eq!(status, 201, "{second}");
    assert_eq!(second["status"], "created");
    assert_ne!(second["card_id"], first["card_id"]);
    assert_eq!(fx.cards(), 1);
    let card: CardId = second["card_id"].as_str().unwrap().parse().unwrap();
    let note = fx.note_of(card);
    assert!(
        note.fields["image"].starts_with("media:"),
        "the upload still serves the new card"
    );
    let rows = fx.attempts(&cid);
    assert_eq!(
        rows.iter()
            .map(|r| (r.result, r.card_id.map(|c| c.to_string())))
            .collect::<Vec<_>>(),
        [
            (
                SubtitleAttemptResult::Created,
                first["card_id"].as_str().map(str::to_owned)
            ),
            (
                SubtitleAttemptResult::Created,
                second["card_id"].as_str().map(str::to_owned)
            ),
        ]
    );
    // And a further resend is a duplicate of the new card.
    let (status, third) = fx.capture(&c);
    assert_eq!(status, 200);
    assert_eq!(third["card_id"], second["card_id"]);
}
