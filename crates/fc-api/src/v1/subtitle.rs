//! Subtitle extension integration (v1.2, docs/06 §5.1): the app side of the "Subtitle
//! Kartları API v1" contract (`/home/user/subtitle/docs/flashcards-api.md` in the extension's
//! repository), served by the local HTTP server under `/subtitle/v1/*`.
//!
//! Three groups of types:
//!
//! * **App methods** for the settings screen: personal access keys (created here, shown once,
//!   stored as SHA-256 only, revocable), the default deck, the status.
//! * **Internal methods** the HTTP server calls as `Actor::Subtitle(<key id>)`: record a
//!   capture (a note directly in a deck, one undo step), store an uploaded picture or sound.
//! * **Wire types** of the contract (the capture record, `/v1/me`, `/v1/decks`, results,
//!   known words, the error body), exported to TypeScript by [`export_http_types`].

use fc_core::{CardId, DeckId, NoteId, SubtitleKeyId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ------------------------------------------------------------------ app methods

/// A personal access key of the Subtitle extension. The key itself is shown once, when it is
/// made ([`CreatedSubtitleKey`]); the app keeps only its SHA-256 hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleKeyDto {
    /// Id (also the oplog actor id: `{"kind":"subtitle","id":…}`).
    pub id: SubtitleKeyId,
    /// Name the user gave it (e.g. the browser it is pasted into).
    pub name: String,
    /// The key's first characters (`sk_fc_ab12…`), to tell keys apart.
    pub prefix: String,
    /// Made.
    pub created_at: Timestamp,
    /// Last request with it (updated at most once a minute).
    pub last_used_at: Option<Timestamp>,
    /// Revoked (it no longer works); `null` = active.
    pub revoked_at: Option<Timestamp>,
}

/// Response of `subtitle.keys.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleKeyList {
    /// Keys, newest first (revoked ones too).
    pub keys: Vec<SubtitleKeyDto>,
}

/// `subtitle.keys.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreateSubtitleKey {
    /// Name (1–60 printable characters).
    pub name: String,
}

/// Response of `subtitle.keys.create`: the key **once**; it cannot be shown again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CreatedSubtitleKey {
    /// The stored record.
    pub key: SubtitleKeyDto,
    /// The access key to paste into the extension (`sk_fc_…`).
    pub secret: String,
}

/// A key reference (`subtitle.keys.revoke`, internal `subtitle.keys.touch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SubtitleKeyRef {
    /// Key.
    pub id: SubtitleKeyId,
}

/// Response of `subtitle.status` (and `subtitle.default_deck.set`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleStatus {
    /// What to enter in the extension as the server address
    /// (`http://127.0.0.1:<port>/subtitle`).
    pub address: String,
    /// Port of the local server (setting `local_api.port`).
    pub port: u16,
    /// Deck captures go to when they name none (setting `subtitle.default_deck`); `null` =
    /// the deck named [`SubtitleStatus::fallback_deck`], made on first use.
    pub default_deck_id: Option<DeckId>,
    /// Full path of the default deck (`null` when unset or gone).
    pub default_deck_path: Option<String>,
    /// Name of the deck used without a default (`Subtitle`).
    pub fallback_deck: String,
    /// Captures received so far (ones undone since are not counted).
    pub captures: u64,
    /// When the last one arrived.
    pub last_capture_at: Option<Timestamp>,
    /// Keys that work.
    pub active_keys: u32,
    /// Whether `GET /subtitle/v1/known-words` shares the word index with the extension
    /// (setting `subtitle.share_known_words`, off by default: Subtitle is a capture tool).
    pub share_known_words: bool,
}

/// `subtitle.default_deck.set`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct SetSubtitleDefaultDeck {
    /// Deck (`null` = back to the `Subtitle` deck).
    #[serde(default)]
    pub deck_id: Option<DeckId>,
}

