//! `onboarding` module (v1.0, docs/02 §32): the first-run guide and the starter decks.
//!
//! * `onboarding.status` says whether to show the guide: never finished/skipped **and** the
//!   collection has no cards (an upgraded collection with data is never interrupted).
//! * `onboarding.apply` carries out `fc_core::ux::onboarding::plan`: a deck named for the
//!   subject, its classification template (CEFR, YKS…; skipped while `attributes` is off),
//!   the preset (reused by name; skipped while `presets` is off) and the chosen starter decks
//!   — all through the ordinary commands, so the whole setup is **one undo step**. The
//!   "finished" flag is bookkeeping (`ux_state`), not undone.
//! * `starter.install` adds one starter deck the same way (one undo step). Starter notes
//!   carry their classification value (`fc_core::ux::starter`).

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::support::attrs::effective_for_deck;
use crate::support::decks::DeckIndex;
use crate::support::{from_json, module_enabled, summary};
use fc_api::v1::attributes::{AttributeAssignment, AttributeDefDto, DefineAttributeFromTemplate};
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::methods::*;
use fc_api::v1::notes::CreateNote;
use fc_api::v1::onboarding::*;
use fc_api::v1::presets::{AssignPreset, CreatePreset, PresetDto};
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::attribute::AttributeValue;
use fc_core::content::builtin;
use fc_core::i18n::Locale;
use fc_core::preset::PresetConfig;
use fc_core::ux::onboarding::{plan, StudySubject};
use fc_core::ux::starter::{self, StarterDeck};
use fc_core::{AttributeDefId, AttributeOptionId, DeckId, NoteTypeId};
use fc_storage::repo::UX_ONBOARDING_DONE;
use fc_storage::Reads;
use serde_json::json;
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

/// Module id.
pub const ID: &str = "onboarding";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.onboarding.name",
        description_key: "modules.onboarding.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![main_menu("onboarding.starters", "nav.starters", "sparkles", "/starters", 94)],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<OnboardingStatus>(status);
    reg.query::<OnboardingOptions>(options);
    reg.command::<OnboardingApply>(apply);
    reg.command::<OnboardingSkip>(skip);
    reg.query::<StarterList>(list);
    reg.command::<StarterInstall>(install);
}

/// Runs `M` inside the current transaction and batch (its own oplog entry and events).
fn call<M: ApiMethod>(ctx: &mut CommandCtx<'_>, req: &M::Request) -> AppResult<M::Response> {
    let v = ctx.call(&inverse_call::<M>(req)?)?;
    from_json(&v)
}

fn notetype_id(key: &str) -> AppResult<NoteTypeId> {
    builtin::find(key)
        .map(|b| b.id)
        .ok_or_else(|| AppError::api(ErrorCode::Internal, format!("no built-in note type {key}")))
}

fn starter_of(id: &str) -> AppResult<&'static StarterDeck> {
    starter::find(id).ok_or_else(|| {
        AppError::Api(
            ApiError::new(ErrorCode::NotFoundStarterDeck, format!("starter deck {id} not found"))
                .with_detail("id", id),
        )
    })
}

fn starter_dto(d: &StarterDeck, locale: Locale) -> AppResult<StarterDeckDto> {
    Ok(StarterDeckDto {
        id: d.id.to_owned(),
        name: d.name.get(locale).to_owned(),
        description: d.description.get(locale).to_owned(),
        note_count: u32::try_from(d.notes.len()).unwrap_or(u32::MAX),
        notetype_id: notetype_id(d.notetype)?,
        template_id: d.template.map(str::to_owned),
    })
}

