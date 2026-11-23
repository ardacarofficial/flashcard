//! `presets` module (v0.4, docs/02 §10 "Deste ayar şablonları"): named study option sets
//! applied to decks.
//!
//! A deck uses its own preset, else the nearest ancestor's, else the built-in defaults
//! ([`effective`] — **the one function the study engine (v0.2) calls** to read a deck's new/day,
//! reviews/day, steps, desired retention, FSRS weights, burying and order). Every command has an
//! exact inverse: delete is undone by the internal `presets.restore` (same id, decks pointed at
//! it again), assign by assigns of the previous presets.

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::support::decks::DeckIndex;
use crate::support::{err_id, summary};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::presets::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::preset::{normalize_preset_name, Preset, PresetConfig, PresetConfigError};
use fc_core::query::text::fold;
use fc_core::{DeckId, PresetId};
use fc_storage::Reads;
use std::collections::BTreeMap;

/// Module id.
pub const ID: &str = "presets";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.presets.name",
        description_key: "modules.presets.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![main_menu("presets.editor", "nav.presets", "sliders", "/presets", 15)],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<PresetsList>(list);
    reg.query::<PresetsGet>(get);
    reg.query::<PresetsForDeck>(for_deck);
    reg.command::<PresetsCreate>(create);
    reg.command::<PresetsUpdate>(update);
    reg.command::<PresetsClone>(clone);
    reg.command::<PresetsDelete>(delete);
    reg.command::<PresetsAssign>(assign);
    reg.command::<PresetsRestore>(restore);
}

/// The options `deck` studies with: its own preset, else the nearest ancestor's, else
/// [`PresetConfig::default`]. Disabling the module only hides the editor; assignments keep
/// applying.
pub fn effective(reads: &dyn Reads, deck: DeckId) -> AppResult<EffectivePreset> {
    let index = DeckIndex::load(reads)?;
    index.require(deck)?;
    for id in index.ancestry(deck)?.into_iter().rev() {
        let Some(p) = index.get(id).and_then(|d| d.preset_id) else {
            continue;
        };
        if let Some(preset) = reads.get_preset(p)? {
            return Ok(EffectivePreset {
                deck_id: deck,
                preset_id: Some(p),
                assigned_on: Some(id),
                config: preset.config,
            });
        }
    }
    Ok(EffectivePreset {
        deck_id: deck,
        preset_id: None,
        assigned_on: None,
        config: PresetConfig::default(),
    })
}

// ------------------------------------------------------------------ helpers

fn config_error(e: &PresetConfigError) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationPresetConfig, e.to_string())
            .with_detail("field", e.field)
            .with_detail("reason", e.reason.clone()),
    )
}

/// Validated, with FSRS weights in their stored (FSRS-6) form.
fn checked(config: PresetConfig) -> AppResult<PresetConfig> {
    config.validate().map_err(|e| config_error(&e))?;
    Ok(config.normalized())
}

/// Normalized name, unique among presets (case-insensitive) except `except`.
fn name(reads: &dyn Reads, raw: &str, except: Option<PresetId>) -> AppResult<String> {
    let name = normalize_preset_name(raw).ok_or_else(|| {
        AppError::Api(ApiError::new(
            ErrorCode::ValidationPresetName,
            format!("invalid preset name {raw:?}"),
        ))
    })?;
    // Case-, accent- and Turkish-i-insensitive (`SINAV` = `sınav`).
    let key = fold(&name);
    if reads
        .list_presets()?
        .iter()
        .any(|p| Some(p.id) != except && fold(&p.name) == key)
    {
        return Err(AppError::Api(
            ApiError::new(
                ErrorCode::ConflictPresetName,
                format!("preset {name:?} already exists"),
            )
            .with_detail("name", name),
        ));
    }
    Ok(name)
}

fn require(reads: &dyn Reads, id: PresetId) -> AppResult<Preset> {
    reads
        .get_preset(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundPreset, format!("preset {id} not found"), id))
}

