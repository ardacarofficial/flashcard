//! Shared machinery of the study handlers: settings, scheduler parameters, session
//! persistence, card rendering (through v0.3's card type engines), snapshots and the
//! session DTO.

use super::state_diff;
use super::*;
use crate::error::{AppError, AppResult};
use crate::settings::resolve_stored;
use crate::handler::{CommandCtx, QueryCtx};
use crate::registry::Registry;
use crate::support::content::{self, card_seed, CardRender, RenderSetup};
use crate::support::notes::{require_note, require_notetype};
use crate::support::{deleted, err_id, from_json};
use fc_api::v1::study::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::card::{Card, CardState, SchedulingState};
use fc_core::preset::PresetConfig;
use fc_core::settings::SettingScope;
use fc_core::time::MILLIS_PER_DAY;
use fc_core::i18n::Locale;
use fc_core::content::render::render_field;
use fc_core::note::Note;
use fc_core::notetype::{FieldRole, NoteType};
use fc_core::{CardId, DeckId, NoteId, NoteTypeId, SessionId, Timestamp};
use fc_providers::strategy::{
    Interaction, ReviewOutcome, Scheduler, SchedulerParams, StrategyError,
};
use fc_storage::repo::SessionRecord;
use fc_storage::Reads;
use fc_study::hint::{ContextSource, HintKind, HintShape, LeveledHint, MaskedSentence};
use fc_study::text::html_to_text;
use fc_study::scheduler::{self, RATINGS};
use fc_study::session::{self as queue, SessionState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

// ------------------------------------------------------------------ settings

/// Resolved value of one of this module's settings at `scope` (stored value along the scope
/// chain, else the schema default).
pub(crate) fn setting(reads: &dyn Reads, scope: &SettingScope, key: &str) -> AppResult<Value> {
    if let Some((v, _)) = resolve_stored(reads, scope, key)? {
        return Ok(v);
    }
    descriptor()
        .settings
        .into_iter()
        .find(|d| d.key == key)
        .map(|d| d.default)
        .ok_or_else(|| AppError::api(ErrorCode::Internal, format!("unknown setting {key}")))
}

pub(crate) fn setting_bool(reads: &dyn Reads, scope: &SettingScope, key: &str) -> AppResult<bool> {
    Ok(setting(reads, scope, key)?.as_bool().unwrap_or(false))
}

pub(crate) fn setting_str(reads: &dyn Reads, scope: &SettingScope, key: &str) -> AppResult<String> {
    Ok(setting(reads, scope, key)?
        .as_str()
        .unwrap_or_default()
        .to_owned())
}

pub(crate) fn setting_u32(reads: &dyn Reads, scope: &SettingScope, key: &str) -> AppResult<u32> {
    Ok(setting(reads, scope, key)?
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0))
}

/// Scope for deck-level settings: the deck if given, else app-wide.
pub(super) fn deck_scope(deck: Option<DeckId>) -> SettingScope {
    deck.map_or(SettingScope::App, SettingScope::Deck)
}

/// The configured scheduler.
pub(crate) fn scheduler(reads: &dyn Reads) -> AppResult<&'static dyn Scheduler> {
    let id = setting_str(reads, &SettingScope::App, SCHEDULER)?;
    scheduler::builtin(&id)
        .or_else(|| scheduler::builtin(scheduler::FSRS_ID))
        .ok_or_else(|| AppError::api(ErrorCode::Internal, "no scheduler"))
}

pub(crate) fn scheduler_error(e: StrategyError) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationSchedulerParams, e.0.clone()).with_detail("reason", e.0),
    )
}

/// Scheduler parameters of `deck`: its effective preset (own, else the nearest ancestor's,
/// else the defaults — v0.4 `presets::effective`, the one read path for deck options).
pub(super) fn params_for_deck(
    reads: &dyn Reads,
    deck: DeckId,
    cache: &mut HashMap<DeckId, SchedulerParams>,
) -> AppResult<SchedulerParams> {
    if let Some(p) = cache.get(&deck) {
        return Ok(p.clone());
    }
    let p = params_from_preset(&effective_options(reads, Some(deck))?);
    cache.insert(deck, p.clone());
    Ok(p)
}

