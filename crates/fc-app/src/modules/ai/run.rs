//! `ai.preview` → `ai.run` → `ai.request` job (and the `ai.models` refresh job).

use super::context::{build, require_provider, Built, Target};
use super::prompts::{parse, repair_message, OutputSpec, Parsed};
use super::providers::{dto, has_key, keychain_error, kind_of, parse_kind};
use super::usage::{budget, cost};
use super::{key_account, AiServices, MODELS_JOB, REQUEST_JOB, RETRIES, TIMEOUT};
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::jobs::{JobContext, JobHandler};
use crate::registry::Registry;
use crate::support::summary;
use fc_ai::secrets::Secret;
use fc_ai::{MediaSource, ProviderConfig};
use fc_api::v1::ai::*;
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::{
    AiPreviewTask as PreviewMethod, AiProvidersSetModels, AiRun, AiUsageRecord as RecordMethod,
};
use fc_api::{ApiError, ErrorCode};
use fc_core::note::fnv1a64;
use fc_core::settings::SettingScope;
use fc_core::AiProviderId;
use fc_providers::ai::{AiProvider, CompletionRequest, CompletionResponse, ContentPart, Message, Role};
use fc_providers::{ProviderError, ProviderErrorKind};
use fc_storage::Reads;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

pub(super) fn register(reg: &mut Registry, services: &Arc<AiServices>) {
    let s = Arc::clone(services);
    reg.query::<PreviewMethod>(move |req: PreviewAi, ctx: &QueryCtx<'_>| {
        preview(&s, ctx.tx, ctx.registry, ctx.locale, ctx.now, &req)
    });
    let s = Arc::clone(services);
    reg.command::<AiRun>(move |req: RunAi, ctx: &mut CommandCtx<'_>| run(&s, req, ctx));
    reg.job(Arc::new(RequestJob(Arc::clone(services))));
    reg.job(Arc::new(ModelsJob(Arc::clone(services))));
}

/// Hash of exactly what will be sent (provider, model and the full request).
fn request_hash(provider: AiProviderId, request: &CompletionRequest) -> AppResult<String> {
    let text = serde_json::to_string(&(provider, request))
        .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
    Ok(format!("{:016x}", fnv1a64(text.as_bytes())))
}

/// ≈ 4 characters per token, ~1.5k tokens per image (an estimate shown as such).
fn estimate_input_tokens(request: &CompletionRequest) -> u32 {
    let mut chars = request.system.as_deref().map_or(0, |s| s.chars().count());
    let mut images = 0u32;
    for m in &request.messages {
        for p in &m.content {
            match p {
                ContentPart::Text { text } => chars += text.chars().count(),
                ContentPart::Image { .. } => images += 1,
            }
        }
    }
    if let Some(schema) = &request.json_schema {
        chars += schema.to_string().len();
    }
    u32::try_from(chars.div_ceil(4)).unwrap_or(u32::MAX).saturating_add(images * 1500)
}

fn preview_of(services: &AiServices, reads: &dyn Reads, now: fc_core::Timestamp, b: &Built) -> AppResult<AiPreview> {
    let prices = reads.list_ai_prices()?;
    let estimated_input_tokens = estimate_input_tokens(&b.request);
    let estimated_cost_usd = cost(
        &prices,
        &b.model,
        u64::from(estimated_input_tokens),
        u64::from(b.request.max_tokens),
    );
    let provider = dto(services, &b.provider)?;
    let missing_key = provider.needs_key && !provider.has_key;
    Ok(AiPreview {
        feature: b.feature,
        missing_key,
        provider,
        model: b.model.clone(),
        system: b.request.system.clone().unwrap_or_default(),
        messages: b
            .request
            .messages
            .iter()
            .map(|m| AiPreviewMessage {
                role: match m.role {
                    Role::User => AiRole::User,
                    Role::Assistant => AiRole::Assistant,
                },
                text: m
                    .content
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => Some(text.as_str()),
                        ContentPart::Image { .. } => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n"),
                images: m
                    .content
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Image { media_hash, .. } => Some(media_hash.clone()),
                        ContentPart::Text { .. } => None,
                    })
                    .collect(),
            })
            .collect(),
        max_tokens: b.request.max_tokens,
        structured: b.request.json_schema.is_some(),
        estimated_input_tokens,
        estimated_cost_usd,
        items: b.items,
        preview_hash: request_hash(b.provider.id, &b.request)?,
        budget: budget(reads, now, estimated_cost_usd.unwrap_or(0.0))?,
    })
}

