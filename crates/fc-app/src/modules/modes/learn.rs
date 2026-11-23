//! `learn` module: Quizlet-style learn sessions (docs/02 §6.1). `study.start { kind: learn }`
//! picks cards like a custom session (deck, query — e.g. `seviye:A2` — classification filter,
//! saved query); each card is asked as multiple choice, then written; `modes.learn.answer`
//! judges the answer, rates the card through the session engine (correct → good, almost →
//! hard, wrong → again; `review_log.mode = learn`, or `trial_learn` in practice mode) and
//! moves it between stages. A card is learned once correct at every stage. Recognition is not
//! recall: a multiple-choice answer on a card already in review/relearning is practice only
//! (schedule unchanged, `review_log.mode = trial_learn`); such cards are rated by the written
//! stage alone ([`pure::updates_schedule`]).

use super::{session_mode, setting};
use crate::error::AppResult;
use crate::handler::{CommandCtx, CommandOutcome};
use crate::module::{main_menu, ModuleDescriptor};
use crate::modules::study::ext::*;
use crate::registry::Registry;
use fc_api::v1::cardtypes::CardInteraction;
use fc_api::v1::methods::ModesLearnAnswer;
use fc_api::v1::modes::{LearnAnswer, LearnAnswerResult, LearnDto, LearnProgressDto, LearnStage};
use fc_api::v1::study::{AnswerCard, AnswerGrade, DiffKind, DiffSpanDto, StudyKind};
use fc_core::card::Card;
use fc_core::rng::{Rng, SplitMix64};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::CardId;
use fc_providers::strategy::DiffSpanKind;
use fc_study::answer::{check_accepting, CheckOptions, CheckerKind, Grade};
use fc_study::modes::learn::{self as pure, LearnState, CHOICE_OPTIONS};
use fc_study::modes::{choice_options, option_key, ModesState};
use fc_study::session::{QueueEntry, SessionState};
use fc_study::text::html_to_text;

/// Module id.
pub const ID: &str = "learn";
/// Most cards in a learn session without an explicit `filter.limit`.
pub const MAX_CARDS: &str = "learn.max_cards";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.learn.name",
        description_key: "modules.learn.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![SettingDef {
            key: MAX_CARDS.to_owned(),
            kind: SettingKind::Integer {
                min: Some(1),
                max: Some(500),
            },
            default: serde_json::Value::from(30),
            label_key: format!("settings.{MAX_CARDS}.label"),
            description_key: Some(format!("settings.{MAX_CARDS}.description")),
            advanced: false,
            scopes: vec![SettingScopeKind::App],
        }],
        ui: vec![main_menu("modes.learn", "nav.learn", "learn", "/learn", 6)],
    }
}

/// Registers the module.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.command::<ModesLearnAnswer>(answer);
}

/// What the session asks for a card: its answer text and, for choice card types, its own
/// options.
struct Asked {
    answer: String,
    own: Option<(Vec<String>, u32)>,
    /// v1.4 accepted answers of `answer` (offline): right when typed, never a wrong option.
    accepted: Vec<String>,
}

fn asked(v: View<'_>, card: &Card) -> AppResult<Asked> {
    let r = render_for_study(v, card)?;
    Ok(match &r.card.sides.interaction {
        CardInteraction::Choice { options, correct } => {
            let options: Vec<String> = options.iter().map(|o| html_to_text(o)).collect();
            Asked {
                answer: options.get(*correct as usize).cloned().unwrap_or_default(),
                own: Some((options, *correct)),
                accepted: Vec::new(),
            }
        }
        CardInteraction::SelfGrade | CardInteraction::Type { .. } => Asked {
            accepted: crate::modules::accepted_answers::alternatives_for(
                v.reads,
                v.registry,
                card.note_id,
                &r.expected,
            )?,
            answer: r.expected,
            own: None,
        },
    })
}

