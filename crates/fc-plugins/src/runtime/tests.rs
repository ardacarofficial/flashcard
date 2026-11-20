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
        ..Limits::default()
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

/// The deadline covers the whole call: a slow host call ends it (checked on exit), and no
/// further host call starts after it (checked on entry).
#[test]
fn host_time_counts_against_the_deadline() {
    struct Slow(u32);
    impl HostCalls for Slow {
        fn call(&mut self, _f: &str, _a: Value) -> Result<Value, HostError> {
            self.0 += 1;
            thread::sleep(Duration::from_millis(120));
            Ok(json!(1))
        }
    }
    let rt = Runtime::new().unwrap();
    let p = rt
        .compile(
            crate::testing::host_loop_wat(r#"{"fn":"log","args":{"message":"x"}}"#, 1_000)
                .as_bytes(),
        )
        .unwrap();
    let l = Limits {
        timeout: Duration::from_millis(300),
        ..limits()
    };
    let mut host = Slow(0);
    let started = Instant::now();
    let e = rt
        .call(&p, "command", json!({}), &l, &mut host)
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::Timeout);
    assert!(host.0 <= 3, "{} host calls ran", host.0);
    assert!(
        started.elapsed() < Duration::from_millis(700),
        "{:?}",
        started.elapsed()
    );
}

/// Time the host reports as waiting for the user (a file dialog) is not counted.
#[test]
fn user_wait_does_not_count_against_the_deadline() {
    struct Dialog;
    impl HostCalls for Dialog {
        fn call(&mut self, _f: &str, _a: Value) -> Result<Value, HostError> {
            thread::sleep(Duration::from_millis(300));
            Ok(json!(1))
        }
        fn paused(&mut self) -> Duration {
            Duration::from_millis(300)
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
        rt.call(&p, "command", json!({}), &l, &mut Dialog).unwrap(),
        json!(1)
    );
}

struct Counting(u64);

impl HostCalls for Counting {
    fn call(&mut self, _f: &str, _a: Value) -> Result<Value, HostError> {
        self.0 += 1;
        Ok(Value::Null)
    }
}

const KV_GET: &str = r#"{"fn":"kv.get","args":{"key":"a"}}"#;

/// The security review's repro: a loop of a million cheap host calls ends on the host-call
/// cap, on fuel (host calls cost fuel), or on the deadline — whichever comes first — and
/// always quickly.
#[test]
fn host_call_loops_are_bounded() {
    let rt = Runtime::new().unwrap();
    let p = rt
        .compile(crate::testing::host_loop_wat(KV_GET, 1_000_000).as_bytes())
        .unwrap();
    let generous = Limits {
        fuel: u64::MAX / 4,
        timeout: Duration::from_secs(60),
        ..limits()
    };
    // The cap on host calls per plugin call.
    let l = Limits {
        max_host_calls: 500,
        ..generous
    };
    let mut host = Counting(0);
    let e = rt
        .call(&p, "command", json!({}), &l, &mut host)
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::HostLimit);
    assert!(e.kind.is_fault());
    assert_eq!(host.0, 500);
    // Fuel: each host call costs fuel even though the WASM side does almost nothing.
    let l = Limits {
        fuel: 1_000_000_000,
        max_host_calls: u32::MAX,
        ..generous
    };
    let mut host = Counting(0);
    let e = rt
        .call(&p, "command", json!({}), &l, &mut host)
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::OutOfFuel);
    let per_call = limits::HOST_CALL_FUEL + limits::host_fn("kv.get").fuel;
    assert!(host.0 <= 1_000_000_000 / per_call, "{}", host.0);
    // The deadline, host time included.
    let l = Limits {
        timeout: Duration::from_millis(200),
        max_host_calls: u32::MAX,
        ..generous
    };
    let started = Instant::now();
    let e = rt
        .call(&p, "command", json!({}), &l, &mut Counting(0))
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

/// Requests over their function's size limit, and calls over its per-call count, are
/// refused before the host sees them; the plugin gets `VALIDATION_PLUGIN_HOST_LIMIT`.
#[test]
fn oversized_and_too_frequent_host_calls_are_refused() {
    let rt = Runtime::new().unwrap();
    let big = crate::testing::notify_request(1024 * 1024);
    let p = rt
        .compile(crate::testing::host_loop_wat(&big, 3).as_bytes())
        .unwrap();
    let mut host = Counting(0);
    let e = rt
        .call(&p, "command", json!({}), &limits(), &mut host)
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::Plugin);
    assert_eq!(e.code, limits::HOST_LIMIT_CODE);
    assert_eq!(e.details["limit"], "request_bytes");
    assert_eq!(host.0, 0, "the host never saw it");
    // Many of them: the byte budget of the call stops the plugin.
    let p = rt
        .compile(crate::testing::host_loop_wat(&big, 400).as_bytes())
        .unwrap();
    let l = Limits {
        fuel: u64::MAX / 4,
        ..limits()
    };
    let e = rt
        .call(&p, "command", json!({}), &l, &mut host)
        .unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::HostLimit);
    assert_eq!(host.0, 0);
    // Small ones: three per call reach the host, the rest are refused.
    let p = rt
        .compile(crate::testing::host_loop_wat(&crate::testing::notify_request(10), 50).as_bytes())
        .unwrap();
    let e = rt
        .call(&p, "command", json!({}), &limits(), &mut host)
        .unwrap_err();
    assert_eq!(e.code, limits::HOST_LIMIT_CODE);
    assert_eq!(e.details["limit"], "per_call");
    assert_eq!(host.0, 3);
}

