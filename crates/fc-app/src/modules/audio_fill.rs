//! `audio_fill` module (v1.4 W4, docs/03 §13, docs/06 §5.1): every card can have sound.
//!
//! * **Origin** (`media.origins`, internal `media.origin.record`): where a stored sound or
//!   picture came from — `original` (the film's, captured by the Subtitle extension), `clip`
//!   (cut from a local video), `tts` (provider, voice, speed, text hash) or `user`. Derived
//!   metadata in `media_origins` (migration 1420), written by the producers next to the file.
//! * **Fill missing audio** (`media.audio.fill_preview`, job `media.audio.fill`): for each
//!   note in the scope whose note type has an audio field and whose audio field is empty, the
//!   first usable source of: the primary encounter's own sound (original) → a cut of the
//!   user-chosen local video when the primary encounter has start–end times (shifted by
//!   [`CLIP_OFFSET`]; never invented when times are missing) → TTS of the term (or the
//!   sentence) through the registered `TtsProvider`s, cached by (provider, language, voice,
//!   speed, text hash). The whole fill is **one undo step** (a crash-safe undo group, like an
//!   import); cancelling reverts what was written.
//! * A language TTS cannot serve is reported per language (`no_tts_provider`, or `no_voice`:
//!   "no offline voice for <lang> installed") instead of failing silently.

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::jobs::{JobContext, JobHandler};
use crate::module::ModuleDescriptor;
use crate::modules::reading::jobs::{audio_args, ffmpeg_program, run_process, FFMPEG_TIMEOUT};
use crate::modules::subtitle::encounters::{row_dto, sync_note};
use crate::registry::Registry;
use crate::support::summary;
use base64::Engine as _;
use fc_api::v1::cardtypes::AddMedia;
use fc_api::v1::common::Empty;
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::*;
use fc_api::v1::notes::UpdateNote;
use fc_api::v1::subtitle::SubtitleCaptureRows;
use fc_api::v1::tts::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::content::media::{media_kind, tts_rate_key, tts_text_hash, MediaKind, MediaOrigin};
use fc_core::content::refs::{extract_media, is_media_hash, MEDIA_SCHEME};
use fc_core::note::{strip_html, Note};
use fc_core::notetype::{FieldKind, FieldRole, NoteType};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::{NoteId, NoteTypeId, Timestamp};
use fc_storage::repo::{MediaOriginRecord, SubtitleCaptureRecord, TtsCacheKey};
use fc_storage::Reads;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Module id.
pub const ID: &str = "audio_fill";
/// Job kind.
pub const JOB: &str = "media.audio.fill";
/// Setting (app): shift of re-cut clips in ms (a release whose timing differs).
pub const CLIP_OFFSET: &str = "audio_fill.clip_offset_ms";
/// Silence kept around a re-cut line.
pub const PAD_MS: u64 = 250;
/// Longest re-cut clip.
pub const MAX_CLIP_MS: u64 = 60_000;
/// Items listed in a preview.
pub const PREVIEW_ITEMS: usize = 200;
/// Longest text sent to TTS.
pub const MAX_TTS_CHARS: usize = 500;

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.audio_fill.name",
        description_key: "modules.audio_fill.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![SettingDef {
            key: CLIP_OFFSET.to_owned(),
            kind: SettingKind::Integer {
                min: Some(-30_000),
                max: Some(30_000),
            },
            default: serde_json::json!(0),
            label_key: format!("settings.{CLIP_OFFSET}.label"),
            description_key: Some(format!("settings.{CLIP_OFFSET}.description")),
            advanced: true,
            scopes: vec![SettingScopeKind::App],
        }],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<MediaOrigins>(origins);
    reg.command::<MediaOriginsRecord>(record);
    reg.query::<MediaAudioFillPreview>(preview);
    reg.command::<MediaAudioFill>(fill);
    reg.command::<MediaAudioAttach>(attach);
    reg.command::<MediaAudioFillRollback>(rollback);
    reg.job(Arc::new(FillJob));
}

fn internal(e: impl ToString) -> AppError {
    AppError::api(ErrorCode::Internal, e.to_string())
}

fn invalid(reason: &'static str, message: impl Into<String>) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationInvalidParams, message).with_detail("reason", reason),
    )
}

// ------------------------------------------------------------------ origin