fn dto(index: &DeckIndex, p: Preset) -> PresetDto {
    let mut decks: Vec<(String, DeckId)> = index
        .live()
        .filter(|d| d.preset_id == Some(p.id))
        .map(|d| (index.path(d.id).to_lowercase(), d.id))
        .collect();
    decks.sort();
    PresetDto {
        id: p.id,
        name: p.name,
        config: p.config,
        deck_ids: decks.into_iter().map(|(_, id)| id).collect(),
        created_at: p.created_at,
        updated_at: p.updated_at,
    }
}

fn changed(index: &DeckIndex, decks: &[DeckId]) -> Vec<DomainEvent> {
    let mut ev = vec![DomainEvent::PresetsChanged];
    ev.extend(
        decks
            .iter()
            .filter_map(|id| index.get(*id))
            .map(|d| DomainEvent::DeckUpdated { deck: index.dto(d) }),
    );
    ev
}

/// Points `decks` at `preset` (deck rows only); returns the decks that changed with their
/// previous preset.
fn set_decks(
    ctx: &mut CommandCtx<'_>,
    decks: &[DeckId],
    preset: Option<PresetId>,
) -> AppResult<Vec<(DeckId, Option<PresetId>)>> {
    let mut out = Vec::new();
    for id in decks {
        let Some(mut d) = ctx.reads().get_deck(*id)? else {
            continue;
        };
        if d.preset_id == preset || out.iter().any(|(x, _)| x == id) {
            continue;
        }
        out.push((d.id, d.preset_id));
        d.preset_id = preset;
        d.updated_at = ctx.now;
        ctx.tx.update_deck(&d)?;
    }
    Ok(out)
}

// ------------------------------------------------------------------ queries

fn list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<PresetList> {
    let index = DeckIndex::load(ctx.tx)?;
    let mut presets = ctx.tx.list_presets()?;
    presets.sort_by(|a, b| fold(&a.name).cmp(&fold(&b.name)).then(a.id.cmp(&b.id)));
    Ok(PresetList {
        items: presets.into_iter().map(|p| dto(&index, p)).collect(),
        defaults: PresetConfig::default(),
    })
}

fn get(req: PresetRef, ctx: &QueryCtx<'_>) -> AppResult<PresetDto> {
    let index = DeckIndex::load(ctx.tx)?;
    Ok(dto(&index, require(ctx.tx, req.id)?))
}

fn for_deck(req: PresetForDeck, ctx: &QueryCtx<'_>) -> AppResult<EffectivePreset> {
    effective(ctx.tx, req.deck_id)
}

// ------------------------------------------------------------------ commands

fn insert<M: fc_api::ApiMethod>(
    ctx: &mut CommandCtx<'_>,
    name: String,
    config: PresetConfig,
) -> AppResult<CommandOutcome<PresetDto>> {
    let p = Preset {
        id: PresetId::generate(ctx.ids),
        name,
        config,
        created_at: ctx.now,
        updated_at: ctx.now,
    };
    ctx.tx.upsert_preset(&p)?;
    let index = DeckIndex::load(ctx.reads())?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<PresetsDelete>(&PresetRef { id: p.id })?],
        summary: summary::<M>().param("name", &p.name),
        events: vec![DomainEvent::PresetsChanged],
        response: dto(&index, p),
    })
}

fn create(req: CreatePreset, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<PresetDto>> {
    let name = name(ctx.reads(), &req.name, None)?;
    let config = checked(req.config.unwrap_or_default())?;
    insert::<PresetsCreate>(ctx, name, config)
}

fn clone(req: ClonePreset, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<PresetDto>> {
    let source = require(ctx.reads(), req.id)?;
    let name = name(ctx.reads(), &req.name, None)?;
    insert::<PresetsClone>(ctx, name, source.config)
}

fn update(req: UpdatePreset, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<PresetDto>> {
    let old = require(ctx.reads(), req.id)?;
    let p = Preset {
        name: name(ctx.reads(), &req.name, Some(old.id))?,
        config: checked(req.config)?,
        updated_at: ctx.now,
        ..old.clone()
    };
    ctx.tx.upsert_preset(&p)?;
    let index = DeckIndex::load(ctx.reads())?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<PresetsUpdate>(&UpdatePreset {
            id: old.id,
            name: old.name,
            config: old.config,
        })?],
        summary: summary::<PresetsUpdate>().param("name", &p.name),
        events: vec![DomainEvent::PresetsChanged],
        response: dto(&index, p),
    })
}

