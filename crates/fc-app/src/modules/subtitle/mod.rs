//! `subtitle` module (v1.2, docs/06 §5.1): the Subtitle browser extension sends the words the
//! user picks in video subtitles ("captures") to the app, which serves the extension's
//! contract ("Subtitle Kartları API v1") on the local HTTP server under `/subtitle/v1/*`.
//!
//! **Off by default**, like `local_api`: while it is off its routes answer `503` +
//! `Retry-After` (the extension waits; a `404` would stop its media for good). The HTTP
//! server runs while either module is on; the pairing-based `/v1/*` API still needs
//! `local_api`.
//!
//! * **Access keys** (`subtitle.keys.*`): long-lived personal keys made in the app, shown
//!   once, stored as SHA-256 only, revocable. Making or revoking one is not an undo step (an
//!   undo must never revive a revoked key). Restoring a backup revokes every key.
//! * **Captures** (`subtitle.captures.create`, internal): a capture becomes a note of the
//!   built-in *Subtitle card* note type **directly in a deck** (not the inbox) — the
//!   capture's `deck_id`, else the default deck (`subtitle.default_deck`), else a top-level
//!   deck named [`FALLBACK_DECK`] made on first use. Idempotent by `capture_id`; the same
//!   language + lemma + sentence is a duplicate too ([`fc_core::subtitle::dedup_key`]); the
//!   same lemma from another line is a new note. One undo step: the note (its nested
//!   `notes.create`), the deck when one was made and the capture → note mapping.
//! * **Uploads** (`subtitle.media.upload`, internal): a JPEG screenshot or a WAV clip goes
//!   to the media store before its capture; the same capture id + kind gets the same upload
//!   id. Uploads older than a day are forgotten (their media rows too while no note uses
//!   them; files are left to the orphan-media maintenance).
//! * **Known words** ([`known_words`]): a captured word is *known* once one of its cards
//!   reached an interval of [`fc_core::subtitle::KNOWN_INTERVAL_DAYS`] days, else *learning*.
//!
//! The HTTP server calls the transport services at the end of this file with the request's
//! key; writes run as `Actor::Subtitle(<key id>)`, which holds no permission of its own and
//! reaches nothing but these internal commands.

pub(crate) mod capture;
pub(crate) mod diagnostics;
#[cfg(test)]
mod tests;

use crate::app::App;
use crate::error::{storage_to_api, AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::settings::resolve_stored;
use crate::support::decks::DeckIndex;
use crate::support::{err_id, from_json, summary};
use base64::Engine as _;
use fc_api::envelope::InvokeRequest;
use fc_api::v1::common::{AffectedCount, Empty};
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::notes::{CreateNote, NoteDto};
use fc_api::v1::oplog::Actor;
use fc_api::v1::settings::{ResetSetting, SetSetting};
use fc_api::v1::subtitle::*;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::content::builtin;
use fc_core::content::media::{detect_mime, media_hash};
use fc_core::reading::WordStatus;
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::subtitle::{dedup_key, normalize_lang, normalize_lemma, CaptureText};
use fc_core::{DeckId, NoteId, SubtitleKeyId, Timestamp};
use fc_storage::repo::{
    SubtitleCaptureRecord, SubtitleKeyRecord, SubtitleUploadRecord,
};
use fc_storage::Reads;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Module id.
pub const ID: &str = "subtitle";

/// Setting (app): id of the deck captures go to when they name none (`""` = the
/// [`FALLBACK_DECK`]).
pub const DEFAULT_DECK: &str = "subtitle.default_deck";
/// Name of the top-level deck used without a default deck (made on first use).
pub const FALLBACK_DECK: &str = "Subtitle";
/// Path prefix of the extension's routes on the local HTTP server.
pub const PATH_PREFIX: &str = "/subtitle";
/// Start of every access key.
pub const KEY_PREFIX: &str = "sk_fc_";
/// Characters of a key shown in the list (`sk_fc_` + 4).
const SHOWN_PREFIX: usize = 10;

/// Largest screenshot (bytes).
pub const MAX_SCREENSHOT_BYTES: usize = 2 * 1024 * 1024;
/// Largest audio clip (bytes).
pub const MAX_AUDIO_BYTES: usize = 5 * 1024 * 1024;
/// Most captures in one batch request.
pub const MAX_BATCH: usize = 50;
/// Uploads older than this are forgotten (ms).
pub const UPLOAD_TTL_MS: i64 = 24 * 60 * 60 * 1000;
/// Words per known-words page.
pub const KNOWN_WORDS_PAGE: u32 = 500;

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.subtitle.name",
        description_key: "modules.subtitle.description",
        enabled_by_default: false,
        can_disable: true,
        settings: vec![SettingDef {
            key: DEFAULT_DECK.to_owned(),
            kind: SettingKind::Text { max_len: Some(64) },
            default: json!(""),
            label_key: format!("settings.{DEFAULT_DECK}.label"),
            description_key: Some(format!("settings.{DEFAULT_DECK}.description")),
            // Chosen on the Subtitle screen (a deck picker), not typed as an id.
            advanced: true,
            scopes: vec![SettingScopeKind::App],
        }],
        ui: vec![main_menu(
            "subtitle.screen",
            "nav.subtitle",
            "plug",
            "/subtitle",
            83,
        )],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<SubtitleKeysList>(keys_list);
    reg.command::<SubtitleKeysCreate>(keys_create);
    reg.command::<SubtitleKeysRevoke>(keys_revoke);
    reg.query::<SubtitleStatusGet>(status);
    reg.command::<SubtitleDefaultDeckSet>(default_deck_set);
    reg.command::<SubtitleCapturesCreate>(captures_create);
    reg.command::<SubtitleCapturesForget>(captures_forget);
    reg.command::<SubtitleCapturesRestore>(captures_restore);
    reg.command::<SubtitleMediaUpload>(media_upload);
    reg.command::<SubtitleKeysTouch>(keys_touch);
    reg.query::<SubtitleCapturesRecent>(diagnostics::recent);
    reg.command::<SubtitleCapturesLog>(diagnostics::log);
}

