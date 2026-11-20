//! Automation rules (docs/02 §23, v0.9): **when** (trigger) + **if** (query) + **do**
//! (commands). A rule's actions are registry commands from a fixed, undo-safe allow-list
//! (`automation.actions`); they run as the actor `automation:<rule id>` and each run is one
//! undo step. Rules run in order, with cycle detection, a depth limit and a per-event budget
//! (settings `automation.max_depth`, `automation.max_runs_per_event`); every run is logged.

use crate::error::ApiError;
use fc_core::automation::RuleTrigger;
use fc_core::{AutomationRuleId, BatchId, CardId, NoteId, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// One action: a registry command and its parameters **without** the selection field
/// (`card_ids` / `note_ids`), which the rule fills with the matched cards (or their notes).
/// Example: `{"method": "bulk.add_tags", "params": {"tags": ["leech"]}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RuleAction {
    /// Unversioned method name (one of `automation.actions`).
    pub method: String,
    /// Parameters except the selection (`{}` if none).
    #[serde(default = "empty_object")]
    #[ts(type = "Record<string, unknown>")]
    pub params: Value,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// A rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RuleDto {
    /// Id.
    pub id: AutomationRuleId,
    /// Name.
    pub name: String,
    /// Runs when triggered (a disabled rule only runs by hand or in a preview).
    pub enabled: bool,
    /// Run order (rules run by position).
    pub position: u32,
    /// When.
    pub trigger: RuleTrigger,
    /// If: query with current names (stored canonically: renaming a classification or option
    /// keeps it working). Empty = every card the trigger brings.
    pub condition: String,
    /// Do.
    pub actions: Vec<RuleAction>,
    /// Last run (any outcome), for schedules.
    pub last_run_at: Option<Timestamp>,
    /// Created.
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
}

/// `automation.rules.list`: every live rule, by position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RuleList {
    /// Rules.
    pub rules: Vec<RuleDto>,
}

/// `automation.rules.create`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct CreateRule {
    /// Name (1–100 characters).
    pub name: String,
    /// Enabled (default `true`).
    #[serde(default)]
    pub enabled: Option<bool>,
    /// When.
    pub trigger: RuleTrigger,
    /// If (query; empty = always).
    #[serde(default)]
    pub condition: String,
    /// Do (1–10 actions).
    pub actions: Vec<RuleAction>,
}

/// `automation.rules.update`: the given fields change, the rest stay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct UpdateRule {
    /// Rule.
    pub id: AutomationRuleId,
    /// Name.
    #[serde(default)]
    pub name: Option<String>,
    /// Enabled.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// When.
    #[serde(default)]
    pub trigger: Option<RuleTrigger>,
    /// If.
    #[serde(default)]
    pub condition: Option<String>,
    /// Do.
    #[serde(default)]
    pub actions: Option<Vec<RuleAction>>,
}

/// A rule by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RuleRef {
    /// Rule.
    pub id: AutomationRuleId,
}

/// `automation.rules.reorder`: the new order of **every** live rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ReorderRules {
    /// Rule ids, first runs first.
    pub ids: Vec<AutomationRuleId>,
}

/// Every stored column of a rule (internal `automation.rules.restore`, the inverse of every
/// rule command). The condition is the stored (canonical) text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RuleSnapshot {
    /// Id.
    pub id: AutomationRuleId,
    /// Name.
    pub name: String,
    /// Enabled.
    pub enabled: bool,
    /// Position.
    pub position: u32,
    /// Trigger.
    pub trigger: RuleTrigger,
    /// Canonical condition.
    pub condition: String,
    /// Actions.
    pub actions: Vec<RuleAction>,
    /// Created.
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
    /// Trashed.
    pub deleted_at: Option<Timestamp>,
}

/// Internal `automation.rules.restore`: writes rows back exactly (or removes those created).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreRules {
    /// Rows to write back.
    #[serde(default)]
    pub rules: Vec<RuleSnapshot>,
    /// Rows to remove (the inverse of a create).
    #[serde(default)]
    pub remove: Vec<AutomationRuleId>,
}

/// An action the rule editor offers (`automation.actions`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ActionInfo {
    /// Method.
    pub method: String,
    /// What the rule fills in: `card_ids` or `note_ids`.
    pub selection: String,
    /// i18n key of its label (`automation.action.<method>`).
    pub label_key: String,
}

