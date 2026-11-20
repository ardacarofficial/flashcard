//! `automation` module (v0.9, docs/02 §23): "**when** (trigger) + **if** (query) + **do**
//! (commands)" rules.
//!
//! * **Rules** (`automation_rules`, migration 0850) are user data: every command is undoable
//!   (inverse: internal `automation.rules.restore`, exact rows). The condition is a query of the
//!   v0.4 language (classifications included: `seviye:A1 lapses>=5`), stored canonically so
//!   renaming a classification or option keeps it working.
//! * **Actions** are registry commands from [`ACTIONS`]: public, undoable and replayable
//!   ("undo-safe": no files, no jobs, nothing outside the database), each taking the matched
//!   cards (or their notes) as its selection. They run as [`Actor::Automation`] through
//!   [`ensure_invocable`] — the automation actor holds [`AUTOMATION_PERMISSIONS`] and reaches
//!   only [`ACTIONS`] ([`action_reachable`], checked in `registry::ensure_access`).
//! * **Runs** ([`engine`]): after a commit the app observes what happened (notes added or
//!   edited, cards answered or flagged, classification values changed, an import finished),
//!   and every enabled rule whose trigger accepts it and whose condition matches some of the
//!   cards it touched runs, in order (internal `automation.apply`). A run triggered by a change
//!   **continues that change's batch**: undoing the change (an answer in the study screen, a
//!   note edit…) undoes what its rules did in the same step, and redo replays both. (Should
//!   anything else commit in between, the run is its own undo step.) Scheduled and manual runs
//!   are their own undo step. Rules a run triggers run after it, under a
//!   [`fc_core::automation::LoopGuard`] (cycle detection, depth limit, budget per source
//!   change). Scheduled rules run from [`engine::run_due`]. Every run is logged
//!   (`automation_runs`, the newest [`KEEP_RUNS`]).
//! * **Preview** (`automation.preview`): a command (`automation:write`: it takes the writer)
//!   that is a dry run — its transaction is always rolled back.
//! * **Who manages rules:** only the user's own clients ([`ensure_manager`]: `User`, `Cli`,
//!   `System`), whatever permissions another actor holds — a rule acts later with the
//!   automation actor's authority, so creating one must not lend it to plugins, MCP, local API
//!   clients or rules.
//!
//! Switching the module off stops every rule (nothing is observed, `automation.apply` is
//! refused) and hides the screen.

pub mod engine;
mod inbox;
#[cfg(test)]
mod tests;

pub use engine::{run_due, spawn_scheduler, MAX_CARDS_PER_RUN};

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::support::{err_id, from_json, summary};
use fc_api::v1::automation::*;
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::automation::{RuleTrigger, MAX_ACTIONS, MAX_NAME_CHARS};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::{AutomationRuleId, Timestamp};
use fc_storage::repo::AutomationRuleRecord;
use fc_storage::Reads;
use serde_json::Value;

#[cfg(doc)]
use crate::registry::ensure_invocable;
use fc_api::v1::oplog::Actor;

/// Module id.
pub const ID: &str = "automation";

/// Setting: longest chain of rules triggering rules.
pub const MAX_DEPTH: &str = "automation.max_depth";
/// Setting: most rule runs one change may start.
pub const MAX_RUNS: &str = "automation.max_runs_per_event";

/// Runs kept in the log.
pub const KEEP_RUNS: u32 = 1000;

/// What the automation actor holds (`Registry::permits`). Narrowed further to [`ACTIONS`].
/// `inbox:write` is for `inbox.accept` (phase 4: rules with the `inbox_added` trigger).
pub const AUTOMATION_PERMISSIONS: &[&str] =
    &["cards:write", "notes:write", "attributes:write", "inbox:write"];

/// Which ids an action takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// `card_ids`: the matched cards.
    Cards,
    /// `note_ids`: the matched cards' notes.
    Notes,
    /// `items`: the matched inbox items, as `[{"id": …}]` (+ the action's `deck_id`, if any,
    /// on each) — rules with the `inbox_added` trigger only (phase 4).
    InboxItems,
}

impl Selection {
    /// Field name in the action's params.
    pub const fn field(self) -> &'static str {
        match self {
            Self::Cards => "card_ids",
            Self::Notes => "note_ids",
            Self::InboxItems => "items",
        }
    }

    /// Whether a rule with `trigger` may use an action taking this selection: inbox rules act
    /// on inbox items, every other rule on cards or notes.
    pub const fn fits(self, trigger: &RuleTrigger) -> bool {
        matches!(self, Self::InboxItems) == trigger.is_inbox()
    }
}

