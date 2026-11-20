//! `plugins` module (v0.7, docs/05): install from a package file, permissions, enable /
//! disable / uninstall (all undoable), plugin settings and storage, contributed commands,
//! views and study modes (declarative UI), the model download manager.
//!
//! The WASM side lives in `fc-plugins`; [`host`] is the bridge: it loads plugins, registers
//! their contributions in the [`Registry`] at runtime, runs hooks around the dispatcher and
//! serves host calls (the public API filtered by the plugin's grants, checked by the same
//! `ensure_invocable` as every other caller).

pub(crate) mod engines;
pub(crate) mod host;
mod ui;

#[cfg(test)]
mod tests;

pub use host::PluginHost;
pub(crate) use host::Hooks;

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::jobs::{JobContext, JobHandler};
use crate::module::{choice_setting, main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::support::{err_id, summary};
use crate::App;
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::modules::{ModuleDto, UiContribution, UiSlot};
use fc_api::v1::oplog::Actor;
use fc_api::v1::plugins::*;
use fc_api::{ApiError, ErrorCode, Visibility};
use fc_core::settings::{validate_setting_value, SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_plugins::manifest::Manifest;
use fc_plugins::models::{FileFetcher, ModelError, ModelStatus, ModelStore};
use fc_plugins::package::{Package, PackageError};
use fc_plugins::source::{FileSource, PluginRegistrySource, SourceError};
use fc_plugins::permission::PermissionKind;
use fc_plugins::{HookPoint, ManifestError, Permission};
use fc_storage::repo::{PluginPackageRecord, PluginRecord};
use fc_storage::Reads;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Module id.
pub const ID: &str = "plugins";

/// Setting: fuel per call, in millions.
pub const SETTING_FUEL: &str = "plugins.fuel_millions";
/// Setting: time limit per call (ms, time inside the plugin).
pub const SETTING_TIMEOUT: &str = "plugins.timeout_ms";
/// Setting: memory cap per instance (MiB).
pub const SETTING_MEMORY: &str = "plugins.memory_mb";
/// Setting: failures in a row before a plugin is switched off.
pub const SETTING_MAX_FAILURES: &str = "plugins.max_failures";
/// Setting: what a failing `before:` hook does (`continue` without it, or `reject`).
pub const SETTING_HOOK_FAILURE: &str = "plugins.before_hook_failure";

fn int_setting(key: &str, default: i64, min: i64, max: i64, advanced: bool) -> SettingDef {
    SettingDef {
        key: key.to_owned(),
        kind: SettingKind::Integer {
            min: Some(min),
            max: Some(max),
        },
        default: json!(default),
        label_key: format!("settings.{key}.label"),
        description_key: Some(format!("settings.{key}.description")),
        advanced,
        scopes: vec![SettingScopeKind::App],
    }
}

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.plugins.name",
        description_key: "modules.plugins.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![
            int_setting(SETTING_TIMEOUT, 3000, 10, 60_000, true),
            int_setting(SETTING_FUEL, 2000, 1, 1_000_000, true),
            int_setting(SETTING_MEMORY, 64, 1, 2048, true),
            int_setting(SETTING_MAX_FAILURES, 3, 1, 100, false),
            choice_setting(SETTING_HOOK_FAILURE, &["continue", "reject"], "continue"),
        ],
        ui: vec![main_menu("plugins.manager", "nav.plugins", "puzzle", "/plugins", 92)],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.action::<PluginsList>(list);
    reg.query::<PluginsInspect>(inspect);
    reg.command::<PluginsInstall>(install);
    reg.command::<PluginsUninstall>(uninstall);
    reg.command::<PluginsSetEnabled>(set_enabled);
    reg.command::<PluginsSetPermissions>(set_permissions);
    reg.query::<PluginsSettingsGet>(settings_get);
    reg.command::<PluginsSettingsSet>(settings_set);
    reg.query::<PluginsLocales>(locales);
    reg.action::<PluginsLogs>(logs);
    reg.action::<PluginsRun>(ui::run);
    reg.action::<PluginsUiOpen>(ui::open);
    reg.action::<PluginsUiAct>(ui::act);
    reg.action::<PluginsUiClose>(ui::close);
    reg.action::<PluginsModelsList>(models_list);
    reg.command::<PluginsModelsDownload>(models_download);
    reg.action::<PluginsModelsDelete>(models_delete);
    reg.command::<PluginsRestore>(restore);
    reg.command::<PluginsKvSet>(kv_set);
    reg.command::<PluginsRecordFailure>(record_failure);
    reg.command::<PluginsRollback>(rollback);
    reg.job(Arc::new(ModelDownloadJob));
}

