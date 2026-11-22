//! `accepted_answers` module (v1.4): per note and field, other answers the **offline**
//! written-answer check also accepts ("give up" → `vazgeçmek` and `bırakmak`). The list is
//! data: the user adds/removes answers, or an applied AI meaning (`ai.meaning.apply`) adds
//! its alternatives — no model is asked at study time. The study check reads it through
//! [`alternatives_for`]; turning the module off makes the check ignore the lists (they stay).

use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::ModuleDescriptor;
use crate::modules::ai::context_meaning_field;
use crate::registry::Registry;
use crate::support::notes::{require_live_note, require_notetype};
use crate::support::summary;
use fc_api::v1::accepted_answers::*;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::NoteId;
use fc_storage::repo::AcceptedAnswerRecord;
use fc_storage::Reads;
use fc_study::text::html_to_text;

/// Module id.
pub const ID: &str = "accepted_answers";

/// Most accepted answers per note field.
pub const MAX_PER_FIELD: usize = 20;
/// Longest accepted answer (characters).
pub const MAX_CHARS: usize = 300;

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.accepted_answers.name",
        description_key: "modules.accepted_answers.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<AcceptedAnswersList>(|req: AcceptedAnswersRef, ctx: &QueryCtx<'_>| {
        require_live_note(ctx.tx, req.note_id)?;
        state(ctx.tx, req.note_id)
    });
    reg.command::<AcceptedAnswersAdd>(add);
    reg.command::<AcceptedAnswersRemove>(remove);
    reg.command::<AcceptedAnswersRestore>(restore);
}

/// A note's lists, by field key (answers in order).
pub(crate) fn lists(reads: &dyn Reads, note: NoteId) -> AppResult<Vec<AcceptedAnswers>> {
    let mut out: Vec<AcceptedAnswers> = Vec::new();
    for r in reads.list_accepted_answers(note)? {
        match out.last_mut() {
            Some(l) if l.field == r.field_key => l.answers.push(r.answer),
            _ => out.push(AcceptedAnswers {
                field: r.field_key,
                answers: vec![r.answer],
            }),
        }
    }
    Ok(out)
}

fn state(reads: &dyn Reads, note: NoteId) -> AppResult<NoteAcceptedAnswers> {
    Ok(NoteAcceptedAnswers {
        note_id: note,
        fields: lists(reads, note)?,
    })
}

/// Lowercase with the Turkish rule (`I → ı`, `İ → i`): "BIRAKMAK" is "bırakmak".
pub(crate) fn fold(s: &str) -> String {
    s.trim()
        .chars()
        .flat_map(|c| match c {
            'I' => vec!['ı'],
            'İ' => vec!['i'],
            c => c.to_lowercase().collect(),
        })
        .collect()
}

fn same(a: &str, b: &str) -> bool {
    fold(a) == fold(b)
}

/// Accepted answers the written check should also take for a card of `note` whose expected
/// answer is `expected`: the lists of the note's fields whose text is that answer (a card
/// asking for the meaning gets the meaning's alternatives, a reverse card asking for the
/// word does not). Empty when the module is off.
pub fn alternatives_for(
    reads: &dyn Reads,
    registry: &Registry,
    note: NoteId,
    expected: &str,
) -> AppResult<Vec<String>> {
    if !crate::support::module_enabled(reads, registry, ID)? {
        return Ok(Vec::new());
    }
    let lists = lists(reads, note)?;
    if lists.is_empty() {
        return Ok(Vec::new());
    }
    let Some(n) = reads.get_note(note)? else {
        return Ok(Vec::new());
    };
    let expected = html_to_text(expected);
    let mut out = Vec::new();
    for l in lists {
        let field_text = n
            .fields
            .iter()
            .find(|(k, _)| k.0 == l.field)
            .map(|(_, v)| html_to_text(v))
            .unwrap_or_default();
        if !field_text.trim().is_empty() && same(&field_text, &expected) {
            out.extend(l.answers);
        }
    }
    Ok(out)
}

fn invalid(reason: &str, message: impl Into<String>) -> crate::error::AppError {
    ApiError::new(ErrorCode::ValidationAcceptedAnswer, message)
        .with_detail("reason", reason)
        .into()
}