/// The origin of a stored file: its row, else `original` for a Subtitle upload name
/// (`subtitle-…`), else `user`.
fn origin_dto(hash: &str, row: Option<&MediaOriginRecord>, name: Option<&str>) -> MediaOriginDto {
    let text = |s: &str| (!s.is_empty()).then(|| s.to_owned());
    match row {
        Some(r) => MediaOriginDto {
            hash: hash.to_owned(),
            origin: r.origin,
            provider: text(&r.provider),
            voice: text(&r.voice),
            language: text(&r.language),
            #[allow(clippy::cast_possible_truncation)]
            rate: (r.origin == MediaOrigin::Tts).then(|| (r.rate_milli as f64 / 1000.0) as f32),
            text_hash: text(&r.text_hash),
            source: text(&r.source),
        },
        None => MediaOriginDto {
            hash: hash.to_owned(),
            origin: if fc_core::query::lang::is_video_media(name) {
                MediaOrigin::Original
            } else {
                MediaOrigin::User
            },
            provider: None,
            voice: None,
            language: None,
            rate: None,
            text_hash: None,
            source: None,
        },
    }
}

/// Origins of the existing files among `hashes`, by hash.
pub(crate) fn origins_of(
    reads: &dyn Reads,
    hashes: &[String],
) -> AppResult<BTreeMap<String, MediaOriginDto>> {
    let wanted: Vec<String> = hashes
        .iter()
        .filter(|h| is_media_hash(h))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let rows: HashMap<String, MediaOriginRecord> = reads
        .media_origins_of(&wanted)?
        .into_iter()
        .map(|r| (r.hash.clone(), r))
        .collect();
    let mut out = BTreeMap::new();
    for h in wanted {
        if let Some(m) = reads.get_media(&h)? {
            out.insert(
                h.clone(),
                origin_dto(&h, rows.get(&h), m.original_name.as_deref()),
            );
        }
    }
    Ok(out)
}

/// Fills `screenshot_origin` / `audio_origin` of encounter rows (the encounter reads).
pub(crate) fn with_origins(
    reads: &dyn Reads,
    rows: &mut [fc_api::v1::subtitle::SubtitleCaptureRow],
) -> AppResult<()> {
    let hashes: Vec<String> = rows
        .iter()
        .flat_map(|r| [r.facts.screenshot_hash.clone(), r.facts.audio_hash.clone()])
        .flatten()
        .collect();
    if hashes.is_empty() {
        return Ok(());
    }
    let o = origins_of(reads, &hashes)?;
    let of = |h: &Option<String>| h.as_ref().and_then(|h| o.get(h)).map(|d| d.origin);
    for r in rows {
        r.facts.screenshot_origin = of(&r.facts.screenshot_hash);
        r.facts.audio_origin = of(&r.facts.audio_hash);
    }
    Ok(())
}

fn origins(req: ListMediaOrigins, ctx: &QueryCtx<'_>) -> AppResult<MediaOriginList> {
    let mut hashes = req.hashes;
    if let Some(id) = req.note_id {
        let note = crate::support::notes::require_note(ctx.tx, id)?;
        for v in note.fields.values() {
            hashes.extend(extract_media(v));
        }
    }
    Ok(MediaOriginList {
        origins: origins_of(ctx.tx, &hashes)?.into_values().collect(),
    })
}

/// Records that `hash` is the film's own picture/sound (Subtitle uploads).
pub(crate) fn record_original(ctx: &mut CommandCtx<'_>, hash: &str) -> AppResult<()> {
    ctx.tx.put_media_origin(&MediaOriginRecord {
        hash: hash.to_owned(),
        origin: MediaOrigin::Original,
        provider: String::new(),
        voice: String::new(),
        language: String::new(),
        rate_milli: 0,
        text_hash: String::new(),
        source: String::new(),
        created_at: ctx.now,
    })?;
    Ok(())
}

/// Records that `hash` was cut from the video `input` (the clip jobs).
pub(crate) fn record_clip(ctx: &JobContext<'_>, hash: &str, input: &Path) -> AppResult<()> {
    ctx.invoke::<MediaOriginsRecord>(&MediaOriginDto {
        hash: hash.to_owned(),
        origin: MediaOrigin::Clip,
        provider: None,
        voice: None,
        language: None,
        rate: None,
        text_hash: None,
        source: input
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_owned),
    })?;
    Ok(())
}

