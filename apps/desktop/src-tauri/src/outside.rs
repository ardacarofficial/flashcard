//! v0.9 "outside world" glue of the desktop shell (docs/06 §3, §6, §7): `flashcard://` links,
//! single instance, the quick-add shortcut, the opt-in clipboard watcher and the local HTTP
//! API server's lifecycle. Transport only: what a link means, whether copied text is worth a
//! suggestion and who may use the API are decided by the app (`deep_links.open`,
//! `clipboard.check`, the `local_api` module).
//!
//! **Links while the app is closed.** The installer registers the `flashcard` scheme
//! (`tauri.conf.json` → `plugins.deep-link`). The OS starts the app with the link as an
//! argument ([`links_in_args`] at startup); if the app is already running, the second process
//! hands its arguments to the first through `tauri-plugin-single-instance` (registered
//! **first**) and exits. Either way the link goes through [`open_link`] and its outcome waits
//! in [`DeepLinkInbox`] until the UI takes it (`take_deep_links`), so a link that arrives
//! before the webview has loaded is not lost.

use crate::ipc::SharedApp;
use fc_api::v1::events::{DomainEvent, EventEnvelope};
use fc_api::v1::inbox::{CheckClipboard, ClipboardVerdict, OpenDeepLink};
use fc_api::v1::methods as m;
use fc_api::v1::oplog::Actor;
use fc_api::v1::settings::GetSettings;
use fc_app::App;
use fc_core::settings::SettingScope;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Shell event: links were handled; the UI calls `take_deep_links`.
pub const DEEP_LINK_EVENT: &str = "fc://deep-link";
/// Shell event: copied text worth suggesting (`{ "text": … }`).
pub const CLIPBOARD_EVENT: &str = "fc://clipboard";
/// Label of the quick-add window.
pub const QUICK_ADD_LABEL: &str = "quick-add";
/// How often the clipboard is looked at while the `clipboard` module is on.
pub const CLIPBOARD_POLL: Duration = Duration::from_millis(1000);
/// How often the server follows the `local_api` module switch and port.
pub const API_SERVER_POLL: Duration = Duration::from_millis(2000);
/// Most link outcomes kept for the UI.
const MAX_QUEUED_LINKS: usize = 20;

// ------------------------------------------------------------------ deep links

/// Outcomes of handled links, waiting for the UI.
#[derive(Debug, Default)]
pub struct DeepLinkInbox(Mutex<Vec<Value>>);

impl DeepLinkInbox {
    fn push(&self, v: Value) {
        let mut q = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if q.len() >= MAX_QUEUED_LINKS {
            q.remove(0);
        }
        q.push(v);
    }

