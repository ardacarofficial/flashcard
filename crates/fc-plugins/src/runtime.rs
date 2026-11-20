//! The WASM sandbox (docs/05 §2, §7).
//!
//! * **Fuel:** every call starts with [`Limits::fuel`]; running out traps (`OUT_OF_FUEL`).
//! * **Deadline:** wall-clock time spent *inside the plugin* is capped by
//!   [`Limits::timeout`] (epoch interruption; time the plugin waits for a host call does not
//!   count). An infinite loop ends either way.
//! * **Memory:** all linear memory of an instance *together* may not grow past
//!   [`Limits::memory_bytes`]; tables together are capped at [`MAX_TABLE_ELEMENTS`]. One
//!   memory, a few tables, one instance. Proposals a plugin does not need and that would let
//!   it get around these caps or the host's assumptions are switched off (multi-memory,
//!   threads/shared memory, memory64, custom page sizes, GC, exceptions, stack switching,
//!   component model…).
//! * **Isolation:** a trap (panic, `unreachable`, out-of-bounds access…) becomes a
//!   [`PluginError`]; the host never panics because of a plugin. Every call gets a fresh
//!   instance (no state leaks between calls or after a failure).
//!
//! **Threading.** The instance runs on a short-lived scoped worker thread while the calling
//! thread services its host calls ([`HostCalls`]) over channels. So the host side may borrow
//! anything the caller holds — a write transaction included — without `'static` bounds, and
//! the worker can be abandoned safely on a trap.

use crate::abi::{
    HostReply, HostRequest, OpReply, OpRequest, WireError, IMPORT_MODULE, MAX_MESSAGE_BYTES,
    SUPPORTED_ABI,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use wasmtime::{
    AsContextMut, Caller, Config, Engine, InstancePre, Linker, Module, ResourceLimiter, Store,
    Trap, WasmFeatures,
};

/// Epoch tick.
const TICK: Duration = Duration::from_millis(5);

/// Per-call resource limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Fuel (≈ executed WASM instructions).
    pub fuel: u64,
    /// Time inside the plugin.
    pub timeout: Duration,
    /// Linear memory cap in bytes.
    pub memory_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            fuel: 2_000_000_000,
            timeout: Duration::from_secs(3),
            memory_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Failure class of a plugin call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginErrorKind {
    /// Fuel exhausted.
    OutOfFuel,
    /// Deadline passed.
    Timeout,
    /// Tried to grow memory past the cap.
    MemoryLimit,
    /// Any other trap (panic, `unreachable`, bad memory access…).
    Trap,
    /// The module breaks the ABI (missing export, bad JSON, unsupported ABI version…).
    Abi,
    /// The plugin returned an error of its own (`code`).
    Plugin,
}

impl PluginErrorKind {
    /// Whether this failure counts towards automatic disabling (the plugin misbehaved, as
    /// opposed to returning a domain error).
    pub fn is_fault(self) -> bool {
        !matches!(self, Self::Plugin)
    }

    /// Stable code (`OUT_OF_FUEL`, `TIMEOUT`, `MEMORY_LIMIT`, `TRAP`, `ABI`, `PLUGIN`).
    pub fn code(self) -> &'static str {
        match self {
            Self::OutOfFuel => "OUT_OF_FUEL",
            Self::Timeout => "TIMEOUT",
            Self::MemoryLimit => "MEMORY_LIMIT",
            Self::Trap => "TRAP",
            Self::Abi => "ABI",
            Self::Plugin => "PLUGIN",
        }
    }
}

/// A failed plugin call.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{}: {message}", kind.code())]
pub struct PluginError {
    /// Class.
    pub kind: PluginErrorKind,
    /// The plugin's own code for [`PluginErrorKind::Plugin`] (an API error code when it
    /// forwards a host failure, e.g. `PERMISSION_DENIED`); otherwise the kind's code.
    pub code: String,
    /// Message.
    pub message: String,
    /// Details from the plugin.
    pub details: Value,
}

impl PluginError {
    /// An error of `kind` with its default code.
    pub fn new(kind: PluginErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            code: kind.code().to_owned(),
            message: message.into(),
            details: Value::Null,
        }
    }

    fn abi(message: impl Into<String>) -> Self {
        Self::new(PluginErrorKind::Abi, message)
    }
}