// ------------------------------------------------------------------ keys

fn key_dto(r: &SubtitleKeyRecord) -> SubtitleKeyDto {
    SubtitleKeyDto {
        id: r.id,
        name: r.name.clone(),
        prefix: r.prefix.clone(),
        created_at: r.created_at,
        last_used_at: r.last_used_at,
        revoked_at: r.revoked_at,
    }
}

fn require_key(reads: &dyn Reads, id: SubtitleKeyId) -> AppResult<SubtitleKeyRecord> {
    reads.get_subtitle_key(id)?.ok_or_else(|| {
        err_id(
            ErrorCode::NotFoundSubtitleKey,
            format!("subtitle key {id} not found"),
            id,
        )
    })
}

/// Hex SHA-256 of an access key (what is stored).
pub fn key_hash(secret: &str) -> String {
    media_hash(secret.as_bytes())
}

fn keys_list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<SubtitleKeyList> {
    Ok(SubtitleKeyList {
        keys: ctx.tx.list_subtitle_keys()?.iter().map(key_dto).collect(),
    })
}

fn keys_create(
    req: CreateSubtitleKey,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<CreatedSubtitleKey>> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
        return Err(AppError::Api(
            ApiError::new(
                ErrorCode::ValidationSubtitleKey,
                "key name must be 1–60 printable characters",
            )
            .with_detail("field", "name"),
        ));
    }
    let mut random = [0u8; 32];
    getrandom::fill(&mut random)
        .map_err(|e| AppError::api(ErrorCode::Internal, format!("random: {e}")))?;
    let secret = format!(
        "{KEY_PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random)
    );
    let r = SubtitleKeyRecord {
        id: SubtitleKeyId::generate(ctx.ids),
        name: name.to_owned(),
        prefix: secret.chars().take(SHOWN_PREFIX).collect(),
        token_hash: key_hash(&secret),
        created_at: ctx.now,
        last_used_at: None,
        revoked_at: None,
    };
    ctx.tx.upsert_subtitle_key(&r)?;
    Ok(CommandOutcome {
        response: CreatedSubtitleKey {
            key: key_dto(&r),
            secret,
        },
        inverse: vec![],
        summary: summary::<SubtitleKeysCreate>().param("name", name.to_owned()),
        events: vec![DomainEvent::SubtitleChanged],
    })
}

fn keys_revoke(
    req: SubtitleKeyRef,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SubtitleKeyDto>> {
    let old = require_key(ctx.reads(), req.id)?;
    if old.revoked_at.is_some() {
        return Err(AppError::Api(
            ApiError::new(
                ErrorCode::ConflictSubtitleKeyState,
                format!("subtitle key {} is already revoked", old.id),
            )
            .with_detail("id", old.id.to_string()),
        ));
    }
    let r = SubtitleKeyRecord {
        revoked_at: Some(ctx.now),
        ..old
    };
    ctx.tx.upsert_subtitle_key(&r)?;
    Ok(CommandOutcome {
        response: key_dto(&r),
        inverse: vec![],
        summary: summary::<SubtitleKeysRevoke>().param("name", r.name.clone()),
        events: vec![DomainEvent::SubtitleChanged],
    })
}

fn keys_touch(req: SubtitleKeyRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let old = require_key(ctx.reads(), req.id)?;
    ctx.tx.upsert_subtitle_key(&SubtitleKeyRecord {
        last_used_at: Some(ctx.now),
        ..old
    })?;
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![],
        summary: summary::<SubtitleKeysTouch>(),
        events: vec![],
    })
}

