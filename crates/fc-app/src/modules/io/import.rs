//! `io.import.start` and the `io.import` job: read the source, parse it into a [`Staged`]
//! value (cancellable between steps), then write it chunk by chunk through `io.import.apply`
//! (see [`super::apply`]): one transaction per chunk, one undo step for the whole import.

use super::rows::{self, RowsImport};
use super::{format_of, interop_err, read_file, decode_text, Staging, JOB_IMPORT};
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome};
use super::apply::{ApplyState, CHUNK_ROWS};
use crate::app::UndoGroup;
use crate::jobs::{enqueue, JobContext, JobHandler};
use crate::support::decks::DeckIndex;
use crate::support::notes::require_notetype;
use crate::support::{deleted, invalid, summary};
use fc_api::v1::io::*;
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::{IoImportApply, IoImportDiscard, IoImportStart};
use fc_api::ErrorCode;
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::{DeckId, Timestamp};
use fc_interop::pack::Pack;
use fc_interop::{anki, fcpack};
use serde_json::Value;
use std::sync::Arc;

/// A structured package to apply.
pub(crate) struct PackImport {
    /// Content.
    pub pack: Pack,
    /// Parent of the package's deck tree (`None` = top level).
    pub parent: Option<DeckId>,
    /// Existing notes.
    pub duplicates: DuplicatePolicy,
    /// Keep scheduling and review history.
    pub include_history: bool,
    /// Conversion warnings.
    pub issues: Vec<ImportIssue>,
}

/// What a job stages for `io.import.apply`.
pub(crate) enum Staged {
    /// Text/Quizlet/JSON rows.
    Rows(RowsImport),
    /// Anki/`.fcpack` package.
    Pack(Box<PackImport>),
}

