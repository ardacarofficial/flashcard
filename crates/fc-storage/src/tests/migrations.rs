use super::Fx;
use crate::migrate::{self, Migration, MIGRATIONS};
use crate::store::{SqliteStore, Store};
use crate::StorageError;
use rusqlite::Connection;

/// Every table of docs/03 §4 (+ note_tags, module_state, schema_migrations).
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
    "schema_migrations",
];

/// Indices the repositories rely on (docs/03 §17).
const INDICES: &[&str] = &[
    "decks_parent",
    "decks_sibling_name",
    "notetypes_builtin",
    "notes_checksum",
    "notes_updated",
    "notes_fts_rowid",
    "cards_deck_due",
    "cards_note",
    "cards_note_template",
    "review_log_card_time",
    "oplog_batch",
    "tags_name",
    "note_tags_tag",
    "attribute_defs_scope_key",
    "attribute_values_entity",
    "attribute_values_value",
    "jobs_status",
];

fn fresh() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    migrate::configure_connection(&conn).unwrap();
    migrate::migrate(&mut conn).unwrap();
    conn
}

fn all_versions() -> Vec<u32> {
    MIGRATIONS.iter().map(|m| m.version).collect()
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
    assert_eq!(migrate::migrate(&mut conn).unwrap(), all_versions());
    assert_eq!(
        migrate::current_version(&conn).unwrap(),
        migrate::latest_version()
    );
    assert!(migrate::migrate(&mut conn).unwrap().is_empty());
    assert!(migrate::pending_versions(&mut conn).unwrap().is_empty());

    for t in TABLES.iter().chain(INDICES) {
        assert!(
            migrate::schema_object_exists(&conn, t).unwrap(),
            "{t} missing"
        );
    }
    let fk: i64 = conn
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .unwrap();
    assert_eq!(fk, 1);
    let fts_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'notes_fts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(fts_sql.to_lowercase().contains("fts5"));
}

#[test]
fn records_applied_migrations_with_checksums() {
    let conn = fresh();
    let rows: Vec<(u32, String, String, i64)> = conn
        .prepare(
            "SELECT version, name, checksum, applied_at FROM schema_migrations ORDER BY version",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows.len(), MIGRATIONS.len());
    for (row, m) in rows.iter().zip(MIGRATIONS) {
        assert_eq!(row.0, m.version);
        assert_eq!(row.1, m.name);
        assert_eq!(row.2, m.checksum());
        assert!(row.3 > 1_600_000_000_000, "applied_at is a ms timestamp");
    }
}

#[test]
fn checksum_ignores_carriage_returns() {
    assert_eq!(
        migrate::checksum("A;\nB;\n"),
        migrate::checksum("A;\r\nB;\r\n")
    );
    assert_ne!(migrate::checksum("A;"), migrate::checksum("B;"));
    assert_eq!(migrate::checksum("x").len(), 16);
}

#[test]
fn edited_migration_is_refused() {
    let mut conn = Connection::open_in_memory().unwrap();
    let v1 = [Migration {
        version: 1,
        name: "0001_t",
        sql: "CREATE TABLE t (x INTEGER);",
    }];
    assert_eq!(migrate::migrate_with(&mut conn, &v1).unwrap(), vec![1]);
    let edited = [Migration {
        version: 1,
        name: "0001_t",
        sql: "CREATE TABLE t (x INTEGER, y INTEGER);",
    }];
    match migrate::migrate_with(&mut conn, &edited) {
        Err(StorageError::Migration {
            version: 1,
            message,
        }) => {
            assert!(message.contains("checksum"), "{message}")
        }
        other => panic!("expected checksum error, got {other:?}"),
    }
}

