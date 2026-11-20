//! Background jobs of the reading module (main lane, like imports):
//!
//! * `reading.import` (`reading.items.import`): reads a file (subtitles, PDF, HTML, plain
//!   text in any encoding) or fetches a web page (the AI module's SSRF-safe fetcher and its
//!   `ai.web.allow_private_hosts` switch), then creates the item through
//!   `reading.items.create` — so the item is validated and undoable like a pasted one.
//! * `reading.clip` (`reading.clips.extract`): runs **ffmpeg** (`reading.ffmpeg_path`) to cut
//!   a subtitle cue's audio (mp3, ±[`CLIP_PAD_MS`]) and optionally a screenshot (jpeg, middle
//!   of the cue) from the item's video, and stores them through `media.add`. The process is
//!   killed when the job is cancelled or after [`FFMPEG_TIMEOUT`].

use super::items::{media_path, require_item};
use super::{CLIP_JOB, FFMPEG_PATH, ID, IMPORT_JOB, MAX_BODY_BYTES};
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome};
use crate::jobs::{JobContext, JobHandler};
use crate::registry::Registry;
use crate::support::summary;
use base64::Engine as _;
use fc_ai::sources::{self, SourceError};
use fc_api::v1::cardtypes::{AddMedia, MediaDto};
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::{MediaAdd, ReadingClipsExtract, ReadingItemsCreate, ReadingItemsImport};
use fc_api::v1::reading::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::settings::SettingScope;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) fn register(reg: &mut Registry) {
    reg.command::<ReadingItemsImport>(import);
    reg.command::<ReadingClipsExtract>(clip);
    reg.job(Arc::new(ImportJob));
    reg.job(Arc::new(ClipJob));
}

/// Largest file `reading.import` reads.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// Silence kept around a cue in its clip.
pub const CLIP_PAD_MS: u64 = 250;
/// Longest clip.
pub const MAX_CLIP_MS: u64 = 60_000;
/// ffmpeg gets this long per run.
pub const FFMPEG_TIMEOUT: Duration = Duration::from_secs(120);

fn source_invalid(reason: &'static str, message: impl Into<String>) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationReadingSource, message).with_detail("reason", reason),
    )
}

fn clip_invalid(reason: &'static str, message: impl Into<String>) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationReadingClip, message).with_detail("reason", reason),
    )
}

fn internal(e: impl ToString) -> AppError {
    AppError::api(ErrorCode::Internal, e.to_string())
}

// ------------------------------------------------------------------ import

fn import(req: ImportReadingItem, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<JobDto>> {
    match &req.source {
        ReadingSource::File { path } => {
            if !Path::new(path.trim()).is_absolute() {
                return Err(source_invalid("path", "the file path must be absolute"));
            }
        }
        ReadingSource::Url { url } => {
            let u = url.trim();
            if !(u.starts_with("https://") || u.starts_with("http://")) || u.len() > 2000 {
                return Err(source_invalid("url", "only http(s) addresses can be fetched"));
            }
        }
    }
    super::language_or_default(ctx.reads(), &req.language)?;
    media_path(req.media_path.as_deref())?;
    let payload = serde_json::to_value(&req).map_err(internal)?;
    let job = crate::jobs::enqueue(ctx, IMPORT_JOB, payload)?;
    Ok(CommandOutcome {
        response: job,
        inverse: vec![],
        summary: summary::<ReadingItemsImport>(),
        events: vec![],
    })
}

fn source_error(e: SourceError) -> AppError {
    let retryable = matches!(e.reason, "http_status" | "unreachable" | "timeout");
    AppError::Api(
        ApiError::new(ErrorCode::ValidationReadingSource, e.to_string())
            .with_detail("reason", e.reason)
            .retryable(retryable),
    )
}

/// What a source turned into.
struct Read {
    title: Option<String>,
    body: String,
    cues: Option<Vec<CueDto>>,
    source: String,
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Reads a local file into text or cues.
pub(crate) fn read_file(path: &Path) -> AppResult<(Option<String>, String, Option<Vec<CueDto>>)> {
    let meta = std::fs::metadata(path).map_err(|e| source_invalid("io", e.to_string()))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(source_invalid("too_large", "the file is too large"));
    }
    let ext = extension(path);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_owned);
    if ext == "pdf" {
        let x = sources::pdf_file(path, None, None).map_err(source_error)?;
        return Ok((x.title.or(stem), x.text, None));
    }
    let bytes = std::fs::read(path).map_err(|e| source_invalid("io", e.to_string()))?;
    match ext.as_str() {
        "srt" | "vtt" => {
            let (_, cues) = fc_interop::subtitle::parse_subtitles(&bytes)
                .map_err(|e| source_invalid("format", e.to_string()))?;
            let cues = cues
                .into_iter()
                .map(|c| CueDto {
                    start_ms: c.start_ms,
                    end_ms: c.end_ms,
                    text: c.text,
                })
                .collect();
            Ok((stem, String::new(), Some(cues)))
        }
        "html" | "htm" | "xhtml" => {
            let x = sources::readable_text(&fc_interop::text::decode(&bytes).text);
            Ok((x.title.or(stem), x.text, None))
        }
        _ => {
            let text = fc_interop::text::normalize_newlines(&fc_interop::text::decode(&bytes).text);
            if text.contains('\0') {
                return Err(source_invalid("format", "not a text file"));
            }
            Ok((stem, text, None))
        }
    }
}