/// Study options of `deck` through [`crate::modules::presets::effective`]; the defaults for
/// whole-collection sessions (`None`) and decks that no longer exist.
pub(super) fn effective_options(reads: &dyn Reads, deck: Option<DeckId>) -> AppResult<PresetConfig> {
    match deck {
        Some(d) if reads.get_deck(d)?.is_some() => {
            Ok(crate::modules::presets::effective(reads, d)?.config)
        }
        _ => Ok(PresetConfig::default()),
    }
}

fn params_from_preset(preset: &PresetConfig) -> SchedulerParams {
    let extra = if preset.fsrs_weights.is_empty() {
        Value::Null
    } else {
        serde_json::json!({ "weights": preset.fsrs_weights })
    };
    SchedulerParams {
        desired_retention: preset.desired_retention,
        maximum_interval_days: preset.maximum_interval_days,
        learning_steps_mins: preset.learning_steps_mins.clone(),
        relearning_steps_mins: preset.relearning_steps_mins.clone(),
        extra,
    }
}

// ------------------------------------------------------------------ sessions

/// Start options persisted in `sessions.config`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SessionConfig {
    pub kind: StudyKind,
    pub mode: StudyMode,
    pub deck_id: Option<DeckId>,
    pub tz_offset_mins: i32,
    pub day_start_hour: u8,
    /// v0.10 mode options (timed, confidence); absent for plain sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<fc_study::modes::ModesConfig>,
}

impl SessionConfig {
    /// The session's study calendar.
    pub fn study_day(&self) -> fc_study::day::StudyDay {
        fc_study::day::StudyDay::new(self.tz_offset_mins, self.day_start_hour)
    }

    /// End of the study day containing `now`.
    pub fn day_end(&self, now: Timestamp) -> Timestamp {
        self.study_day().end(now)
    }

    /// Scheduler parameters of `deck` on this session's study calendar.
    pub fn params(&self, reads: &dyn Reads, deck: DeckId) -> AppResult<SchedulerParams> {
        let params = params_for_deck(reads, deck, &mut HashMap::new())?;
        Ok(scheduler::with_study_day(params, self.study_day()))
    }
}

/// A loaded session.
#[derive(Debug, Clone)]
pub(crate) struct Session {
    pub record: SessionRecord,
    pub config: SessionConfig,
    pub state: SessionState,
    /// What the database holds (as loaded or last saved): [`Session::save`] writes only the
    /// difference (v1.0).
    stored: Stored,
    /// The app's queue cache (saves update it).
    cache: Option<Arc<state_diff::QueueCache>>,
    /// `study.learn_ahead_mins` in ms (v1.3), as read when the session was loaded (the
    /// session works with [`Session::shown_window_ms`]).
    pub learn_ahead_ms: i64,
}

/// The persisted state of a [`Session`].
#[derive(Debug, Clone, Default)]
struct Stored {
    state: SessionState,
    /// `seq` of each queue row, parallel to `state.queue`. `None`: no rows yet (a new
    /// session, or a pre-v1.0 queue kept inside `sessions.state`) — the next save writes them
    /// all.
    seqs: Option<Vec<i64>>,
}

impl Session {
    /// A session that is not stored yet ([`Session::save`] writes it).
    pub fn new(
        registry: &Registry,
        record: SessionRecord,
        config: SessionConfig,
        state: SessionState,
        learn_ahead_ms: i64,
    ) -> Self {
        Self {
            record,
            config,
            state,
            stored: Stored::default(),
            cache: registry.extension::<state_diff::QueueCache>(),
            learn_ahead_ms,
        }
    }

