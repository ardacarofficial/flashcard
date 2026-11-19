//! `palette` module (v0.4, docs/02 §13): the command palette lists **every public method of
//! the registry**, and keyboard shortcuts are defined here (defaults + user overrides with
//! conflict detection). The frontend only binds what `shortcuts.list` returns.
//!
//! Bindable actions are the UI actions in [`UI_ACTIONS`] plus every public method
//! (`v1.<method>`, global context, unbound by default). Overrides live in the app-scope
//! setting row [`OVERRIDES_KEY`] as `{action: combo | null}` (`null` = explicitly unbound).

use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::ModuleDescriptor;
use crate::registry::{ErasedHandler, Registry};
use crate::support::{module_states, summary};
use fc_api::v1::browse::*;
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::{ApiError, ApiMethod, ErrorCode, Visibility};
use fc_core::settings::SettingScope;
use fc_core::shortcut::{find_conflicts, normalize_combo};
use fc_storage::Reads;
use std::collections::BTreeMap;

/// Module id.
pub const ID: &str = "palette";

/// Setting row holding the user's overrides.
pub const OVERRIDES_KEY: &str = "palette.shortcuts";

/// Built-in UI actions: `(action, context, default combo)`. Labels are `shortcuts.<action>`.
/// A feature adding keyboard actions (e.g. the study screen) appends its rows here.
pub const UI_ACTIONS: &[(&str, ShortcutContext, Option<&str>)] = &[
    ("ui.palette", ShortcutContext::Global, Some("Ctrl+K")),
    ("ui.search", ShortcutContext::Global, Some("Ctrl+F")),
    ("ui.new_note", ShortcutContext::Global, Some("Ctrl+N")),
    ("ui.undo", ShortcutContext::Global, Some("Ctrl+Z")),
    ("ui.redo", ShortcutContext::Global, Some("Ctrl+Shift+Z")),
    ("browse.select_all", ShortcutContext::Browse, Some("Ctrl+A")),
    ("browse.suspend", ShortcutContext::Browse, Some("Ctrl+J")),
    ("browse.info", ShortcutContext::Browse, Some("Ctrl+I")),
    ("browse.delete", ShortcutContext::Browse, Some("Delete")),
];

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.palette.name",
        description_key: "modules.palette.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![crate::module::main_menu(
            "palette.shortcuts",
            "nav.shortcuts",
            "keyboard",
            "/shortcuts",
            95,
        )],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<PaletteListMethod>(palette_list);
    reg.query::<ShortcutsList>(shortcuts_list);
    reg.command::<ShortcutsSet>(shortcuts_set);
    reg.command::<ShortcutsReset>(shortcuts_reset);
    reg.command::<ShortcutsRestore>(shortcuts_restore);
}

type Overrides = BTreeMap<String, Option<String>>;

fn load_overrides(reads: &dyn Reads) -> AppResult<Option<Overrides>> {
    Ok(reads
        .get_setting(&SettingScope::App, OVERRIDES_KEY)?
        .and_then(|v| serde_json::from_value(v).ok()))
}

/// `(action, label_key, context, default)` of every bindable action.
fn actions(registry: &Registry) -> Vec<(String, String, ShortcutContext, Option<String>)> {
    let mut out: Vec<_> = UI_ACTIONS
        .iter()
        .map(|(a, c, d)| {
            (
                (*a).to_owned(),
                format!("shortcuts.{a}"),
                *c,
                d.map(str::to_owned),
            )
        })
        .collect();
    let mut methods: Vec<_> = registry
        .methods()
        .filter(|d| d.visibility == Visibility::Public)
        .map(|d| {
            (
                d.qualified_name(),
                format!("commands.{}", d.name),
                ShortcutContext::Global,
                None,
            )
        })
        .collect();
    methods.sort_by(|a, b| a.0.cmp(&b.0));
    out.extend(methods);
    out
}

fn build(registry: &Registry, overrides: &Overrides) -> ShortcutList {
    let items: Vec<ShortcutDto> = actions(registry)
        .into_iter()
        .map(|(action, label_key, context, default_combo)| ShortcutDto {
            combo: match overrides.get(&action) {
                Some(o) => o.clone(),
                None => default_combo.clone(),
            },
            action,
            label_key,
            context,
            default_combo,
        })
        .collect();
    let ctx_name = |c: ShortcutContext| match c {
        ShortcutContext::Global => "global",
        ShortcutContext::Browse => "browse",
        ShortcutContext::Study => "study",
    };
    let conflicts = find_conflicts(
        items.iter().filter_map(|i| {
            i.combo
                .as_deref()
                .map(|c| (i.action.as_str(), ctx_name(i.context), c))
        }),
        "global",
    )
    .into_iter()
    .map(|(combo, actions)| ShortcutConflict { combo, actions })
    .collect();
    ShortcutList { items, conflicts }
}

fn shortcuts_list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<ShortcutList> {
    Ok(build(
        ctx.registry,
        &load_overrides(ctx.tx)?.unwrap_or_default(),
    ))
}