#[test]
fn applies_missing_migrations_in_order_and_rejects_unknown_ones() {
    let mut conn = Connection::open_in_memory().unwrap();
    let m1 = Migration {
        version: 1,
        name: "0001_a",
        sql: "CREATE TABLE a (x INTEGER);",
    };
    let m2 = Migration {
        version: 2,
        name: "0002_b",
        sql: "CREATE TABLE b (x INTEGER);",
    };
    let m100 = Migration {
        version: 100,
        name: "0100_c",
        sql: "INSERT INTO a VALUES (1);",
    };
    // A lane merge: 1 and 100 applied, 2 arrives later.
    assert_eq!(
        migrate::migrate_with(&mut conn, &[m1, m100]).unwrap(),
        vec![1, 100]
    );
    assert_eq!(migrate::current_version(&conn).unwrap(), 100);
    assert_eq!(
        migrate::migrate_with(&mut conn, &[m1, m2, m100]).unwrap(),
        vec![2]
    );
    assert_eq!(migrate::current_version(&conn).unwrap(), 100);
    // A build that doesn't know 2 refuses rather than guessing.
    assert!(matches!(
        migrate::migrate_with(&mut conn, &[m1, m100]),
        Err(StorageError::Migration { version: 2, .. })
    ));
}

#[test]
fn failed_migration_rolls_back_and_reports_version() {
    let mut conn = Connection::open_in_memory().unwrap();
    let good = Migration {
        version: 1,
        name: "0001_a",
        sql: "CREATE TABLE a (x INTEGER);",
    };
    let bad = Migration {
        version: 2,
        name: "0002_bad",
        sql: "CREATE TABLE b (x INTEGER); THIS IS NOT SQL;",
    };
    assert!(matches!(
        migrate::migrate_with(&mut conn, &[good, bad]),
        Err(StorageError::Migration { version: 2, .. })
    ));
    assert_eq!(migrate::current_version(&conn).unwrap(), 1);
    assert!(!migrate::schema_object_exists(&conn, "b").unwrap());
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

/// A database created before `schema_migrations` existed (only `0001_init`, tracked by
/// `user_version`) is adopted, snapshotted and upgraded by `SqliteStore::open`.
#[test]
fn adopts_legacy_database_with_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    {
        let conn = Connection::open(&path).unwrap();
        migrate::configure_connection(&conn).unwrap();
        conn.execute_batch(MIGRATIONS[0].sql).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute_batch(
            "INSERT INTO notetypes (id, name, kind, fields, templates, sort_field, created_at, updated_at)
               VALUES ('nt', 'N', 'standard', '[]', '[]', 'front', 0, 0);
             INSERT INTO notes (id, notetype_id, fields, checksum, created_at, updated_at)
               VALUES ('n1', 'nt', '{}', 'x', 0, 0);
             INSERT INTO notes_fts (note_id, sort_field, content) VALUES ('n1', 'dog', 'dog');",
        )
        .unwrap();
    }
    let store = SqliteStore::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), migrate::latest_version());
    let backup = store.migration_backup().unwrap().to_owned();
    assert!(backup.exists(), "snapshot written before migrating");
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("collection.db.pre-v1-to-v"));
    drop(store);

    let conn = Connection::open(&path).unwrap();
    let recorded: Vec<u32> = conn
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(recorded, all_versions());
    // 0002 back-filled the FTS link.
    let linked: Option<i64> = conn
        .query_row("SELECT fts_rowid FROM notes WHERE id = 'n1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(linked.is_some());
    // The snapshot is the old schema.
    let old = Connection::open(&backup).unwrap();
    assert_eq!(migrate::current_version(&old).unwrap(), 1);

    // Re-opening an up-to-date database takes no snapshot.
    let again = SqliteStore::open(&path).unwrap();
    assert!(again.migration_backup().is_none());
}

#[test]
fn fresh_file_database_gets_no_snapshot_and_reopens() {
    let fx = Fx::new();
    assert!(fx.store.migration_backup().is_none());
    assert_eq!(
        fx.store.schema_version().unwrap(),
        migrate::latest_version()
    );
    let path = fx.db_path();
    drop(fx.store);
    let reopened = SqliteStore::open(&path).unwrap();
    assert_eq!(
        reopened.schema_version().unwrap(),
        migrate::latest_version()
    );
    let mode: String = fx_raw_journal_mode(&path);
    assert_eq!(mode.to_lowercase(), "wal");
}

fn fx_raw_journal_mode(path: &std::path::Path) -> String {
    Connection::open(path)
        .unwrap()
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap()
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