/// `subtitle.captures.recent` (v1.4): the newest capture attempts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct RecentSubtitleCaptures {
    /// How many (default 50, at most the number kept, 500).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// What happened to one capture attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleAttemptResult {
    /// A new card.
    Created,
    /// Matched a card already there (same capture id, or same language + lemma + sentence).
    Duplicate,
    /// Not recorded: `reason` is the contract's error code, `:` and the field when one is
    /// known (`invalid_capture:term.lemma`).
    Rejected,
    /// A new card, but a picture or sound it referred to was not there: `reason` lists
    /// `screenshot:<why>` / `audio:<why>` (`upload_unknown`, `upload_mismatch`,
    /// `file_missing`).
    MediaMissing,
}

/// One capture attempt (v1.4 diagnostics). Only ids, codes and times: never the sentence,
/// the word, a URL or a video title.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SubtitleCaptureAttempt {
    /// The capture id as sent (empty when the request had none).
    pub capture_id: String,
    /// When it arrived.
    pub at: Timestamp,
    /// Result.
    pub result: SubtitleAttemptResult,
    /// Machine reason (see [`SubtitleAttemptResult`]).
    pub reason: Option<String>,
    /// The card made or matched.
    pub card_id: Option<CardId>,
    /// Time the app spent on it, ms.
    pub duration_ms: u32,
}

/// Response of `subtitle.captures.recent`, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleCaptureAttempts {
    /// Attempts.
    pub attempts: Vec<SubtitleCaptureAttempt>,
    /// How many the app keeps (older ones are forgotten).
    pub kept: u32,
}

// ------------------------------------------------------------------ internal methods

/// Internal `subtitle.captures.log` (v1.4): notes one capture attempt (diagnostics; never in
/// the oplog).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct LogSubtitleCaptureAttempt {
    /// The attempt (`at` is the time it arrived).
    pub attempt: SubtitleCaptureAttempt,
}

/// Internal `subtitle.captures.create`: a validated capture from the HTTP server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RecordSubtitleCapture {
    /// Key it came with.
    pub key_id: Option<SubtitleKeyId>,
    /// The capture.
    pub capture: SubtitleCapture,
}

/// Whether a capture made a card now or matched one already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleCaptureStatus {
    /// A new note (HTTP 201).
    Created,
    /// Same `capture_id`, or same language + lemma + sentence, seen before (HTTP 200).
    Duplicate,
}

/// Response of `subtitle.captures.create`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleCaptureOutcome {
    /// Created or duplicate.
    pub status: SubtitleCaptureStatus,
    /// The note's card.
    pub card_id: CardId,
    /// The note.
    pub note_id: NoteId,
}

/// One capture → note mapping (`subtitle_captures` row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleCaptureRow {
    /// The extension's id (UUIDv7).
    pub capture_id: String,
    /// `<lang>|<lemma>|<sentence hash>`.
    pub dedup_key: String,
    /// `lang.target`, lowercase.
    pub lang: String,
    /// Lemma, lowercase.
    pub lemma: String,
    /// Note made.
    pub note_id: NoteId,
    /// Its card.
    pub card_id: CardId,
    /// Key it came with.
    pub key_id: Option<SubtitleKeyId>,
    /// Received.
    pub created_at: Timestamp,
}

/// Internal `subtitle.captures.restore` (inverse of forget): rows to put back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SubtitleCaptureRows {
    /// Rows.
    pub rows: Vec<SubtitleCaptureRow>,
}

/// Internal `subtitle.captures.forget` (inverse of create/restore).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SubtitleCaptureIds {
    /// Capture ids.
    pub capture_ids: Vec<String>,
}

/// What an upload is (`X-Media-Kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleMediaKind {
    /// A JPEG frame of the video.
    Screenshot,
    /// A WAV clip of the line.
    Audio,
}

/// Internal `subtitle.media.upload`: raw bytes of `POST /subtitle/v1/media`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UploadSubtitleMedia {
    /// Key it came with.
    pub key_id: Option<SubtitleKeyId>,
    /// `X-Capture-Id`.
    pub capture_id: String,
    /// `X-Media-Kind`.
    pub kind: SubtitleMediaKind,
    /// `Content-Type` (`image/jpeg` or `audio/wav`).
    pub mime: String,
    /// The bytes, base64.
    pub data_base64: String,
}