    /// Takes every waiting outcome.
    pub fn take(&self) -> Vec<Value> {
        std::mem::take(
            &mut *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

/// The `flashcard://` link among process arguments (cold start or a second instance).
pub fn links_in_args<S: AsRef<str>>(args: &[S]) -> Option<String> {
    fc_core::inbox::deeplink::find_in_args(args)
}

/// Runs `deep_links.open` as the user and queues its outcome
/// (`{url, ok: DeepLinkOutcome}` or `{url, error: ApiError}`).
pub fn open_link(app: &App, queue: &DeepLinkInbox, url: &str) {
    let r = app.call::<m::DeepLinksOpen>(
        &OpenDeepLink {
            url: url.to_owned(),
        },
        &Actor::User,
    );
    queue.push(match r {
        Ok(outcome) => json!({ "url": url, "ok": outcome }),
        Err(error) => json!({ "url": url, "error": error }),
    });
}

/// Handles `url` and tells the UI; brings the main window forward.
pub fn deliver_link<R: Runtime>(handle: &AppHandle<R>, url: &str) {
    let (Some(app), Some(queue)) = (
        handle.try_state::<SharedApp>(),
        handle.try_state::<DeepLinkInbox>(),
    ) else {
        return;
    };
    open_link(&app, &queue, url);
    focus_main(handle);
    let _ = handle.emit(DEEP_LINK_EVENT, ());
}

/// The UI takes the outcomes of links handled so far.
#[tauri::command]
pub fn take_deep_links(queue: tauri::State<'_, DeepLinkInbox>) -> Vec<Value> {
    queue.take()
}

/// Single-instance callback: a second launch forwards its arguments here and exits.
pub fn on_second_instance<R: Runtime>(handle: &AppHandle<R>, argv: Vec<String>) {
    match links_in_args(&argv) {
        Some(url) => deliver_link(handle, &url),
        None => focus_main(handle),
    }
}

fn focus_main<R: Runtime>(handle: &AppHandle<R>) {
    if let Some(w) = handle.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

// ------------------------------------------------------------------ settings

fn setting(app: &App, key: &str) -> Option<Value> {
    app.call::<m::SettingsGet>(
        &GetSettings {
            scope: SettingScope::App,
            keys: Some(vec![key.to_owned()]),
        },
        &Actor::System,
    )
    .ok()?
    .values
    .into_iter()
    .find(|v| v.key == key)
    .map(|v| v.value)
}

// ------------------------------------------------------------------ quick add

/// Text to prefill the quick-add window with: the clipboard, when the `quick_add` module and
/// its `prefill_clipboard` setting are on and the text is not something secret-looking.
pub fn prefill_text(app: &App, clipboard: Option<String>) -> Option<String> {
    use fc_app::modules::quick_add::{ID, PREFILL_CLIPBOARD};
    if !app.is_module_enabled(ID) || setting(app, PREFILL_CLIPBOARD) != Some(Value::Bool(true)) {
        return None;
    }
    let filter = fc_core::inbox::clipboard::ClipboardFilter {
        min_chars: 1,
        max_chars: 2000,
    };
    fc_core::inbox::clipboard::check(&clipboard?, filter).ok()
}

/// The quick-add window asks for its prefill (read only when the user pressed the shortcut).
#[tauri::command]
pub async fn quick_add_prefill<R: Runtime>(
    handle: AppHandle<R>,
    state: tauri::State<'_, SharedApp>,
) -> Result<Option<String>, fc_api::ApiError> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let app = state.inner().clone();
    let text = handle.clipboard().read_text().ok();
    tauri::async_runtime::spawn_blocking(move || prefill_text(&app, text))
        .await
        .map_err(|e| fc_api::ApiError::new(fc_api::ErrorCode::Internal, e.to_string()))
}

fn show_quick_add<R: Runtime>(handle: &AppHandle<R>) {
    if let Some(w) = handle.get_webview_window(QUICK_ADD_LABEL) {
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("fc://quick-add", ());
        return;
    }
    let built = tauri::WebviewWindowBuilder::new(
        handle,
        QUICK_ADD_LABEL,
        tauri::WebviewUrl::App("index.html#quick-add".into()),
    )
    .title("Flashcard")
    .inner_size(440.0, 320.0)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(true)
    .build();
    if let Err(e) = built {
        eprintln!("fc-desktop: quick add window: {e}");
    }
}

fn register_quick_add<R: Runtime>(handle: &AppHandle<R>, app: &App) {
    use fc_app::modules::quick_add::{DEFAULT_SHORTCUT, ID, SHORTCUT};
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    if !app.is_module_enabled(ID) {
        return;
    }
    let shortcut = setting(app, SHORTCUT)
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SHORTCUT.to_owned());
    let r = handle
        .global_shortcut()
        .on_shortcut(shortcut.as_str(), |h, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                show_quick_add(h);
            }
        });
    if let Err(e) = r {
        eprintln!("fc-desktop: quick add shortcut {shortcut}: {e}");
    }
}

// ------------------------------------------------------------------ clipboard watcher

fn fingerprint(text: &str) -> u64 {
    fc_core::note::fnv1a64(text.as_bytes())
}

/// One look at the clipboard. Nothing is read while the `clipboard` module is off; content
/// the OS marks as private is skipped; text already there when watching started is not
/// offered; only a fingerprint of the last text is kept. Returns the text to suggest.
pub fn clipboard_tick(
    app: &App,
    last: &mut Option<u64>,
    is_private: impl FnOnce() -> bool,
    read: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if !app.is_module_enabled(fc_app::modules::clipboard::ID) || app.background_blocked() {
        *last = None;
        return None;
    }
    if is_private() {
        return None;
    }
    let text = read()?;
    let fp = fingerprint(&text);
    let first = last.is_none();
    if *last == Some(fp) {
        return None;
    }
    *last = Some(fp);
    if first {
        return None;
    }
    let verdict: ClipboardVerdict = app
        .call::<m::ClipboardCheck>(&CheckClipboard { text }, &Actor::User)
        .ok()?;
    verdict.suggest.then_some(verdict.text)
}