// ------------------------------------------------------------------ errors and helpers

pub(crate) fn not_installed(id: &str) -> AppError {
    err_id(ErrorCode::NotFoundPlugin, format!("plugin {id} is not installed"), id)
}

pub(crate) fn inactive(id: &str) -> AppError {
    err_id(
        ErrorCode::ConflictPluginInactive,
        format!("plugin {id} is disabled, broken or lacks the permission"),
        id,
    )
}

pub(crate) fn no_contribution(plugin_id: &str, id: &str) -> AppError {
    AppError::Api(
        ApiError::new(
            ErrorCode::NotFoundPluginContribution,
            format!("plugin {plugin_id} has no {id}"),
        )
        .with_detail("plugin_id", plugin_id)
        .with_detail("id", id),
    )
}

/// The loaded, active plugin `id`.
pub(crate) fn require_active(host: &PluginHost, id: &str) -> AppResult<Arc<host::LoadedPlugin>> {
    match host.get_any(id) {
        None => Err(not_installed(id)),
        Some(p) if p.active => Ok(p),
        Some(_) => Err(inactive(id)),
    }
}

fn package_error(e: PackageError) -> AppError {
    let api = match e {
        PackageError::Invalid(reason) => {
            ApiError::new(ErrorCode::ValidationPluginPackage, reason.clone()).with_detail("reason", reason)
        }
        PackageError::Manifest(ManifestError::Parse(reason)) => {
            ApiError::new(ErrorCode::ValidationPluginManifest, reason.clone())
                .with_detail("field", "")
                .with_detail("reason", reason)
        }
        PackageError::Manifest(ManifestError::Invalid { field, reason }) => {
            ApiError::new(ErrorCode::ValidationPluginManifest, format!("{field}: {reason}"))
                .with_detail("field", field)
                .with_detail("reason", reason)
        }
        PackageError::Manifest(ManifestError::UnsupportedApiVersion(v)) => ApiError::new(
            ErrorCode::ValidationPluginApiVersion,
            format!("plugin API {v} is not supported"),
        )
        .with_detail("api_version", v),
        PackageError::Manifest(ManifestError::AppTooOld { required, current }) => ApiError::new(
            ErrorCode::ValidationPluginAppVersion,
            format!("needs app {required}"),
        )
        .with_detail("required", required)
        .with_detail("current", current),
    };
    AppError::Api(api)
}

/// Reads a package through the store-source interface (built-in source: local files; a
/// remote source plugs in behind the same trait, docs/08 K3).
fn read_package(path: &str) -> AppResult<Package> {
    let source = FileSource::new(fc_plugins::APP_VERSION, vec![]);
    let bytes = source.download(path).map_err(source_error)?;
    Package::read(&bytes, fc_plugins::APP_VERSION).map_err(package_error)
}

fn source_error(e: SourceError) -> AppError {
    match e {
        SourceError::Package(p) => package_error(p),
        SourceError::NotFound(path) => AppError::Api(
            ApiError::new(ErrorCode::IoFile, format!("{path} not found")).with_detail("path", path),
        ),
        SourceError::Io(msg) => AppError::api(ErrorCode::IoFile, msg),
    }
}

fn manifest_of(record: &PluginRecord) -> Option<Manifest> {
    serde_json::from_value(record.manifest.clone()).ok()
}

fn live(reads: &dyn Reads, id: &str) -> AppResult<PluginRecord> {
    reads
        .get_plugin(id)?
        .filter(|p| p.deleted_at.is_none())
        .ok_or_else(|| not_installed(id))
}

fn to_json<T: serde::Serialize>(v: &T) -> AppResult<Value> {
    serde_json::to_value(v).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
}

fn changed(id: &str) -> DomainEvent {
    DomainEvent::PluginsChanged {
        plugin_id: id.to_owned(),
        auto_disabled: false,
    }
}