fn record(req: MediaOriginDto, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    if !is_media_hash(&req.hash) {
        return Err(invalid("hash", "not a media hash"));
    }
    let existing = ctx.reads().media_origins_of(std::slice::from_ref(&req.hash))?;
    // A known origin is never downgraded to "user".
    let keep = req.origin == MediaOrigin::User
        && existing.iter().any(|r| r.origin != MediaOrigin::User);
    if !keep {
        let rate = req.rate.map_or(0, tts_rate_key);
        ctx.tx.put_media_origin(&MediaOriginRecord {
            hash: req.hash,
            origin: req.origin,
            provider: req.provider.unwrap_or_default(),
            voice: req.voice.unwrap_or_default(),
            language: req.language.unwrap_or_default(),
            rate_milli: rate,
            text_hash: req.text_hash.unwrap_or_default(),
            source: req.source.unwrap_or_default(),
            created_at: ctx.now,
        })?;
    }
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![],
        summary: summary::<MediaOriginsRecord>(),
        events: vec![],
    })
}

// ------------------------------------------------------------------ plan

/// The audio field of a note type: its first field of kind audio.
fn audio_field(nt: &NoteType) -> Option<&str> {
    nt.fields
        .iter()
        .find(|f| f.kind == FieldKind::Audio)
        .map(|f| f.key.as_str())
}

fn field_text(note: &Note, nt: &NoteType, role: FieldRole, fallback: &str) -> Option<String> {
    let key = nt
        .field_with_role(role)
        .map(|f| f.key.clone())
        .or_else(|| nt.field(&fc_core::notetype::FieldKey::new(fallback)).map(|f| f.key.clone()))?;
    let text = strip_html(note.fields.get(&key)?);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then(|| text.chars().take(MAX_TTS_CHARS).collect())
}

/// One note's plan.
#[derive(Debug, Clone)]
struct Step {
    note_id: NoteId,
    source: AudioFillSource,
    reason: Option<String>,
    /// The primary encounter (Subtitle notes).
    capture_id: Option<String>,
    /// Original: the encounter's own sound.
    hash: Option<String>,
    /// Clip: `[start, end)` ms in the video.
    window: Option<(u64, u64)>,
    /// TTS: text, language, cache key.
    tts: Option<(String, String, TtsCacheKey)>,
}

/// The TTS side as the preview (registry) and the job (`invoke`) see it.
trait Speech {
    fn providers(&self) -> AppResult<Vec<TtsProviderDto>>;
    fn voices(&self, provider: &str) -> AppResult<Vec<VoiceDto>>;
    fn synthesize(&self, req: Synthesize) -> AppResult<AudioDto>;
}

struct ViaQuery<'a, 'b>(&'a QueryCtx<'b>);

impl Speech for ViaQuery<'_, '_> {
    fn providers(&self) -> AppResult<Vec<TtsProviderDto>> {
        Ok(crate::modules::tts::provider_list(self.0).providers)
    }
    fn voices(&self, provider: &str) -> AppResult<Vec<VoiceDto>> {
        Ok(crate::modules::tts::voice_list(self.0, Some(provider.to_owned()))?.voices)
    }
    fn synthesize(&self, _: Synthesize) -> AppResult<AudioDto> {
        Err(internal("the preview does not synthesise"))
    }
}

/// Providers and voices read up front (the job plans inside a read snapshot and must not
/// call methods while it is open).
struct Prefetched {
    providers: Vec<TtsProviderDto>,
    voices: HashMap<String, Vec<VoiceDto>>,
}

impl Prefetched {
    fn new(speech: &dyn Speech) -> AppResult<Self> {
        let providers = speech.providers()?;
        let voices = providers
            .iter()
            .filter_map(|p| speech.voices(&p.id).ok().map(|v| (p.id.clone(), v)))
            .collect();
        Ok(Self { providers, voices })
    }
}

impl Speech for Prefetched {
    fn providers(&self) -> AppResult<Vec<TtsProviderDto>> {
        Ok(self.providers.clone())
    }
    fn voices(&self, provider: &str) -> AppResult<Vec<VoiceDto>> {
        self.voices
            .get(provider)
            .cloned()
            .ok_or_else(|| internal("no voices"))
    }
    fn synthesize(&self, _: Synthesize) -> AppResult<AudioDto> {
        Err(internal("prefetched speech does not synthesise"))
    }
}

struct ViaJob<'a, 'b>(&'a JobContext<'b>);

impl Speech for ViaJob<'_, '_> {
    fn providers(&self) -> AppResult<Vec<TtsProviderDto>> {
        Ok(self.0.invoke::<TtsProviders>(&Empty {})?.providers)
    }
    fn voices(&self, provider: &str) -> AppResult<Vec<VoiceDto>> {
        Ok(self
            .0
            .invoke::<TtsVoices>(&ListVoices {
                provider: Some(provider.to_owned()),
            })?
            .voices)
    }
    fn synthesize(&self, req: Synthesize) -> AppResult<AudioDto> {
        self.0.invoke::<TtsSynthesize>(&req)
    }
}