/// Writes `overrides` (`None` = remove the row) and returns the outcome with the inverse that
/// restores `previous`.
fn store<M: ApiMethod>(
    ctx: &mut CommandCtx<'_>,
    previous: Option<Overrides>,
    overrides: Option<Overrides>,
    summary_param: &str,
) -> AppResult<CommandOutcome<ShortcutList>> {
    match &overrides {
        Some(map) => {
            let v = serde_json::to_value(map).map_err(|e| {
                crate::error::AppError::api(ErrorCode::Internal, e.to_string())
            })?;
            ctx.tx.set_setting(&SettingScope::App, OVERRIDES_KEY, &v, ctx.now)?;
        }
        None => {
            ctx.tx.delete_setting(&SettingScope::App, OVERRIDES_KEY)?;
        }
    }
    let list = build(ctx.registry, &overrides.unwrap_or_default());
    Ok(CommandOutcome {
        response: list,
        inverse: vec![inverse_call::<ShortcutsRestore>(&RestoreShortcuts {
            overrides: previous,
        })?],
        summary: summary::<M>().param("action", summary_param),
        events: vec![DomainEvent::SettingChanged {
            scope: SettingScope::App,
            key: OVERRIDES_KEY.to_owned(),
        }],
    })
}

fn shortcuts_set(req: SetShortcut, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ShortcutList>> {
    let all = actions(ctx.registry);
    let Some((_, _, context, default)) = all.iter().find(|(a, ..)| *a == req.action) else {
        return Err(ApiError::new(
            ErrorCode::NotFoundShortcutAction,
            format!("unknown shortcut action {}", req.action),
        )
        .with_detail("action", req.action.clone())
        .into());
    };
    let combo = match &req.combo {
        None => None,
        Some(raw) => Some(normalize_combo(raw).ok_or_else(|| {
            ApiError::new(ErrorCode::ValidationShortcut, format!("invalid combination {raw:?}"))
                .with_detail("combo", raw.clone())
        })?),
    };
    let previous = load_overrides(ctx.reads())?;
    let current = build(ctx.registry, &previous.clone().unwrap_or_default());
    if let Some(c) = &combo {
        let overlaps = |o: ShortcutContext| {
            o == *context || o == ShortcutContext::Global || *context == ShortcutContext::Global
        };
        if let Some(other) = current
            .items
            .iter()
            .find(|i| i.action != req.action && i.combo.as_ref() == Some(c) && overlaps(i.context))
        {
            return Err(ApiError::new(
                ErrorCode::ConflictShortcut,
                format!("{c} is already bound to {}", other.action),
            )
            .with_detail("combo", c.clone())
            .with_detail("action", other.action.clone())
            .into());
        }
    }
    let mut map = previous.clone().unwrap_or_default();
    if combo == *default {
        map.remove(&req.action);
    } else {
        map.insert(req.action.clone(), combo);
    }
    let next = (!map.is_empty()).then_some(map);
    store::<ShortcutsSet>(ctx, previous, next, &req.action)
}

fn shortcuts_reset(
    req: ResetShortcut,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ShortcutList>> {
    let previous = load_overrides(ctx.reads())?;
    let next = match &req.action {
        None => None,
        Some(a) => {
            let mut map = previous.clone().unwrap_or_default();
            map.remove(a);
            (!map.is_empty()).then_some(map)
        }
    };
    let label = req.action.clone().unwrap_or_else(|| "*".to_owned());
    store::<ShortcutsReset>(ctx, previous, next, &label)
}

fn shortcuts_restore(
    req: RestoreShortcuts,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ShortcutList>> {
    let previous = load_overrides(ctx.reads())?;
    store::<ShortcutsRestore>(ctx, previous, req.overrides, "*")
}

/// Every public method of the registry, with whether it can run without parameters (its
/// request parses from `{}`) and its bound shortcut.
fn palette_list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<PaletteList> {
    let states = module_states(ctx.tx, ctx.registry)?;
    let shortcuts = build(
        ctx.registry,
        &load_overrides(ctx.tx)?.unwrap_or_default(),
    );
    let empty = serde_json::json!({});
    let mut entries: Vec<PaletteEntry> = ctx
        .registry
        .methods()
        .filter(|d| d.visibility == Visibility::Public)
        .map(|d| {
            let qualified = d.qualified_name();
            let direct = ctx
                .registry
                .entry(d.name)
                .is_some_and(|e| match &e.handler {
                    ErasedHandler::Command(h) => h.validate(&empty).is_ok(),
                    ErasedHandler::Query(h) => h.validate(&empty).is_ok(),
                });
            PaletteEntry {
                kind: d.kind,
                module: d.module.to_owned(),
                label_key: format!("commands.{}", d.name),
                direct,
                enabled: states.get(d.module).copied().unwrap_or(false),
                shortcut: shortcuts
                    .items
                    .iter()
                    .find(|i| i.action == qualified)
                    .and_then(|i| i.combo.clone()),
                method: qualified,
            }
        })
        .collect();
    entries.sort_by(|a, b| a.method.cmp(&b.method));
    Ok(PaletteList { entries })
}
