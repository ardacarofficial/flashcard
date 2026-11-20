use super::*;
use serde_json::json;

/// A module whose `fc_handle` body is `body` (locals: `$ptr`, `$len`), with a reply
/// `{"ok":"pong"}` at offset 0 (13 bytes) and a bump allocator.
fn module(body: &str) -> String {
    format!(
        r#"(module
  (import "fc" "host_call" (func $host_call (param i32 i32) (result i32)))
  (import "fc" "host_reply" (func $host_reply (param i32)))
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 1024))
  (data (i32.const 0) "{{\"ok\":\"pong\"}}")
  (func (export "fc_abi_version") (result i32) (i32.const 1))
  (func $alloc (export "fc_alloc") (param $len i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $len)))
    (local.get $p))
  (func (export "fc_handle") (param $ptr i32) (param $len i32) (result i64)
    {body})
)"#
    )
}

const PONG: &str = "(i64.const 13)";

struct Recorder(Vec<(String, Value)>);

impl HostCalls for Recorder {
    fn call(&mut self, func: &str, args: Value) -> Result<Value, HostError> {
        self.0.push((func.to_owned(), args));
        if func == "invoke" {
            return Err(WireError {
                code: "PERMISSION_DENIED".into(),
                message: "no".into(),
                details: json!({ "permission": "decks:read" }),
            });
        }
        Ok(json!({ "decks": [] }))
    }
}

fn limits() -> Limits {
    Limits {
        fuel: 50_000_000,
        timeout: Duration::from_secs(5),
        memory_bytes: 4 * 1024 * 1024,
    }
}

#[test]
fn calls_return_the_plugin_reply() {
    let rt = Runtime::new().unwrap();
    let p = rt.compile(module(PONG).as_bytes()).unwrap();
    let out = rt
        .call(&p, "command", json!({}), &limits(), &mut PureHost)
        .unwrap();
    assert_eq!(out, json!("pong"));
    // Cached by content hash.
    let again = rt.compile(module(PONG).as_bytes()).unwrap();
    assert!(Arc::ptr_eq(&p, &again));
}

#[test]
fn infinite_loop_runs_out_of_fuel() {
    let rt = Runtime::new().unwrap();
    let p = rt
        .compile(module("(loop $l (br $l)) (i64.const 0)").as_bytes())
        .unwrap();
    let e = rt
        .call(&p, "command", json!({}), &limits(), &mut PureHost)
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::OutOfFuel);
    assert!(e.kind.is_fault());
}

#[test]
fn infinite_loop_hits_the_deadline() {
    let rt = Runtime::new().unwrap();
    let p = rt
        .compile(module("(loop $l (br $l)) (i64.const 0)").as_bytes())
        .unwrap();
    let l = Limits {
        fuel: u64::MAX / 2,
        timeout: Duration::from_millis(100),
        ..limits()
    };
    let started = Instant::now();
    let e = rt
        .call(&p, "command", json!({}), &l, &mut PureHost)
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn traps_and_memory_cap_are_errors_not_crashes() {
    let rt = Runtime::new().unwrap();
    let p = rt.compile(module("unreachable").as_bytes()).unwrap();
    let e = rt
        .call(&p, "command", json!({}), &limits(), &mut PureHost)
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::Trap);

    // Grows memory a page at a time until refused, then gives up like an allocator would.
    let hog = "(block $done (loop $l (br_if $done (i32.eq (memory.grow (i32.const 1)) (i32.const -1))) (br $l))) unreachable";
    let p = rt.compile(module(hog).as_bytes()).unwrap();
    let e = rt
        .call(&p, "command", json!({}), &limits(), &mut PureHost)
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::MemoryLimit);

    // Out-of-bounds reply.
    let p = rt
        .compile(module("(i64.const 0x7fffffff00000010)").as_bytes())
        .unwrap();
    let e = rt
        .call(&p, "command", json!({}), &limits(), &mut PureHost)
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::Abi);

    // The runtime keeps working.
    let p = rt.compile(module(PONG).as_bytes()).unwrap();
    assert_eq!(
        rt.call(&p, "command", json!({}), &limits(), &mut PureHost)
            .unwrap(),
        json!("pong")
    );
}

#[test]
fn modules_breaking_the_abi_are_rejected() {
    let rt = Runtime::new().unwrap();
    let e = rt.compile(b"(module)").unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::Abi);
    let wasi = module(PONG).replace(
        "(memory",
        "(import \"wasi_snapshot_preview1\" \"fd_write\" (func (param i32 i32 i32 i32) (result i32)))\n  (memory",
    );
    assert_eq!(
        rt.compile(wasi.as_bytes()).unwrap_err().kind,
        PluginErrorKind::Abi
    );
    let v2 = module(PONG).replace("(result i32) (i32.const 1)", "(result i32) (i32.const 2)");
    let p = rt.compile(v2.as_bytes()).unwrap();
    let e = rt
        .call(&p, "command", json!({}), &limits(), &mut PureHost)
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::Abi);
}

/// The frozen v1 fixture: host calls are serviced on the calling thread, errors forwarded.
#[test]
fn abi_v1_contract() {
    let rt = Runtime::new().unwrap();
    let wat = include_str!("../../tests/fixtures/abi_v1.wat");
    let p = rt.compile(wat.as_bytes()).unwrap();
    let mut host = Recorder(Vec::new());
    let e = rt
        .call(
            &p,
            "command",
            json!({ "command": "x" }),
            &limits(),
            &mut host,
        )
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::Plugin);
    assert_eq!(e.code, "PERMISSION_DENIED");
    assert_eq!(e.details, json!({ "permission": "decks:read" }));
    assert_eq!(
        host.0,
        vec![(
            "invoke".to_owned(),
            json!({ "method": "v1.decks.list", "params": {} })
        )]
    );
    // Host fn names of v1 never disappear.
    for f in [
        "invoke",
        "kv.get",
        "kv.set",
        "kv.delete",
        "kv.list",
        "settings.get",
        "log",
        "ui.notify",
        "tts.synthesize",
        "tts.voices",
        "http.fetch",
        "ai.complete",
        "models.list",
    ] {
        assert!(crate::abi::host_fns::ALL.contains(&f), "{f}");
    }
}

#[test]
fn host_time_does_not_count_against_the_deadline() {
    struct Slow;
    impl HostCalls for Slow {
        fn call(&mut self, _f: &str, _a: Value) -> Result<Value, HostError> {
            thread::sleep(Duration::from_millis(300));
            Ok(json!(1))
        }
    }
    let rt = Runtime::new().unwrap();
    let wat = include_str!("../../tests/fixtures/abi_v1.wat");
    let p = rt.compile(wat.as_bytes()).unwrap();
    let l = Limits {
        timeout: Duration::from_millis(150),
        ..limits()
    };
    assert_eq!(
        rt.call(&p, "command", json!({}), &l, &mut Slow).unwrap(),
        json!(1)
    );
}