    pub fn load(reads: &dyn Reads, registry: &Registry, id: SessionId) -> AppResult<Self> {
        let record = reads.get_session(id)?.ok_or_else(|| {
            err_id(ErrorCode::NotFoundSession, format!("session {id} not found"), id)
        })?;
        let mut state: SessionState = from_json(&record.state)?;
        let cache = registry.extension::<state_diff::QueueCache>();
        let cached = match (&cache, record.state.get(state_diff::REV_KEY)) {
            (Some(c), Some(Value::String(rev))) if state.queue.is_empty() => c.get(id, rev),
            _ => None,
        };
        // v1.0: the queue is in `session_queue` (a non-empty queue in the JSON is from before
        // migration 1071 and moves to rows on the next save), or still in memory from the
        // last save.
        let seqs = if let Some((queue, seqs)) = cached {
            state.queue = queue;
            Some(seqs)
        } else if state.queue.is_empty() {
            let rows = reads.session_queue(id)?;
            let mut seqs = Vec::with_capacity(rows.len());
            let mut queue = Vec::with_capacity(rows.len());
            for r in rows {
                seqs.push(r.seq);
                queue.push(queue::QueueEntry {
                    card_id: r.card_id,
                    note_id: r.note_id,
                    kind: state_diff::queue_kind(&r.kind)?,
                    due: r.due,
                });
            }
            state.queue = queue;
            Some(seqs)
        } else {
            None
        };
        Ok(Self {
            config: from_json(&record.config)?,
            stored: Stored {
                state: state.clone(),
                seqs,
            },
            state,
            record,
            cache,
            learn_ahead_ms: super::plan::learn_ahead_ms(reads)?,
        })
    }

    pub fn require_open(&self) -> AppResult<()> {
        if self.record.ended_at.is_some() {
            return Err(err_id(
                ErrorCode::ConflictSessionEnded,
                format!("session {} has ended", self.record.id),
                self.record.id,
            ));
        }
        Ok(())
    }

    /// Writes the session: its record once, and of the queue only the rows that changed
    /// since it was loaded or last saved. Returns what undoes this save (for the
    /// `study.restore` inverse): a diff back to the stored state, not a copy of the queue.
    pub fn save(&mut self, tx: &mut dyn fc_storage::Writes) -> AppResult<state_diff::SessionUndo> {
        // v1.3: the window the card on screen was chosen with, widened by a raised setting,
        // never narrowed between two answers ([`Session::shown_window_ms`]).
        self.state.learn_ahead_ms = Some(self.shown_window_ms());
        let ops = state_diff::diff(&self.stored.state.queue, &self.state.queue);
        let stored_queue = std::mem::take(&mut self.stored.state.queue);
        let stored_rest = serde_json::to_value(&self.stored.state);
        self.stored.state.queue = stored_queue;
        let queue = std::mem::take(&mut self.state.queue);
        let rest = serde_json::to_value(&self.state);
        self.state.queue = queue;
        let mut rest = rest.map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
        let rev = state_diff::new_rev();
        if let Value::Object(m) = &mut rest {
            m.insert(state_diff::REV_KEY.to_owned(), Value::String(rev.clone()));
        }
        self.record.state = rest;
        tx.upsert_session(&self.record)?;
        let planned = self
            .stored
            .seqs
            .as_deref()
            .and_then(|seqs| state_diff::plan_rows(seqs, &ops, &self.state.queue));
        let seqs = match planned {
            Some(plan) => {
                if !plan.remove.is_empty() || !plan.add.is_empty() {
                    tx.edit_session_queue(self.record.id, false, &plan.remove, &plan.add)?;
                }
                plan.seqs
            }
            None => {
                // Every row (first save, a pre-v1.0 queue, or no room between two seqs).
                let seqs: Vec<i64> = (1..=self.state.queue.len() as i64)
                    .map(|i| i * state_diff::SEQ_GAP)
                    .collect();
                let rows: Vec<_> = self
                    .state
                    .queue
                    .iter()
                    .zip(&seqs)
                    .map(|(e, seq)| state_diff::row(*seq, e))
                    .collect();
                tx.edit_session_queue(self.record.id, true, &[], &rows)?;
                seqs
            }
        };
        let undo = state_diff::SessionUndo {
            id: self.record.id,
            rest: stored_rest.map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?,
            ops,
        };
        if let Some(cache) = &self.cache {
            cache.put(self.record.id, rev, &self.state.queue, &seqs);
        }
        self.stored = Stored {
            state: self.state.clone(),
            seqs: Some(seqs),
        };
        Ok(undo)
    }

