//! `quiz` module (v1.4) — the mode the app calls **"Test"** (docs/02 §6.1): one word at a
//! time, up to four meanings, the right one shown at once, then the next word; the score and
//! the missed words at the end. `study.start { kind: quiz }` takes the cards like a learn
//! session (deck, scope, query); one word per note. `modes.quiz.answer` judges a pick
//! (`review_log.mode = trial_quiz`, schedule unchanged — or `quiz`, rated right → good /
//! wrong → again, when the `quiz.affects_schedule` setting was on at the start);
//! `modes.quiz.result` returns the score and the missed words, and `study.start { kind: quiz,
//! from_session }` asks those again.
//!
//! **Options by field role** (v1.3 roles): the word asked is the note's `term` field, the
//! options are `meaning` fields — of the other words of the session and of other notes in the
//! same decks, in the same language (the words index), never a note with the same word,
//! never the example or the source. Notes without those roles are asked by their card (the
//! question side; the expected answer and the other such cards' answers as options); choice
//! card types bring their own options. A word with no distinct distractor is left out; one
//! with fewer than three is asked with fewer options and says so (`QuizDto.few_options`).

use super::{session_mode, setting};
use crate::error::AppResult;
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{bool_setting, ModuleDescriptor};
use crate::modules::study::ext::*;
use crate::registry::Registry;
use crate::support::content::role_text;
use crate::support::notes::{require_note, require_notetype};
use fc_api::v1::cardtypes::CardInteraction;
use fc_api::v1::methods::{ModesQuizAnswer, ModesQuizResult};
use fc_api::v1::modes::{
    QuizAnswer, QuizAnswerResult, QuizDto, QuizMissDto, QuizProgressDto, QuizRef, QuizResultDto,
};
use fc_api::v1::study::{AnswerCard, StudyKind};
use fc_core::card::{Card, Rating};
use fc_core::notetype::{FieldRole, NoteType};
use fc_core::rng::Rng;
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::{CardId, DeckId, NoteId, NoteTypeId};
use fc_storage::repo::CardFilter;
use fc_study::modes::quiz::{self as pure, QuizItem, QuizState};
use fc_study::modes::{option_key, shuffle, ModesState};
use fc_study::session::SessionState;
use fc_study::text::html_to_text;
use std::collections::{HashMap, HashSet};

/// Module id.
pub const ID: &str = "quiz";
/// Answers change the schedule (default off: the quiz is practice).
pub const AFFECTS_SCHEDULE: &str = "quiz.affects_schedule";
/// Most words in a quiz without an explicit `filter.limit`.
pub const MAX_CARDS: &str = "quiz.max_cards";
/// Most notes outside the session read for distractors (a random sample of the decks).
const POOL_NOTES: usize = 300;

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.quiz.name",
        description_key: "modules.quiz.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![
            bool_setting(AFFECTS_SCHEDULE, false, &[SettingScopeKind::App]),
            SettingDef {
                key: MAX_CARDS.to_owned(),
                kind: SettingKind::Integer {
                    min: Some(1),
                    max: Some(200),
                },
                default: serde_json::Value::from(20),
                label_key: format!("settings.{MAX_CARDS}.label"),
                description_key: Some(format!("settings.{MAX_CARDS}.description")),
                advanced: false,
                scopes: vec![SettingScopeKind::App],
            },
        ],
        // Started from the study launcher (no page of its own).
        ui: Vec::new(),
    }
}

/// Registers the module.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.command::<ModesQuizAnswer>(answer);
    reg.query::<ModesQuizResult>(result);
}

/// `quiz.affects_schedule` now.
pub(crate) fn affects_schedule(reads: &dyn fc_storage::Reads) -> AppResult<bool> {
    Ok(setting(reads, &SettingScope::App, AFFECTS_SCHEDULE, descriptor)?
        .as_bool()
        .unwrap_or(false))
}

