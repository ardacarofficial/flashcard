//! Plugin permissions (docs/05 §4). Listed in the manifest, approved by the user at install
//! (any subset), revocable later. The check is at host-function level; for API calls the
//! needed permission is the method's own (`MethodDescriptor::permission`, e.g. `notes:write`)
//! — the plugin API is the public API filtered by these grants.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Fixed host permissions (not tied to an API method).
pub const HOST_PERMISSIONS: &[&str] = &[
    "ai:invoke",
    "stt:invoke",
    "translate:invoke",
    "audio:record",
    "fs:pick",
    "models:manage",
    "tts:provide",
    "card_types:provide",
    // Its own private storage (`kv.set`; reading its own storage needs nothing). Bounded by
    // a per-plugin quota the user sets (`plugins.kv_max_keys`, `plugins.kv_max_mb`).
    "storage:kv",
];

/// UI contribution points (`ui:<point>`).
pub const UI_POINTS: &[&str] = &[
    "study_mode",
    "view",
    "menu",
    "sidebar",
    "card_toolbar",
    "command",
    "settings",
    // `ui.notify`: notices shown to the user (rate limited, docs/05 §12).
    "notify",
];

/// API areas a plugin may never be granted, in either access:
///
/// * `plugins`: plugin management (install, grants, limits).
/// * `io`: import/export/backup methods take free file paths (reads included: preview and
///   inspect open any file); plugins get files only through `fs:pick` (the user chooses, the
///   plugin sees bytes and a file name, never a path).
/// * `mcp`, `ai_keys`, `ai_requests`: external tool servers, API keys, paid AI calls (plugins
///   use the `ai:invoke` host permission).
pub const FORBIDDEN_API_AREAS: &[&str] = &[
    "plugins",
    "io",
    "mcp",
    "ai_keys",
    "ai_requests",
    // v0.9: a rule acts later as the automation actor — creating one would lend a plugin
    // that authority.
    "automation",
];

/// Single API permissions a plugin may never be granted (reading these areas is fine):
///
/// * `settings:write`, `modules:write`: app-wide switches, including the sandbox limits
///   (`plugins.fuel_millions`…). A plugin's own settings go through the `settings.set` host
///   function (its namespace and schema only).
/// * `oplog:write`: undo/redo belong to the user.
/// * `ai:write`: AI providers and routes (could redirect requests and keys).
/// * `oplog:read`: the history holds every domain's data (payloads and inverses of notes,
///   other plugins' settings and storage…); no single read permission covers it.
/// * `batch:write`: runs many calls as one, bypassing the per-call result filtering and the
///   `before:` hooks of the calls inside; a plugin makes the calls one by one instead (one
///   undo step anyway).
/// * `jobs:write`: cancelling jobs (backups, exports) acts on areas plugins never reach.
/// * `stats:write`: rebuilding the statistics cache is whole-history work (v0.6); the
///   optimizer (`optimizer.run`, `decks:write`) is refused by method in the app.
pub const FORBIDDEN_API_PERMISSIONS: &[&str] = &[
    "settings:write",
    "modules:write",
    "oplog:write",
    "oplog:read",
    "ai:write",
    "batch:write",
    "jobs:write",
    "stats:write",
];

/// Methods no plugin may hook, whatever it was granted: a `before:` hook rewrites the
/// *user's* call, so a hook here would act with the user's authority (approve its own
/// permissions, redirect an export or restore, change settings, run a rewritten batch).
pub const FORBIDDEN_HOOK_PREFIXES: &[&str] = &[
    "plugins.",
    "oplog.",
    "modules.",
    "settings.",
    "shortcuts.",
    "io.",
    "batch.",
    "jobs.",
    "mcp.",
    "ai.providers.",
    "ai.prices.",
    "ai.routes.",
    "automation.",
];

/// Whether permission `p` (`<area>:<access>`) may be granted to a plugin at all.
pub fn api_grantable(p: &str) -> bool {
    let area = p.split_once(':').map_or(p, |(a, _)| a);
    !FORBIDDEN_API_AREAS.contains(&area) && !FORBIDDEN_API_PERMISSIONS.contains(&p)
}

