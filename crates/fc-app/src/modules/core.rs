//! `core` module: undo/redo, batches, module registry, settings, jobs. Cannot be disabled.

use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, PendingOplogEntry, QueryCtx};
use crate::module::{choice_setting, settings_page, ModuleDescriptor};
use crate::registry::{ensure_invocable, unknown_method, Registry};
use crate::settings::{chain_for, resolve_setting, stored_chain, SettingsSchema as Schema};
use crate::support::history::{display_summary, scan};
use crate::support::{err_id, from_json, invalid, module_states, summary};
use fc_api::method::parse_qualified_name;
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::jobs::{JobDto, JobList, JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::modules::*;
use fc_api::v1::oplog::*;
use fc_api::v1::settings::*;
use fc_api::{ApiError, ApiMethod, ErrorCode, InvokeRequest, Visibility};
use fc_core::error::CoreError;
use fc_core::settings::validate_setting_value;
use fc_storage::repo::{JobRecord, OplogBatch, OplogRecordKind};
use serde_json::Value;

/// Module id.
pub const ID: &str = "core";

/// Setting: UI locale.
pub const LOCALE: &str = "app.locale";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.core.name",
        description_key: "modules.core.description",
        enabled_by_default: true,
        can_disable: false,
        settings: vec![
            choice_setting(LOCALE, &["tr", "en"], "tr"),
            choice_setting("app.theme", &["system", "light", "dark"], "system"),
        ],
        ui: vec![settings_page(
            "core.general",
            "settings.pages.general",
            "/settings/general",
            0,
        )],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.command::<OplogUndo>(undo);
    reg.command::<OplogRedo>(redo);
    reg.query::<OplogStatus>(status);
    reg.query::<OplogList>(oplog_list);
    reg.command::<BatchExecute>(batch);
    reg.query::<ModulesList>(modules_list);
    reg.command::<ModulesSetEnabled>(set_enabled);
    reg.query::<CommandsList>(commands_list);
    reg.query::<SettingsSchemaGet>(settings_schema);
    reg.query::<SettingsGet>(settings_get);
    reg.command::<SettingsSet>(settings_set);
    reg.command::<SettingsReset>(settings_reset);
    reg.query::<JobsList>(jobs_list);
    reg.query::<JobsGet>(jobs_get);
    reg.command::<JobsCancel>(jobs_cancel);
}

// ------------------------------------------------------------------ undo / redo

/// Executes the inverses of `target` (entries in reverse order, each entry's calls in order)
/// as a new batch of `kind` reverting `target`.
fn revert<M: ApiMethod>(
    ctx: &mut CommandCtx<'_>,
    target: &OplogBatch,
    kind: OplogKind,
) -> AppResult<CommandOutcome<UndoResult>> {
    let shown = display_summary(ctx.reads(), target)?;
    let entries = ctx.reads().oplog_batch_entries(target.batch_id)?;
    // The oplog is data (a restored backup brings its own): check every stored inverse
    // against the allow-list before running any of them.
    let mut steps: Vec<Vec<InvokeRequest>> = Vec::with_capacity(entries.len());
    for entry in entries.iter().rev() {
        let calls: Vec<InvokeRequest> = from_json(&entry.inverse)?;
        for call in &calls {
            ensure_replayable(ctx.registry, call)?;
        }
        steps.push(calls);
    }
    ctx.batch_kind = kind;
    ctx.reverts_batch_id = Some(target.batch_id);
    for call in steps.iter().flatten() {
        ctx.call(call)?;
    }
    if !ctx.has_oplog_entries() {
        // Nothing to re-execute (a no-op batch): still record the step so history advances.
        ctx.oplog.push(Some(PendingOplogEntry {
            command: format!("{}.{}", fc_api::API_VERSION, M::NAME),
            payload: Value::Object(Default::default()),
            inverse: Vec::new(),
            summary: shown.clone(),
        }));
    }
    let event = match kind {
        OplogKind::Redo => DomainEvent::Redone {
            reverted_batch_id: target.batch_id,
            summary: shown.clone(),
        },
        _ => DomainEvent::Undone {
            reverted_batch_id: target.batch_id,
            summary: shown.clone(),
        },
    };
    Ok(CommandOutcome {
        response: UndoResult {
            batch_id: ctx.batch_id,
            reverted_batch_id: target.batch_id,
            summary: shown,
        },
        inverse: Vec::new(),
        summary: summary::<M>(),
        events: vec![event],
    })
}

