//! The plugin ABI, **version 1**. Frozen: later app versions keep accepting v1 plugins
//! (new operations and host functions are additive; a breaking change is ABI 2, served next
//! to 1). The contract test `abi_v1_contract` runs a hand-written v1 module against the host.
//!
//! ## Exports (plugin → host)
//!
//! | Export | Signature | Meaning |
//! |---|---|---|
//! | `memory` | memory | Linear memory used for all exchanges |
//! | `fc_abi_version` | `() -> i32` | Must return an ABI the host supports ([`SUPPORTED_ABI`]) |
//! | `fc_alloc` | `(len: i32) -> i32` | Returns a buffer of `len` bytes the host may write |
//! | `fc_handle` | `(ptr: i32, len: i32) -> i64` | Handles one operation: input = JSON [`OpRequest`] at `ptr..ptr+len`; returns `(out_ptr << 32) \| out_len` of a JSON [`OpReply`] |
//!
//! ## Imports (host → plugin), module `fc`
//!
//! | Import | Signature | Meaning |
//! |---|---|---|
//! | `host_call` | `(ptr: i32, len: i32) -> i32` | Calls the host with a JSON [`HostRequest`]; returns the byte length of the JSON [`HostReply`] waiting to be fetched |
//! | `host_reply` | `(ptr: i32)` | Copies the waiting reply to `ptr` (a buffer of the returned length) |
//!
//! Every call gets a fresh instance: plugins keep state in `kv`, or in the `state` value of a
//! UI session.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// ABI versions this host runs.
pub const SUPPORTED_ABI: &[i32] = &[1];

/// Manifest `api_version`s this host accepts (the plugin API is the ABI plus the `v1` method
/// catalog; both are versioned together).
pub const SUPPORTED_API_VERSIONS: &[&str] = &["1"];

/// Import module name.
pub const IMPORT_MODULE: &str = "fc";

/// Largest JSON message accepted in either direction.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

/// Operations the host asks a plugin to perform (`OpRequest::op`). Additive.
pub mod ops {
    /// `before:<method>` hook: `{hook, method, params}` → [`super::BeforeHookReply`].
    pub const HOOK_BEFORE: &str = "hook.before";
    /// `after:<method>` hook: `{hook, method, params, response}` → anything (ignored).
    pub const HOOK_AFTER: &str = "hook.after";
    /// `event:<type>` hook: `{hook, event}` → anything (ignored).
    pub const HOOK_EVENT: &str = "hook.event";
    /// Contributed command: `{command, params}` → result JSON.
    pub const COMMAND: &str = "command";
    /// Card type: `{card_type, note, notetype, template}` → `[CardSpec]`.
    pub const CARD_TYPE_GENERATE: &str = "card_type.generate";
    /// Card type: `{card_type, note, notetype, template, template_key, ctx}` → `RenderedCard`.
    pub const CARD_TYPE_RENDER: &str = "card_type.render";
    /// Card type: `{card_type, rendered, response}` → `Evaluation` or `null` (= default).
    pub const CARD_TYPE_EVALUATE: &str = "card_type.evaluate";
    /// TTS provider: `{provider}` → `[Voice]`.
    pub const TTS_VOICES: &str = "tts.voices";
    /// TTS provider: `{provider, request: SpeechRequest}` → `{mime, data_b64}`.
    pub const TTS_SYNTHESIZE: &str = "tts.synthesize";
    /// Declarative UI (views and study modes): `{view, session, input}` → `UiReply`.
    pub const UI_OPEN: &str = "ui.open";
    /// Declarative UI: `{view, session, action, values, state}` → `UiReply`.
    pub const UI_ACT: &str = "ui.act";
}

/// Host functions a plugin may call (`HostRequest::fn`), with the permission each needs.
/// Additive.
pub mod host_fns {
    /// `{method, params}` → response. Needs the method's own permission (`decks:write`…).
    pub const INVOKE: &str = "invoke";
    /// `{key}` → value or `null`. Plugin-private storage, no permission.
    pub const KV_GET: &str = "kv.get";
    /// `{key, value}` → `null`.
    pub const KV_SET: &str = "kv.set";
    /// `{key}` → `null`.
    pub const KV_DELETE: &str = "kv.delete";
    /// `{prefix}` → `[[key, value]]`.
    pub const KV_LIST: &str = "kv.list";
    /// `{}` → `{key: value}` of the plugin's own settings (defaults applied).
    pub const SETTINGS_GET: &str = "settings.get";
    /// `{level, message}` → `null`. The plugin's own log channel.
    pub const LOG: &str = "log";
    /// `{message, level}` → `null`. A notice shown to the user.
    pub const UI_NOTIFY: &str = "ui.notify";
    /// `{provider?, text, language, voice?, rate?}` → `{mime, data_b64}`. Sugar for `invoke v1.tts.synthesize`: needs `tts:read`.
    pub const TTS_SYNTHESIZE: &str = "tts.synthesize";
    /// `{provider?}` → voices. Sugar for `invoke v1.tts.voices`: needs `tts:read`.
    pub const TTS_VOICES: &str = "tts.voices";
    /// `{url, method?, headers?, body?}`. Needs `network:<host>`.
    pub const HTTP_FETCH: &str = "http.fetch";
    /// `{model, messages…}`. Needs `ai:invoke`.
    pub const AI_COMPLETE: &str = "ai.complete";
    /// `{}` → model catalog entries with status. Needs `models:manage`.
    pub const MODELS_LIST: &str = "models.list";

    /// Every v1 host function.
    pub const ALL: &[&str] = &[
        INVOKE,
        KV_GET,
        KV_SET,
        KV_DELETE,
        KV_LIST,
        SETTINGS_GET,
        LOG,
        UI_NOTIFY,
        TTS_SYNTHESIZE,
        TTS_VOICES,
        HTTP_FETCH,
        AI_COMPLETE,
        MODELS_LIST,
    ];
}

/// Input of `fc_handle`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpRequest {
    /// Operation ([`ops`]).
    pub op: String,
    /// Operation input.
    pub input: Value,
}

/// Output of `fc_handle`: exactly one of `ok` / `err`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpReply {
    /// Result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<Value>,
    /// Failure reported by the plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<WireError>,
}

/// An error crossing the boundary (API errors keep their `code`, e.g. `PERMISSION_DENIED`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireError {
    /// Stable code (`PERMISSION_DENIED`, `UNSUPPORTED_OP`, a plugin's own code…).
    pub code: String,
    /// Developer-facing message.
    #[serde(default)]
    pub message: String,
    /// Structured details.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

/// Input of `host_call`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostRequest {
    /// Host function ([`host_fns`]).
    #[serde(rename = "fn")]
    pub func: String,
    /// Arguments.
    #[serde(default)]
    pub args: Value,
}

/// Reply of `host_call`: exactly one of `ok` / `err`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostReply {
    /// Result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<Value>,
    /// Failure (permission denied, validation…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<WireError>,
}

/// What a `before:` hook decides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BeforeHookReply {
    /// Go on, optionally with changed params (re-validated by the method).
    Continue {
        /// Replacement params (`None` = unchanged).
        #[serde(default)]
        params: Option<Value>,
    },
    /// Refuse the call.
    Reject {
        /// Shown to the user.
        #[serde(default)]
        message: String,
    },
}