/// Restoring a backup revokes every Subtitle key (a crafted backup must not bring a working
/// key with it): an active key is marked revoked as of its creation time.
pub(crate) fn forget_keys(dump: &mut fc_storage::repo::CollectionDump) {
    use fc_storage::repo::SqlValue;
    for t in dump.tables.iter_mut().filter(|t| t.name == "subtitle_keys") {
        let col = |n: &str| t.columns.iter().position(|c| c == n);
        let (Some(revoked), Some(created)) = (col("revoked_at"), col("created_at")) else {
            continue;
        };
        for row in &mut t.rows {
            if matches!(row[revoked], SqlValue::Null) {
                row[revoked] = match &row[created] {
                    SqlValue::Integer(n) => SqlValue::Integer(*n),
                    _ => SqlValue::Integer(1),
                };
            }
        }
    }
}

// ------------------------------------------------------------------ status, default deck

/// The default deck setting, if it names a live deck.
fn default_deck(reads: &dyn Reads) -> AppResult<Option<DeckId>> {
    let id = resolve_stored(reads, &SettingScope::App, DEFAULT_DECK)?
        .and_then(|(v, _)| v.as_str().and_then(|s| s.parse::<DeckId>().ok()));
    Ok(match id {
        Some(id) => reads
            .get_deck(id)?
            .filter(|d| d.deleted_at.is_none())
            .map(|d| d.id),
        None => None,
    })
}

/// `http://127.0.0.1:<port>/subtitle`.
pub fn address(port: u16) -> String {
    format!("http://127.0.0.1:{port}{PATH_PREFIX}")
}

fn status_of(reads: &dyn Reads) -> AppResult<SubtitleStatus> {
    let port = crate::modules::local_api::port_of(reads)?;
    let deck = default_deck(reads)?;
    let path = match deck {
        Some(id) => Some(DeckIndex::load(reads)?.path(id)),
        None => None,
    };
    let (captures, last) = reads.subtitle_capture_stats()?;
    let active = reads
        .list_subtitle_keys()?
        .iter()
        .filter(|k| k.revoked_at.is_none())
        .count();
    Ok(SubtitleStatus {
        address: address(port),
        port,
        default_deck_id: deck,
        default_deck_path: path,
        fallback_deck: FALLBACK_DECK.to_owned(),
        captures,
        last_capture_at: last,
        active_keys: u32::try_from(active).unwrap_or(u32::MAX),
    })
}