/// The actions a rule may use, in menu order: registry commands that are public, undoable and
/// replayable (tests prove it for every entry), with the selection the rule fills in. A new
/// action is one line here (+ its `automation.action.<method>` label).
pub const ACTIONS: &[(&str, Selection)] = &[
    ("bulk.add_tags", Selection::Notes),
    ("bulk.remove_tags", Selection::Notes),
    ("bulk.set_suspended", Selection::Cards),
    ("bulk.set_buried", Selection::Cards),
    ("bulk.set_flag", Selection::Cards),
    ("bulk.move_cards", Selection::Cards),
    ("bulk.set_attribute", Selection::Cards),
    ("bulk.reset_progress", Selection::Cards),
    // Phase 4: "add note" — accepts the matched inbox items as notes (inbox rules).
    ("inbox.accept", Selection::InboxItems),
];

/// The selection of action `method`, if it is one.
pub fn action_selection(method: &str) -> Option<Selection> {
    ACTIONS
        .iter()
        .find(|(m, _)| *m == method)
        .map(|(_, s)| *s)
}

/// Whether the automation actor may call `method` (unversioned) at all.
pub fn action_reachable(method: &str) -> bool {
    action_selection(method).is_some()
}

fn int_setting(key: &str, min: i64, max: i64, default: i64) -> SettingDef {
    SettingDef {
        key: key.to_owned(),
        kind: SettingKind::Integer {
            min: Some(min),
            max: Some(max),
        },
        default: Value::from(default),
        label_key: format!("settings.{key}.label"),
        description_key: Some(format!("settings.{key}.description")),
        advanced: true,
        scopes: vec![SettingScopeKind::App],
    }
}

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.automation.name",
        description_key: "modules.automation.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![
            int_setting(MAX_DEPTH, 1, 10, 3),
            int_setting(MAX_RUNS, 1, 100, 20),
        ],
        ui: vec![main_menu(
            "automation.screen",
            "nav.automation",
            "zap",
            "/automation",
            85,
        )],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<AutomationRulesList>(list);
    reg.command::<AutomationRulesCreate>(create);
    reg.command::<AutomationRulesUpdate>(update);
    reg.command::<AutomationRulesDelete>(delete);
    reg.command::<AutomationRulesReorder>(reorder);
    reg.command::<AutomationRulesRestore>(restore);
    reg.action::<AutomationRulesRun>(engine::run_by_hand);
    reg.action::<AutomationPreview>(engine::preview);
    reg.query::<AutomationActions>(actions);
    reg.query::<AutomationRunsList>(runs);
    reg.command::<AutomationApply>(engine::apply);
}

/// An integer setting of this module (stored value, else `default`), clamped to `1..=max`.
pub(crate) fn setting(reads: &dyn Reads, key: &str, default: u32, max: u32) -> AppResult<u32> {
    Ok(crate::settings::resolve_stored(reads, &SettingScope::App, key)?
        .and_then(|(v, _)| v.as_u64())
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(default)
        .clamp(1, max))
}

// ------------------------------------------------------------------ helpers

fn rule_error(field: &str, message: impl Into<String>) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationAutomationRule, message).with_detail("field", field),
    )
}

fn action_error(index: usize, method: &str, reason: &str, message: impl Into<String>) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationAutomationRule, message)
            .with_detail("field", "actions")
            .with_detail("index", index)
            .with_detail("method", method)
            .with_detail("reason", reason),
    )
}

pub(crate) fn require(reads: &dyn Reads, id: AutomationRuleId) -> AppResult<AutomationRuleRecord> {
    reads
        .get_automation_rule(id)?
        .filter(|r| r.deleted_at.is_none())
        .ok_or_else(|| {
            err_id(
                ErrorCode::NotFoundAutomationRule,
                format!("automation rule {id} not found"),
                id,
            )
        })
}

pub(crate) fn trigger_of(r: &AutomationRuleRecord) -> AppResult<RuleTrigger> {
    from_json(&r.trigger)
}

pub(crate) fn actions_of(r: &AutomationRuleRecord) -> AppResult<Vec<RuleAction>> {
    from_json(&r.actions)
}

fn to_value<T: serde::Serialize>(v: &T) -> AppResult<Value> {
    serde_json::to_value(v).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
}

