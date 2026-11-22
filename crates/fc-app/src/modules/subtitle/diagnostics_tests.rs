//! v1.4 A5: capture diagnostics — results, reasons, the bound, the privacy property.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::{authenticate, batch, capture, key_hash, upload, ID};
use super::*;
use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_storage::Store;
use serde_json::json;
use std::sync::Arc;

const SENTENCE: &str = "Almost. I just need to pick up my zyxwvkeys.";
const TITLE: &str = "Secret Film Title Qwerty";
const URL: &str = "https://example.test/watch?v=privateVideo123";
const JPEG: &[u8] = b"\xff\xd8\xff\xe0\x00\x10JFIF\x00jpeg-bytes";

fn id(n: u32) -> String {
    format!("0192812a-6f3c-7a41-9c2e-{n:012x}")
}

fn example(n: u32, lemma: &str) -> Value {
    json!({
      "schema": 1,
      "capture_id": id(n),
      "kind": "word",
      "lang": { "target": "en", "native": "tr" },
      "term": { "surface": lemma, "lemma": lemma, "pos": "noun" },
      "definition": { "text": "gizli-anlam", "source": "dict" },
      "context": { "sentence": SENTENCE, "translation": "gizli çeviri", "term_offset": null },
      "source": { "type": "video", "url": URL, "title": TITLE, "site": "youtube",
                  "video_id": "privateVideo123", "t_start_ms": 1000, "t_end_ms": 2000 },
      "media": {},
      "deck_id": null,
      "tags": [],
      "created_at": "2026-10-12T10:00:00.000Z",
      "client": { "name": "subtitle-extension", "ext_version": "3.1.3" }
    })
}

fn setup(store: Arc<dyn Store>) -> (TestApp, SubtitleKeyDto) {
    let t = TestApp::new(store);
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: ID.into(),
        enabled: true,
    });
    let c = t.ok::<SubtitleKeysCreate>(CreateSubtitleKey { name: "Chrome".into() });
    let key = authenticate(&t.app, &key_hash(&c.secret)).unwrap();
    (t, key)
}

fn recent(t: &TestApp, limit: Option<u32>) -> Vec<SubtitleCaptureAttempt> {
    t.ok::<SubtitleCapturesRecent>(RecentSubtitleCaptures { limit })
        .attempts
}

fn every_result_is_recorded(store: Arc<dyn Store>) {
    let (t, key) = setup(store);
    assert!(recent(&t, None).is_empty());
    // created
    let o = capture(&t.app, &key, &example(1, "zyxwvkeys")).unwrap();
    // duplicate (same id)
    capture(&t.app, &key, &example(1, "zyxwvkeys")).unwrap();
    // rejected: no lemma
    let mut bad = example(2, "zyxwvkeys");
    bad["term"]["lemma"] = json!("");
    capture(&t.app, &key, &bad).unwrap_err();
    // rejected: not even a capture id
    capture(&t.app, &key, &json!({"schema": 1, "capture_id": "Secret Film Title Qwerty"}))
        .unwrap_err();
    // media missing: one unknown upload, one belonging to another capture
    let shot = upload(&t.app, &key, &id(1), "screenshot", "image/jpeg", JPEG)
        .unwrap()
        .upload_id;
    let mut m = example(3, "pick up");
    m["media"] = json!({"screenshot": {"upload_id": shot, "mime": "image/jpeg"},
                        "audio": {"upload_id": "up_nope", "mime": "audio/wav"}});
    let o3 = capture(&t.app, &key, &m).unwrap();
    // a batch item counts too
    batch(&t.app, &key, &json!({"captures": [example(4, "carry on")]})).unwrap();

    let got = recent(&t, None);
    let summary: Vec<(SubtitleAttemptResult, Option<&str>)> = got
        .iter()
        .map(|a| (a.result, a.reason.as_deref()))
        .collect();
    assert_eq!(
        summary,
        vec![
            (SubtitleAttemptResult::Created, None),
            (
                SubtitleAttemptResult::MediaMissing,
                Some("screenshot:upload_mismatch,audio:upload_unknown")
            ),
            (SubtitleAttemptResult::Rejected, Some("invalid_capture:capture_id")),
            (SubtitleAttemptResult::Rejected, Some("invalid_capture:term.lemma")),
            (SubtitleAttemptResult::Duplicate, None),
            (SubtitleAttemptResult::Created, None),
        ]
    );
    assert_eq!(got[1].card_id, Some(o3.card_id));
    assert_eq!(got[4].card_id, Some(o.card_id));
    assert_eq!(got[5].card_id, Some(o.card_id));
    assert_eq!(got[5].capture_id, id(1));
    // A malformed id is not kept (it could be anything the client sent).
    assert_eq!(got[2].capture_id, "");
    assert_eq!(got[3].capture_id, id(2));
    assert!(got.iter().all(|a| a.at == t.app.now() || a.at.0 > 0));
    assert_eq!(recent(&t, Some(2)).len(), 2);
    // Diagnostics are not undo steps: undo still takes back the last capture.
    assert!(t
        .ok::<OplogList>(fc_api::v1::oplog::ListOplog::default())
        .items
        .iter()
        .all(|e| !e.command.contains("captures.log")));

    // Privacy: no sentence, word, meaning, URL or title anywhere in what is kept.
    let text = export_text(&*t.store.read().unwrap()).unwrap().unwrap();
    let dump = format!("{got:?}\n{text}");
    for secret in [
        "zyxwvkeys",
        "pick up",
        "carry on",
        "gizli",
        "Almost",
        "Secret",
        "Qwerty",
        "example.test",
        "privateVideo123",
        "youtube",
    ] {
        assert!(!dump.contains(secret), "{secret} leaked: {dump}");
    }
    assert_eq!(text.lines().count(), 1 + 6);
}

fn only_the_newest_are_kept(store: Arc<dyn Store>) {
    let (t, key) = setup(store);
    let mut bad = example(0, "x");
    bad["schema"] = json!(2);
    for _ in 0..(KEEP + 7) {
        capture(&t.app, &key, &bad).unwrap_err();
    }
    capture(&t.app, &key, &example(1, "last")).unwrap();
    let all = recent(&t, Some(10_000));
    assert_eq!(all.len(), KEEP as usize);
    assert_eq!(all[0].result, SubtitleAttemptResult::Created);
    assert_eq!(
        t.store.read().unwrap().recent_capture_log(10_000).unwrap().len(),
        KEEP as usize
    );
    assert_eq!(recent(&t, None).len(), DEFAULT_RECENT as usize);
}

fn nothing_is_recorded_while_the_module_is_off(store: Arc<dyn Store>) {
    let (t, key) = setup(store);
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: ID.into(),
        enabled: false,
    });
    capture(&t.app, &key, &example(1, "word")).unwrap_err();
    assert!(t.store.read().unwrap().recent_capture_log(10).unwrap().is_empty());
}

macro_rules! suite {
    ($($name:ident),* $(,)?) => {
        mod mem {
            $( #[test] fn $name() { super::$name(std::sync::Arc::new(super::MemStore::new())); } )*
        }
        mod sqlite_file {
            $( #[test] fn $name() { super::$name(std::sync::Arc::new(super::TempSqlite::new())); } )*
        }
    };
}

suite!(
    every_result_is_recorded,
    only_the_newest_are_kept,
    nothing_is_recorded_while_the_module_is_off,
);