fn status(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<SubtitleStatus> {
    status_of(ctx.tx)
}

fn default_deck_set(
    req: SetSubtitleDefaultDeck,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SubtitleStatus>> {
    match req.deck_id {
        Some(id) => {
            DeckIndex::load(ctx.reads())?.require_live(id)?;
            ctx.call(&inverse_call::<SettingsSet>(&SetSetting {
                scope: SettingScope::App,
                key: DEFAULT_DECK.to_owned(),
                value: json!(id.to_string()),
            })?)?;
        }
        None => {
            ctx.call(&inverse_call::<SettingsReset>(&ResetSetting {
                scope: SettingScope::App,
                key: DEFAULT_DECK.to_owned(),
            })?)?;
        }
    }
    Ok(CommandOutcome {
        response: status_of(ctx.reads())?,
        // Undone through the nested settings entry.
        inverse: vec![],
        summary: summary::<SubtitleDefaultDeckSet>(),
        events: vec![DomainEvent::SubtitleChanged],
    })
}

// ------------------------------------------------------------------ captures

fn outcome_of(status: SubtitleCaptureStatus, r: &SubtitleCaptureRecord) -> SubtitleCaptureOutcome {
    SubtitleCaptureOutcome {
        status,
        card_id: r.card_id,
        note_id: r.note_id,
    }
}

/// The deck a capture goes to: its own (if it is a live deck), else the default deck, else
/// the [`FALLBACK_DECK`] (made now when missing).
fn target_deck(ctx: &mut CommandCtx<'_>, asked: Option<&str>) -> AppResult<DeckId> {
    if let Some(id) = asked.and_then(|s| s.parse::<DeckId>().ok()) {
        if ctx
            .reads()
            .get_deck(id)?
            .is_some_and(|d| d.deleted_at.is_none())
        {
            return Ok(id);
        }
    }
    if let Some(id) = default_deck(ctx.reads())? {
        return Ok(id);
    }
    if let Some(d) = ctx.reads().find_child_deck(None, FALLBACK_DECK)? {
        return Ok(d.id);
    }
    let v = ctx.call(&inverse_call::<DecksCreate>(&CreateDeck {
        path: FALLBACK_DECK.to_owned(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    })?)?;
    Ok(from_json::<DeckDto>(&v)?.id)
}

/// The media hash of an upload the capture refers to, if it is still there (a missing or
/// forgotten upload only costs the card its picture or sound).
fn uploaded(
    ctx: &mut CommandCtx<'_>,
    capture_id: &str,
    kind: SubtitleMediaKind,
    r: Option<&SubtitleMediaRef>,
) -> AppResult<Option<String>> {
    let Some(r) = r else {
        return Ok(None);
    };
    let Some(u) = ctx.reads().get_subtitle_upload(&r.upload_id)? else {
        return Ok(None);
    };
    if u.capture_id != capture_id || u.kind != kind_str(kind) {
        return Ok(None);
    }
    if ctx.reads().get_media(&u.media_hash)?.is_none() {
        // The row went with an old upload of the same bytes; the file is still there.
        let Some(bytes) = ctx.reads().read_media_file(&u.media_hash)? else {
            return Ok(None);
        };
        crate::modules::media::store(ctx, &bytes, &u.mime, None)?;
    }
    Ok(Some(u.media_hash))
}

/// Capture tags as note tags: invalid ones are dropped (a tag is never a reason to lose the
/// word), duplicates merged.
fn note_tags(tags: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    tags.iter()
        .filter_map(|t| fc_core::tag::normalize_tag(t).ok())
        .filter(|t| seen.insert(t.to_lowercase()))
        .collect()
}

fn notetype(ctx: &CommandCtx<'_>) -> AppResult<fc_core::NoteTypeId> {
    let conflict = || {
        AppError::Api(ApiError::new(
            ErrorCode::ConflictSubtitleNoteType,
            "the built-in Subtitle card note type is missing or in the trash",
        ))
    };
    let id = builtin::find(builtin::SUBTITLE).ok_or_else(conflict)?.id;
    match ctx.reads().get_notetype(id)? {
        Some(nt) if nt.deleted_at.is_none() => Ok(id),
        _ => Err(conflict()),
    }
}

/// Whether a mapped note is still there (not deleted, not in the trash).
fn note_is_live(reads: &dyn Reads, id: NoteId) -> AppResult<bool> {
    Ok(reads.get_note(id)?.is_some_and(|n| n.deleted_at.is_none()))
}

fn captures_create(
    req: RecordSubtitleCapture,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SubtitleCaptureOutcome>> {
    let c = &req.capture;
    let none = |r: &SubtitleCaptureRecord| CommandOutcome {
        response: outcome_of(SubtitleCaptureStatus::Duplicate, r),
        inverse: vec![],
        summary: summary::<SubtitleCapturesCreate>(),
        events: vec![],
    };
    let key = dedup_key(&c.lang.target, &c.term.lemma, c.context.sentence.as_deref());
    // A mapping whose note the user deleted is stale: the word comes back as a new card
    // (the mapping is replaced; undo puts the old one back).
    let mut stale: Vec<SubtitleCaptureRecord> = Vec::new();
    for r in [
        ctx.reads().get_subtitle_capture(&c.capture_id)?,
        ctx.reads().subtitle_capture_by_dedup_key(&key)?,
    ]
    .into_iter()
    .flatten()
    {
        if note_is_live(ctx.reads(), r.note_id)? {
            return Ok(none(&r));
        }
        if !stale.iter().any(|s| s.capture_id == r.capture_id) {
            stale.push(r);
        }
    }
    for r in &stale {
        ctx.tx.delete_subtitle_capture(&r.capture_id)?;
    }
    let notetype_id = notetype(ctx)?;
    let deck_id = target_deck(ctx, c.deck_id.as_deref())?;
    let screenshot = uploaded(
        ctx,
        &c.capture_id,
        SubtitleMediaKind::Screenshot,
        c.media.screenshot.as_ref(),
    )?;
    let audio = uploaded(ctx, &c.capture_id, SubtitleMediaKind::Audio, c.media.audio.as_ref())?;
    let fields: BTreeMap<String, String> = fc_core::subtitle::note_fields(&CaptureText {
        lemma: &c.term.lemma,
        surface: &c.term.surface,
        pos: c.term.pos.as_deref(),
        meaning: &c.definition.text,
        sentence: c.context.sentence.as_deref(),
        translation: c.context.translation.as_deref(),
        term_offset: c.context.term_offset,
        title: c.source.title.as_deref(),
        site: &c.source.site,
        url: c.source.url.as_deref(),
        t_start_ms: c.source.t_start_ms,
        screenshot: screenshot.as_deref(),
        audio: audio.as_deref(),
    });
    let v = ctx.call(&inverse_call::<NotesCreate>(&CreateNote {
        notetype_id,
        deck_id,
        fields,
        tags: note_tags(&c.tags),
        attributes: vec![],
        // The same word from another line is another card; the dedup key above decides.
        allow_duplicate: Some(true),
    })?)?;
    let note: NoteDto = from_json(&v)?;
    let card_id = note
        .cards
        .first()
        .map(|c| c.id)
        .ok_or_else(|| AppError::api(ErrorCode::Internal, "the subtitle note has no card"))?;
    let row = SubtitleCaptureRecord {
        capture_id: c.capture_id.clone(),
        dedup_key: key,
        lang: normalize_lang(&c.lang.target),
        lemma: normalize_lemma(&c.term.lemma),
        note_id: note.id,
        card_id,
        key_id: req.key_id,
        created_at: ctx.now,
    };
    ctx.tx.insert_subtitle_capture(&row)?;
    let mut inverse = vec![inverse_call::<SubtitleCapturesForget>(&SubtitleCaptureIds {
        capture_ids: vec![row.capture_id.clone()],
    })?];
    if !stale.is_empty() {
        inverse.push(inverse_call::<SubtitleCapturesRestore>(&SubtitleCaptureRows {
            rows: stale.into_iter().map(row_dto).collect(),
        })?);
    }
    Ok(CommandOutcome {
        response: outcome_of(SubtitleCaptureStatus::Created, &row),
        inverse,
        summary: summary::<SubtitleCapturesCreate>().param("word", c.term.lemma.trim().to_owned()),
        events: vec![DomainEvent::SubtitleChanged],
    })
}

fn row_dto(r: SubtitleCaptureRecord) -> SubtitleCaptureRow {
    SubtitleCaptureRow {
        capture_id: r.capture_id,
        dedup_key: r.dedup_key,
        lang: r.lang,
        lemma: r.lemma,
        note_id: r.note_id,
        card_id: r.card_id,
        key_id: r.key_id,
        created_at: r.created_at,
    }
}

fn captures_forget(
    req: SubtitleCaptureIds,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AffectedCount>> {
    let mut rows = Vec::new();
    for id in &req.capture_ids {
        if let Some(r) = ctx.reads().get_subtitle_capture(id)? {
            ctx.tx.delete_subtitle_capture(id)?;
            rows.push(row_dto(r));
        }
    }
    let inverse = if rows.is_empty() {
        vec![]
    } else {
        vec![inverse_call::<SubtitleCapturesRestore>(&SubtitleCaptureRows { rows })?]
    };
    Ok(CommandOutcome {
        response: AffectedCount::default(),
        inverse,
        summary: summary::<SubtitleCapturesForget>(),
        events: vec![DomainEvent::SubtitleChanged],
    })
}

fn captures_restore(
    req: SubtitleCaptureRows,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AffectedCount>> {
    let mut ids = Vec::new();
    for r in req.rows {
        if ctx.reads().get_subtitle_capture(&r.capture_id)?.is_some()
            || ctx.reads().subtitle_capture_by_dedup_key(&r.dedup_key)?.is_some()
        {
            continue;
        }
        ctx.tx.insert_subtitle_capture(&SubtitleCaptureRecord {
            capture_id: r.capture_id.clone(),
            dedup_key: r.dedup_key,
            lang: r.lang,
            lemma: r.lemma,
            note_id: r.note_id,
            card_id: r.card_id,
            key_id: r.key_id,
            created_at: r.created_at,
        })?;
        ids.push(r.capture_id);
    }
    let inverse = if ids.is_empty() {
        vec![]
    } else {
        vec![inverse_call::<SubtitleCapturesForget>(&SubtitleCaptureIds { capture_ids: ids })?]
    };
    Ok(CommandOutcome {
        response: AffectedCount::default(),
        inverse,
        summary: summary::<SubtitleCapturesRestore>(),
        events: vec![DomainEvent::SubtitleChanged],
    })
}

// ------------------------------------------------------------------ uploads

fn kind_str(k: SubtitleMediaKind) -> &'static str {
    match k {
        SubtitleMediaKind::Screenshot => "screenshot",
        SubtitleMediaKind::Audio => "audio",
    }
}

fn media_error(field: &str, message: impl Into<String>) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationSubtitleMedia, message).with_detail("field", field),
    )
}

/// The media type an upload of `kind` declared, normalised; only JPEG screenshots and WAV
/// clips are taken.
fn accepted_mime(kind: SubtitleMediaKind, declared: &str) -> Option<&'static str> {
    let m = declared
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match (kind, m.as_str()) {
        (SubtitleMediaKind::Screenshot, "image/jpeg" | "image/jpg") => Some("image/jpeg"),
        (SubtitleMediaKind::Audio, "audio/wav" | "audio/x-wav" | "audio/wave" | "audio/vnd.wave") => {
            Some("audio/wav")
        }
        _ => None,
    }
}

fn media_upload(
    req: UploadSubtitleMedia,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<SubtitleUploadResponse>> {
    if !capture::is_uuid_v7(&req.capture_id) {
        return Err(media_error(
            "x-capture-id",
            "X-Capture-Id must be the capture's UUIDv7",
        ));
    }
    let mime = accepted_mime(req.kind, &req.mime).ok_or_else(|| {
        media_error(
            "content-type",
            format!(
                "a {} upload must be {}",
                kind_str(req.kind),
                match req.kind {
                    SubtitleMediaKind::Screenshot => "image/jpeg",
                    SubtitleMediaKind::Audio => "audio/wav",
                }
            ),
        )
    })?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(req.data_base64.trim())
        .map_err(|e| media_error("body", format!("base64: {e}")))?;
    let max = match req.kind {
        SubtitleMediaKind::Screenshot => MAX_SCREENSHOT_BYTES,
        SubtitleMediaKind::Audio => MAX_AUDIO_BYTES,
    };
    if bytes.len() > max {
        return Err(AppError::Api(
            ApiError::new(ErrorCode::ValidationPayloadTooLarge, "media file too large")
                .with_detail("max_bytes", max),
        ));
    }
    // The content must be what the headers say.
    if detect_mime(&bytes, None) != Some(mime) {
        return Err(media_error(
            "body",
            format!("the body is not {mime} data"),
        ));
    }
    let kind = kind_str(req.kind);
    let existing = ctx.reads().subtitle_upload_for(&req.capture_id, kind)?;
    let ext = if mime == "image/jpeg" { "jpg" } else { "wav" };
    let (media, _) = crate::modules::media::store(
        ctx,
        &bytes,
        mime,
        Some(format!("subtitle-{kind}-{}.{ext}", req.capture_id)),
    )?;
    let id = existing
        .map(|u| u.id)
        .unwrap_or_else(|| format!("up_{}", ctx.ids.next_uuid().simple()));
    // Forget uploads whose capture never came (and their media, unless a note uses it).
    for old in ctx
        .tx
        .delete_subtitle_uploads_before(ctx.now.plus_millis(-UPLOAD_TTL_MS))?
    {
        if old.media_hash != media.hash {
            ctx.tx.delete_unreferenced_media(&old.media_hash)?;
        }
    }
    ctx.tx.upsert_subtitle_upload(&SubtitleUploadRecord {
        id: id.clone(),
        capture_id: req.capture_id.clone(),
        kind: kind.to_owned(),
        media_hash: media.hash,
        mime: mime.to_owned(),
        created_at: ctx.now,
    })?;
    Ok(CommandOutcome {
        response: SubtitleUploadResponse { upload_id: id },
        inverse: vec![],
        summary: summary::<SubtitleMediaUpload>(),
        events: vec![],
    })
}

// ------------------------------------------------------------------ transport services
//
// What `fc-http` calls for `/subtitle/v1/*`. The key never reaches the app: the server
// hashes it.

/// `CONFLICT_MODULE_DISABLED` unless the module is on (the server answers `503` +
/// `Retry-After`, so the extension waits instead of giving up).
pub fn ensure_enabled(app: &App) -> Result<(), ApiError> {
    if app.is_module_enabled(ID) {
        Ok(())
    } else {
        Err(
            ApiError::new(ErrorCode::ConflictModuleDisabled, "module subtitle is disabled")
                .with_detail("module", ID),
        )
    }
}

fn unauthenticated(reason: &str) -> ApiError {
    ApiError::new(
        ErrorCode::PermissionUnauthenticated,
        format!("the access key is not valid ({reason})"),
    )
    .with_detail("reason", reason)
}

fn read<T>(app: &App, f: impl FnOnce(&dyn Reads) -> AppResult<T>) -> Result<T, ApiError> {
    let tx = app.store().read().map_err(storage_to_api)?;
    f(&*tx).map_err(AppError::into_api)
}

/// The active key whose hash is `token_hash`: unknown → `PERMISSION_UNAUTHENTICATED`
/// (`invalid`), revoked → the same (`revoked`).
pub fn authenticate(app: &App, token_hash: &str) -> Result<SubtitleKeyDto, ApiError> {
    ensure_enabled(app)?;
    let rec = read(app, |r| Ok(r.subtitle_key_by_token_hash(token_hash)?))?
        .ok_or_else(|| unauthenticated("invalid"))?;
    if rec.revoked_at.is_some() {
        return Err(unauthenticated("revoked"));
    }
    Ok(key_dto(&rec))
}

/// The actor of a key.
pub fn actor(key: &SubtitleKeyDto) -> Actor {
    Actor::Subtitle(key.id.to_string())
}

fn invoke<M: ApiMethod>(app: &App, key: &SubtitleKeyDto, req: &M::Request) -> Result<M::Response, ApiError> {
    let params =
        serde_json::to_value(req).map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))?;
    let v = app.invoke_trusted(
        &InvokeRequest {
            method: format!("{}.{}", fc_api::API_VERSION, M::NAME),
            params,
        },
        &actor(key),
    )?;
    serde_json::from_value(v).map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))
}

