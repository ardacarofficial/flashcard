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
];

/// API areas a plugin may never be granted (plugin management: a plugin cannot install
/// plugins or change grants).
pub const FORBIDDEN_API_AREAS: &[&str] = &["plugins"];

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
            _ if FORBIDDEN_API_AREAS.contains(&area) => Err(fail("not grantable to plugins")),
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
