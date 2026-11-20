//! Phase-3 integration, round 4 (v0.7 plugins next to v0.8 MCP/AI and v0.6 statistics;
//! docs/10 §25): `Registry::permits` is right for **every** actor once real grants exist.
//!
//! | Actor | Holds |
//! |---|---|
//! | `User`, `Cli`, `System` | everything |
//! | `Mcp` | `MCP_PERMISSIONS` only (and only the methods its tools call) |
//! | `Plugin` | its grants, never a forbidden area (`mcp`, `ai_keys`, `ai:write`, `io`, `oplog`, `stats:write`…) |
//! | `Automation`, `LocalApi` | **nothing** until v0.9 defines their grants |

use crate::modules::mcp::MCP_PERMISSIONS;
use crate::modules::plugins::redact::{exposure, Exposure};
use crate::testing::TestApp;
use fc_api::v1::methods::*;
use fc_api::v1::oplog::Actor;
use fc_api::{ApiMethod, ErrorCode, Visibility};
use fc_plugins::permission::api_grantable;
use serde_json::json;
use std::collections::BTreeSet;

/// Every permission some method of the catalog declares, plus ones no method uses yet.
fn all_permissions() -> BTreeSet<String> {
    let mut all: BTreeSet<String> = catalog().iter().map(|d| d.permission.to_owned()).collect();
    for p in [
        "mcp:write",
        "ai_keys:write",
        "io:read",
        "oplog:read",
        "stats:write",
        "plugins:read",
    ] {
        all.insert(p.to_owned());
    }
    all
}

/// What plugins may never reach, whatever a (tampered) grant says.
const NEVER_FOR_PLUGINS: &[&str] = &[
    "mcp:read",
    "mcp:write",
    "ai:write",
    "ai_keys:write",
    "ai_requests:write",
    "io:read",
    "io:write",
    "oplog:read",
    "oplog:write",
    "stats:write",
    "settings:write",
    "modules:write",
    "plugins:read",
    "plugins:write",
    "batch:write",
    "jobs:write",
];

#[test]
fn permits_is_right_for_every_actor() {
    let t = TestApp::mem();
    let reg = t.app.registry();
    let all = all_permissions();
    for p in &all {
        for actor in [Actor::User, Actor::Cli, Actor::System] {
            assert!(reg.permits(&actor, p), "{actor:?} holds {p}");
        }
        assert_eq!(
            reg.permits(&Actor::Mcp("claude".into()), p),
            MCP_PERMISSIONS.contains(&p.as_str()),
            "mcp: {p}"
        );
        // Deny by default until v0.9 defines these actors' grants.
        for actor in [Actor::Automation("a".into()), Actor::LocalApi("c".into())] {
            assert!(!reg.permits(&actor, p), "{actor:?} holds nothing ({p})");
        }
        assert!(
            !reg.permits(&Actor::Plugin("com.none".into()), p),
            "ungranted plugin: {p}"
        );
    }
    // MCP never manages servers, keys, AI, settings, history or plugins (`mcp:read` lists
    // its own tools).
    for p in MCP_PERMISSIONS {
        assert!(
            *p != "mcp:write"
                && !p.starts_with("ai")
                && !p.starts_with("plugins:")
                && !p.starts_with("io:")
                && !p.starts_with("oplog:")
                && *p != "settings:write"
                && *p != "modules:write",
            "{p}"
        );
    }
    // A plugin granted everything (a tampered row included) holds exactly the grantable part.
    reg.set_plugin_grants("com.all", Some(all.clone()));
    let plugin = Actor::Plugin("com.all".into());
    for p in &all {
        assert_eq!(reg.permits(&plugin, p), api_grantable(p), "plugin: {p}");
    }
    for p in NEVER_FOR_PLUGINS {
        assert!(!api_grantable(p), "{p} must not be grantable");
        assert!(!reg.permits(&plugin, p), "plugin: {p}");
    }
    // Grants are per plugin and go away when revoked.
    assert!(!reg.permits(&Actor::Plugin("com.other".into()), "notes:read"));
    reg.set_plugin_grants("com.all", None);
    assert!(!reg.permits(&plugin, "notes:read"));
}

fn denied(t: &TestApp, name: &str, actor: &Actor) -> bool {
    let method = format!("{}.{name}", fc_api::API_VERSION);
    matches!(t.app.invoke(&method, json!({}), actor), Err(e) if e.code == ErrorCode::PermissionDenied)
}

/// Local API clients and automations are refused every public method (reads included) and
/// write nothing.
#[test]
fn local_api_and_automation_are_denied_until_v09() {
    let t = TestApp::mem();
    let before = crate::testing::suite::snapshot(&t);
    for actor in [
        Actor::Automation("rule".into()),
        Actor::LocalApi("anki-connect".into()),
    ] {
        for d in catalog()
            .iter()
            .filter(|d| d.visibility == Visibility::Public)
        {
            assert!(denied(&t, d.name, &actor), "{actor:?} reached {}", d.name);
        }
    }
    assert_eq!(crate::testing::suite::snapshot(&t), before);
}

/// A plugin holding every grantable permission (and tampered forbidden ones) still reaches
/// no MCP server, AI key, AI write, io, oplog, statistics cache or optimizer method — and
/// every other public method passes the permission check.
#[test]
fn plugins_reach_no_mcp_ai_writes_io_oplog_or_optimizer() {
    let t = TestApp::mem();
    t.app
        .registry()
        .set_plugin_grants("com.all", Some(all_permissions()));
    let plugin = Actor::Plugin("com.all".into());
    let must_deny = [
        McpServersList::NAME,
        McpServersAdd::NAME,
        McpServersUpdate::NAME,
        McpServersRemove::NAME,
        McpToolsList::NAME,
        AiProvidersCreate::NAME,
        AiProvidersUpdate::NAME,
        AiProvidersDelete::NAME,
        AiProvidersSetKey::NAME,
        AiProvidersRefreshModels::NAME,
        AiRoutesSet::NAME,
        AiPricesSet::NAME,
        AiPricesDelete::NAME,
        AiRun::NAME,
        AiSourcesExtract::NAME,
        AiNotesAdd::NAME,
        AiNotesUpdate::NAME,
        OplogList::NAME,
        OplogUndo::NAME,
        OplogRedo::NAME,
        StatsRefreshCache::NAME,
        OptimizerRun::NAME,
        BatchExecute::NAME,
    ];
    for name in must_deny {
        assert!(denied(&t, name, &plugin), "plugin reached {name}");
    }
    for d in catalog()
        .iter()
        .filter(|d| d.visibility == Visibility::Public)
    {
        let expected =
            !api_grantable(d.permission) || matches!(exposure(d.name), Exposure::Refused(_));
        assert_eq!(denied(&t, d.name, &plugin), expected, "{}", d.name);
        if d.permission.starts_with("io:") || d.name.starts_with("mcp.") {
            assert!(expected, "{}", d.name);
        }
    }
    // Reads of the new domains are grantable (redacted by `plugins::redact`).
    for name in [
        StatsGetOverview::NAME,
        GoalsList::NAME,
        PlanningExamsList::NAME,
        AiProvidersList::NAME,
    ] {
        assert!(!denied(&t, name, &plugin), "{name}");
    }
}