/// Primary subtag, lower case (`en-US` → `en`).
fn primary(lang: &str) -> String {
    lang.split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Whether TTS can speak `language`: `Ok(provider id)` or the reason it cannot.
struct TtsCheck {
    provider: Option<String>,
    /// Voices of the provider (`None` = it lists none / could not list).
    voices: Option<Vec<VoiceDto>>,
}

impl TtsCheck {
    fn new(speech: &dyn Speech, reads: &dyn Reads, wanted: Option<&str>) -> AppResult<Self> {
        let wanted = match wanted {
            Some(p) => Some(p.to_owned()),
            None => crate::settings::resolve_stored(
                reads,
                &SettingScope::App,
                crate::modules::tts::PROVIDER,
            )?
            .and_then(|(v, _)| v.as_str().map(str::to_owned))
            .filter(|s| !s.is_empty()),
        };
        let list = speech.providers()?;
        let provider = match wanted {
            Some(id) => list.iter().find(|p| p.id == id).map(|p| p.id.clone()),
            None => list.first().map(|p| p.id.clone()),
        };
        let voices = match &provider {
            Some(p) => speech.voices(p).ok().filter(|v| !v.is_empty()),
            None => None,
        };
        Ok(Self { provider, voices })
    }

    fn problem(&self, language: &str, voice: Option<&str>) -> Option<&'static str> {
        if self.provider.is_none() {
            return Some("no_tts_provider");
        }
        let voices = self.voices.as_ref()?;
        let ok = match voice {
            Some(v) => voices.iter().any(|x| x.id == v),
            None => voices
                .iter()
                .any(|x| primary(&x.language) == primary(language)),
        };
        (!ok).then_some("no_voice")
    }
}

/// Live notes of the scope, in id order.
fn scope_notes(reads: &dyn Reads, scope: &AudioFillScope, now: Timestamp) -> AppResult<Vec<Note>> {
    let ids: BTreeSet<NoteId> = match (&scope.query, &scope.note_ids) {
        (None, Some(ids)) => ids.iter().copied().collect(),
        (Some(q), None) => {
            let filter = crate::modules::browse::compile(reads, q, now)?;
            let cards = reads.search_card_ids(&filter, now)?;
            let mut notes = BTreeSet::new();
            for c in cards {
                if let Some(card) = reads.get_card(c)? {
                    notes.insert(card.note_id);
                }
            }
            notes
        }
        _ => {
            return Err(invalid(
                "scope",
                "give exactly one of: scope.query, scope.note_ids",
            ))
        }
    };
    let mut out = Vec::new();
    for id in ids {
        if let Some(n) = reads.get_note(id)? {
            if n.deleted_at.is_none() {
                out.push(n);
            }
        }
    }
    Ok(out)
}

fn is_audio(reads: &dyn Reads, hash: &str) -> AppResult<bool> {
    Ok(reads
        .get_media(hash)?
        .is_some_and(|m| media_kind(&m.mime) == Some(MediaKind::Audio)))
}

fn validate(req: &FillAudio) -> AppResult<()> {
    if let Some(r) = req.rate {
        if !(0.25..=4.0).contains(&r) {
            return Err(invalid("rate", "rate must be 0.25–4"));
        }
    }
    if let Some(o) = req.offset_ms {
        if !(-30_000..=30_000).contains(&o) {
            return Err(invalid("offset", "the offset must be within ±30 s"));
        }
    }
    if let Some(v) = &req.video {
        if !Path::new(v.path.trim()).is_absolute() {
            return Err(invalid("video_path", "the video path must be absolute"));
        }
    }
    Ok(())
}

fn offset_of(reads: &dyn Reads, req: &FillAudio) -> AppResult<i64> {
    Ok(match req.offset_ms {
        Some(o) => o,
        None => crate::settings::resolve_stored(reads, &SettingScope::App, CLIP_OFFSET)?
            .and_then(|(v, _)| v.as_i64())
            .unwrap_or(0),
    })
}

/// The cut window of a line `[start, end)` shifted by `offset` (padded, capped).
pub fn clip_window(start: u64, end: u64, offset: i64) -> Option<(u64, u64)> {
    if end <= start {
        return None;
    }
    let shift = |t: u64| -> u64 {
        if offset >= 0 {
            t.saturating_add(offset.unsigned_abs())
        } else {
            t.saturating_sub(offset.unsigned_abs())
        }
    };
    let s = shift(start).saturating_sub(PAD_MS);
    let e = shift(end).saturating_add(PAD_MS).min(s + MAX_CLIP_MS);
    Some((s, e.max(s + 100)))
}