/// Whether the clipboard holds content the OS marks as private (Windows: password managers
/// set `ExcludeClipboardContentFromMonitorProcessing`, or opt out of history/cloud sync).
#[cfg(windows)]
fn clipboard_is_private() -> bool {
    use clipboard_win::raw::{is_format_avail, register_format};
    [
        "ExcludeClipboardContentFromMonitorProcessing",
        "CanIncludeInClipboardHistory",
        "CanUploadToCloudClipboard",
    ]
    .iter()
    .filter_map(|name| register_format(name))
    .any(|f| is_format_avail(f.get()))
}

/// Other platforms have no such marker; the text filter still skips secret-looking text.
#[cfg(not(windows))]
fn clipboard_is_private() -> bool {
    false
}

fn watch_clipboard<R: Runtime>(handle: AppHandle<R>, app: SharedApp) {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    std::thread::spawn(move || {
        let mut last = None;
        loop {
            std::thread::sleep(CLIPBOARD_POLL);
            let suggestion = clipboard_tick(&app, &mut last, clipboard_is_private, || {
                handle.clipboard().read_text().ok()
            });
            if let Some(text) = suggestion {
                if let Some(w) = handle.get_webview_window("main") {
                    let _ = w.emit(CLIPBOARD_EVENT, json!({ "text": text }));
                }
            }
        }
    });
}

// ------------------------------------------------------------------ local HTTP API

/// The running server and its port.
#[derive(Default)]
pub struct ApiServerSlot(Mutex<Option<(fc_http::Server, u16)>>);

impl ApiServerSlot {
    /// The bound address, if running.
    pub fn addr(&self) -> Option<std::net::SocketAddr> {
        self.0
            .lock()
            .ok()
            .and_then(|s| s.as_ref().map(|(srv, _)| srv.local_addr()))
    }
}

/// Starts, stops or restarts the server to match the `local_api` module switch and port.
/// Returns an error text when it could not start (e.g. the port is taken).
pub fn sync_api_server(app: &Arc<App>, slot: &ApiServerSlot) -> Result<(), String> {
    use fc_app::modules::local_api;
    // v1.0: not while the app is locked or switching profile (the requests would be refused
    // anyway); a restart on another profile binds a new server to that collection.
    let want = (app.is_module_enabled(local_api::ID) && !app.background_blocked())
        .then(|| local_api::port(app).unwrap_or(local_api::DEFAULT_PORT));
    let mut guard = slot.0.lock().map_err(|e| e.to_string())?;
    let running = guard.as_ref().map(|(_, p)| *p);
    if running == want {
        return Ok(());
    }
    if let Some((server, _)) = guard.take() {
        server.stop();
    }
    if let Some(port) = want {
        let server = fc_http::Server::start(app.clone(), fc_http::ServerConfig::localhost(port))
            .map_err(|e| format!("local API on 127.0.0.1:{port}: {e}"))?;
        *guard = Some((server, port));
    }
    Ok(())
}

fn run_api_server(app: SharedApp) {
    std::thread::spawn(move || {
        let slot = ApiServerSlot::default();
        let mut reported = None;
        loop {
            let r = sync_api_server(&app, &slot);
            if let Err(e) = &r {
                if reported.as_ref() != Some(e) {
                    eprintln!("fc-desktop: {e}");
                }
            }
            reported = r.err();
            std::thread::sleep(API_SERVER_POLL);
        }
    });
}

/// A pairing request needs the user's decision: bring the window forward.
fn attend_pairings<R: Runtime>(handle: AppHandle<R>, app: &App) {
    app.events().subscribe(Arc::new(move |e: &EventEnvelope| {
        if matches!(e.event, DomainEvent::LocalApiPairing { .. }) {
            focus_main(&handle);
            if let Some(w) = handle.get_webview_window("main") {
                let _ = w.request_user_attention(Some(tauri::UserAttentionType::Informational));
            }
        }
    }));
}