    /// The state a `study.restore` snapshot describes: a v1.0 diff applied to this
    /// session's state, or (older undo entries) the full state.
    pub fn restored_state(&self, snapshot: &Value) -> AppResult<SessionState> {
        state_diff::apply(&self.state, snapshot)
    }

    /// The card the session shows now.
    pub fn current(&self, now: Timestamp) -> Option<&queue::QueueEntry> {
        if self.record.ended_at.is_some() {
            return None;
        }
        queue::next_entry(&self.state.queue, now, self.shown_window_ms())
    }

    /// The learn-ahead window the session works with (v1.3): the larger of the setting now
    /// and the one recorded at the start / last answer. A raised `study.learn_ahead_mins`
    /// applies at once; a lowered one from the next answer, so the card on screen never
    /// disappears or becomes unanswerable because of a settings change.
    pub fn shown_window_ms(&self) -> i64 {
        self.state
            .learn_ahead_ms
            .map_or(self.learn_ahead_ms, |w| w.max(self.learn_ahead_ms))
    }

    /// Starts the window over from the setting (a new card is chosen: start, answer).
    pub fn reset_shown_window(&mut self) {
        self.state.learn_ahead_ms = Some(self.learn_ahead_ms);
    }

    /// Fails unless `card` can be answered now: it is queued and available (no due time, or
    /// due within the learn-ahead window it was shown with). Not necessarily
    /// [`Session::current`]: a learning card coming due while another card is on screen must
    /// not invalidate the shown card, and neither may a lowered learn-ahead setting.
    pub fn require_available(&self, card: CardId, now: Timestamp) -> AppResult<()> {
        let window = self.shown_window_ms();
        let available = self.record.ended_at.is_none()
            && self.state.queue.iter().any(|e| {
                e.card_id == card && e.due.is_none_or(|d| d <= now.plus_millis(window))
            });
        if !available {
            return Err(not_available(card));
        }
        Ok(())
    }

    /// Scheduling state the session sees for `card` (simulated in trial sessions).
    pub fn scheduling_of(&self, card: &Card) -> SchedulingState {
        match self.config.mode {
            StudyMode::Trial => queue::trial_state(&self.state, card.id)
                .cloned()
                .unwrap_or_else(|| card.scheduling.clone()),
            StudyMode::Scheduled => card.scheduling.clone(),
        }
    }
}

/// `CONFLICT_NOT_CURRENT_CARD` for `card`.
pub(crate) fn not_available(card: CardId) -> AppError {
    AppError::Api(
        ApiError::new(
            ErrorCode::ConflictNotCurrentCard,
            format!("card {card} is not available in this session"),
        )
        .with_detail("card_id", card.to_string()),
    )
}

/// Database form of a study kind.
pub(super) fn kind_str(kind: StudyKind) -> &'static str {
    match kind {
        StudyKind::Normal => "normal",
        StudyKind::Custom => "custom",
        StudyKind::Mistakes => "mistakes",
        StudyKind::Learn => "learn",
        StudyKind::Test => "test",
        StudyKind::Match => "match",
        StudyKind::Quiz => "quiz",
    }
}

