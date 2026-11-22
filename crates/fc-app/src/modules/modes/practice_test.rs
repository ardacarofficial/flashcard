//! `practice_test` module: test papers (docs/02 §6.1). `study.start { kind: test }` picks the
//! cards like a custom session (deck, query — e.g. `sınav:TYT` — classification filter, saved
//! query) and fixes a paper of mixed question types (multiple choice, written, true/false)
//! with its answer key in the session. `modes.test.paper` returns it (printable; the key only
//! after submission or on request), `modes.test.submit` grades it, logs one review per question
//! (`review_log.mode = test`) and never changes the schedule. Submission is one undo step.
//! v1.3: `modes.test.save_draft` keeps the answers of an open paper, so an unfinished test is
//! continued from `study.options.resumable` with its answers.

use super::{session_mode, setting};
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{main_menu, ModuleDescriptor};
use crate::modules::study::ext::*;
use crate::registry::Registry;
use crate::support::summary;
use fc_api::v1::cardtypes::CardInteraction;
use fc_api::v1::methods::{ModesTestPaper, ModesTestSaveDraft, ModesTestSubmit, StudyRestore};
use fc_api::v1::modes::*;
use fc_api::v1::study::{RestoreStudy, StartSession, StudyKind};
use fc_api::{ApiError, ErrorCode};
use fc_core::rng::Rng;
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::ReviewLogId;
use fc_storage::repo::ReviewLogRecord;
use fc_study::answer::{check, CheckOptions, CheckerKind};
use fc_study::modes::test::{self as pure, Question, QuestionKind, Response, TestSource, TestState, Verdict};
use fc_study::modes::ModesState;
use fc_study::session::{self as queue, SessionState};
use fc_study::text::html_to_text;

/// Module id.
pub const ID: &str = "practice_test";
/// Default number of questions.
pub const QUESTIONS: &str = "practice_test.questions";
/// `review_log.mode` of test answers.
pub const REVIEW_MODE: &str = "test";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.practice_test.name",
        description_key: "modules.practice_test.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![SettingDef {
            key: QUESTIONS.to_owned(),
            kind: SettingKind::Integer {
                min: Some(1),
                max: Some(pure::MAX_QUESTIONS as i64),
            },
            default: serde_json::Value::from(20),
            label_key: format!("settings.{QUESTIONS}.label"),
            description_key: Some(format!("settings.{QUESTIONS}.description")),
            advanced: false,
            scopes: vec![SettingScopeKind::App],
        }],
        ui: vec![main_menu("modes.test", "nav.test", "test", "/test", 7)],
    }
}

/// Registers the module.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<ModesTestPaper>(paper);
    reg.command::<ModesTestSubmit>(submit);
    reg.command::<ModesTestSaveDraft>(save_draft);
}

fn kind_of(k: TestQuestionKind) -> QuestionKind {
    match k {
        TestQuestionKind::MultipleChoice => QuestionKind::MultipleChoice,
        TestQuestionKind::Written => QuestionKind::Written,
        TestQuestionKind::TrueFalse => QuestionKind::TrueFalse,
    }
}

fn kind_dto(k: QuestionKind) -> TestQuestionKind {
    match k {
        QuestionKind::MultipleChoice => TestQuestionKind::MultipleChoice,
        QuestionKind::Written => TestQuestionKind::Written,
        QuestionKind::TrueFalse => TestQuestionKind::TrueFalse,
    }
}

/// `study.start` hook: builds the paper from the queued cards; the queue is emptied (a test
/// is answered as a whole, not card by card).
pub(super) fn init(
    v: View<'_>,
    req: &StartSession,
    state: &mut SessionState,
    rng: &mut dyn Rng,
) -> AppResult<()> {
    let opts = req
        .modes
        .as_ref()
        .and_then(|m| m.test.clone())
        .unwrap_or_default();
    let count = match opts.questions {
        Some(n) => n,
        None => setting(v.reads, &SettingScope::App, QUESTIONS, descriptor)?
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(20),
    };
    if count == 0 || count as usize > pure::MAX_QUESTIONS {
        return Err(crate::support::invalid(format!(
            "test.questions must be 1–{}",
            pure::MAX_QUESTIONS
        )));
    }
    let kinds: Vec<QuestionKind> = opts
        .kinds
        .unwrap_or_default()
        .into_iter()
        .map(kind_of)
        .collect();
    let mut sources = Vec::with_capacity(state.queue.len());
    for e in &state.queue {
        let card = require_live_card(v.reads, e.card_id)?;
        let r = render_for_study(v, &card)?;
        let (answer, own) = match &r.card.sides.interaction {
            CardInteraction::Choice { options, correct } => {
                let options: Vec<String> = options.iter().map(|o| html_to_text(o)).collect();
                (
                    options.get(*correct as usize).cloned().unwrap_or_default(),
                    Some((options, *correct)),
                )
            }
            CardInteraction::SelfGrade | CardInteraction::Type { .. } => (r.expected.clone(), None),
        };
        if answer.trim().is_empty() {
            continue;
        }
        sources.push(TestSource {
            card_id: card.id,
            prompt_html: r.card.sides.question.clone(),
            answer,
            own_options: own,
        });
    }
    let questions = pure::generate(&sources, &kinds, count as usize, rng);
    state.queue.clear();
    state.modes = Some(ModesState {
        test: Some(TestState {
            questions,
            graded: None,
            draft: Vec::new(),
        }),
        ..ModesState::default()
    });
    Ok(())
}

