//! `flags` module (v0.3): seven colored card flags with optional custom names ("hard",
//! "check source"…). A card may carry several (bitmask `cards.flags`, bit `flag - 1`).

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::ModuleDescriptor;
use crate::registry::Registry;
use crate::support::{err_id, summary};
use fc_api::v1::cardtypes::{
    CardFlags, CardFlagsList, FlagDto, FlagList, SetCardFlag, SetCardFlags, UpdateFlag,
};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::card::Card;
use fc_core::CardId;
use fc_storage::repo::FlagDef;

/// Module id.
pub const ID: &str = "flags";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.flags.name",
        description_key: "modules.flags.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<FlagsList>(list);
    reg.command::<FlagsUpdate>(update);
    reg.command::<CardsSetFlag>(set_flag);
    reg.command::<CardsSetFlags>(set_flags);
}

fn check_flag(flag: u8) -> AppResult<u32> {
    if (1..=7).contains(&flag) {
        Ok(1 << (flag - 1))
    } else {
        Err(AppError::Api(
            ApiError::new(ErrorCode::ValidationFlag, format!("flag {flag} is not 1–7"))
                .with_detail("flag", flag),
        ))
    }
}

fn dto(d: FlagDef) -> FlagDto {
    FlagDto {
        flag: d.flag,
        name: d.name,
        color: d.color,
    }
}

fn list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<FlagList> {
    Ok(FlagList {
        flags: ctx.tx.list_flag_defs()?.into_iter().map(dto).collect(),
    })
}

fn valid_color(c: &str) -> bool {
    !c.is_empty()
        && c.len() <= 32
        && c.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'#')
}

fn update(req: UpdateFlag, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<FlagDto>> {
    check_flag(req.flag)?;
    let old = ctx
        .reads()
        .list_flag_defs()?
        .into_iter()
        .find(|d| d.flag == req.flag)
        .ok_or_else(|| AppError::api(ErrorCode::Internal, "flag row missing"))?;
    let color = match req.color {
        Some(c) if valid_color(c.trim()) => c.trim().to_owned(),
        Some(c) => {
            return Err(AppError::Api(
                ApiError::new(ErrorCode::ValidationInvalidParams, "invalid color")
                    .with_detail("field", "color")
                    .with_detail("value", c),
            ))
        }
        None => old.color.clone(),
    };
    let name = req
        .name
        .map(|n| n.trim().chars().take(64).collect::<String>())
        .filter(|n| !n.is_empty());
    let def = FlagDef {
        flag: req.flag,
        name,
        color,
        updated_at: ctx.now,
    };
    ctx.tx.set_flag_def(&def)?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<FlagsUpdate>(&UpdateFlag {
            flag: old.flag,
            name: old.name,
            color: Some(old.color),
        })?],
        summary: summary::<FlagsUpdate>().param("flag", req.flag),
        events: vec![DomainEvent::FlagsChanged],
        response: dto(def),
    })
}

fn require_card(ctx: &CommandCtx<'_>, id: CardId) -> AppResult<Card> {
    ctx.reads()
        .get_card(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundCard, format!("card {id} not found"), id))
}

/// Writes new bits; returns (new state of every card, previous bits of changed cards).
fn apply(
    ctx: &mut CommandCtx<'_>,
    items: Vec<(CardId, u32)>,
) -> AppResult<(Vec<CardFlags>, Vec<CardFlags>)> {
    let mut now_state = Vec::new();
    let mut previous = Vec::new();
    let mut seen = Vec::new();
    for (id, flags) in items {
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        let mut card = require_card(ctx, id)?;
        if card.flags != flags {
            previous.push(CardFlags {
                id,
                flags: card.flags,
            });
            card.flags = flags;
            card.updated_at = ctx.now;
            ctx.tx.update_card(&card)?;
        }
        now_state.push(CardFlags { id, flags });
    }
    Ok((now_state, previous))
}

fn outcome<M: fc_api::ApiMethod>(
    now_state: Vec<CardFlags>,
    previous: Vec<CardFlags>,
) -> AppResult<CommandOutcome<CardFlagsList>> {
    let ids: Vec<CardId> = previous.iter().map(|c| c.id).collect();
    Ok(CommandOutcome {
        inverse: if previous.is_empty() {
            Vec::new()
        } else {
            vec![inverse_call::<CardsSetFlags>(&SetCardFlags { items: previous })?]
        },
        summary: summary::<M>().param("count", ids.len()),
        events: if ids.is_empty() {
            Vec::new()
        } else {
            vec![DomainEvent::CardsUpdated { ids }]
        },
        response: CardFlagsList { items: now_state },
    })
}

fn set_flag(req: SetCardFlag, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<CardFlagsList>> {
    let bit = check_flag(req.flag)?;
    let mut items = Vec::new();
    for id in req.ids {
        let card = require_card(ctx, id)?;
        let flags = if req.on {
            card.flags | bit
        } else {
            card.flags & !bit
        };
        items.push((id, flags));
    }
    let (now_state, previous) = apply(ctx, items)?;
    outcome::<CardsSetFlag>(now_state, previous)
}

fn set_flags(req: SetCardFlags, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<CardFlagsList>> {
    if let Some(bad) = req.items.iter().find(|c| c.flags >= 1 << 7) {
        return Err(AppError::Api(
            ApiError::new(ErrorCode::ValidationFlag, "unknown flag bits")
                .with_detail("flags", bad.flags),
        ));
    }
    let items = req.items.into_iter().map(|c| (c.id, c.flags)).collect();
    let (now_state, previous) = apply(ctx, items)?;
    outcome::<CardsSetFlags>(now_state, previous)
}