/// `review_log.mode` of an answer in session `config` (docs/10 §24): the mode the answer
/// came from — `learn` (learn sessions), `timed` (timed sessions), `confidence` (rated on the
/// confidence scale), else `normal` — prefixed `trial_` in trial sessions (plain trial
/// answers stay `trial`). Test answers are `test` (written by the test module), match answers
/// `match` (v1.1, the match module).
pub(crate) fn review_mode(config: &SessionConfig, confidence: bool) -> String {
    let modes = config.modes.as_ref();
    let base = match config.kind {
        StudyKind::Learn => Some("learn"),
        StudyKind::Test => Some("test"),
        StudyKind::Match => Some("match"),
        StudyKind::Quiz => Some("quiz"),
        StudyKind::Normal | StudyKind::Custom | StudyKind::Mistakes => {
            if modes.is_some_and(|m| m.timed.is_some()) {
                Some("timed")
            } else if confidence {
                Some("confidence")
            } else {
                None
            }
        }
    };
    match (config.mode, base) {
        (_, Some("test")) => "test".to_owned(),
        (_, Some("match")) => "match".to_owned(),
        (StudyMode::Scheduled, b) => b.unwrap_or("normal").to_owned(),
        (StudyMode::Trial, Some(b)) => format!("trial_{b}"),
        (StudyMode::Trial, None) => "trial".to_owned(),
    }
}

// ------------------------------------------------------------------ cards

/// A live card (not trashed, note live).
pub(crate) fn require_live_card(reads: &dyn Reads, id: CardId) -> AppResult<Card> {
    let card = reads
        .get_card(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundCard, format!("card {id} not found"), id))?;
    if card.deleted_at.is_some() {
        return Err(deleted("card", id));
    }
    Ok(card)
}

/// Study-owned columns of a card.
pub(crate) fn snapshot(card: &Card) -> CardStudySnapshot {
    let s = &card.scheduling;
    CardStudySnapshot {
        id: card.id,
        state: s.state,
        due: s.due,
        stability: s.stability,
        difficulty: s.difficulty,
        reps: s.reps,
        lapses: s.lapses,
        last_review: s.last_review,
        suspended: card.suspended,
        buried_until: card.buried_until,
        updated_at: card.updated_at,
    }
}

/// Writes a snapshot's columns into `card`.
pub(super) fn apply_snapshot(card: &mut Card, snap: &CardStudySnapshot) {
    card.scheduling = SchedulingState {
        state: snap.state,
        due: snap.due,
        stability: snap.stability,
        difficulty: snap.difficulty,
        reps: snap.reps,
        lapses: snap.lapses,
        last_review: snap.last_review,
    };
    card.suspended = snap.suspended;
    card.buried_until = snap.buried_until;
    card.updated_at = snap.updated_at;
}

/// What rendering needs: reads plus the card type engines and the UI locale.
#[derive(Clone, Copy)]
pub(crate) struct View<'a> {
    pub reads: &'a dyn Reads,
    pub registry: &'a Registry,
    pub locale: Locale,
}

impl<'a> View<'a> {
    pub fn cmd(ctx: &'a CommandCtx<'_>) -> Self {
        Self {
            reads: ctx.reads(),
            registry: ctx.registry,
            locale: ctx.locale,
        }
    }

    pub fn query(ctx: &'a QueryCtx<'_>) -> Self {
        Self {
            reads: ctx.tx,
            registry: ctx.registry,
            locale: ctx.locale,
        }
    }
}

/// A card rendered by its card type engine (v0.3), plus what study derives from it.
pub(crate) struct StudyRender {
    pub card: CardRender,
    /// Text hints and typed-answer checks compare with: the engine's expected answer for
    /// typing card types, else the text of the answer part.
    pub expected: String,
    /// The card type asks for typing.
    pub types: bool,
}

