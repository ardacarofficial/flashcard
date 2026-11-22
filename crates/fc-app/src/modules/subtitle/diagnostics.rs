//! v1.4 A5: "what happened to my last capture?" Every capture attempt the Subtitle extension
//! makes (`POST /subtitle/v1/captures`, each item of a batch) leaves one local row: when, the
//! result (`created`, `duplicate`, `rejected` + reason, `media_missing` + reason), the matched
//! card and how long it took. Readable through `subtitle.captures.recent` (and `fc call`),
//! included in the log bundle (`maintenance.logs.export`).
//!
//! **Privacy:** only ids, codes and times. Never the sentence, the word, the meaning, a URL or
//! a video title — the capture id is kept only when it is a well-formed UUIDv7, a reason only
//! as a code plus a field *name*. Kept bounded ([`KEEP`] newest rows); diagnostics, not user
//! content (never in the oplog or a backup). Part of the `subtitle` module: off, nothing is
//! recorded.

use super::capture;
use crate::app::App;
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::support::summary;
use fc_api::envelope::InvokeRequest;
use fc_api::v1::common::Empty;
use fc_api::v1::methods::SubtitleCapturesLog;
use fc_api::v1::subtitle::*;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_storage::repo::CaptureLogRecord;
use fc_storage::Reads;
use serde_json::Value;
use std::time::Instant;

/// Attempts kept (older rows are deleted as new ones arrive).
pub const KEEP: u32 = 500;
/// Attempts `subtitle.captures.recent` returns without a `limit`.
pub const DEFAULT_RECENT: u32 = 50;
/// Longest stored reason (characters).
const MAX_REASON: usize = 200;

fn result_str(r: SubtitleAttemptResult) -> &'static str {
    match r {
        SubtitleAttemptResult::Created => "created",
        SubtitleAttemptResult::Duplicate => "duplicate",
        SubtitleAttemptResult::Rejected => "rejected",
        SubtitleAttemptResult::MediaMissing => "media_missing",
    }
}

fn result_of(s: &str) -> SubtitleAttemptResult {
    match s {
        "created" => SubtitleAttemptResult::Created,
        "duplicate" => SubtitleAttemptResult::Duplicate,
        "media_missing" => SubtitleAttemptResult::MediaMissing,
        _ => SubtitleAttemptResult::Rejected,
    }
}

/// A field name or code as stored: `[a-z0-9._-]` only (anything else becomes `?`), short.
fn token(s: &str) -> String {
    s.chars()
        .take(64)
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '?'
            }
        })
        .collect()
}

/// `rejected` reason: the contract's error code, plus `:<field>` when the error names one.
pub(crate) fn rejection_reason(e: &ApiError) -> String {
    let code = super::error_code(e);
    match e.details.get("field").and_then(Value::as_str) {
        Some(field) => format!("{code}:{}", token(field)),
        None => code.to_owned(),
    }
}

/// Why a referenced upload will not reach the card (the same checks as the capture makes):
/// `None` when it will.
fn upload_problem(
    reads: &dyn Reads,
    capture_id: &str,
    kind: &str,
    r: &SubtitleMediaRef,
) -> AppResult<Option<&'static str>> {
    let Some(u) = reads.get_subtitle_upload(&r.upload_id)? else {
        return Ok(Some("upload_unknown"));
    };
    if u.capture_id != capture_id || u.kind != kind {
        return Ok(Some("upload_mismatch"));
    }
    if reads.get_media(&u.media_hash)?.is_none() && reads.read_media_file(&u.media_hash)?.is_none()
    {
        return Ok(Some("file_missing"));
    }
    Ok(None)
}

/// `media_missing` reason of a capture (`screenshot:upload_unknown,audio:file_missing`), or
/// `None` when every picture and sound it refers to is there.
pub(crate) fn missing_media(reads: &dyn Reads, c: &SubtitleCapture) -> AppResult<Option<String>> {
    let mut out = Vec::new();
    for (kind, r) in [
        ("screenshot", c.media.screenshot.as_ref()),
        ("audio", c.media.audio.as_ref()),
    ] {
        if let Some(r) = r {
            if let Some(why) = upload_problem(reads, &c.capture_id, kind, r)? {
                out.push(format!("{kind}:{why}"));
            }
        }
    }
    Ok((!out.is_empty()).then(|| out.join(",")))
}

