//! v1.4 (W3, docs/02 §17.4): **repairing a leech** instead of only setting it aside.
//!
//! * `stats.leech_repairs.plan`: the evidence — the latest failed reviews (time, hints; typed
//!   answers are not kept by the study flow), the example on the card and its score
//!   (`examples` module), the other encounters ranked, the meaning against the accepted
//!   answers, other notes of the same word (confusable) — and previewed proposals. Each
//!   proposal is one **existing** undoable command: `subtitle.encounters.set_primary`,
//!   `accepted_answers.add`, `subtitle.encounters.move`, or the AI rewrite task
//!   (`ai.preview` → `ai.run` with `rewrite_leech`, applied with `ai.notes.update`).
//! * `stats.leech_repairs.apply`: runs the chosen command nested (its module must be on; the nested
//!   call checks that) and records which repair was applied (`leech_repairs`, migration 1470).
//!   One undo step: the nested command's own inverse plus the removal of the record.
//! * `stats.leech_repairs.list`: per repair, the pass rate of the card's last scheduled reviews
//!   before it against the ones since (until the card's next repair), with the honest caveat
//!   `small_sample` while fewer than [`SMALL_SAMPLE`] reviews came after.

use super::*;
use crate::handler::inverse_call;
use crate::modules::ai::context_meaning_field;
use crate::modules::{accepted_answers, examples};
use crate::support::notes::{require_live_note, require_notetype};
use fc_api::v1::accepted_answers::EditAcceptedAnswers;
use fc_api::v1::ai::{AiTask, PreviewAi, UpdateAiNote};
use fc_api::v1::common::Empty;
use fc_api::v1::examples::RankedExamples;
use fc_api::v1::subtitle::{MoveSubtitleEncounter, SubtitleEncounterRef};
use fc_api::{ApiError, ApiMethod};
use fc_core::card::{Card, Rating};
use fc_core::subtitle::{same_meaning, senses};
use fc_core::{CardId, NoteId};
use fc_storage::repo::{is_practice_mode, LeechRepairRecord, ReviewLogRecord};
use fc_storage::Reads;
use fc_study::text::html_to_text;

/// Failed reviews shown as evidence.
const EVIDENCE: usize = 5;
/// Scheduled reviews before a repair that the comparison uses.
const BEFORE_WINDOW: usize = 10;
/// Reviews after a repair listed.
const AFTER_LIST: usize = 20;
/// Fewer reviews after a repair than this: "n is small".
pub const SMALL_SAMPLE: u32 = 10;
/// Most "move this meaning" proposals.
const MAX_MOVES: usize = 3;

pub(super) fn register(reg: &mut Registry) {
    reg.query::<StatsLeechesRepairPlan>(plan);
    reg.command::<StatsLeechesRepair>(repair);
    reg.query::<StatsLeechesRepairs>(repairs);
    reg.command::<StatsLeechesRepairsRestore>(restore);
}

fn invalid(reason: &str, message: impl Into<String>) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationInvalidParams, message).with_detail("reason", reason),
    )
}

fn require_card(reads: &dyn Reads, id: CardId) -> AppResult<Card> {
    reads
        .get_card(id)?
        .filter(|c| c.deleted_at.is_none())
        .ok_or_else(|| {
            AppError::Api(
                ApiError::new(ErrorCode::NotFoundCard, format!("card {id} not found"))
                    .with_detail("id", id.to_string()),
            )
        })
}

/// The note's meaning field and its plain text.
fn meaning_of(reads: &dyn Reads, note: NoteId) -> AppResult<(Option<String>, String)> {
    let n = require_live_note(reads, note)?;
    let nt = require_notetype(reads, n.notetype_id)?;
    let field = context_meaning_field(&nt);
    let text = field
        .as_deref()
        .and_then(|f| n.fields.iter().find(|(k, _)| k.0 == f))
        .map(|(_, v)| html_to_text(v))
        .unwrap_or_default();
    Ok((field, text.trim().to_owned()))
}

fn accepted_of(reads: &dyn Reads, note: NoteId, field: Option<&str>) -> AppResult<Vec<String>> {
    Ok(accepted_answers::lists(reads, note)?
        .into_iter()
        .filter(|l| Some(l.field.as_str()) == field)
        .flat_map(|l| l.answers)
        .collect())
}

fn dto_of(r: LeechRepairRecord) -> AppResult<LeechRepairDto> {
    let repair: LeechRepair = serde_json::from_str(&r.detail)
        .map_err(|e| AppError::api(ErrorCode::Internal, format!("leech repair {}: {e}", r.id)))?;
    Ok(LeechRepairDto {
        id: r.id,
        card_id: r.card_id,
        note_id: r.note_id,
        kind: repair.kind(),
        repair,
        lapses: r.lapses,
        applied_at: r.applied_at,
    })
}

