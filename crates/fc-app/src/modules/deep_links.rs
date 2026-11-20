//! `deep_links` module (v0.9, docs/06 §6): `flashcard://` links handed to the app by the OS
//! (single instance: a second launch forwards its link to the running window).
//!
//! Parsing is pure (`fc_core::inbox::deeplink`); here ids are checked and the link acts:
//! navigation links (`deck`, `note`, `study`) write nothing and tell the shell where to go;
//! the writing link (`add`) **never** writes a note — it becomes an inbox item (source
//! `deep_link`) the user reviews, as an ordinary undoable `inbox.add`. `import` links are
//! refused for now (`VALIDATION_DEEP_LINK`, `reason: unsupported`).

use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome};
use crate::module::ModuleDescriptor;
use crate::registry::Registry;
use crate::support::{err_id, from_json, summary};
use fc_api::envelope::InvokeRequest;
use fc_api::v1::inbox::*;
use fc_api::v1::methods::*;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::inbox::deeplink::{self, DeepLink};
use fc_core::{DeckId, NoteId, NoteTypeId};
use serde_json::Value;

/// Module id.
pub const ID: &str = "deep_links";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.deep_links.name",
        description_key: "modules.deep_links.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.command::<DeepLinksOpen>(open);
}

fn refused(reason: &str, message: impl Into<String>) -> AppError {
    AppError::Api(ApiError::new(ErrorCode::ValidationDeepLink, message).with_detail("reason", reason))
}

fn parse_id<T: std::str::FromStr>(raw: &str) -> AppResult<T> {
    raw.trim()
        .parse()
        .map_err(|_| refused("id", format!("{raw:?} is not an id")))
}

fn live_deck(ctx: &CommandCtx<'_>, raw: &str) -> AppResult<DeckId> {
    let id: DeckId = parse_id(raw)?;
    match ctx.reads().get_deck(id)? {
        Some(d) if d.deleted_at.is_none() => Ok(id),
        Some(_) => Err(crate::support::deleted("deck", id)),
        None => Err(err_id(ErrorCode::NotFoundDeck, format!("deck {id} not found"), id)),
    }
}

fn open(req: OpenDeepLink, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<DeepLinkOutcome>> {
    let link = deeplink::parse(&req.url).map_err(|e| {
        let mut err = ApiError::new(ErrorCode::ValidationDeepLink, format!("{e:?}"))
            .with_detail("reason", e.reason());
        if let deeplink::DeepLinkError::Missing(what) = e {
            err = err.with_detail("missing", what);
        }
        AppError::Api(err)
    })?;
    let navigate = |target| CommandOutcome {
        response: DeepLinkOutcome::Navigate { target },
        inverse: vec![],
        summary: summary::<DeepLinksOpen>(),
        events: vec![],
    };
    match link {
        DeepLink::Deck { id } => Ok(navigate(DeepLinkTarget::Deck {
            deck_id: live_deck(ctx, &id)?,
        })),
        DeepLink::Note { id } => {
            let note_id: NoteId = parse_id(&id)?;
            crate::support::notes::require_live_note(ctx.reads(), note_id)?;
            Ok(navigate(DeepLinkTarget::Note { note_id }))
        }
        DeepLink::Study { deck, query } => {
            let deck_id = deck.as_deref().map(|d| live_deck(ctx, d)).transpose()?;
            if let Some(q) = &query {
                crate::modules::browse::compile(ctx.reads(), q, ctx.now)?;
            }
            Ok(navigate(DeepLinkTarget::Study { deck_id, query }))
        }
        DeepLink::Add(add) => {
            let item = AddInboxItem {
                front: add.front,
                back: add.back,
                fields: add.fields,
                deck_id: add.deck.as_deref().map(|d| live_deck(ctx, d)).transpose()?,
                notetype_id: add
                    .notetype
                    .as_deref()
                    .map(parse_id::<NoteTypeId>)
                    .transpose()?,
                tags: add.tags,
                attributes: add
                    .attributes
                    .into_iter()
                    .map(|(k, mut v)| {
                        let value = if v.len() == 1 {
                            Value::String(v.remove(0))
                        } else {
                            Value::from(v)
                        };
                        (k, value)
                    })
                    .collect(),
                context: None,
                client_ref: None,
                via: Some(InboxVia::DeepLink),
            };
            let params = serde_json::to_value(&item)
                .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
            let created = ctx.call(&InvokeRequest {
                method: format!("{}.{}", fc_api::API_VERSION, InboxAdd::NAME),
                params,
            })?;
            Ok(CommandOutcome {
                response: DeepLinkOutcome::Inbox {
                    item: from_json(&created)?,
                },
                inverse: vec![],
                summary: summary::<DeepLinksOpen>(),
                events: vec![],
            })
        }
        DeepLink::Import { .. } => Err(refused(
            "unsupported",
            "flashcard://import links are not supported yet",
        )),
    }
}