/// Notes a key's request time (the server calls this at most once a minute per key).
pub fn touch_key(app: &App, key: &SubtitleKeyDto) -> Result<(), ApiError> {
    invoke::<SubtitleKeysTouch>(app, key, &SubtitleKeyRef { id: key.id }).map(|_| ())
}

/// `GET /subtitle/v1/me`.
pub fn me(app: &App, key: &SubtitleKeyDto) -> Result<SubtitleMe, ApiError> {
    let decks = read(app, |r| {
        Ok(r.list_decks(false)?
            .iter()
            .filter(|d| d.deleted_at.is_none())
            .count())
    })?;
    Ok(SubtitleMe {
        user: SubtitleUser {
            id: key.id.to_string(),
            name: Some(key.name.clone()),
            email: None,
        },
        decks: u32::try_from(decks).unwrap_or(u32::MAX),
        features: SubtitleFeatures {
            media: true,
            batch: true,
            known_words: true,
        },
    })
}

/// `GET /subtitle/v1/decks`: every live deck by path, with its own card count.
pub fn decks(app: &App) -> Result<SubtitleDeckList, ApiError> {
    let now = app.now();
    read(app, |r| {
        let index = DeckIndex::load(r)?;
        let counts: BTreeMap<DeckId, u32> = r
            .card_counts_by_deck(now)?
            .into_iter()
            .map(|(id, c)| (id, c.total))
            .collect();
        let mut decks: Vec<SubtitleDeck> = index
            .live()
            .map(|d| SubtitleDeck {
                id: d.id.to_string(),
                name: index.path(d.id),
                cards: counts.get(&d.id).copied().unwrap_or(0),
            })
            .collect();
        decks.sort_by_key(|a| a.name.to_lowercase());
        Ok(SubtitleDeckList { decks })
    })
}

