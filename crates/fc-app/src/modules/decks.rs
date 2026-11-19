//! `decks` module: hierarchical decks (docs/02 §1).
//!
//! Positions are sort keys among siblings (ties broken by name), not dense indexes: a move
//! never renumbers siblings, so every command has an exact inverse.

use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{bool_setting, main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::support::decks::DeckIndex;
use crate::support::{deleted, err_id, summary};
use fc_api::v1::common::{AffectedCount, Empty};
use fc_api::v1::decks::*;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::deck::{check_move, normalize_deck_segment, Deck, DeckPath};
use fc_core::settings::SettingScopeKind;
use fc_core::{CardId, DeckId, NoteId};
use fc_storage::repo::CardFilter;
use std::collections::{BTreeSet, HashMap};

/// Module id.
pub const ID: &str = "decks";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.decks.name",
        description_key: "modules.decks.description",
        enabled_by_default: true,
        can_disable: false,
        settings: vec![bool_setting(
            "decks.include_subdecks",
            true,
            &[
                SettingScopeKind::App,
                SettingScopeKind::Preset,
                SettingScopeKind::Deck,
            ],
        )],
        ui: vec![main_menu("decks.tree", "nav.decks", "decks", "/decks", 10)],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<DecksTree>(tree);
    reg.query::<DecksList>(list);
    reg.query::<DecksGet>(get);
    reg.command::<DecksCreate>(create);
    reg.command::<DecksRename>(rename);
    reg.command::<DecksMove>(move_deck);
    reg.command::<DecksUpdate>(update);
    reg.command::<DecksDelete>(delete);
    reg.command::<DecksRestore>(restore);
}

fn exists(path: String) -> ApiError {
    ApiError::new(
        ErrorCode::ConflictDeckExists,
        format!("deck {path:?} already exists"),
    )
    .with_detail("path", path)
}

fn ensure_name_free(
    index: &DeckIndex,
    parent: Option<DeckId>,
    name: &str,
    except: DeckId,
) -> AppResult<()> {
    let taken = index
        .children(parent)
        .iter()
        .any(|d| d.id != except && fc_core::deck::name_eq(&d.name, name));
    if taken {
        let path = match parent {
            Some(p) => format!("{}::{name}", index.path(p)),
            None => name.to_owned(),
        };
        return Err(exists(path).into());
    }
    Ok(())
}

// ------------------------------------------------------------------ queries

