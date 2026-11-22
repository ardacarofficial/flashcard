//! In-memory evaluation of a [`Filter`] against one card. The reference semantics: the SQLite
//! compiler in `fc-storage` must agree with this (a differential test checks it).

use super::ast::CmpOp;
use super::filter::{AttrTest, CardNum, Filter, Pred, TimeCol};
use super::lang::{LangFacts, MediaFrom};
use super::text::{
    compile_regex, field_glob_matches, fields_text, fold, phrase_matches, regex_matches, Glob,
};
use crate::attribute::AttributeTarget;
use crate::card::{Card, CardState};
use crate::id::{AttributeDefId, TagId};
use crate::note::Note;
use crate::time::Timestamp;
use std::collections::BTreeMap;

/// One stored classification value row of the card or its note.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttrFact<'a> {
    /// Definition.
    pub attr_id: AttributeDefId,
    /// Note or card value.
    pub target: AttributeTarget,
    /// `attribute_values.value`.
    pub value: &'a str,
    /// `attribute_values.value_num`.
    pub value_num: Option<f64>,
}

/// What a filter can look at for one card.
#[derive(Debug, Clone, Copy)]
pub struct CardFacts<'a> {
    /// The card.
    pub card: &'a Card,
    /// Its note.
    pub note: &'a Note,
    /// Ids of the note's tags.
    pub tag_ids: &'a [TagId],
    /// Value rows of the note (target `Note`) and of the card (target `Card`).
    pub attrs: &'a [AttrFact<'a>],
    /// The note's indexed text (`fc_core::note::searchable_text`).
    pub search_text: &'a str,
    /// v1.4: what the language predicates look at ([`LangFacts::NONE`] when unknown).
    pub lang: &'a LangFacts,
}

/// Whether the card matches.
pub fn matches(filter: &Filter, f: &CardFacts<'_>, now: Timestamp) -> bool {
    match filter {
        Filter::All => true,
        Filter::Nothing => false,
        Filter::And(v) => v.iter().all(|x| matches(x, f, now)),
        Filter::Or(v) => v.iter().any(|x| matches(x, f, now)),
        Filter::Not(x) => !matches(x, f, now),
        Filter::Pred(p) => pred(p, f, now),
    }
}

fn fields(note: &Note) -> BTreeMap<String, String> {
    note.fields
        .iter()
        .map(|(k, v)| (k.0.clone(), v.clone()))
        .collect()
}

fn buried(card: &Card, now: Timestamp) -> bool {
    card.buried_until.is_some_and(|b| b > now)
}

fn scheduled_due(card: &Card) -> Option<Timestamp> {
    if card.scheduling.state == CardState::New {
        None
    } else {
        card.scheduling.due
    }
}

