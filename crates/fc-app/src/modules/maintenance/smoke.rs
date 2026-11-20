//! Self test of an installed build (`Flashcard.exe --smoke`, run by CI after a silent install,
//! docs/07 §8): in a **fresh, temporary** collection — never the user's — it walks the first
//! run of a real user: open (create + migrate), seed, create a deck and a note, study the card,
//! undo and redo the answer, check the database, keep and drop a draft, write a log line that
//! carries a secret, export the logs and prove the secret is not in the bundle. Every step
//! goes through the public API, like the UI.

use super::export::write_bundle;
use super::logging::{FileLog, Level};
use crate::App;
use fc_api::v1::oplog::Actor;
use fc_core::notetype::builtin::CLASSIC_ID;
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

/// A secret-shaped value the smoke run logs and then looks for in the export.
pub const PLANTED_SECRET: &str = "sk-ant-api03-SMOKEsmoke0123456789SMOKEsmoke";

fn call(app: &App, method: &str, params: Value) -> Result<Value, String> {
    app.invoke(&format!("v1.{method}"), params, &Actor::User)
        .map_err(|e| format!("{method}: {} {}", e.code, e.message))
}

fn field<'a>(v: &'a Value, path: &[&str]) -> Result<&'a Value, String> {
    let mut cur = v;
    for p in path {
        cur = cur.get(*p).ok_or_else(|| format!("missing `{}` in {v}", path.join(".")))?;
    }
    Ok(cur)
}

fn ensure(ok: bool, what: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(what.to_owned())
    }
}

/// Runs the smoke test in `dir` (must be empty or new). Returns the steps that passed.
pub fn run(dir: &Path) -> Result<Vec<String>, String> {
    let mut steps = Vec::new();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let app = App::open_sqlite(&dir.join("collection.db")).map_err(|e| {
        let e = e.into_api();
        format!("open: {} {}", e.code, e.message)
    })?;
    steps.push(format!("collection created and migrated ({})", super::APP_VERSION));

    let deck = call(&app, "decks.create", json!({"path": "Smoke"}))?;
    let deck_id = field(&deck, &["id"])?.clone();
    call(
        &app,
        "notes.create",
        json!({"notetype_id": CLASSIC_ID, "deck_id": deck_id, "fields": {"front": "kılıç", "back": "sword"}, "tags": [], "attributes": []}),
    )?;
    steps.push("deck and note created".into());

    let session = call(&app, "study.start", json!({"kind": "normal", "deck_id": deck_id}))?;
    let session_id = field(&session, &["id"])?.clone();
    let card_id = field(&session, &["current", "card_id"])?.clone();
    let answered = call(
        &app,
        "study.answer",
        json!({"session_id": session_id, "card_id": card_id, "rating": "good", "duration_ms": 1200}),
    )?;
    ensure(field(&answered, &["answered"])? == &json!(1), "the answer was not counted")?;
    call(&app, "oplog.undo", json!({}))?;
    let back = call(&app, "study.session", json!({"session_id": session_id}))?;
    ensure(
        field(&back, &["current", "card_id"])? == &card_id,
        "undo did not bring the card back",
    )?;
    call(&app, "oplog.redo", json!({}))?;
    call(&app, "study.end", json!({"session_id": session_id}))?;
    steps.push("studied, undone and redone".into());

    let integrity = call(&app, "maintenance.integrity.check", json!({"quick": false}))?;
    ensure(field(&integrity, &["ok"])? == &json!(true), "integrity check failed")?;
    steps.push("integrity ok".into());

    call(&app, "maintenance.drafts.save", json!({"key": "notes.add", "payload": {"front": "x"}}))?;
    let got = call(&app, "maintenance.drafts.get", json!({"key": "notes.add"}))?;
    ensure(field(&got, &["draft", "payload", "front"])? == &json!("x"), "draft not kept")?;
    call(&app, "maintenance.drafts.discard", json!({"key": "notes.add"}))?;
    steps.push("draft kept and discarded".into());

    let log = FileLog::open(&dir.join(super::LOG_DIR), Arc::new(crate::system::SystemClock))
        .map_err(|e| format!("log: {e}"))?;
    log.write_line(Level::Warn, "fc_app::smoke", &format!("planted {PLANTED_SECRET}"));
    log.flush();
    let mut files = vec![("system.txt".to_owned(), format!("smoke {PLANTED_SECRET}\n"))];
    for (name, _) in log.files() {
        let text = std::fs::read_to_string(log.dir().join(&name)).map_err(|e| e.to_string())?;
        files.push((format!("logs/{name}"), text));
    }
    let bundle = dir.join("logs.zip");
    write_bundle(&bundle, &files, &log.redactor()).map_err(|e| e.into_api().message)?;
    let leaked = zip_contains(&bundle, PLANTED_SECRET)?;
    ensure(!leaked, "a secret reached the exported logs")?;
    steps.push("logs exported without the secret".into());
    Ok(steps)
}

/// Whether any entry of the zip at `path` contains `needle`.
pub fn zip_contains(path: &Path, needle: &str) -> Result<bool, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let mut text = String::new();
        entry.read_to_string(&mut text).map_err(|e| e.to_string())?;
        if text.contains(needle) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_run_passes_in_a_fresh_folder() {
        let dir = tempfile::tempdir().unwrap();
        let steps = super::run(&dir.path().join("smoke")).unwrap();
        assert_eq!(steps.len(), 6, "{steps:?}");
    }
}