pub(crate) fn render_for_study(v: View<'_>, card: &Card) -> AppResult<StudyRender> {
    let note = require_note(v.reads, card.note_id)?;
    let nt = require_notetype(v.reads, note.notetype_id)?;
    let r = content::render_card(
        v.registry,
        v.reads,
        &nt,
        &note,
        &card.template_key,
        &RenderSetup {
            deck: Some(card.deck_id),
            seed: card_seed(card.id),
            locale: v.locale,
        },
    )?;
    let (expected, types) = match &r.raw.interaction {
        Interaction::Type { expected } => (expected.clone(), true),
        Interaction::Choice { .. } => (answer_text(&r.sides.answer), false),
        // v1.3: the field the template asks for (a vocabulary card owes its meaning, not the
        // example and source printed under it), else the answer part of the back.
        Interaction::SelfGrade => (
            content::answer_field_text(&nt, &note, &card.template_key)
                .unwrap_or_else(|| answer_text(&r.sides.answer)),
            false,
        ),
    };
    Ok(StudyRender {
        card: r,
        expected,
        types,
    })
}

/// Rendered HTML of the field with `role` in `note` (`None` when absent or empty).
fn role_html(nt: &NoteType, note: &Note, role: FieldRole) -> Option<String> {
    let def = nt.field_with_role(role)?;
    let raw = note.fields.get(&def.key)?;
    let html = render_field(def.kind, raw, None, &|_| None);
    (!html_to_text(&html).trim().is_empty()).then_some(html)
}

/// The v1.3 context hint of a rendered card (X1 rule: a hint may reveal context but never
/// the answer). The note's example sentence: on a card asking for the word (reverse /
/// production) its occurrence is blanked; on one asking for the meaning the word is
/// highlighted and the meaning blanked only where it occurs. `None` when the question already
/// shows that sentence (the hint would show nothing new) or the sentence would still give the
/// answer away (letters then).
fn context_of(nt: &NoteType, note: &Note, template_key: &str, r: &StudyRender) -> Option<MaskedSentence> {
    let example = role_html(nt, note, FieldRole::Example)?;
    let norm = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let sentence = norm(&html_to_text(&example));
    if sentence.is_empty() || norm(&html_to_text(&r.card.sides.question)).contains(&sentence) {
        return None;
    }
    let text = |role| role_html(nt, note, role).map(|h| html_to_text(&h));
    let word_forms: Vec<String> = [text(FieldRole::Surface), text(FieldRole::Term)]
        .into_iter()
        .flatten()
        .collect();
    // What the card asks for: its answer field's role, else whether the expected answer is
    // the word itself.
    let answer_role = content::template_for(nt, template_key)
        .and_then(|t| t.answer_field.as_ref())
        .and_then(|k| nt.field(k))
        .and_then(|f| f.role);
    let asks_for_word = match answer_role {
        Some(role) => matches!(role, FieldRole::Term | FieldRole::Surface),
        None => word_forms.iter().any(|w| norm(w) == norm(&r.expected)),
    };
    let answer_forms = fc_study::hint::answer_forms(&r.expected);
    fc_study::hint::context_hint(&ContextSource {
        example_html: &example,
        word_forms: &word_forms,
        asks_for_word,
        answer_forms: &answer_forms,
    })
}

/// `study.hint` for `card` at `level` (1 = smallest): the context sentence first when the
/// note has a usable one, then letters of the expected answer by the hint strategy `kind`.
pub(crate) fn study_hint(
    v: View<'_>,
    card: CardId,
    kind: HintKind,
    level: u32,
) -> AppResult<Option<LeveledHint>> {
    let card = require_live_card(v.reads, card)?;
    let r = render_for_study(v, &card)?;
    let note = require_note(v.reads, card.note_id)?;
    let nt = require_notetype(v.reads, note.notetype_id)?;
    let context = context_of(&nt, &note, &card.template_key, &r);
    Ok(fc_study::hint::leveled_hint(
        kind,
        &r.expected,
        context.as_ref(),
        level,
    ))
}

