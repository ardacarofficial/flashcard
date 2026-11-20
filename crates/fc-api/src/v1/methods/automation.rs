//! Automation rules (v0.9, docs/02 §23).

use crate::v1::automation::*;
use crate::v1::common::Empty;

methods! {
    // ---- automation ----
    /// Live rules, by position.
    AutomationRulesList = "automation.rules.list", Query, "automation", "automation:read", Public, undo: false, Empty => RuleList;
    /// Creates a rule (validated: trigger, condition query, allow-listed actions).
    AutomationRulesCreate = "automation.rules.create", Command, "automation", "automation:write", Public, undo: true, CreateRule => RuleDto;
    /// Edits a rule (name, switch, trigger, condition, actions).
    AutomationRulesUpdate = "automation.rules.update", Command, "automation", "automation:write", Public, undo: true, UpdateRule => RuleDto;
    /// Trashes a rule.
    AutomationRulesDelete = "automation.rules.delete", Command, "automation", "automation:write", Public, undo: true, RuleRef => Empty;
    /// Sets the run order of every live rule.
    AutomationRulesReorder = "automation.rules.reorder", Command, "automation", "automation:write", Public, undo: true, ReorderRules => RuleList;
    /// Internal inverse of every rule command: writes rows back exactly.
    AutomationRulesRestore = "automation.rules.restore", Command, "automation", "automation:write", Internal, undo: true, RestoreRules => Empty;
    /// Runs a rule now over every card matching its condition (one undo step).
    AutomationRulesRun = "automation.rules.run", Command, "automation", "automation:write", Public, undo: false, RuleRef => RunResult;
    /// Dry run: which cards a rule (saved or draft) would change; writes nothing.
    AutomationPreview = "automation.preview", Query, "automation", "automation:read", Public, undo: false, PreviewRule => RulePreview;
    /// The actions a rule may use (registry commands, public and undo-safe).
    AutomationActions = "automation.actions", Query, "automation", "automation:read", Public, undo: false, Empty => ActionList;
    /// The run log (newest first).
    AutomationRunsList = "automation.runs.list", Query, "automation", "automation:read", Public, undo: false, ListRuns => RunList;
    /// Internal: one rule run, its actions as nested commands of one batch (their entries are
    /// the undo step; this one records nothing itself and is never an inverse).
    AutomationApply = "automation.apply", Command, "automation", "automation:write", Internal, undo: true (replay: false), ApplyRule => ApplyResult;
}