/// Validates the request against the collection and enqueues the job.
pub(super) fn start(req: StartImport, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<JobDto>> {
    match &req.source {
        ImportSource::File { path } if path.trim().is_empty() => {
            return Err(invalid("source.path is empty"));
        }
        _ => {}
    }
    if let Some(o) = &req.text_options {
        super::text_options(o).validate().map_err(interop_err)?;
    }
    if let Some(d) = req.deck_id {
        DeckIndex::load(ctx.reads())?.require_live(d)?;
    }
    let nt = require_notetype(ctx.reads(), req.notetype_id.unwrap_or(CLASSIC_ID))?;
    if nt.deleted_at.is_some() {
        return Err(deleted("notetype", nt.id));
    }
    for (i, c) in req.columns.iter().flatten().enumerate() {
        let bad = |reason: &str| {
            AppError::Api(
                fc_api::ApiError::new(ErrorCode::ValidationColumnMapping, reason.to_owned())
                    .with_detail("column", i)
                    .with_detail("reason", reason),
            )
        };
        match c {
            ColumnTarget::Field { key } => {
                if nt.field(&fc_core::notetype::FieldKey::new(key.clone())).is_none() {
                    return Err(bad("unknown field"));
                }
            }
            ColumnTarget::Attribute { attr_id } => {
                let def = ctx.reads().get_attribute_def(*attr_id)?;
                if def.is_none_or(|d| d.deleted_at.is_some()) {
                    return Err(bad("unknown classification"));
                }
            }
            ColumnTarget::Ignore | ColumnTarget::Tags => {}
        }
    }
    let payload = serde_json::to_value(&req)
        .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
    let job = enqueue(ctx, JOB_IMPORT, payload)?;
    Ok(CommandOutcome {
        response: job,
        inverse: Vec::new(),
        summary: summary::<IoImportStart>(),
        events: Vec::new(),
    })
}

/// The `io.import` job.
pub(crate) struct ImportJob(pub Arc<Staging>);

impl ImportJob {
    fn stage(&self, req: &StartImport, ctx: &JobContext<'_>) -> AppResult<Staged> {
        let (bytes, path) = match &req.source {
            ImportSource::File { path } => (read_file(path)?, Some(path.as_str())),
            ImportSource::Text { text } => (text.clone().into_bytes(), None),
        };
        ctx.report(Some(0.2), "io.progress.parsing");
        ctx.cancel.check()?;
        let format = format_of(req.format, path);
        let notetype_id = req.notetype_id.unwrap_or(CLASSIC_ID);
        Ok(match format {
            ImportFormat::Text | ImportFormat::Quizlet => {
                let keys: Vec<String> = {
                    let tx = ctx.read()?;
                    require_notetype(&*tx, notetype_id)?
                        .fields
                        .iter()
                        .map(|f| f.key.0.clone())
                        .collect()
                };
                let text = decode_text(&bytes);
                Staged::Rows(rows::from_text(req, &text, format, notetype_id, &keys)?)
            }
            ImportFormat::Json => {
                Staged::Rows(rows::from_json(req, &decode_text(&bytes), notetype_id)?)
            }
            ImportFormat::Anki => {
                let col = anki::read_package(&bytes).map_err(interop_err)?;
                ctx.report(Some(0.5), "io.progress.converting");
                ctx.cancel.check()?;
                let (pack, report) = anki::to_pack(&col, ctx.now());
                let mut issues = Vec::new();
                let warn = |key: &str, n: usize| ImportIssue {
                    row: 0,
                    message: fc_api::v1::common::I18nMessage::key(key.to_owned())
                        .param("count", n),
                };
                for (key, n) in [
                    ("io.issue.anki_skipped_notes", report.skipped_notes),
                    ("io.issue.anki_skipped_cards", report.skipped_cards),
                    ("io.issue.anki_skipped_reviews", report.skipped_reviews),
                    ("io.issue.anki_skipped_tags", report.skipped_tags),
                    ("io.issue.anki_missing_media", report.missing_media.len()),
                ] {
                    if n > 0 {
                        issues.push(warn(key, n));
                    }
                }
                self.pack(req, pack, issues)
            }
            ImportFormat::Fcpack => {
                let (_, pack) = fcpack::read(&bytes).map_err(interop_err)?;
                self.pack(req, pack, Vec::new())
            }
        })
    }

    fn pack(&self, req: &StartImport, pack: Pack, issues: Vec<ImportIssue>) -> Staged {
        Staged::Pack(Box::new(PackImport {
            pack,
            parent: req.deck_id,
            duplicates: req.duplicates.unwrap_or_default(),
            include_history: req.include_history.unwrap_or(true),
            issues,
        }))
    }
}

impl JobHandler for ImportJob {
    fn kind(&self) -> &'static str {
        JOB_IMPORT
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let req: StartImport = serde_json::from_value(payload)
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
        ctx.report(Some(0.0), "io.progress.reading");
        let staged = self.stage(&req, ctx)?;
        ctx.cancel.check()?;
        ctx.report(Some(0.0), "io.progress.writing");
        let job = ctx.job_id;
        let since = ctx.now();
        let knobs = knobs::get(job);
        let chunk = knobs.as_ref().map_or(CHUNK_ROWS, |k| k.chunk_rows);
        self.0
            .put_import(job, ApplyState::new(staged, chunk), ctx.cancel.clone());
        // Crash-safe: if the app stops between chunks, startup discards the committed ones.
        let mut group = ctx.begin_group::<IoImportDiscard>(&DiscardImport {
            calls: Vec::new(),
            since,
        })?;
        let mut chunks = 0usize;
        let outcome = loop {
            if let Err(e) = ctx.cancel.check() {
                break Err(e);
            }
            match ctx.invoke_in_group::<IoImportApply>(&mut group, &StagedJob { job_id: job }) {
                Err(e) => break Err(e),
                Ok(report) => {
                    chunks += 1;
                    if let Some(k) = &knobs {
                        (k.after_chunk)(chunks, &ctx.cancel);
                    }
                    match self.0.import_progress(job) {
                        None => break Ok(report),
                        Some(p) => ctx.report(Some(p), "io.progress.writing"),
                    }
                    ctx.yield_writer();
                }
            }
        };
        // The apply takes the staged data; drop what is left after a failure or cancel.
        let _ = self.0.take_import(job);
        match outcome {
            Ok(report) => {
                let summary = summary::<IoImportApply>().param("count", report.added_notes);
                if let Err(e) = ctx.commit_group(&group, Some(summary)) {
                    // Undo history could not be written: remove the import rather than leave
                    // rows nothing can undo.
                    let _ = discard(ctx, &group, since);
                    return Err(e);
                }
                serde_json::to_value(report)
                    .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
            }
            Err(e) => {
                if let Err(d) = discard(ctx, &group, since) {
                    // Could not remove the committed chunks: keep them undoable instead.
                    let _ = ctx.commit_group(&group, None);
                    return Err(d);
                }
                Err(e)
            }
        }
    }
}

/// Reverts the committed chunks of an unfinished import (no undo step) and ends the group.
fn discard(ctx: &JobContext<'_>, group: &UndoGroup, since: Timestamp) -> AppResult<()> {
    ctx.discard_group::<IoImportDiscard>(
        group,
        &DiscardImport {
            calls: group.inverses_rev(),
            since,
        },
    )
}

/// Test hooks of the import job, keyed by job id (chunk size, a callback after each chunk).
pub(crate) mod knobs {
    use crate::jobs::CancelToken;
    use fc_core::JobId;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Callback after each committed chunk.
    pub(crate) type AfterChunk = Box<dyn Fn(usize, &CancelToken) + Send + Sync>;

    /// Overrides for one job.
    pub(crate) struct Knobs {
        /// Units per chunk.
        pub chunk_rows: usize,
        /// Called after each committed chunk (1-based count) outside any transaction.
        pub after_chunk: AfterChunk,
    }

    static KNOBS: Mutex<Option<HashMap<JobId, Arc<Knobs>>>> = Mutex::new(None);

    /// Sets the knobs of `job` (tests).
    #[cfg(test)]
    pub(crate) fn set(job: JobId, knobs: Knobs) {
        if let Ok(mut m) = KNOBS.lock() {
            m.get_or_insert_with(HashMap::new).insert(job, Arc::new(knobs));
        }
    }

    pub(super) fn get(job: JobId) -> Option<Arc<Knobs>> {
        KNOBS
            .lock()
            .ok()
            .and_then(|m| m.as_ref().and_then(|m| m.get(&job).cloned()))
    }
}