/// Every API permission and hook target must exist in this app (a typo is an error at
/// install, not a silent no-op).
fn check_against_registry(registry: &Registry, m: &Manifest) -> AppResult<()> {
    let bad = |p: &Permission, reason: &str| {
        AppError::Api(
            ApiError::new(ErrorCode::ValidationPluginManifest, format!("{p}: {reason}"))
                .with_detail("field", "permissions")
                .with_detail("reason", reason)
                .with_detail("permission", p.as_str()),
        )
    };
    for p in &m.permissions {
        match p.kind() {
            PermissionKind::Api { .. } => {
                if !registry.methods().any(|d| d.permission == p.as_str()) {
                    return Err(bad(p, "no method needs this permission"));
                }
            }
            PermissionKind::Hook(HookPoint::Before(method) | HookPoint::After(method)) => {
                let ok = registry.descriptor(&method).is_some_and(|d| {
                    d.visibility == Visibility::Public && d.kind == fc_api::MethodKind::Command
                });
                if !ok {
                    return Err(bad(p, "hooks attach to public commands"));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn checked_grant(m: &Manifest, grant: &[String]) -> AppResult<Vec<String>> {
    let mut out = BTreeSet::new();
    for g in grant {
        if !m.requests(g) {
            return Err(AppError::Api(
                ApiError::new(
                    ErrorCode::ValidationPluginPermission,
                    format!("{g} was not requested by the plugin"),
                )
                .with_detail("permission", g.clone()),
            ));
        }
        out.insert(g.clone());
    }
    Ok(out.into_iter().collect())
}

/// Active contributions (permission granted).
fn contributions(m: &Manifest, granted: &dyn Fn(&str) -> bool) -> PluginContributions {
    let c = &m.contributes;
    let id = &m.id;
    let if_granted = |p: &str, v: &[String]| if granted(p) { v.to_vec() } else { vec![] };
    PluginContributions {
        commands: if_granted("ui:command", &c.commands),
        study_modes: if_granted("ui:study_mode", &c.study_modes),
        views: if_granted("ui:view", &c.views),
        card_types: if granted("card_types:provide") {
            c.card_types.iter().map(|t| format!("plugin.{id}.{t}")).collect()
        } else {
            vec![]
        },
        tts: if granted("tts:provide") {
            c.tts.iter().map(|t| format!("plugin.{id}.{t}")).collect()
        } else {
            vec![]
        },
        menu_items: if granted("ui:menu") {
            c.menu_items
                .iter()
                .map(|i| PluginMenuItem {
                    id: i.id.clone(),
                    view: i.view.clone(),
                    icon: i.icon.clone(),
                })
                .collect()
        } else {
            vec![]
        },
        sidebar_panels: if granted("ui:sidebar") {
            c.sidebar_panels
                .iter()
                .map(|p| PluginPanel {
                    id: p.id.clone(),
                    view: p.view.clone(),
                })
                .collect()
        } else {
            vec![]
        },
        card_toolbar: if granted("ui:card_toolbar") && granted("ui:command") {
            c.card_toolbar
                .iter()
                .map(|t| PluginToolbarButton {
                    id: t.id.clone(),
                    command: t.command.clone(),
                    icon: t.icon.clone(),
                })
                .collect()
        } else {
            vec![]
        },
        hooks: m
            .permissions
            .iter()
            .filter(|p| granted(p.as_str()))
            .filter_map(|p| p.hook().map(|h| h.id()))
            .collect(),
    }
}

fn dto(record: &PluginRecord, has_settings: bool, loaded: Option<&host::LoadedPlugin>) -> PluginDto {
    let manifest = manifest_of(record);
    let granted = |p: &str| record.granted.iter().any(|g| g == p);
    let broken = loaded.and_then(|l| l.broken.clone());
    let status = if !record.enabled {
        PluginStatus::Disabled
    } else if record.auto_disabled {
        PluginStatus::AutoDisabled
    } else if broken.is_some() {
        PluginStatus::Broken
    } else {
        PluginStatus::Active
    };
    PluginDto {
        id: record.id.clone(),
        name: manifest.as_ref().map_or_else(|| record.id.clone(), |m| m.name.clone()),
        version: record.version.clone(),
        description: manifest.as_ref().map(|m| m.description.clone()).unwrap_or_default(),
        authors: manifest.as_ref().map(|m| m.authors.clone()).unwrap_or_default(),
        api_version: manifest.as_ref().map(|m| m.api_version.clone()).unwrap_or_default(),
        enabled: record.enabled,
        status,
        failures: record.failures,
        last_error: record.last_error.clone().or(broken),
        permissions: manifest
            .as_ref()
            .map(|m| {
                m.permissions
                    .iter()
                    .map(|p| PermissionDto {
                        permission: p.as_str().to_owned(),
                        granted: granted(p.as_str()),
                        sensitive: p.is_sensitive(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        contributes: manifest
            .as_ref()
            .map(|m| contributions(m, &granted))
            .unwrap_or_default(),
        has_settings,
        installed_at: record.installed_at,
        updated_at: record.updated_at,
    }
}

fn package_meta(reads: &dyn Reads, record: &PluginRecord) -> AppResult<Option<PluginPackageRecord>> {
    Ok(reads.get_plugin_package(&record.id, &record.package_sha256, false)?)
}

fn schema_of(reads: &dyn Reads, record: &PluginRecord) -> AppResult<Vec<SettingDef>> {
    Ok(package_meta(reads, record)?
        .and_then(|p| serde_json::from_value(p.settings).ok())
        .unwrap_or_default())
}

/// Resolved settings of plugin `id` (stored value, else default).
pub(crate) fn settings_values(
    reads: &dyn Reads,
    id: &str,
    schema: &[SettingDef],
) -> AppResult<BTreeMap<String, Value>> {
    let stored: BTreeMap<String, Value> = reads
        .list_settings(&SettingScope::Plugin(id.to_owned()))?
        .into_iter()
        .collect();
    Ok(schema
        .iter()
        .map(|d| {
            (
                d.key.clone(),
                stored.get(&d.key).cloned().unwrap_or_else(|| d.default.clone()),
            )
        })
        .collect())
}

/// Model files live in `<collection folder>/models/<plugin id>/<model id>`.
pub(crate) fn model_store(app: &App) -> Option<ModelStore> {
    app.data_dir().map(|d| ModelStore::new(d.join("models")))
}

// ------------------------------------------------------------------ management

fn list(_: Empty, app: &App, _actor: &Actor) -> AppResult<PluginList> {
    let tx = app.store().read()?;
    let mut plugins = Vec::new();
    for record in tx.list_plugins(false)? {
        let has_settings = !schema_of(&*tx, &record)?.is_empty();
        let loaded = app.plugins().get_any(&record.id);
        plugins.push(dto(&record, has_settings, loaded.as_deref()));
    }
    plugins.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    Ok(PluginList { plugins })
}

fn inspect(req: InspectPackage, ctx: &QueryCtx<'_>) -> AppResult<PackageInfo> {
    let pkg = read_package(&req.path)?;
    let m = &pkg.manifest;
    let installed = ctx.tx.get_plugin(&m.id)?.filter(|p| p.deleted_at.is_none());
    Ok(PackageInfo {
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        description: m.description.clone(),
        authors: m.authors.clone(),
        permissions: m
            .permissions
            .iter()
            .map(|p| PermissionDto {
                permission: p.as_str().to_owned(),
                granted: installed
                    .as_ref()
                    .is_some_and(|i| i.granted.iter().any(|g| g == p.as_str())),
                sensitive: p.is_sensitive(),
            })
            .collect(),
        installed_version: installed.map(|i| i.version),
        sha256: pkg.sha256,
    })
}

fn install(req: InstallPlugin, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<PluginDto>> {
    let pkg = read_package(&req.path)?;
    let m = &pkg.manifest;
    check_against_registry(ctx.registry, m)?;
    let granted = checked_grant(m, &req.grant)?;
    let old = ctx.reads().get_plugin(&m.id)?;
    let manifest_json = to_json(m)?;
    ctx.tx.insert_plugin_package(&PluginPackageRecord {
        plugin_id: m.id.clone(),
        sha256: pkg.sha256.clone(),
        version: m.version.clone(),
        manifest: manifest_json.clone(),
        wasm: pkg.wasm.clone(),
        settings: to_json(&pkg.settings)?,
        locales: to_json(&pkg.locales)?,
        created_at: ctx.now,
    })?;
    let live_old = old.as_ref().filter(|o| o.deleted_at.is_none());
    let record = PluginRecord {
        id: m.id.clone(),
        version: m.version.clone(),
        manifest: manifest_json,
        enabled: live_old.is_none_or(|o| o.enabled),
        granted,
        package_sha256: pkg.sha256.clone(),
        failures: 0,
        last_error: None,
        auto_disabled: false,
        installed_at: old.as_ref().map_or(ctx.now, |o| o.installed_at),
        updated_at: ctx.now,
        deleted_at: None,
    };
    ctx.tx.upsert_plugin(&record)?;
    let msg = summary::<PluginsInstall>()
        .param("name", &m.name)
        .param("version", &m.version);
    Ok(CommandOutcome {
        response: dto(&record, !pkg.settings.is_empty(), None),
        inverse: vec![inverse_call::<PluginsRestore>(&PluginSnapshot {
            id: m.id.clone(),
            row: old.as_ref().map(to_json).transpose()?,
            kv: None,
            settings: None,
        })?],
        summary: msg,
        events: vec![changed(&m.id)],
    })
}

/// Writes `next` (or removes the plugin), recording the exact way back.
fn replace_row(
    ctx: &mut CommandCtx<'_>,
    current: &PluginRecord,
    next: &PluginRecord,
    msg: fc_api::v1::common::I18nMessage,
) -> AppResult<CommandOutcome<PluginDto>> {
    let has_settings = !schema_of(ctx.reads(), next)?.is_empty();
    if current == next {
        return Ok(CommandOutcome {
            response: dto(next, has_settings, None),
            inverse: vec![],
            summary: msg,
            events: vec![],
        });
    }
    ctx.tx.upsert_plugin(next)?;
    Ok(CommandOutcome {
        response: dto(next, has_settings, None),
        inverse: vec![inverse_call::<PluginsRestore>(&PluginSnapshot {
            id: current.id.clone(),
            row: Some(to_json(current)?),
            kv: None,
            settings: None,
        })?],
        summary: msg,
        events: vec![changed(&current.id)],
    })
}

fn name_of(record: &PluginRecord) -> String {
    manifest_of(record).map_or_else(|| record.id.clone(), |m| m.name)
}

fn uninstall(req: UninstallPlugin, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<PluginDto>> {
    let current = live(ctx.reads(), &req.id)?;
    let mut next = current.clone();
    next.deleted_at = Some(ctx.now);
    next.updated_at = ctx.now;
    let scope = SettingScope::Plugin(req.id.clone());
    let (kv, settings) = if req.delete_data {
        (
            Some(ctx.reads().list_plugin_kv(&req.id, "")?),
            Some(ctx.reads().list_settings(&scope)?),
        )
    } else {
        (None, None)
    };
    ctx.tx.upsert_plugin(&next)?;
    for (key, _) in kv.iter().flatten() {
        ctx.tx.set_plugin_kv(&req.id, key, None, ctx.now)?;
    }
    for (key, _) in settings.iter().flatten() {
        ctx.tx.delete_setting(&scope, key)?;
    }
    Ok(CommandOutcome {
        response: dto(&next, false, None),
        inverse: vec![inverse_call::<PluginsRestore>(&PluginSnapshot {
            id: req.id.clone(),
            row: Some(to_json(&current)?),
            kv: kv.as_ref().map(to_json).transpose()?,
            settings: settings.as_ref().map(to_json).transpose()?,
        })?],
        summary: summary::<PluginsUninstall>().param("name", name_of(&current)),
        events: vec![changed(&req.id)],
    })
}

fn set_enabled(req: SetPluginEnabled, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<PluginDto>> {
    let current = live(ctx.reads(), &req.id)?;
    let mut next = current.clone();
    next.enabled = req.enabled;
    if req.enabled {
        next.auto_disabled = false;
        next.failures = 0;
    }
    if next != current {
        next.updated_at = ctx.now;
    }
    let msg = summary::<PluginsSetEnabled>()
        .param("name", name_of(&current))
        .param("enabled", req.enabled);
    replace_row(ctx, &current, &next, msg)
}

fn set_permissions(
    req: SetPluginPermissions,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<PluginDto>> {
    let current = live(ctx.reads(), &req.id)?;
    let manifest = manifest_of(&current).ok_or_else(|| not_installed(&req.id))?;
    let mut next = current.clone();
    next.granted = checked_grant(&manifest, &req.granted)?;
    if next != current {
        next.updated_at = ctx.now;
    }
    let msg = summary::<PluginsSetPermissions>().param("name", name_of(&current));
    replace_row(ctx, &current, &next, msg)
}

fn settings_get(req: PluginRef, ctx: &QueryCtx<'_>) -> AppResult<PluginSettings> {
    let record = live(ctx.tx, &req.id)?;
    let schema = schema_of(ctx.tx, &record)?;
    let values = settings_values(ctx.tx, &req.id, &schema)?;
    Ok(PluginSettings {
        id: req.id,
        schema,
        values,
    })
}

fn settings_set(
    req: SetPluginSetting,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<PluginSettings>> {
    let record = live(ctx.reads(), &req.id)?;
    let schema = schema_of(ctx.reads(), &record)?;
    let def = schema.iter().find(|d| d.key == req.key).ok_or_else(|| {
        err_id(
            ErrorCode::NotFoundSetting,
            format!("plugin {} has no setting {}", req.id, req.key),
            &req.key,
        )
    })?;
    let scope = SettingScope::Plugin(req.id.clone());
    let old = ctx.reads().get_setting(&scope, &req.key)?;
    match &req.value {
        Some(v) => {
            validate_setting_value(def, v)?;
            ctx.tx.set_setting(&scope, &req.key, v, ctx.now)?;
        }
        None => {
            ctx.tx.delete_setting(&scope, &req.key)?;
        }
    }
    let values = settings_values(ctx.reads(), &req.id, &schema)?;
    let inverse = if old == req.value {
        vec![]
    } else {
        vec![inverse_call::<PluginsSettingsSet>(&SetPluginSetting {
            id: req.id.clone(),
            key: req.key.clone(),
            value: old,
        })?]
    };
    Ok(CommandOutcome {
        response: PluginSettings {
            id: req.id.clone(),
            schema,
            values,
        },
        inverse,
        summary: summary::<PluginsSettingsSet>()
            .param("name", name_of(&record))
            .param("key", &req.key),
        events: vec![DomainEvent::SettingChanged {
            scope,
            key: req.key,
        }],
    })
}

fn locales(req: GetPluginLocales, ctx: &QueryCtx<'_>) -> AppResult<PluginLocales> {
    let mut plugins = BTreeMap::new();
    for record in ctx.tx.list_plugins(false)? {
        let Some(pkg) = package_meta(ctx.tx, &record)? else {
            continue;
        };
        let all: BTreeMap<String, BTreeMap<String, String>> =
            serde_json::from_value(pkg.locales).unwrap_or_default();
        let pick = all
            .get(&req.lang)
            .or_else(|| all.get("en"))
            .or_else(|| all.values().next())
            .cloned()
            .unwrap_or_default();
        plugins.insert(record.id, pick);
    }
    Ok(PluginLocales { plugins })
}

fn logs(req: PluginRef, app: &App, _actor: &Actor) -> AppResult<PluginLogs> {
    live(&*app.store().read()?, &req.id)?;
    Ok(PluginLogs {
        entries: app.plugins().core.logs(&req.id),
    })
}

// ------------------------------------------------------------------ internal

fn restore(req: PluginSnapshot, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let id = req.id.clone();
    let scope = SettingScope::Plugin(id.clone());
    let current = ctx.reads().get_plugin(&id)?;
    let removing = req.row.is_none() && current.is_some();
    let current_kv = if req.kv.is_some() || removing {
        Some(ctx.reads().list_plugin_kv(&id, "")?)
    } else {
        None
    };
    let current_settings = if req.settings.is_some() {
        Some(ctx.reads().list_settings(&scope)?)
    } else {
        None
    };
    match &req.row {
        None => {
            if current.is_some() {
                ctx.tx.delete_plugin(&id)?;
            }
        }
        Some(v) => {
            let record: PluginRecord = crate::support::from_json(v)?;
            ctx.tx.upsert_plugin(&record)?;
        }
    }
    if let (Some(kv), Some(_)) = (&req.kv, &req.row) {
        let entries: Vec<(String, Value)> = crate::support::from_json(kv)?;
        for (key, _) in ctx.reads().list_plugin_kv(&id, "")? {
            ctx.tx.set_plugin_kv(&id, &key, None, ctx.now)?;
        }
        for (key, value) in &entries {
            ctx.tx.set_plugin_kv(&id, key, Some(value), ctx.now)?;
        }
    }
    if let Some(settings) = &req.settings {
        let entries: Vec<(String, Value)> = crate::support::from_json(settings)?;
        for (key, _) in ctx.reads().list_settings(&scope)? {
            ctx.tx.delete_setting(&scope, &key)?;
        }
        for (key, value) in &entries {
            ctx.tx.set_setting(&scope, key, value, ctx.now)?;
        }
    }
    let name = req
        .row
        .as_ref()
        .and_then(|v| serde_json::from_value::<PluginRecord>(v.clone()).ok())
        .or(current.clone())
        .map_or_else(|| id.clone(), |r| name_of(&r));
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![inverse_call::<PluginsRestore>(&PluginSnapshot {
            id: id.clone(),
            row: current.as_ref().map(to_json).transpose()?,
            kv: current_kv.as_ref().map(to_json).transpose()?,
            settings: current_settings.as_ref().map(to_json).transpose()?,
        })?],
        summary: summary::<PluginsRestore>().param("name", name),
        events: vec![changed(&id)],
    })
}

fn kv_set(req: SetPluginKv, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    live(ctx.reads(), &req.plugin_id)?;
    if req.key.is_empty() || req.key.len() > 256 {
        return Err(crate::support::invalid("kv key must be 1–256 bytes"));
    }
    if req.value.as_ref().is_some_and(|v| v.to_string().len() > 1024 * 1024) {
        return Err(crate::support::invalid("kv value larger than 1 MiB"));
    }
    ctx.tx
        .set_plugin_kv(&req.plugin_id, &req.key, req.value.as_ref(), ctx.now)?;
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![],
        summary: summary::<PluginsKvSet>(),
        events: vec![],
    })
}

fn record_failure(
    req: RecordPluginFailure,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<Empty>> {
    let current = live(ctx.reads(), &req.id)?;
    let mut next = current.clone();
    next.failures = req.failures;
    if req.error.is_some() {
        next.last_error = req.error.clone();
    }
    if req.disable {
        next.auto_disabled = true;
        next.updated_at = ctx.now;
    }
    if next != current {
        ctx.tx.upsert_plugin(&next)?;
    }
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![],
        summary: summary::<PluginsRecordFailure>(),
        events: if req.disable && !current.auto_disabled {
            vec![DomainEvent::PluginsChanged {
                plugin_id: req.id,
                auto_disabled: true,
            }]
        } else {
            vec![]
        },
    })
}

fn rollback(req: RollbackPluginOp, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    for call in &req.calls {
        ctx.call(call)?;
    }
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![],
        summary: summary::<PluginsRollback>(),
        events: vec![],
    })
}

// ------------------------------------------------------------------ models

fn model_dto(store: Option<&ModelStore>, plugin_id: &str, m: &fc_plugins::models::ModelSpec) -> ModelDto {
    let status = store
        .and_then(|s| s.status(plugin_id, m).ok())
        .unwrap_or(ModelStatus::Missing);
    let (state, downloaded) = match status {
        ModelStatus::Missing => (ModelState::Missing, 0),
        ModelStatus::Partial { downloaded } => (ModelState::Partial, downloaded),
        ModelStatus::Ready => (ModelState::Ready, m.size),
    };
    ModelDto {
        plugin_id: plugin_id.to_owned(),
        id: m.id.clone(),
        name: m.name.clone(),
        size: m.size,
        license: m.license.clone(),
        url: m.url.clone(),
        state,
        downloaded,
    }
}

fn models_list(_: Empty, app: &App, _actor: &Actor) -> AppResult<ModelList> {
    let store = model_store(app);
    let mut models = Vec::new();
    for p in app.plugins().active() {
        if !p.has("models:manage") {
            continue;
        }
        for m in &p.manifest.contributes.models {
            models.push(model_dto(store.as_ref(), p.id(), m));
        }
    }
    Ok(ModelList {
        models,
        disk_usage: store.map_or(0, |s| s.disk_usage()),
    })
}

fn model_spec(reads: &dyn Reads, r: &ModelRef) -> AppResult<fc_plugins::models::ModelSpec> {
    let record = live(reads, &r.plugin_id)?;
    let manifest = manifest_of(&record).ok_or_else(|| not_installed(&r.plugin_id))?;
    if !record.granted.iter().any(|g| g == "models:manage") {
        return Err(inactive(&r.plugin_id));
    }
    manifest
        .contributes
        .models
        .into_iter()
        .find(|m| m.id == r.id)
        .ok_or_else(|| no_contribution(&r.plugin_id, &r.id))
}

fn models_download(req: ModelRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ModelDownload>> {
    model_spec(ctx.reads(), &req)?;
    let job = crate::jobs::enqueue(ctx, ModelDownloadJob::KIND, to_json(&req)?)?;
    Ok(CommandOutcome {
        response: ModelDownload { job },
        inverse: vec![],
        summary: summary::<PluginsModelsDownload>(),
        events: vec![],
    })
}

fn model_error(e: ModelError) -> AppError {
    match e {
        ModelError::Unsupported(url) => AppError::Api(
            ApiError::new(ErrorCode::ProviderUnavailable, format!("cannot fetch {url}"))
                .with_detail("provider", "models"),
        ),
        ModelError::Cancelled => crate::jobs::cancelled(),
        other => AppError::api(ErrorCode::IoFile, other.to_string()),
    }
}

fn models_delete(req: ModelRef, app: &App, _actor: &Actor) -> AppResult<ModelDto> {
    let spec = model_spec(&*app.store().read()?, &req)?;
    let store = model_store(app)
        .ok_or_else(|| AppError::api(ErrorCode::IoFile, "collection folder unknown"))?;
    store.delete(&req.plugin_id, &req.id).map_err(model_error)?;
    Ok(model_dto(Some(&store), &req.plugin_id, &spec))
}

/// Background download of a plugin model (resumable, SHA-256 verified).
struct ModelDownloadJob;

impl ModelDownloadJob {
    const KIND: &'static str = "plugins.model_download";
}

impl JobHandler for ModelDownloadJob {
    fn kind(&self) -> &'static str {
        Self::KIND
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let req: ModelRef = crate::support::from_json(&payload)?;
        let spec = model_spec(&*ctx.read()?, &req)?;
        let dir = ctx
            .data_dir()
            .ok_or_else(|| AppError::api(ErrorCode::IoFile, "collection folder unknown"))?;
        let store = ModelStore::new(dir.join("models"));
        let mut last = 0u64;
        let path = store
            .download(
                &req.plugin_id,
                &spec,
                &FileFetcher,
                &|| ctx.is_cancelled(),
                &mut |done, total| {
                    // Report every ~1 %.
                    if total > 0 && (done - last) * 100 >= total {
                        last = done;
                        ctx.report(Some(done as f32 / total as f32), "jobs.plugins.model_download");
                    }
                },
            )
            .map_err(model_error)?;
        Ok(json!({ "path": path.display().to_string(), "size": spec.size }))
    }
}

// ------------------------------------------------------------------ modules.list

/// Plugins as modules (docs/03 §11: plugins register as modules): their UI contributions go
/// into `modules.list`, so the shell draws plugin menus like any other.
pub(crate) fn plugin_modules(reads: &dyn Reads) -> AppResult<Vec<ModuleDto>> {
    let mut out = Vec::new();
    for record in reads.list_plugins(false)? {
        let Some(m) = manifest_of(&record) else {
            continue;
        };
        let id = &record.id;
        let enabled = record.enabled && !record.auto_disabled;
        let granted = |p: &str| record.granted.iter().any(|g| g == p);
        let c = contributions(&m, &granted);
        let mut ui = Vec::new();
        if enabled {
            for (i, item) in c.menu_items.iter().enumerate() {
                ui.push(UiContribution {
                    slot: UiSlot::MainMenu,
                    id: format!("plugin.{id}.menu.{}", item.id),
                    label_key: format!("plugin.{id}.menu.{}", item.id),
                    icon: item.icon.clone().or_else(|| Some("puzzle".into())),
                    route: Some(format!("/plugins/view/{id}/{}", item.view)),
                    method: None,
                    order: 1000 + i as i32,
                });
            }
            for (i, panel) in c.sidebar_panels.iter().enumerate() {
                ui.push(UiContribution {
                    slot: UiSlot::SidebarPanel,
                    id: format!("plugin.{id}.panels.{}", panel.id),
                    label_key: format!("plugin.{id}.panels.{}", panel.id),
                    icon: None,
                    route: Some(format!("/plugins/view/{id}/{}", panel.view)),
                    method: None,
                    order: i as i32,
                });
            }
            for (i, button) in c.card_toolbar.iter().enumerate() {
                ui.push(UiContribution {
                    slot: UiSlot::CardToolbar,
                    id: format!("plugin.{id}.toolbar.{}", button.id),
                    label_key: format!("plugin.{id}.toolbar.{}", button.id),
                    icon: button.icon.clone(),
                    route: None,
                    method: Some(format!("{}.plugin.{id}.{}", fc_api::API_VERSION, button.command)),
                    order: i as i32,
                });
            }
            if !schema_of(reads, &record)?.is_empty() {
                ui.push(UiContribution {
                    slot: UiSlot::SettingsPage,
                    id: format!("plugin.{id}.settings"),
                    label_key: format!("plugin.{id}.name"),
                    icon: None,
                    route: Some(format!("/plugins/settings/{id}")),
                    method: None,
                    order: 1000,
                });
            }
        }
        out.push(ModuleDto {
            id: format!("plugin:{id}"),
            name_key: format!("plugin.{id}.name"),
            description_key: format!("plugin.{id}.description"),
            enabled,
            can_disable: true,
            ui,
            methods: if enabled {
                c.commands
                    .iter()
                    .map(|cmd| format!("{}.plugin.{id}.{cmd}", fc_api::API_VERSION))
                    .collect()
            } else {
                vec![]
            },
        });
    }
    Ok(out)
}
