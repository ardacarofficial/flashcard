//! `ai.sources.extract` → `ai.source` job: card-generation text from a PDF page range or a
//! web page (docs/06 §1.4). The job only reads a file or fetches a page; the text comes back
//! as the job result and goes into the editable source of `ai.preview`, so the preview rule
//! ("nothing is sent that was not shown") holds unchanged.

use super::context::MAX_SOURCE_CHARS;
use super::{ID, LANE, SOURCE_JOB, WEB_ALLOW_PRIVATE};
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome};
use crate::jobs::{JobContext, JobHandler};
use crate::registry::Registry;
use crate::support::summary;
use fc_ai::sources::{self, Extracted, SourceError};
use fc_api::v1::ai::{AiSourceInput, AiSourceText, ExtractAiSource};
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::AiSourcesExtract;
use fc_api::{ApiError, ErrorCode};
use fc_core::settings::SettingScope;
use serde_json::Value;
use std::sync::Arc;

pub(super) fn register(reg: &mut Registry) {
    reg.command::<AiSourcesExtract>(extract);
    reg.job(Arc::new(SourceJob));
}

fn invalid(reason: &'static str, message: impl Into<String>) -> AppError {
    AppError::Api(ApiError::new(ErrorCode::ValidationAiSource, message).with_detail("reason", reason))
}

/// Checks what can be checked without I/O, then queues the job.
fn extract(req: ExtractAiSource, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<JobDto>> {
    match &req.source {
        AiSourceInput::Pdf {
            path,
            from_page,
            to_page,
        } => {
            if !std::path::Path::new(path.trim()).is_absolute() {
                return Err(invalid("io", "the PDF path must be absolute"));
            }
            if from_page == &Some(0) || matches!((from_page, to_page), (Some(f), Some(t)) if f > t)
            {
                return Err(invalid("pages", "the page range is empty"));
            }
        }
        AiSourceInput::Url { url } => {
            let u = url.trim();
            if !(u.starts_with("https://") || u.starts_with("http://")) || u.len() > 2000 {
                return Err(invalid("url", "only http(s) addresses can be fetched"));
            }
        }
    }
    let payload =
        serde_json::to_value(&req).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
    let job = crate::jobs::enqueue(ctx, SOURCE_JOB, payload)?;
    Ok(CommandOutcome {
        response: job,
        inverse: vec![],
        summary: summary::<AiSourcesExtract>(),
        events: vec![],
    })
}

fn source_error(e: SourceError) -> AppError {
    let (code, retryable) = match e.reason {
        "http_status" | "unreachable" | "timeout" => (ErrorCode::ProviderAiSource, true),
        _ => (ErrorCode::ValidationAiSource, false),
    };
    AppError::Api(
        ApiError::new(code, e.to_string())
            .with_detail("reason", e.reason)
            .retryable(retryable),
    )
}

/// Cuts `text` to the source limit (on a char boundary).
fn limit(text: String) -> (String, bool) {
    match text.char_indices().nth(MAX_SOURCE_CHARS) {
        Some((cut, _)) => (text[..cut].trim_end().to_owned(), true),
        None => (text, false),
    }
}

/// Reads the source.
struct SourceJob;

impl JobHandler for SourceJob {
    fn kind(&self) -> &'static str {
        SOURCE_JOB
    }

    fn lane(&self) -> &'static str {
        LANE
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let req: ExtractAiSource = crate::support::from_json(&payload)?;
        ctx.ensure_module_enabled(ID)?;
        ctx.cancel.check()?;
        ctx.report(None, "ai.job.reading_source");
        let extracted: Extracted = match &req.source {
            AiSourceInput::Pdf {
                path,
                from_page,
                to_page,
            } => sources::pdf_file(std::path::Path::new(path.trim()), *from_page, *to_page),
            AiSourceInput::Url { url } => {
                let allow_private_hosts = {
                    let reads = ctx.read()?;
                    crate::settings::resolve_stored(&*reads, &SettingScope::App, WEB_ALLOW_PRIVATE)?
                        .and_then(|(v, _)| v.as_bool())
                        .unwrap_or(false)
                };
                sources::fetch_page_with(
                    url,
                    sources::FetchPolicy { allow_private_hosts },
                    &sources::SystemResolver,
                )
            }
        }
        .map_err(source_error)?;
        ctx.cancel.check()?;
        let (text, truncated) = limit(extracted.text);
        let result = AiSourceText {
            title: extracted.title,
            text,
            page_count: extracted.page_count,
            from_page: extracted.from_page,
            to_page: extracted.to_page,
            truncated,
        };
        serde_json::to_value(result).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
    }
}