/// Response of `automation.actions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ActionList {
    /// Actions, in menu order.
    pub actions: Vec<ActionInfo>,
}

/// A rule to preview: a saved one (`rule_id`) or an unsaved draft (`draft`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct PreviewRule {
    /// Saved rule.
    #[serde(default)]
    pub rule_id: Option<AutomationRuleId>,
    /// Unsaved draft (the editor's current state).
    #[serde(default)]
    pub draft: Option<RuleDraft>,
}

/// The parts of a rule a preview needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RuleDraft {
    /// When.
    pub trigger: RuleTrigger,
    /// If.
    #[serde(default)]
    pub condition: String,
    /// Do.
    pub actions: Vec<RuleAction>,
}

/// A matched card in a preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PreviewCard {
    /// Card.
    pub card_id: CardId,
    /// Its note.
    pub note_id: NoteId,
    /// Sort field text of the note.
    pub text: String,
    /// Deck path.
    pub deck: String,
    /// Whether the actions would change it (or its note).
    pub changes: bool,
}

/// Outcome of one action in a preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ActionPreview {
    /// Method.
    pub method: String,
    /// The command's response (e.g. `{changed, skipped}`).
    #[ts(type = "unknown")]
    pub response: Value,
}

/// Response of `automation.preview` (**dry run**): the condition is evaluated over the whole
/// collection, the actions run in a transaction that is always rolled back — nothing is
/// written, no undo step, no event. The rule need not be saved or enabled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RulePreview {
    /// Cards matching the condition.
    pub matched: u32,
    /// Cards that would change (themselves or through their note).
    pub changed: u32,
    /// The first matched cards (at most 100; changing ones first).
    pub cards: Vec<PreviewCard>,
    /// One entry per action.
    pub actions: Vec<ActionPreview>,
    /// More cards matched than one run handles (`automation` runs at most 5000 at a time).
    pub truncated: bool,
}

/// Outcome of one rule run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RunDto {
    /// Log position (newest first in lists).
    pub seq: i64,
    /// Rule.
    pub rule_id: AutomationRuleId,
    /// When.
    pub at: Timestamp,
    /// What started it (`note_added`, `card_answered`, `schedule`, `manual`, …).
    pub trigger: String,
    /// Chain depth (0 = started by a user change, the scheduler or by hand).
    pub depth: u32,
    /// Cards the condition matched.
    pub matched: u32,
    /// `applied`, `no_match` (a scheduled or manual run found no card), `skipped_cycle`,
    /// `skipped_depth`, `skipped_budget` or `failed`.
    pub status: String,
    /// Oplog batch of an applied run that changed something (its undo step).
    pub batch_id: Option<BatchId>,
    /// Why it failed.
    pub error: Option<ApiError>,
}

/// `automation.runs.list`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ListRuns {
    /// Only this rule's runs.
    #[serde(default)]
    pub rule_id: Option<AutomationRuleId>,
    /// At most this many (default 50, at most 500).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Response of `automation.runs.list` (newest first). The log keeps the last 1000 runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RunList {
    /// Runs.
    pub runs: Vec<RunDto>,
}

/// Response of `automation.rules.run`: the run of the rule started by hand (and, after it,
/// those of any rules its actions triggered).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RunResult {
    /// Runs, in order (the first is the rule's own).
    pub runs: Vec<RunDto>,
}

/// Internal `automation.apply`: one rule run (its actions as nested commands of one batch).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ApplyRule {
    /// Rule.
    pub rule_id: AutomationRuleId,
    /// Matched cards.
    pub card_ids: Vec<CardId>,
    /// What started it (log).
    pub trigger: String,
    /// Chain depth (log).
    pub depth: u32,
    /// Run a disabled rule too (manual runs).
    #[serde(default)]
    pub manual: bool,
}

/// Response of `automation.apply`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ApplyResult {
    /// Log position of the run.
    pub seq: i64,
    /// Oplog batch, if the run changed something.
    pub batch_id: Option<BatchId>,
    /// Responses of the actions.
    #[ts(type = "unknown[]")]
    pub results: Vec<Value>,
}
