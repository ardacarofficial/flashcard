//! `media` module (v0.3, docs/03 §13): content-addressed media store. Adding the same bytes
//! twice keeps one file and one entry. Fields reference media as `media:<sha256>`; reference
//! counts follow note writes. Backbone (cards show media), so it cannot be switched off.

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::ModuleDescriptor;
use crate::registry::Registry;
use crate::support::summary;
use base64::Engine as _;
use fc_api::v1::cardtypes::{AddMedia, MediaContent, MediaDto, MediaEntry, MediaItems, MediaRef};
use fc_api::v1::common::{AffectedCount, Empty};
use fc_api::v1::methods::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::content::media::{detect_mime, media_hash, media_kind, MediaKind};
use fc_core::content::refs::{is_media_hash, MEDIA_SCHEME};
use fc_storage::repo::MediaRecord;
use fc_storage::Reads;

/// Module id.
pub const ID: &str = "media";

/// Largest accepted file (50 MiB).
pub const MAX_BYTES: usize = 50 * 1024 * 1024;

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.media.name",
        description_key: "modules.media.description",
        enabled_by_default: true,
        can_disable: false,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.command::<MediaAdd>(add);
    reg.command::<MediaForget>(forget);
    reg.command::<MediaReinstate>(reinstate);
    reg.query::<MediaGet>(get);
    reg.query::<MediaList>(list);
    reg.query::<MediaRead>(read);
}

fn not_found(hash: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::NotFoundMedia, format!("media {hash} not found"))
            .with_detail("hash", hash),
    )
}

fn require(reads: &dyn Reads, hash: &str) -> AppResult<MediaRecord> {
    if !is_media_hash(hash) {
        return Err(not_found(hash));
    }
    reads.get_media(hash)?.ok_or_else(|| not_found(hash))
}

fn to_dto(m: MediaRecord, created: bool) -> MediaDto {
    MediaDto {
        reference: format!("{MEDIA_SCHEME}{}", m.hash),
        kind: media_kind(&m.mime).unwrap_or(MediaKind::Image),
        hash: m.hash,
        mime: m.mime,
        size: m.size,
        original_name: m.original_name,
        ref_count: m.ref_count,
        created,
    }
}

fn add(req: AddMedia, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<MediaDto>> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(req.data_base64.trim())
        .map_err(|e| {
            AppError::Api(
                ApiError::new(ErrorCode::ValidationInvalidParams, format!("base64: {e}"))
                    .with_detail("field", "data_base64"),
            )
        })?;
    if bytes.len() > MAX_BYTES {
        return Err(AppError::Api(
            ApiError::new(ErrorCode::ValidationMediaTooLarge, "media file too large")
                .with_detail("max_bytes", MAX_BYTES),
        ));
    }
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| n.chars().take(255).collect::<String>());
    let mime = detect_mime(&bytes, name.as_deref()).ok_or_else(|| {
        AppError::Api(
            ApiError::new(ErrorCode::ValidationMediaType, "unsupported media type")
                .with_detail("name", name.clone().unwrap_or_default()),
        )
    })?;
    let hash = media_hash(&bytes);
    // Content-addressed: writing an existing file is a no-op (and repairs a missing one).
    ctx.tx.write_media_file(&hash, &bytes)?;
    let existing = ctx.reads().get_media(&hash)?;
    let created = existing.is_none();
    if created {
        ctx.tx.insert_media(&MediaRecord {
            hash: hash.clone(),
            mime: mime.to_owned(),
            size: bytes.len() as u64,
            original_name: name,
            ref_count: 0,
            created_at: ctx.now,
        })?;
    }
    let record = require(ctx.reads(), &hash)?;
    let inverse = if created {
        vec![inverse_call::<MediaForget>(&MediaRef { hash: hash.clone() })?]
    } else {
        Vec::new()
    };
    Ok(CommandOutcome {
        summary: summary::<MediaAdd>().param(
            "name",
            record.original_name.clone().unwrap_or_else(|| hash.clone()),
        ),
        response: to_dto(record, created),
        inverse,
        events: Vec::new(),
    })
}

fn forget(req: MediaRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AffectedCount>> {
    let m = require(ctx.reads(), &req.hash)?;
    let removed = ctx.tx.delete_unreferenced_media(&req.hash)?;
    let inverse = if removed {
        vec![inverse_call::<MediaReinstate>(&MediaEntry {
            hash: m.hash.clone(),
            mime: m.mime.clone(),
            size: m.size,
            original_name: m.original_name.clone(),
        })?]
    } else {
        Vec::new()
    };
    Ok(CommandOutcome {
        summary: summary::<MediaForget>().param("name", m.original_name.unwrap_or(m.hash)),
        response: AffectedCount::default(),
        inverse,
        events: Vec::new(),
    })
}

fn reinstate(req: MediaEntry, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AffectedCount>> {
    if !is_media_hash(&req.hash) || ctx.reads().read_media_file(&req.hash)?.is_none() {
        return Err(not_found(&req.hash));
    }
    ctx.tx.insert_media(&MediaRecord {
        hash: req.hash.clone(),
        mime: req.mime.clone(),
        size: req.size,
        original_name: req.original_name.clone(),
        ref_count: 0,
        created_at: ctx.now,
    })?;
    Ok(CommandOutcome {
        summary: summary::<MediaAdd>().param(
            "name",
            req.original_name.clone().unwrap_or_else(|| req.hash.clone()),
        ),
        inverse: vec![inverse_call::<MediaForget>(&MediaRef { hash: req.hash })?],
        response: AffectedCount::default(),
        events: Vec::new(),
    })
}

fn get(req: MediaRef, ctx: &QueryCtx<'_>) -> AppResult<MediaDto> {
    Ok(to_dto(require(ctx.tx, &req.hash)?, false))
}

fn list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<MediaItems> {
    Ok(MediaItems {
        items: ctx
            .tx
            .list_media()?
            .into_iter()
            .map(|m| to_dto(m, false))
            .collect(),
    })
}

fn read(req: MediaRef, ctx: &QueryCtx<'_>) -> AppResult<MediaContent> {
    let m = require(ctx.tx, &req.hash)?;
    let bytes = ctx
        .tx
        .read_media_file(&req.hash)?
        .ok_or_else(|| not_found(&req.hash))?;
    Ok(MediaContent {
        mime: m.mime,
        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}