fn snapshot(r: &AutomationRuleRecord) -> AppResult<RuleSnapshot> {
    Ok(RuleSnapshot {
        id: r.id,
        name: r.name.clone(),
        enabled: r.enabled,
        position: r.position,
        trigger: trigger_of(r)?,
        condition: r.condition.clone(),
        actions: actions_of(r)?,
        created_at: r.created_at,
        updated_at: r.updated_at,
        deleted_at: r.deleted_at,
    })
}

fn dto(reads: &dyn Reads, r: &AutomationRuleRecord, now: Timestamp) -> AppResult<RuleDto> {
    let condition = crate::modules::stats::facts::rendered_query(
        reads,
        Some(&r.condition).filter(|c| !c.is_empty()).map(String::as_str),
        now,
    )?
    .unwrap_or_default();
    Ok(RuleDto {
        id: r.id,
        name: r.name.clone(),
        enabled: r.enabled,
        position: r.position,
        trigger: trigger_of(r)?,
        condition,
        actions: actions_of(r)?,
        last_run_at: r.last_run_at,
        created_at: r.created_at,
        updated_at: r.updated_at,
    })
}

fn check_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(rule_error("name", "a rule needs a name of 1–100 characters"));
    }
    Ok(name.to_owned())
}

fn check_trigger(t: &RuleTrigger) -> AppResult<()> {
    t.validate()
        .map_err(|field| rule_error(field, format!("invalid trigger parameter {field}")))
}

/// Canonical form of a condition query ('' = always); a query that does not compile is
/// refused with its positioned `VALIDATION_QUERY` error.
pub(crate) fn canonical_condition(
    reads: &dyn Reads,
    text: &str,
    now: Timestamp,
) -> AppResult<String> {
    Ok(crate::modules::stats::facts::canonical_query(reads, Some(text), now, true)?
        .unwrap_or_default())
}

/// A stand-in selection for checking an action's params (never stored, never run).
const PROBE_ID: &str = "01900000-0000-7000-8000-000000000000";

/// The params `action` is called with for `ids` (the selection field set, overriding any).
/// Inbox items become `[{"id": …, "deck_id": …}]`: an action-level `deck_id` is the deck each
/// accepted item goes to when it names none of its own.
pub(crate) fn action_params(action: &RuleAction, selection: Selection, ids: Value) -> Value {
    let mut params = match &action.params {
        Value::Object(m) => m.clone(),
        _ => serde_json::Map::new(),
    };
    let ids = match selection {
        Selection::InboxItems => {
            let deck = params.remove("deck_id");
            let items = ids
                .as_array()
                .map(|ids| {
                    ids.iter()
                        .map(|id| {
                            let mut item = serde_json::Map::new();
                            item.insert("id".into(), id.clone());
                            if let Some(d) = &deck {
                                item.insert("deck_id".into(), d.clone());
                            }
                            Value::Object(item)
                        })
                        .collect()
                })
                .unwrap_or_default();
            Value::Array(items)
        }
        Selection::Cards | Selection::Notes => ids,
    };
    params.insert(selection.field().to_owned(), ids);
    Value::Object(params)
}

/// Checks the actions of a rule: 1–10, each an allow-listed command that fits the trigger's
/// subject ([`Selection::fits`]) and whose params (with a stand-in selection) parse and
/// validate.
pub(crate) fn check_actions(
    registry: &Registry,
    trigger: &RuleTrigger,
    actions: &[RuleAction],
) -> AppResult<()> {
    if actions.is_empty() || actions.len() > MAX_ACTIONS {
        return Err(rule_error("actions", "a rule needs 1–10 actions"));
    }
    for (i, a) in actions.iter().enumerate() {
        let Some(selection) = action_selection(&a.method) else {
            return Err(action_error(
                i,
                &a.method,
                "not_allowed",
                format!("{} is not an automation action", a.method),
            ));
        };
        if !selection.fits(trigger) {
            return Err(action_error(
                i,
                &a.method,
                "trigger",
                format!("{} does not act on what a {} rule matches", a.method, trigger.kind()),
            ));
        }
        if !a.params.is_object() {
            return Err(action_error(i, &a.method, "params", "params must be an object"));
        }
        if a.params.get(selection.field()).is_some() {
            return Err(action_error(
                i,
                &a.method,
                "params",
                format!("{} is filled in by the rule", selection.field()),
            ));
        }
        let entry = registry.entry(&a.method).ok_or_else(|| {
            action_error(i, &a.method, "not_allowed", "unknown method".to_owned())
        })?;
        let probe = action_params(a, selection, Value::from(vec![PROBE_ID]));
        entry
            .handler
            .validate(&probe)
            .map_err(|e| action_error(i, &a.method, "params", e.into_api().message))?;
    }
    Ok(())
}