/// `study.start` hook: caps the set, makes every card available now and records the stage
/// each card starts at (written only when no distractor exists).
pub(super) fn init(v: View<'_>, state: &mut SessionState, _rng: &mut dyn Rng) -> AppResult<()> {
    let max = setting(v.reads, &SettingScope::App, MAX_CARDS, descriptor)?
        .as_u64()
        .unwrap_or(30) as usize;
    state.queue.truncate(max);
    let mut items = Vec::with_capacity(state.queue.len());
    for e in &mut state.queue {
        e.due = None;
        let card = require_live_card(v.reads, e.card_id)?;
        let a = asked(v, &card)?;
        items.push((e.card_id, a.answer, a.own.is_some()));
    }
    // Written-only: no other card offers a distinct answer (and no own options).
    let keys: Vec<String> = items.iter().map(|(_, a, _)| option_key(a)).collect();
    let items = items
        .iter()
        .enumerate()
        .map(|(i, (id, answer, own))| {
            let distinct = keys
                .iter()
                .enumerate()
                .any(|(j, k)| j != i && !k.is_empty() && *k != keys[i]);
            (*id, answer.clone(), !own && !distinct)
        })
        .collect::<Vec<_>>();
    state.modes = Some(ModesState {
        learn: Some(LearnState::new(items)),
        ..ModesState::default()
    });
    Ok(())
}

fn learn_state(s: &Session) -> AppResult<&LearnState> {
    s.state
        .modes
        .as_ref()
        .and_then(|m| m.learn.as_ref())
        .filter(|_| s.config.kind == StudyKind::Learn)
        .ok_or_else(|| session_mode("not_learn", "not a learn session"))
}

/// Options of `card`'s multiple-choice question: its own options, else distractors from the
/// other cards' answers. Stable for one showing (seeded by session, card and the number of
/// answers so far), so the answer handler sees the options the screen showed.
fn options(s: &Session, learn: &LearnState, card: CardId, a: &Asked) -> Option<(Vec<String>, u32)> {
    if let Some(own) = &a.own {
        return Some(own.clone());
    }
    let seed = s.record.id.0.as_u64_pair().1
        ^ card.0.as_u64_pair().1.rotate_left(17)
        ^ u64::from(s.state.answered).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut rng = SplitMix64::new(seed);
    let pool = fc_study::modes::without_accepted(&pure::pool(learn, card), &a.accepted);
    choice_options(&a.answer, &pool, CHOICE_OPTIONS, &mut rng)
}

fn stage_dto(s: pure::LearnStage) -> LearnStage {
    match s {
        pure::LearnStage::Choice => LearnStage::Choice,
        pure::LearnStage::Written => LearnStage::Written,
    }
}

/// `SessionDto.modes.learn`.
pub(super) fn dto(v: View<'_>, s: &Session, shown: Option<&Card>) -> AppResult<LearnDto> {
    let learn = learn_state(s)?;
    let c = learn.counts();
    let progress = LearnProgressDto {
        total: c.total,
        learned: c.learned,
        choice: c.choice,
        written: c.written,
    };
    let Some(card) = shown else {
        return Ok(LearnDto {
            stage: None,
            options: Vec::new(),
            mistakes: 0,
            progress,
        });
    };
    let item = learn.item(card.id);
    let stage = item.map_or(pure::LearnStage::Written, |i| i.stage);
    let mut opts = Vec::new();
    let mut stage_now = stage;
    if stage == pure::LearnStage::Choice {
        match options(s, learn, card.id, &asked(v, card)?) {
            Some((o, _)) => opts = o,
            // The pool changed (cards deleted): ask it written.
            None => stage_now = pure::LearnStage::Written,
        }
    }
    Ok(LearnDto {
        stage: Some(stage_dto(stage_now)),
        options: opts,
        mistakes: item.map_or(0, |i| i.mistakes),
        progress,
    })
}

fn grade_dto(g: Grade) -> AnswerGrade {
    match g {
        Grade::Correct => AnswerGrade::Correct,
        Grade::Almost => AnswerGrade::Almost,
        Grade::Wrong => AnswerGrade::Wrong,
    }
}

