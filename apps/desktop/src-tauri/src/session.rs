//! v1.0 session glue of the desktop shell (docs/02 §19, §28, §29): which profile's
//! collection opens, the app lock's idle watch and IPC gate, the daily reminder as an OS
//! notification, the system tray and "keep running in the tray". Transport only: the rules
//! (who may call what while locked, when a reminder is due, which collection a profile owns)
//! live in `fc_app::modules::{profiles, reminders}`.
//!
//! **Profile switch.** `profiles.switch` retires the running session (the local HTTP API
//! stops and refuses, scheduled automation rules skip: `App::background_blocked`) and
//! publishes `profile_switched`; the shell then restarts the process, which opens the new
//! profile's collection and binds every background service (HTTP API, scheduler, reminders,
//! clipboard watcher) to it from scratch.

use crate::ipc::SharedApp;
use fc_api::v1::common::Empty;
use fc_api::v1::events::{DomainEvent, EventEnvelope};
use fc_api::v1::methods as m;
use fc_api::v1::oplog::Actor;
use fc_app::modules::profiles::{self, ProfileStore, Session};
use fc_app::modules::reminders;
use fc_core::Clock;
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Manager, Runtime};

/// Id of the tray icon.
pub const TRAY_ID: &str = "main";

/// Opens the active profile of app data folder `root`: its session (locked when the profile
/// has a lock) and its collection, with the session installed. `secrets` keeps the lock hash
/// and AI keys (the OS keychain in the app); the collection sees only its profile's accounts
/// (`Session::profile_secrets`: AI keys are not shared between profiles).
pub fn open_profile(
    root: &Path,
    secrets: Arc<dyn fc_ai::secrets::SecretStore>,
    clock: Arc<dyn Clock>,
) -> Result<(SharedApp, Arc<Session>), String> {
    std::fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let session = Session::open(ProfileStore::new(root), secrets, clock)
        .map(Arc::new)
        .map_err(|e| {
            let e = e.into_api();
            format!("{}: {}", e.code, e.message)
        })?;
    let path = session.collection_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let collection = fc_app::App::open_sqlite(&path).map_err(|e| {
        let e = e.into_api();
        format!("{}\n{}: {}", path.display(), e.code, e.message)
    })?;
    collection.set_secret_store(session.profile_secrets());
    collection.install_session(session.clone());
    Ok((Arc::new(collection), session))
}

/// The OS's current UTC offset in minutes (follows daylight saving time).
pub fn local_offset_mins() -> i32 {
    chrono::Local::now().offset().local_minus_utc() / 60
}

/// Starts the lock's idle watch, the reminder checks and the tray, and follows the events
/// that need the shell (reminder → notification, lock → tray menu, profile switch →
/// restart).
pub fn start<R: Runtime>(handle: &AppHandle<R>, app: &SharedApp) -> std::io::Result<()> {
    profiles::spawn_idle_watch(app)?;
    reminders::spawn_reminders(app, Arc::new(local_offset_mins))?;
    if let Err(e) = build_tray(handle, app) {
        // No tray (e.g. a Linux desktop without an indicator area) is not fatal.
        eprintln!("fc-desktop: tray: {e}");
    }
    let h = handle.clone();
    let weak = Arc::downgrade(app);
    app.events().subscribe(Arc::new(move |e: &EventEnvelope| {
        let Some(app) = weak.upgrade() else { return };
        on_event(&h, &app, &e.event);
    }));
    Ok(())
}

fn on_event<R: Runtime>(handle: &AppHandle<R>, app: &fc_app::App, event: &DomainEvent) {
    match event {
        DomainEvent::ReminderDue { counts, .. } => {
            use tauri_plugin_notification::NotificationExt;
            let (title, body) = reminders::notification_text(app.locale(), *counts);
            if let Err(e) = handle
                .notification()
                .builder()
                .title(title)
                .body(body)
                .show()
            {
                eprintln!("fc-desktop: notification: {e}");
            }
        }
        DomainEvent::ProfileSwitched { .. } => {
            // Off the event thread: the IPC reply of `profiles.switch` goes out first.
            let h = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                h.restart();
            });
        }
        _ => {}
    }
}