fn pred(p: &Pred, f: &CardFacts<'_>, now: Timestamp) -> bool {
    let c = f.card;
    let s = &c.scheduling;
    match p {
        Pred::Text { tokens, prefix } => phrase_matches(f.search_text, tokens, *prefix),
        Pred::TextGlob(src) => Glob::new(src).matches(&fields_text(&fields(f.note), None)),
        Pred::Field { keys, glob, .. } => {
            field_glob_matches(&fields(f.note), keys, &Glob::new(glob))
        }
        Pred::Regex { keys, pattern } => compile_regex(pattern)
            .is_ok_and(|re| regex_matches(&fields(f.note), keys.as_deref(), &re)),
        Pred::Decks(ids) => ids.contains(&c.deck_id),
        Pred::Tags(ids) => f.tag_ids.iter().any(|t| ids.contains(t)),
        Pred::Untagged => f.tag_ids.is_empty(),
        Pred::NoteTypes(ids) => ids.contains(&f.note.notetype_id),
        Pred::CardType(t) => fold(&c.card_type) == *t,
        Pred::Template(t) => fold(&c.template_key) == *t,
        Pred::State(st) => s.state == *st,
        Pred::DueNow => {
            !c.suspended && !buried(c, now) && scheduled_due(c).is_some_and(|d| d <= now)
        }
        Pred::Suspended => c.suspended,
        Pred::Buried => buried(c, now),
        Pred::Flagged => c.flags != 0,
        Pred::FlagBit(b) => *b < 32 && c.flags & (1u32 << b) != 0,
        Pred::Num(prop, op, n) => {
            let v = match prop {
                CardNum::Stability => s.stability,
                CardNum::Difficulty => s.difficulty,
                CardNum::Lapses => Some(f64::from(s.lapses)),
                CardNum::Reps => Some(f64::from(s.reps)),
            };
            v.is_some_and(|v| op.holds(v, *n))
        }
        Pred::Recall {
            op,
            value,
            decay,
            day_start,
        } => crate::recall::card_retrievability(s, *day_start, *decay)
            .is_some_and(|r| op.holds(r, *value)),
        Pred::Time(col, op, at) => {
            let v = match col {
                TimeCol::CardCreated => Some(c.created_at),
                TimeCol::NoteEdited => Some(f.note.updated_at),
                TimeCol::LastReview => s.last_review,
                TimeCol::Due => scheduled_due(c),
            };
            v.is_some_and(|v| op.holds(v, *at))
        }
        Pred::Attr(alts) => alts.iter().any(|alt| {
            f.attrs
                .iter()
                .filter(|r| r.attr_id == alt.attr_id && r.target == alt.target)
                .any(|r| attr_row(&alt.test, r))
        }),
        Pred::NoteIds(ids) => ids.contains(&f.note.id),
        Pred::CardIds(ids) => ids.contains(&c.id),
        Pred::WordStatus {
            statuses,
            known_days,
        } => f
            .lang
            .status(*known_days)
            .is_some_and(|s| statuses.contains(&s)),
        Pred::Language(Some(l)) => !f.lang.language.is_empty() && f.lang.language == *l,
        Pred::Language(None) => f.lang.language.is_empty(),
        Pred::WordKeys(keys) => f.lang.key.as_ref().is_some_and(|k| keys.contains(k)),
        Pred::WordKeyGlob(src) => f
            .lang
            .key
            .as_ref()
            .is_some_and(|k| Glob::new(src).matches(k)),
        Pred::Encounters(op, n) => f
            .lang
            .word
            .as_ref()
            .is_some_and(|w| op.holds(f64::from(w.notes), *n)),
        Pred::Origins(list) => list.contains(&f.lang.origin),
        Pred::Source(src) => {
            let g = Glob::new(src);
            f.lang.source.iter().any(|s| g.matches(s))
        }
        Pred::Media(kind, from) => f.lang.media.iter().any(|(k, video)| {
            k == kind
                && match from {
                    MediaFrom::Any => true,
                    MediaFrom::Video => *video,
                    MediaFrom::File => !*video,
                }
        }),
        Pred::FreqRank(op, n) => f.lang.rank.is_some_and(|r| op.holds(f64::from(r), *n)),
        Pred::Band(bands) => f.lang.band.is_some_and(|b| bands.contains(&b)),
    }
}

fn attr_row(test: &AttrTest, r: &AttrFact<'_>) -> bool {
    match test {
        AttrTest::Any => true,
        AttrTest::Options(ids) => ids.iter().any(|id| id.to_string() == r.value),
        AttrTest::Number(op, n) => r.value_num.is_some_and(|v| op.holds(v, *n)),
        AttrTest::Text(src) => Glob::new(src).matches(r.value),
        AttrTest::Date(op, d) => op.holds(r.value, d.as_str()),
        AttrTest::Bool(b) => r.value == if *b { "true" } else { "false" },
    }
}

/// Sanity helper for callers mapping ops to SQL.
pub fn sql_op(op: CmpOp) -> &'static str {
    match op {
        CmpOp::Colon | CmpOp::Eq => "=",
        CmpOp::Ne => "<>",
        CmpOp::Lt => "<",
        CmpOp::Le => "<=",
        CmpOp::Gt => ">",
        CmpOp::Ge => ">=",
    }
}