fn answer(req: LearnAnswer, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<LearnAnswerResult>> {
    let session = Session::load(ctx.reads(), ctx.registry, req.session_id)?;
    let learn = learn_state(&session)?.clone();
    session.require_open()?;
    session.require_available(req.card_id, ctx.now)?;
    let item = learn
        .item(req.card_id)
        .filter(|i| !i.learned)
        .ok_or_else(|| not_available(req.card_id))?;
    let card = require_live_card(ctx.reads(), req.card_id)?;
    let a = asked(View::cmd(ctx), &card)?;
    let choice_opts = match item.stage {
        pure::LearnStage::Choice => options(&session, &learn, card.id, &a),
        pure::LearnStage::Written => None,
    };
    // Asked as shown: multiple choice when it has options, else written.
    let (stage, grade, expected, diff) = match choice_opts {
        Some((opts, correct)) => {
            let picked = req
                .choice
                .ok_or_else(|| session_mode("answer", "pick an option (multiple-choice stage)"))?;
            if picked as usize >= opts.len() {
                return Err(crate::support::invalid("choice out of range"));
            }
            let grade = if picked == correct {
                Grade::Correct
            } else {
                Grade::Wrong
            };
            let expected = opts.get(correct as usize).cloned().unwrap_or_default();
            (pure::LearnStage::Choice, grade, expected, Vec::new())
        }
        None => {
            let given = req
                .given
                .clone()
                .ok_or_else(|| session_mode("answer", "type the answer (written stage)"))?;
            let kind = CheckerKind::parse(&setting_str(ctx.reads(), &SettingScope::App, ANSWER_CHECKER)?)
                .unwrap_or(CheckerKind::Multi);
            let opts = CheckOptions {
                fold_turkish: setting_bool(ctx.reads(), &SettingScope::App, FOLD_TURKISH)?,
                ..CheckOptions::default()
            };
            let c = check_accepting(kind, &a.answer, &a.accepted, &given, &opts);
            let diff = c
                .diff
                .into_iter()
                .map(|d| DiffSpanDto {
                    kind: match d.kind {
                        DiffSpanKind::Same => DiffKind::Same,
                        DiffSpanKind::Missing => DiffKind::Missing,
                        DiffSpanKind::Extra => DiffKind::Extra,
                    },
                    text: d.text,
                })
                .collect();
            (pure::LearnStage::Written, c.grade, c.matched, diff)
        }
    };
    // A card asked written because it had no options counts as passing the choice stage.
    let mut probe = learn.clone();
    if stage == pure::LearnStage::Written {
        if let Some(i) = probe.items.iter_mut().find(|i| i.card_id == card.id) {
            i.stage = pure::LearnStage::Written;
        }
    }
    let practice = !pure::updates_schedule(stage, session.scheduling_of(&card).state);
    let mut after_state = probe.clone();
    let step = probe
        .record(card.id, grade)
        .ok_or_else(|| not_available(card.id))?;
    let outcome = answer_core(
        ctx,
        &AnswerCard {
            session_id: req.session_id,
            card_id: req.card_id,
            rating: pure::rating(grade),
            duration_ms: req.duration_ms,
            reveal_ms: None,
        },
        AnswerHooks {
            confidence: false,
            no_requeue: true,
            practice,
            after: Some(Box::new(move |state: &mut SessionState, card_id, note_id, kind| {
                after_state.record(card_id, grade);
                let learned = after_state.item(card_id).is_some_and(|i| i.learned);
                if let Some(m) = state.modes.as_mut() {
                    m.learn = Some(after_state);
                }
                if !learned {
                    let at = pure::requeue_index(state.queue.len());
                    state.queue.insert(
                        at,
                        QueueEntry {
                            card_id,
                            note_id,
                            kind,
                            due: None,
                        },
                    );
                }
            })),
        },
    )?;
    Ok(CommandOutcome {
        response: LearnAnswerResult {
            grade: grade_dto(grade),
            stage: stage_dto(stage),
            learned: step.learned,
            expected,
            diff,
            session: outcome.response,
        },
        inverse: outcome.inverse,
        summary: super::answer_summary::<ModesLearnAnswer>(&outcome.summary),
        events: outcome.events,
    })
}
