//! Inbox rules (phase 4): rules with the `inbox_added` trigger act on **inbox items** instead
//! of cards. Their condition is the same query language, evaluated in memory
//! ([`fc_core::query::eval`]) on the card the pending item would become — its deck, note type,
//! fields, tags and classification values (resolved against the deck like `inbox.accept`
//! does), on a new card. So `deck:İngilizce seviye:B1` matches a proposal for that deck with
//! level B1. Values that do not resolve (unknown classification, no deck) are simply absent:
//! a condition on them does not match. Their actions are inbox actions (`inbox.accept`).

use crate::error::AppResult;
use crate::modules::inbox;
use fc_api::v1::inbox::{InboxItemDto, InboxStatus};
use fc_core::attribute::AttributeTarget;
use fc_core::card::{standard_card_specs, Card, CardSpec};
use fc_core::note::{searchable_text, Note};
use fc_core::notetype::FieldKey;
use fc_core::paging::PageRequest;
use fc_core::query::eval::{matches, AttrFact, CardFacts};
use fc_core::query::filter::Filter;
use fc_core::tag::TagName;
use fc_core::{CardId, DeckId, InboxItemId, NoteId, Timestamp};
use fc_storage::Reads;
use std::collections::BTreeSet;
use uuid::Uuid;

/// Most inbox items one run handles (a manual run over a full inbox).
pub const MAX_ITEMS_PER_RUN: usize = 500;

/// Pending items among `scope` (all pending items when `None`) whose would-be card matches
/// `filter`, oldest first, at most [`MAX_ITEMS_PER_RUN`].
pub(crate) fn matching_items(
    reads: &dyn Reads,
    filter: &Filter,
    now: Timestamp,
    scope: Option<&BTreeSet<InboxItemId>>,
) -> AppResult<Vec<InboxItemId>> {
    let mut items = Vec::new();
    match scope {
        Some(ids) => {
            for id in ids {
                if let Some(r) = reads.get_inbox_item(*id)? {
                    items.push(inbox::dto(&r)?);
                }
            }
        }
        None => {
            let mut page = PageRequest::default();
            loop {
                let p = reads.list_inbox("pending", &page)?;
                for r in &p.items {
                    items.push(inbox::dto(r)?);
                }
                match p.next_cursor {
                    Some(c) => page.cursor = Some(c),
                    None => break,
                }
            }
            items.reverse();
        }
    }
    let mut out = Vec::new();
    for item in items {
        if item.status == InboxStatus::Pending && item_matches(reads, filter, now, &item)? {
            out.push(item.id);
            if out.len() == MAX_ITEMS_PER_RUN {
                break;
            }
        }
    }
    Ok(out)
}

/// Whether the card `item` would become matches `filter`.
fn item_matches(
    reads: &dyn Reads,
    filter: &Filter,
    now: Timestamp,
    item: &InboxItemDto,
) -> AppResult<bool> {
    if matches!(filter, Filter::All) {
        return Ok(true);
    }
    let p = &item.item;
    // The note type acceptance would use (v1.3: a captured sentence picks the Subtitle card).
    let nt_id = match p.notetype_id {
        Some(id) => id,
        None => inbox::default_notetype(reads, p, false)?,
    };
    let Some(nt) = reads.get_notetype(nt_id)?.filter(|n| n.deleted_at.is_none()) else {
        return Ok(false);
    };
    // A proposal whose fields do not fit its note type never matches (it cannot be accepted
    // as it stands either).
    let Ok(given) = inbox::note_fields(&nt, p.front.as_deref(), p.back.as_deref(), &p.fields)
    else {
        return Ok(false);
    };
    let fields = given
        .into_iter()
        .map(|(k, v)| (FieldKey(k), v))
        .collect();
    let deck = p.deck_id.unwrap_or(DeckId(Uuid::nil()));
    let note = Note {
        id: NoteId(Uuid::nil()),
        notetype_id: nt.id,
        home_deck_id: deck,
        sort_field: String::new(),
        checksum: String::new(),
        tags: Vec::new(),
        created_at: now,
        updated_at: now,
        deleted_at: None,
        fields,
    };
    let spec = standard_card_specs(&nt)
        .into_iter()
        .next()
        .unwrap_or_else(|| CardSpec {
            template_key: "c1".into(),
            card_type: fc_core::content::builtin::engine::CLOZE.into(),
        });
    let card = Card::new(CardId(Uuid::nil()), note.id, deck, spec, now);
    let mut tag_ids = Vec::new();
    for t in &p.tags {
        if let Ok(name) = TagName::parse(t) {
            if let Some(tag) = reads.get_tag_by_name(&name)? {
                tag_ids.push(tag.id);
            }
        }
    }
    let stored: Vec<(fc_core::AttributeDefId, fc_core::attribute::StoredValue)> =
        match p.deck_id {
            Some(d) => inbox::assignments(reads, d, &p.attributes)
                .map(|a| {
                    a.into_iter()
                        .flat_map(|a| {
                            a.value
                                .map(|v| v.to_stored())
                                .unwrap_or_default()
                                .into_iter()
                                .map(move |v| (a.attr_id, v))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            None => Vec::new(),
        };
    let attrs: Vec<AttrFact<'_>> = stored
        .iter()
        .map(|(id, v)| AttrFact {
            attr_id: *id,
            target: AttributeTarget::Note,
            value: &v.value,
            value_num: v.value_num,
        })
        .collect();
    let text = searchable_text(&nt, &note.fields);
    Ok(matches(
        filter,
        &CardFacts {
            card: &card,
            note: &note,
            tag_ids: &tag_ids,
            attrs: &attrs,
            search_text: &text,
        },
        now,
    ))
}