fn matches_source(r: &SubtitleCaptureRecord, source: Option<&str>) -> bool {
    let Some(want) = source.map(str::trim).filter(|s| !s.is_empty()) else {
        return true;
    };
    [&r.facts.video_id, &r.facts.title]
        .into_iter()
        .flatten()
        .any(|v| v.trim().eq_ignore_ascii_case(want))
}

/// The plan of a fill: counts and one step per note that lacks sound.
fn plan(
    reads: &dyn Reads,
    speech: &dyn Speech,
    req: &FillAudio,
    now: Timestamp,
) -> AppResult<(AudioFillPreview, Vec<Step>)> {
    validate(req)?;
    let notes = scope_notes(reads, &req.scope, now)?;
    let offset = offset_of(reads, req)?;
    let use_tts = req.tts.unwrap_or(true);
    let text_kind = req.tts_text.unwrap_or_default();
    let rate = req.rate.unwrap_or(1.0);
    let mut check: Option<TtsCheck> = None;
    let mut types: HashMap<NoteTypeId, Option<NoteType>> = HashMap::new();
    let mut p = AudioFillPreview {
        notes: u32::try_from(notes.len()).unwrap_or(u32::MAX),
        ..Default::default()
    };
    let mut problems: BTreeMap<(String, String), u32> = BTreeMap::new();
    let mut steps = Vec::new();
    for note in notes {
        if let std::collections::hash_map::Entry::Vacant(e) = types.entry(note.notetype_id) {
            e.insert(reads.get_notetype(note.notetype_id)?);
        }
        let Some(nt) = types.get(&note.notetype_id).and_then(Option::as_ref) else {
            continue;
        };
        let mut step = Step {
            note_id: note.id,
            source: AudioFillSource::None,
            reason: None,
            capture_id: None,
            hash: None,
            window: None,
            tts: None,
        };
        let Some(field) = audio_field(nt) else {
            step.reason = Some("no_audio_field".into());
            steps.push(step);
            continue;
        };
        let current = note
            .fields
            .get(&fc_core::notetype::FieldKey::new(field))
            .map(|v| v.trim().to_owned())
            .unwrap_or_default();
        if !current.is_empty() {
            p.with_audio += 1;
            continue;
        }
        let encounters = reads.subtitle_encounters_of_note(note.id)?;
        let primary_enc = encounters.iter().find(|r| r.is_primary);
        step.capture_id = primary_enc.map(|r| r.capture_id.clone());
        // 1. The film's own sound on the primary encounter.
        if let Some(h) = primary_enc.and_then(|r| r.facts.audio_hash.clone()) {
            if is_audio(reads, &h)? {
                step.source = AudioFillSource::Original;
                step.hash = Some(h);
            }
        }
        // 2. A cut of the given video at the line's times (never without times).
        if step.source == AudioFillSource::None {
            if let (Some(video), Some(r)) = (&req.video, primary_enc) {
                match (r.facts.t_start_ms, r.facts.t_end_ms) {
                    (Some(s), Some(e)) => {
                        let (s, e) = (u64::try_from(s).unwrap_or(0), u64::try_from(e).unwrap_or(0));
                        if !matches_source(r, video.source.as_deref()) {
                            p.other_video += 1;
                        } else if let Some(w) = clip_window(s, e, offset) {
                            step.source = AudioFillSource::Clip;
                            step.window = Some(w);
                        }
                    }
                    _ => p.no_times += 1,
                }
            }
        }
        // 3. TTS of the term (or the sentence).
        if step.source == AudioFillSource::None {
            let term = field_text(&note, nt, FieldRole::Term, "word")
                .or_else(|| nt.fields.first().and_then(|f| {
                    let t = strip_html(note.fields.get(&f.key)?);
                    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
                    (!t.is_empty()).then(|| t.chars().take(MAX_TTS_CHARS).collect())
                }));
            let text = match text_kind {
                AudioFillText::Term => term,
                AudioFillText::Sentence => {
                    field_text(&note, nt, FieldRole::Example, "sentence").or(term)
                }
            };
            if !use_tts {
                step.reason = Some("tts_off".into());
            } else if let Some(text) = text {
                let language = match primary_enc.map(|r| r.lang.clone()).filter(|l| !l.is_empty())
                {
                    Some(l) => l,
                    None => match reads
                        .word_index_notes_by_id(&[note.id])?
                        .into_iter()
                        .map(|r| r.language)
                        .find(|l| !l.is_empty())
                    {
                        Some(l) => l,
                        None => crate::modules::reading::language_or_default(reads, "")?,
                    },
                };
                if check.is_none() {
                    check = Some(TtsCheck::new(speech, reads, req.provider.as_deref())?);
                }
                let c = check.as_ref().ok_or_else(|| internal("tts check"))?;
                match c.problem(&language, req.voice.as_deref()) {
                    Some(reason) => {
                        step.reason = Some(reason.into());
                        *problems
                            .entry((language.clone(), reason.to_owned()))
                            .or_default() += 1;
                    }
                    None => {
                        let key = TtsCacheKey {
                            provider: c.provider.clone().unwrap_or_default(),
                            language: language.clone(),
                            voice: req.voice.clone().unwrap_or_default(),
                            rate_milli: tts_rate_key(rate),
                            text_hash: tts_text_hash(&text),
                        };
                        if reads.find_tts_media(&key)?.is_some() {
                            p.tts_cached += 1;
                            step.reason = Some("cached".into());
                        }
                        step.source = AudioFillSource::Tts;
                        step.tts = Some((text, language, key));
                    }
                }
            } else {
                step.reason = Some("no_text".into());
            }
        }
        steps.push(step);
    }
    for s in &steps {
        match s.source {
            AudioFillSource::Original => p.original += 1,
            AudioFillSource::Clip => p.clip += 1,
            AudioFillSource::Tts => p.tts += 1,
            AudioFillSource::None => p.none += 1,
        }
    }
    let provider = check.and_then(|c| c.provider);
    p.tts_problems = problems
        .into_iter()
        .map(|((language, reason), notes)| AudioFillTtsProblem {
            language,
            reason,
            provider: provider.clone(),
            notes,
        })
        .collect();
    p.items = steps.iter().take(PREVIEW_ITEMS).map(item).collect();
    Ok((p, steps))
}