/// An error returned to the plugin by a host function (same shape as an API error).
pub type HostError = WireError;

/// What host functions do; implemented by the app (permission checks included). Called on
/// the thread that called [`Runtime::call`].
pub trait HostCalls {
    /// Runs host function `func` (see [`crate::abi::host_fns`]).
    fn call(&mut self, func: &str, args: Value) -> Result<Value, HostError>;
}

/// A host offering nothing but `log` (pure contexts: card type engines, TTS synthesis).
#[derive(Debug, Default, Clone, Copy)]
pub struct PureHost;

impl HostCalls for PureHost {
    fn call(&mut self, func: &str, _args: Value) -> Result<Value, HostError> {
        if func == crate::abi::host_fns::LOG {
            return Ok(Value::Null);
        }
        Err(WireError {
            code: "PERMISSION_DENIED".into(),
            message: format!("host function {func} is not available in this context"),
            details: serde_json::json!({ "function": func }),
        })
    }
}

/// Most table elements one instance may hold, all tables together.
pub const MAX_TABLE_ELEMENTS: usize = 100_000;
/// Most tables per instance.
const MAX_TABLES: usize = 4;

/// Caps what one call may allocate. Totals are kept over *all* memories and tables of the
/// store, so splitting an allocation over several of them gains nothing.
#[derive(Debug)]
pub(crate) struct Limiter {
    pub(crate) memory_bytes: usize,
    /// Bytes of linear memory in use, all memories.
    pub(crate) memory_total: usize,
    /// Table elements in use, all tables.
    pub(crate) table_total: usize,
    pub(crate) hit: bool,
}

impl Limiter {
    pub(crate) fn new(memory_bytes: usize) -> Self {
        Self {
            memory_bytes,
            memory_total: 0,
            table_total: 0,
            hit: false,
        }
    }
}

impl ResourceLimiter for Limiter {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let total = self
            .memory_total
            .saturating_sub(current)
            .saturating_add(desired);
        if total > self.memory_bytes {
            self.hit = true;
            return Ok(false);
        }
        self.memory_total = total;
        Ok(true)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let total = self
            .table_total
            .saturating_sub(current)
            .saturating_add(desired);
        if total > MAX_TABLE_ELEMENTS {
            return Ok(false);
        }
        self.table_total = total;
        Ok(true)
    }

    fn instances(&self) -> usize {
        1
    }

    fn tables(&self) -> usize {
        MAX_TABLES
    }

    fn memories(&self) -> usize {
        1
    }
}

/// Store data of one call.
pub struct CallState {
    limiter: Limiter,
    to_host: Sender<Vec<u8>>,
    from_host: Receiver<Vec<u8>>,
    pending: Option<Vec<u8>>,
    budget: Duration,
    used: Duration,
    segment: Instant,
}

/// A compiled, linked plugin module (cheap to instantiate).
pub struct CompiledPlugin {
    pre: InstancePre<CallState>,
    sha256: String,
}

impl CompiledPlugin {
    /// SHA-256 of the WASM it was compiled from.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

impl std::fmt::Debug for CompiledPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledPlugin")
            .field("sha256", &self.sha256)
            .finish()
    }
}

struct Ticker {
    stop: Arc<AtomicBool>,
    active: Arc<AtomicUsize>,
    thread: thread::Thread,
}

impl Drop for Ticker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.unpark();
    }
}

/// The plugin runtime: one wasmtime engine, a module cache, the epoch ticker.
pub struct Runtime {
    engine: Engine,
    linker: Linker<CallState>,
    cache: Mutex<HashMap<String, Arc<CompiledPlugin>>>,
    ticker: OnceLock<Option<Ticker>>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime").finish_non_exhaustive()
    }
}

fn read_guest(caller: &mut Caller<'_, CallState>, ptr: i32, len: i32) -> wasmtime::Result<Vec<u8>> {
    let memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmtime::Error::msg("plugin exports no memory"))?;
    let (ptr, len) = (ptr as u32 as usize, len as u32 as usize);
    if len > MAX_MESSAGE_BYTES {
        return Err(wasmtime::Error::msg("host call message too large"));
    }
    memory
        .data(&caller)
        .get(ptr..ptr.saturating_add(len))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| wasmtime::Error::msg("host call message out of bounds"))
}