/// A reply over the function's limit is replaced by a refusal.
#[test]
fn oversized_host_replies_are_refused() {
    struct Big;
    impl HostCalls for Big {
        fn call(&mut self, _f: &str, _a: Value) -> Result<Value, HostError> {
            Ok(json!("y".repeat(4096)))
        }
    }
    let rt = Runtime::new().unwrap();
    let p = rt
        .compile(
            crate::testing::host_loop_wat(r#"{"fn":"log","args":{"message":"x"}}"#, 1).as_bytes(),
        )
        .unwrap();
    let e = rt
        .call(&p, "command", json!({}), &limits(), &mut Big)
        .unwrap_err();
    assert_eq!(e.code, limits::HOST_LIMIT_CODE);
    assert_eq!(e.details["limit"], "reply_bytes");
}

/// 99 memories × 64 MiB against a 64 MiB cap (security review): refused at compile time,
/// because multi-memory is off.
#[test]
fn many_memories_are_rejected() {
    let rt = Runtime::new().unwrap();
    let wat = include_str!("../../tests/fixtures/many_memories.wat");
    let e = rt.compile(wat.as_bytes()).unwrap_err();
    assert_eq!(e.kind, PluginErrorKind::Abi);
    assert!(e.message.contains("multi"), "{}", e.message);
}

/// Other proposals that would sidestep the memory cap or the 32-bit host pointers.
#[test]
fn unneeded_proposals_are_off() {
    let rt = Runtime::new().unwrap();
    for (what, memory) in [
        ("memory64", "(memory (export \"memory\") i64 1)"),
        ("shared memory", "(memory (export \"memory\") 1 2 shared)"),
    ] {
        let m = module(PONG).replace("(memory (export \"memory\") 1)", memory);
        assert!(rt.compile(m.as_bytes()).is_err(), "{what} must be refused");
    }
    // A second table is fine (reference types), but many are not.
    let tables: String = (0..8)
        .map(|i| format!("(table $t{i} 1 funcref)\n"))
        .collect();
    let m = module(PONG).replace("(global $heap", &format!("{tables}(global $heap"));
    let p = rt.compile(m.as_bytes()).unwrap();
    assert!(rt
        .call(&p, "command", json!({}), &limits(), &mut PureHost)
        .is_err());
}

/// The limiter caps memories and tables *together*, not each on its own, and allows one
/// memory: checked on an engine that does allow multi-memory, so the caps hold even if a
/// proposal is switched on later.
#[test]
fn limiter_counts_all_memories_and_tables_together() {
    let mut config = Config::new();
    config.wasm_multi_memory(true);
    let engine = Engine::new(&config).unwrap();
    let cap = 4 * 1024 * 1024;
    let instantiate = |wat: &str| {
        let module = Module::new(&engine, wat).unwrap();
        let mut store = Store::new(&engine, Limiter::new(cap));
        store.limiter(|l| l as &mut dyn ResourceLimiter);
        let r = wasmtime::Instance::new(&mut store, &module, &[]);
        (r.is_ok(), store.data().memory_total, store.data().hit)
    };
    // Two memories of 2 MiB each: refused (one memory per instance), although together
    // they would fit.
    let (ok, _, _) = instantiate("(module (memory 32) (memory 32))");
    assert!(!ok);
    // One memory: up to the cap.
    let (ok, total, _) = instantiate("(module (memory 64))");
    assert!(ok);
    assert_eq!(total, cap);
    let (ok, _, hit) = instantiate("(module (memory 65))");
    assert!(!ok && hit);

    // Growth is counted against the total as well.
    let mut l = Limiter::new(cap);
    assert!(l.memory_growing(0, cap / 2, None).unwrap());
    assert!(l.memory_growing(0, cap / 2, None).unwrap());
    assert!(!l.memory_growing(cap / 2, cap / 2 + 65536, None).unwrap());
    assert!(l.hit);
    // Tables: total elements.
    let mut l = Limiter::new(cap);
    assert!(l.table_growing(0, MAX_TABLE_ELEMENTS / 2, None).unwrap());
    assert!(l.table_growing(0, MAX_TABLE_ELEMENTS / 2, None).unwrap());
    assert!(!l.table_growing(0, 1, None).unwrap());
    assert_eq!(l.memories(), 1);
}
