//! What one host function may cost the host (docs/05 §12, "Host fonksiyonu sınırları").
//!
//! Every host function has a row in [`HOST_FNS`]: the largest request it accepts, the
//! largest reply it may send back, how often it may run per plugin call and per minute, and
//! the fuel it costs. The runtime enforces the per-call parts for every host
//! ([`crate::runtime::Runtime::call`]); the app enforces the per-minute window per plugin
//! with [`RateWindows`] (it knows which plugin is calling).
//!
//! On top of these, each plugin call has three budgets of its own ([`crate::Limits`]): host
//! calls in total (`max_host_calls`), bytes crossing the boundary in both directions
//! (`host_bytes`) and wall-clock time *including* host calls (`timeout`).
//!
//! A request over its size, or a call over its per-call / per-minute count, gets the error
//! [`HOST_LIMIT_CODE`] back (the plugin may go on); a call over one of the call budgets is
//! stopped (a fault, `HOST_LIMIT` / `TIMEOUT` / `OUT_OF_FUEL`).

use crate::abi::{host_fns as f, WireError, MAX_MESSAGE_BYTES};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const KIB: usize = 1024;
const MIB: usize = 1024 * 1024;

/// Wire code of a refused host call (same string as `fc_api::ErrorCode::ValidationPluginHostLimit`).
pub const HOST_LIMIT_CODE: &str = "VALIDATION_PLUGIN_HOST_LIMIT";

/// Fuel every host call costs on top of its own [`HostFnSpec::fuel`].
pub const HOST_CALL_FUEL: u64 = 50_000;

/// Fuel per byte crossing the boundary (request + reply).
pub const FUEL_PER_BYTE: u64 = 4;

/// Length of the rate window of [`HostFnSpec::max_per_minute`].
pub const RATE_WINDOW: Duration = Duration::from_secs(60);

/// Largest serialized value a plugin may return from one operation (`OpReply`), by
/// operation; checked before the reply is parsed.
pub fn max_op_reply_bytes(op: &str) -> usize {
    match op {
        crate::abi::ops::TTS_SYNTHESIZE => MAX_MESSAGE_BYTES,
        _ => 4 * MIB,
    }
}

/// Limits of one host function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostFnSpec {
    /// Name ([`crate::abi::host_fns`]).
    pub name: &'static str,
    /// Permission it needs (for the docs table; the app checks it).
    pub permission: &'static str,
    /// Largest request (the whole `{fn, args}` JSON), bytes.
    pub max_request_bytes: usize,
    /// Largest reply (the whole `{ok}` / `{err}` JSON), bytes.
    pub max_reply_bytes: usize,
    /// Most calls in one plugin call.
    pub max_per_call: u32,
    /// Most calls per plugin per [`RATE_WINDOW`] (0 = only the per-call limit).
    pub max_per_minute: u32,
    /// Fuel it costs (on top of [`HOST_CALL_FUEL`] and [`FUEL_PER_BYTE`]).
    pub fuel: u64,
}

const fn spec(
    name: &'static str,
    permission: &'static str,
    max_request_bytes: usize,
    max_reply_bytes: usize,
    max_per_call: u32,
    max_per_minute: u32,
    fuel: u64,
) -> HostFnSpec {
    HostFnSpec {
        name,
        permission,
        max_request_bytes,
        max_reply_bytes,
        max_per_call,
        max_per_minute,
        fuel,
    }
}

/// Every v1 host function with its limits (one row each; see the docs/05 table).
pub const HOST_FNS: &[HostFnSpec] = &[
    spec(
        f::INVOKE,
        "<method's own>",
        MIB,
        MAX_MESSAGE_BYTES,
        1_000,
        0,
        1_000_000,
    ),
    spec(f::KV_GET, "-", KIB, 2 * MIB, 10_000, 0, 200_000),
    spec(f::KV_SET, "storage:kv", MIB + KIB, KIB, 1_000, 0, 1_000_000),
    spec(f::KV_DELETE, "-", KIB, KIB, 1_000, 0, 500_000),
    spec(
        f::KV_LIST,
        "-",
        KIB,
        KV_LIST_MAX_BYTES + 64 * KIB,
        100,
        0,
        1_000_000,
    ),
    spec(f::SETTINGS_GET, "-", KIB, MIB, 100, 0, 200_000),
    spec(f::SETTINGS_SET, "-", 64 * KIB, MIB, 100, 0, 1_000_000),
    spec(f::FS_OPEN, "fs:pick", 4 * KIB, 12 * MIB, 2, 10, 1_000_000),
    spec(f::FS_SAVE, "fs:pick", 12 * MIB, 4 * KIB, 2, 10, 1_000_000),
    spec(f::LOG, "-", 8 * KIB, KIB, 100, 600, 50_000),
    spec(f::UI_NOTIFY, "ui:notify", 4 * KIB, KIB, 3, 10, 500_000),
    spec(
        f::TTS_SYNTHESIZE,
        "tts:read",
        64 * KIB,
        MAX_MESSAGE_BYTES,
        50,
        0,
        1_000_000,
    ),
    spec(f::TTS_VOICES, "tts:read", KIB, MIB, 10, 0, 200_000),
    spec(
        f::HTTP_FETCH,
        "network:<host>",
        MIB,
        MAX_MESSAGE_BYTES,
        50,
        120,
        1_000_000,
    ),
    spec(f::AI_COMPLETE, "ai:invoke", MIB, MIB, 10, 60, 1_000_000),
    spec(
        f::MODELS_LIST,
        "models:manage",
        KIB,
        64 * KIB,
        10,
        0,
        200_000,
    ),
];