/// The field a request names, else the note type's meaning field; it must exist.
fn field_of(ctx: &CommandCtx<'_>, note: NoteId, field: Option<&str>) -> AppResult<String> {
    let n = require_live_note(ctx.reads(), note)?;
    let nt = require_notetype(ctx.reads(), n.notetype_id)?;
    let key = match field.map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => f.to_owned(),
        None => context_meaning_field(&nt)
            .ok_or_else(|| invalid("field", "the note type has no meaning field: name the field"))?,
    };
    if !nt.fields.iter().any(|f| f.key.0 == key) {
        return Err(invalid("field", format!("the note has no field {key:?}")));
    }
    Ok(key)
}

fn rows_of(note: NoteId, lists: &[AcceptedAnswers]) -> Vec<AcceptedAnswerRecord> {
    lists
        .iter()
        .flat_map(|l| {
            l.answers.iter().enumerate().map(|(i, a)| AcceptedAnswerRecord {
                note_id: note,
                field_key: l.field.clone(),
                answer: a.clone(),
                position: i as i64,
            })
        })
        .collect()
}

fn write(
    ctx: &mut CommandCtx<'_>,
    note: NoteId,
    before: Vec<AcceptedAnswers>,
    after: Vec<AcceptedAnswers>,
    summary_key: fc_api::v1::common::I18nMessage,
) -> AppResult<CommandOutcome<NoteAcceptedAnswers>> {
    if before == after {
        return Ok(CommandOutcome {
            response: NoteAcceptedAnswers {
                note_id: note,
                fields: after,
            },
            inverse: vec![],
            summary: summary_key,
            events: vec![],
        });
    }
    ctx.tx.set_accepted_answers(note, &rows_of(note, &after))?;
    Ok(CommandOutcome {
        response: NoteAcceptedAnswers {
            note_id: note,
            fields: after,
        },
        inverse: vec![inverse_call::<AcceptedAnswersRestore>(&RestoreAcceptedAnswers {
            note_id: note,
            fields: before,
        })?],
        summary: summary_key,
        events: vec![DomainEvent::NoteUpdated { note_id: note }],
    })
}

fn add(req: EditAcceptedAnswers, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<NoteAcceptedAnswers>> {
    let field = field_of(ctx, req.note_id, req.field.as_deref())?;
    let before = lists(ctx.reads(), req.note_id)?;
    let mut after = before.clone();
    let pos = match after.iter().position(|l| l.field == field) {
        Some(p) => p,
        None => {
            after.push(AcceptedAnswers {
                field: field.clone(),
                answers: vec![],
            });
            after.sort_by(|a, b| a.field.cmp(&b.field));
            after.iter().position(|l| l.field == field).unwrap_or(0)
        }
    };
    let list = &mut after[pos].answers;
    for a in &req.answers {
        let a = a.trim();
        if a.is_empty() || list.iter().any(|x| same(x, a)) {
            continue;
        }
        if a.chars().count() > MAX_CHARS {
            return Err(invalid("too_long", "an accepted answer is too long"));
        }
        list.push(a.to_owned());
    }
    if list.len() > MAX_PER_FIELD {
        return Err(invalid("too_many", format!("at most {MAX_PER_FIELD} accepted answers per field")));
    }
    after.retain(|l| !l.answers.is_empty());
    let n = req.answers.len();
    write(ctx, req.note_id, before, after, summary::<AcceptedAnswersAdd>().param("count", n))
}

fn remove(req: EditAcceptedAnswers, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<NoteAcceptedAnswers>> {
    let field = field_of(ctx, req.note_id, req.field.as_deref())?;
    let before = lists(ctx.reads(), req.note_id)?;
    let mut after = before.clone();
    for l in after.iter_mut().filter(|l| l.field == field) {
        l.answers.retain(|x| !req.answers.iter().any(|a| same(a, x)));
    }
    after.retain(|l| !l.answers.is_empty());
    let n = req.answers.len();
    write(ctx, req.note_id, before, after, summary::<AcceptedAnswersRemove>().param("count", n))
}

fn restore(req: RestoreAcceptedAnswers, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<NoteAcceptedAnswers>> {
    let before = lists(ctx.reads(), req.note_id)?;
    ctx.tx.set_accepted_answers(req.note_id, &rows_of(req.note_id, &req.fields))?;
    Ok(CommandOutcome {
        response: state(ctx.reads(), req.note_id)?,
        inverse: vec![inverse_call::<AcceptedAnswersRestore>(&RestoreAcceptedAnswers {
            note_id: req.note_id,
            fields: before,
        })?],
        summary: summary::<AcceptedAnswersRestore>(),
        events: vec![DomainEvent::NoteUpdated { note_id: req.note_id }],
    })
}