fn tree(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<DeckTree> {
    let index = DeckIndex::load(ctx.tx)?;
    let own: HashMap<DeckId, DeckCounts> = ctx
        .tx
        .card_counts_by_deck(ctx.now)?
        .into_iter()
        .map(|(id, c)| {
            (
                id,
                DeckCounts {
                    new: c.new,
                    learning: c.learning,
                    due: c.review_due,
                    total: c.total,
                },
            )
        })
        .collect();
    fn node(index: &DeckIndex, own: &HashMap<DeckId, DeckCounts>, d: &Deck) -> DeckTreeNode {
        let children: Vec<DeckTreeNode> = index
            .children(Some(d.id))
            .into_iter()
            .map(|c| node(index, own, c))
            .collect();
        let mut counts = own.get(&d.id).copied().unwrap_or_default();
        for c in &children {
            counts.new += c.counts.new;
            counts.learning += c.counts.learning;
            counts.due += c.counts.due;
            counts.total += c.counts.total;
        }
        DeckTreeNode {
            deck: index.dto(d),
            counts,
            children,
        }
    }
    let roots = index
        .children(None)
        .into_iter()
        .map(|d| node(&index, &own, d))
        .collect();
    Ok(DeckTree { roots })
}

fn list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<DeckList> {
    let index = DeckIndex::load(ctx.tx)?;
    let mut decks: Vec<DeckDto> = index.live().map(|d| index.dto(d)).collect();
    decks.sort_by(|a, b| {
        a.path
            .to_lowercase()
            .cmp(&b.path.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok(DeckList { decks })
}

fn get(req: DeckRef, ctx: &QueryCtx<'_>) -> AppResult<DeckDto> {
    let index = DeckIndex::load(ctx.tx)?;
    Ok(index.dto(index.require(req.id)?))
}

// ------------------------------------------------------------------ commands

fn create(req: CreateDeck, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<DeckDto>> {
    let path = DeckPath::parse(&req.path)?;
    let mut index = DeckIndex::load(ctx.reads())?;
    let segments = path.segments();
    let mut parent: Option<DeckId> = None;
    let mut created: Vec<Deck> = Vec::new();
    for (i, segment) in segments.iter().enumerate() {
        let leaf = i + 1 == segments.len();
        let existing = if created.is_empty() {
            ctx.reads().find_child_deck(parent, segment)?
        } else {
            None
        };
        if let Some(d) = existing {
            if leaf {
                return Err(exists(path.to_string()).into());
            }
            parent = Some(d.id);
            continue;
        }
        let deck = Deck {
            id: DeckId::generate(ctx.ids),
            parent_id: parent,
            name: segment.clone(),
            description: if leaf {
                req.description.clone().unwrap_or_default()
            } else {
                String::new()
            },
            icon: if leaf { req.icon.clone() } else { None },
            color: if leaf { req.color.clone() } else { None },
            preset_id: None,
            position: index.next_position(parent, None),
            created_at: ctx.now,
            updated_at: ctx.now,
            deleted_at: None,
        };
        ctx.tx.insert_deck(&deck)?;
        parent = Some(deck.id);
        created.push(deck);
        index = DeckIndex::load(ctx.reads())?;
    }
    let (Some(top), Some(leaf)) = (created.first(), created.last()) else {
        return Err(exists(path.to_string()).into());
    };
    Ok(CommandOutcome {
        response: index.dto(leaf),
        inverse: vec![inverse_call::<DecksDelete>(&DeckRef { id: top.id })?],
        summary: summary::<DecksCreate>().param("path", path.to_string()),
        events: created
            .iter()
            .map(|d| DomainEvent::DeckCreated { deck: index.dto(d) })
            .collect(),
    })
}

fn rename(req: RenameDeck, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<DeckDto>> {
    let index = DeckIndex::load(ctx.reads())?;
    let mut deck = index.require_live(req.id)?.clone();
    let name = normalize_deck_segment(&req.name)?;
    ensure_name_free(&index, deck.parent_id, &name, deck.id)?;
    let old = std::mem::replace(&mut deck.name, name);
    deck.updated_at = ctx.now;
    ctx.tx.update_deck(&deck)?;
    let dto = DeckIndex::load(ctx.reads())?.dto(&deck);
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<DecksRename>(&RenameDeck {
            id: deck.id,
            name: old.clone(),
        })?],
        summary: summary::<DecksRename>()
            .param("from", old)
            .param("to", &deck.name),
        events: vec![DomainEvent::DeckUpdated { deck: dto.clone() }],
        response: dto,
    })
}

fn move_deck(req: MoveDeck, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<DeckDto>> {
    let index = DeckIndex::load(ctx.reads())?;
    let mut deck = index.require_live(req.id)?.clone();
    if let Some(p) = req.parent_id {
        index.require_live(p)?;
        check_move(deck.id, &index.ancestry(p)?)?;
    }
    ensure_name_free(&index, req.parent_id, &deck.name, deck.id)?;
    let (old_parent, old_position) = (deck.parent_id, deck.position);
    deck.parent_id = req.parent_id;
    deck.position = req
        .position
        .unwrap_or_else(|| index.next_position(req.parent_id, Some(deck.id)));
    deck.updated_at = ctx.now;
    ctx.tx.update_deck(&deck)?;
    let dto = DeckIndex::load(ctx.reads())?.dto(&deck);
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<DecksMove>(&MoveDeck {
            id: deck.id,
            parent_id: old_parent,
            position: Some(old_position),
        })?],
        summary: summary::<DecksMove>().param("path", &dto.path),
        events: vec![DomainEvent::DeckMoved {
            deck: dto.clone(),
            old_parent_id: old_parent,
        }],
        response: dto,
    })
}

fn update(req: UpdateDeck, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<DeckDto>> {
    let index = DeckIndex::load(ctx.reads())?;
    let mut deck = index.require_live(req.id)?.clone();
    let inverse = UpdateDeck {
        id: deck.id,
        description: std::mem::replace(&mut deck.description, req.description),
        icon: std::mem::replace(&mut deck.icon, req.icon),
        color: std::mem::replace(&mut deck.color, req.color),
    };
    deck.updated_at = ctx.now;
    ctx.tx.update_deck(&deck)?;
    let dto = index.dto(&deck);
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<DecksUpdate>(&inverse)?],
        summary: summary::<DecksUpdate>().param("path", &dto.path),
        events: vec![DomainEvent::DeckUpdated { deck: dto.clone() }],
        response: dto,
    })
}

