//! Phase-4 integration, round 2 (v0.9 automation + local API/inbox next to everything else;
//! docs/10 §33): **one matrix** — every actor × every sensitive domain, through the real entry
//! point (`App::invoke`: `Registry::permits` + `ensure_access`).
//!
//! | Domain | User / Cli / System | Mcp | Plugin (all grants) | Automation | LocalApi (all scopes) |
//! |---|---|---|---|---|---|
//! | plugins, mcp.servers, AI keys/writes, io, oplog, `settings:exec` | ✓ | ✗ | ✗ | ✗ | ✗ |
//! | automation (rule management, runs, preview, log) | ✓ | ✗ | ✗ | ✗ | ✗ |
//! | local API client management | ✓ | ✗ | ✗ | ✗ | ✗ |
//! | reading clips / imports (programs, free paths) | ✓ | ✗ | ✗ | ✗ | ✗ |
//! | modes (public) | ✓ | ✗ | ✓ | ✗ | ✗ |
//! | internal methods (modes internals included) | ✗ | ✗ | ✗ | ✗ | ✗ |
//! | `inbox.add` | ✓ | ✗ | ✓ | ✗ | ✓ |
//! | `inbox.accept` | ✓ | ✗ | ✗ | ✓ (rule action) | ✗ |

use crate::modules::local_api::{self, GRANTABLE_SCOPES};
use crate::testing::TestApp;
use fc_api::method::MethodDescriptor;
use fc_api::v1::local_api::{ApiClientKind, ApproveApiClient, PairRequest};
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::oplog::Actor;
use fc_api::{ErrorCode, Visibility};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Who {
    User,
    Cli,
    System,
    Mcp,
    Plugin,
    Automation,
    LocalApi,
}

const EVERYONE: &[Who] = &[
    Who::User,
    Who::Cli,
    Who::System,
    Who::Mcp,
    Who::Plugin,
    Who::Automation,
    Who::LocalApi,
];

fn users_own(w: Who) -> bool {
    matches!(w, Who::User | Who::Cli | Who::System)
}

/// The sensitive domain of a method, if it is in one.
fn domain(d: &MethodDescriptor) -> Option<&'static str> {
    let n = d.name;
    let p = d.permission;
    Some(if d.visibility == Visibility::Internal {
        "internal"
    } else if d.module == "plugins" || n.starts_with("plugins.") {
        "plugins"
    } else if n.starts_with("mcp.servers.") {
        "mcp.servers"
    } else if p.starts_with("ai_keys") || p.starts_with("ai_requests") || p == "ai:write" {
        "ai writes"
    } else if p.starts_with("io:") {
        "io"
    } else if p.starts_with("oplog:") {
        "oplog"
    } else if d.module == "automation" {
        "automation"
    } else if d.module == "local_api" {
        "local_api"
    } else if n == "reading.clips.extract" || n == "reading.items.import" {
        "reading clips"
    } else if n.starts_with("modes.") {
        "modes"
    } else if n == "inbox.add" || n == "inbox.accept" {
        n
    } else {
        return None;
    })
}

/// Whether `who` reaches a method of `domain`.
fn expected(who: Who, domain: &str) -> bool {
    match domain {
        "internal" => false,
        "modes" => users_own(who) || who == Who::Plugin,
        "inbox.add" => users_own(who) || matches!(who, Who::Plugin | Who::LocalApi),
        "inbox.accept" => users_own(who) || who == Who::Automation,
        _ => users_own(who),
    }
}

fn setup() -> (TestApp, BTreeMap<Who, Actor>) {
    let t = TestApp::mem();
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: local_api::ID.into(),
        enabled: true,
    });
    // A plugin granted everything (tampered rows included).
    let mut all: BTreeSet<String> = catalog().iter().map(|d| d.permission.to_owned()).collect();
    all.extend(["settings:exec", "mcp:write", "ai_keys:write"].map(str::to_owned));
    t.app.registry().set_plugin_grants("com.all", Some(all));
    // A local API client approved with every scope a client may hold.
    let hash = format!("{:064x}", 7);
    let ext = "chrome-extension://abcdefghijklmnop";
    let c = local_api::record_pairing(
        &t.app,
        &PairRequest {
            client_name: "Chrome".into(),
            client_kind: ApiClientKind::BrowserExtension,
            scopes: GRANTABLE_SCOPES.iter().map(|s| (*s).to_owned()).collect(),
        },
        Some(ext),
        &hash,
    )
    .unwrap();
    t.ok::<LocalApiClientsApprove>(ApproveApiClient {
        id: c.id,
        scopes: None,
    });
    let client = local_api::authenticate(&t.app, &hash, Some(ext)).unwrap();
    let actors = BTreeMap::from([
        (Who::User, Actor::User),
        (Who::Cli, Actor::Cli),
        (Who::System, Actor::System),
        (Who::Mcp, Actor::Mcp("claude".into())),
        (Who::Plugin, Actor::Plugin("com.all".into())),
        (
            Who::Automation,
            Actor::Automation("01900000-0000-7000-8000-0000000000aa".into()),
        ),
        (Who::LocalApi, local_api::actor(&client)),
    ]);
    (t, actors)
}

fn permission_denied(t: &TestApp, method: &str, params: Value, actor: &Actor) -> bool {
    matches!(t.app.invoke(method, params, actor), Err(e) if e.code == ErrorCode::PermissionDenied)
}

#[test]
fn every_actor_against_every_sensitive_domain() {
    let (t, actors) = setup();
    let mut checked: BTreeMap<&str, usize> = BTreeMap::new();
    for d in catalog() {
        let Some(dom) = domain(&d) else { continue };
        *checked.entry(dom).or_default() += 1;
        for who in EVERYONE {
            let denied = permission_denied(&t, &d.qualified_name(), json!({}), &actors[who]);
            assert_eq!(
                !denied,
                expected(*who, dom),
                "{who:?} × {} ({dom}, {})",
                d.name,
                d.permission
            );
        }
    }
    for (dom, min) in [
        ("internal", 20),
        ("plugins", 5),
        ("mcp.servers", 3),
        ("ai writes", 3),
        ("io", 5),
        ("oplog", 2),
        ("automation", 9),
        ("local_api", 4),
        ("reading clips", 2),
        ("modes", 8),
        ("inbox.add", 1),
        ("inbox.accept", 1),
    ] {
        assert!(
            checked.get(dom).copied().unwrap_or(0) >= min,
            "{dom}: {checked:?}"
        );
    }

    // `settings:exec`: only the user's own clients name a program the app runs.
    for who in EVERYONE {
        let denied = permission_denied(
            &t,
            "v1.settings.set",
            json!({ "scope": { "kind": "app" }, "key": "reading.ffmpeg_path", "value": "" }),
            &actors[who],
        );
        assert_eq!(!denied, users_own(*who), "{who:?} × settings:exec");
    }
    // Nobody but them holds the user-only permissions, whatever a grant says.
    for p in crate::registry::USER_ONLY_PERMISSIONS {
        for who in EVERYONE {
            assert_eq!(
                t.app.registry().permits(&actors[who], p),
                users_own(*who),
                "{who:?} × {p}"
            );
        }
    }
}
