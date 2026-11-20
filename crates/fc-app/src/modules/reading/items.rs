//! Reading items and the incremental-reading queue.

use super::{bad_range, language_or_default, Utf16, MAX_BODY_BYTES};
use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::registry::Registry;
use crate::support::{err_id, summary};
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::reading::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::reading::{next_reading_interval, reading_queue_key, ReadingPriority};
use fc_core::time::MILLIS_PER_DAY;
use fc_core::{ReadingItemId, Timestamp};
use fc_storage::repo::ReadingItemRecord;
use fc_storage::Reads;

pub(super) fn register(reg: &mut Registry) {
    reg.query::<ReadingItemsList>(list);
    reg.query::<ReadingItemsGet>(get);
    reg.command::<ReadingItemsCreate>(create);
    reg.command::<ReadingItemsUpdate>(update);
    reg.command::<ReadingItemsSetPosition>(set_position);
    reg.command::<ReadingItemsDelete>(delete);
    reg.command::<ReadingItemsUndelete>(undelete);
    reg.command::<ReadingItemsRestoreState>(restore_state);
    reg.command::<ReadingQueueSet>(queue_set);
    reg.command::<ReadingQueueRead>(queue_read);
    reg.command::<ReadingQueuePostpone>(queue_postpone);
    reg.command::<ReadingExtractsCreate>(create_extract);
}

/// Longest title (chars).
const MAX_TITLE_CHARS: usize = 200;
/// Characters of the text a default title is made of.
const TITLE_FROM_TEXT_CHARS: usize = 60;

// ------------------------------------------------------------------ helpers

/// Live (not trashed) item, with body.
pub(crate) fn require_item(reads: &dyn Reads, id: ReadingItemId) -> AppResult<ReadingItemRecord> {
    reads
        .get_reading_item(id)?
        .filter(|i| i.deleted_at.is_none())
        .ok_or_else(|| not_found(id))
}

fn not_found(id: ReadingItemId) -> AppError {
    err_id(
        ErrorCode::NotFoundReadingItem,
        format!("reading item {id} not found"),
        id,
    )
}

fn u32_of(v: i64) -> u32 {
    u32::try_from(v.max(0)).unwrap_or(u32::MAX)
}

fn priority_of(s: &str) -> ReadingPriority {
    ReadingPriority::parse(s).unwrap_or_default()
}

/// Item → DTO at `now`.
pub(crate) fn dto(r: &ReadingItemRecord, now: Timestamp) -> AppResult<ReadingItemDto> {
    let kind = ReadingItemKind::parse(&r.kind).ok_or_else(|| {
        AppError::api(ErrorCode::Internal, format!("unknown reading kind {}", r.kind))
    })?;
    let length = u32_of(r.length);
    let position = u32_of(r.position).min(length);
    Ok(ReadingItemDto {
        id: r.id,
        kind,
        parent_id: r.parent_id,
        title: r.title.clone(),
        language: r.language.clone(),
        source: r.source.clone(),
        media_path: r.media_path.clone(),
        length,
        position,
        progress_pct: if length == 0 {
            0.0
        } else {
            (f64::from(position) * 1000.0 / f64::from(length)).round() / 10.0
        },
        cue_count: u32_of(r.cue_count),
        queued: r.queued,
        priority: priority_of(&r.priority),
        due: r.due,
        is_due: r.queued && r.due.is_some_and(|d| d <= now),
        interval_days: u32_of(r.interval_days),
        read_count: u32_of(r.read_count),
        last_read: r.last_read,
        created_at: r.created_at,
        updated_at: r.updated_at,
    })
}

/// The mutable columns of `r` (inverse of the state-changing commands).
fn state_of(r: &ReadingItemRecord) -> ReadingItemState {
    ReadingItemState {
        id: r.id,
        title: r.title.clone(),
        language: r.language.clone(),
        media_path: r.media_path.clone(),
        position: r.position,
        queued: r.queued,
        priority: priority_of(&r.priority),
        due: r.due,
        interval_days: r.interval_days,
        read_count: r.read_count,
        last_read: r.last_read,
        updated_at: r.updated_at,
    }
}

fn restore_call(r: &ReadingItemRecord) -> AppResult<fc_api::InvokeRequest> {
    inverse_call::<ReadingItemsRestoreState>(&state_of(r))
}

/// Writes `new` (an edit of `old`) and returns the outcome with the restore-state inverse.
fn save<M: fc_api::ApiMethod>(
    ctx: &mut CommandCtx<'_>,
    old: &ReadingItemRecord,
    new: ReadingItemRecord,
) -> AppResult<CommandOutcome<ReadingItemDto>> {
    ctx.tx.upsert_reading_item(&new)?;
    Ok(CommandOutcome {
        response: dto(&new, ctx.now)?,
        inverse: vec![restore_call(old)?],
        summary: summary::<M>().param("title", new.title.clone()),
        events: vec![DomainEvent::ReadingItemsChanged],
    })
}