fn record_of(d: &LeechRepairDto) -> AppResult<LeechRepairRecord> {
    Ok(LeechRepairRecord {
        id: d.id,
        card_id: d.card_id,
        note_id: d.note_id,
        kind: d.kind.as_str().to_owned(),
        detail: serde_json::to_string(&d.repair)
            .map_err(|e| AppError::api(ErrorCode::Internal, format!("leech repair: {e}")))?,
        lapses: d.lapses,
        applied_at: d.applied_at,
    })
}

/// Other live notes of the same word (word index), with their meanings.
fn confusables(reads: &dyn Reads, note: NoteId) -> AppResult<Vec<LeechConfusable>> {
    let Some(w) = reads
        .word_index_notes_by_id(&[note])?
        .into_iter()
        .find(|w| !w.key.is_empty())
    else {
        return Ok(vec![]);
    };
    let mut out = Vec::new();
    for n in reads.word_index_notes_of(&w.language, std::slice::from_ref(&w.key))? {
        if n.note_id == note || out.iter().any(|c: &LeechConfusable| c.note_id == n.note_id) {
            continue;
        }
        if let Ok((_, meaning)) = meaning_of(reads, n.note_id) {
            out.push(LeechConfusable {
                note_id: n.note_id,
                meaning,
            });
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ plan

fn plan(req: LeechCardRef, ctx: &QueryCtx<'_>) -> AppResult<LeechRepairPlan> {
    let reads = ctx.tx;
    let card = require_card(reads, req.card_id)?;
    let note = card.note_id;
    let threshold = leech_threshold(reads)?;
    let lapses = card.scheduling.lapses;
    let mut logs = reads.review_logs_for_cards(&[card.id])?;
    logs.sort_by_key(|l| std::cmp::Reverse((l.created_at, l.id)));
    let failed_reviews = logs
        .iter()
        .filter(|l| l.rating == Rating::Again)
        .take(EVIDENCE)
        .map(|l| LeechFailedReview {
            at: l.created_at,
            mode: l.mode.clone(),
            duration_ms: l.duration_ms,
            answer_duration_ms: l.answer_duration_ms,
            hints_used: l.hints_used,
            typed_answer: None,
        })
        .collect();
    let (answer_field, answer) = meaning_of(reads, note)?;
    let note_senses = senses(&answer);
    let accepted = accepted_of(reads, note, answer_field.as_deref())?;
    let with_examples = crate::support::module_enabled(reads, ctx.registry, examples::ID)?;
    let ranked: Option<RankedExamples> = if with_examples
        && !reads.subtitle_encounters_of_note(note)?.is_empty()
    {
        Some(examples::rank_note(reads, ctx.registry, note)?)
    } else {
        None
    };
    let confusables = confusables(reads, note)?;
    let mut proposals = Vec::new();
    // 1. A clearly better example.
    if with_examples && ranked.is_some() {
        let s = examples::suggestion(reads, ctx.registry, note)?;
        if let (Some(best), Some(cur)) = (&s.suggested, &s.current) {
            let sentence = |id: &str| {
                s.ranking
                    .items
                    .iter()
                    .find(|i| i.id == id)
                    .map(|i| i.sentence.clone())
                    .unwrap_or_default()
            };
            proposals.push(LeechRepairProposal {
                repair: LeechRepair::SwitchExample {
                    capture_id: best.clone(),
                },
                method: SubtitleEncountersSetPrimary::NAME.to_owned(),
                why: LeechRepairWhy::BetterExample,
                preview: vec![LeechFieldChange {
                    field: "sentence".into(),
                    before: sentence(cur),
                    after: sentence(best),
                }],
                ai_preview: None,
            });
        }
    }
    // 2. Encounters captured with another meaning: move them out.
    let encounters = reads.subtitle_encounters_of_note(note)?;
    if encounters.len() > 1 && !answer.is_empty() {
        for e in encounters
            .iter()
            .filter(|e| {
                e.facts
                    .definition
                    .as_deref()
                    .is_some_and(|d| !d.trim().is_empty() && !same_meaning(d, &answer))
            })
            .take(MAX_MOVES)
        {
            let def = e.facts.definition.clone().unwrap_or_default();
            let to = confusables
                .iter()
                .find(|c| same_meaning(&def, &c.meaning))
                .map(|c| c.note_id);
            proposals.push(LeechRepairProposal {
                repair: LeechRepair::MoveMeaning {
                    capture_id: e.capture_id.clone(),
                    to_note_id: to,
                },
                method: SubtitleEncountersMove::NAME.to_owned(),
                why: LeechRepairWhy::MixedMeanings,
                preview: vec![LeechFieldChange {
                    field: "sentence".into(),
                    before: e.facts.sentence.clone().unwrap_or_default(),
                    after: String::new(),
                }],
                ai_preview: None,
            });
        }
    }
    // 3./4. Answers the written check should also take.
    let with_accepted = crate::support::module_enabled(reads, ctx.registry, accepted_answers::ID)?;
    if with_accepted && answer_field.is_some() {
        let fold = accepted_answers::fold;
        let missing = |list: &[String]| -> Vec<String> {
            let mut out: Vec<String> = Vec::new();
            for a in list {
                let a = a.trim();
                if a.is_empty()
                    || fold(a) == fold(&answer)
                    || accepted.iter().chain(out.iter()).any(|x| fold(x) == fold(a))
                {
                    continue;
                }
                out.push(a.to_owned());
            }
            out
        };
        let mut add = |answers: Vec<String>, why| {
            if answers.is_empty() {
                return;
            }
            let mut after = accepted.clone();
            after.extend(answers.iter().cloned());
            proposals.push(LeechRepairProposal {
                repair: LeechRepair::AddAcceptedAnswers {
                    field: answer_field.clone(),
                    answers,
                },
                method: AcceptedAnswersAdd::NAME.to_owned(),
                why,
                preview: vec![LeechFieldChange {
                    field: "accepted_answers".into(),
                    before: accepted.join("; "),
                    after: after.join("; "),
                }],
                ai_preview: None,
            });
        };
        if note_senses.len() > 1 {
            add(missing(&note_senses), LeechRepairWhy::SensesNotAccepted);
        }
        if let Some(m) = reads.get_note_meaning(note)? {
            add(missing(&m.also_correct), LeechRepairWhy::AlsoCorrectNotAccepted);
        }
    }
    // 5. A fresh angle: the AI rewrite task (previewed before anything leaves).
    if crate::support::module_enabled(reads, ctx.registry, crate::modules::ai::ID)? {
        proposals.push(LeechRepairProposal {
            repair: LeechRepair::AiRewrite {
                fields: Default::default(),
            },
            method: AiNotesUpdate::NAME.to_owned(),
            why: LeechRepairWhy::FreshAngle,
            preview: vec![],
            ai_preview: Some(PreviewAi {
                task: AiTask::RewriteLeech { note_id: note },
                provider_id: None,
                model: None,
            }),
        });
    }
    let repairs = reads
        .list_leech_repairs(Some(&[card.id]))?
        .into_iter()
        .map(dto_of)
        .collect::<AppResult<_>>()?;
    Ok(LeechRepairPlan {
        card_id: card.id,
        note_id: note,
        lapses,
        threshold,
        is_leech: lapses >= threshold,
        failed_reviews,
        typed_answers_recorded: false,
        answer_field,
        answer,
        senses: note_senses,
        accepted_answers: accepted,
        examples: ranked,
        confusables,
        proposals,
        repairs,
    })
}

// ------------------------------------------------------------------ apply

fn encounter_of_note(reads: &dyn Reads, capture_id: &str, note: NoteId) -> AppResult<()> {
    match reads.get_subtitle_capture(capture_id)? {
        Some(r) if r.note_id == note => Ok(()),
        _ => Err(invalid(
            "capture_id",
            format!("encounter {capture_id} is not an example of this card's note"),
        )),
    }
}

fn repair(
    req: ApplyLeechRepair,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<LeechRepairDto>> {
    let card = require_card(ctx.reads(), req.card_id)?;
    let note = card.note_id;
    require_live_note(ctx.reads(), note)?;
    let call = match &req.repair {
        LeechRepair::SwitchExample { capture_id } => {
            encounter_of_note(ctx.reads(), capture_id, note)?;
            inverse_call::<SubtitleEncountersSetPrimary>(&SubtitleEncounterRef {
                capture_id: capture_id.clone(),
            })?
        }
        LeechRepair::MoveMeaning {
            capture_id,
            to_note_id,
        } => {
            encounter_of_note(ctx.reads(), capture_id, note)?;
            inverse_call::<SubtitleEncountersMove>(&MoveSubtitleEncounter {
                capture_id: capture_id.clone(),
                to_note_id: *to_note_id,
            })?
        }
        LeechRepair::AddAcceptedAnswers { field, answers } => {
            if answers.iter().all(|a| a.trim().is_empty()) {
                return Err(invalid("answers", "no answer to add"));
            }
            inverse_call::<AcceptedAnswersAdd>(&EditAcceptedAnswers {
                note_id: note,
                field: field.clone(),
                answers: answers.clone(),
            })?
        }
        LeechRepair::AiRewrite { fields } => {
            if fields.is_empty() {
                return Err(invalid(
                    "fields",
                    "run the AI rewrite (ai.preview → ai.run) and pass the reviewed fields",
                ));
            }
            inverse_call::<AiNotesUpdate>(&UpdateAiNote {
                note_id: note,
                fields: fields.clone(),
            })?
        }
    };
    // The nested command checks its own module and validation; it is part of this undo step.
    ctx.call(&call)?;
    let mut dto = LeechRepairDto {
        id: 0,
        card_id: card.id,
        note_id: note,
        kind: req.repair.kind(),
        repair: req.repair,
        lapses: card.scheduling.lapses,
        applied_at: ctx.now,
    };
    dto.id = ctx.tx.insert_leech_repair(&record_of(&dto)?)?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<StatsLeechesRepairsRestore>(&RestoreLeechRepairs {
            remove: vec![dto.id],
            put: vec![],
        })?],
        summary: summary::<StatsLeechesRepair>().param("kind", dto.kind.as_str()),
        response: dto,
        events: Vec::new(),
    })
}

fn restore(
    req: RestoreLeechRepairs,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<Empty>> {
    let mut back = RestoreLeechRepairs::default();
    for id in req.remove {
        if let Some(r) = ctx.reads().get_leech_repair(id)? {
            back.put.push(dto_of(r)?);
            ctx.tx.delete_leech_repair(id)?;
        }
    }
    for d in req.put {
        let id = ctx.tx.insert_leech_repair(&record_of(&d)?)?;
        back.remove.push(id);
    }
    Ok(CommandOutcome {
        response: Empty::default(),
        inverse: vec![inverse_call::<StatsLeechesRepairsRestore>(&back)?],
        summary: summary::<StatsLeechesRepairsRestore>(),
        events: Vec::new(),
    })
}

// ------------------------------------------------------------------ report

fn pass_rate(logs: &[&ReviewLogRecord]) -> LeechPassRate {
    let reviews = u32::try_from(logs.len()).unwrap_or(u32::MAX);
    let passed = u32::try_from(logs.iter().filter(|l| l.rating != Rating::Again).count())
        .unwrap_or(u32::MAX);
    LeechPassRate {
        reviews,
        passed,
        rate: (reviews > 0).then(|| f64::from(passed) / f64::from(reviews)),
    }
}

fn repairs(req: LeechRepairsRequest, ctx: &QueryCtx<'_>) -> AppResult<LeechRepairReport> {
    let reads = ctx.tx;
    let filter = req.card_id.map(|c| vec![c]);
    let all = reads.list_leech_repairs(filter.as_deref())?;
    let mut cards: Vec<CardId> = all.iter().map(|r| r.card_id).collect();
    cards.sort_unstable();
    cards.dedup();
    let mut logs_of: HashMap<CardId, Vec<ReviewLogRecord>> = HashMap::new();
    for chunk in cards.chunks(500) {
        for l in reads.review_logs_for_cards(chunk)? {
            if !is_practice_mode(&l.mode) {
                logs_of.entry(l.card_id).or_default().push(l);
            }
        }
    }
    for v in logs_of.values_mut() {
        v.sort_by_key(|l| (l.created_at, l.id));
    }
    let limit = req.limit.unwrap_or(100).clamp(1, 1000) as usize;
    let mut items = Vec::new();
    for (i, r) in all.iter().enumerate().rev().take(limit) {
        // Until the card's next repair, so two repairs' effects are not mixed.
        let next_at = all[i + 1..]
            .iter()
            .find(|n| n.card_id == r.card_id)
            .map(|n| n.applied_at);
        let logs = logs_of.get(&r.card_id).map_or(&[][..], Vec::as_slice);
        let before: Vec<&ReviewLogRecord> = {
            let earlier: Vec<&ReviewLogRecord> =
                logs.iter().filter(|l| l.created_at < r.applied_at).collect();
            earlier[earlier.len().saturating_sub(BEFORE_WINDOW)..].to_vec()
        };
        let after: Vec<&ReviewLogRecord> = logs
            .iter()
            .filter(|l| l.created_at >= r.applied_at && next_at.is_none_or(|n| l.created_at < n))
            .collect();
        let after_rate = pass_rate(&after);
        items.push(LeechRepairOutcome {
            repair: dto_of(r.clone())?,
            before: pass_rate(&before),
            small_sample: after_rate.reviews < SMALL_SAMPLE,
            after: after_rate,
            next_reviews: after
                .iter()
                .take(AFTER_LIST)
                .map(|l| LeechReviewAfter {
                    at: l.created_at,
                    passed: l.rating != Rating::Again,
                    duration_ms: l.duration_ms,
                    hints_used: l.hints_used,
                })
                .collect(),
        });
    }
    Ok(LeechRepairReport { items })
}
