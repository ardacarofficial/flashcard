//! v1.0 i18n lint, backend side (docs/04 v1.0: "every UI text comes from a translation key,
//! proven by a lint"; CLAUDE.md rule 8). The backend never returns user-facing text, only
//! keys (docs/02 §30); this proves every key it can send has a TR **and** EN text in the
//! desktop dictionaries (`apps/desktop/src/lib/i18n/locales/<lang>/*.json`):
//!
//! 1. **What the registry advertises.** Every public query that takes `{}` is run on a fresh
//!    app; every string under a `*_key` field of its response (`name_key`, `label_key`,
//!    `description_key`, `message_key`, …) must be a desktop key. This covers module names,
//!    menus and settings pages, settings labels and descriptions, shortcut/palette labels,
//!    automation actions, card type names, …
//! 2. **What the code emits.** Every dotted string literal in the backend's non-test sources
//!    (`fc-app`, `fc-study`, `fc-core`) that lives in a desktop namespace (`io.…`, `errors.…`,
//!    `plugins.…` …) and is not an identifier (a method name, a setting key, a menu id, a
//!    shortcut action, a job kind) is a message key — job progress, import issues, notices —
//!    and must exist too.
//! 3. Error codes: every `ErrorCode` has its text (`fc-api` `tests.rs`); the CLI prints the
//!    translated text of an error's `message_key` (`fc-cli` `i18n.rs`).

use crate::testing::TestApp;
use fc_api::v1::oplog::Actor;
use fc_api::MethodKind;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn leaves(v: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(map) => {
            for (k, v) in map {
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                leaves(v, &p, out);
            }
        }
        _ => {
            out.insert(prefix.to_owned());
        }
    }
}

/// Leaf keys of the desktop dictionary of `lang`.
fn desktop_keys(lang: &str) -> BTreeSet<String> {
    let dir = workspace()
        .join("apps/desktop/src/lib/i18n/locales")
        .join(lang);
    let mut keys = BTreeSet::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            leaves(&v, "", &mut keys);
        }
    }
    assert!(keys.len() > 100, "no dictionary at {}", dir.display());
    keys
}

/// Walks a response: strings under `*_key` fields are message keys, other strings are
/// identifiers (ids, actions, setting keys, routes…).
fn walk(v: &Value, field: &str, keys: &mut BTreeSet<String>, idents: &mut BTreeSet<String>) {
    match v {
        Value::Object(map) => {
            for (k, v) in map {
                walk(v, k, keys, idents);
            }
        }
        Value::Array(items) => {
            for v in items {
                walk(v, field, keys, idents);
            }
        }
        Value::String(s) => {
            // `template_key` is a card template id, not text.
            if field.ends_with("_key") && field != "template_key" {
                keys.insert(s.clone());
            } else {
                idents.insert(s.clone());
            }
        }
        _ => {}
    }
}

/// Keys the registry advertises and the identifiers next to them.
fn advertised() -> (BTreeSet<String>, BTreeSet<String>) {
    let t = TestApp::mem();
    let mut keys = BTreeSet::new();
    let mut idents: BTreeSet<String> = BTreeSet::new();
    let mut answered = 0;
    for m in fc_api::v1::catalog() {
        idents.insert(m.name.to_owned());
        if m.kind != MethodKind::Query {
            continue;
        }
        let wire = format!("{}.{}", fc_api::API_VERSION, m.name);
        if let Ok(v) = t.app.invoke(&wire, json!({}), &Actor::User) {
            answered += 1;
            walk(&v, "", &mut keys, &mut idents);
        }
    }
    assert!(answered > 30, "only {answered} queries answered `{{}}`");
    for m in t.app.registry().modules() {
        idents.insert(m.id.to_owned());
        for s in &m.settings {
            idents.insert(s.key.clone());
        }
        // UI contribution ids (`local_api.screen`) name menu entries, not texts.
        for u in &m.ui {
            idents.insert(u.id.clone());
        }
    }
    // Job kinds: stored identifiers (`jobs.kind`), never shown as text.
    idents.extend(t.app.registry().job_kinds().map(str::to_owned));
    (keys, idents)
}