/// Most entries one `kv.list` page returns.
pub const KV_LIST_MAX_ENTRIES: usize = 1_000;
/// Most bytes (keys + JSON values) one `kv.list` page returns.
pub const KV_LIST_MAX_BYTES: usize = 4 * MIB;

/// Limits of an unknown host function name (refused by the host anyway).
const UNKNOWN: HostFnSpec = spec("?", "-", KIB, KIB, 100, 0, HOST_CALL_FUEL);

/// Limits of host function `name` (an unknown name gets tight defaults).
pub fn host_fn(name: &str) -> &'static HostFnSpec {
    HOST_FNS.iter().find(|s| s.name == name).unwrap_or(&UNKNOWN)
}

/// The error a refused host call gets back: `limit` is `request_bytes`, `reply_bytes`,
/// `per_call` or `per_minute`; `max` the limit's value.
pub fn limit_error(func: &str, limit: &str, max: u64) -> WireError {
    WireError {
        code: HOST_LIMIT_CODE.into(),
        message: format!("{func}: host limit {limit} = {max} exceeded"),
        details: json!({ "function": func, "limit": limit, "max": max }),
    }
}

/// Per-minute counters of one plugin, all host functions ([`HostFnSpec::max_per_minute`]).
/// Memory is bounded: a function keeps at most its own limit of timestamps.
#[derive(Debug, Default)]
pub struct RateWindows {
    hits: HashMap<&'static str, VecDeque<Instant>>,
}

impl RateWindows {
    /// Counts one call of `func` at `now`; `Err` (to hand back to the plugin) when it is over
    /// its per-minute limit (then it is not counted).
    pub fn admit(&mut self, func: &str, now: Instant) -> Result<(), WireError> {
        let spec = host_fn(func);
        if spec.max_per_minute == 0 {
            return Ok(());
        }
        let q = self.hits.entry(spec.name).or_default();
        while q
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= RATE_WINDOW)
        {
            q.pop_front();
        }
        if q.len() >= spec.max_per_minute as usize {
            return Err(limit_error(
                func,
                "per_minute",
                u64::from(spec.max_per_minute),
            ));
        }
        q.push_back(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_host_function_has_limits() {
        for name in crate::abi::host_fns::ALL {
            let s = host_fn(name);
            assert_eq!(s.name, *name, "{name} has no row in HOST_FNS");
            assert!(s.max_request_bytes <= MAX_MESSAGE_BYTES, "{name}");
            assert!(s.max_reply_bytes <= MAX_MESSAGE_BYTES, "{name}");
            assert!(s.max_per_call > 0 && s.fuel > 0, "{name}");
        }
        assert_eq!(HOST_FNS.len(), crate::abi::host_fns::ALL.len());
        assert_eq!(host_fn("nope").name, "?");
    }

    #[test]
    fn rate_windows_slide() {
        let mut w = RateWindows::default();
        let t0 = Instant::now();
        for _ in 0..10 {
            w.admit(f::UI_NOTIFY, t0).unwrap();
        }
        let e = w.admit(f::UI_NOTIFY, t0).unwrap_err();
        assert_eq!(e.code, HOST_LIMIT_CODE);
        assert_eq!(e.details["limit"], "per_minute");
        // Unlimited per minute.
        for _ in 0..1000 {
            w.admit(f::KV_GET, t0).unwrap();
        }
        // The window moves on.
        w.admit(f::UI_NOTIFY, t0 + RATE_WINDOW).unwrap();
    }

    /// The docs/05 table lists every host function with its limits.
    #[test]
    fn docs_table_lists_every_host_function() {
        let docs = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/05-eklenti-sistemi.md"),
        )
        .unwrap();
        for s in HOST_FNS {
            let row = docs
                .lines()
                .find(|l| l.starts_with(&format!("| `{}` |", s.name)))
                .unwrap_or_else(|| panic!("docs/05 has no row for {}", s.name));
            assert!(
                row.contains(&format!("`{}`", s.permission)) || s.permission == "-",
                "{row}"
            );
            assert!(row.contains(&format!("{} /", s.max_per_call)), "{row}");
        }
    }
}