// ------------------------------------------------------------------ startup

/// Wires everything up once the collection is open.
pub fn start<R: Runtime>(handle: &AppHandle<R>, app: SharedApp) {
    use tauri_plugin_deep_link::DeepLinkExt;
    handle.manage(DeepLinkInbox::default());
    // Unbundled dev builds register the scheme at runtime (installers do it at install).
    #[cfg(all(debug_assertions, any(windows, target_os = "linux")))]
    {
        let _ = handle.deep_link().register_all();
    }
    // Cold start: the link the OS started us with.
    let args: Vec<String> = std::env::args().collect();
    let cold = links_in_args(&args).or_else(|| {
        handle
            .deep_link()
            .get_current()
            .ok()
            .flatten()
            .and_then(|urls| urls.first().map(ToString::to_string))
    });
    if let Some(url) = cold {
        deliver_link(handle, &url);
    }
    // Links while running (macOS delivers them here; Windows/Linux via the second instance).
    let h = handle.clone();
    handle.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            deliver_link(&h, url.as_str());
        }
    });
    register_quick_add(handle, &app);
    watch_clipboard(handle.clone(), app.clone());
    attend_pairings(handle.clone(), &app);
    run_api_server(app);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use fc_api::v1::decks::CreateDeck;
    use fc_api::v1::inbox::{DeepLinkOutcome, DeepLinkTarget, ListInbox};
    use fc_api::v1::modules::SetModuleEnabled;
    use fc_app::testing::{TempSqlite, TestApp};

    fn deck(t: &TestApp) -> fc_core::DeckId {
        t.ok::<m::DecksCreate>(CreateDeck {
            path: "İngilizce".into(),
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        })
        .id
    }

    /// v0.9 acceptance ("flashcard:// links open the app even when it is closed"), shell side:
    /// the OS starts the app with the link as an argument; it is handled at startup and kept
    /// until the UI asks, so nothing is lost before the webview listens.
    #[test]
    fn cold_start_link_is_handled_and_kept_for_the_ui() {
        let t = TestApp::mem();
        let d = deck(&t);
        let argv = [
            "C:\\Program Files\\Flashcard\\flashcard.exe".to_owned(),
            format!("flashcard://deck/{d}"),
        ];
        let url = links_in_args(&argv).unwrap();
        let queue = DeepLinkInbox::default();
        open_link(&t.app, &queue, &url);
        let taken = queue.take();
        assert_eq!(taken.len(), 1);
        let outcome: DeepLinkOutcome = serde_json::from_value(taken[0]["ok"].clone()).unwrap();
        assert_eq!(
            outcome,
            DeepLinkOutcome::Navigate {
                target: DeepLinkTarget::Deck { deck_id: d }
            }
        );
        assert!(queue.take().is_empty(), "taken once");
    }

    /// A second launch (app already running) forwards its link to the first instance: the
    /// same handling, so a writing link lands in the inbox, a bad one comes back as an error.
    #[test]
    fn second_instance_links_reach_the_running_app() {
        let t = TestApp::mem();
        let d = deck(&t);
        let queue = DeepLinkInbox::default();
        let argv = vec![
            "flashcard.exe".to_owned(),
            format!("\"flashcard://add?deck={d}&front=porsuk&back=badger\""),
        ];
        open_link(&t.app, &queue, &links_in_args(&argv).unwrap());
        open_link(&t.app, &queue, "flashcard://wipe");
        let taken = queue.take();
        assert_eq!(taken[0]["ok"]["kind"], "inbox");
        assert_eq!(taken[1]["error"]["code"], "VALIDATION_DEEP_LINK");
        let inbox = t.ok::<m::InboxList>(ListInbox::default());
        assert_eq!(inbox.items[0].item.front.as_deref(), Some("porsuk"));
        assert!(
            links_in_args(&["flashcard.exe"]).is_none(),
            "plain relaunch = focus only"
        );
        for n in 0..30 {
            open_link(&t.app, &queue, &format!("flashcard://deck/{n}"));
        }
        assert_eq!(queue.take().len(), MAX_QUEUED_LINKS);
    }

    /// The scheme is registered by the bundle, and single-instance is the first plugin (it
    /// must run before anything else to forward and exit).
    #[test]
    fn scheme_and_single_instance_are_configured() {
        let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let schemes = &conf["plugins"]["deep-link"]["desktop"]["schemes"];
        assert_eq!(schemes, &json!(["flashcard"]));
        let lib = include_str!("lib.rs");
        let first_plugin = lib.find(".plugin(").unwrap();
        assert_eq!(
            lib.find(".plugin(tauri_plugin_single_instance::init"),
            Some(first_plugin)
        );
        assert!(lib.contains("outside::on_second_instance"));
    }

    #[test]
    fn clipboard_is_only_read_while_the_module_is_on() {
        let t = TestApp::mem();
        let mut last = None;
        let mut reads = 0;
        let tick = |text: &str, last: &mut Option<u64>, reads: &mut i32| {
            clipboard_tick(
                &t.app,
                last,
                || false,
                || {
                    *reads += 1;
                    Some(text.to_owned())
                },
            )
        };
        assert_eq!(tick("porsuk", &mut last, &mut reads), None);
        assert_eq!(reads, 0, "off by default: the clipboard is not even read");
        t.ok::<m::ModulesSetEnabled>(SetModuleEnabled {
            id: "clipboard".into(),
            enabled: true,
        });
        // What was there when watching started is not offered.
        assert_eq!(tick("already there", &mut last, &mut reads), None);
        assert_eq!(
            tick("porsuk", &mut last, &mut reads).as_deref(),
            Some("porsuk")
        );
        assert_eq!(
            tick("porsuk", &mut last, &mut reads),
            None,
            "same text once"
        );
        assert_eq!(
            tick("Tr0ub4dor&3", &mut last, &mut reads),
            None,
            "secret-looking"
        );
        // Private content (OS marker) is not read at all.
        let before = reads;
        assert_eq!(
            clipboard_tick(&t.app, &mut last, || true, || panic!("read")),
            None
        );
        assert_eq!(reads, before);
    }

    #[test]
    fn quick_add_prefill_follows_module_and_setting() {
        let t = TestApp::mem();
        assert_eq!(
            prefill_text(&t.app, Some(" porsuk ".into())).as_deref(),
            Some("porsuk")
        );
        assert_eq!(prefill_text(&t.app, Some("sk-ant-api03-x".into())), None);
        assert_eq!(prefill_text(&t.app, None), None);
        t.ok::<m::SettingsSet>(fc_api::v1::settings::SetSetting {
            scope: SettingScope::App,
            key: fc_app::modules::quick_add::PREFILL_CLIPBOARD.into(),
            value: json!(false),
        });
        assert_eq!(prefill_text(&t.app, Some("porsuk".into())), None);
    }

    /// The server follows the module switch: off by default, on → listening on loopback,
    /// off → stopped.
    #[test]
    fn api_server_follows_the_module_switch() {
        let t = TestApp::new(Arc::new(TempSqlite::new()));
        t.ok::<m::SettingsSet>(fc_api::v1::settings::SetSetting {
            scope: SettingScope::App,
            key: fc_app::modules::local_api::PORT.into(),
            value: json!(free_port()),
        });
        let app = Arc::new(TestApp::new(t.store.clone()).app);
        let slot = ApiServerSlot::default();
        sync_api_server(&app, &slot).unwrap();
        assert!(slot.addr().is_none(), "off by default");
        t.ok::<m::ModulesSetEnabled>(SetModuleEnabled {
            id: "local_api".into(),
            enabled: true,
        });
        app.reload_cached_state().unwrap();
        sync_api_server(&app, &slot).unwrap();
        let addr = slot.addr().unwrap();
        assert!(addr.ip().is_loopback());
        assert!(std::net::TcpStream::connect(addr).is_ok());
        t.ok::<m::ModulesSetEnabled>(SetModuleEnabled {
            id: "local_api".into(),
            enabled: false,
        });
        app.reload_cached_state().unwrap();
        sync_api_server(&app, &slot).unwrap();
        assert!(slot.addr().is_none());
        assert!(std::net::TcpStream::connect(addr).is_err());
    }

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }
}