/// `POST /subtitle/v1/captures`: validates the capture and records it.
pub fn capture(
    app: &App,
    key: &SubtitleKeyDto,
    value: &Value,
) -> Result<SubtitleCaptureOutcome, ApiError> {
    // v1.4: every attempt leaves a privacy-safe diagnostic row (`diagnostics`).
    diagnostics::observe(app, key, value, |capture| {
        invoke::<SubtitleCapturesCreate>(
            app,
            key,
            &RecordSubtitleCapture {
                key_id: Some(key.id),
                capture,
            },
        )
    })
}

/// `POST /subtitle/v1/captures:batch`: `{ "captures": [...] }`, 1–[`MAX_BATCH`]; each judged
/// on its own (an invalid one is a final error, any other failure is retryable).
pub fn batch(app: &App, key: &SubtitleKeyDto, value: &Value) -> Result<SubtitleBatchResult, ApiError> {
    let bad = |m: &str| {
        ApiError::new(ErrorCode::ValidationSubtitleCapture, m).with_detail("field", "captures")
    };
    let items = value
        .get("captures")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("captures must be an array"))?;
    if items.len() > MAX_BATCH {
        return Err(bad(&format!("at most {MAX_BATCH} captures per request")));
    }
    let results = items
        .iter()
        .map(|item| {
            let capture_id = capture::capture_id_of(item);
            match capture(app, key, item) {
                Ok(o) => SubtitleBatchItem {
                    capture_id,
                    status: match o.status {
                        SubtitleCaptureStatus::Created => SubtitleBatchStatus::Created,
                        SubtitleCaptureStatus::Duplicate => SubtitleBatchStatus::Duplicate,
                    },
                    card_id: Some(o.card_id.to_string()),
                    error: None,
                },
                Err(e) => SubtitleBatchItem {
                    capture_id,
                    status: SubtitleBatchStatus::Error,
                    card_id: None,
                    error: Some(error_body(&e)),
                },
            }
        })
        .collect();
    Ok(SubtitleBatchResult { results })
}