/// A word of the session as read at the start.
struct Word {
    card_id: CardId,
    note_id: NoteId,
    /// `term` field (roles), else `None`.
    term: Option<String>,
    /// Comparison key of the term ("" without roles).
    term_key: String,
    /// The card's question side as text.
    prompt: String,
    /// The right option: the `meaning` field (roles) or the card's expected answer.
    answer: String,
    /// Choice card types: their own options.
    own: Option<(Vec<String>, u32)>,
    /// Words-index language ("" = unknown).
    language: String,
}

/// A distractor candidate: another note's meaning.
struct Candidate {
    note_id: NoteId,
    term_key: String,
    meaning: String,
    language: String,
    /// From a note with roles (`term` + `meaning`).
    roles: bool,
}

/// `term` and `meaning` of a note with those roles (both non-empty).
fn term_meaning(nt: &NoteType, note: &fc_core::note::Note) -> Option<(String, String)> {
    let term = role_text(nt, note, FieldRole::Term).or_else(|| role_text(nt, note, FieldRole::Surface))?;
    let meaning = role_text(nt, note, FieldRole::Meaning)?;
    Some((term, meaning))
}

struct Types<'a> {
    reads: &'a dyn fc_storage::Reads,
    cache: HashMap<NoteTypeId, NoteType>,
}

impl Types<'_> {
    fn get(&mut self, id: NoteTypeId) -> AppResult<&NoteType> {
        if !self.cache.contains_key(&id) {
            let nt = require_notetype(self.reads, id)?;
            self.cache.insert(id, nt);
        }
        Ok(&self.cache[&id])
    }
}

/// Words-index language per note ("" = unknown or not indexed yet).
fn languages(reads: &dyn fc_storage::Reads, notes: &[NoteId]) -> AppResult<HashMap<NoteId, String>> {
    Ok(reads
        .word_index_notes_by_id(notes)?
        .into_iter()
        .map(|r| (r.note_id, r.language))
        .collect())
}