/// Response of `POST /subtitle/v1/media` (and `subtitle.media.upload`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleUploadResponse {
    /// Id the capture refers to (`media.screenshot.upload_id`, `media.audio.upload_id`).
    pub upload_id: String,
}

// ------------------------------------------------------------------ the contract's wire types

/// `kind` of a capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleCaptureKind {
    /// One word.
    Word,
    /// An expression or words picked together.
    Phrase,
    /// The whole line.
    Sentence,
}

/// Where a definition came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleDefinitionSource {
    /// A dictionary.
    Dict,
    /// An AI model.
    Ai,
    /// Machine translation.
    Mt,
}

/// `lang` of a capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleCaptureLang {
    /// The language learned (ISO 639-1).
    pub target: String,
    /// The user's own.
    pub native: String,
}

/// `term` of a capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleCaptureTerm {
    /// As clicked.
    pub surface: String,
    /// Dictionary form, lowercase.
    pub lemma: String,
    /// Part of speech, if known.
    pub pos: Option<String>,
}

/// `definition` of a capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleCaptureDefinition {
    /// The meaning the user picked.
    pub text: String,
    /// Where it came from.
    pub source: SubtitleDefinitionSource,
}

/// `context` of a capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleCaptureContext {
    /// The subtitle line, plain text.
    pub sentence: Option<String>,
    /// The line in the user's language.
    pub translation: Option<String>,
    /// UTF-16 `[start, end)` of the term in `sentence`.
    pub term_offset: Option<(i64, i64)>,
}

/// `source` of a capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleCaptureSource {
    /// Always `video`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The page at the line (YouTube gets `&t=83s`).
    pub url: Option<String>,
    /// Title (`The Night Train · S1E2`).
    pub title: Option<String>,
    /// `youtube`, `netflix`, …
    pub site: String,
    /// The service's video id.
    pub video_id: Option<String>,
    /// When the line starts (ms).
    pub t_start_ms: Option<u64>,
    /// When it ends (ms).
    pub t_end_ms: Option<u64>,
}

/// An uploaded file a capture refers to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleMediaRef {
    /// From `POST /v1/media`.
    pub upload_id: String,
    /// Its type.
    pub mime: String,
}

/// `media` of a capture.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct SubtitleCaptureMedia {
    /// The frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<SubtitleMediaRef>,
    /// The clip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<SubtitleMediaRef>,
}

/// `client` of a capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleCaptureClient {
    /// `subtitle-extension`.
    pub name: String,
    /// Its version.
    pub ext_version: String,
}

/// The capture record (schema 1). The server ignores fields it does not know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SubtitleCapture {
    /// `1`.
    pub schema: u32,
    /// UUIDv7 made on the device: the idempotency key.
    pub capture_id: String,
    /// What was picked.
    pub kind: SubtitleCaptureKind,
    /// Languages.
    pub lang: SubtitleCaptureLang,
    /// The term.
    pub term: SubtitleCaptureTerm,
    /// Its meaning.
    pub definition: SubtitleCaptureDefinition,
    /// The line.
    pub context: SubtitleCaptureContext,
    /// The video.
    pub source: SubtitleCaptureSource,
    /// Uploaded files.
    pub media: SubtitleCaptureMedia,
    /// From `GET /v1/decks`; `null` = the default deck.
    pub deck_id: Option<String>,
    /// Tags.
    pub tags: Vec<String>,
    /// ISO 8601, on the device.
    pub created_at: String,
    /// The extension.
    pub client: SubtitleCaptureClient,
}

/// `user` of `GET /subtitle/v1/me`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleUser {
    /// The key's id.
    pub id: String,
    /// The key's name (the extension shows it after a connection test).
    pub name: Option<String>,
    /// Never sent (no accounts).
    pub email: Option<String>,
}

