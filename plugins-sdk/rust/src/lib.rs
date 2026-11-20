//! # fc-plugin-sdk
//!
//! Write Flashcard plugins in Rust, compiled to `wasm32-unknown-unknown`.
//!
//! ```ignore
//! use fc_plugin_sdk::prelude::*;
//!
//! #[derive(Default)]
//! struct AutoFill;
//!
//! impl Plugin for AutoFill {
//!     fn before_hook(&self, _hook: &str, _method: &str, mut params: Value) -> Result<BeforeHook> {
//!         params["fields"]["back"] = json!("…");
//!         Ok(BeforeHook::Continue(Some(params)))
//!     }
//! }
//!
//! export_plugin!(AutoFill);
//! ```
//!
//! * [`Plugin`]: one method per operation the host may ask for (hooks, commands, card types,
//!   TTS, declarative UI); unimplemented ones answer `UNSUPPORTED_OP`.
//! * [`host`]: host functions — typed API calls ([`host::call`]), private storage
//!   ([`host::kv`]), settings, log, notices, TTS. Every call is permission-checked by the
//!   host; a missing permission comes back as an [`Error`] with code `PERMISSION_DENIED`.
//! * [`ui`]: builders for the declarative UI.
//! * [`testing`]: a fake host for native unit tests (`cargo test` on the host target).
//!
//! The ABI (v1) is described in `fc_plugins::abi`. Each call runs in a fresh instance, so
//! keep state in `kv` or in a UI session's `state`.

pub use fc_api as api;
pub use fc_api::v1::plugins::{UiNode, UiOption, UiReply, UiText};
pub use fc_core as core;
pub use fc_providers::speech::{AudioClip, SpeechRequest, Voice};
pub use fc_providers::strategy::{
    CardResponse, Evaluation, Interaction, RenderContext, RenderedCard, TtsRequest,
};
pub use serde_json::{json, Value};

use fc_core::card::CardSpec;
use fc_core::note::Note;
use fc_core::notetype::{CardTemplate, NoteType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// ABI version this SDK speaks.
pub const ABI_VERSION: i32 = 1;

/// Everything a plugin usually needs.
pub mod prelude {
    pub use crate::{
        export_plugin, host, json, ui, BeforeHook, CardTypeInput, Error, Plugin, Result, UiNode,
        UiReply, UiText, Value,
    };
}

/// An error crossing the boundary (same shape as an API error).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Error {
    /// Stable code (`PERMISSION_DENIED`, `UNSUPPORTED_OP`, your own…).
    pub code: String,
    /// Developer-facing message.
    #[serde(default)]
    pub message: String,
    /// Details.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

impl Error {
    /// An error with `code` and `message`.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: Value::Null,
        }
    }

    /// `UNSUPPORTED_OP`.
    pub fn unsupported(what: &str) -> Self {
        Self::new(
            "UNSUPPORTED_OP",
            format!("{what} is not implemented by this plugin"),
        )
    }

    fn json(e: serde_json::Error) -> Self {
        Self::new("INVALID_JSON", e.to_string())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// Result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Decision of a `before:` hook.
#[derive(Debug, Clone, PartialEq)]
pub enum BeforeHook {
    /// Continue, optionally with replaced params (the method re-validates them).
    Continue(Option<Value>),
    /// Reject the call with a message shown to the user.
    Reject(String),
}

/// Input of card type operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardTypeInput {
    /// The note.
    pub note: Note,
    /// Its note type.
    pub notetype: NoteType,
    /// The template.
    pub template: CardTemplate,
    /// The card's key (render only).
    #[serde(default)]
    pub template_key: String,
}

impl CardTypeInput {
    /// Value of field `key` (empty if missing).
    pub fn field(&self, key: &str) -> &str {
        self.note
            .fields
            .iter()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }
}

