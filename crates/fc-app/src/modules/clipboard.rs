//! `clipboard` module (v0.9, docs/06 §7): suggests copied text as a card. **Off by default**
//! (opt-in, privacy): while the module is off the desktop shell does not read the clipboard
//! at all. When on, the shell hands each new clipboard text to `clipboard.check`, which keeps
//! nothing and only answers whether to show the "add as a card?" suggestion (the filter is
//! `fc_core::inbox::clipboard`: length, lines, and never anything that looks like a password,
//! key, token or card number; on Windows content the OS marks as private is skipped by the
//! shell before it gets here). Accepting the suggestion is an ordinary `inbox.add` with
//! `via: clipboard`.

use crate::error::AppResult;
use crate::handler::QueryCtx;
use crate::module::ModuleDescriptor;
use crate::registry::Registry;
use crate::settings::resolve_stored;
use fc_api::v1::inbox::{CheckClipboard, ClipboardVerdict};
use fc_api::v1::methods::*;
use fc_core::inbox::clipboard::{check as filter_text, ClipboardFilter};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use serde_json::json;

/// Module id.
pub const ID: &str = "clipboard";

/// Setting: fewest characters a suggestion may have.
pub const MIN_CHARS: &str = "clipboard.min_chars";
/// Setting: most characters a suggestion may have.
pub const MAX_CHARS: &str = "clipboard.max_chars";

fn int_setting(key: &str, default: i64, min: i64, max: i64) -> SettingDef {
    SettingDef {
        key: key.to_owned(),
        kind: SettingKind::Integer {
            min: Some(min),
            max: Some(max),
        },
        default: json!(default),
        label_key: format!("settings.{key}.label"),
        description_key: Some(format!("settings.{key}.description")),
        advanced: true,
        scopes: vec![SettingScopeKind::App],
    }
}

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    let d = ClipboardFilter::default();
    ModuleDescriptor {
        id: ID,
        name_key: "modules.clipboard.name",
        description_key: "modules.clipboard.description",
        enabled_by_default: false,
        can_disable: true,
        settings: vec![
            int_setting(MIN_CHARS, i64::try_from(d.min_chars).unwrap_or(2), 1, 100),
            int_setting(MAX_CHARS, i64::try_from(d.max_chars).unwrap_or(200), 10, 2000),
        ],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<ClipboardCheck>(check);
}

fn setting(ctx: &QueryCtx<'_>, key: &str, default: usize) -> AppResult<usize> {
    Ok(resolve_stored(ctx.tx, &SettingScope::App, key)?
        .and_then(|(v, _)| v.as_u64())
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(default))
}

fn check(req: CheckClipboard, ctx: &QueryCtx<'_>) -> AppResult<ClipboardVerdict> {
    let d = ClipboardFilter::default();
    let filter = ClipboardFilter {
        min_chars: setting(ctx, MIN_CHARS, d.min_chars)?,
        max_chars: setting(ctx, MAX_CHARS, d.max_chars)?,
    };
    Ok(match filter_text(&req.text, filter) {
        Ok(text) => ClipboardVerdict {
            suggest: true,
            text,
            reason: None,
        },
        Err(why) => ClipboardVerdict {
            suggest: false,
            text: String::new(),
            reason: Some(why.reason().to_owned()),
        },
    })
}