fn completed(reads: &dyn Reads) -> AppResult<bool> {
    Ok(reads
        .ux_state(UX_ONBOARDING_DONE)?
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}

fn state(reads: &dyn Reads, now: fc_core::Timestamp) -> AppResult<OnboardingState> {
    let completed = completed(reads)?;
    let empty = reads
        .card_counts_by_deck(now)?
        .iter()
        .all(|(_, c)| c.total == 0);
    Ok(OnboardingState {
        show: !completed && empty,
        completed,
    })
}

fn mark_done(ctx: &mut CommandCtx<'_>) -> AppResult<()> {
    ctx.tx.set_ux_state(UX_ONBOARDING_DONE, &json!(true), ctx.now)?;
    Ok(())
}

// ------------------------------------------------------------------ queries

fn status(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<OnboardingState> {
    state(ctx.tx, ctx.now)
}

fn options(req: LocaleRequest, ctx: &QueryCtx<'_>) -> AppResult<OnboardingChoices> {
    let locale = req.locale.unwrap_or(ctx.locale);
    let subjects = StudySubject::ALL
        .iter()
        .map(|s| {
            let p = plan(*s);
            Ok(OnboardingSubjectDto {
                subject: *s,
                goal: s.goal(),
                deck_name: p.deck_name.get(locale).to_owned(),
                template_id: p.template.map(str::to_owned),
                notetype_id: notetype_id(p.notetype)?,
                starters: p.starters.iter().map(|id| (*id).to_owned()).collect(),
            })
        })
        .collect::<AppResult<_>>()?;
    Ok(OnboardingChoices {
        subjects,
        starters: all_starters(locale)?,
    })
}

fn all_starters(locale: Locale) -> AppResult<Vec<StarterDeckDto>> {
    starter::all().iter().map(|d| starter_dto(d, locale)).collect()
}

fn list(req: LocaleRequest, ctx: &QueryCtx<'_>) -> AppResult<StarterDeckList> {
    Ok(StarterDeckList {
        starters: all_starters(req.locale.unwrap_or(ctx.locale))?,
    })
}

// ------------------------------------------------------------------ commands

/// A deck name not taken among the live children of `parent`: `name`, `name (2)`, …
fn free_name(reads: &dyn Reads, parent: Option<DeckId>, name: &str) -> AppResult<String> {
    let mut candidate = name.to_owned();
    let mut n = 2;
    while reads.find_child_deck(parent, &candidate)?.is_some() {
        candidate = format!("{name} ({n})");
        n += 1;
    }
    Ok(candidate)
}

/// Defines the template on `deck` when the classification module is on.
fn define_template(
    ctx: &mut CommandCtx<'_>,
    template: Option<&str>,
    deck: DeckId,
    locale: Locale,
) -> AppResult<Option<AttributeDefDto>> {
    let Some(template_id) = template else {
        return Ok(None);
    };
    if !module_enabled(ctx.reads(), ctx.registry, crate::modules::attributes::ID)? {
        return Ok(None);
    }
    call::<AttributesDefineFromTemplate>(
        ctx,
        &DefineAttributeFromTemplate {
            template_id: template_id.to_owned(),
            scope_deck_id: Some(deck),
            locale: Some(locale),
        },
    )
    .map(Some)
}

/// The classification a starter deck's notes are valued with: the one the parent deck already
/// has or inherits from the same template (never a duplicate definition), else a fresh copy
/// defined on the starter deck. Returns the definition id and its option ids in template order.
fn starter_classification(
    ctx: &mut CommandCtx<'_>,
    template: Option<&str>,
    deck: DeckId,
    parent: Option<DeckId>,
    locale: Locale,
) -> AppResult<Option<(AttributeDefId, Vec<AttributeOptionId>)>> {
    let (Some(template_id), Some(parent)) = (template, parent) else {
        return Ok(define_template(ctx, template, deck, locale)?.map(|a| {
            (a.id, a.options.iter().map(|o| o.id).collect())
        }));
    };
    if !module_enabled(ctx.reads(), ctx.registry, crate::modules::attributes::ID)? {
        return Ok(None);
    }
    let index = DeckIndex::load(ctx.reads())?;
    let existing = effective_for_deck(ctx.reads(), &index, Some(parent))?
        .into_iter()
        .map(|(d, _)| d)
        .find(|d| d.template_id.as_deref() == Some(template_id));
    if let Some(def) = existing {
        let mut options = def.options;
        options.sort_by_key(|o| o.order);
        return Ok(Some((def.id, options.iter().map(|o| o.id).collect())));
    }
    Ok(define_template(ctx, template, deck, locale)?
        .map(|a| (a.id, a.options.iter().map(|o| o.id).collect())))
}

/// Installs one starter deck under `parent`: deck, classification, notes with values.
fn install_into(
    ctx: &mut CommandCtx<'_>,
    d: &StarterDeck,
    parent: Option<DeckId>,
    locale: Locale,
) -> AppResult<(DeckId, u32)> {
    let name = free_name(ctx.reads(), parent, d.name.get(locale))?;
    let deck = call::<DecksCreate>(
        ctx,
        &CreateDeck {
            path: name,
            parent_id: parent,
            description: Some(d.description.get(locale).to_owned()),
            icon: None,
            color: None,
        },
    )?;
    let attr = starter_classification(ctx, d.template, deck.id, parent, locale)?;
    let notetype_id = notetype_id(d.notetype)?;
    let mut created = 0u32;
    for n in d.notes {
        let fields: BTreeMap<String, String> = n
            .fields_in(locale)
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        let attributes = match (&attr, n.option) {
            (Some((attr_id, options)), Some(o)) => options
                .get(o)
                .map(|opt| AttributeAssignment {
                    attr_id: *attr_id,
                    value: Some(AttributeValue::Choice(*opt)),
                })
                .into_iter()
                .collect(),
            _ => vec![],
        };
        call::<NotesCreate>(
            ctx,
            &CreateNote {
                notetype_id,
                deck_id: deck.id,
                fields,
                tags: vec![],
                attributes,
                // A starter deck never fails on a word the user already has.
                allow_duplicate: Some(true),
            },
        )?;
        created += 1;
    }
    Ok((deck.id, created))
}

fn deck_dto(ctx: &mut CommandCtx<'_>, name: String) -> AppResult<DeckDto> {
    call::<DecksCreate>(
        ctx,
        &CreateDeck {
            path: name,
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        },
    )
}

/// The preset named `name` (reused), else a new one with `new_per_day`.
fn preset_for(
    ctx: &mut CommandCtx<'_>,
    name: &str,
    new_per_day: u32,
) -> AppResult<fc_core::PresetId> {
    let existing = ctx
        .reads()
        .list_presets()?
        .into_iter()
        .find(|p| p.name.to_lowercase() == name.to_lowercase());
    if let Some(p) = existing {
        return Ok(p.id);
    }
    let dto: PresetDto = call::<PresetsCreate>(
        ctx,
        &CreatePreset {
            name: name.to_owned(),
            config: Some(PresetConfig {
                new_per_day,
                ..PresetConfig::default()
            }),
        },
    )?;
    Ok(dto.id)
}

fn apply(req: ApplyOnboarding, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<OnboardingResult>> {
    let locale = req.locale.unwrap_or(ctx.locale);
    let p = plan(req.subject);
    let starters: Vec<&StarterDeck> = req
        .starters
        .iter()
        .map(|id| starter_of(id))
        .collect::<AppResult<_>>()?;
    let name = req
        .deck_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(p.deck_name.get(locale))
        .to_owned();
    let deck = deck_dto(ctx, name)?;
    define_template(ctx, p.template, deck.id, locale)?;
    if let Some(preset) = p.preset {
        if module_enabled(ctx.reads(), ctx.registry, crate::modules::presets::ID)? {
            let id = preset_for(ctx, preset.name.get(locale), preset.new_per_day)?;
            call::<PresetsAssign>(
                ctx,
                &AssignPreset {
                    deck_ids: vec![deck.id],
                    preset_id: Some(id),
                },
            )?;
        }
    }
    let mut starter_deck_ids = Vec::new();
    let mut notes_created = 0;
    for d in starters {
        // A starter deck of the subject's kind goes under its deck (inherits the template
        // there too); the others (the app guide) stay at the top level.
        let parent = (d.template.is_some() && d.template == p.template).then_some(deck.id);
        let (id, n) = install_into(ctx, d, parent, locale)?;
        starter_deck_ids.push(id);
        notes_created += n;
    }
    mark_done(ctx)?;
    Ok(CommandOutcome {
        response: OnboardingResult {
            deck_id: deck.id,
            starter_deck_ids,
            notes_created,
        },
        inverse: vec![],
        summary: summary::<OnboardingApply>(),
        events: vec![],
    })
}

fn skip(_: Empty, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<OnboardingState>> {
    mark_done(ctx)?;
    Ok(CommandOutcome {
        response: state(ctx.reads(), ctx.now)?,
        inverse: vec![],
        summary: summary::<OnboardingSkip>(),
        events: vec![],
    })
}

fn install(req: InstallStarter, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<StarterInstalled>> {
    let d = starter_of(&req.id)?;
    let locale = req.locale.unwrap_or(ctx.locale);
    let (deck_id, notes_created) = install_into(ctx, d, req.parent_id, locale)?;
    Ok(CommandOutcome {
        response: StarterInstalled {
            deck_id,
            notes_created,
        },
        inverse: vec![],
        summary: summary::<StarterInstall>(),
        events: vec![],
    })
}
