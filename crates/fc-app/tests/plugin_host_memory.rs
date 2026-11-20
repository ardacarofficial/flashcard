//! Third security review, finding 2 (host memory bypass), the exact repro: a plugin sends
//! 400 `ui.notify` requests of 1 MiB each. They are refused before the host parses them and
//! the call's byte budget stops the plugin, so the host's peak memory stays bounded (before
//! the fix every notice was kept and published: hundreds of MiB).
//!
//! Its own test binary: it measures the whole process's peak resident memory (Linux
//! `VmHWM`, reset through `/proc/self/clear_refs`), so nothing else may run beside it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use fc_api::v1::methods::{PluginsInstall, PluginsRun};
use fc_api::v1::oplog::Actor;
use fc_api::v1::plugins::{InstallPlugin, RunPluginCommand};
use fc_app::testing::{MemStore, TestApp};
use serde_json::Value;
use std::sync::Arc;

const MIB: usize = 1024 * 1024;

/// A `/proc/self/status` field in bytes (`VmRSS`, `VmHWM`).
fn status(field: &str) -> Option<usize> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with(field))?;
    let kib: usize = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

/// Resets the peak resident size to the current one (Linux ≥ 4.0).
fn reset_peak() -> bool {
    std::fs::write("/proc/self/clear_refs", "5").is_ok()
}

#[test]
fn notify_flood_keeps_host_memory_bounded() {
    let t = TestApp::new(Arc::new(MemStore::new()));
    let dir = tempfile::tempdir().unwrap();
    let id = "com.test.flood";
    let request = fc_plugins::testing::notify_request(MIB);
    let wasm =
        fc_plugins::testing::wat2wasm(fc_plugins::testing::host_loop_wat(&request, 400).as_bytes())
            .unwrap();
    drop(request);
    let manifest = format!(
        "id = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\napi_version = \"1\"\n\
         min_app_version = \"0.7.0\"\npermissions = [\"ui:command\", \"ui:notify\"]\n\n\
         [contributes]\ncommands = [\"go\"]\n"
    );
    let path = dir.path().join("flood.fcplugin");
    std::fs::write(
        &path,
        fc_plugins::package::pack(&manifest, &wasm, &[]).unwrap(),
    )
    .unwrap();
    drop(wasm);
    t.ok::<PluginsInstall>(InstallPlugin {
        path: path.display().to_string(),
        grant: vec!["ui:command".into(), "ui:notify".into()],
    });
    t.take_events();

    let measured = reset_peak();
    let baseline = status("VmRSS:").unwrap_or(0);
    let r = t.app.call::<PluginsRun>(
        &RunPluginCommand {
            plugin_id: id.into(),
            command: "go".into(),
            params: Value::Null,
        },
        &Actor::User,
    );
    let peak = status("VmHWM:").unwrap_or(0).saturating_sub(baseline);

    let e = r.unwrap_err();
    assert_eq!(e.details["kind"], "HOST_LIMIT", "{e:?}");
    assert!(
        !t.take_events().iter().any(|e| matches!(
            e.event,
            fc_api::v1::events::DomainEvent::PluginNotice { .. }
        )),
        "no notice was published"
    );
    if measured && cfg!(target_os = "linux") {
        assert!(peak < 48 * MIB, "host peak grew by {} MiB", peak / MIB);
    } else {
        eprintln!("peak memory not measurable on this system; behaviour checked only");
    }
}
