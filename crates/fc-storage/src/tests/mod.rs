use crate::migrate::{self, MIGRATIONS};
use crate::store::{ReadTx, SqliteStore, Store, WriteTx};
use crate::StorageError;
use rusqlite::Connection;

/// Every table of docs/03 §4 (+ note_tags, module_state).
const TABLES: &[&str] = &[
    "presets",
    "decks",
    "notetypes",
    "notes",
    "cards",
    "review_log",
    "oplog",
    "tags",
    "note_tags",
    "attribute_defs",
    "attribute_values",
    "media",
    "links",
    "saved_queries",
    "sessions",
    "settings",
    "module_state",
    "jobs",
    "inbox",
    "automations",
    "plugins",
    "plugin_kv",
    "goals",
    "ai_usage",
    "notes_fts",
];

fn fresh() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    migrate::configure_connection(&conn).unwrap();
    migrate::migrate(&mut conn).unwrap();
    conn
}

#[test]
fn migrations_are_ordered_and_named() {
    let mut prev = 0;
    for m in MIGRATIONS {
        assert!(m.version > prev, "versions must increase");
        assert!(
            m.name.starts_with(&format!("{:04}_", m.version)),
            "{}",
            m.name
        );
        prev = m.version;
    }
    // v0.1 owns 1..=99.
    assert!(MIGRATIONS.iter().all(|m| (1..=99).contains(&m.version)));
}

#[test]
fn migrates_empty_database_and_is_idempotent() {
    let mut conn = Connection::open_in_memory().unwrap();
    migrate::configure_connection(&conn).unwrap();
    assert_eq!(migrate::current_version(&conn).unwrap(), 0);
    assert_eq!(migrate::migrate(&mut conn).unwrap(), vec![1]);
    assert_eq!(
        migrate::current_version(&conn).unwrap(),
        migrate::latest_version()
    );
    assert!(migrate::migrate(&mut conn).unwrap().is_empty());

    for t in TABLES {
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = ?1",
                [t],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "table {t} missing");
    }
    let fk: i64 = conn
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .unwrap();
    assert_eq!(fk, 1);
}

#[test]
fn refuses_newer_schema() {
    let mut conn = fresh();
    conn.pragma_update(None, "user_version", 9999).unwrap();
    assert!(matches!(
        migrate::migrate(&mut conn),
        Err(StorageError::TooNew { found: 9999, .. })
    ));
}

#[test]
fn schema_enforces_core_constraints() {
    let conn = fresh();
    let deck = |id: &str, parent: Option<&str>, name: &str| {
        conn.execute(
            "INSERT INTO decks (id, parent_id, name, created_at, updated_at) VALUES (?1, ?2, ?3, 0, 0)",
            rusqlite::params![id, parent, name],
        )
    };
    deck("d1", None, "English").unwrap();
    deck("d2", Some("d1"), "Verbs").unwrap();
    assert!(
        deck("d3", None, "english").is_err(),
        "sibling names unique (NOCASE)"
    );
    assert!(deck("d4", Some("d1"), "a::b").is_err(), "no :: in names");
    assert!(deck("d5", Some("missing"), "X").is_err(), "parent FK");
    conn.execute("UPDATE decks SET deleted_at = 1 WHERE id = 'd2'", [])
        .unwrap();
    deck("d6", Some("d1"), "Verbs").unwrap();

    conn.execute(
        "INSERT INTO attribute_defs (id, key, name, kind, created_at, updated_at) VALUES ('a1', 'level', 'Level', 'ordered_enum', 0, 0)",
        [],
    )
    .unwrap();
    assert!(conn
        .execute(
            "INSERT INTO attribute_defs (id, key, name, kind, created_at, updated_at) VALUES ('a2', 'level', 'Level', 'enum', 0, 0)",
            [],
        )
        .is_err(), "key unique per scope");
    assert!(conn
        .execute(
            "INSERT INTO attribute_defs (id, key, name, kind, created_at, updated_at) VALUES ('a3', 'x', 'X', 'weird', 0, 0)",
            [],
        )
        .is_err(), "kind checked");
    // multi_enum: several rows per (attr, entity).
    for v in ["o1", "o2"] {
        conn.execute(
            "INSERT INTO attribute_values (attr_id, entity_kind, entity_id, value) VALUES ('a1', 'note', 'n1', ?1)",
            [v],
        )
        .unwrap();
    }
    assert!(conn
        .execute("INSERT INTO settings (scope, key, value, updated_at) VALUES ('app', 'k', 'not json', 0)", [])
        .is_err());
}

#[test]
fn fts5_is_available() {
    let conn = fresh();
    conn.execute(
        "INSERT INTO notes_fts (note_id, sort_field, content) VALUES ('n1', 'koşmak', 'koşmak\nto run')",
        [],
    )
    .unwrap();
    let hits: i64 = conn
        .query_row(
            "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'kosmak'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1, "diacritics removed");
}

#[test]
fn store_opens_and_reports_version() {
    let store = SqliteStore::open_in_memory().unwrap();
    assert_eq!(store.schema_version().unwrap(), migrate::latest_version());
    assert!(store.path().is_none());

    let dir = std::env::temp_dir().join(format!("fc-storage-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("collection.db");
    {
        let s = SqliteStore::open(&file).unwrap();
        assert_eq!(s.schema_version().unwrap(), 1);
    }
    let reopened = SqliteStore::open(&file).unwrap();
    assert_eq!(reopened.schema_version().unwrap(), 1);
    drop(reopened);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The interface must stay object-safe: the app holds `Arc<dyn Store>`.
#[allow(dead_code)]
fn object_safety(store: &dyn Store, r: &dyn ReadTx, w: Box<dyn WriteTx + '_>) {
    let _: &dyn crate::Reads = r;
    let _: &dyn crate::Reads = &*w;
    let _ = store;
}