/// Rules are managed by the user's own clients only (`User`, `Cli`, `System`) — checked on
/// top of the `automation:*` permission, so that no grant to another actor (a plugin, MCP, a
/// local API client, a rule) ever lets it create, change, order, run or preview rules.
pub(crate) fn ensure_manager(actor: &Actor) -> AppResult<()> {
    match actor {
        Actor::User | Actor::Cli | Actor::System => Ok(()),
        other => Err(AppError::Api(
            ApiError::new(
                ErrorCode::PermissionDenied,
                format!("{other:?} cannot manage automation rules"),
            )
            .with_detail("reason", "actor"),
        )),
    }
}

fn changed() -> Vec<DomainEvent> {
    vec![DomainEvent::AutomationChanged]
}

// ------------------------------------------------------------------ queries

fn list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<RuleList> {
    let rules = ctx
        .tx
        .list_automation_rules(false)?
        .iter()
        .map(|r| dto(ctx.tx, r, ctx.now))
        .collect::<AppResult<_>>()?;
    Ok(RuleList { rules })
}

fn actions(_: Empty, _: &QueryCtx<'_>) -> AppResult<ActionList> {
    Ok(ActionList {
        actions: ACTIONS
            .iter()
            .map(|(m, s)| ActionInfo {
                method: (*m).to_owned(),
                selection: s.field().to_owned(),
                label_key: format!("automation.action.{m}"),
            })
            .collect(),
    })
}

fn runs(req: ListRuns, ctx: &QueryCtx<'_>) -> AppResult<RunList> {
    let limit = req.limit.unwrap_or(50).clamp(1, 500);
    let runs = ctx
        .tx
        .list_automation_runs(req.rule_id, limit)?
        .into_iter()
        .map(engine::run_dto)
        .collect();
    Ok(RunList { runs })
}

// ------------------------------------------------------------------ commands

fn create(req: CreateRule, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<RuleDto>> {
    ensure_manager(ctx.actor)?;
    let name = check_name(&req.name)?;
    check_trigger(&req.trigger)?;
    check_actions(ctx.registry, &req.trigger, &req.actions)?;
    let condition = canonical_condition(ctx.reads(), &req.condition, ctx.now)?;
    let position = ctx
        .reads()
        .list_automation_rules(false)?
        .iter()
        .map(|r| r.position + 1)
        .max()
        .unwrap_or(0);
    let rule = AutomationRuleRecord {
        id: AutomationRuleId::generate(ctx.ids),
        name,
        enabled: req.enabled.unwrap_or(true),
        position,
        trigger: to_value(&req.trigger)?,
        condition,
        actions: to_value(&req.actions)?,
        last_run_at: None,
        created_at: ctx.now,
        updated_at: ctx.now,
        deleted_at: None,
    };
    ctx.tx.upsert_automation_rule(&rule)?;
    Ok(CommandOutcome {
        response: dto(ctx.reads(), &rule, ctx.now)?,
        inverse: vec![inverse_call::<AutomationRulesRestore>(&RestoreRules {
            rules: Vec::new(),
            remove: vec![rule.id],
        })?],
        summary: summary::<AutomationRulesCreate>().param("name", &rule.name),
        events: changed(),
    })
}

fn update(req: UpdateRule, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<RuleDto>> {
    ensure_manager(ctx.actor)?;
    let before = require(ctx.reads(), req.id)?;
    let mut rule = before.clone();
    if let Some(name) = &req.name {
        rule.name = check_name(name)?;
    }
    if let Some(enabled) = req.enabled {
        rule.enabled = enabled;
    }
    if let Some(t) = &req.trigger {
        check_trigger(t)?;
        rule.trigger = to_value(t)?;
    }
    if let Some(c) = &req.condition {
        rule.condition = canonical_condition(ctx.reads(), c, ctx.now)?;
    }
    if let Some(a) = &req.actions {
        rule.actions = to_value(a)?;
    }
    if req.trigger.is_some() || req.actions.is_some() {
        // A new trigger must fit the kept actions and vice versa.
        check_actions(ctx.registry, &trigger_of(&rule)?, &actions_of(&rule)?)?;
    }
    let mut inverse = Vec::new();
    if rule != before {
        rule.updated_at = ctx.now;
        ctx.tx.upsert_automation_rule(&rule)?;
        inverse.push(inverse_call::<AutomationRulesRestore>(&RestoreRules {
            rules: vec![snapshot(&before)?],
            remove: Vec::new(),
        })?);
    }
    Ok(CommandOutcome {
        response: dto(ctx.reads(), &rule, ctx.now)?,
        events: if inverse.is_empty() {
            Vec::new()
        } else {
            changed()
        },
        inverse,
        summary: summary::<AutomationRulesUpdate>().param("name", &rule.name),
    })
}

fn delete(req: RuleRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    ensure_manager(ctx.actor)?;
    let before = require(ctx.reads(), req.id)?;
    let rule = AutomationRuleRecord {
        deleted_at: Some(ctx.now),
        updated_at: ctx.now,
        ..before.clone()
    };
    ctx.tx.upsert_automation_rule(&rule)?;
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![inverse_call::<AutomationRulesRestore>(&RestoreRules {
            rules: vec![snapshot(&before)?],
            remove: Vec::new(),
        })?],
        summary: summary::<AutomationRulesDelete>().param("name", &rule.name),
        events: changed(),
    })
}