/// End (exclusive, char index) of the item that starts at `from`: the `;` of an item without
/// a body (`mod tests;`, `use …;`), else the brace that closes its body. Strings, char
/// literals and comments do not count.
fn item_end(src: &[char], from: usize) -> usize {
    let mut depth = 0usize;
    let mut i = from;
    while i < src.len() {
        match src[i] {
            '/' if src.get(i + 1) == Some(&'/') => {
                while i < src.len() && src[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '/' if src.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < src.len() && !(src[i] == '*' && src[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
                continue;
            }
            '\'' if src.get(i + 2) == Some(&'\'') => {
                i += 3;
                continue;
            }
            '\'' if src.get(i + 1) == Some(&'\\') && src.get(i + 3) == Some(&'\'') => {
                i += 4;
                continue;
            }
            '"' => {
                i += 1;
                while i < src.len() && src[i] != '"' {
                    if src[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            ';' if depth == 0 => return i + 1,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    src.len()
}

/// The source without its `#[cfg(test)]` items (the test module's span; code after it is
/// still scanned).
fn without_test_items(src: &str) -> Vec<char> {
    const ATTR: &str = "#[cfg(test)]";
    let chars: Vec<char> = src.chars().collect();
    let attr: Vec<char> = ATTR.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i..].starts_with(&attr) {
            i = item_end(&chars, i + attr.len());
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// The string literals of a Rust source, without comments and `#[cfg(test)]` items.
fn literals(src: &str) -> Vec<String> {
    let bytes = without_test_items(src);
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            '/' if bytes.get(i + 1) == Some(&'/') => {
                while i < bytes.len() && bytes[i] != '\n' {
                    i += 1;
                }
            }
            '/' if bytes.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == '*' && bytes[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            // A char literal such as '"' must not open a string.
            '\'' if bytes.get(i + 2) == Some(&'\'') => i += 3,
            '\'' if bytes.get(i + 1) == Some(&'\\') && bytes.get(i + 3) == Some(&'\'') => i += 4,
            '"' => {
                let mut s = String::new();
                i += 1;
                while i < bytes.len() && bytes[i] != '"' {
                    if bytes[i] == '\\' {
                        i += 1;
                    }
                    if let Some(c) = bytes.get(i) {
                        s.push(*c);
                    }
                    i += 1;
                }
                out.push(s);
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn is_dotted_key(s: &str) -> bool {
    s.contains('.')
        && !s.starts_with('.')
        && !s.ends_with('.')
        && s.split('.').all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && !is_file_name(s)
}

/// File names the backend writes (`profiles.json`, `run.json`, `flashcard.log`…) look like
/// keys of a namespace (`profiles`) but are not texts.
fn is_file_name(s: &str) -> bool {
    const EXTENSIONS: &[&str] = &["json", "db", "log", "zip", "toml", "txt", "sql", "wasm"];
    s.matches('.').count() == 1
        && s.rsplit('.')
            .next()
            .is_some_and(|e| EXTENSIONS.contains(&e))
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if p.is_dir() {
            if name != "testing" && name != "tests" {
                rust_sources(&p, out);
            }
        } else if name.ends_with(".rs") && !name.contains("test") {
            out.push(p);
        }
    }
}

/// Message keys the backend's own code emits (rule 2 above), with the file they appear in.
fn emitted(namespaces: &BTreeSet<String>, idents: &BTreeSet<String>) -> BTreeMap<String, String> {
    let mut files = Vec::new();
    for krate in ["fc-app", "fc-study", "fc-core"] {
        rust_sources(
            &workspace().join("crates").join(krate).join("src"),
            &mut files,
        );
    }
    let mut found = BTreeMap::new();
    for f in files {
        let src = std::fs::read_to_string(&f).unwrap();
        // The plugin redaction table names permission domains, not texts.
        if f.ends_with("plugins/redact.rs") {
            continue;
        }
        for lit in literals(&src) {
            let ns = lit.split('.').next().unwrap_or_default();
            if is_dotted_key(&lit) && namespaces.contains(ns) && !idents.contains(&lit) {
                found.entry(lit).or_insert_with(|| f.display().to_string());
            }
        }
    }
    found
}

#[test]
fn literal_scanner_skips_comments_and_chars() {
    let src = "// \"no.thing\"\nlet a = '\"'; let b = \"io.progress.x\"; /* \"c.d\" */ \
               let e = \"esc\\\"aped\";\n#[cfg(test)] \"test.only\"";
    assert_eq!(literals(src), vec!["io.progress.x", "esc\"aped"]);
    assert!(is_dotted_key("io.progress.converting"));
    assert!(!is_dotted_key("Hello. World"));
    assert!(!is_dotted_key("v1"));
    assert!(!is_dotted_key("profiles.json"), "a file name, not a text");
    assert!(is_dotted_key("profiles.screen"));
}

/// Only the `#[cfg(test)]` item is skipped: code after a test module is still scanned.
#[test]
fn literal_scanner_skips_only_the_test_item() {
    let src = "let a = \"a.before\";\n#[cfg(test)]\nmod tests {\n    fn f() { let s = \"}\"; \
               let t = '{'; /* } */ \"test.only\"; }\n}\nlet b = \"b.after\";\n\
               #[cfg(test)]\nmod more;\nlet c = \"c.after\";";
    assert_eq!(literals(src), vec!["a.before", "b.after", "c.after"]);
}

#[test]
fn every_key_the_backend_sends_has_tr_and_en_text() {
    let en = desktop_keys("en");
    let tr = desktop_keys("tr");
    let (keys, idents) = advertised();
    assert!(keys.len() > 150, "only {} advertised keys", keys.len());
    let namespaces: BTreeSet<String> = en
        .iter()
        .map(|k| k.split('.').next().unwrap_or_default().to_owned())
        .collect();
    let emitted = emitted(&namespaces, &idents);
    assert!(
        emitted.contains_key("io.progress.converting"),
        "scanner found no progress keys"
    );
    let mut missing = Vec::new();
    for (key, from) in keys
        .iter()
        .map(|k| (k, "registry".to_owned()))
        .chain(emitted.iter().map(|(k, f)| (k, f.clone())))
    {
        // Plugins bring their own texts (`plugin.<id>.*`, docs/05 §6).
        if key.starts_with("plugin.") {
            continue;
        }
        for (lang, dict) in [("en", &en), ("tr", &tr)] {
            if !dict.contains(key) {
                missing.push(format!("{lang}: {key} ({from})"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "keys without text:\n{}",
        missing.join("\n")
    );
}