fn delete(req: PresetRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<PresetDto>> {
    let old = require(ctx.reads(), req.id)?;
    let before = DeckIndex::load(ctx.reads())?;
    let response = dto(&before, old.clone());
    // Every deck pointing at it, trashed ones included (they may come back).
    let decks: Vec<DeckId> = ctx
        .reads()
        .list_decks(true)?
        .into_iter()
        .filter(|d| d.preset_id == Some(old.id))
        .map(|d| d.id)
        .collect();
    set_decks(ctx, &decks, None)?;
    ctx.tx.delete_preset(old.id)?;
    let index = DeckIndex::load(ctx.reads())?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<PresetsRestore>(&RestorePreset {
            preset: PresetDto {
                id: old.id,
                name: old.name.clone(),
                config: old.config.clone(),
                deck_ids: Vec::new(),
                created_at: old.created_at,
                updated_at: old.updated_at,
            },
            deck_ids: decks.clone(),
        })?],
        summary: summary::<PresetsDelete>().param("name", &old.name),
        events: changed(&index, &decks),
        response,
    })
}

fn restore(req: RestorePreset, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<PresetDto>> {
    let d = req.preset;
    let p = Preset {
        id: d.id,
        name: d.name,
        config: d.config,
        created_at: d.created_at,
        updated_at: d.updated_at,
    };
    ctx.tx.upsert_preset(&p)?;
    set_decks(ctx, &req.deck_ids, Some(p.id))?;
    let index = DeckIndex::load(ctx.reads())?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<PresetsDelete>(&PresetRef { id: p.id })?],
        summary: summary::<PresetsRestore>().param("name", &p.name),
        events: changed(&index, &req.deck_ids),
        response: dto(&index, p),
    })
}

