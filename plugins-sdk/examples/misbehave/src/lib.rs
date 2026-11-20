//! Test plugin for the sandbox guarantees: infinite loops, panics, memory exhaustion and
//! calls without permission must end as errors while the app keeps running.

use fc_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct Misbehave;

#[inline(never)]
fn spin() -> ! {
    let mut x: u64 = 0;
    loop {
        x = std::hint::black_box(x.wrapping_add(1));
    }
}

impl Plugin for Misbehave {
    fn command(&self, command: &str, _params: Value) -> Result<Value> {
        match command {
            "ok" => Ok(json!("ok")),
            "loop" => spin(),
            "panic" => panic!("misbehave: deliberate panic"),
            "alloc" => {
                let mut hog: Vec<Vec<u8>> = Vec::new();
                loop {
                    hog.push(vec![1u8; 1024 * 1024]);
                    std::hint::black_box(&hog);
                }
            }
            // Needs decks:write, which the tests do not grant.
            "forbidden" => host::invoke("v1.decks.create", json!({ "path": "Plugin deck" })),
            // Needs decks:read (granted in tests).
            "allowed" => host::invoke("v1.decks.list", json!({})),
            // Writes, then fails: the write must be rolled back.
            "write-panic" => {
                host::invoke("v1.decks.create", json!({ "path": "Half done" }))?;
                panic!("misbehave: failed after writing")
            }
            "kv" => {
                let n: u64 = host::kv::get("n")?.unwrap_or(0);
                host::kv::set("n", &(n + 1))?;
                Ok(json!(n + 1))
            }
            other => Err(Error::unsupported(other)),
        }
    }

    fn before_hook(&self, _hook: &str, _method: &str, params: Value) -> Result<BeforeHook> {
        let name = params["path"].as_str().unwrap_or_default();
        match name {
            "blocked" => Ok(BeforeHook::Reject("this name is not allowed".into())),
            "hook-loop" => spin(),
            "hook-panic" => panic!("misbehave: hook panic"),
            _ => Ok(BeforeHook::Continue(None)),
        }
    }

    fn on_event(&self, _hook: &str, _event: Value) -> Result<()> {
        panic!("misbehave: event handler panic")
    }
}

export_plugin!(Misbehave);
