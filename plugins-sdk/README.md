# Flashcard plugin SDK

Plugins are WASM modules (`wasm32-unknown-unknown`) run in a sandbox with fuel, time and
memory limits. They can only do what the user allowed (docs/05).

```
plugins-sdk/
  rust/         fc-plugin-sdk: Plugin trait, export_plugin!, typed host calls, UI builders, fake host
  examples/     autofill (hook), scramble-card (card type), tone-tts (TTS), reverse-drill (study mode),
                misbehave (sandbox tests)
```

## A plugin in Rust

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib"]
[dependencies]
fc-plugin-sdk = { path = "…/plugins-sdk/rust" }
```

```rust
use fc_plugin_sdk::prelude::*;

#[derive(Default)]
struct Hello;

impl Plugin for Hello {
    fn command(&self, command: &str, _params: Value) -> Result<Value> {
        let decks = host::invoke("v1.decks.list", json!({}))?; // needs `decks:read`
        Ok(json!({ "command": command, "decks": decks }))
    }
}

export_plugin!(Hello);
```

`plugin.toml` (see `examples/*/plugin.toml`):

```toml
id = "com.example.hello"
name = "Hello"
version = "1.0.0"
api_version = "1"
permissions = ["ui:command", "decks:read"]
[contributes]
commands = ["hello"]
locales = "locales"       # locales/<lang>.json, flat key → text
```

Build and pack:

```bash
cargo build --release --target wasm32-unknown-unknown
# zip plugin.toml + the .wasm (as `entry`, default plugin.wasm) + locales/ + settings schema
# into <name>.fcplugin — fc_plugins::package::pack_dir does exactly this.
```

Install in the app: Plugins → Install from file (or `fc call v1.plugins.install '{"path": "…", "grant": [...]}'`).

## Rules of the road

* Every call runs in a fresh instance: keep state in `host::kv` or a UI session's `state`.
  Writing `host::kv` needs the `storage:kv` permission (reading and deleting do not); each
  plugin's storage has a quota the user sets (entries and MB).
* Storage (`host::kv`) and your own settings (`host::set_setting`) are written with the rest
  of the call: if the plugin fails, they are rolled back too.
* Files only through the user's dialog: `host::fs::open` / `host::fs::save` (`fs:pick`, during
  a user-started command or view). You get a file name and bytes, never a path.
* Never grantable: `plugins:*`, `io:*`, `mcp:*`, `ai_keys:*`, `ai_requests:*`,
  `settings:write`, `modules:write`, `oplog:write`, `oplog:read`, `ai:write`, `batch:write`,
  `jobs:write`. A `hooks:before|after:<method>` needs that method's own permission and the
  read permission of the data it sees (`notes:read` for `notes.create`…), and plugin
  management, settings, modules, undo, `io.*`, `batch.*`, `jobs.*`, `mcp.*` and AI provider
  methods cannot be hooked.
* You see only what you may read: without `<area>:read` a query is refused, a command's
  response and an `event:` hook's event are cut to ids; results that hold several areas (search
  rows, study sessions) need each area's read permission (docs/05 §13).
* `fc plugin pack <folder>` packs a built plugin; `fc plugin install <file> --grant …` installs it.
* A `before:` hook may change params or reject, but cannot write. Changes made by plugins
  trigger no hooks. What an operation writes is one undo step, rolled back if the plugin fails.
* Limits (docs/05 §12, "Host fonksiyonu sınırları"): one call — host calls included — must
  finish within the time limit (3 s by default) and makes at most 10 000 host calls moving at
  most 64 MiB; each host function has its own size and count limits (`host::notify` needs
  `ui:notify`: 3 per call, 10 per minute). Host calls cost fuel. A refused host call returns
  `VALIDATION_PLUGIN_HOST_LIMIT`; going past a call budget stops the plugin.
* Texts: return `ui::key("…")` (your locale files) or `ui::text(data)`; never UI wording as data.
* Native unit tests: `fc_plugin_sdk::testing::with_host(|func, args| …, || …)`.