/// Title: trimmed, else the first words of `body`; cut to [`MAX_TITLE_CHARS`].
fn title_or_default(title: &str, body: &str) -> String {
    let t = title.trim();
    let base: String = if t.is_empty() {
        let words = body.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut s: String = words.chars().take(TITLE_FROM_TEXT_CHARS).collect();
        if words.chars().count() > TITLE_FROM_TEXT_CHARS {
            s.push('…');
        }
        s
    } else {
        t.to_owned()
    };
    base.chars().take(MAX_TITLE_CHARS).collect()
}

fn no_text(field: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationReadingText, "the text is empty or too long")
            .with_detail("field", field),
    )
}

/// Permission to name the media file of a subtitle item: a **free file path** the app reads
/// and hands to ffmpeg (`reading.clips.extract`). `io:read` like the importers': the user,
/// the CLI and the app hold it; plugins never can (docs/05 §13), MCP and the v0.9 actors do
/// not. Without it `media_path` must be absent (an update keeps the item's file).
pub(crate) const MEDIA_PATH_PERMISSION: &str = "io:read";

/// The media path `ctx.actor` may give an item that has `old`: validated with
/// [`media_path`] for callers holding [`MEDIA_PATH_PERMISSION`]; for anyone else a given path
/// is `PERMISSION_DENIED` and `old` stays.
fn caller_media_path(
    ctx: &CommandCtx<'_>,
    raw: Option<&str>,
    old: Option<&str>,
) -> AppResult<Option<String>> {
    if ctx.registry.permits(ctx.actor, MEDIA_PATH_PERMISSION) {
        return media_path(raw);
    }
    if raw.map(str::trim).is_some_and(|p| !p.is_empty()) {
        return Err(AppError::Api(
            ApiError::new(ErrorCode::PermissionDenied, "only the user may choose a media file")
                .with_detail("field", "media_path")
                .with_detail("permission", MEDIA_PATH_PERMISSION),
        ));
    }
    Ok(old.map(str::to_owned))
}

/// An optional absolute media path (trimmed; empty = none).
pub(crate) fn media_path(raw: Option<&str>) -> AppResult<Option<String>> {
    match raw.map(str::trim).filter(|p| !p.is_empty()) {
        None => Ok(None),
        Some(p) if std::path::Path::new(p).is_absolute() && p.len() <= 4096 => {
            Ok(Some(p.to_owned()))
        }
        Some(_) => Err(AppError::Api(
            ApiError::new(ErrorCode::ValidationReadingClip, "the media path must be absolute")
                .with_detail("reason", "path"),
        )),
    }
}

fn days_after(now: Timestamp, days: u32) -> Timestamp {
    Timestamp(now.0.saturating_add(i64::from(days).saturating_mul(MILLIS_PER_DAY)))
}

// ------------------------------------------------------------------ queries

fn list(req: ListReadingItems, ctx: &QueryCtx<'_>) -> AppResult<ReadingItemList> {
    let mut rows = ctx.tx.list_reading_items(false)?;
    let due_count = rows
        .iter()
        .filter(|r| r.queued && r.due.is_some_and(|d| d <= ctx.now))
        .count();
    if req.queued_only {
        rows.retain(|r| r.queued);
        // Due texts first, by priority, then oldest due (fc_core::reading::reading_queue_key).
        rows.sort_by_key(|r| {
            let key = reading_queue_key(r.due.map(|d| d.0), priority_of(&r.priority), ctx.now.0);
            (key, r.created_at)
        });
    }
    Ok(ReadingItemList {
        items: rows
            .iter()
            .map(|r| dto(r, ctx.now))
            .collect::<AppResult<_>>()?,
        due_count: u32::try_from(due_count).unwrap_or(u32::MAX),
    })
}

fn get(req: ReadingItemRef, ctx: &QueryCtx<'_>) -> AppResult<ReadingItemDto> {
    dto(&require_item(ctx.tx, req.id)?, ctx.now)
}

// ------------------------------------------------------------------ commands