/// The `study.hint` response for `hint` at `level`.
pub(crate) fn hint_dto(level: u32, hint: Option<LeveledHint>) -> HintDto {
    match hint {
        Some(h) => HintDto {
            level,
            kind: match h.shape {
                HintShape::Context => fc_api::v1::study::HintKind::Context,
                HintShape::Letters => fc_api::v1::study::HintKind::Letters,
            },
            text: Some(h.text),
            html: Some(h.html),
        },
        None => HintDto {
            level,
            text: None,
            kind: fc_api::v1::study::HintKind::Letters,
            html: None,
        },
    }
}

/// Text of the answer part of (sanitized) answer HTML: what follows the rule that separates
/// the repeated question (`<hr class="fc-answer">`, else the first `<hr>` — the v0.1 / Anki
/// `<hr id="answer">` loses its id in sanitizing); the whole side when there is none.
pub(super) fn answer_text(html: &str) -> String {
    let after_tag = |i: usize| html[i..].find('>').map(|j| &html[i + j + 1..]);
    let rest = html
        .find("fc-answer")
        .and_then(after_tag)
        .or_else(|| html.find("<hr").and_then(after_tag))
        .unwrap_or(html);
    html_to_text(rest)
}

/// Whether cards are **empty** (v0.3: the note no longer yields them — a removed cloze
/// number or mask, an optional reverse switched off). Study never shows them. Caches the
/// card keys per note.
pub(crate) struct EmptyCards<'a> {
    view: View<'a>,
    keys: HashMap<NoteId, BTreeSet<String>>,
    notetypes: HashMap<NoteTypeId, NoteType>,
}

impl<'a> EmptyCards<'a> {
    pub fn new(view: View<'a>) -> Self {
        Self {
            view,
            keys: HashMap::new(),
            notetypes: HashMap::new(),
        }
    }

    pub fn is_empty(&mut self, card: &Card) -> AppResult<bool> {
        if let Some(keys) = self.keys.get(&card.note_id) {
            return Ok(!keys.contains(&card.template_key));
        }
        let note = require_note(self.view.reads, card.note_id)?;
        let nt = match self.notetypes.entry(note.notetype_id) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(require_notetype(self.view.reads, note.notetype_id)?)
            }
        };
        let keys: BTreeSet<String> = content::card_specs(self.view.registry, nt, &note)?
            .into_iter()
            .map(|s| s.template_key)
            .collect();
        let empty = !keys.contains(&card.template_key);
        self.keys.insert(card.note_id, keys);
        Ok(empty)
    }
}

/// Interval previews of the four buttons. `plan` is applied to each outcome exactly as
/// `study.answer` applies it (v0.6 planning: load balancing, easy days, exam-date mode), so a
/// button shows the interval its answer will get.
pub(super) fn previews(
    sched: &dyn Scheduler,
    state: &SchedulingState,
    now: Timestamp,
    params: &SchedulerParams,
    plan: &mut dyn FnMut(&mut ReviewOutcome) -> AppResult<()>,
) -> AppResult<Vec<IntervalPreview>> {
    let mut outcomes: [ReviewOutcome; 4] =
        sched.preview(state, now, params).map_err(scheduler_error)?;
    for o in &mut outcomes {
        plan(o)?;
    }
    Ok(RATINGS
        .iter()
        .zip(outcomes.iter())
        .map(|(rating, o)| IntervalPreview {
            rating: *rating,
            // Whole-day intervals are shown as days (the due time is the start of the due
            // study day, so `due - now` would read "13 h" for "1 day").
            interval_ms: if o.scheduled_days > 0 {
                i64::from(o.scheduled_days).saturating_mul(MILLIS_PER_DAY)
            } else {
                o.state
                    .due
                    .map_or(0, |d| d.as_millis().saturating_sub(now.as_millis()).max(0))
            },
            scheduled_days: o.scheduled_days,
        })
        .collect())
}

/// Whether `outcome` keeps the card in today's session (an intraday step due before the day
/// ends) and when.
pub(super) fn requeue_at(state: &SchedulingState, day_end: Timestamp) -> Option<Timestamp> {
    match state.state {
        CardState::Learning | CardState::Relearning => state.due.filter(|d| *d < day_end),
        CardState::New | CardState::Review => None,
    }
}