/// Whether a plugin may hook `method`, whose own permission is `method_permission`. The
/// method's permission must be grantable to plugins too, and the plugin must hold it (checked
/// by the host): a hook never gives more than calling the method directly would.
pub fn hook_target_allowed(method: &str, method_permission: &str) -> Result<(), &'static str> {
    if FORBIDDEN_HOOK_PREFIXES
        .iter()
        .any(|p| method.starts_with(p))
    {
        return Err("this method cannot be hooked by plugins");
    }
    if !api_grantable(method_permission) {
        return Err("the hooked method needs a permission plugins cannot get");
    }
    Ok(())
}

/// A hook a plugin subscribes to (`hooks:<point>`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HookPoint {
    /// Before a command runs; may change its params or reject it (`before:<method>`).
    Before(String),
    /// After a command committed (`after:<method>`).
    After(String),
    /// After a domain event was published (`event:<type>`, e.g. `event:note_created`).
    Event(String),
}

impl HookPoint {
    /// `before:notes.create` / `after:…` / `event:…`.
    pub fn id(&self) -> String {
        match self {
            Self::Before(m) => format!("before:{m}"),
            Self::After(m) => format!("after:{m}"),
            Self::Event(e) => format!("event:{e}"),
        }
    }

    fn parse(s: &str) -> Option<Self> {
        let (kind, rest) = s.split_once(':')?;
        if rest.is_empty()
            || !rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_' || b == b'.')
        {
            return None;
        }
        match kind {
            "before" => Some(Self::Before(rest.to_owned())),
            "after" => Some(Self::After(rest.to_owned())),
            "event" => Some(Self::Event(rest.to_owned())),
            _ => None,
        }
    }
}

/// What a permission is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionKind {
    /// An API permission (`<area>:<access>`, as declared by methods).
    Api {
        /// Area (`notes`).
        area: String,
        /// Access (`read`, `write`).
        access: String,
    },
    /// A fixed host permission ([`HOST_PERMISSIONS`]).
    Host,
    /// A hook subscription.
    Hook(HookPoint),
    /// HTTP to one host (`network:<host>`).
    Network(String),
    /// A UI contribution point ([`UI_POINTS`]).
    Ui(String),
}

/// A validated permission string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Permission(String);

/// Why a permission string was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid permission {permission:?}: {reason}")]
pub struct PermissionError {
    /// The string.
    pub permission: String,
    /// Reason.
    pub reason: String,
}

fn ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
}

fn host_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        && !s.starts_with('.')
        && !s.ends_with('.')
}

impl Permission {
    /// Parses and classifies `s`.
    pub fn parse(s: &str) -> Result<Self, PermissionError> {
        let p = Self(s.to_owned());
        p.kind_checked()?;
        Ok(p)
    }

    /// The string form.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn kind_checked(&self) -> Result<PermissionKind, PermissionError> {
        let fail = |reason: &str| PermissionError {
            permission: self.0.clone(),
            reason: reason.to_owned(),
        };
        let (area, rest) = self
            .0
            .split_once(':')
            .ok_or_else(|| fail("expected <area>:<access>"))?;
        if HOST_PERMISSIONS.contains(&self.0.as_str()) {
            return Ok(PermissionKind::Host);
        }
        match area {
            "hooks" => HookPoint::parse(rest)
                .map(PermissionKind::Hook)
                .ok_or_else(|| fail("expected hooks:before:<method>, hooks:after:<method> or hooks:event:<type>")),
            "network" => {
                if host_name(rest) {
                    Ok(PermissionKind::Network(rest.to_owned()))
                } else {
                    Err(fail("expected network:<host name>"))
                }
            }
            "ui" => {
                if UI_POINTS.contains(&rest) {
                    Ok(PermissionKind::Ui(rest.to_owned()))
                } else {
                    Err(fail("unknown UI contribution point"))
                }
            }
            _ if !api_grantable(&self.0) => Err(fail("not grantable to plugins")),
            _ if ident(area) && (rest == "read" || rest == "write") => Ok(PermissionKind::Api {
                area: area.to_owned(),
                access: rest.to_owned(),
            }),
            _ => Err(fail("unknown permission")),
        }
    }

    /// Classification (always valid for a constructed value).
    pub fn kind(&self) -> PermissionKind {
        self.kind_checked().unwrap_or(PermissionKind::Host)
    }

    /// Hook point, if this is a hook subscription.
    pub fn hook(&self) -> Option<HookPoint> {
        match self.kind() {
            PermissionKind::Hook(h) => Some(h),
            _ => None,
        }
    }

    /// Sensitive permissions are shown apart at install and need a separate approval
    /// (docs/05 §4: microphone, network, files, paid AI).
    pub fn is_sensitive(&self) -> bool {
        matches!(self.kind(), PermissionKind::Network(_))
            || matches!(self.0.as_str(), "audio:record" | "fs:pick" | "ai:invoke")
    }
}