fn build_tray<R: Runtime>(handle: &AppHandle<R>, app: &SharedApp) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;
    let locale = app.locale();
    let open = MenuItem::with_id(
        handle,
        "open",
        reminders::TRAY_OPEN.get(locale),
        true,
        None::<&str>,
    )?;
    let lock = MenuItem::with_id(
        handle,
        "lock",
        reminders::TRAY_LOCK.get(locale),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        handle,
        "quit",
        reminders::TRAY_QUIT.get(locale),
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(handle, &[&open, &lock, &quit])?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(reminders::TRAY_TITLE.get(locale))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|h, event| match event.id().as_ref() {
            "open" => show_main(h),
            "lock" => {
                if let Some(app) = h.try_state::<SharedApp>() {
                    // No lock set → CONFLICT_LOCK_STATE; nothing to do.
                    let _ = app.call::<m::LockLock>(&Empty {}, &Actor::User);
                }
            }
            "quit" => h.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = handle.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(handle)?;
    Ok(())
}

fn show_main<R: Runtime>(handle: &AppHandle<R>) {
    if let Some(w) = handle.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Whether closing the main window should only hide it (the `reminders.keep_in_tray`
/// setting): reminders keep coming while it is hidden.
pub fn hide_instead_of_close(app: &fc_app::App, window_label: &str) -> bool {
    window_label == "main" && reminders::keeps_running_in_tray(app)
}

/// Window events the session cares about: closing the main window while "keep in tray" is
/// on hides it instead.
pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        let Some(app) = window.try_state::<SharedApp>() else {
            return;
        };
        if hide_instead_of_close(&app, window.label()) {
            api.prevent_close();
            let _ = window.hide();
        }
    }
}

/// Pseudo method name the shell's own entry points (media protocol, links, quick add) pass
/// to [`profiles::gate`]: not on the lock screen's list, so refused while locked.
pub const SHELL_GATE: &str = "v1.shell";

/// Whether the window may not use the collection now (locked; also locks an idle app).
pub fn is_locked(app: &fc_app::App) -> bool {
    profiles::gate(app, SHELL_GATE).is_err()
}

/// The database file whose `media/` the media protocol serves, or the HTTP status to answer:
/// 423 while the app is locked (no card images or audio behind the lock screen).
pub fn media_db(app: &fc_app::App) -> Result<std::path::PathBuf, u16> {
    if is_locked(app) {
        return Err(423);
    }
    app.session().map(|s| s.collection_path()).ok_or(500)
}

/// [`media_db`] of the running app.
pub fn collection_path<R: Runtime>(handle: &AppHandle<R>) -> Result<std::path::PathBuf, u16> {
    handle
        .try_state::<SharedApp>()
        .ok_or(500)
        .and_then(|app| media_db(&app))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::outside::{sync_api_server, ApiServerSlot};
    use fc_api::v1::decks::CreateDeck;
    use fc_api::v1::modules::SetModuleEnabled;
    use fc_api::v1::session::{CreateProfile, SetLock, SwitchProfile, Unlock};
    use fc_api::v1::settings::SetSetting;
    use fc_core::settings::SettingScope;
    use fc_core::ux::lock::LockKind;
    use serde_json::json;

    fn user<M: fc_api::ApiMethod>(app: &fc_app::App, req: M::Request) -> M::Response {
        app.call::<M>(&req, &Actor::User).unwrap()
    }

    fn open(root: &Path, secrets: &Arc<fc_ai::secrets::MemorySecrets>) -> SharedApp {
        open_profile(root, secrets.clone(), Arc::new(fc_app::system::SystemClock))
            .unwrap()
            .0
    }

    fn serve_api(app: &fc_app::App, port: u16) {
        user::<m::SettingsSet>(
            app,
            SetSetting {
                scope: SettingScope::App,
                key: fc_app::modules::local_api::PORT.into(),
                value: json!(port),
            },
        );
        user::<m::ModulesSetEnabled>(
            app,
            SetModuleEnabled {
                id: "local_api".into(),
                enabled: true,
            },
        );
    }

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn deck_names(app: &fc_app::App) -> Vec<String> {
        user::<m::DecksList>(app, Empty {})
            .decks
            .into_iter()
            .map(|d| d.name)
            .collect()
    }

    /// Phase-4 review: the local HTTP API server (bound to one collection) stops while the
    /// app is locked and comes back on unlock; after a profile switch it stops, and the
    /// restart binds a new server to the other profile's collection.
    #[test]
    fn api_server_stops_while_locked_and_is_rebound_after_a_profile_switch() {
        let root = tempfile::tempdir().unwrap();
        let secrets = Arc::new(fc_ai::secrets::MemorySecrets::new());
        let first = open(root.path(), &secrets);
        assert_eq!(
            first.session().unwrap().collection_path(),
            root.path().join(crate::COLLECTION_FILE)
        );
        user::<m::DecksCreate>(
            &first,
            CreateDeck {
                path: "Varsayılan".into(),
                parent_id: None,
                description: None,
                icon: None,
                color: None,
            },
        );
        let port = free_port();
        serve_api(&first, port);
        let slot = ApiServerSlot::default();
        sync_api_server(&first, &slot).unwrap();
        let addr = slot.addr().expect("serving");

        // Locked → stopped; unlocked → serving again.
        user::<m::LockSet>(
            &first,
            SetLock {
                kind: LockKind::Pin,
                secret: "4821".into(),
                current: None,
                idle_minutes: None,
            },
        );
        user::<m::LockLock>(&first, Empty {});
        sync_api_server(&first, &slot).unwrap();
        assert!(slot.addr().is_none(), "stopped while locked");
        assert!(std::net::TcpStream::connect(addr).is_err());
        user::<m::LockUnlock>(
            &first,
            Unlock {
                secret: "4821".into(),
            },
        );
        sync_api_server(&first, &slot).unwrap();
        assert!(slot.addr().is_some(), "back after unlock");

        // Switch: the old collection's server stops at once.
        let work = user::<m::ProfilesCreate>(
            &first,
            CreateProfile {
                name: "İş".into()
            },
        );
        let switched = user::<m::ProfilesSwitch>(
            &first,
            SwitchProfile {
                id: work.id.clone(),
            },
        );
        assert!(switched.restart_required);
        sync_api_server(&first, &slot).unwrap();
        assert!(slot.addr().is_none(), "stopped after the switch");
        // Drop the old app like the old process does.
        drop(slot);
        drop(first);

        // The restart opens the other profile: its own (empty) collection, a new server.
        let second = open(root.path(), &secrets);
        let s = second.session().unwrap();
        assert_eq!(s.profile_id(), work.id);
        assert!(!s.is_locked(), "the lock belongs to the other profile");
        assert!(s
            .collection_path()
            .starts_with(root.path().join("profiles")));
        assert!(deck_names(&second).iter().all(|n| n != "Varsayılan"));
        serve_api(&second, port);
        let slot = ApiServerSlot::default();
        sync_api_server(&second, &slot).unwrap();
        assert_eq!(slot.addr().map(|a| a.port()), Some(port));
    }

    /// While locked the window reaches only the lock screen (the IPC bridge's gate).
    #[test]
    fn ipc_is_gated_while_locked() {
        let root = tempfile::tempdir().unwrap();
        let secrets = Arc::new(fc_ai::secrets::MemorySecrets::new());
        let app = open(root.path(), &secrets);
        user::<m::LockSet>(
            &app,
            SetLock {
                kind: LockKind::Pin,
                secret: "4821".into(),
                current: None,
                idle_minutes: None,
            },
        );
        user::<m::LockLock>(&app, Empty {});
        let e = crate::ipc::dispatch(&app, "v1.decks.tree", serde_json::Value::Null).unwrap_err();
        assert_eq!(e.code, fc_api::ErrorCode::PermissionAppLocked);
        let st = crate::ipc::dispatch(&app, "v1.lock.status", serde_json::Value::Null).unwrap();
        assert_eq!(st["locked"], true);
        crate::ipc::dispatch(&app, "v1.lock.unlock", json!({ "secret": "4821" })).unwrap();
        assert!(crate::ipc::dispatch(&app, "v1.decks.tree", serde_json::Value::Null).is_ok());
        // A restart of a locked profile starts locked.
        user::<m::LockLock>(&app, Empty {});
        drop(app);
        let again = open(root.path(), &secrets);
        assert!(again.session().unwrap().is_locked());
    }

    fn lock(app: &fc_app::App) {
        user::<m::LockSet>(
            app,
            SetLock {
                kind: LockKind::Pin,
                secret: "4821".into(),
                current: None,
                idle_minutes: None,
            },
        );
        user::<m::LockLock>(app, Empty {});
    }

    /// Security review: no media bytes behind the lock screen.
    #[test]
    fn media_protocol_is_gated_while_locked() {
        let root = tempfile::tempdir().unwrap();
        let secrets = Arc::new(fc_ai::secrets::MemorySecrets::new());
        let app = open(root.path(), &secrets);
        assert_eq!(media_db(&app), Ok(root.path().join(crate::COLLECTION_FILE)));
        lock(&app);
        assert_eq!(media_db(&app), Err(423));
        assert!(is_locked(&app));
        user::<m::LockUnlock>(
            &app,
            Unlock {
                secret: "4821".into(),
            },
        );
        assert!(media_db(&app).is_ok());
    }

    /// Security review: the collection's AI keys are its profile's keychain accounts; the
    /// default profile keeps the pre-v1.0 names (existing keys keep working).
    #[test]
    fn ai_keys_are_scoped_to_the_open_profile() {
        use fc_ai::secrets::{Secret, SecretStore};
        let root = tempfile::tempdir().unwrap();
        let secrets = Arc::new(fc_ai::secrets::MemorySecrets::new());
        let account = "ai.provider.x";
        secrets.set(account, &Secret::new("sk-default")).unwrap();
        let first = open(root.path(), &secrets);
        let ai = first.ai_services().unwrap().secrets();
        assert_eq!(ai.get(account).unwrap().unwrap().expose(), "sk-default");
        let work = user::<m::ProfilesCreate>(
            &first,
            CreateProfile {
                name: "İş".into()
            },
        );
        user::<m::ProfilesSwitch>(
            &first,
            SwitchProfile {
                id: work.id.clone(),
            },
        );
        drop(first);
        let second = open(root.path(), &secrets);
        let ai = second.ai_services().unwrap().secrets();
        assert!(
            ai.get(account).unwrap().is_none(),
            "not the other profile's key"
        );
        ai.set(account, &Secret::new("sk-work")).unwrap();
        assert_eq!(
            secrets
                .get(&format!("profile.{}.{account}", work.id))
                .unwrap()
                .unwrap()
                .expose(),
            "sk-work"
        );
        assert_eq!(
            secrets.get(account).unwrap().unwrap().expose(),
            "sk-default"
        );
    }

    #[test]
    fn keep_in_tray_only_hides_the_main_window() {
        let root = tempfile::tempdir().unwrap();
        let secrets = Arc::new(fc_ai::secrets::MemorySecrets::new());
        let app = open(root.path(), &secrets);
        assert!(!hide_instead_of_close(&app, "main"));
        user::<m::SettingsSet>(
            &app,
            SetSetting {
                scope: SettingScope::App,
                key: reminders::KEEP_IN_TRAY.into(),
                value: json!(true),
            },
        );
        assert!(hide_instead_of_close(&app, "main"));
        assert!(!hide_instead_of_close(&app, "quick-add"));
        let offset = local_offset_mins();
        assert!((-14 * 60..=14 * 60).contains(&offset));
    }
}
