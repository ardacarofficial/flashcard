//! Shared machinery of the study handlers: settings, scheduler parameters, session
//! persistence, card rendering (through v0.3's card type engines), snapshots and the
//! session DTO.

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
use fc_core::notetype::NoteType;
use fc_core::{CardId, DeckId, NoteId, NoteTypeId, SessionId, Timestamp};
use fc_providers::strategy::{
    Interaction, ReviewOutcome, Scheduler, SchedulerParams, StrategyError,
};
use fc_storage::repo::SessionRecord;
use fc_storage::Reads;
use fc_study::text::html_to_text;
use fc_study::scheduler::{self, RATINGS};
use fc_study::session::{self as queue, SessionState, LEARN_AHEAD_MS};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

// ------------------------------------------------------------------ settings

/// Resolved value of one of this module's settings at `scope` (stored value along the scope
/// chain, else the schema default).
pub(super) fn setting(reads: &dyn Reads, scope: &SettingScope, key: &str) -> AppResult<Value> {
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

pub(super) fn setting_bool(reads: &dyn Reads, scope: &SettingScope, key: &str) -> AppResult<bool> {
    Ok(setting(reads, scope, key)?.as_bool().unwrap_or(false))
}

pub(super) fn setting_str(reads: &dyn Reads, scope: &SettingScope, key: &str) -> AppResult<String> {
    Ok(setting(reads, scope, key)?
        .as_str()
        .unwrap_or_default()
        .to_owned())
}

pub(super) fn setting_u32(reads: &dyn Reads, scope: &SettingScope, key: &str) -> AppResult<u32> {
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
pub(super) fn scheduler(reads: &dyn Reads) -> AppResult<&'static dyn Scheduler> {
    let id = setting_str(reads, &SettingScope::App, SCHEDULER)?;
    scheduler::builtin(&id)
        .or_else(|| scheduler::builtin(scheduler::FSRS_ID))
        .ok_or_else(|| AppError::api(ErrorCode::Internal, "no scheduler"))
}

pub(super) fn scheduler_error(e: StrategyError) -> AppError {
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
pub(super) struct SessionConfig {
    pub kind: StudyKind,
    pub mode: StudyMode,
    pub deck_id: Option<DeckId>,
    pub tz_offset_mins: i32,
    pub day_start_hour: u8,
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
pub(super) struct Session {
    pub record: SessionRecord,
    pub config: SessionConfig,
    pub state: SessionState,
}

impl Session {
    pub fn load(reads: &dyn Reads, id: SessionId) -> AppResult<Self> {
        let record = reads.get_session(id)?.ok_or_else(|| {
            err_id(ErrorCode::NotFoundSession, format!("session {id} not found"), id)
        })?;
        Ok(Self {
            config: from_json(&record.config)?,
            state: from_json(&record.state)?,
            record,
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

    /// Snapshot of the persisted state (for inverses).
    pub fn snapshot(&self) -> SessionStateSnapshot {
        SessionStateSnapshot {
            id: self.record.id,
            state: self.record.state.clone(),
        }
    }

    /// Writes `state` into the record (call [`Session::save`] after).
    pub fn set_state(&mut self) -> AppResult<()> {
        self.record.state = serde_json::to_value(&self.state)
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
        Ok(())
    }

    pub fn save(&mut self, tx: &mut dyn fc_storage::Writes) -> AppResult<()> {
        self.set_state()?;
        tx.upsert_session(&self.record)?;
        Ok(())
    }

    /// The card the session shows now.
    pub fn current(&self, now: Timestamp) -> Option<&queue::QueueEntry> {
        if self.record.ended_at.is_some() {
            return None;
        }
        queue::next_entry(&self.state.queue, now, LEARN_AHEAD_MS)
    }

    /// Fails unless `card` can be answered now: it is queued and available (no due time, or
    /// due within the learn-ahead window). Not necessarily [`Session::current`]: a learning
    /// card coming due while another card is on screen must not invalidate the shown card.
    pub fn require_available(&self, card: CardId, now: Timestamp) -> AppResult<()> {
        let available = self.record.ended_at.is_none()
            && self.state.queue.iter().any(|e| {
                e.card_id == card
                    && e
                        .due
                        .is_none_or(|d| d <= now.plus_millis(LEARN_AHEAD_MS))
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
pub(super) fn not_available(card: CardId) -> AppError {
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
    }
}

/// `review_log.mode` of a session mode.
pub(super) fn review_mode(mode: StudyMode) -> &'static str {
    match mode {
        StudyMode::Scheduled => "normal",
        StudyMode::Trial => "trial",
    }
}

// ------------------------------------------------------------------ cards

/// A live card (not trashed, note live).
pub(super) fn require_live_card(reads: &dyn Reads, id: CardId) -> AppResult<Card> {
    let card = reads
        .get_card(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundCard, format!("card {id} not found"), id))?;
    if card.deleted_at.is_some() {
        return Err(deleted("card", id));
    }
    Ok(card)
}

/// Study-owned columns of a card.
pub(super) fn snapshot(card: &Card) -> CardStudySnapshot {
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
pub(super) struct View<'a> {
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
pub(super) struct StudyRender {
    pub card: CardRender,
    /// Text hints and typed-answer checks compare with: the engine's expected answer for
    /// typing card types, else the text of the answer part.
    pub expected: String,
    /// The card type asks for typing.
    pub types: bool,
}

pub(super) fn render_for_study(v: View<'_>, card: &Card) -> AppResult<StudyRender> {
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
        Interaction::SelfGrade | Interaction::Choice { .. } => (answer_text(&r.sides.answer), false),
    };
    Ok(StudyRender {
        card: r,
        expected,
        types,
    })
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
pub(super) struct EmptyCards<'a> {
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

/// Interval previews of the four buttons.
pub(super) fn previews(
    sched: &dyn Scheduler,
    state: &SchedulingState,
    now: Timestamp,
    params: &SchedulerParams,
) -> AppResult<Vec<IntervalPreview>> {
    let outcomes: [ReviewOutcome; 4] =
        sched.preview(state, now, params).map_err(scheduler_error)?;
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
pub(super) fn session_dto(v: View<'_>, s: &Session, now: Timestamp) -> AppResult<SessionDto> {
    let reads = v.reads;
    // Cards deleted or emptied (v0.3) since the queue was built are skipped (a query cannot
    // rewrite the session; the next answer persists the queue without them).
    let mut view = s.clone();
    let mut live = None;
    let mut empty = EmptyCards::new(v);
    while let Some(entry) = view.current(now) {
        let id = entry.card_id;
        match require_live_card(reads, id) {
            Ok(card) if !empty.is_empty(&card)? => {
                live = Some(card);
                break;
            }
            _ => {
                queue::remove_cards(&mut view.state, &[id]);
            }
        }
    }
    let s = &view;
    let (new, learning, review) = queue::counts(&s.state.queue);
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
                intervals: previews(scheduler(reads)?, &state, now, &params)?,
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
        next_due: if current.is_none() && s.record.ended_at.is_none() {
            queue::next_due(&s.state.queue)
        } else {
            None
        },
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