/// `SessionDto.modes.test`.
pub(super) fn info(t: &TestState) -> TestInfoDto {
    TestInfoDto {
        questions: u32::try_from(t.questions.len()).unwrap_or(u32::MAX),
        submitted: t.graded.is_some(),
        percent: t.graded.as_ref().map(|g| g.percent),
    }
}

fn test_state(s: &Session) -> AppResult<&TestState> {
    s.state
        .modes
        .as_ref()
        .and_then(|m| m.test.as_ref())
        .filter(|_| s.config.kind == StudyKind::Test)
        .ok_or_else(|| session_mode("not_test", "not a test session"))
}

fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

fn question_dto(i: usize, q: &Question, key: bool) -> TestQuestionDto {
    TestQuestionDto {
        index: index(i),
        card_id: q.card_id,
        kind: kind_dto(q.kind),
        prompt_html: q.prompt_html.clone(),
        options: q.options.clone(),
        statement: q.statement.clone(),
        key: key.then(|| TestKeyDto {
            option: q.key.option,
            truth: q.key.truth,
            expected: q.key.expected.clone(),
        }),
    }
}

/// Answers in paper order as DTOs (blank ones included: index order is the paper's).
fn answers_dto(responses: &[Response]) -> Vec<TestAnswerDto> {
    responses
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut a = TestAnswerDto {
                index: index(i),
                ..TestAnswerDto::default()
            };
            match r {
                Response::Choice(c) => a.choice = Some(*c),
                Response::Text(t) => a.text = Some(t.clone()),
                Response::Truth(b) => a.truth = Some(*b),
                Response::Blank => {}
            }
            a
        })
        .collect()
}

/// v1.3 resume: `(answered, remaining)` questions of an unsubmitted paper (answered = not
/// blank on the saved draft); `None` once submitted (nothing to continue).
pub(crate) fn progress(t: &TestState) -> Option<(u32, u32)> {
    if t.graded.is_some() {
        return None;
    }
    let answered = t.draft.iter().filter(|r| !is_blank(r)).count();
    Some((index(answered), index(t.questions.len().saturating_sub(answered))))
}

fn is_blank(r: &Response) -> bool {
    match r {
        Response::Blank => true,
        Response::Text(t) => t.trim().is_empty(),
        Response::Choice(_) | Response::Truth(_) => false,
    }
}

fn paper_dto(s: &Session, t: &TestState, with_key: bool) -> TestPaperDto {
    let key = with_key || t.graded.is_some();
    TestPaperDto {
        session_id: s.record.id,
        questions: t
            .questions
            .iter()
            .enumerate()
            .map(|(i, q)| question_dto(i, q, key))
            .collect(),
        answers: t.graded.as_ref().map(|g| answers_dto(&g.responses)),
        draft: match t.graded {
            Some(_) => None,
            None => Some(answers_dto(&t.draft)),
        },
        result: t.graded.as_ref().map(|g| {
            let count = |v: Verdict| index(g.verdicts.iter().filter(|x| **x == v).count());
            TestResultDto {
                total: index(g.verdicts.len()),
                correct: count(Verdict::Correct),
                almost: count(Verdict::Almost),
                wrong: count(Verdict::Wrong),
                blank: count(Verdict::Blank),
                points: g.points,
                percent: g.percent,
                items: g
                    .verdicts
                    .iter()
                    .enumerate()
                    .map(|(i, v)| TestItemDto {
                        index: index(i),
                        verdict: match v {
                            Verdict::Correct => TestVerdict::Correct,
                            Verdict::Almost => TestVerdict::Almost,
                            Verdict::Wrong => TestVerdict::Wrong,
                            Verdict::Blank => TestVerdict::Blank,
                        },
                    })
                    .collect(),
            }
        }),
    }
}

fn paper(req: TestPaperRequest, ctx: &QueryCtx<'_>) -> AppResult<TestPaperDto> {
    let s = Session::load(ctx.tx, ctx.registry, req.session_id)?;
    let t = test_state(&s)?;
    Ok(paper_dto(&s, t, req.with_key.unwrap_or(false)))
}

/// The learner's answer to question `q`, by its type (other fields are ignored).
fn response(q: &Question, a: Option<&TestAnswerDto>) -> Response {
    let Some(a) = a else {
        return Response::Blank;
    };
    let r = match q.kind {
        QuestionKind::MultipleChoice => a.choice.map(Response::Choice),
        QuestionKind::TrueFalse => a.truth.map(Response::Truth),
        QuestionKind::Written => a.text.clone().map(Response::Text),
    };
    r.unwrap_or(Response::Blank)
}