/// The session as the study screen sees it at `now`.
pub(crate) fn session_dto(v: View<'_>, s: &Session, now: Timestamp) -> AppResult<SessionDto> {
    let reads = v.reads;
    // Cards deleted or emptied (v0.3) since the queue was built are skipped (a query cannot
    // rewrite the session; the next answer persists the queue without them). The session is
    // copied only when there is one to skip.
    let mut view: Option<Session> = None;
    let mut live = None;
    let mut empty = EmptyCards::new(v);
    while let Some(entry) = view.as_ref().unwrap_or(s).current(now) {
        let id = entry.card_id;
        match require_live_card(reads, id) {
            Ok(card) if !empty.is_empty(&card)? => {
                live = Some(card);
                break;
            }
            _ => {
                let copy = view.get_or_insert_with(|| s.clone());
                queue::remove_cards(&mut copy.state, &[id]);
            }
        }
    }
    let s = view.as_ref().unwrap_or(s);
    let (new, learning, review) = queue::counts(&s.state.queue);
    let shown = live.clone();
    let current = match live {
        Some(card) => {
            let state = s.scheduling_of(&card);
            let params = s.config.params(reads, card.deck_id)?;
            let rendered = render_for_study(v, &card)?;
            // Choice cards are answered by picking, never by typing.
            let choice = matches!(rendered.card.raw.interaction, Interaction::Choice { .. });
            let type_setting = !choice
                && setting_bool(reads, &SettingScope::Deck(card.deck_id), TYPE_ANSWER)?;
            Some(StudyCardDto {
                card_id: card.id,
                note_id: card.note_id,
                deck_id: card.deck_id,
                state: state.state,
                intervals: previews(scheduler(reads)?, &state, now, &params, &mut |o| {
                    if s.config.mode == StudyMode::Scheduled {
                        crate::modules::planning::plan_outcome(
                            reads,
                            v.registry,
                            &card,
                            o,
                            s.config.study_day(),
                            now,
                            params.maximum_interval_days,
                        )?;
                    }
                    Ok(())
                })?,
                type_answer: rendered.types || type_setting,
                flags: card.flags,
                sides: rendered.card.sides,
                hints_used: if s.state.hint_card == Some(card.id) {
                    s.state.hints
                } else {
                    0
                },
            })
        }
        None => None,
    };
    Ok(SessionDto {
        id: s.record.id,
        kind: s.config.kind,
        mode: s.config.mode,
        deck_id: s.config.deck_id,
        started_at: s.record.started_at,
        ended_at: s.record.ended_at,
        counts: StudyCounts {
            new,
            learning,
            review,
        },
        answered: s.state.answered,
        correct: s.state.correct,
        mistakes: u32::try_from(s.state.again.len()).unwrap_or(u32::MAX),
        // v1.3: when the first waiting card is shown (its due time minus the learn-ahead
        // window `study.learn_ahead_mins`), so the waiting screen's countdown ends exactly
        // when the card comes.
        next_due: if current.is_none() && s.record.ended_at.is_none() {
            queue::next_due(&s.state.queue).map(|d| queue::available_at(d, s.shown_window_ms()))
        } else {
            None
        },
        // v0.10 study modes (learn stage/options, timer, confidence buttons, test state).
        modes: crate::modules::modes::session_modes(v, s, shown.as_ref(), current.as_ref(), now)?,
        current,
    })
}

/// End-of-session numbers.
pub(super) fn summary_dto(s: &Session) -> SessionSummary {
    SessionSummary {
        session_id: s.record.id,
        kind: s.config.kind,
        mode: s.config.mode,
        answered: s.state.answered,
        correct: s.state.correct,
        mistakes: u32::try_from(s.state.again.len()).unwrap_or(u32::MAX),
        time_ms: s.state.time_ms,
        started_at: s.record.started_at,
        ended_at: s.record.ended_at,
    }
}