fn item(s: &Step) -> AudioFillItem {
    AudioFillItem {
        note_id: s.note_id,
        source: s.source,
        reason: s.reason.clone(),
        text: s.tts.as_ref().map(|t| t.0.clone()),
        language: s.tts.as_ref().map(|t| t.1.clone()),
    }
}

fn preview(req: FillAudio, ctx: &QueryCtx<'_>) -> AppResult<AudioFillPreview> {
    Ok(plan(ctx.tx, &ViaQuery(ctx), &req, ctx.now)?.0)
}

// ------------------------------------------------------------------ commands

fn fill(req: FillAudio, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<JobDto>> {
    validate(&req)?;
    // Fail early on a bad scope (the query compiles, the selector is one of the two).
    scope_notes(ctx.reads(), &req.scope, ctx.now)?;
    let payload = serde_json::to_value(&req).map_err(internal)?;
    let job = crate::jobs::enqueue(ctx, JOB, payload)?;
    Ok(CommandOutcome {
        response: job,
        inverse: vec![],
        summary: summary::<MediaAudioFill>(),
        events: vec![],
    })
}

/// `media.audio.attach`: the note's (empty) audio field gets `media:<hash>`; with an
/// encounter, its `audio_hash` and shown `audio` too. Undo = the nested calls' inverses.
fn attach(req: AttachAudio, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let note = crate::support::notes::require_live_note(ctx.reads(), req.note_id)?;
    if !is_audio(ctx.reads(), &req.hash)? {
        return Err(invalid("hash", "not a stored audio file"));
    }
    let nt = ctx
        .reads()
        .get_notetype(note.notetype_id)?
        .ok_or_else(|| internal("note type missing"))?;
    let field = audio_field(&nt)
        .ok_or_else(|| invalid("no_audio_field", "the note type has no audio field"))?
        .to_owned();
    let value = format!("{MEDIA_SCHEME}{}", req.hash);
    if let Some(cid) = &req.capture_id {
        let mut row = ctx
            .reads()
            .get_subtitle_capture(cid)?
            .filter(|r| r.note_id == note.id)
            .ok_or_else(|| invalid("capture_id", "the encounter is not one of the note's"))?;
        row.facts.audio_hash = Some(req.hash.clone());
        row.facts
            .example_fields
            .insert("audio".to_owned(), value.clone());
        ctx.call(&inverse_call::<SubtitleEncountersPut>(&SubtitleCaptureRows {
            rows: vec![row_dto(row)],
        })?)?;
        sync_note(ctx, note.id, false)?;
    }
    let note = crate::support::notes::require_live_note(ctx.reads(), req.note_id)?;
    let mut fields: BTreeMap<String, String> = note
        .fields
        .iter()
        .map(|(k, v)| (k.as_str().to_owned(), v.clone()))
        .collect();
    if fields.get(&field).is_none_or(|v| v.trim().is_empty()) {
        fields.insert(field, value);
        ctx.call(&inverse_call::<NotesUpdate>(&UpdateNote {
            id: note.id,
            deck_id: None,
            fields,
            tags: note.tags.iter().map(|t| t.as_str().to_owned()).collect(),
            attributes: vec![],
        })?)?;
    }
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![],
        summary: summary::<MediaAudioAttach>(),
        events: vec![],
    })
}