/// `study.start` hook: one word per note (capped by `quiz.max_cards`), the options of every
/// word fixed now; words without a distractor leave the queue.
pub(super) fn init(v: View<'_>, state: &mut SessionState, rng: &mut dyn Rng) -> AppResult<()> {
    let max = setting(v.reads, &SettingScope::App, MAX_CARDS, descriptor)?
        .as_u64()
        .unwrap_or(20) as usize;
    let affects = affects_schedule(v.reads)?;
    let mut notes_seen = HashSet::new();
    state.queue.retain(|e| notes_seen.insert(e.note_id));
    state.queue.truncate(max);
    let mut types = Types {
        reads: v.reads,
        cache: HashMap::new(),
    };
    let mut words = Vec::with_capacity(state.queue.len());
    let mut decks: Vec<DeckId> = Vec::new();
    for e in &mut state.queue {
        e.due = None;
        let card = require_live_card(v.reads, e.card_id)?;
        if !decks.contains(&card.deck_id) {
            decks.push(card.deck_id);
        }
        let r = render_for_study(v, &card)?;
        let note = require_note(v.reads, card.note_id)?;
        let nt = types.get(note.notetype_id)?;
        let prompt = html_to_text(&r.card.sides.question).trim().to_owned();
        let word = match (&r.card.sides.interaction, term_meaning(nt, &note)) {
            (CardInteraction::Choice { options, correct }, _) => {
                let options: Vec<String> = options.iter().map(|o| html_to_text(o)).collect();
                Word {
                    card_id: card.id,
                    note_id: card.note_id,
                    term: None,
                    term_key: String::new(),
                    prompt,
                    answer: options.get(*correct as usize).cloned().unwrap_or_default(),
                    own: Some((options, *correct)),
                    language: String::new(),
                }
            }
            (_, Some((term, meaning))) => Word {
                card_id: card.id,
                note_id: card.note_id,
                term_key: option_key(&term),
                term: Some(term),
                prompt,
                answer: meaning,
                own: None,
                language: String::new(),
            },
            (_, None) => Word {
                card_id: card.id,
                note_id: card.note_id,
                term: None,
                term_key: String::new(),
                prompt,
                answer: r.expected.clone(),
                own: None,
                language: String::new(),
            },
        };
        words.push(word);
    }
    // Distractor candidates: the session's words, then a sample of the other notes of the
    // same decks.
    let session_notes: HashSet<NoteId> = words.iter().map(|w| w.note_id).collect();
    let mut others: Vec<NoteId> = Vec::new();
    if !decks.is_empty() {
        let mut seen = HashSet::new();
        for c in v.reads.list_cards(&CardFilter {
            deck_ids: Some(decks),
            ..CardFilter::default()
        })? {
            if !session_notes.contains(&c.note_id) && seen.insert(c.note_id) {
                others.push(c.note_id);
            }
        }
        others.sort();
        shuffle(&mut others, rng);
        others.truncate(POOL_NOTES);
    }
    let all_notes: Vec<NoteId> = words.iter().map(|w| w.note_id).chain(others.iter().copied()).collect();
    let langs = languages(v.reads, &all_notes)?;
    for w in &mut words {
        w.language = langs.get(&w.note_id).cloned().unwrap_or_default();
    }
    let mut candidates: Vec<Candidate> = words
        .iter()
        .filter(|w| w.own.is_none())
        .map(|w| Candidate {
            note_id: w.note_id,
            term_key: w.term_key.clone(),
            meaning: w.answer.clone(),
            language: w.language.clone(),
            roles: w.term.is_some(),
        })
        .collect();
    for id in others {
        let Some(note) = v.reads.get_note(id)? else {
            continue;
        };
        if note.deleted_at.is_some() {
            continue;
        }
        let nt = types.get(note.notetype_id)?;
        if let Some((term, meaning)) = term_meaning(nt, &note) {
            candidates.push(Candidate {
                note_id: id,
                term_key: option_key(&term),
                meaning,
                language: langs.get(&id).cloned().unwrap_or_default(),
                roles: true,
            });
        }
    }
    // Every meaning of a word (all its notes): none of them is a wrong option for it.
    let mut meanings_of: HashMap<&str, HashSet<String>> = HashMap::new();
    for c in candidates.iter().filter(|c| c.roles) {
        meanings_of
            .entry(c.term_key.as_str())
            .or_default()
            .insert(option_key(&c.meaning));
    }
    let mut items = Vec::with_capacity(words.len());
    for w in &words {
        let opts = match &w.own {
            Some((own, correct)) => pure::own_options(own, *correct, rng),
            None => {
                let roles = w.term.is_some();
                let pool: Vec<String> = candidates
                    .iter()
                    .filter(|c| c.note_id != w.note_id && c.roles == roles)
                    // Another note of the same word holds one of its meanings, and so does
                    // any note meaning what one of its notes means.
                    .filter(|c| !roles || c.term_key != w.term_key)
                    .filter(|c| {
                        !roles
                            || !meanings_of
                                .get(w.term_key.as_str())
                                .is_some_and(|m| m.contains(&option_key(&c.meaning)))
                    })
                    // Same language when both are known.
                    .filter(|c| w.language.is_empty() || c.language.is_empty() || c.language == w.language)
                    .map(|c| c.meaning.clone())
                    .collect();
                pure::options(&w.answer, &pool, rng)
            }
        };
        if let Some((options, correct)) = opts {
            items.push(QuizItem {
                card_id: w.card_id,
                term: w.term.clone(),
                prompt: w.prompt.clone(),
                options,
                correct,
                picked: None,
            });
        }
    }
    if items.is_empty() {
        return Err(session_mode(
            "too_few_words",
            "a quiz needs at least two words with different meanings",
        ));
    }
    let asked: HashSet<CardId> = items.iter().map(|i| i.card_id).collect();
    state.queue.retain(|e| asked.contains(&e.card_id));
    state.modes = Some(ModesState {
        quiz: Some(QuizState {
            items,
            affects_schedule: affects,
        }),
        ..ModesState::default()
    });
    Ok(())
}