/// Undo/redo only run stored inverses whose method is replayable
/// ([`fc_api::ApiMethod::REPLAYABLE`]): undoable commands that start no job, write no file
/// and consume no staged job data. Anything else is refused (`PERMISSION_DENIED`).
fn ensure_replayable(registry: &Registry, call: &InvokeRequest) -> AppResult<()> {
    let d = parse_qualified_name(&call.method)
        .and_then(|n| registry.descriptor(n))
        .ok_or_else(|| unknown_method(&call.method))?;
    if d.replayable {
        return Ok(());
    }
    Err(AppError::Api(
        ApiError::new(
            ErrorCode::PermissionDenied,
            format!("{} cannot run as an undo/redo step", call.method),
        )
        .with_detail("method", call.method.as_str())
        .with_detail("reason", "not_replayable"),
    ))
}

fn undo(_: Empty, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<UndoResult>> {
    let target = scan(ctx.reads())?.undo_target()?;
    revert::<OplogUndo>(ctx, &target, OplogKind::Undo)
}

fn redo(_: Empty, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<UndoResult>> {
    let target = scan(ctx.reads())?
        .redo
        .ok_or_else(|| AppError::api(ErrorCode::ConflictNothingToRedo, "nothing to redo"))?;
    revert::<OplogRedo>(ctx, &target, OplogKind::Redo)
}

fn status(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<UndoStatus> {
    let h = scan(ctx.tx)?;
    Ok(UndoStatus {
        can_undo: h.undo.is_some(),
        can_redo: h.redo.is_some(),
        undo_summary: h
            .undo
            .as_ref()
            .map(|b| display_summary(ctx.tx, b))
            .transpose()?,
        redo_summary: h
            .redo
            .as_ref()
            .map(|b| display_summary(ctx.tx, b))
            .transpose()?,
    })
}

fn oplog_list(req: ListOplog, ctx: &QueryCtx<'_>) -> AppResult<OplogPage> {
    let page = ctx.tx.list_oplog(&req.page)?;
    let items = page
        .items
        .into_iter()
        .map(|r| {
            Ok(OplogEntryDto {
                id: r.id,
                batch_id: r.batch_id,
                command: r.command,
                payload: r.payload,
                inverse: from_json(&r.inverse)?,
                actor: from_json(&r.actor)?,
                kind: match r.kind {
                    OplogRecordKind::Do => OplogKind::Do,
                    OplogRecordKind::Undo => OplogKind::Undo,
                    OplogRecordKind::Redo => OplogKind::Redo,
                },
                reverts_batch_id: r.reverts_batch_id,
                summary: from_json(&r.summary)?,
                created_at: r.created_at,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(OplogPage {
        items,
        next_cursor: page.next_cursor,
    })
}

/// Runs the calls in this transaction and batch; their oplog entries form one undo step. Each
/// call passes the same access check as a top-level invoke ([`ensure_invocable`]), against the
/// module switches as they stand inside this transaction (an earlier call may have changed
/// them).
fn batch(req: ExecuteBatch, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<BatchResult>> {
    let mut results = Vec::with_capacity(req.calls.len());
    for call in &req.calls {
        let name =
            parse_qualified_name(&call.method).ok_or_else(|| unknown_method(&call.method))?;
        if [OplogUndo::NAME, OplogRedo::NAME, BatchExecute::NAME].contains(&name) {
            return Err(invalid(format!("{name} cannot run inside batch.execute")));
        }
        let d = *ctx
            .registry
            .descriptor(name)
            .ok_or_else(|| unknown_method(&call.method))?;
        let enabled = module_states(ctx.reads(), ctx.registry)?
            .get(d.module)
            .copied()
            .unwrap_or(false);
        ensure_invocable(ctx.registry, &d, ctx.actor, enabled)?;
        results.push(ctx.call(call)?);
    }
    Ok(CommandOutcome {
        response: BatchResult {
            batch_id: ctx.batch_id,
            results,
        },
        inverse: Vec::new(),
        summary: summary::<BatchExecute>().param("count", req.calls.len()),
        events: Vec::new(),
    })
}

// ------------------------------------------------------------------ modules & methods

fn module_dto(registry: &Registry, m: &ModuleDescriptor, enabled: bool) -> ModuleDto {
    let mut ui = if enabled { m.ui.clone() } else { Vec::new() };
    ui.sort_by_key(|u| u.order);
    ModuleDto {
        id: m.id.to_owned(),
        name_key: m.name_key.to_owned(),
        description_key: m.description_key.to_owned(),
        enabled,
        can_disable: m.can_disable,
        ui,
        methods: registry
            .methods()
            .filter(|d| d.module == m.id && d.visibility == Visibility::Public)
            .map(|d| d.qualified_name())
            .collect(),
    }
}

fn modules_list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<ModuleList> {
    let states = module_states(ctx.tx, ctx.registry)?;
    let mut modules: Vec<ModuleDto> = ctx
        .registry
        .modules()
        .iter()
        .map(|m| module_dto(ctx.registry, m, states.get(m.id).copied().unwrap_or(false)))
        .collect();
    // Plugins register as modules too (v0.7); their menus come with them.
    if states.get(crate::modules::plugins::ID).copied().unwrap_or(false) {
        modules.extend(crate::modules::plugins::plugin_modules(ctx.tx)?);
    }
    Ok(ModuleList { modules })
}

fn set_enabled(
    req: SetModuleEnabled,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ModuleDto>> {
    let registry = ctx.registry;
    let m = registry
        .modules()
        .iter()
        .find(|m| m.id == req.id)
        .ok_or_else(|| {
            err_id(
                ErrorCode::NotFoundModule,
                format!("module {} not found", req.id),
                &req.id,
            )
        })?;
    if !m.can_disable && !req.enabled {
        return Err(ApiError::new(
            ErrorCode::ConflictModuleRequired,
            format!("module {} cannot be disabled", m.id),
        )
        .with_detail("module", m.id)
        .into());
    }
    let old = module_states(ctx.reads(), registry)?
        .get(m.id)
        .copied()
        .unwrap_or(false);
    let msg = summary::<ModulesSetEnabled>()
        .param("module", m.id)
        .param("enabled", req.enabled);
    if old == req.enabled {
        return Ok(CommandOutcome {
            response: module_dto(registry, m, old),
            inverse: Vec::new(),
            summary: msg,
            events: Vec::new(),
        });
    }
    ctx.tx.set_module_enabled(m.id, req.enabled, ctx.now)?;
    Ok(CommandOutcome {
        response: module_dto(registry, m, req.enabled),
        inverse: vec![crate::inverse_call::<ModulesSetEnabled>(
            &SetModuleEnabled {
                id: m.id.to_owned(),
                enabled: old,
            },
        )?],
        summary: msg,
        events: vec![DomainEvent::ModuleToggled {
            id: m.id.to_owned(),
            enabled: req.enabled,
        }],
    })
}

fn commands_list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<MethodList> {
    let states = module_states(ctx.tx, ctx.registry)?;
    Ok(MethodList {
        methods: ctx
            .registry
            .methods()
            .map(|d| MethodInfo {
                name: d.qualified_name(),
                kind: d.kind,
                module: d.module.to_owned(),
                permission: d.permission.to_owned(),
                visibility: d.visibility,
                undoable: d.undoable,
                enabled: states.get(d.module).copied().unwrap_or(false),
            })
            .chain(ctx.registry.dynamic_methods().into_iter().map(|m| MethodInfo {
                name: m.qualified_name(),
                kind: fc_api::MethodKind::Command,
                module: m.module.to_string(),
                permission: m.permission.to_string(),
                visibility: Visibility::Public,
                undoable: false,
                enabled: states.get(&*m.module).copied().unwrap_or(false),
            }))
            .collect(),
    })
}

// ------------------------------------------------------------------ settings

fn enabled_schema(
    reads: &dyn fc_storage::Reads,
    registry: &Registry,
) -> AppResult<Vec<(String, fc_core::settings::SettingDef)>> {
    let states = module_states(reads, registry)?;
    Ok(Schema::from_modules(registry.modules())
        .entries()
        .iter()
        .filter(|(m, _)| states.get(m.as_str()).copied().unwrap_or(false))
        .cloned()
        .collect())
}

fn find_def(registry: &Registry, key: &str) -> AppResult<fc_core::settings::SettingDef> {
    Schema::from_modules(registry.modules())
        .find(key)
        .cloned()
        .ok_or_else(|| {
            err_id(
                ErrorCode::NotFoundSetting,
                format!("setting {key} not found"),
                key,
            )
        })
}

fn settings_schema(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<SettingsSchema> {
    Ok(SettingsSchema {
        settings: enabled_schema(ctx.tx, ctx.registry)?
            .into_iter()
            .map(|(module, def)| SettingSchemaEntry { module, def })
            .collect(),
    })
}

fn resolve(
    reads: &dyn fc_storage::Reads,
    def: &fc_core::settings::SettingDef,
    scope: &fc_core::settings::SettingScope,
) -> AppResult<SettingValueDto> {
    let (value, source) = resolve_setting(def, &stored_chain(reads, scope, &def.key)?);
    Ok(SettingValueDto {
        key: def.key.clone(),
        value,
        source,
    })
}

fn settings_get(req: GetSettings, ctx: &QueryCtx<'_>) -> AppResult<SettingValues> {
    chain_for(ctx.tx, &req.scope)?;
    let defs = match &req.keys {
        Some(keys) => keys
            .iter()
            .map(|k| find_def(ctx.registry, k))
            .collect::<AppResult<Vec<_>>>()?,
        None => enabled_schema(ctx.tx, ctx.registry)?
            .into_iter()
            .map(|(_, d)| d)
            .collect(),
    };
    Ok(SettingValues {
        values: defs
            .iter()
            .map(|d| resolve(ctx.tx, d, &req.scope))
            .collect::<AppResult<Vec<_>>>()?,
    })
}

fn settings_set(
    req: SetSetting,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SettingValueDto>> {
    let def = find_def(ctx.registry, &req.key)?;
    if !def.scopes.contains(&req.scope.kind()) {
        return Err(CoreError::InvalidSettingValue {
            key: def.key,
            reason: format!("not settable at scope {}", req.scope.storage_key()),
        }
        .into());
    }
    validate_setting_value(&def, &req.value)?;
    chain_for(ctx.reads(), &req.scope)?;
    let old = ctx.reads().get_setting(&req.scope, &req.key)?;
    ctx.tx
        .set_setting(&req.scope, &req.key, &req.value, ctx.now)?;
    let inverse = match old {
        Some(value) => crate::inverse_call::<SettingsSet>(&SetSetting {
            scope: req.scope.clone(),
            key: req.key.clone(),
            value,
        })?,
        None => crate::inverse_call::<SettingsReset>(&ResetSetting {
            scope: req.scope.clone(),
            key: req.key.clone(),
        })?,
    };
    Ok(CommandOutcome {
        response: SettingValueDto {
            key: req.key.clone(),
            value: req.value,
            source: Some(req.scope.clone()),
        },
        inverse: vec![inverse],
        summary: summary::<SettingsSet>().param("key", &req.key),
        events: vec![DomainEvent::SettingChanged {
            scope: req.scope,
            key: req.key,
        }],
    })
}

fn settings_reset(
    req: ResetSetting,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SettingValueDto>> {
    let def = find_def(ctx.registry, &req.key)?;
    chain_for(ctx.reads(), &req.scope)?;
    let old = ctx.reads().get_setting(&req.scope, &req.key)?;
    let msg = summary::<SettingsReset>().param("key", &req.key);
    let Some(old) = old else {
        return Ok(CommandOutcome {
            response: resolve(ctx.reads(), &def, &req.scope)?,
            inverse: Vec::new(),
            summary: msg,
            events: Vec::new(),
        });
    };
    ctx.tx.delete_setting(&req.scope, &req.key)?;
    Ok(CommandOutcome {
        response: resolve(ctx.reads(), &def, &req.scope)?,
        inverse: vec![crate::inverse_call::<SettingsSet>(&SetSetting {
            scope: req.scope.clone(),
            key: req.key.clone(),
            value: old,
        })?],
        summary: msg,
        events: vec![DomainEvent::SettingChanged {
            scope: req.scope,
            key: req.key,
        }],
    })
}

// ------------------------------------------------------------------ jobs

fn job_dto(r: JobRecord) -> AppResult<JobDto> {
    let status: JobStatus = from_json(&Value::String(r.status))?;
    Ok(JobDto {
        id: r.id,
        kind: r.kind,
        status,
        progress: r.progress.map(|p| p as f32),
        message: None,
        error: r.error.as_ref().map(from_json).transpose()?,
        result: r.result,
        created_at: r.created_at,
        updated_at: r.updated_at,
    })
}

fn require_job(reads: &dyn fc_storage::Reads, id: fc_core::JobId) -> AppResult<JobRecord> {
    reads
        .get_job(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundJob, format!("job {id} not found"), id))
}

fn jobs_list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<JobList> {
    Ok(JobList {
        jobs: ctx
            .tx
            .list_jobs(100)?
            .into_iter()
            .map(job_dto)
            .collect::<AppResult<Vec<_>>>()?,
    })
}

fn jobs_get(req: JobRef, ctx: &QueryCtx<'_>) -> AppResult<JobDto> {
    job_dto(require_job(ctx.tx, req.id)?)
}

/// Marks an unfinished job cancelled (v0.1 has no worker pool yet; a running worker will
/// observe this through its `CancelToken` once it exists).
fn jobs_cancel(req: JobRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<JobDto>> {
    let mut job = require_job(ctx.reads(), req.id)?;
    let mut events = Vec::new();
    if ["queued", "running", "interrupted"].contains(&job.status.as_str()) {
        job.status = "cancelled".into();
        job.updated_at = ctx.now;
        ctx.tx.upsert_job(&job)?;
        events.push(DomainEvent::JobProgress {
            job: job_dto(job.clone())?,
        });
    }
    Ok(CommandOutcome {
        response: job_dto(job)?,
        inverse: Vec::new(),
        summary: summary::<JobsCancel>(),
        events,
    })
}