fn rollback(req: RollbackAudioFill, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    for call in &req.calls {
        ctx.call(call)?;
    }
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![],
        summary: summary::<MediaAudioFillRollback>(),
        events: vec![],
    })
}

// ------------------------------------------------------------------ job

fn extension_of(mime: &str) -> &'static str {
    match mime {
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/webm" => "webm",
        "audio/mp4" | "audio/aac" => "m4a",
        _ => "bin",
    }
}

fn file_stem(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .take(40)
        .collect::<String>()
        .trim()
        .replace(' ', "_")
}

struct FillJob;

/// What one step produced.
enum Made {
    Hash(String),
    Failed(String),
}

impl FillJob {
    fn make(
        &self,
        ctx: &JobContext<'_>,
        group: &mut crate::app::UndoGroup,
        req: &FillAudio,
        step: &Step,
        ffmpeg: &Option<String>,
        cache: &mut HashMap<String, String>,
    ) -> AppResult<Made> {
        match step.source {
            AudioFillSource::Original => Ok(step
                .hash
                .clone()
                .map_or_else(|| Made::Failed("no_original".into()), Made::Hash)),
            AudioFillSource::Clip => {
                let (Some(video), Some((start, end))) = (&req.video, step.window) else {
                    return Ok(Made::Failed("no_window".into()));
                };
                let program = match ffmpeg {
                    Some(p) => p.clone(),
                    None => return Ok(Made::Failed("ffmpeg_path".into())),
                };
                let input = PathBuf::from(video.path.trim());
                if !input.is_file() {
                    return Ok(Made::Failed("no_video_file".into()));
                }
                let dir = tempfile::Builder::new()
                    .prefix("fc-fill-")
                    .tempdir()
                    .map_err(internal)?;
                let out = dir.path().join("clip.mp3");
                let r = run_process(
                    &program,
                    &audio_args(&input, start, end, &out),
                    FFMPEG_TIMEOUT,
                    &|| ctx.is_cancelled(),
                );
                ctx.cancel.check()?;
                if let Err(e) = r {
                    return Ok(Made::Failed(format!("ffmpeg: {e}")));
                }
                let bytes = std::fs::read(&out).unwrap_or_default();
                if bytes.is_empty() {
                    return Ok(Made::Failed("ffmpeg: empty output".into()));
                }
                let name = input
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(file_stem)
                    .unwrap_or_default();
                let m = ctx.invoke_in_group::<MediaAdd>(
                    group,
                    &AddMedia {
                        name: Some(format!("clip-{name}-{start}.mp3")),
                        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
                    },
                )?;
                record_clip(ctx, &m.hash, &input)?;
                Ok(Made::Hash(m.hash))
            }
            AudioFillSource::Tts => {
                let Some((text, language, key)) = &step.tts else {
                    return Ok(Made::Failed("no_text".into()));
                };
                let cache_id = format!(
                    "{}|{}|{}|{}|{}",
                    key.provider, key.language, key.voice, key.rate_milli, key.text_hash
                );
                if let Some(h) = cache.get(&cache_id) {
                    return Ok(Made::Hash(h.clone()));
                }
                let cached = ctx.read()?.find_tts_media(key)?;
                if let Some(h) = cached {
                    cache.insert(cache_id, h.clone());
                    return Ok(Made::Hash(h));
                }
                let audio = match ViaJob(ctx).synthesize(Synthesize {
                    provider: Some(key.provider.clone()),
                    text: text.clone(),
                    language: language.clone(),
                    voice: req.voice.clone(),
                    rate: req.rate,
                }) {
                    Ok(a) => a,
                    Err(e) => return Ok(Made::Failed(format!("tts: {e}"))),
                };
                let m = ctx.invoke_in_group::<MediaAdd>(
                    group,
                    &AddMedia {
                        name: Some(format!(
                            "tts-{}-{}.{}",
                            primary(language),
                            file_stem(text),
                            extension_of(&audio.mime)
                        )),
                        data_base64: audio.data_b64,
                    },
                )?;
                ctx.invoke::<MediaOriginsRecord>(&MediaOriginDto {
                    hash: m.hash.clone(),
                    origin: MediaOrigin::Tts,
                    provider: Some(key.provider.clone()),
                    voice: req.voice.clone(),
                    language: Some(language.clone()),
                    rate: Some(req.rate.unwrap_or(1.0)),
                    text_hash: Some(key.text_hash.clone()),
                    source: None,
                })?;
                cache.insert(cache_id, m.hash.clone());
                Ok(Made::Hash(m.hash))
            }
            AudioFillSource::None => Ok(Made::Failed("none".into())),
        }
    }
}