impl TryFrom<String> for Permission {
    type Error = PermissionError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}

impl From<Permission> for String {
    fn from(p: Permission) -> Self {
        p.0
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        let k = |s: &str| Permission::parse(s).map(|p| p.kind());
        assert_eq!(
            k("notes:write"),
            Ok(PermissionKind::Api {
                area: "notes".into(),
                access: "write".into()
            })
        );
        assert_eq!(k("tts:provide"), Ok(PermissionKind::Host));
        assert_eq!(k("storage:kv"), Ok(PermissionKind::Host));
        assert_eq!(
            k("hooks:before:notes.create"),
            Ok(PermissionKind::Hook(HookPoint::Before(
                "notes.create".into()
            )))
        );
        assert_eq!(
            k("hooks:event:note_created"),
            Ok(PermissionKind::Hook(HookPoint::Event(
                "note_created".into()
            )))
        );
        assert_eq!(
            k("network:api.example.com"),
            Ok(PermissionKind::Network("api.example.com".into()))
        );
        assert_eq!(
            k("ui:study_mode"),
            Ok(PermissionKind::Ui("study_mode".into()))
        );
        for bad in [
            "plugins:write",
            "plugins:read",
            "io:read",
            "io:write",
            "settings:write",
            "modules:write",
            "oplog:write",
            "mcp:write",
            "ai_keys:write",
            "ai_requests:write",
            "ai:write",
            "oplog:read",
            "batch:write",
            "jobs:write",
            "notes",
            "notes:delete",
            "hooks:during:x",
            "network:",
            "network:http://x",
            "ui:everything",
            "NOTES:read",
        ] {
            assert!(Permission::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn hook_targets() {
        assert_eq!(hook_target_allowed("notes.create", "notes:write"), Ok(()));
        for ok in ["ai:read", "settings:read", "modules:read", "jobs:read"] {
            assert!(Permission::parse(ok).is_ok(), "{ok}");
        }
        for (method, perm) in [
            ("plugins.set_permissions", "plugins:write"),
            ("plugins.install", "plugins:write"),
            ("io.export.start", "io:write"),
            ("io.backup.restore", "io:write"),
            ("settings.set", "settings:write"),
            ("shortcuts.set", "settings:write"),
            ("modules.set_enabled", "modules:write"),
            ("oplog.undo", "oplog:write"),
            ("batch.execute", "batch:write"),
            ("mcp.servers.add", "mcp:write"),
            ("ai.providers.set_key", "ai_keys:write"),
            ("ai.routes.set", "ai:write"),
            ("ai.run", "ai_requests:write"),
            // A future method in a forbidden area, whatever its name.
            ("x.y", "io:write"),
        ] {
            assert!(hook_target_allowed(method, perm).is_err(), "{method}");
        }
    }

    #[test]
    fn sensitive() {
        for s in [
            "network:example.com",
            "audio:record",
            "fs:pick",
            "ai:invoke",
        ] {
            assert!(
                Permission::parse(s)
                    .map(|p| p.is_sensitive())
                    .unwrap_or(false),
                "{s}"
            );
        }
        assert!(!Permission::parse("notes:read")
            .map(|p| p.is_sensitive())
            .unwrap_or(true));
    }
}