/// Trashes the deck, its live sub-decks, their live cards and the notes left without live
/// cards — all with the same `deleted_at`, which is how `decks.restore` finds them again.
fn delete(req: DeckRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AffectedCount>> {
    let index = DeckIndex::load(ctx.reads())?;
    let deck = index.require_live(req.id)?;
    let path = index.path(deck.id);
    let decks = index.live_subtree(deck.id);
    let cards = ctx.reads().list_cards(&CardFilter {
        deck_ids: Some(decks.clone()),
        note_ids: None,
        include_deleted: false,
    })?;
    let card_ids: Vec<CardId> = cards.iter().map(|c| c.id).collect();
    let touched: BTreeSet<NoteId> = cards.iter().map(|c| c.note_id).collect();
    let mut notes: Vec<NoteId> = Vec::new();
    for n in touched {
        let live = ctx.reads().list_cards(&CardFilter {
            deck_ids: None,
            note_ids: Some(vec![n]),
            include_deleted: false,
        })?;
        if live.iter().all(|c| card_ids.contains(&c.id)) {
            notes.push(n);
        }
    }
    let now = Some(ctx.now);
    let affected = AffectedCount {
        decks: ctx.tx.set_decks_deleted(&decks, now)?,
        notes: ctx.tx.set_notes_deleted(&notes, now)?,
        cards: ctx.tx.set_cards_deleted(&card_ids, now)?,
    };
    Ok(CommandOutcome {
        response: affected,
        inverse: vec![inverse_call::<DecksRestore>(&req)?],
        summary: summary::<DecksDelete>().param("path", path),
        events: vec![DomainEvent::DeckDeleted {
            id: req.id,
            affected,
        }],
    })
}

/// Restores a trashed deck with everything trashed together with it. A live deck is a no-op.
fn restore(req: DeckRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AffectedCount>> {
    let index = DeckIndex::load(ctx.reads())?;
    let deck = index.require(req.id)?.clone();
    let Some(at) = deck.deleted_at else {
        return Ok(CommandOutcome {
            response: AffectedCount::default(),
            inverse: Vec::new(),
            summary: summary::<DecksRestore>().param("path", index.path(deck.id)),
            events: Vec::new(),
        });
    };
    if let Some(p) = deck.parent_id {
        if index.require(p)?.is_deleted() {
            return Err(deleted("deck", p));
        }
    }
    ensure_name_free(&index, deck.parent_id, &deck.name, deck.id)?;
    let mut decks = vec![deck.id];
    decks.extend(
        index
            .descendants(deck.id)
            .into_iter()
            .filter(|d| index.get(*d).is_some_and(|d| d.deleted_at == Some(at))),
    );
    let cards: Vec<_> = ctx
        .reads()
        .list_cards(&CardFilter {
            deck_ids: Some(decks.clone()),
            note_ids: None,
            include_deleted: true,
        })?
        .into_iter()
        .filter(|c| c.deleted_at == Some(at))
        .collect();
    let mut notes: Vec<NoteId> = Vec::new();
    for c in &cards {
        if notes.contains(&c.note_id) {
            continue;
        }
        let note = ctx
            .reads()
            .get_note(c.note_id)?
            .ok_or_else(|| err_id(ErrorCode::NotFoundNote, "card without note", c.note_id))?;
        if note.deleted_at == Some(at) {
            notes.push(note.id);
        }
    }
    let card_ids: Vec<CardId> = cards.iter().map(|c| c.id).collect();
    let affected = AffectedCount {
        decks: ctx.tx.set_decks_deleted(&decks, None)?,
        notes: ctx.tx.set_notes_deleted(&notes, None)?,
        cards: ctx.tx.set_cards_deleted(&card_ids, None)?,
    };
    let restored = DeckIndex::load(ctx.reads())?;
    let dto = restored.dto(restored.require(deck.id)?);
    Ok(CommandOutcome {
        response: affected,
        inverse: vec![inverse_call::<DecksDelete>(&req)?],
        summary: summary::<DecksRestore>().param("path", &dto.path),
        events: vec![DomainEvent::DeckRestored { deck: dto }],
    })
}