struct ImportJob;

impl JobHandler for ImportJob {
    fn kind(&self) -> &'static str {
        IMPORT_JOB
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let req: ImportReadingItem = crate::support::from_json(&payload)?;
        ctx.ensure_module_enabled(ID)?;
        ctx.cancel.check()?;
        ctx.report(None, "reading.job.reading_source");
        let read = match &req.source {
            ReadingSource::File { path } => {
                let p = Path::new(path.trim());
                let (title, body, cues) = read_file(p)?;
                Read {
                    title,
                    body,
                    cues,
                    source: p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(path)
                        .to_owned(),
                }
            }
            ReadingSource::Url { url } => {
                let allow_private_hosts = {
                    let reads = ctx.read()?;
                    crate::settings::resolve_stored(
                        &*reads,
                        &SettingScope::App,
                        crate::modules::ai::WEB_ALLOW_PRIVATE,
                    )?
                    .and_then(|(v, _)| v.as_bool())
                    .unwrap_or(false)
                };
                let x = sources::fetch_page_with(
                    url,
                    sources::FetchPolicy { allow_private_hosts },
                    &sources::SystemResolver,
                )
                .map_err(source_error)?;
                Read {
                    title: x.title,
                    body: x.text,
                    cues: None,
                    source: url.trim().to_owned(),
                }
            }
        };
        ctx.cancel.check()?;
        let mut body = read.body;
        if body.len() > MAX_BODY_BYTES {
            let mut cut = MAX_BODY_BYTES;
            while !body.is_char_boundary(cut) {
                cut -= 1;
            }
            body.truncate(cut);
        }
        let title = if req.title.trim().is_empty() {
            read.title.unwrap_or_default()
        } else {
            req.title.clone()
        };
        let item = ctx.invoke::<ReadingItemsCreate>(&CreateReadingItem {
            title,
            language: req.language,
            body,
            cues: read.cues,
            source: Some(read.source),
            media_path: req.media_path,
            queue: req.queue,
            priority: req.priority,
        })?;
        serde_json::to_value(item).map_err(internal)
    }
}

// ------------------------------------------------------------------ clips