/// `modes.test.save_draft` (v1.3): keeps the answers of an open paper. Session bookkeeping
/// like `study.hint` (no undo step: the answers count only when the paper is submitted).
fn save_draft(req: SaveTestDraft, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<TestPaperDto>> {
    let mut session = Session::load(ctx.reads(), ctx.registry, req.session_id)?;
    session.require_open()?;
    let t = test_state(&session)?;
    if t.graded.is_some() {
        return Err(ApiError::new(ErrorCode::ConflictTestSubmitted, "the test was submitted already")
            .with_detail("id", session.record.id.to_string())
            .into());
    }
    if let Some(a) = req.answers.iter().find(|a| a.index as usize >= t.questions.len()) {
        return Err(crate::support::invalid(format!(
            "answer index {} is outside the paper ({} questions)",
            a.index,
            t.questions.len()
        )));
    }
    let draft: Vec<Response> = t
        .questions
        .iter()
        .enumerate()
        .map(|(i, q)| response(q, req.answers.iter().find(|a| a.index as usize == i)))
        .collect();
    if let Some(t) = session.state.modes.as_mut().and_then(|m| m.test.as_mut()) {
        t.draft = draft;
    }
    session.save(ctx.tx)?;
    let t = test_state(&session)?;
    Ok(CommandOutcome {
        response: paper_dto(&session, t, false),
        inverse: Vec::new(),
        summary: summary::<ModesTestSaveDraft>(),
        events: Vec::new(),
    })
}

fn submit(req: SubmitTest, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<TestPaperDto>> {
    let mut session = Session::load(ctx.reads(), ctx.registry, req.session_id)?;
    session.require_open()?;
    let t = test_state(&session)?.clone();
    if t.graded.is_some() {
        return Err(ApiError::new(ErrorCode::ConflictTestSubmitted, "the test was submitted already")
            .with_detail("id", session.record.id.to_string())
            .into());
    }
    let responses: Vec<Response> = t
        .questions
        .iter()
        .enumerate()
        .map(|(i, q)| response(q, req.answers.iter().find(|a| a.index as usize == i)))
        .collect();
    let kind = CheckerKind::parse(&setting_str(ctx.reads(), &SettingScope::App, ANSWER_CHECKER)?)
        .unwrap_or(CheckerKind::Multi);
    let opts = CheckOptions {
        fold_turkish: setting_bool(ctx.reads(), &SettingScope::App, FOLD_TURKISH)?,
        ..CheckOptions::default()
    };
    let graded = pure::grade(&t.questions, &responses, |e, g| check(kind, e, g, &opts).grade);
    // One review per question on a live card: the card's state as it is (a test never
    // changes the schedule).
    let duration = queue::clamp_duration(req.duration_ms, session.state.last_event_at, ctx.now);
    let per = duration / u32::try_from(t.questions.len().max(1)).unwrap_or(u32::MAX);
    let sched = scheduler(ctx.reads())?;
    let mut written = Vec::new();
    for (q, v) in t.questions.iter().zip(&graded.verdicts) {
        let Some(card) = ctx.reads().get_card(q.card_id)? else {
            continue;
        };
        if card.deleted_at.is_some() {
            continue;
        }
        let params = session.config.params(ctx.reads(), card.deck_id)?;
        let elapsed = sched
            .schedule(&card.scheduling, v.rating(), ctx.now, &params)
            .map_err(scheduler_error)?
            .elapsed_days;
        let review = ReviewLogRecord {
            id: ReviewLogId::generate(ctx.ids),
            card_id: card.id,
            session_id: Some(session.record.id),
            rating: v.rating(),
            state_before: card.scheduling.state,
            state_after: card.scheduling.state,
            scheduled_days: 0,
            elapsed_days: elapsed,
            duration_ms: per,
            answer_duration_ms: None,
            hints_used: 0,
            mode: REVIEW_MODE.to_owned(),
            stability_after: card.scheduling.stability,
            difficulty_after: card.scheduling.difficulty,
            created_at: ctx.now,
        };
        ctx.tx.insert_review_log(&review)?;
        written.push(review.id);
    }
    let n = index(graded.verdicts.len());
    let correct = index(graded.verdicts.iter().filter(|v| **v == Verdict::Correct).count());
    let percent = graded.percent;
    session.state.answered = session.state.answered.saturating_add(n);
    session.state.correct = session.state.correct.saturating_add(correct);
    session.state.time_ms = session.state.time_ms.saturating_add(u64::from(duration));
    session.state.last_event_at = Some(ctx.now);
    if let Some(m) = session.state.modes.as_mut() {
        if let Some(t) = m.test.as_mut() {
            t.graded = Some(graded);
        }
    }
    let before = session.save(ctx.tx)?.snapshot()?;
    let t = test_state(&session)?;
    Ok(CommandOutcome {
        response: paper_dto(&session, t, true),
        inverse: vec![inverse_call::<StudyRestore>(&RestoreStudy {
            cards: Vec::new(),
            session: Some(before),
            remove_reviews: written,
            add_reviews: Vec::new(),
        })?],
        summary: summary::<ModesTestSubmit>().param("percent", percent),
        events: Vec::new(),
    })
}