fn quiz_state(s: &Session) -> AppResult<&QuizState> {
    s.state
        .modes
        .as_ref()
        .and_then(|m| m.quiz.as_ref())
        .filter(|_| s.config.kind == StudyKind::Quiz)
        .ok_or_else(|| session_mode("not_quiz", "not a quiz session"))
}

/// The missed words of quiz session `s` (`study.start { kind: quiz, from_session }`).
pub(crate) fn missed_cards(s: &Session) -> AppResult<Vec<CardId>> {
    Ok(quiz_state(s)?.missed().map(|i| i.card_id).collect())
}

fn progress(q: &QuizState) -> QuizProgressDto {
    let c = q.counts();
    QuizProgressDto {
        total: c.total,
        answered: c.answered,
        correct: c.correct,
    }
}

/// `SessionDto.modes.quiz`.
pub(super) fn dto(s: &Session, shown: Option<&Card>) -> AppResult<QuizDto> {
    let q = quiz_state(s)?;
    let item = shown.and_then(|c| q.item(c.id)).filter(|i| i.picked.is_none());
    Ok(QuizDto {
        term: item.and_then(|i| i.term.clone()),
        options: item.map(|i| i.options.clone()).unwrap_or_default(),
        few_options: item.is_some_and(QuizItem::few_options),
        progress: progress(q),
        affects_schedule: q.affects_schedule,
    })
}

fn answer(req: QuizAnswer, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<QuizAnswerResult>> {
    let session = Session::load(ctx.reads(), ctx.registry, req.session_id)?;
    let quiz = quiz_state(&session)?.clone();
    session.require_open()?;
    session.require_available(req.card_id, ctx.now)?;
    let item = quiz
        .item(req.card_id)
        .filter(|i| i.picked.is_none())
        .ok_or_else(|| not_available(req.card_id))?;
    if req.choice as usize >= item.options.len() {
        return Err(crate::support::invalid("choice out of range"));
    }
    let correct = req.choice == item.correct;
    let (correct_option, expected) = (item.correct, item.expected().to_owned());
    let mut after = quiz.clone();
    let outcome = answer_core(
        ctx,
        &AnswerCard {
            session_id: req.session_id,
            card_id: req.card_id,
            rating: if correct { Rating::Good } else { Rating::Again },
            duration_ms: req.duration_ms,
            reveal_ms: None,
        },
        AnswerHooks {
            confidence: false,
            no_requeue: true,
            practice: !quiz.affects_schedule,
            after: Some(Box::new(move |state: &mut SessionState, card_id, _, _| {
                after.record(card_id, req.choice);
                if let Some(m) = state.modes.as_mut() {
                    m.quiz = Some(after);
                }
            })),
        },
    )?;
    Ok(CommandOutcome {
        response: QuizAnswerResult {
            correct,
            picked: req.choice,
            correct_option,
            expected,
            session: outcome.response,
        },
        inverse: outcome.inverse,
        summary: super::answer_summary::<ModesQuizAnswer>(&outcome.summary),
        events: outcome.events,
    })
}

fn result(req: QuizRef, ctx: &QueryCtx<'_>) -> AppResult<QuizResultDto> {
    let s = Session::load(ctx.tx, ctx.registry, req.session_id)?;
    let q = quiz_state(&s)?;
    let c = q.counts();
    Ok(QuizResultDto {
        session_id: s.record.id,
        total: c.total,
        answered: c.answered,
        correct: c.correct,
        wrong: c.wrong(),
        percent: c.percent(),
        finished: q.finished(),
        affects_schedule: q.affects_schedule,
        missed: q
            .missed()
            .map(|i| QuizMissDto {
                card_id: i.card_id,
                term: i.label().to_owned(),
                expected: i.expected().to_owned(),
                picked: i.picked.and_then(|p| i.options.get(p as usize).cloned()),
            })
            .collect(),
    })
}
