//! Plugins (migration 0600).

use super::{all, exec, json, one, opt_ms, opt_ts, to_json, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{PluginPackageRecord, PluginRead, PluginRecord, PluginWrite};
use fc_core::Timestamp;
use rusqlite::{params, Row};
use serde_json::Value;

const T: &str = "plugins";
const COLS: &str =
    "id, version, manifest, enabled, granted_permissions, package_sha256, failures, \
                    last_error, auto_disabled, installed_at, updated_at, deleted_at";

fn plugin_row(r: &Row<'_>) -> rusqlite::Result<PluginRecord> {
    Ok(PluginRecord {
        id: r.get(0)?,
        version: r.get(1)?,
        manifest: json(r, 2)?,
        enabled: r.get(3)?,
        granted: json(r, 4)?,
        package_sha256: r.get(5)?,
        failures: r.get(6)?,
        last_error: r.get(7)?,
        auto_disabled: r.get(8)?,
        installed_at: ts(r, 9)?,
        updated_at: ts(r, 10)?,
        deleted_at: opt_ts(r, 11)?,
    })
}

impl<M: Mode> PluginRead for SqliteTx<'_, M> {
    fn list_plugins(&self, include_deleted: bool) -> StorageResult<Vec<PluginRecord>> {
        all(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM plugins WHERE ?1 OR deleted_at IS NULL ORDER BY id"),
            [include_deleted],
            plugin_row,
        )
    }

    fn get_plugin(&self, id: &str) -> StorageResult<Option<PluginRecord>> {
        one(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM plugins WHERE id = ?1"),
            [id],
            plugin_row,
        )
    }

    fn get_plugin_package(
        &self,
        plugin_id: &str,
        sha256: &str,
        with_wasm: bool,
    ) -> StorageResult<Option<PluginPackageRecord>> {
        one(
            self.conn(),
            "plugin_packages",
            "SELECT plugin_id, sha256, version, manifest, CASE WHEN ?3 THEN wasm ELSE x'' END,
                    settings, locales, created_at
             FROM plugin_packages WHERE plugin_id = ?1 AND sha256 = ?2",
            params![plugin_id, sha256, with_wasm],
            |r| {
                Ok(PluginPackageRecord {
                    plugin_id: r.get(0)?,
                    sha256: r.get(1)?,
                    version: r.get(2)?,
                    manifest: json(r, 3)?,
                    wasm: r.get(4)?,
                    settings: json(r, 5)?,
                    locales: json(r, 6)?,
                    created_at: ts(r, 7)?,
                })
            },
        )
    }

    fn get_plugin_kv(&self, plugin_id: &str, key: &str) -> StorageResult<Option<Value>> {
        one(
            self.conn(),
            "plugin_kv",
            "SELECT value FROM plugin_kv WHERE plugin_id = ?1 AND key = ?2",
            params![plugin_id, key],
            |r| json(r, 0),
        )
    }

    fn list_plugin_kv(&self, plugin_id: &str, prefix: &str) -> StorageResult<Vec<(String, Value)>> {
        all(
            self.conn(),
            "plugin_kv",
            "SELECT key, value FROM plugin_kv
             WHERE plugin_id = ?1 AND substr(key, 1, length(?2)) = ?2 ORDER BY key",
            params![plugin_id, prefix],
            |r| Ok((r.get(0)?, json(r, 1)?)),
        )
    }
}

impl PluginWrite for SqliteTx<'_, WriteMode> {
    fn upsert_plugin(&mut self, p: &PluginRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            T,
            &format!(
                "INSERT INTO plugins ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(id) DO UPDATE SET version = excluded.version,
                    manifest = excluded.manifest, enabled = excluded.enabled,
                    granted_permissions = excluded.granted_permissions,
                    package_sha256 = excluded.package_sha256, failures = excluded.failures,
                    last_error = excluded.last_error, auto_disabled = excluded.auto_disabled,
                    installed_at = excluded.installed_at, updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at"
            ),
            params![
                p.id,
                p.version,
                to_json(T, &p.manifest)?,
                p.enabled,
                to_json(T, &p.granted)?,
                p.package_sha256,
                p.failures,
                p.last_error,
                p.auto_disabled,
                p.installed_at.0,
                p.updated_at.0,
                opt_ms(p.deleted_at),
            ],
        )?;
        Ok(())
    }

    fn delete_plugin(&mut self, id: &str) -> StorageResult<()> {
        exec(self.conn(), T, "DELETE FROM plugins WHERE id = ?1", [id])?;
        Ok(())
    }

    fn insert_plugin_package(&mut self, p: &PluginPackageRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            "plugin_packages",
            "INSERT OR IGNORE INTO plugin_packages
                (plugin_id, sha256, version, manifest, wasm, settings, locales, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                p.plugin_id,
                p.sha256,
                p.version,
                to_json("plugin_packages", &p.manifest)?,
                p.wasm,
                to_json("plugin_packages", &p.settings)?,
                to_json("plugin_packages", &p.locales)?,
                p.created_at.0,
            ],
        )?;
        Ok(())
    }

    fn set_plugin_kv(
        &mut self,
        plugin_id: &str,
        key: &str,
        value: Option<&Value>,
        now: Timestamp,
    ) -> StorageResult<()> {
        match value {
            Some(v) => exec(
                self.conn(),
                "plugin_kv",
                "INSERT INTO plugin_kv (plugin_id, key, value, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(plugin_id, key) DO UPDATE SET value = excluded.value,
                     updated_at = excluded.updated_at",
                params![plugin_id, key, to_json("plugin_kv", v)?, now.0],
            )?,
            None => exec(
                self.conn(),
                "plugin_kv",
                "DELETE FROM plugin_kv WHERE plugin_id = ?1 AND key = ?2",
                params![plugin_id, key],
            )?,
        };
        Ok(())
    }
}