/// A plugin. Implement the operations you contributed; [`export_plugin!`] wires it up.
#[allow(unused_variables)]
pub trait Plugin: Default {
    /// `hooks:before:<method>`.
    fn before_hook(&self, hook: &str, method: &str, params: Value) -> Result<BeforeHook> {
        Ok(BeforeHook::Continue(None))
    }
    /// `hooks:after:<method>` (after commit).
    fn after_hook(&self, hook: &str, method: &str, params: Value, response: Value) -> Result<()> {
        Ok(())
    }
    /// `hooks:event:<type>` (after commit).
    fn on_event(&self, hook: &str, event: Value) -> Result<()> {
        Ok(())
    }
    /// A contributed command.
    fn command(&self, command: &str, params: Value) -> Result<Value> {
        Err(Error::unsupported(command))
    }
    /// Card type: which cards a note yields.
    fn generate_cards(&self, card_type: &str, input: CardTypeInput) -> Result<Vec<CardSpec>> {
        Err(Error::unsupported(card_type))
    }
    /// Card type: renders a card (the host sanitizes the HTML).
    fn render_card(
        &self,
        card_type: &str,
        input: CardTypeInput,
        ctx: RenderContext,
    ) -> Result<RenderedCard> {
        Err(Error::unsupported(card_type))
    }
    /// Card type: judges a response (`None` = the host's default for the interaction).
    fn evaluate(
        &self,
        card_type: &str,
        rendered: RenderedCard,
        response: CardResponse,
    ) -> Result<Option<Evaluation>> {
        Ok(None)
    }
    /// TTS provider: voices.
    fn tts_voices(&self, provider: &str) -> Result<Vec<Voice>> {
        Err(Error::unsupported(provider))
    }
    /// TTS provider: speech.
    fn tts_synthesize(&self, provider: &str, request: SpeechRequest) -> Result<AudioClip> {
        Err(Error::unsupported(provider))
    }
    /// Declarative UI: a view or study mode opens.
    fn ui_open(&self, view: &str, session: &str, input: Value) -> Result<UiReply> {
        Err(Error::unsupported(view))
    }
    /// Declarative UI: the user acted (`values` = input values by node id).
    fn ui_act(
        &self,
        view: &str,
        session: &str,
        action: &str,
        values: BTreeMap<String, Value>,
        state: Value,
    ) -> Result<UiReply> {
        Err(Error::unsupported(view))
    }
}

fn input<T: serde::de::DeserializeOwned>(v: Value) -> Result<T> {
    serde_json::from_value(v).map_err(Error::json)
}