/// `POST /subtitle/v1/media`: `capture_id` (`X-Capture-Id`), `kind` (`X-Media-Kind`), `mime`
/// (`Content-Type`) and the raw bytes.
pub fn upload(
    app: &App,
    key: &SubtitleKeyDto,
    capture_id: &str,
    kind: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<SubtitleUploadResponse, ApiError> {
    let kind = match kind.trim().to_ascii_lowercase().as_str() {
        "screenshot" => SubtitleMediaKind::Screenshot,
        "audio" => SubtitleMediaKind::Audio,
        _ => {
            return Err(media_error(
                "x-media-kind",
                "X-Media-Kind must be screenshot or audio",
            )
            .into_api())
        }
    };
    invoke::<SubtitleMediaUpload>(
        app,
        key,
        &UploadSubtitleMedia {
            key_id: Some(key.id),
            capture_id: capture_id.trim().to_owned(),
            kind,
            mime: mime.to_owned(),
            data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        },
    )
}

/// The contract's error code of an error (`error.code`).
pub fn error_code(e: &ApiError) -> &'static str {
    match e.code {
        ErrorCode::PermissionUnauthenticated => "unauthorized",
        ErrorCode::PermissionOriginNotAllowed => "forbidden",
        ErrorCode::PermissionAppLocked => "locked",
        ErrorCode::ValidationSubtitleCapture => "invalid_capture",
        ErrorCode::ValidationSubtitleMedia => "invalid_media",
        ErrorCode::ValidationPayloadTooLarge => "too_large",
        ErrorCode::ConflictRateLimited => "rate_limited",
        ErrorCode::ConflictModuleDisabled => "module_disabled",
        ErrorCode::NotFoundMethod => "not_found",
        c if c.category() == fc_api::ErrorCategory::Validation => "invalid_request",
        _ => "server_error",
    }
}