fn ticks(d: Duration) -> u64 {
    (d.as_micros() / TICK.as_micros()).max(1) as u64 + 1
}

fn host_call(mut caller: Caller<'_, CallState>, ptr: i32, len: i32) -> wasmtime::Result<i32> {
    let msg = read_guest(&mut caller, ptr, len)?;
    let state = caller.data_mut();
    state.used += state.segment.elapsed();
    state
        .to_host
        .send(msg)
        .map_err(|_| wasmtime::Error::msg("host went away"))?;
    let reply = state
        .from_host
        .recv()
        .map_err(|_| wasmtime::Error::msg("host went away"))?;
    let reply_len =
        i32::try_from(reply.len()).map_err(|_| wasmtime::Error::msg("reply too large"))?;
    state.pending = Some(reply);
    state.segment = Instant::now();
    // Time spent in the host does not count against the plugin.
    let remaining = state.budget.saturating_sub(state.used);
    caller.as_context_mut().set_epoch_deadline(ticks(remaining));
    Ok(reply_len)
}

fn host_reply(mut caller: Caller<'_, CallState>, ptr: i32) -> wasmtime::Result<()> {
    let reply = caller
        .data_mut()
        .pending
        .take()
        .ok_or_else(|| wasmtime::Error::msg("no host reply waiting"))?;
    let memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmtime::Error::msg("plugin exports no memory"))?;
    memory
        .write(&mut caller, ptr as u32 as usize, &reply)
        .map_err(|_| wasmtime::Error::msg("host reply out of bounds"))
}

fn map_error(e: wasmtime::Error, memory_hit: bool) -> PluginError {
    let trap = e.downcast_ref::<Trap>().copied();
    match trap {
        Some(Trap::OutOfFuel) => PluginError::new(PluginErrorKind::OutOfFuel, "fuel exhausted"),
        Some(Trap::Interrupt) => PluginError::new(PluginErrorKind::Timeout, "deadline passed"),
        _ if memory_hit => PluginError::new(
            PluginErrorKind::MemoryLimit,
            format!("memory limit reached ({e})"),
        ),
        _ => PluginError::new(PluginErrorKind::Trap, format!("{e:#}")),
    }
}