fn output<T: Serialize>(v: &T) -> Result<Value> {
    serde_json::to_value(v).map_err(Error::json)
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Runs operation `op` on a fresh `P` (what `fc_handle` does; also usable in native tests).
pub fn dispatch<P: Plugin>(op: &str, mut inp: Value) -> Result<Value> {
    let p = P::default();
    let take = |v: &mut Value, k: &str| v.get_mut(k).map(Value::take).unwrap_or(Value::Null);
    match op {
        "hook.before" => {
            let params = take(&mut inp, "params");
            match p.before_hook(&s(&inp, "hook"), &s(&inp, "method"), params)? {
                BeforeHook::Continue(params) => {
                    Ok(json!({ "action": "continue", "params": params }))
                }
                BeforeHook::Reject(message) => {
                    Ok(json!({ "action": "reject", "message": message }))
                }
            }
        }
        "hook.after" => {
            let (params, response) = (take(&mut inp, "params"), take(&mut inp, "response"));
            p.after_hook(&s(&inp, "hook"), &s(&inp, "method"), params, response)
                .map(|()| Value::Null)
        }
        "hook.event" => {
            let event = take(&mut inp, "event");
            p.on_event(&s(&inp, "hook"), event).map(|()| Value::Null)
        }
        "command" => {
            let params = take(&mut inp, "params");
            p.command(&s(&inp, "command"), params)
        }
        "card_type.generate" => {
            let ct = s(&inp, "card_type");
            output(&p.generate_cards(&ct, input(inp)?)?)
        }
        "card_type.render" => {
            let ct = s(&inp, "card_type");
            let ctx: RenderContext = input(take(&mut inp, "ctx"))?;
            output(&p.render_card(&ct, input(inp)?, ctx)?)
        }
        "card_type.evaluate" => {
            let ct = s(&inp, "card_type");
            let rendered = input(take(&mut inp, "rendered"))?;
            let response = input(take(&mut inp, "response"))?;
            output(&p.evaluate(&ct, rendered, response)?)
        }
        "tts.voices" => output(&p.tts_voices(&s(&inp, "provider"))?),
        "tts.synthesize" => {
            use base64::Engine as _;
            let request = input(take(&mut inp, "request"))?;
            let clip = p.tts_synthesize(&s(&inp, "provider"), request)?;
            Ok(json!({
                "mime": clip.mime,
                "data_b64": base64::engine::general_purpose::STANDARD.encode(&clip.bytes),
            }))
        }
        "ui.open" => {
            let i = take(&mut inp, "input");
            output(&p.ui_open(&s(&inp, "view"), &s(&inp, "session"), i)?)
        }
        "ui.act" => {
            let values: BTreeMap<String, Value> =
                input(take(&mut inp, "values")).unwrap_or_default();
            let state = take(&mut inp, "state");
            output(&p.ui_act(
                &s(&inp, "view"),
                &s(&inp, "session"),
                &s(&inp, "action"),
                values,
                state,
            )?)
        }
        other => Err(Error::unsupported(other)),
    }
}

/// Exports the ABI functions for plugin type `$ty` (`Default + Plugin`).
#[macro_export]
macro_rules! export_plugin {
    ($ty:ty) => {
        #[no_mangle]
        pub extern "C" fn fc_abi_version() -> i32 {
            $crate::ABI_VERSION
        }

        #[no_mangle]
        pub extern "C" fn fc_alloc(len: i32) -> i32 {
            $crate::__abi::alloc(len)
        }

        #[no_mangle]
        pub extern "C" fn fc_handle(ptr: i32, len: i32) -> i64 {
            $crate::__abi::handle(ptr, len, $crate::dispatch::<$ty>)
        }
    };
}

#[doc(hidden)]
pub mod __abi {
    use super::*;

    /// A buffer the host fills; never freed (the instance is discarded after the call).
    pub fn alloc(len: i32) -> i32 {
        let mut buf = Vec::<u8>::with_capacity(len.max(0) as usize);
        let ptr = buf.as_mut_ptr();
        std::mem::forget(buf);
        ptr as usize as i32
    }

    fn leak(bytes: Vec<u8>) -> i64 {
        let len = bytes.len() as u64;
        let ptr = Box::into_raw(bytes.into_boxed_slice()) as *mut u8 as usize as u64;
        ((ptr << 32) | len) as i64
    }

    /// `fc_handle`: parses the request, dispatches, writes the reply.
    pub fn handle(ptr: i32, len: i32, dispatch: fn(&str, Value) -> Result<Value>) -> i64 {
        // SAFETY: `ptr` was returned by `alloc(len)` and the host wrote `len` bytes into it.
        let bytes =
            unsafe { Vec::from_raw_parts(ptr as usize as *mut u8, len as usize, len as usize) };
        let reply = match serde_json::from_slice::<Value>(&bytes) {
            Ok(mut req) => {
                let op = req
                    .get("op")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let inp = req.get_mut("input").map(Value::take).unwrap_or(Value::Null);
                match dispatch(&op, inp) {
                    Ok(v) => json!({ "ok": v }),
                    Err(e) => json!({ "err": e }),
                }
            }
            Err(e) => json!({ "err": Error::json(e) }),
        };
        leak(serde_json::to_vec(&reply).unwrap_or_default())
    }
}

/// Host functions. Each is permission-checked by the host.
pub mod host {
    use super::*;
    use serde::de::DeserializeOwned;

    #[cfg(target_arch = "wasm32")]
    mod ffi {
        #[link(wasm_import_module = "fc")]
        extern "C" {
            pub fn host_call(ptr: i32, len: i32) -> i32;
            pub fn host_reply(ptr: i32);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn transport(request: &[u8]) -> Vec<u8> {
        // SAFETY: the host reads `request` and then writes exactly `len` bytes into `buf`.
        unsafe {
            let len = ffi::host_call(request.as_ptr() as usize as i32, request.len() as i32);
            let mut buf = vec![0u8; len.max(0) as usize];
            ffi::host_reply(buf.as_mut_ptr() as usize as i32);
            buf
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn transport(request: &[u8]) -> Vec<u8> {
        crate::testing::transport(request)
    }

    /// Calls host function `func` with JSON `args`.
    pub fn call_raw(func: &str, args: Value) -> Result<Value> {
        let request =
            serde_json::to_vec(&json!({ "fn": func, "args": args })).map_err(Error::json)?;
        let reply: Value = serde_json::from_slice(&transport(&request)).map_err(Error::json)?;
        if let Some(err) = reply.get("err") {
            return Err(serde_json::from_value(err.clone()).map_err(Error::json)?);
        }
        Ok(reply.get("ok").cloned().unwrap_or(Value::Null))
    }

    /// Calls an API method by wire name (`v1.notes.create`).
    pub fn invoke(method: &str, params: Value) -> Result<Value> {
        call_raw("invoke", json!({ "method": method, "params": params }))
    }

    /// Calls an API method with its typed request/response (`call::<NotesCreate>(&req)`).
    pub fn call<M: fc_api::ApiMethod>(request: &M::Request) -> Result<M::Response> {
        let params = serde_json::to_value(request).map_err(Error::json)?;
        let v = invoke(&format!("{}.{}", fc_api::API_VERSION, M::NAME), params)?;
        serde_json::from_value(v).map_err(Error::json)
    }

    /// Plugin-private storage.
    pub mod kv {
        use super::*;

        /// Value at `key`.
        pub fn get<T: DeserializeOwned>(key: &str) -> Result<Option<T>> {
            match call_raw("kv.get", json!({ "key": key }))? {
                Value::Null => Ok(None),
                v => serde_json::from_value(v).map(Some).map_err(Error::json),
            }
        }

        /// Stores `value` at `key`. Needs the `storage:kv` permission; refused past the
        /// plugin's storage quota.
        pub fn set<T: Serialize>(key: &str, value: &T) -> Result<()> {
            let value = serde_json::to_value(value).map_err(Error::json)?;
            call_raw("kv.set", json!({ "key": key, "value": value })).map(|_| ())
        }

        /// Removes `key`.
        pub fn delete(key: &str) -> Result<()> {
            call_raw("kv.delete", json!({ "key": key })).map(|_| ())
        }

        /// One page of entries whose key starts with `prefix`, by key, starting after
        /// `after` (at most 1000 entries / about 4 MiB per page). The next page starts after
        /// the last key returned; an empty page is the end.
        pub fn list(prefix: &str, after: Option<&str>) -> Result<Vec<(String, Value)>> {
            let v = call_raw("kv.list", json!({ "prefix": prefix, "after": after }))?;
            serde_json::from_value(v).map_err(Error::json)
        }
    }

    /// The plugin's settings (defaults applied).
    pub fn settings() -> Result<BTreeMap<String, Value>> {
        serde_json::from_value(call_raw("settings.get", json!({}))?).map_err(Error::json)
    }

    /// Changes one of the plugin's own settings (`None` = back to the default); returns all of
    /// them. Only keys of the plugin's settings schema; app settings are out of reach.
    pub fn set_setting(key: &str, value: Option<Value>) -> Result<BTreeMap<String, Value>> {
        let v = call_raw("settings.set", json!({ "key": key, "value": value }))?;
        serde_json::from_value(v).map_err(Error::json)
    }

    /// Files the user picks (`fs:pick`, only during a user-started operation). The plugin
    /// gets names and bytes, never paths.
    pub mod fs {
        use super::*;

        /// A file the user chose to open.
        #[derive(Debug, Clone, Deserialize)]
        pub struct PickedFile {
            /// File name (no folder).
            pub name: String,
            /// Size in bytes.
            pub size: u64,
            /// Content, standard base64.
            pub data_b64: String,
        }

        /// Shows an open dialog (`extensions` filter it, e.g. `["csv"]`). `None` = cancelled.
        pub fn open(extensions: &[&str]) -> Result<Option<PickedFile>> {
            match call_raw("fs.open", json!({ "extensions": extensions }))? {
                Value::Null => Ok(None),
                v => serde_json::from_value(v).map(Some).map_err(Error::json),
            }
        }

        /// Shows a save dialog and writes `data_b64` where the user chose. Returns the chosen
        /// file name, `None` = cancelled.
        pub fn save(
            suggested_name: &str,
            data_b64: &str,
            extensions: &[&str],
        ) -> Result<Option<String>> {
            let v = call_raw(
                "fs.save",
                json!({ "suggested_name": suggested_name, "data_b64": data_b64, "extensions": extensions }),
            )?;
            Ok(v.get("name").and_then(Value::as_str).map(str::to_owned))
        }
    }

    /// Writes to the plugin's log channel (`debug`, `info`, `warn`, `error`).
    pub fn log(level: &str, message: &str) {
        let _ = call_raw("log", json!({ "level": level, "message": message }));
    }

    /// Shows a notice to the user. Needs `ui:notify`; at most 3 per call and 10 per minute,
    /// text up to about 4 KiB.
    pub fn notify(text: UiText, level: &str) -> Result<()> {
        call_raw("ui.notify", json!({ "text": text, "level": level })).map(|_| ())
    }
}

/// Declarative UI builders.
pub mod ui {
    use super::*;

    /// Text from the plugin's locale files.
    pub fn key(key: &str) -> UiText {
        UiText::Key {
            key: key.to_owned(),
            params: BTreeMap::new(),
        }
    }

    /// Locale text with params.
    pub fn key_with(key: &str, params: &[(&str, String)]) -> UiText {
        UiText::Key {
            key: key.to_owned(),
            params: params
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        }
    }

    /// Literal content (data, not UI wording).
    pub fn text(text: impl Into<String>) -> UiText {
        UiText::Text { text: text.into() }
    }

    /// Vertical stack.
    pub fn column(children: Vec<UiNode>) -> UiNode {
        UiNode::Column { children }
    }

    /// Heading.
    pub fn heading(text: UiText) -> UiNode {
        UiNode::Heading { text }
    }

    /// Paragraph.
    pub fn paragraph(text: UiText) -> UiNode {
        UiNode::Text { text }
    }

    /// Text input.
    pub fn input(id: &str, label: Option<UiText>) -> UiNode {
        UiNode::Input {
            id: id.to_owned(),
            label,
            value: None,
            multiline: false,
        }
    }

    /// Button.
    pub fn button(action: &str, label: UiText, primary: bool) -> UiNode {
        UiNode::Button {
            action: action.to_owned(),
            label,
            primary,
        }
    }

    /// Feedback.
    pub fn feedback(correct: bool, text: UiText) -> UiNode {
        UiNode::Feedback { correct, text }
    }

    /// Progress bar.
    pub fn progress(value: u32, max: u32) -> UiNode {
        UiNode::Progress { value, max }
    }

    /// A step to show.
    pub fn show(title: Option<UiText>, node: UiNode, state: Value) -> UiReply {
        UiReply {
            title,
            node: Some(node),
            done: false,
            state,
        }
    }

    /// The session is over.
    pub fn done() -> UiReply {
        UiReply {
            title: None,
            node: None,
            done: true,
            state: Value::Null,
        }
    }
}

/// A fake host for native unit tests of plugin logic (no WASM needed).
pub mod testing {
    use super::*;
    use std::cell::RefCell;

    type Handler = Box<dyn FnMut(&str, Value) -> Result<Value>>;

    thread_local! {
        static HOST: RefCell<Option<Handler>> = RefCell::new(None);
    }

    /// Runs `f` with `handler` answering host calls (`(func, args) → result`).
    pub fn with_host<R>(
        handler: impl FnMut(&str, Value) -> Result<Value> + 'static,
        f: impl FnOnce() -> R,
    ) -> R {
        HOST.with(|h| *h.borrow_mut() = Some(Box::new(handler)));
        let r = f();
        HOST.with(|h| *h.borrow_mut() = None);
        r
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn transport(request: &[u8]) -> Vec<u8> {
        let reply = match serde_json::from_slice::<Value>(request) {
            Ok(req) => {
                let func = req
                    .get("fn")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let args = req.get("args").cloned().unwrap_or(Value::Null);
                let result = HOST.with(|h| match h.borrow_mut().as_mut() {
                    Some(handler) => handler(&func, args),
                    None => Err(Error::new("PERMISSION_DENIED", "no fake host installed")),
                });
                match result {
                    Ok(v) => json!({ "ok": v }),
                    Err(e) => json!({ "err": e }),
                }
            }
            Err(e) => json!({ "err": Error::json(e) }),
        };
        serde_json::to_vec(&reply).unwrap_or_default()
    }
}