fn elapsed_ms(started: Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

/// One capture attempt, observed: parses `value`, runs `record` (the capture command) on the
/// parsed capture, and notes what happened. A failure to note it never fails the capture.
pub(crate) fn observe(
    app: &App,
    key: &SubtitleKeyDto,
    value: &Value,
    record: impl FnOnce(SubtitleCapture) -> Result<SubtitleCaptureOutcome, ApiError>,
) -> Result<SubtitleCaptureOutcome, ApiError> {
    let started = Instant::now();
    let at = app.now();
    let sent_id = capture::capture_id_of(value);
    let capture_id = if capture::is_uuid_v7(&sent_id) {
        sent_id
    } else {
        String::new()
    };
    let parsed = capture::parse(value);
    let media = match &parsed {
        Ok(c) => app
            .store()
            .read()
            .ok()
            .and_then(|tx| missing_media(&*tx, c).ok())
            .flatten(),
        Err(_) => None,
    };
    let outcome = parsed.and_then(record);
    let (result, reason, card_id) = match &outcome {
        Ok(o) => match (o.status, media) {
            (SubtitleCaptureStatus::Duplicate, _) => {
                (SubtitleAttemptResult::Duplicate, None, Some(o.card_id))
            }
            (SubtitleCaptureStatus::Created, None) => {
                (SubtitleAttemptResult::Created, None, Some(o.card_id))
            }
            (SubtitleCaptureStatus::Created, Some(why)) => {
                (SubtitleAttemptResult::MediaMissing, Some(why), Some(o.card_id))
            }
        },
        Err(e) => (SubtitleAttemptResult::Rejected, Some(rejection_reason(e)), None),
    };
    let attempt = SubtitleCaptureAttempt {
        capture_id,
        at,
        result,
        reason,
        card_id,
        duration_ms: elapsed_ms(started),
    };
    note(app, key, attempt);
    outcome
}

/// Writes one attempt through the internal command (errors are dropped: diagnostics never
/// cost a capture).
fn note(app: &App, key: &SubtitleKeyDto, attempt: SubtitleCaptureAttempt) {
    let Ok(params) = serde_json::to_value(LogSubtitleCaptureAttempt { attempt }) else {
        return;
    };
    let _ = app.invoke_trusted(
        &InvokeRequest {
            method: format!("{}.{}", fc_api::API_VERSION, SubtitleCapturesLog::NAME),
            params,
        },
        &super::actor(key),
    );
}

fn invalid(field: &str, message: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationInvalidParams, message.to_owned())
            .with_detail("field", field),
    )
}

/// Internal `subtitle.captures.log`.
pub(super) fn log(
    req: LogSubtitleCaptureAttempt,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<Empty>> {
    let a = req.attempt;
    if !(a.capture_id.is_empty() || capture::is_uuid_v7(&a.capture_id)) {
        return Err(invalid("attempt.capture_id", "capture_id must be a UUIDv7 or empty"));
    }
    if a.reason.as_ref().is_some_and(|r| r.chars().count() > MAX_REASON) {
        return Err(invalid("attempt.reason", "reason is too long"));
    }
    ctx.tx.append_capture_log(
        &CaptureLogRecord {
            seq: 0,
            capture_id: a.capture_id,
            at: a.at,
            result: result_str(a.result).to_owned(),
            reason: a.reason,
            card_id: a.card_id,
            duration_ms: i64::from(a.duration_ms),
        },
        KEEP,
    )?;
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![],
        summary: summary::<SubtitleCapturesLog>(),
        events: vec![],
    })
}

fn attempt_dto(r: CaptureLogRecord) -> SubtitleCaptureAttempt {
    SubtitleCaptureAttempt {
        capture_id: r.capture_id,
        at: r.at,
        result: result_of(&r.result),
        reason: r.reason,
        card_id: r.card_id,
        duration_ms: u32::try_from(r.duration_ms.max(0)).unwrap_or(u32::MAX),
    }
}

/// `subtitle.captures.recent`.
pub(super) fn recent(
    req: RecentSubtitleCaptures,
    ctx: &QueryCtx<'_>,
) -> AppResult<SubtitleCaptureAttempts> {
    let limit = req.limit.unwrap_or(DEFAULT_RECENT).min(KEEP);
    Ok(SubtitleCaptureAttempts {
        attempts: ctx
            .tx
            .recent_capture_log(limit)?
            .into_iter()
            .map(attempt_dto)
            .collect(),
        kept: KEEP,
    })
}

/// The kept attempts as text for the log bundle (`None` when there are none): one line each,
/// oldest first — `time result reason capture_id card_id duration`.
pub(crate) fn export_text(reads: &dyn Reads) -> AppResult<Option<String>> {
    let rows = reads.recent_capture_log(KEEP)?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut s = String::from("# Subtitle capture attempts, oldest first (no text, only ids)\n");
    for r in rows.into_iter().rev() {
        s.push_str(&format!(
            "{} {} {} {} {} {}ms\n",
            fc_core::maintenance::iso_datetime(r.at),
            r.result,
            r.reason.as_deref().unwrap_or("-"),
            if r.capture_id.is_empty() {
                "-"
            } else {
                &r.capture_id
            },
            r.card_id.map_or_else(|| "-".to_owned(), |c| c.to_string()),
            r.duration_ms
        ));
    }
    Ok(Some(s))
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
