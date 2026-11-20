//! `themes` module (v1.0, docs/02 §31): colour scheme (`app.theme`, declared by `core`),
//! preset (`themes.preset`) and custom accent (`themes.accent`). The colours themselves are
//! computed by `fc_core::theme`; the frontend applies the returned tokens as CSS variables.
//!
//! Switched off, the app keeps the CSS defaults (the `default` preset) and the light/dark/
//! system choice of `app.theme`.

use crate::error::AppResult;
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{choice_setting, settings_page, ModuleDescriptor};
use crate::registry::Registry;
use crate::support::summary;
use crate::{inverse_call, AppError};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::{SettingsReset, SettingsSet, ThemesCurrent, ThemesPresets, ThemesSet};
use fc_api::v1::settings::{ResetSetting, SetSetting};
use fc_api::v1::themes::*;
use fc_api::InvokeRequest;
use fc_core::error::CoreError;
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::theme::{self, Rgb, Scheme};
use fc_storage::Reads;
use serde_json::Value;

/// Module id.
pub const ID: &str = "themes";
/// Setting: colour scheme (declared by the `core` module).
pub const MODE: &str = "app.theme";
/// Setting: preset id.
pub const PRESET: &str = "themes.preset";
/// Setting: custom accent `#rrggbb` (`""` = the preset's).
pub const ACCENT: &str = "themes.accent";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    let accent = SettingDef {
        key: ACCENT.to_owned(),
        kind: SettingKind::Text { max_len: Some(7) },
        default: Value::String(String::new()),
        label_key: format!("settings.{ACCENT}.label"),
        description_key: Some(format!("settings.{ACCENT}.description")),
        advanced: false,
        scopes: vec![SettingScopeKind::App],
    };
    ModuleDescriptor {
        id: ID,
        name_key: "modules.themes.name",
        description_key: "modules.themes.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![
            choice_setting(PRESET, &theme::preset_ids(), theme::DEFAULT_PRESET),
            accent,
        ],
        ui: vec![settings_page(
            "themes.appearance",
            "settings.pages.appearance",
            "/settings/appearance",
            1,
        )],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<ThemesCurrent>(current);
    reg.query::<ThemesPresets>(presets);
    reg.command::<ThemesSet>(set);
}

fn stored(reads: &dyn Reads, key: &str) -> AppResult<Option<Value>> {
    Ok(reads.get_setting(&SettingScope::App, key)?)
}

fn mode_of(v: Option<&Value>) -> ThemeMode {
    match v.and_then(Value::as_str) {
        Some("light") => ThemeMode::Light,
        Some("dark") => ThemeMode::Dark,
        _ => ThemeMode::System,
    }
}

fn mode_value(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::System => "system",
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
    }
}

/// The effective theme from the stored settings.
fn effective(reads: &dyn Reads) -> AppResult<ThemeDto> {
    let mode = mode_of(stored(reads, MODE)?.as_ref());
    let preset = stored(reads, PRESET)?;
    let accent = stored(reads, ACCENT)?;
    let t = theme::resolve(
        preset
            .as_ref()
            .and_then(Value::as_str)
            .unwrap_or(theme::DEFAULT_PRESET),
        accent.as_ref().and_then(Value::as_str),
    );
    Ok(ThemeDto {
        mode,
        preset: t.preset.to_owned(),
        accent: t.accent,
        light: t.light,
        dark: t.dark,
    })
}

fn current(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<ThemeDto> {
    effective(ctx.tx)
}

fn presets(_: Empty, _ctx: &QueryCtx<'_>) -> AppResult<ThemePresetList> {
    Ok(ThemePresetList {
        presets: theme::PRESETS
            .iter()
            .map(|p| ThemePresetDto {
                id: p.id.to_owned(),
                name_key: format!("themes.preset.{}", p.id),
                light: theme::tokens(p, Scheme::Light, None),
                dark: theme::tokens(p, Scheme::Dark, None),
            })
            .collect(),
    })
}

fn invalid(key: &str, reason: &str) -> AppError {
    CoreError::InvalidSettingValue {
        key: key.to_owned(),
        reason: reason.to_owned(),
    }
    .into()
}

fn set(req: SetTheme, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ThemeDto>> {
    let mut writes: Vec<(&str, Value)> = Vec::new();
    if let Some(mode) = req.mode {
        writes.push((MODE, Value::String(mode_value(mode).to_owned())));
    }
    if let Some(preset) = &req.preset {
        if theme::preset(preset).is_none() {
            return Err(invalid(PRESET, "unknown preset"));
        }
        writes.push((PRESET, Value::String(preset.clone())));
    }
    if let Some(accent) = &req.accent {
        let value = if accent.trim().is_empty() {
            String::new()
        } else {
            Rgb::parse(accent)
                .ok_or_else(|| invalid(ACCENT, "not a #rrggbb colour"))?
                .hex()
        };
        writes.push((ACCENT, Value::String(value)));
    }
    if writes.is_empty() {
        return Err(crate::support::invalid("nothing to change"));
    }
    let mut inverse: Vec<InvokeRequest> = Vec::new();
    let mut events = Vec::new();
    for (key, value) in writes {
        let old = stored(ctx.reads(), key)?;
        if old.as_ref() == Some(&value) {
            continue;
        }
        ctx.tx.set_setting(&SettingScope::App, key, &value, ctx.now)?;
        inverse.push(match old {
            Some(value) => inverse_call::<SettingsSet>(&SetSetting {
                scope: SettingScope::App,
                key: key.to_owned(),
                value,
            })?,
            None => inverse_call::<SettingsReset>(&ResetSetting {
                scope: SettingScope::App,
                key: key.to_owned(),
            })?,
        });
        events.push(DomainEvent::SettingChanged {
            scope: SettingScope::App,
            key: key.to_owned(),
        });
    }
    Ok(CommandOutcome {
        response: effective(ctx.reads())?,
        inverse,
        summary: summary::<ThemesSet>(),
        events,
    })
}

#[cfg(test)]
mod tests;