impl Runtime {
    /// A runtime with fuel metering and epoch interruption enabled.
    pub fn new() -> Result<Self, PluginError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.max_wasm_stack(1024 * 1024);
        // Only what plugins need: one memory, no shared memory, no 64-bit memory (the memory
        // cap and the host's 32-bit pointers rely on it), nothing that changes control flow
        // or allocation outside the limiter.
        config.wasm_features(
            WasmFeatures::MULTI_MEMORY
                | WasmFeatures::THREADS
                | WasmFeatures::SHARED_EVERYTHING_THREADS
                | WasmFeatures::MEMORY64
                | WasmFeatures::CUSTOM_PAGE_SIZES
                | WasmFeatures::COMPONENT_MODEL
                | WasmFeatures::GC
                | WasmFeatures::FUNCTION_REFERENCES
                | WasmFeatures::EXCEPTIONS
                | WasmFeatures::LEGACY_EXCEPTIONS
                | WasmFeatures::STACK_SWITCHING
                | WasmFeatures::WIDE_ARITHMETIC
                | WasmFeatures::RELAXED_SIMD,
            false,
        );
        let engine = Engine::new(&config).map_err(|e| PluginError::abi(format!("engine: {e}")))?;
        let mut linker = Linker::new(&engine);
        linker
            .func_wrap(IMPORT_MODULE, "host_call", host_call)
            .and_then(|l| l.func_wrap(IMPORT_MODULE, "host_reply", host_reply))
            .map_err(|e| PluginError::abi(format!("linker: {e}")))?;
        Ok(Self {
            engine,
            linker,
            cache: Mutex::new(HashMap::new()),
            ticker: OnceLock::new(),
        })
    }

    /// Compiles (or returns the cached) module for `wasm` (binary or text format).
    /// Imports other than the `fc` host functions are rejected here.
    pub fn compile(&self, wasm: &[u8]) -> Result<Arc<CompiledPlugin>, PluginError> {
        let sha256 = crate::package::sha256_hex(wasm);
        if let Some(hit) = self.cache.lock().ok().and_then(|c| c.get(&sha256).cloned()) {
            return Ok(hit);
        }
        let module = Module::new(&self.engine, wasm)
            .map_err(|e| PluginError::abi(format!("invalid module: {e:#}")))?;
        for export in ["memory", "fc_abi_version", "fc_alloc", "fc_handle"] {
            if module.get_export(export).is_none() {
                return Err(PluginError::abi(format!("missing export {export}")));
            }
        }
        let pre = self
            .linker
            .instantiate_pre(&module)
            .map_err(|e| PluginError::abi(format!("{e:#}")))?;
        let compiled = Arc::new(CompiledPlugin {
            pre,
            sha256: sha256.clone(),
        });
        if let Ok(mut c) = self.cache.lock() {
            c.insert(sha256, compiled.clone());
        }
        Ok(compiled)
    }

    /// Forgets cached modules not in `keep` (after plugins were removed or updated).
    pub fn retain(&self, keep: &[&str]) {
        if let Ok(mut c) = self.cache.lock() {
            c.retain(|k, _| keep.contains(&k.as_str()));
        }
    }

    fn ticker(&self) -> Option<&Ticker> {
        self.ticker
            .get_or_init(|| {
                let stop = Arc::new(AtomicBool::new(false));
                let active = Arc::new(AtomicUsize::new(0));
                let engine = self.engine.clone();
                let (s, a) = (stop.clone(), active.clone());
                let handle = thread::Builder::new()
                    .name("fc-plugin-epoch".into())
                    .spawn(move || {
                        while !s.load(Ordering::SeqCst) {
                            if a.load(Ordering::SeqCst) == 0 {
                                thread::park_timeout(Duration::from_millis(250));
                                continue;
                            }
                            thread::sleep(TICK);
                            engine.increment_epoch();
                        }
                    })
                    .ok()?;
                Some(Ticker {
                    stop,
                    active,
                    thread: handle.thread().clone(),
                })
            })
            .as_ref()
    }

    /// Runs operation `op` with `input` in a fresh instance of `plugin`, servicing its host
    /// calls with `host` on this thread.
    pub fn call(
        &self,
        plugin: &CompiledPlugin,
        op: &str,
        input: Value,
        limits: &Limits,
        host: &mut dyn HostCalls,
    ) -> Result<Value, PluginError> {
        let request = serde_json::to_vec(&OpRequest {
            op: op.to_owned(),
            input,
        })
        .map_err(|e| PluginError::abi(e.to_string()))?;
        let ticker = self
            .ticker()
            .ok_or_else(|| PluginError::abi("cannot start the epoch ticker"))?;
        ticker.active.fetch_add(1, Ordering::SeqCst);
        ticker.thread.unpark();
        let (to_host, host_rx) = channel::<Vec<u8>>();
        let (host_tx, from_host) = channel::<Vec<u8>>();
        let engine = &self.engine;
        let result = thread::scope(|s| {
            let worker = thread::Builder::new()
                .name("fc-plugin".into())
                .stack_size(8 * 1024 * 1024)
                .spawn_scoped(s, move || {
                    run(engine, plugin, limits, request, to_host, from_host)
                })
                .map_err(|e| PluginError::abi(format!("cannot start plugin thread: {e}")))?;
            while let Ok(msg) = host_rx.recv() {
                if host_tx.send(service(host, &msg)).is_err() {
                    break;
                }
            }
            worker.join().unwrap_or_else(|_| {
                Err(PluginError::new(
                    PluginErrorKind::Trap,
                    "plugin thread failed",
                ))
            })
        });
        ticker.active.fetch_sub(1, Ordering::SeqCst);
        let out = result?;
        let reply: OpReply = serde_json::from_slice(&out)
            .map_err(|e| PluginError::abi(format!("reply is not an OpReply: {e}")))?;
        match (reply.ok, reply.err) {
            (_, Some(err)) => Err(PluginError {
                kind: PluginErrorKind::Plugin,
                code: err.code,
                message: err.message,
                details: err.details,
            }),
            (Some(v), None) => Ok(v),
            (None, None) => Ok(Value::Null),
        }
    }
}