/// What this server supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleFeatures {
    /// `POST /v1/media`.
    pub media: bool,
    /// `POST /v1/captures:batch`.
    pub batch: bool,
    /// `GET /v1/known-words` answers with the word index (`false` while the user does not
    /// share it, setting `subtitle.share_known_words`; the route still answers, empty).
    pub known_words: bool,
}

/// `GET /subtitle/v1/me`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleMe {
    /// Who the key belongs to.
    pub user: SubtitleUser,
    /// Number of decks.
    pub decks: u32,
    /// Features.
    pub features: SubtitleFeatures,
}

/// One deck of `GET /subtitle/v1/decks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleDeck {
    /// Deck id (what captures send back as `deck_id`).
    pub id: String,
    /// Full path (`Languages::English`).
    pub name: String,
    /// Cards in it (sub-decks not counted).
    pub cards: u32,
}

/// `GET /subtitle/v1/decks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleDeckList {
    /// Decks by path.
    pub decks: Vec<SubtitleDeck>,
}

/// Answer of `POST /subtitle/v1/captures` (201 created, 200 duplicate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleCaptureResult {
    /// Created or duplicate.
    pub status: SubtitleCaptureStatus,
    /// The card.
    pub card_id: String,
}

/// Status of one capture of a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleBatchStatus {
    /// A new card.
    Created,
    /// Already there.
    Duplicate,
    /// Not stored (`error`).
    Error,
}

/// The contract's error body: `{ "error": { "code", "message", "retryable"? } }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct SubtitleErrorBody {
    /// `invalid_capture`, `unauthorized`, `not_found`, …
    pub code: String,
    /// What went wrong (English; the extension shows it).
    pub message: String,
    /// In a batch result: `true` = try again later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
}

/// Every error answer of `/subtitle/v1/*`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleError {
    /// The error.
    pub error: SubtitleErrorBody,
}

/// One result of `POST /subtitle/v1/captures:batch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct SubtitleBatchItem {
    /// The capture's id (as sent; empty when it had none).
    pub capture_id: String,
    /// What happened.
    pub status: SubtitleBatchStatus,
    /// The card, unless `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<String>,
    /// Why not, for `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SubtitleErrorBody>,
}

/// `POST /subtitle/v1/captures:batch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleBatchResult {
    /// One per capture, in request order.
    pub results: Vec<SubtitleBatchItem>,
}

/// A word's status from the app's reviews.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleWordStatus {
    /// Its card's interval is under 21 days.
    Learning,
    /// A card of it reached an interval of 21 days or more.
    Known,
}

/// One word of `GET /subtitle/v1/known-words`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleKnownWord {
    /// Lemma, lowercase.
    pub lemma: String,
    /// Language.
    pub lang: String,
    /// Known or learning.
    pub status: SubtitleWordStatus,
    /// Longest current interval among its cards (days; 0 before the first review).
    pub interval_days: u32,
    /// Last review (else when it was captured), ISO 8601 UTC.
    pub updated_at: String,
}

/// `GET /subtitle/v1/known-words`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubtitleKnownWords {
    /// Words by language and lemma.
    pub words: Vec<SubtitleKnownWord>,
    /// Pass as `?cursor=` for the next page; `null` = no more.
    pub next_cursor: Option<String>,
    /// `false` = the user does not share known words with the extension (setting
    /// `subtitle.share_known_words`, off by default): `words` is empty and `next_cursor`
    /// `null`, a valid empty answer for any client (v1.4; clients ignore unknown fields).
    pub shared: bool,
}

/// Exports the wire types (not reachable from public method signatures) to TypeScript.
pub fn export_http_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    SubtitleCapture::export_all(cfg)?;
    SubtitleMe::export_all(cfg)?;
    SubtitleDeckList::export_all(cfg)?;
    SubtitleCaptureResult::export_all(cfg)?;
    SubtitleBatchResult::export_all(cfg)?;
    SubtitleKnownWords::export_all(cfg)?;
    SubtitleError::export_all(cfg)?;
    SubtitleUploadResponse::export_all(cfg)?;
    SubtitleMediaKind::export_all(cfg)?;
    Ok(())
}
