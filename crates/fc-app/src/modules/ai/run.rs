//! `ai.preview` → `ai.run` → `ai.request` job (and the `ai.models` refresh job).

use super::context::{build, require_provider, Built, Target};
use super::prompts::{parse, repair_message, OutputSpec, Parsed};
use super::providers::{dto, has_key, keychain_error, kind_of, needs_key, parse_kind};
use super::usage::{budget, cost};
use super::{key_account, AiServices, ID, LANE, MODELS_JOB, REQUEST_JOB, RETRIES, TEST_JOB, TIMEOUT};
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::jobs::{JobContext, JobHandler};
use crate::registry::Registry;
use crate::support::summary;
use fc_ai::secrets::Secret;
use fc_ai::{MediaSource, ProviderConfig};
use fc_api::v1::ai::*;
use fc_api::v1::events::DomainEvent;
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
use fc_storage::repo::AiProviderRecord;
use fc_storage::Reads;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Longest `Retry-After` a job waits for; a longer one fails the job ("try again later").
const MAX_HONOURED_RETRY_AFTER_MS: u64 = 60_000;

/// Longest wait of a connection test.
const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Least time between two `ai_partial` events of a streaming job.
const PARTIAL_EVERY: Duration = Duration::from_millis(60);

pub(super) fn register(reg: &mut Registry, services: &Arc<AiServices>) {
    let s = Arc::clone(services);
    reg.query::<PreviewMethod>(move |req: PreviewAi, ctx: &QueryCtx<'_>| {
        preview(&s, ctx.tx, ctx.registry, ctx.locale, ctx.now, &req)
    });
    let s = Arc::clone(services);
    reg.command::<AiRun>(move |req: RunAi, ctx: &mut CommandCtx<'_>| run(&s, req, ctx));
    reg.job(Arc::new(RequestJob(Arc::clone(services))));
    reg.job(Arc::new(ModelsJob(Arc::clone(services))));
    reg.job(Arc::new(TestJob(Arc::clone(services))));
}

/// Hash of exactly what will be sent and where: the provider, its protocol and address, the
/// model and the full request. Changing the provider's URL or kind after the preview makes
/// `ai.run` refuse (`CONFLICT_AI_PREVIEW_STALE`).
fn request_hash(provider: &AiProviderRecord, request: &CompletionRequest) -> AppResult<String> {
    let text = serde_json::to_string(&(
        provider.id,
        &provider.kind,
        &provider.base_url,
        &request.model,
        request,
    ))
    .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
    Ok(format!("{:016x}", fnv1a64(text.as_bytes())))
}

