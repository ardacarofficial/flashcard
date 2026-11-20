//! Provider, route and price commands. Every database change has an exact inverse (provider
//! rows are restored verbatim by the internal `ai.providers.restore`); setting a key touches
//! only the keychain and is deliberately not an undo step (its params must never reach the
//! oplog).

use super::context::{require_live_provider, require_provider};
use super::{key_account, AiServices, MODELS_JOB};
use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::registry::Registry;
use crate::support::summary;
use fc_ai::secrets::{Secret, SecretError};
use fc_ai::ProviderKind;
use fc_api::v1::ai::*;
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::{
    AiPricesDelete, AiPricesList, AiPricesSet, AiProvidersCreate, AiProvidersDelete,
    AiProvidersList, AiProvidersRefreshModels, AiProvidersRestore, AiProvidersSetKey,
    AiProvidersSetModels, AiProvidersUpdate, AiRoutesList, AiRoutesSet,
};
use fc_api::{ApiError, ErrorCode};
use fc_core::AiProviderId;
use fc_storage::repo::{AiPriceRecord, AiProviderRecord, AiRouteRecord};
use fc_storage::Reads;
use std::sync::Arc;

pub(super) fn register(reg: &mut Registry, services: &Arc<AiServices>) {
    let s = Arc::clone(services);
    reg.query::<AiProvidersList>(move |_: Empty, ctx: &QueryCtx<'_>| list(&s, ctx.tx));
    let s = Arc::clone(services);
    reg.command::<AiProvidersCreate>(move |r, ctx: &mut CommandCtx<'_>| create(&s, r, ctx));
    let s = Arc::clone(services);
    reg.command::<AiProvidersUpdate>(move |r, ctx: &mut CommandCtx<'_>| update(&s, r, ctx));
    let s = Arc::clone(services);
    reg.command::<AiProvidersDelete>(move |r, ctx: &mut CommandCtx<'_>| delete(&s, r, ctx));
    let s = Arc::clone(services);
    reg.command::<AiProvidersSetKey>(move |r, ctx: &mut CommandCtx<'_>| set_key(&s, r, ctx));
    reg.command::<AiProvidersRefreshModels>(refresh_models);
    let s = Arc::clone(services);
    reg.command::<AiProvidersRestore>(move |r, ctx: &mut CommandCtx<'_>| restore(&s, r, ctx));
    reg.command::<AiProvidersSetModels>(set_models);
    reg.query::<AiRoutesList>(|_: Empty, ctx: &QueryCtx<'_>| routes(ctx.tx));
    reg.command::<AiRoutesSet>(set_route);
    reg.query::<AiPricesList>(|_: Empty, ctx: &QueryCtx<'_>| prices(ctx.tx));
    reg.command::<AiPricesSet>(set_price);
    reg.command::<AiPricesDelete>(delete_price);
}

// ------------------------------------------------------------------ conversions

pub(crate) fn kind_of(k: AiProviderKind) -> ProviderKind {
    match k {
        AiProviderKind::Anthropic => ProviderKind::Anthropic,
        AiProviderKind::Openai => ProviderKind::Openai,
        AiProviderKind::OpenaiCompatible => ProviderKind::OpenaiCompatible,
        AiProviderKind::Ollama => ProviderKind::Ollama,
    }
}

fn api_kind(k: ProviderKind) -> AiProviderKind {
    match k {
        ProviderKind::Anthropic => AiProviderKind::Anthropic,
        ProviderKind::Openai => AiProviderKind::Openai,
        ProviderKind::OpenaiCompatible => AiProviderKind::OpenaiCompatible,
        ProviderKind::Ollama => AiProviderKind::Ollama,
    }
}

fn kind_str(k: AiProviderKind) -> &'static str {
    match k {
        AiProviderKind::Anthropic => "anthropic",
        AiProviderKind::Openai => "openai",
        AiProviderKind::OpenaiCompatible => "openai_compatible",
        AiProviderKind::Ollama => "ollama",
    }
}

pub(crate) fn parse_kind(s: &str) -> AppResult<AiProviderKind> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .map_err(|e| AppError::api(ErrorCode::Internal, format!("provider kind {s:?}: {e}")))
}

/// Suggested starting values of a kind (Claude first; model ids are data, editable).
fn kind_info(kind: ProviderKind) -> AiProviderKindInfo {
    let (default_model, suggested): (&str, &[&str]) = match kind {
        ProviderKind::Anthropic => (
            "claude-sonnet-5-5",
            &["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"],
        ),
        _ => ("", &[]),
    };
    AiProviderKindInfo {
        kind: api_kind(kind),
        default_base_url: kind.default_base_url().into(),
        default_model: default_model.into(),
        suggested_models: suggested.iter().map(|s| (*s).to_owned()).collect(),
        needs_key: kind.needs_key(),
        local: kind.local(),
    }
}

fn default_name(kind: AiProviderKind) -> &'static str {
    match kind {
        AiProviderKind::Anthropic => "Claude",
        AiProviderKind::Openai => "OpenAI",
        AiProviderKind::OpenaiCompatible => "OpenAI-compatible",
        AiProviderKind::Ollama => "Ollama",
    }
}

/// Whether a key is stored (a failing keychain counts as "no key").
pub(crate) fn has_key(services: &AiServices, id: AiProviderId) -> bool {
    matches!(services.secrets().get(&key_account(id)), Ok(Some(_)))
}

pub(crate) fn dto(services: &AiServices, p: &AiProviderRecord) -> AppResult<AiProviderDto> {
    let kind = parse_kind(&p.kind)?;
    let k = kind_of(kind);
    Ok(AiProviderDto {
        id: p.id,
        kind,
        name: p.name.clone(),
        base_url: p.base_url.clone(),
        default_model: p.default_model.clone(),
        models: p.models.clone(),
        is_default: p.is_default,
        has_key: has_key(services, p.id),
        needs_key: k.needs_key(),
        local: k.local() || is_loopback(&p.base_url),
        created_at: p.created_at,
        updated_at: p.updated_at,
    })
}

/// A base URL on this computer.
fn is_loopback(url: &str) -> bool {
    let rest = url.split("://").nth(1).unwrap_or(url);
    if rest.starts_with("[::1]") {
        return true;
    }
    let host = rest.split(['/', ':']).next().unwrap_or_default();
    host == "localhost" || host.starts_with("127.")
}

fn snapshot(p: &AiProviderRecord) -> AppResult<AiProviderSnapshot> {
    Ok(AiProviderSnapshot {
        id: p.id,
        kind: parse_kind(&p.kind)?,
        name: p.name.clone(),
        base_url: p.base_url.clone(),
        default_model: p.default_model.clone(),
        models: p.models.clone(),
        is_default: p.is_default,
        position: p.position,
        created_at: p.created_at,
        updated_at: p.updated_at,
        deleted_at: p.deleted_at,
    })
}

fn from_snapshot(s: &AiProviderSnapshot) -> AiProviderRecord {
    AiProviderRecord {
        id: s.id,
        kind: kind_str(s.kind).into(),
        name: s.name.clone(),
        base_url: s.base_url.clone(),
        default_model: s.default_model.clone(),
        models: s.models.clone(),
        is_default: s.is_default,
        position: s.position,
        created_at: s.created_at,
        updated_at: s.updated_at,
        deleted_at: s.deleted_at,
    }
}

fn provider_error(field: &str, message: impl Into<String>) -> AppError {
    AppError::Api(ApiError::new(ErrorCode::ValidationAiProvider, message).with_detail("field", field))
}

fn clean_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(provider_error("name", "name must be 1–80 characters"));
    }
    Ok(name.to_owned())
}

fn clean_url(url: &str) -> AppResult<String> {
    let url = url.trim().trim_end_matches('/');
    let ok = (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() > "http://".len()
        && url.len() <= 500
        && !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if !ok {
        return Err(provider_error("base_url", "base URL must be an http(s) URL"));
    }
    Ok(url.to_owned())
}

fn clean_model(model: &str, field: &str) -> AppResult<String> {
    let model = model.trim();
    if model.chars().count() > 200 || model.chars().any(char::is_control) {
        return Err(provider_error(field, "model id is too long or invalid"));
    }
    Ok(model.to_owned())
}

fn changed_event() -> Vec<DomainEvent> {
    vec![DomainEvent::AiConfigChanged]
}

/// Clears the default flag of every other live provider; returns their rows as they were.
fn clear_other_defaults(
    ctx: &mut CommandCtx<'_>,
    keep: AiProviderId,
) -> AppResult<Vec<AiProviderSnapshot>> {
    let mut before = Vec::new();
    for mut p in ctx.reads().list_ai_providers(true)? {
        if p.id != keep && p.is_default {
            before.push(snapshot(&p)?);
            p.is_default = false;
            p.updated_at = ctx.now;
            ctx.tx.upsert_ai_provider(&p)?;
        }
    }
    Ok(before)
}

// ------------------------------------------------------------------ providers

fn list(services: &AiServices, reads: &dyn Reads) -> AppResult<AiProviderList> {
    Ok(AiProviderList {
        providers: reads
            .list_ai_providers(false)?
            .iter()
            .map(|p| dto(services, p))
            .collect::<AppResult<_>>()?,
        kinds: ProviderKind::ALL.into_iter().map(kind_info).collect(),
        keychain: services.has_keychain(),
        keychain_session_only: services.has_keychain() && !services.keys_persist(),
    })
}

fn create(
    services: &AiServices,
    req: CreateAiProvider,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AiProviderDto>> {
    let info = kind_info(kind_of(req.kind));
    let name = clean_name(req.name.as_deref().unwrap_or(default_name(req.kind)))?;
    let base_url = clean_url(req.base_url.as_deref().unwrap_or(&info.default_base_url))?;
    let default_model = clean_model(
        req.default_model.as_deref().unwrap_or(&info.default_model),
        "default_model",
    )?;
    let live = ctx.reads().list_ai_providers(false)?;
    let all = ctx.reads().list_ai_providers(true)?;
    let id = AiProviderId::generate(ctx.ids);
    let is_default = req.is_default.unwrap_or(false) || !live.iter().any(|p| p.is_default);
    let record = AiProviderRecord {
        id,
        kind: kind_str(req.kind).into(),
        name: name.clone(),
        base_url,
        default_model,
        models: info.suggested_models.clone(),
        is_default,
        position: all.iter().map(|p| p.position + 1).max().unwrap_or(0),
        created_at: ctx.now,
        updated_at: ctx.now,
        deleted_at: None,
    };
    ctx.tx.upsert_ai_provider(&record)?;
    let mut rows = if is_default {
        clear_other_defaults(ctx, id)?
    } else {
        Vec::new()
    };
    let mut gone = snapshot(&record)?;
    gone.deleted_at = Some(ctx.now);
    rows.insert(0, gone);
    Ok(CommandOutcome {
        response: dto(services, &record)?,
        inverse: vec![inverse_call::<AiProvidersRestore>(&RestoreAiProviders { rows })?],
        summary: summary::<AiProvidersCreate>().param("name", &name),
        events: changed_event(),
    })
}

fn update(
    services: &AiServices,
    req: UpdateAiProvider,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AiProviderDto>> {
    let old = require_live_provider(ctx.reads(), req.id)?;
    let mut models: Vec<String> = Vec::new();
    for m in &req.models {
        let m = clean_model(m, "models")?;
        if !m.is_empty() && !models.contains(&m) {
            models.push(m);
        }
    }
    if models.len() > 500 {
        return Err(provider_error("models", "too many models"));
    }
    let mut p = old.clone();
    p.name = clean_name(&req.name)?;
    p.base_url = clean_url(&req.base_url)?;
    p.default_model = clean_model(&req.default_model, "default_model")?;
    p.models = models;
    p.is_default = old.is_default || req.is_default;
    p.updated_at = ctx.now;
    // A key was entered for one host: it never follows the provider to another one unless
    // the user confirms and may manage keys anyway.
    let keep_key = req.keep_key == Some(true) && ctx.registry.permits(ctx.actor, KEYS_PERMISSION);
    let drop_key = endpoint_changed(&old, &p) && !keep_key;
    let unchanged = AiProviderRecord {
        updated_at: p.updated_at,
        ..old.clone()
    };
    if p == unchanged {
        return Ok(CommandOutcome {
            response: dto(services, &old)?,
            inverse: vec![],
            summary: summary::<AiProvidersUpdate>().param("name", &old.name),
            events: vec![],
        });
    }
    ctx.tx.upsert_ai_provider(&p)?;
    let mut rows = vec![snapshot(&old)?];
    if p.is_default && !old.is_default {
        rows.extend(clear_other_defaults(ctx, p.id)?);
    }
    if drop_key {
        services
            .secrets()
            .delete(&key_account(p.id))
            .map_err(keychain_error)?;
    }
    Ok(CommandOutcome {
        response: dto(services, &p)?,
        inverse: vec![inverse_call::<AiProvidersRestore>(&RestoreAiProviders { rows })?],
        summary: summary::<AiProvidersUpdate>().param("name", &p.name),
        events: changed_event(),
    })
}

fn delete(
    services: &AiServices,
    req: AiProviderRef,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AiProviderDto>> {
    let old = require_live_provider(ctx.reads(), req.id)?;
    let mut p = old.clone();
    p.deleted_at = Some(ctx.now);
    p.is_default = false;
    p.updated_at = ctx.now;
    ctx.tx.upsert_ai_provider(&p)?;
    let mut rows = vec![snapshot(&old)?];
    // The default moves to the first remaining provider.
    if old.is_default {
        if let Some(mut next) = ctx.reads().list_ai_providers(false)?.into_iter().next() {
            rows.push(snapshot(&next)?);
            next.is_default = true;
            next.updated_at = ctx.now;
            ctx.tx.upsert_ai_provider(&next)?;
        }
    }
    // The key goes with the provider (privacy over convenience): undo brings the provider
    // back without it — keys are never part of the undo history.
    services
        .secrets()
        .delete(&key_account(old.id))
        .map_err(keychain_error)?;
    Ok(CommandOutcome {
        response: dto(services, &p)?,
        inverse: vec![inverse_call::<AiProvidersRestore>(&RestoreAiProviders { rows })?],
        summary: summary::<AiProvidersDelete>().param("name", &old.name),
        events: changed_event(),
    })
}

/// Permission needed to keep a key when a provider's address changes.
const KEYS_PERMISSION: &str = "ai_keys:write";

/// The provider now talks to a different host or protocol.
fn endpoint_changed(old: &AiProviderRecord, new: &AiProviderRecord) -> bool {
    old.base_url != new.base_url || old.kind != new.kind
}

fn restore(
    services: &AiServices,
    req: RestoreAiProviders,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<Empty>> {
    let mut before = Vec::new();
    for row in &req.rows {
        let restored = from_snapshot(row);
        if let Some(current) = ctx.reads().get_ai_provider(row.id)? {
            before.push(snapshot(&current)?);
            // Undo/redo of an address change: the key stored now was meant for the current
            // address, not the restored one.
            if endpoint_changed(&current, &restored) {
                services
                    .secrets()
                    .delete(&key_account(row.id))
                    .map_err(keychain_error)?;
            }
        }
        ctx.tx.upsert_ai_provider(&restored)?;
    }
    let name = req.rows.first().map(|r| r.name.clone()).unwrap_or_default();
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![inverse_call::<AiProvidersRestore>(&RestoreAiProviders { rows: before })?],
        summary: summary::<AiProvidersRestore>().param("name", name),
        events: changed_event(),
    })
}

pub(crate) fn keychain_error(e: SecretError) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ProviderAiKeychain, e.to_string()).with_detail("reason", e.to_string()),
    )
}

/// Checks a key without ever echoing it.
fn clean_key(key: &str) -> AppResult<Secret> {
    let key = key.trim();
    let ok = !key.is_empty()
        && key.chars().count() <= 512
        && !key.chars().any(|c| c.is_whitespace() || c.is_control());
    if !ok {
        return Err(AppError::api(
            ErrorCode::ValidationAiKey,
            "the API key is empty, too long or contains spaces",
        ));
    }
    Ok(Secret::new(key))
}

fn set_key(
    services: &AiServices,
    req: SetAiProviderKey,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AiProviderDto>> {
    let p = require_live_provider(ctx.reads(), req.provider_id)?;
    let account = key_account(p.id);
    let store = services.secrets();
    match req.key.as_deref() {
        Some(k) => store.set(&account, &clean_key(k)?),
        None => store.delete(&account),
    }
    .map_err(keychain_error)?;
    Ok(CommandOutcome {
        response: dto(services, &p)?,
        // Not an undo step: the params (the key) must never be written to the oplog.
        inverse: vec![],
        summary: summary::<AiProvidersSetKey>().param("name", &p.name),
        events: changed_event(),
    })
}

fn refresh_models(req: AiProviderRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<JobDto>> {
    require_live_provider(ctx.reads(), req.id)?;
    let job = crate::jobs::enqueue(ctx, MODELS_JOB, serde_json::json!({ "provider_id": req.id }))?;
    Ok(CommandOutcome {
        response: job,
        inverse: vec![],
        summary: summary::<AiProvidersRefreshModels>(),
        events: vec![],
    })
}

fn set_models(req: SetAiModels, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let mut p = require_provider(ctx.reads(), req.id)?;
    let mut models: Vec<String> = req
        .models
        .iter()
        .map(|m| m.trim().to_owned())
        .filter(|m| !m.is_empty() && m.len() <= 200)
        .collect();
    models.sort();
    models.dedup();
    models.truncate(500);
    p.models = models;
    p.updated_at = ctx.now;
    ctx.tx.upsert_ai_provider(&p)?;
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![],
        summary: summary::<AiProvidersSetModels>(),
        events: changed_event(),
    })
}

// ------------------------------------------------------------------ routes

fn routes(reads: &dyn Reads) -> AppResult<AiRouteList> {
    let stored = reads.list_ai_routes()?;
    Ok(AiRouteList {
        routes: AiFeature::ALL
            .into_iter()
            .map(|f| {
                let r = stored.iter().find(|r| r.feature == f.id());
                AiRouteDto {
                    feature: f,
                    provider_id: r.and_then(|r| r.provider_id),
                    model: r.and_then(|r| r.model.clone()),
                }
            })
            .collect(),
    })
}

fn set_route(req: AiRouteDto, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AiRouteDto>> {
    if let Some(p) = req.provider_id {
        require_live_provider(ctx.reads(), p)?;
    }
    let model = match req.model.as_deref() {
        Some(m) => Some(clean_model(m, "model")?).filter(|m| !m.is_empty()),
        None => None,
    };
    let old = routes(ctx.reads())?
        .routes
        .into_iter()
        .find(|r| r.feature == req.feature)
        .unwrap_or(AiRouteDto {
            feature: req.feature,
            provider_id: None,
            model: None,
        });
    let new = AiRouteDto {
        feature: req.feature,
        provider_id: req.provider_id,
        model,
    };
    if new == old {
        return Ok(CommandOutcome {
            response: new,
            inverse: vec![],
            summary: summary::<AiRoutesSet>(),
            events: vec![],
        });
    }
    if new.provider_id.is_none() && new.model.is_none() {
        ctx.tx.delete_ai_route(req.feature.id())?;
    } else {
        ctx.tx.upsert_ai_route(&AiRouteRecord {
            feature: req.feature.id().into(),
            provider_id: new.provider_id,
            model: new.model.clone(),
            updated_at: ctx.now,
        })?;
    }
    Ok(CommandOutcome {
        response: new,
        inverse: vec![inverse_call::<AiRoutesSet>(&old)?],
        summary: summary::<AiRoutesSet>().param("feature", req.feature.id()),
        events: changed_event(),
    })
}

// ------------------------------------------------------------------ prices

fn price_dto(p: &AiPriceRecord) -> AiPriceDto {
    AiPriceDto {
        model: p.model.clone(),
        input_per_mtok: p.input_per_mtok,
        output_per_mtok: p.output_per_mtok,
    }
}

fn prices(reads: &dyn Reads) -> AppResult<AiPriceList> {
    Ok(AiPriceList {
        prices: reads.list_ai_prices()?.iter().map(price_dto).collect(),
    })
}

fn price_error(message: &str) -> AppError {
    AppError::api(ErrorCode::ValidationAiPrice, message)
}

fn set_price(req: AiPriceDto, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AiPriceDto>> {
    let model = req.model.trim().to_owned();
    if model.is_empty() || model.chars().count() > 200 {
        return Err(price_error("model id must be 1–200 characters"));
    }
    let valid = |v: f64| v.is_finite() && (0.0..=1_000_000.0).contains(&v);
    if !valid(req.input_per_mtok) || !valid(req.output_per_mtok) {
        return Err(price_error("prices must be numbers from 0"));
    }
    let old = ctx
        .reads()
        .list_ai_prices()?
        .into_iter()
        .find(|p| p.model == model);
    let record = AiPriceRecord {
        model: model.clone(),
        input_per_mtok: req.input_per_mtok,
        output_per_mtok: req.output_per_mtok,
        updated_at: ctx.now,
    };
    ctx.tx.upsert_ai_price(&record)?;
    let inverse = match &old {
        Some(o) => inverse_call::<AiPricesSet>(&price_dto(o))?,
        None => inverse_call::<AiPricesDelete>(&AiPriceRef { model: model.clone() })?,
    };
    Ok(CommandOutcome {
        response: price_dto(&record),
        inverse: vec![inverse],
        summary: summary::<AiPricesSet>().param("model", &model),
        events: changed_event(),
    })
}

fn delete_price(req: AiPriceRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let old = ctx
        .reads()
        .list_ai_prices()?
        .into_iter()
        .find(|p| p.model == req.model);
    let Some(old) = old else {
        return Ok(CommandOutcome {
            response: Empty {},
            inverse: vec![],
            summary: summary::<AiPricesDelete>().param("model", &req.model),
            events: vec![],
        });
    };
    ctx.tx.delete_ai_price(&old.model)?;
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![inverse_call::<AiPricesSet>(&price_dto(&old))?],
        summary: summary::<AiPricesDelete>().param("model", &old.model),
        events: changed_event(),
    })
}
