//! `decks` module: hierarchical decks (docs/02 §1).
//!
//! Positions are sort keys among siblings (ties broken by name), not dense indexes: a move
//! never renumbers siblings, so every command has an exact inverse.

use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{bool_setting, main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::support::decks::DeckIndex;
use crate::support::store_ops::{self, Trashed};
use crate::support::{deleted, invalid, summary};
use fc_api::v1::common::{AffectedCount, Empty};
use fc_api::v1::decks::*;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::deck::{check_move, normalize_deck_segment, Deck, DeckPath};
use fc_core::settings::SettingScopeKind;
use fc_core::DeckId;
use std::collections::{HashMap, HashSet};

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
    reg.query::<DecksSummaries>(summaries);
    reg.query::<DecksGet>(get);
    reg.command::<DecksCreate>(create);
    reg.command::<DecksRename>(rename);
    reg.command::<DecksMove>(move_deck);
    reg.command::<DecksUpdate>(update);
    reg.command::<DecksDelete>(delete);
    reg.command::<DecksRestore>(restore);
    reg.command::<DecksRestoreAffected>(restore_affected);
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

/// v1.3: a deck's counts — today's numbers are its study queue (`study.start { kind:
/// normal }` on it now, through the study module's planner), `total` the live cards of the
/// deck and its sub-decks.
fn deck_counts(today: (u32, u32, u32), total: u32) -> DeckCounts {
    let (new, learning, due) = today;
    DeckCounts {
        new,
        learning,
        due,
        total,
        today_due: due,
        today_learning: learning,
        today_new: new,
        today_total: new + learning + due,
    }
}

/// The study planner for today's counts, on the client's study day (`tz_offset_mins` as
/// last given to `study.start`).
fn today_planner<'a>(ctx: &'a QueryCtx<'_>) -> AppResult<crate::modules::study::today::Planner<'a>> {
    use crate::modules::study::today::{client_tz, Planner};
    let v = crate::modules::study::ext::View::query(ctx);
    Planner::new(v, ctx.now, client_tz(ctx.tx, None)?)
}

fn tree(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<DeckTree> {
    let index = DeckIndex::load(ctx.tx)?;
    let own_total: HashMap<DeckId, u32> = ctx.tx.card_totals_by_deck()?.into_iter().collect();
    let mut planner = today_planner(ctx)?;
    fn node(
        index: &DeckIndex,
        own_total: &HashMap<DeckId, u32>,
        planner: &mut crate::modules::study::today::Planner<'_>,
        d: &Deck,
    ) -> AppResult<DeckTreeNode> {
        let children: Vec<DeckTreeNode> = index
            .children(Some(d.id))
            .into_iter()
            .map(|c| node(index, own_total, planner, c))
            .collect::<AppResult<_>>()?;
        let total = own_total.get(&d.id).copied().unwrap_or_default()
            + children.iter().map(|c| c.counts.total).sum::<u32>();
        let today = crate::modules::study::today::today_counts(planner, Some(d.id))?;
        Ok(DeckTreeNode {
            deck: index.dto(d),
            counts: deck_counts(today, total),
            children,
        })
    }
    let roots = index
        .children(None)
        .into_iter()
        .map(|d| node(&index, &own_total, &mut planner, d))
        .collect::<AppResult<_>>()?;
    Ok(DeckTree { roots })
}

/// Per-deck accumulator of [`summaries`].
#[derive(Default)]
struct Progress {
    new: u32,
    learning: u32,
    mastered: u32,
    /// Last review of the deck's cards, sub-decks included.
    last: Option<fc_core::Timestamp>,
    /// Last review of the deck's own cards (the recent list).
    own_last: Option<fc_core::Timestamp>,
    /// `(definition, option)` → notes.
    chips: HashMap<(usize, usize), u32>,
}

/// Chips shown per deck.
const DECK_CHIPS: usize = 3;

fn summaries(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<DeckSummaries> {
    use fc_core::stats::{card_mastery, mastered_percent, Mastery};
    let reads = ctx.tx;
    let index = DeckIndex::load(reads)?;
    let live: HashSet<DeckId> = index.live().map(|d| d.id).collect();
    let mut total = 0;
    for (id, n) in reads.card_totals_by_deck()? {
        if live.contains(&id) {
            total += n;
        }
    }
    // Whole collection: the queue of `study.start { deck_id: null }` (Home's Start).
    let today = deck_counts(
        crate::modules::study::today::today_counts(&mut today_planner(ctx)?, None)?,
        total,
    );
    let cards = reads.study_cards(&fc_storage::repo::StudyCardFilter {
        include_suspended: true,
        ..Default::default()
    })?;
    let mut progress: HashMap<DeckId, Progress> = HashMap::new();
    // Note → decks (with ancestors) its cards are in: a note's chips count once per deck.
    let mut note_decks: HashMap<fc_core::NoteId, HashSet<DeckId>> = HashMap::new();
    let mut card_note = HashMap::new();
    for card in &cards {
        card_note.insert(card.id, card.note_id);
        let level = card_mastery(&card.scheduling);
        let reviewed = card.scheduling.last_review;
        let chain = index.ancestry(card.deck_id).unwrap_or_default();
        let decks = note_decks.entry(card.note_id).or_default();
        for d in chain {
            decks.insert(d);
            let p = progress.entry(d).or_default();
            match level {
                Mastery::New => p.new += 1,
                Mastery::Learning => p.learning += 1,
                Mastery::Mastered => p.mastered += 1,
            }
            p.last = p.last.max(reviewed);
            if d == card.deck_id {
                p.own_last = p.own_last.max(reviewed);
            }
        }
    }
    let notes: HashSet<fc_core::NoteId> = note_decks.keys().copied().collect();
    let choices = crate::support::attrs::note_choices(reads, &card_note, &notes)?;
    for (note, values) in &choices.of_note {
        for d in note_decks.get(note).into_iter().flatten() {
            let p = progress.entry(*d).or_default();
            for v in values {
                *p.chips.entry(*v).or_default() += 1;
            }
        }
    }
    // Tree order: depth-first in sibling order.
    fn walk(index: &DeckIndex, parent: Option<DeckId>, order: &mut Vec<DeckId>) {
        for d in index.children(parent) {
            order.push(d.id);
            walk(index, Some(d.id), order);
        }
    }
    let mut order = Vec::new();
    walk(&index, None, &mut order);
    let mut decks = Vec::with_capacity(order.len());
    let mut recent: Vec<(fc_core::Timestamp, usize, DeckId)> = Vec::new();
    for (pos, id) in order.into_iter().enumerate() {
        let p = progress.remove(&id).unwrap_or_default();
        if let Some(at) = p.own_last {
            recent.push((at, pos, id));
        }
        let mut chips: Vec<((usize, usize), u32)> = p.chips.into_iter().collect();
        chips.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let chips = chips
            .into_iter()
            .take(DECK_CHIPS)
            .filter_map(|((di, oi), n)| {
                let def = choices.defs.get(di)?;
                let option = def.options.get(oi)?;
                Some(DeckChip {
                    attr_id: def.id,
                    name: def.name.clone(),
                    label: option.label.clone(),
                    notes: n,
                })
            })
            .collect();
        let total = p.new + p.learning + p.mastered;
        decks.push(DeckSummary {
            deck_id: id,
            new: p.new,
            learning: p.learning,
            mastered: p.mastered,
            mastered_pct: mastered_percent(p.mastered, total),
            last_studied_at: p.last,
            chips,
        });
    }
    recent.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    Ok(DeckSummaries {
        today,
        decks,
        recent: recent
            .into_iter()
            .take(RECENT_DECKS)
            .map(|(_, _, id)| id)
            .collect(),
    })
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
    let relative = DeckPath::parse(&req.path)?;
    let mut index = DeckIndex::load(ctx.reads())?;
    // The full path (for messages): the parent's path joined with the relative one, built
    // here so clients never join paths themselves.
    let path = match req.parent_id {
        Some(p) => {
            index.require_live(p)?;
            DeckPath::parse(&format!("{}::{relative}", index.path(p)))?
        }
        None => relative.clone(),
    };
    let segments = relative.segments();
    let mut parent: Option<DeckId> = req.parent_id;
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
/// cards — all with the same `deleted_at`. The inverse (internal `decks.restore_affected`)
/// carries exactly the trashed ids.
fn delete(req: DeckRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AffectedCount>> {
    let index = DeckIndex::load(ctx.reads())?;
    let deck = index.require_live(req.id)?;
    let path = index.path(deck.id);
    let trashed = store_ops::trash_deck_subtree(ctx.tx, deck.id, ctx.now)?;
    let affected = count(&trashed);
    Ok(CommandOutcome {
        response: affected,
        inverse: vec![inverse_call::<DecksRestoreAffected>(
            &RestoreDeckAffected {
                id: req.id,
                decks: trashed.decks,
                notes: trashed.notes,
                cards: trashed.cards,
            },
        )?],
        summary: summary::<DecksDelete>().param("path", path),
        events: vec![DomainEvent::DeckDeleted {
            id: req.id,
            affected,
        }],
    })
}

fn count(t: &Trashed) -> AffectedCount {
    let n = |len: usize| u32::try_from(len).unwrap_or(u32::MAX);
    AffectedCount {
        decks: n(t.decks.len()),
        notes: n(t.notes.len()),
        cards: n(t.cards.len()),
    }
}

/// Restores what `apply` brings back (entities trashed together with deck `id`). The parent
/// must be live and the name free. Inverse: `decks.delete`, which re-trashes the same set from
/// this state.
fn restore_items<M: fc_api::ApiMethod>(
    ctx: &mut CommandCtx<'_>,
    id: DeckId,
    apply: impl FnOnce(&mut CommandCtx<'_>) -> AppResult<Trashed>,
) -> AppResult<CommandOutcome<AffectedCount>> {
    let index = DeckIndex::load(ctx.reads())?;
    let deck = index.require(id)?.clone();
    if !deck.is_deleted() {
        return Ok(CommandOutcome {
            response: AffectedCount::default(),
            inverse: Vec::new(),
            summary: summary::<M>().param("path", index.path(deck.id)),
            events: Vec::new(),
        });
    }
    if let Some(p) = deck.parent_id {
        if index.require(p)?.is_deleted() {
            return Err(deleted("deck", p));
        }
    }
    ensure_name_free(&index, deck.parent_id, &deck.name, deck.id)?;
    let restored_items = apply(ctx)?;
    let restored = DeckIndex::load(ctx.reads())?;
    let dto = restored.dto(restored.require(deck.id)?);
    Ok(CommandOutcome {
        response: count(&restored_items),
        inverse: vec![inverse_call::<DecksDelete>(&DeckRef { id })?],
        summary: summary::<M>().param("path", &dto.path),
        events: vec![DomainEvent::DeckRestored { deck: dto }],
    })
}

/// Restores a trashed deck (from the trash UI) with everything whose `deleted_at` equals the
/// deck's. A live deck is a no-op.
fn restore(req: DeckRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AffectedCount>> {
    restore_items::<DecksRestore>(ctx, req.id, |ctx| {
        store_ops::restore_deck_subtree(ctx.tx, req.id)
    })
}

/// Internal inverse of `decks.delete`: restores exactly the listed entities.
fn restore_affected(
    req: RestoreDeckAffected,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AffectedCount>> {
    if !req.decks.contains(&req.id) {
        return Err(invalid("the deleted deck must be among the restored decks"));
    }
    let items = Trashed {
        decks: req.decks,
        notes: req.notes,
        cards: req.cards,
    };
    restore_items::<DecksRestoreAffected>(ctx, req.id, |ctx| {
        store_ops::restore_trashed(ctx.tx, &items)?;
        Ok(items)
    })
}