fn reorder(req: ReorderRules, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<RuleList>> {
    ensure_manager(ctx.actor)?;
    let live = ctx.reads().list_automation_rules(false)?;
    let mut given = req.ids.clone();
    given.sort();
    given.dedup();
    let mut ids: Vec<AutomationRuleId> = live.iter().map(|r| r.id).collect();
    ids.sort();
    if given != ids || req.ids.len() != live.len() {
        return Err(rule_error("ids", "reorder lists every live rule exactly once"));
    }
    let mut before = Vec::new();
    for (position, id) in req.ids.iter().enumerate() {
        let Some(r) = live.iter().find(|r| r.id == *id) else {
            continue;
        };
        let position = u32::try_from(position).unwrap_or(u32::MAX);
        if r.position != position {
            before.push(snapshot(r)?);
            ctx.tx.upsert_automation_rule(&AutomationRuleRecord {
                position,
                updated_at: ctx.now,
                ..r.clone()
            })?;
        }
    }
    let rules = ctx
        .reads()
        .list_automation_rules(false)?
        .iter()
        .map(|r| dto(ctx.reads(), r, ctx.now))
        .collect::<AppResult<_>>()?;
    let inverse = if before.is_empty() {
        Vec::new()
    } else {
        vec![inverse_call::<AutomationRulesRestore>(&RestoreRules {
            rules: before,
            remove: Vec::new(),
        })?]
    };
    Ok(CommandOutcome {
        response: RuleList { rules },
        events: if inverse.is_empty() {
            Vec::new()
        } else {
            changed()
        },
        inverse,
        summary: summary::<AutomationRulesReorder>(),
    })
}

/// Writes rows back exactly (keeping each row's current `last_run_at`: schedule bookkeeping is
/// not history) and removes created ones; its own inverse is read from the current rows.
fn restore(req: RestoreRules, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let mut inv = RestoreRules {
        rules: Vec::new(),
        remove: Vec::new(),
    };
    for s in &req.rules {
        let current = ctx.reads().get_automation_rule(s.id)?;
        match &current {
            Some(c) => inv.rules.push(snapshot(c)?),
            None => inv.remove.push(s.id),
        }
        ctx.tx.upsert_automation_rule(&AutomationRuleRecord {
            id: s.id,
            name: s.name.clone(),
            enabled: s.enabled,
            position: s.position,
            trigger: to_value(&s.trigger)?,
            condition: s.condition.clone(),
            actions: to_value(&s.actions)?,
            last_run_at: current.and_then(|c| c.last_run_at),
            created_at: s.created_at,
            updated_at: s.updated_at,
            deleted_at: s.deleted_at,
        })?;
    }
    for id in &req.remove {
        if let Some(c) = ctx.reads().get_automation_rule(*id)? {
            inv.rules.push(snapshot(&c)?);
            ctx.tx.delete_automation_rule(*id)?;
        }
    }
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![inverse_call::<AutomationRulesRestore>(&inv)?],
        summary: summary::<AutomationRulesRestore>(),
        events: changed(),
    })
}
