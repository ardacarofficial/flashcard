//! Hostile test plugin. Every command and hook here is an attempt to get out of the sandbox
//! that the host must refuse (or contain); `fc-app`'s plugin tests assert each outcome:
//!
//! * hooks on plugin management, export/restore, settings, undo → the manifest is refused at
//!   install, so `before_hook` below never sees those methods;
//! * a hook on `notes.create` runs only while `notes:write` is granted too;
//! * calls into `plugins.*`, `io.*`, `settings.*`, `modules.*`, `oplog.*` → `PERMISSION_DENIED`;
//! * raising its own limits through its plugin settings → only keys of its own schema exist;
//! * storage written before a crash → rolled back with the rest of the call;
//! * files → only through the user's dialog; a name and bytes, never a path.

use fc_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct Escape;

impl Plugin for Escape {
    fn command(&self, command: &str, _params: Value) -> Result<Value> {
        match command {
            "self-grant" => host::invoke(
                "v1.plugins.set_permissions",
                json!({ "id": "com.example.escape", "granted": ["decks:write", "notes:write"] }),
            ),
            "export" => host::invoke(
                "v1.io.export.start",
                json!({ "format": "json", "path": "/tmp/stolen.json", "scope": { "kind": "all" } }),
            ),
            "restore" => host::invoke(
                "v1.io.backup.restore",
                json!({ "path": "/tmp/evil.fcbackup" }),
            ),
            "raise-limits" => host::invoke(
                "v1.settings.set",
                json!({ "scope": { "kind": "app" }, "key": "plugins.fuel_millions", "value": 1_000_000 }),
            ),
            "module-off" => host::invoke(
                "v1.modules.set_enabled",
                json!({ "id": "plugins", "enabled": false }),
            ),
            "undo" => host::invoke("v1.oplog.undo", json!({})),
            "own-setting" => Ok(json!(host::set_setting("level", Some(json!(5)))?)),
            "app-setting-as-own" => Ok(json!(host::set_setting(
                "plugins.fuel_millions",
                Some(json!(1_000_000))
            )?)),
            "kv-only" => {
                let n: u64 = host::kv::get("n")?.unwrap_or(0);
                host::kv::set("n", &(n + 1))?;
                Ok(json!(n + 1))
            }
            "kv-then-panic" => {
                host::kv::set("secret", &"left behind")?;
                host::set_setting("level", Some(json!(9)))?;
                panic!("escape: crash after writing storage")
            }
            "write-and-kv" => {
                host::invoke("v1.decks.create", json!({ "path": "Escape deck" }))?;
                host::kv::set("deck", &true)?;
                Ok(json!(true))
            }
            "open" => Ok(json!(host::fs::open(&["txt"])?.map(|f| json!({
                "name": f.name,
                "size": f.size,
                "data_b64": f.data_b64,
            })))),
            "save" => Ok(json!(host::fs::save("../../escaped.txt", "aGk=", &[])?)),
            other => Err(Error::unsupported(other)),
        }
    }

    fn before_hook(&self, _hook: &str, method: &str, mut params: Value) -> Result<BeforeHook> {
        match method {
            // What it would do with the user's own calls if it could hook them.
            "plugins.set_permissions" | "plugins.install" => {
                params["granted"] = json!(["decks:write", "notes:write", "fs:pick"]);
                params["grant"] = params["granted"].clone();
            }
            "io.export.start" | "io.backup.restore" | "io.backup.create" => {
                params["path"] = json!("/tmp/stolen");
            }
            "settings.set" => params["value"] = json!(1_000_000),
            "notes.create" => params["fields"]["front"] = json!("hooked"),
            _ => return Ok(BeforeHook::Continue(None)),
        }
        Ok(BeforeHook::Continue(Some(params)))
    }
}

export_plugin!(Escape);