/// The contract's error body of an error; `retryable` unless the request itself was wrong.
pub fn error_body(e: &ApiError) -> SubtitleErrorBody {
    let invalid = e.code.category() == fc_api::ErrorCategory::Validation;
    SubtitleErrorBody {
        code: error_code(e).to_owned(),
        message: e.message.clone(),
        retryable: Some(!invalid),
    }
}

/// `GET /subtitle/v1/known-words?lang=&cursor=`: the words of the word index (v1.4,
/// `crate::modules::words`) the extension can colour — **known** (a manual "I know this"
/// mark, or a card whose interval reached `words.known_interval_days`, default 21) and
/// **learning** (any other card) — whatever made their notes (Subtitle, reading mode, by hand,
/// an Anki import). New and ignored words are left out (the contract has no such statuses).
/// [`KNOWN_WORDS_PAGE`] per page by language and lemma.
pub fn known_words(
    app: &App,
    lang: Option<&str>,
    cursor: Option<&str>,
) -> Result<SubtitleKnownWords, ApiError> {
    known_words_page(app, lang, cursor, KNOWN_WORDS_PAGE)
}

/// [`known_words`] with `size` words per page.
pub(crate) fn known_words_page(
    app: &App,
    lang: Option<&str>,
    cursor: Option<&str>,
    size: u32,
) -> Result<SubtitleKnownWords, ApiError> {
    use crate::modules::words;
    let after = match cursor.filter(|c| !c.trim().is_empty()) {
        Some(c) => Some(words::parse_cursor(c).ok_or_else(|| {
            ApiError::new(ErrorCode::ValidationInvalidParams, "cursor is not valid")
                .with_detail("field", "cursor")
        })?),
        None => None,
    };
    let lang = lang
        .map(normalize_lang)
        .filter(|l| !l.is_empty())
        .map(|l| fc_core::words::index_language(&l));
    read(app, |r| {
        let threshold_days = words::threshold(r)?;
        let page = r.word_index_page(&fc_storage::repo::WordIndexQuery {
            language: lang.clone(),
            statuses: vec!["known".into(), "learning".into()],
            threshold_days,
            after,
            limit: size.saturating_add(1),
        })?;
        let more = page.len() > size as usize;
        let words: Vec<SubtitleKnownWord> = page
            .iter()
            .take(size as usize)
            .map(|w| SubtitleKnownWord {
                lemma: w.key.clone(),
                lang: w.language.clone(),
                status: if words::row_status(w, threshold_days) == WordStatus::Known {
                    SubtitleWordStatus::Known
                } else {
                    SubtitleWordStatus::Learning
                },
                interval_days: w.max_interval_days.unwrap_or(0),
                updated_at: fc_core::maintenance::iso_datetime(
                    w.last_review
                        .or(w.mark_at)
                        .or(w.first_seen)
                        .unwrap_or(Timestamp::EPOCH),
                ),
            })
            .collect();
        let next_cursor = if more {
            words.last().map(|w| words::cursor_of(&w.lang, &w.lemma))
        } else {
            None
        };
        Ok(SubtitleKnownWords { words, next_cursor })
    })
}

/// A media row for an upload (tests).
#[cfg(test)]
pub(crate) fn upload_media(
    reads: &dyn Reads,
    upload_id: &str,
) -> Option<fc_storage::repo::MediaRecord> {
    let u = reads.get_subtitle_upload(upload_id).ok()??;
    reads.get_media(&u.media_hash).ok()?
}