fn assign(req: AssignPreset, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AssignResult>> {
    if let Some(p) = req.preset_id {
        require(ctx.reads(), p)?;
    }
    let index = DeckIndex::load(ctx.reads())?;
    for id in &req.deck_ids {
        index.require_live(*id)?;
    }
    let previous = set_decks(ctx, &req.deck_ids, req.preset_id)?;
    // Inverse: one assign per previous preset.
    let mut by_old: BTreeMap<Option<PresetId>, Vec<DeckId>> = BTreeMap::new();
    for (deck, old) in &previous {
        by_old.entry(*old).or_default().push(*deck);
    }
    let inverse = by_old
        .into_iter()
        .map(|(preset_id, deck_ids)| {
            inverse_call::<PresetsAssign>(&AssignPreset {
                deck_ids,
                preset_id,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let decks: Vec<DeckId> = previous.iter().map(|(d, _)| *d).collect();
    let index = DeckIndex::load(ctx.reads())?;
    Ok(CommandOutcome {
        response: AssignResult {
            changed: u32::try_from(decks.len()).unwrap_or(u32::MAX),
        },
        inverse,
        summary: summary::<PresetsAssign>().param("count", decks.len()),
        events: if decks.is_empty() {
            Vec::new()
        } else {
            changed(&index, &decks)
        },
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::testing::{MemStore, TempSqlite, TestApp};
    use fc_api::v1::decks::{CreateDeck, DeckRef};
    use fc_api::v1::modules::SetModuleEnabled;
    use fc_core::preset::{NewCardOrder, ReviewOrder};
    use fc_storage::{SqliteStore, Store};
    use std::sync::Arc;

    type Factory = fn() -> Arc<dyn Store>;

    macro_rules! suite {
        ($($name:ident),* $(,)?) => {
            mod mem {
                $( #[test] fn $name() { super::$name(|| std::sync::Arc::new(super::MemStore::new())); } )*
            }
            mod sqlite {
                $( #[test] fn $name() {
                    super::$name(|| std::sync::Arc::new(super::SqliteStore::open_in_memory().unwrap()));
                } )*
            }
            mod sqlite_file {
                $( #[test] fn $name() { super::$name(|| std::sync::Arc::new(super::TempSqlite::new())); } )*
            }
        };
    }

    suite!(
        crud_validation_and_undo,
        assignment_inheritance_delete_and_undo,
        module_can_be_disabled
    );

    fn deck(t: &TestApp, path: &str) -> DeckId {
        t.ok::<DecksCreate>(CreateDeck {
            path: path.into(),
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        })
        .id
    }

    fn undo(t: &TestApp) {
        t.ok::<OplogUndo>(Empty {});
    }

    fn redo(t: &TestApp) {
        t.ok::<OplogRedo>(Empty {});
    }

    fn names(t: &TestApp) -> Vec<String> {
        t.ok::<PresetsList>(Empty {})
            .items
            .into_iter()
            .map(|p| p.name)
            .collect()
    }

    fn crud_validation_and_undo(f: Factory) {
        let t = TestApp::new(f());
        let list = t.ok::<PresetsList>(Empty {});
        assert!(list.items.is_empty());
        assert_eq!(list.defaults, PresetConfig::default());

        let exam = t.ok::<PresetsCreate>(CreatePreset {
            name: "  Sınav   haftası ".into(),
            config: None,
        });
        assert_eq!(exam.name, "Sınav haftası");
        assert_eq!(exam.config, PresetConfig::default());

        // Validation happens in Rust, with the offending field named.
        let bad = PresetConfig {
            desired_retention: 0.5,
            ..PresetConfig::default()
        };
        let e = t.err::<PresetsUpdate>(UpdatePreset {
            id: exam.id,
            name: "Sınav haftası".into(),
            config: bad,
        });
        assert_eq!(e.code, ErrorCode::ValidationPresetConfig);
        assert_eq!(e.details["field"], "desired_retention");
        let e = t.err::<PresetsCreate>(CreatePreset {
            name: "SINAV HAFTASI".into(),
            config: None,
        });
        assert_eq!(e.code, ErrorCode::ConflictPresetName);
        let e = t.err::<PresetsCreate>(CreatePreset {
            name: " ".into(),
            config: None,
        });
        assert_eq!(e.code, ErrorCode::ValidationPresetName);

        let cfg = PresetConfig {
            new_per_day: 50,
            reviews_per_day: 500,
            learning_steps_mins: vec![1, 10, 60],
            desired_retention: 0.95,
            // FSRS-5 weights (the reference defaults): stored upgraded to FSRS-6 (v1.4.1).
            fsrs_weights: vec![
                0.40255, 1.18385, 3.173, 15.69105, 7.1949, 0.5345, 1.4604, 0.0046, 1.54575,
                0.1192, 1.01925, 1.9395, 0.11, 0.29605, 2.2698, 0.2315, 2.9898, 0.51655, 0.6621,
            ],
            bury_siblings: true,
            new_order: NewCardOrder::Random,
            review_order: ReviewOrder::Retrievability,
            ..PresetConfig::default()
        };
        let upd = t.ok::<PresetsUpdate>(UpdatePreset {
            id: exam.id,
            name: "Yoğun".into(),
            config: cfg.clone(),
        });
        let cfg = cfg.normalized();
        assert_eq!(cfg.fsrs_weights.len(), 21);
        assert_eq!((upd.name.as_str(), &upd.config), ("Yoğun", &cfg));
        let copy = t.ok::<PresetsClone>(ClonePreset {
            id: exam.id,
            name: "Yoğun 2".into(),
        });
        assert_eq!(copy.config, cfg);
        assert_ne!(copy.id, exam.id);
        assert_eq!(names(&t), ["Yoğun", "Yoğun 2"]);

        undo(&t); // clone
        assert_eq!(names(&t), ["Yoğun"]);
        undo(&t); // update
        let back = t.ok::<PresetsGet>(PresetRef { id: exam.id });
        assert_eq!((back.name.as_str(), back.config), ("Sınav haftası", PresetConfig::default()));
        redo(&t);
        assert_eq!(t.ok::<PresetsGet>(PresetRef { id: exam.id }).config, cfg);
        undo(&t);
        undo(&t); // create
        assert!(names(&t).is_empty());
        redo(&t); // same id comes back
        assert_eq!(t.ok::<PresetsGet>(PresetRef { id: exam.id }).name, "Sınav haftası");
    }

    fn assignment_inheritance_delete_and_undo(f: Factory) {
        let t = TestApp::new(f());
        let lang = deck(&t, "Dil");
        let en = deck(&t, "Dil::İngilizce");
        let de = deck(&t, "Dil::Almanca");
        let other = deck(&t, "YKS");
        let eff = |t: &TestApp, d: DeckId| t.ok::<PresetsForDeck>(PresetForDeck { deck_id: d });

        // No preset anywhere: built-in defaults.
        let e = eff(&t, en);
        assert_eq!((e.preset_id, e.assigned_on), (None, None));
        assert_eq!(e.config, PresetConfig::default());

        let cfg = PresetConfig {
            desired_retention: 0.85,
            ..PresetConfig::default()
        };
        let calm = t.ok::<PresetsCreate>(CreatePreset {
            name: "Sakin".into(),
            config: Some(cfg.clone()),
        });
        let fast = t.ok::<PresetsCreate>(CreatePreset {
            name: "Hızlı".into(),
            config: None,
        });
        let r = t.ok::<PresetsAssign>(AssignPreset {
            deck_ids: vec![lang, other],
            preset_id: Some(calm.id),
        });
        assert_eq!(r.changed, 2);
        // Sub-decks inherit from the nearest ancestor with a preset.
        let e = eff(&t, en);
        assert_eq!((e.preset_id, e.assigned_on), (Some(calm.id), Some(lang)));
        assert_eq!(e.config.desired_retention, 0.85);
        t.ok::<PresetsAssign>(AssignPreset {
            deck_ids: vec![en, other],
            preset_id: Some(fast.id),
        });
        assert_eq!(eff(&t, en).preset_id, Some(fast.id));
        assert_eq!(eff(&t, de).preset_id, Some(calm.id));
        let listed = t.ok::<PresetsGet>(PresetRef { id: calm.id });
        assert_eq!(listed.deck_ids, vec![lang]);

        // One undo puts every deck back on its own previous preset.
        undo(&t);
        assert_eq!(eff(&t, en).assigned_on, Some(lang));
        assert_eq!(eff(&t, other).preset_id, Some(calm.id));
        redo(&t);
        assert_eq!(eff(&t, other).preset_id, Some(fast.id));

        // Deleting a preset unassigns its decks (trashed ones too); undo restores both.
        t.ok::<DecksDelete>(DeckRef { id: de });
        t.ok::<PresetsAssign>(AssignPreset {
            deck_ids: vec![lang],
            preset_id: Some(fast.id),
        });
        let e = t.err::<PresetsAssign>(AssignPreset {
            deck_ids: vec![de],
            preset_id: Some(fast.id),
        });
        assert_eq!(e.code, ErrorCode::ConflictDeleted);
        t.ok::<PresetsDelete>(PresetRef { id: fast.id });
        assert_eq!(eff(&t, en).preset_id, None);
        assert_eq!(names(&t), ["Sakin"]);
        undo(&t);
        assert_eq!(eff(&t, en).preset_id, Some(fast.id));
        assert_eq!(eff(&t, lang).preset_id, Some(fast.id));
        let back = t.ok::<PresetsGet>(PresetRef { id: fast.id });
        assert_eq!(back.deck_ids, vec![lang, en, other]);
        redo(&t);
        assert_eq!(names(&t), ["Sakin"]);

        let e = t.err::<PresetsAssign>(AssignPreset {
            deck_ids: vec![lang],
            preset_id: Some(fast.id),
        });
        assert_eq!(e.code, ErrorCode::NotFoundPreset);
        // The internal inverse is not callable from outside.
        let e = t.err::<PresetsRestore>(RestorePreset {
            preset: back,
            deck_ids: vec![],
        });
        assert_eq!(e.code, ErrorCode::PermissionDenied);
    }

    fn module_can_be_disabled(f: Factory) {
        let t = TestApp::new(f());
        t.ok::<ModulesSetEnabled>(SetModuleEnabled {
            id: ID.into(),
            enabled: false,
        });
        let e = t.err::<PresetsList>(Empty {});
        assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
        let modules = t.ok::<ModulesList>(Empty {}).modules;
        let m = modules.iter().find(|m| m.id == ID).unwrap();
        assert!(!m.enabled && m.ui.is_empty());
    }
}