fn create(
    req: CreateReadingItem,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ReadingItemDto>> {
    let language = language_or_default(ctx.reads(), &req.language)?;
    let (kind, body, cues) = match &req.cues {
        Some(cues) => {
            let lines: Vec<String> = cues
                .iter()
                .map(|c| c.text.split_whitespace().collect::<Vec<_>>().join(" "))
                .collect();
            if cues.is_empty() || lines.iter().all(String::is_empty) {
                return Err(no_text("cues"));
            }
            let clean: Vec<CueDto> = cues
                .iter()
                .zip(&lines)
                .map(|(c, t)| CueDto {
                    start_ms: c.start_ms,
                    end_ms: c.end_ms.max(c.start_ms),
                    text: t.clone(),
                })
                .collect();
            let json = serde_json::to_string(&clean)
                .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
            (ReadingItemKind::Subtitle, lines.join("\n"), Some((json, clean.len())))
        }
        None => {
            let body = req.body.replace("\r\n", "\n").replace('\r', "\n");
            if body.trim().is_empty() {
                return Err(no_text("body"));
            }
            (ReadingItemKind::Text, body, None)
        }
    };
    if body.len() > MAX_BODY_BYTES {
        return Err(no_text(if cues.is_some() { "cues" } else { "body" }));
    }
    let r = ReadingItemRecord {
        id: ReadingItemId::generate(ctx.ids),
        kind: kind.as_str().to_owned(),
        parent_id: None,
        title: title_or_default(&req.title, &body),
        language,
        source: req
            .source
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().take(2000).collect()),
        length: i64::from(Utf16::len(&body)),
        body,
        cue_count: cues.as_ref().map_or(0, |c| i64::try_from(c.1).unwrap_or(i64::MAX)),
        cues: cues.map(|c| c.0),
        media_path: caller_media_path(ctx, req.media_path.as_deref(), None)?,
        extract_range: None,
        position: 0,
        queued: req.queue,
        priority: req.priority.as_str().to_owned(),
        due: req.queue.then_some(ctx.now),
        interval_days: 0,
        read_count: 0,
        last_read: None,
        created_at: ctx.now,
        updated_at: ctx.now,
        deleted_at: None,
    };
    ctx.tx.upsert_reading_item(&r)?;
    Ok(CommandOutcome {
        response: dto(&r, ctx.now)?,
        inverse: vec![inverse_call::<ReadingItemsDelete>(&ReadingItemRef { id: r.id })?],
        summary: summary::<ReadingItemsCreate>().param("title", r.title.clone()),
        events: vec![DomainEvent::ReadingItemsChanged],
    })
}