impl JobHandler for FillJob {
    fn kind(&self) -> &'static str {
        JOB
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let req: FillAudio = crate::support::from_json(&payload)?;
        ctx.ensure_module_enabled(ID)?;
        ctx.report(None, "audio_fill.job.planning");
        let speech = Prefetched::new(&ViaJob(ctx))?;
        let (steps, ffmpeg) = {
            let reads = ctx.read()?;
            let (_, steps) = plan(&*reads, &speech, &req, ctx.now())?;
            let ffmpeg = if req.video.is_some() {
                let stored = crate::settings::resolve_stored(
                    &*reads,
                    &SettingScope::App,
                    crate::modules::reading::FFMPEG_PATH,
                )?
                .map(|(v, _)| v.as_str().map(str::to_owned).unwrap_or_default());
                ffmpeg_program(stored.as_deref()).ok()
            } else {
                None
            };
            (steps, ffmpeg)
        };
        let todo: Vec<&Step> = steps
            .iter()
            .filter(|s| s.source != AudioFillSource::None)
            .collect();
        let mut group = ctx.begin_group::<MediaAudioFillRollback>(&RollbackAudioFill {
            calls: Vec::new(),
        })?;
        let mut done = AudioFillPreview {
            notes: u32::try_from(steps.len()).unwrap_or(u32::MAX),
            ..Default::default()
        };
        let mut failed = 0u32;
        let mut cache = HashMap::new();
        let total = todo.len().max(1);
        let outcome: AppResult<()> = (|| {
            for (i, step) in todo.iter().enumerate() {
                ctx.cancel.check()?;
                #[allow(clippy::cast_precision_loss)]
                ctx.report(Some(i as f32 / total as f32), "audio_fill.job.filling");
                match self.make(ctx, &mut group, &req, step, &ffmpeg, &mut cache)? {
                    Made::Hash(hash) => {
                        ctx.invoke_in_group::<MediaAudioAttach>(
                            &mut group,
                            &AttachAudio {
                                note_id: step.note_id,
                                hash,
                                capture_id: step.capture_id.clone(),
                            },
                        )?;
                        match step.source {
                            AudioFillSource::Original => done.original += 1,
                            AudioFillSource::Clip => done.clip += 1,
                            AudioFillSource::Tts => {
                                done.tts += 1;
                                if step.reason.as_deref() == Some("cached") {
                                    done.tts_cached += 1;
                                }
                            }
                            AudioFillSource::None => {}
                        }
                    }
                    Made::Failed(reason) => {
                        failed += 1;
                        if done.items.len() < PREVIEW_ITEMS {
                            done.items.push(AudioFillItem {
                                reason: Some(reason),
                                ..item(step)
                            });
                        }
                    }
                }
                ctx.yield_writer();
            }
            Ok(())
        })();
        match outcome {
            Ok(()) => {
                let count = done.original + done.clip + done.tts;
                let s = summary::<MediaAudioFill>().param("count", count);
                if let Err(e) = ctx.commit_group(&group, Some(s)) {
                    let _ = discard(ctx, &group);
                    return Err(e);
                }
                done.none = u32::try_from(steps.len() - todo.len()).unwrap_or(u32::MAX);
                serde_json::to_value(AudioFillReport { done, failed }).map_err(internal)
            }
            Err(e) => {
                if let Err(d) = discard(ctx, &group) {
                    let _ = ctx.commit_group(&group, None);
                    return Err(d);
                }
                Err(e)
            }
        }
    }
}

/// Reverts what an unfinished fill wrote (no undo step) and ends the group.
fn discard(ctx: &JobContext<'_>, group: &crate::app::UndoGroup) -> AppResult<()> {
    ctx.discard_group::<MediaAudioFillRollback>(
        group,
        &RollbackAudioFill {
            calls: group.inverses_rev(),
        },
    )
}

#[cfg(test)]
#[path = "audio_fill_tests.rs"]
mod tests;
