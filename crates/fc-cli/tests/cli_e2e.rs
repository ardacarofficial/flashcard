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