fn stale(message: &str) -> AppError {
    AppError::api(ErrorCode::ConflictAiPreviewStale, message)
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
        preview_hash: request_hash(&b.provider, &b.request)?,
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
    /// Protocol and address the request was previewed for: the job refuses to send if the
    /// provider changed since.
    #[serde(default)]
    kind: String,
    #[serde(default)]
    base_url: String,
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
    if request_hash(&built.provider, &built.request)? != req.preview_hash {
        return Err(stale("the request changed since it was previewed"));
    }
    if needs_key(&built.provider)? && !has_key(services, built.provider.id) {
        return Err(AppError::Api(
            ApiError::new(ErrorCode::ProviderAiUnauthorized, "no API key is set for this provider")
                .with_detail("provider", built.provider.name.clone()),
        ));
    }
    let payload = RequestPayload {
        provider_id: built.provider.id,
        kind: built.provider.kind.clone(),
        base_url: built.provider.base_url.clone(),
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
    config: ProviderConfig,
    record: AiProviderRecord,
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
    endpoint: Option<(&str, &str)>,
) -> AppResult<Connection> {
    let (record, timeout, retries, media) = {
        let reads = ctx.read()?;
        let record = require_provider(&*reads, id)?;
        if let Some((kind, base_url)) = endpoint {
            // The key goes only where the user saw the request going.
            if record.kind != kind || record.base_url != base_url || record.deleted_at.is_some() {
                return Err(stale("the provider changed since the request was previewed"));
            }
        }
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
    let kind = kind_of(parse_kind(&record.kind)?);
    // A key-less provider (Ollama, a local OpenAI-compatible server) must not die on a
    // missing/unusable keychain: no key is the normal case there.
    let key = match services.secrets().get(&key_account(id)) {
        Ok(key) => key,
        Err(_) if !needs_key(&record)? => None,
        Err(e) => return Err(keychain_error(e)),
    };
    let config = ProviderConfig {
        kind,
        base_url: record.base_url.clone(),
        api_key: key.clone(),
        timeout: Duration::from_secs(timeout),
        media: Some(Arc::new(Images(media))),
    };
    let provider = fc_ai::build(config.clone());
    Ok(Connection {
        provider,
        config,
        id,
        name: record.name.clone(),
        record,
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

/// Streamed output so far, published as throttled `ai_partial` events.
struct Partial<'a> {
    ctx: &'a JobContext<'a>,
    state: Mutex<(String, Option<Instant>)>,
}

impl<'a> Partial<'a> {
    fn new(ctx: &'a JobContext<'a>) -> Self {
        Self {
            ctx,
            state: Mutex::new((String::new(), None)),
        }
    }

    fn emit(&self, text: String) {
        self.ctx.publish(vec![DomainEvent::AiPartial {
            job_id: self.ctx.job_id,
            text,
        }]);
    }

    fn push(&self, piece: &str) {
        let Ok(mut s) = self.state.lock() else { return };
        s.0.push_str(piece);
        let due = s.1.is_none_or(|t| t.elapsed() >= PARTIAL_EVERY);
        if due {
            s.1 = Some(Instant::now());
            let text = s.0.clone();
            drop(s);
            self.emit(text);
        }
    }

    /// A new attempt starts from nothing (a retry must not append to a failed stream).
    fn reset(&self) {
        let Ok(mut s) = self.state.lock() else { return };
        let had = !s.0.is_empty();
        *s = (String::new(), None);
        drop(s);
        if had {
            self.emit(String::new());
        }
    }

    /// The last pieces, which throttling may have held back.
    fn flush(&self) {
        let text = match self.state.lock() {
            Ok(s) => s.0.clone(),
            Err(_) => return,
        };
        self.emit(text);
    }
}

/// Sends `request` (retrying rate limits and outages: after the provider's `Retry-After`, else
/// with growing, jittered pauses), records its usage. With `stream`, output text is published
/// as it arrives.
fn send(
    services: &AiServices,
    ctx: &JobContext<'_>,
    conn: &Connection,
    feature: AiFeature,
    request: &CompletionRequest,
    attempts: &mut u32,
    stream: Option<&Partial<'_>>,
) -> AppResult<CompletionResponse> {
    let mut attempt = 0;
    loop {
        ctx.cancel.check()?;
        // Turned off while queued or between retries: nothing more goes out.
        ctx.ensure_module_enabled(ID)?;
        *attempts += 1;
        let result = match stream {
            Some(partial) => {
                partial.reset();
                let on_text = |t: &str| partial.push(t);
                let r = fc_ai::block_on(conn.provider.complete_stream(request.clone(), &on_text));
                if r.is_ok() {
                    partial.flush();
                }
                r
            }
            None => fc_ai::block_on(conn.provider.complete(request.clone())),
        };
        match result {
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
            Err(e)
                if e.retryable
                    && attempt < conn.retries
                    && e.retry_after_ms.is_none_or(|ms| ms <= MAX_HONOURED_RETRY_AFTER_MS) =>
            {
                pause(ctx, services.retry_pause(attempt, e.retry_after_ms))?;
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

    fn lane(&self) -> &'static str {
        LANE
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
        // Queued before the module was turned off: the request never leaves.
        ctx.ensure_module_enabled(ID)?;
        ctx.report(None, "ai.job.connecting");
        let conn = connect(
            &self.0,
            ctx,
            payload.provider_id,
            &images,
            Some((&payload.kind, &payload.base_url)),
        )?;
        // Plain-text answers (explain, hint) are shown while they arrive.
        let partial = Partial::new(ctx);
        let stream = matches!(payload.spec, OutputSpec::Text).then_some(&partial);
        let mut attempts = 0;
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        ctx.report(None, "ai.job.waiting");
        let first = send(
            &self.0,
            ctx,
            &conn,
            payload.feature,
            &payload.request,
            &mut attempts,
            stream,
        )?;
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
                let second = send(&self.0, ctx, &conn, payload.feature, &repair, &mut attempts, None)?;
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

    fn lane(&self) -> &'static str {
        LANE
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let payload: ModelsPayload = crate::support::from_json(&payload)?;
        ctx.ensure_module_enabled(ID)?;
        let conn = connect(&self.0, ctx, payload.provider_id, &[], None)?;
        ctx.cancel.check()?;
        let models = fc_ai::block_on(conn.provider.list_models())
            .map_err(|e| provider_error(e, &conn.name, conn.key.as_ref()))?;
        ctx.cancel.check()?;
        ctx.invoke::<AiProvidersSetModels>(&SetAiModels {
            id: conn.id,
            models: models.clone(),
        })?;
        // The list as stored (sorted, deduplicated): what the picker will show.
        let stored = require_provider(&*ctx.read()?, conn.id)?.models;
        serde_json::to_value(AiModelList {
            provider_id: conn.id,
            models: stored,
        })
        .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
    }
}

/// Tests a provider's connection: one minimal request ([`fc_ai::diagnose`]), explained. The
/// result is the job's result and the provider's `last_test` for this app run. Its few tokens
/// are not recorded as usage (no feature asked for them; the probe is capped at
/// [`fc_ai::diagnose::PROBE_MAX_TOKENS`] output tokens).
struct TestJob(Arc<AiServices>);

impl JobHandler for TestJob {
    fn kind(&self) -> &'static str {
        TEST_JOB
    }

    fn lane(&self) -> &'static str {
        LANE
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        use fc_ai::diagnose::{test_connection, Outcome};
        let payload: ModelsPayload = crate::support::from_json(&payload)?;
        ctx.ensure_module_enabled(ID)?;
        ctx.report(None, "ai.job.connecting");
        let conn = connect(&self.0, ctx, payload.provider_id, &[], None)?;
        ctx.cancel.check()?;
        let model = Some(conn.record.default_model.trim())
            .filter(|m| !m.is_empty())
            .map(str::to_owned);
        // A test answers quickly or not at all: do not wait the full request timeout.
        let mut config = conn.config.clone();
        config.timeout = config.timeout.min(TEST_TIMEOUT);
        let d = test_connection(&config, model.as_deref());
        ctx.cancel.check()?;
        let outcome = match d.outcome {
            Outcome::Ok => AiTestOutcome::Ok,
            Outcome::NoKey => AiTestOutcome::NoKey,
            Outcome::WrongKey => AiTestOutcome::WrongKey,
            Outcome::NoCredit => AiTestOutcome::NoCredit,
            Outcome::ModelNotFound => AiTestOutcome::ModelNotFound,
            Outcome::WrongAddress => AiTestOutcome::WrongAddress,
            Outcome::RateLimited => AiTestOutcome::RateLimited,
            Outcome::ServiceDown => AiTestOutcome::ServiceDown,
            Outcome::CannotReach => AiTestOutcome::CannotReach,
            Outcome::Tls => AiTestOutcome::Tls,
            Outcome::Failed => AiTestOutcome::Failed,
        };
        let result = AiConnectionTest {
            provider_id: conn.id,
            outcome,
            model,
            answered_model: d.model,
            http_status: d.status,
            message: d.message,
            latency_ms: u32::try_from(d.latency_ms).unwrap_or(u32::MAX),
            checked_at: ctx.now(),
        };
        self.0.remember_test(&conn.record, result.clone());
        // The provider list shows the new status.
        ctx.publish(vec![DomainEvent::AiConfigChanged]);
        serde_json::to_value(result).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
    }
}