fn update(
    req: UpdateReadingItem,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ReadingItemDto>> {
    let old = require_item(ctx.reads(), req.id)?;
    let title = req.title.trim();
    if title.is_empty() {
        return Err(no_text("title"));
    }
    let new = ReadingItemRecord {
        title: title.chars().take(MAX_TITLE_CHARS).collect(),
        language: language_or_default(ctx.reads(), &req.language)?,
        media_path: caller_media_path(ctx, req.media_path.as_deref(), old.media_path.as_deref())?,
        updated_at: ctx.now,
        ..old.clone()
    };
    save::<ReadingItemsUpdate>(ctx, &old, new)
}

fn set_position(
    req: SetReadingPosition,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ReadingItemDto>> {
    let old = require_item(ctx.reads(), req.id)?;
    let new = ReadingItemRecord {
        position: i64::from(req.position).min(old.length),
        updated_at: ctx.now,
        ..old.clone()
    };
    save::<ReadingItemsSetPosition>(ctx, &old, new)
}

fn delete(req: ReadingItemRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ReadingItemDto>> {
    let mut r = require_item(ctx.reads(), req.id)?;
    ctx.tx.set_reading_items_deleted(&[r.id], Some(ctx.now))?;
    r.deleted_at = Some(ctx.now);
    Ok(CommandOutcome {
        response: dto(&r, ctx.now)?,
        inverse: vec![inverse_call::<ReadingItemsUndelete>(&req)?],
        summary: summary::<ReadingItemsDelete>().param("title", r.title.clone()),
        events: vec![DomainEvent::ReadingItemsChanged],
    })
}

fn undelete(req: ReadingItemRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ReadingItemDto>> {
    let mut r = ctx
        .reads()
        .get_reading_item(req.id)?
        .ok_or_else(|| not_found(req.id))?;
    ctx.tx.set_reading_items_deleted(&[r.id], None)?;
    r.deleted_at = None;
    Ok(CommandOutcome {
        response: dto(&r, ctx.now)?,
        inverse: vec![inverse_call::<ReadingItemsDelete>(&req)?],
        summary: summary::<ReadingItemsUndelete>().param("title", r.title.clone()),
        events: vec![DomainEvent::ReadingItemsChanged],
    })
}

fn restore_state(
    req: ReadingItemState,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ReadingItemDto>> {
    let old = ctx
        .reads()
        .get_reading_item(req.id)?
        .ok_or_else(|| not_found(req.id))?;
    let new = ReadingItemRecord {
        title: req.title,
        language: req.language,
        media_path: req.media_path,
        position: req.position.clamp(0, old.length),
        queued: req.queued,
        priority: req.priority.as_str().to_owned(),
        due: req.due,
        interval_days: req.interval_days.max(0),
        read_count: req.read_count.max(0),
        last_read: req.last_read,
        updated_at: req.updated_at,
        ..old.clone()
    };
    save::<ReadingItemsRestoreState>(ctx, &old, new)
}

fn queue_set(req: SetReadingQueue, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ReadingItemDto>> {
    let old = require_item(ctx.reads(), req.id)?;
    let priority = req
        .priority
        .map_or_else(|| old.priority.clone(), |p| p.as_str().to_owned());
    let new = if req.queued {
        ReadingItemRecord {
            queued: true,
            priority,
            // Joining the queue: due now; already queued: keep its schedule.
            due: if old.queued { old.due.or(Some(ctx.now)) } else { Some(ctx.now) },
            updated_at: ctx.now,
            ..old.clone()
        }
    } else {
        ReadingItemRecord {
            queued: false,
            priority,
            due: None,
            updated_at: ctx.now,
            ..old.clone()
        }
    };
    save::<ReadingQueueSet>(ctx, &old, new)
}

fn queue_read(
    req: ReadingSessionDone,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ReadingItemDto>> {
    let old = require_item(ctx.reads(), req.id)?;
    let previous = u32::try_from(old.interval_days).ok().filter(|d| *d > 0);
    let days = next_reading_interval(previous, priority_of(&old.priority));
    let new = ReadingItemRecord {
        position: req
            .position
            .map_or(old.position, |p| i64::from(p).min(old.length)),
        queued: true,
        due: Some(days_after(ctx.now, days)),
        interval_days: i64::from(days),
        read_count: old.read_count.saturating_add(1),
        last_read: Some(ctx.now),
        updated_at: ctx.now,
        ..old.clone()
    };
    save::<ReadingQueueRead>(ctx, &old, new)
}

fn queue_postpone(
    req: PostponeReading,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ReadingItemDto>> {
    if !(1..=365).contains(&req.days) {
        return Err(bad_range("days"));
    }
    let old = require_item(ctx.reads(), req.id)?;
    let new = ReadingItemRecord {
        queued: true,
        due: Some(days_after(ctx.now, req.days)),
        updated_at: ctx.now,
        ..old.clone()
    };
    save::<ReadingQueuePostpone>(ctx, &old, new)
}

/// UTF-16 `[start, end)` of `text` → byte range (validated, non-empty after trimming).
pub(crate) fn byte_range(text: &str, start: u32, end: u32, field: &str) -> AppResult<(usize, usize)> {
    let len = Utf16::len(text);
    if start >= end || end > len {
        return Err(bad_range(field));
    }
    let (s, e) = (Utf16::byte_of(text, start), Utf16::byte_of(text, end));
    if s >= e || text[s..e].trim().is_empty() {
        return Err(bad_range(field));
    }
    Ok((s, e))
}

fn create_extract(
    req: CreateExtract,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ReadingItemDto>> {
    let parent = require_item(ctx.reads(), req.item_id)?;
    let (s, e) = byte_range(&parent.body, req.start, req.end, "range")?;
    let body = parent.body[s..e].trim().to_owned();
    let r = ReadingItemRecord {
        id: ReadingItemId::generate(ctx.ids),
        kind: ReadingItemKind::Extract.as_str().to_owned(),
        parent_id: Some(parent.id),
        title: title_or_default(&req.title, &body),
        language: parent.language.clone(),
        source: parent.source.clone(),
        length: i64::from(Utf16::len(&body)),
        body,
        cues: None,
        cue_count: 0,
        media_path: None,
        extract_range: Some((i64::from(req.start), i64::from(req.end))),
        position: 0,
        queued: true,
        priority: ReadingPriority::High.as_str().to_owned(),
        due: Some(ctx.now),
        interval_days: 0,
        read_count: 0,
        last_read: None,
        created_at: ctx.now,
        updated_at: ctx.now,
        deleted_at: None,
    };
    ctx.tx.upsert_reading_item(&r)?;
    Ok(CommandOutcome {
        response: dto(&r, ctx.now)?,
        inverse: vec![inverse_call::<ReadingItemsDelete>(&ReadingItemRef { id: r.id })?],
        summary: summary::<ReadingExtractsCreate>().param("title", r.title.clone()),
        events: vec![DomainEvent::ReadingItemsChanged],
    })
}