fn clip(req: ExtractClip, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<JobDto>> {
    let item = require_item(ctx.reads(), req.item_id)?;
    clip_window(&item, req.cue_index)?;
    let payload = serde_json::to_value(req).map_err(internal)?;
    let job = crate::jobs::enqueue(ctx, CLIP_JOB, payload)?;
    Ok(CommandOutcome {
        response: job,
        inverse: vec![],
        summary: summary::<ReadingClipsExtract>(),
        events: vec![],
    })
}

/// Media file and `[start, end)` ms of the clip of cue `index` (padded, capped).
fn clip_window(
    item: &fc_storage::repo::ReadingItemRecord,
    index: u32,
) -> AppResult<(PathBuf, u64, u64)> {
    let media = item
        .media_path
        .as_deref()
        .ok_or_else(|| clip_invalid("no_media", "the item has no video or audio file"))?;
    let cues: Vec<CueDto> = item
        .cues
        .as_deref()
        .and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_default();
    let cue = cues
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .ok_or_else(|| clip_invalid("cue", "no such cue"))?;
    let start = cue.start_ms.saturating_sub(CLIP_PAD_MS);
    let end = cue
        .end_ms
        .saturating_add(CLIP_PAD_MS)
        .min(start + MAX_CLIP_MS)
        .max(start + 100);
    Ok((PathBuf::from(media), start, end))
}

/// `s.mmm` for ffmpeg.
fn secs(ms: u64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

/// ffmpeg arguments: mono mp3 of `[start, end)`.
pub fn audio_args(input: &Path, start_ms: u64, end_ms: u64, out: &Path) -> Vec<String> {
    vec![
        "-nostdin".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-ss".into(),
        secs(start_ms),
        "-i".into(),
        input.display().to_string(),
        "-t".into(),
        secs(end_ms.saturating_sub(start_ms)),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-c:a".into(),
        "libmp3lame".into(),
        "-q:a".into(),
        "5".into(),
        out.display().to_string(),
    ]
}

/// ffmpeg arguments: one 640 px wide jpeg frame at `at_ms`.
pub fn image_args(input: &Path, at_ms: u64, out: &Path) -> Vec<String> {
    vec![
        "-nostdin".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-ss".into(),
        secs(at_ms),
        "-i".into(),
        input.display().to_string(),
        "-frames:v".into(),
        "1".into(),
        "-vf".into(),
        "scale=640:-2".into(),
        "-q:v".into(),
        "4".into(),
        out.display().to_string(),
    ]
}

fn ffmpeg_error(reason: &'static str, message: impl Into<String>) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ProviderFfmpeg, message)
            .with_detail("reason", reason)
            .retryable(false),
    )
}

/// Runs `program args`, killing it on cancellation or timeout.
fn run_ffmpeg(program: &str, args: &[String], ctx: &JobContext<'_>) -> AppResult<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ffmpeg_error("missing", format!("cannot start {program}: {e}")))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let mut err = String::new();
                if let Some(mut s) = child.stderr.take() {
                    use std::io::Read as _;
                    let _ = s.read_to_string(&mut err);
                }
                let tail: String = err.chars().rev().take(500).collect::<Vec<_>>().into_iter().rev().collect();
                return Err(ffmpeg_error("failed", format!("ffmpeg failed ({status}): {tail}")));
            }
            Ok(None) => {}
            Err(e) => return Err(ffmpeg_error("failed", e.to_string())),
        }
        if ctx.is_cancelled() || started.elapsed() > FFMPEG_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            ctx.cancel.check()?;
            return Err(ffmpeg_error("timeout", "ffmpeg took too long"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// A temporary folder removed on drop.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn add_media(ctx: &JobContext<'_>, path: &Path, name: &str) -> AppResult<MediaDto> {
    let bytes = std::fs::read(path)
        .map_err(|e| ffmpeg_error("failed", format!("ffmpeg wrote no output: {e}")))?;
    if bytes.is_empty() {
        return Err(ffmpeg_error("failed", "ffmpeg wrote an empty file"));
    }
    ctx.invoke::<MediaAdd>(&AddMedia {
        name: Some(name.to_owned()),
        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

struct ClipJob;

impl JobHandler for ClipJob {
    fn kind(&self) -> &'static str {
        CLIP_JOB
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let req: ExtractClip = crate::support::from_json(&payload)?;
        ctx.ensure_module_enabled(ID)?;
        let (item, program) = {
            let reads = ctx.read()?;
            (
                require_item(&*reads, req.item_id)?,
                super::setting_text(&*reads, FFMPEG_PATH, "ffmpeg")?,
            )
        };
        let (input, start, end) = clip_window(&item, req.cue_index)?;
        if !input.is_file() {
            return Err(clip_invalid("no_media", "the video or audio file is missing"));
        }
        let program = if program.trim().is_empty() {
            "ffmpeg".to_owned()
        } else {
            program.trim().to_owned()
        };
        let dir = TempDir(std::env::temp_dir().join(format!("fc-clip-{}", ctx.job_id)));
        std::fs::create_dir_all(&dir.0).map_err(|e| ffmpeg_error("failed", e.to_string()))?;
        ctx.cancel.check()?;
        ctx.report(Some(0.1), "reading.job.cutting_audio");
        let audio_out = dir.0.join("clip.mp3");
        run_ffmpeg(&program, &audio_args(&input, start, end, &audio_out), ctx)?;
        let image_out = dir.0.join("frame.jpg");
        if req.image {
            ctx.report(Some(0.6), "reading.job.taking_screenshot");
            run_ffmpeg(&program, &image_args(&input, (start + end) / 2, &image_out), ctx)?;
        }
        ctx.cancel.check()?;
        let stem = format!("{}-{}", item.title.chars().take(40).collect::<String>(), req.cue_index);
        let audio = add_media(ctx, &audio_out, &format!("{stem}.mp3"))?;
        let image = if req.image {
            Some(add_media(ctx, &image_out, &format!("{stem}.jpg"))?)
        } else {
            None
        };
        serde_json::to_value(ReadingClip {
            audio,
            image,
            start_ms: start,
            end_ms: end,
        })
        .map_err(internal)
    }
}