fn preview(
    services: &AiServices,
    reads: &dyn Reads,
    registry: &Registry,
    locale: fc_core::i18n::Locale,
    now: fc_core::Timestamp,
    req: &PreviewAi,
) -> AppResult<AiPreview> {
    let built = build(reads, registry, locale, req)?;
    preview_of(services, reads, now, &built)
}

/// What the `ai.request` job runs: the previewed request itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RequestPayload {
    provider_id: AiProviderId,
    model: String,
    feature: AiFeature,
    request: CompletionRequest,
    spec: OutputSpec,
    target: Target,
}

fn run(services: &AiServices, req: RunAi, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<JobDto>> {
    let preview = PreviewAi {
        task: req.task,
        provider_id: req.provider_id,
        model: req.model,
    };
    let built = build(ctx.reads(), ctx.registry, ctx.locale, &preview)?;
    if request_hash(built.provider.id, &built.request)? != req.preview_hash {
        return Err(AppError::api(
            ErrorCode::ConflictAiPreviewStale,
            "the request changed since it was previewed",
        ));
    }
    if kind_of(parse_kind(&built.provider.kind)?).needs_key() && !has_key(services, built.provider.id) {
        return Err(AppError::Api(
            ApiError::new(ErrorCode::ProviderAiUnauthorized, "no API key is set for this provider")
                .with_detail("provider", built.provider.name.clone()),
        ));
    }
    let payload = RequestPayload {
        provider_id: built.provider.id,
        model: built.model,
        feature: built.feature,
        request: built.request,
        spec: built.spec,
        target: built.target,
    };
    let payload =
        serde_json::to_value(&payload).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
    let job = crate::jobs::enqueue(ctx, REQUEST_JOB, payload)?;
    Ok(CommandOutcome {
        response: job,
        inverse: vec![],
        summary: summary::<AiRun>(),
        events: vec![],
    })
}

// ------------------------------------------------------------------ jobs

/// Media bytes read up front (the provider runs outside any transaction).
struct Images(HashMap<String, Vec<u8>>);

impl MediaSource for Images {
    fn media_bytes(&self, hash: &str) -> Option<Vec<u8>> {
        self.0.get(hash).cloned()
    }
}

fn setting_u64(reads: &dyn Reads, key: &str, default: u64) -> AppResult<u64> {
    Ok(crate::settings::resolve_stored(reads, &SettingScope::App, key)?
        .and_then(|(v, _)| v.as_u64())
        .unwrap_or(default))
}

/// Provider error → API error. The message is scrubbed of the key again (plugin providers
/// may not do it themselves).
fn provider_error(e: ProviderError, provider: &str, key: Option<&Secret>) -> AppError {
    let code = match e.kind {
        ProviderErrorKind::Unauthorized => ErrorCode::ProviderAiUnauthorized,
        ProviderErrorKind::RateLimited => ErrorCode::ProviderAiRateLimited,
        ProviderErrorKind::InvalidRequest => ErrorCode::ProviderAiInvalidRequest,
        ProviderErrorKind::Unavailable => ErrorCode::ProviderUnavailable,
        ProviderErrorKind::Cancelled => ErrorCode::ConflictJobCancelled,
        ProviderErrorKind::Failed => ErrorCode::ProviderFailed,
    };
    let message = fc_ai::redact(&e.message, key);
    AppError::Api(
        ApiError::new(code, message.clone())
            .with_detail("provider", provider)
            .with_detail("reason", message)
            .retryable(e.retryable),
    )
}

/// The provider of a job: row, key (from the keychain), timeout.
struct Connection {
    provider: Arc<dyn AiProvider>,
    id: AiProviderId,
    name: String,
    key: Option<Secret>,
    retries: u32,
}

fn connect(
    services: &AiServices,
    ctx: &JobContext<'_>,
    id: AiProviderId,
    images: &[String],
) -> AppResult<Connection> {
    let (record, timeout, retries, media) = {
        let reads = ctx.read()?;
        let record = require_provider(&*reads, id)?;
        let timeout = setting_u64(&*reads, TIMEOUT, 120)?.clamp(5, 600);
        let retries = setting_u64(&*reads, RETRIES, 2)?.min(5) as u32;
        let mut media = HashMap::new();
        for h in images {
            if let Some(bytes) = reads.read_media_file(h)? {
                media.insert(h.clone(), bytes);
            }
        }
        (record, timeout, retries, media)
    };
    let key = services.secrets().get(&key_account(id)).map_err(keychain_error)?;
    let kind = kind_of(parse_kind(&record.kind)?);
    let provider = fc_ai::build(ProviderConfig {
        kind,
        base_url: record.base_url.clone(),
        api_key: key.clone(),
        timeout: Duration::from_secs(timeout),
        media: Some(Arc::new(Images(media))),
    });
    Ok(Connection {
        provider,
        id,
        name: record.name,
        key,
        retries,
    })
}

/// Sleeps `d` in small steps, stopping early (with an error) if the job is cancelled.
fn pause(ctx: &JobContext<'_>, d: Duration) -> AppResult<()> {
    let step = Duration::from_millis(50);
    let mut left = d;
    while !left.is_zero() {
        ctx.cancel.check()?;
        let s = left.min(step);
        std::thread::sleep(s);
        left -= s;
    }
    ctx.cancel.check()
}

/// Sends `request` (retrying rate limits and outages with growing pauses), records its usage.
fn send(
    services: &AiServices,
    ctx: &JobContext<'_>,
    conn: &Connection,
    feature: AiFeature,
    request: &CompletionRequest,
    attempts: &mut u32,
) -> AppResult<CompletionResponse> {
    let mut attempt = 0;
    loop {
        ctx.cancel.check()?;
        *attempts += 1;
        match fc_ai::block_on(conn.provider.complete(request.clone())) {
            Ok(response) => {
                // Spent tokens are recorded even if the job is cancelled right after.
                ctx.invoke::<RecordMethod>(&RecordAiUsage {
                    provider_id: conn.id,
                    model: response.model.clone(),
                    feature,
                    input_tokens: response.usage.input_tokens,
                    output_tokens: response.usage.output_tokens,
                })?;
                ctx.cancel.check()?;
                return Ok(response);
            }
            Err(e) if e.retryable && attempt < conn.retries => {
                pause(ctx, services.retry_pause(attempt))?;
                attempt += 1;
            }
            Err(e) => return Err(provider_error(e, &conn.name, conn.key.as_ref())),
        }
    }
}

fn output(parsed: Parsed, payload: &RequestPayload) -> AppResult<AiOutput> {
    let t = &payload.target;
    let missing = || AppError::api(ErrorCode::Internal, "job target incomplete");
    Ok(match (payload.feature, parsed) {
        (AiFeature::GenerateCards, Parsed::Notes(notes)) => AiOutput::GenerateCards {
            deck_id: t.deck_id.ok_or_else(missing)?,
            notetype_id: t.notetype_id.ok_or_else(missing)?,
            notes,
            attributes: t.attributes.clone(),
        },
        (AiFeature::FillFields, Parsed::Fill(fields)) => AiOutput::FillFields { fields },
        (AiFeature::Explain, Parsed::Text(text)) => AiOutput::Explain { text },
        (AiFeature::Hint, Parsed::Text(text)) => AiOutput::Hint { text },
        (AiFeature::CheckAnswer, Parsed::Verdict(verdict, feedback)) => {
            AiOutput::CheckAnswer { verdict, feedback }
        }
        (AiFeature::Improve, Parsed::Rewrite(changes, reason)) => AiOutput::Improve {
            note_id: t.note_id.ok_or_else(missing)?,
            changes,
            reason,
        },
        (AiFeature::RewriteLeech, Parsed::Rewrite(changes, mnemonic)) => AiOutput::RewriteLeech {
            note_id: t.note_id.ok_or_else(missing)?,
            changes,
            mnemonic,
        },
        (AiFeature::Distractors, Parsed::Options(items)) => AiOutput::Distractors {
            note_id: t.note_id.ok_or_else(missing)?,
            items,
            field: t.field.clone(),
        },
        _ => return Err(AppError::api(ErrorCode::Internal, "output does not match the feature")),
    })
}

/// Runs a confirmed request.
struct RequestJob(Arc<AiServices>);

impl JobHandler for RequestJob {
    fn kind(&self) -> &'static str {
        REQUEST_JOB
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let payload: RequestPayload = crate::support::from_json(&payload)?;
        let images: Vec<String> = payload
            .request
            .messages
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|p| match p {
                ContentPart::Image { media_hash, .. } => Some(media_hash.clone()),
                ContentPart::Text { .. } => None,
            })
            .collect();
        ctx.report(None, "ai.job.connecting");
        let conn = connect(&self.0, ctx, payload.provider_id, &images)?;
        let mut attempts = 0;
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        ctx.report(None, "ai.job.waiting");
        let first = send(&self.0, ctx, &conn, payload.feature, &payload.request, &mut attempts)?;
        input_tokens += first.usage.input_tokens;
        output_tokens += first.usage.output_tokens;
        let mut truncated = first.truncated;
        let mut model = first.model.clone();
        let parsed = match parse(&payload.spec, &first.text) {
            Ok(p) => p,
            Err(problem) => {
                // One repair attempt: the same request, the model's answer and what is wrong.
                ctx.report(None, "ai.job.repairing");
                let mut repair = payload.request.clone();
                repair.messages.push(Message {
                    role: Role::Assistant,
                    content: vec![ContentPart::Text {
                        text: if first.text.trim().is_empty() {
                            "(empty)".to_owned()
                        } else {
                            first.text.clone()
                        },
                    }],
                });
                repair.messages.push(Message {
                    role: Role::User,
                    content: vec![ContentPart::Text {
                        text: repair_message(&problem),
                    }],
                });
                let second = send(&self.0, ctx, &conn, payload.feature, &repair, &mut attempts)?;
                input_tokens += second.usage.input_tokens;
                output_tokens += second.usage.output_tokens;
                truncated = second.truncated;
                model = second.model.clone();
                parse(&payload.spec, &second.text).map_err(|reason| {
                    AppError::Api(
                        ApiError::new(ErrorCode::ProviderAiInvalidOutput, reason.clone())
                            .with_detail("provider", conn.name.clone())
                            .with_detail("reason", reason),
                    )
                })?
            }
        };
        let prices = ctx.read()?.list_ai_prices()?;
        let result = AiJobResult {
            output: output(parsed, &payload)?,
            provider_id: conn.id,
            cost_usd: cost(&prices, &model, u64::from(input_tokens), u64::from(output_tokens)),
            model,
            input_tokens,
            output_tokens,
            attempts,
            truncated,
        };
        serde_json::to_value(result).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
    }
}

/// Fetches a provider's model list.
struct ModelsJob(Arc<AiServices>);

#[derive(Deserialize)]
struct ModelsPayload {
    provider_id: AiProviderId,
}

impl JobHandler for ModelsJob {
    fn kind(&self) -> &'static str {
        MODELS_JOB
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let payload: ModelsPayload = crate::support::from_json(&payload)?;
        let conn = connect(&self.0, ctx, payload.provider_id, &[])?;
        ctx.cancel.check()?;
        let models = fc_ai::block_on(conn.provider.list_models())
            .map_err(|e| provider_error(e, &conn.name, conn.key.as_ref()))?;
        ctx.cancel.check()?;
        ctx.invoke::<AiProvidersSetModels>(&SetAiModels {
            id: conn.id,
            models: models.clone(),
        })?;
        Ok(serde_json::json!({ "models": models }))
    }
}
