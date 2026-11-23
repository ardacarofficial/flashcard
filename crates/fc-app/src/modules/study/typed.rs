//! v1.4 (Z1): **typed answers** kept with the review they were rated with (`typed_answers`,
//! migration 1421), so a leech's evidence (`stats.leech_repairs.plan`) can show what the
//! user actually wrote. Written by the three typed paths — Cards type-in (`study.answer`
//! with `typed_answer`), Learn's written stage (`modes.learn.answer`) and the Exam's written
//! questions (`modes.test.submit`) — inside the answer's own transaction and undo step.
//!
//! * Bounded: the newest [`KEEP_PER_CARD`] per card; older rows are pruned on insert and
//!   brought back by undo.
//! * Switchable: nothing is written while `study.keep_typed_answers` is off (rows kept
//!   before stay).
//! * Private: the text is user history (backups keep it) and never goes to a log line, an
//!   oplog summary or the log bundle.

use super::engine::setting_bool;
use super::KEEP_TYPED_ANSWERS;
use crate::error::AppResult;
use crate::handler::CommandCtx;
use fc_api::v1::study::TypedAnswerDto;
use fc_core::settings::SettingScope;
use fc_core::{CardId, ReviewLogId};
use fc_storage::repo::TypedAnswerRecord;
use fc_storage::Reads;

/// Typed answers kept per card (the newest).
pub const KEEP_PER_CARD: usize = 20;
/// Longest text kept, in characters (the rest is cut).
pub const MAX_CHARS: usize = 1_000;

/// What recording a typed answer changed: the inverse's part (`study.restore`).
#[derive(Debug, Default)]
pub(crate) struct TypedUndo {
    /// Rows written (undo removes them).
    pub remove: Vec<i64>,
    /// Rows pruned (undo puts them back).
    pub add: Vec<TypedAnswerDto>,
}

impl TypedUndo {
    pub(crate) fn extend(&mut self, other: TypedUndo) {
        self.remove.extend(other.remove);
        self.add.extend(other.add);
    }
}

/// One typed answer to keep.
pub(crate) struct TypedAnswer<'a> {
    pub card_id: CardId,
    pub review_log_id: Option<ReviewLogId>,
    pub mode: &'a str,
    pub typed: &'a str,
    /// Judged right (passed: right or typo-level); `None` = rated by the user.
    pub correct: Option<bool>,
}

/// Whether typed answers are kept now (`study.keep_typed_answers`).
pub(crate) fn keeping(reads: &dyn Reads) -> AppResult<bool> {
    setting_bool(reads, &SettingScope::App, KEEP_TYPED_ANSWERS)
}

pub(crate) fn dto(r: &TypedAnswerRecord) -> TypedAnswerDto {
    TypedAnswerDto {
        id: r.id,
        card_id: r.card_id,
        review_log_id: r.review_log_id,
        mode: r.mode.clone(),
        typed: r.typed.clone(),
        correct: r.correct,
        created_at: r.created_at,
    }
}

pub(crate) fn record_of(d: &TypedAnswerDto) -> TypedAnswerRecord {
    TypedAnswerRecord {
        id: d.id,
        card_id: d.card_id,
        review_log_id: d.review_log_id,
        mode: d.mode.clone(),
        typed: d.typed.clone(),
        correct: d.correct,
        created_at: d.created_at,
    }
}

/// Keeps `answer` (nothing when it is blank or the setting is off) and prunes the card's
/// older rows past [`KEEP_PER_CARD`]. Returns what the answer's inverse must undo.
pub(crate) fn record(ctx: &mut CommandCtx<'_>, answer: &TypedAnswer<'_>) -> AppResult<TypedUndo> {
    let text = answer.typed.trim();
    if text.is_empty() || !keeping(ctx.reads())? {
        return Ok(TypedUndo::default());
    }
    let typed: String = text.chars().take(MAX_CHARS).collect();
    let id = ctx.tx.insert_typed_answer(&TypedAnswerRecord {
        id: 0,
        card_id: answer.card_id,
        review_log_id: answer.review_log_id,
        mode: answer.mode.to_owned(),
        typed,
        correct: answer.correct,
        created_at: ctx.now,
    })?;
    let mut undo = TypedUndo {
        remove: vec![id],
        add: Vec::new(),
    };
    for old in ctx
        .reads()
        .typed_answers_of_card(answer.card_id)?
        .into_iter()
        .skip(KEEP_PER_CARD)
    {
        ctx.tx.delete_typed_answer(old.id)?;
        undo.add.push(dto(&old));
    }
    Ok(undo)
}
