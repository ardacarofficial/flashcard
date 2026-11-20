//! `mcp_servers` (v0.8): migration `0750` and the repository round trip.

use super::{u, Fx};
use crate::migrate::MIGRATIONS;
use crate::repo::McpServerRecord;
use crate::store::{SqliteStore, Store};
use fc_core::{McpServerId, Timestamp};

fn record(n: u128, name: &str) -> McpServerRecord {
    McpServerRecord {
        id: McpServerId(u(n)),
        name: name.into(),
        transport: "stdio".into(),
        command: "npx".into(),
        args: vec![
            "-y".into(),
            "@modelcontextprotocol/server-filesystem".into(),
        ],
        url: None,
        enabled: true,
        approved: Some("[\"stdio\"]".into()),
        created_at: Timestamp(1),
        updated_at: Timestamp(2),
    }
}

#[test]
fn migration_0750_adds_mcp_servers_to_an_existing_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    // A Phase-2 database (everything before the v0.8 range).
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 750)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &before).unwrap());
    let has_table = |conn: &rusqlite::Connection| -> bool {
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'mcp_servers'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            == 1
    };
    assert!(!has_table(&rusqlite::Connection::open(&path).unwrap()));

    let store = SqliteStore::open(&path).unwrap();
    assert!(has_table(&rusqlite::Connection::open(&path).unwrap()));
    let mut tx = store.write().unwrap();
    tx.upsert_mcp_server(&record(1, "Files")).unwrap();
    tx.commit().unwrap();
    assert_eq!(store.read().unwrap().list_mcp_servers().unwrap().len(), 1);
}

#[test]
fn mcp_servers_round_trip_and_names_are_unique() {
    let fx = Fx::new();
    let a = record(1, "Notion");
    let mut b = record(2, "files");
    b.transport = "http".into();
    b.command = String::new();
    b.args = vec![];
    b.url = Some("http://127.0.0.1:9000/mcp".into());
    b.enabled = false;
    fx.write(|tx| {
        tx.upsert_mcp_server(&a).unwrap();
        tx.upsert_mcp_server(&b).unwrap();
    });
    // By name, case-insensitively.
    assert_eq!(
        fx.read(|r| r.list_mcp_servers().unwrap()),
        vec![b.clone(), a.clone()]
    );
    assert_eq!(
        fx.read(|r| r.get_mcp_server(b.id).unwrap()),
        Some(b.clone())
    );

    // Same name in another case is refused by the index.
    let clash = record(3, "NOTION");
    let mut tx = fx.store.write().unwrap();
    assert!(tx.upsert_mcp_server(&clash).is_err());
    drop(tx);

    let mut renamed = a.clone();
    renamed.name = "Notion work".into();
    renamed.updated_at = Timestamp(9);
    fx.write(|tx| tx.upsert_mcp_server(&renamed).unwrap());
    assert_eq!(fx.read(|r| r.get_mcp_server(a.id).unwrap()), Some(renamed));
    assert!(fx.write(|tx| tx.delete_mcp_server(a.id).unwrap()));
    assert!(!fx.write(|tx| tx.delete_mcp_server(a.id).unwrap()));
    assert_eq!(fx.read(|r| r.list_mcp_servers().unwrap()), vec![b]);
}

#[test]
fn migration_0751_adds_approval_and_leaves_old_rows_unapproved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 751)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &before).unwrap());
    // A registration made before approvals existed.
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO mcp_servers (id, name, transport, command, args, enabled, created_at, updated_at)
             VALUES (?1, 'Old', 'stdio', 'npx', '[]', 1, 1, 2)",
            [McpServerId(u(7)).to_string()],
        )
        .unwrap();

    let store = SqliteStore::open(&path).unwrap();
    let rows = store.read().unwrap().list_mcp_servers().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].approved, None,
        "nothing was ever approved: the user re-saves it"
    );
}

#[test]
fn open_current_never_migrates_or_creates() {
    use crate::migrate::pending_versions;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    // Missing file: not created.
    assert!(SqliteStore::open_current(&path).is_err());
    assert!(!path.exists());

    // Older schema: refused, and left exactly as it was.
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 750)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &before).unwrap());
    let err = SqliteStore::open_current(&path).err().unwrap();
    assert!(
        matches!(err, crate::StorageError::NeedsMigration { .. }),
        "{err:?}"
    );
    let mut conn = rusqlite::Connection::open(&path).unwrap();
    assert!(!pending_versions(&mut conn).unwrap().is_empty());
    drop(conn);
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        1,
        "no snapshot either"
    );

    // Current schema: opens and works.
    drop(SqliteStore::open(&path).unwrap());
    let store = SqliteStore::open_current(&path).unwrap();
    assert!(store.read().unwrap().list_mcp_servers().unwrap().is_empty());
    assert!(store.migration_backup().is_none());
}
