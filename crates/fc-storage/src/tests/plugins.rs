//! Plugins (migration 0600).

use super::*;
use crate::repo::{PluginPackageRecord, PluginRecord};
use serde_json::json;

fn record(id: &str, sha: &str) -> PluginRecord {
    PluginRecord {
        id: id.into(),
        version: "1.0.0".into(),
        manifest: json!({ "id": id }),
        enabled: true,
        granted: vec!["notes:read".into()],
        package_sha256: sha.into(),
        failures: 0,
        last_error: None,
        auto_disabled: false,
        installed_at: Timestamp(1),
        updated_at: Timestamp(2),
        deleted_at: None,
    }
}

fn package(id: &str, sha: &str) -> PluginPackageRecord {
    PluginPackageRecord {
        plugin_id: id.into(),
        sha256: sha.into(),
        version: "1.0.0".into(),
        manifest: json!({ "id": id }),
        wasm: b"\0asm\x01\0\0\0".to_vec(),
        settings: json!([]),
        locales: json!({ "tr": { "name": "Ad" } }),
        created_at: Timestamp(1),
    }
}

#[test]
fn plugins_packages_and_kv_roundtrip() {
    let fx = Fx::new();
    let sha = "a".repeat(64);
    fx.write(|tx| {
        tx.insert_plugin_package(&package("com.x.a", &sha)).unwrap();
        // Idempotent.
        tx.insert_plugin_package(&package("com.x.a", &sha)).unwrap();
        tx.upsert_plugin(&record("com.x.a", &sha)).unwrap();
        tx.set_plugin_kv("com.x.a", "n", Some(&json!(3)), Timestamp(5))
            .unwrap();
        tx.set_plugin_kv("com.x.a", "other", Some(&json!("x")), Timestamp(5))
            .unwrap();
    });
    fx.read(|r| {
        assert_eq!(
            r.get_plugin("com.x.a").unwrap(),
            Some(record("com.x.a", &sha))
        );
        let p = r
            .get_plugin_package("com.x.a", &sha, true)
            .unwrap()
            .unwrap();
        assert_eq!(p, package("com.x.a", &sha));
        let light = r
            .get_plugin_package("com.x.a", &sha, false)
            .unwrap()
            .unwrap();
        assert!(light.wasm.is_empty());
        assert_eq!(r.get_plugin_kv("com.x.a", "n").unwrap(), Some(json!(3)));
        assert_eq!(
            r.list_plugin_kv("com.x.a", "n").unwrap(),
            vec![("n".into(), json!(3))]
        );
        assert_eq!(r.list_plugin_kv("com.x.a", "").unwrap().len(), 2);
    });
    // Soft delete hides it from the default list; hard delete drops its storage.
    let mut gone = record("com.x.a", &sha);
    gone.deleted_at = Some(Timestamp(9));
    fx.write(|tx| tx.upsert_plugin(&gone).unwrap());
    fx.read(|r| {
        assert!(r.list_plugins(false).unwrap().is_empty());
        assert_eq!(r.list_plugins(true).unwrap(), vec![gone.clone()]);
    });
    fx.write(|tx| {
        tx.set_plugin_kv("com.x.a", "n", None, Timestamp(6))
            .unwrap();
        tx.delete_plugin("com.x.a").unwrap();
    });
    fx.read(|r| {
        assert_eq!(r.get_plugin("com.x.a").unwrap(), None);
        assert!(r.list_plugin_kv("com.x.a", "").unwrap().is_empty());
        // Packages stay (redo of an undone install needs them).
        assert!(r
            .get_plugin_package("com.x.a", &sha, false)
            .unwrap()
            .is_some());
    });
}

#[test]
fn migration_0600_extends_plugin_rows_of_older_databases() {
    use crate::migrate::MIGRATIONS;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let upto: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 600)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &upto).unwrap());
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO plugins (id, version, manifest, enabled, granted_permissions, installed_at, updated_at)
             VALUES ('com.old.one', '0.1.0', '{}', 1, '[\"notes:read\"]', 1, 1)",
            [],
        )
        .unwrap();
    }
    let store = SqliteStore::open(&path).unwrap();
    let r = store.read().unwrap();
    let p = r.get_plugin("com.old.one").unwrap().unwrap();
    assert_eq!(p.package_sha256, "");
    assert_eq!(
        (p.failures, p.auto_disabled, p.deleted_at),
        (0, false, None)
    );
    assert_eq!(p.granted, vec!["notes:read".to_owned()]);
    let conn = Connection::open(&path).unwrap();
    assert!(crate::migrate::schema_object_exists(&conn, "plugin_packages").unwrap());
    // A package needs a 64-hex sha.
    assert!(conn
        .execute(
            "INSERT INTO plugin_packages (plugin_id, sha256, version, manifest, wasm, created_at)
             VALUES ('p', 'short', '1', '{}', x'00', 1)",
            [],
        )
        .is_err());
}