fn service(host: &mut dyn HostCalls, msg: &[u8]) -> Vec<u8> {
    let reply = match serde_json::from_slice::<HostRequest>(msg) {
        Ok(req) => match host.call(&req.func, req.args) {
            Ok(v) => HostReply {
                ok: Some(v),
                err: None,
            },
            Err(e) => HostReply {
                ok: None,
                err: Some(e),
            },
        },
        Err(e) => HostReply {
            ok: None,
            err: Some(WireError {
                code: "VALIDATION_INVALID_PARAMS".into(),
                message: format!("host call is not a HostRequest: {e}"),
                details: Value::Null,
            }),
        },
    };
    serde_json::to_vec(&reply)
        .unwrap_or_else(|_| br#"{"err":{"code":"INTERNAL","message":"serialize"}}"#.to_vec())
}

fn run(
    engine: &Engine,
    plugin: &CompiledPlugin,
    limits: &Limits,
    input: Vec<u8>,
    to_host: Sender<Vec<u8>>,
    from_host: Receiver<Vec<u8>>,
) -> Result<Vec<u8>, PluginError> {
    let mut store = Store::new(
        engine,
        CallState {
            limiter: Limiter::new(limits.memory_bytes),
            to_host,
            from_host,
            pending: None,
            budget: limits.timeout,
            used: Duration::ZERO,
            segment: Instant::now(),
        },
    );
    store.limiter(|s| &mut s.limiter);
    store
        .set_fuel(limits.fuel)
        .map_err(|e| PluginError::abi(e.to_string()))?;
    store.set_epoch_deadline(ticks(limits.timeout));
    store.epoch_deadline_trap();
    macro_rules! wasm {
        ($e:expr) => {
            match $e {
                Ok(v) => v,
                Err(e) => return Err(map_error(e, store.data().limiter.hit)),
            }
        };
    }
    let instance = wasm!(plugin.pre.instantiate(&mut store));
    let typed = |store: &mut Store<CallState>, name: &str| {
        instance
            .get_export(&mut *store, name)
            .and_then(|e| e.into_func())
            .ok_or_else(|| PluginError::abi(format!("missing export {name}")))
    };
    let version = typed(&mut store, "fc_abi_version")?
        .typed::<(), i32>(&store)
        .map_err(|e| PluginError::abi(format!("fc_abi_version: {e}")))?;
    let alloc = typed(&mut store, "fc_alloc")?
        .typed::<i32, i32>(&store)
        .map_err(|e| PluginError::abi(format!("fc_alloc: {e}")))?;
    let handle = typed(&mut store, "fc_handle")?
        .typed::<(i32, i32), i64>(&store)
        .map_err(|e| PluginError::abi(format!("fc_handle: {e}")))?;
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| PluginError::abi("missing export memory"))?;
    let abi = wasm!(version.call(&mut store, ()));
    if !SUPPORTED_ABI.contains(&abi) {
        return Err(PluginError::abi(format!(
            "plugin ABI {abi} is not supported"
        )));
    }
    let len = i32::try_from(input.len()).map_err(|_| PluginError::abi("input too large"))?;
    let ptr = wasm!(alloc.call(&mut store, len));
    memory
        .write(&mut store, ptr as u32 as usize, &input)
        .map_err(|_| PluginError::abi("fc_alloc returned an out-of-bounds buffer"))?;
    store.data_mut().segment = Instant::now();
    let packed = wasm!(handle.call(&mut store, (ptr, len))) as u64;
    let (out_ptr, out_len) = ((packed >> 32) as usize, (packed & 0xffff_ffff) as usize);
    if out_len > MAX_MESSAGE_BYTES {
        return Err(PluginError::abi("reply too large"));
    }
    memory
        .data(&store)
        .get(out_ptr..out_ptr.saturating_add(out_len))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| PluginError::abi("reply out of bounds"))
}

#[cfg(test)]
mod tests;
