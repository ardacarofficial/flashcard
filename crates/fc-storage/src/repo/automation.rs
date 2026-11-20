//! Automation rules and their run log (v0.9, migration `0850_automation`).

use super::*;
use fc_core::AutomationRuleId;

/// One `automation_rules` row. Trigger and actions are JSON the app owns.
#[derive(Debug, Clone, PartialEq)]
pub struct AutomationRuleRecord {
    /// Id.
    pub id: AutomationRuleId,
    /// Name.
    pub name: String,
    /// Enabled.
    pub enabled: bool,
    /// Run order.
    pub position: u32,
    /// Trigger (JSON object).
    pub trigger: Value,
    /// Canonical condition query ('' = always).
    pub condition: String,
    /// Actions (JSON array).
    pub actions: Value,
    /// Last run (schedules).
    pub last_run_at: Option<Timestamp>,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
    /// Trash time.
    pub deleted_at: Option<Timestamp>,
}

/// One `automation_runs` row.
#[derive(Debug, Clone, PartialEq)]
pub struct AutomationRunRecord {
    /// Log position (assigned on insert; ignored when inserting).
    pub seq: i64,
    /// Rule.
    pub rule_id: AutomationRuleId,
    /// Time.
    pub at: Timestamp,
    /// What started it.
    pub trigger: String,
    /// Chain depth.
    pub depth: u32,
    /// Cards matched.
    pub matched: u32,
    /// Outcome.
    pub status: String,
    /// Oplog batch of the run.
    pub batch_id: Option<BatchId>,
    /// Error (JSON `ApiError`).
    pub error: Option<Value>,
}

/// Automation reads.
pub trait AutomationRead {
    /// Rules by position, then creation; trashed ones only with `include_deleted`.
    fn list_automation_rules(
        &self,
        include_deleted: bool,
    ) -> StorageResult<Vec<AutomationRuleRecord>>;
    /// One rule (trashed too).
    fn get_automation_rule(
        &self,
        id: AutomationRuleId,
    ) -> StorageResult<Option<AutomationRuleRecord>>;
    /// The newest `limit` runs (of `rule` only, if given), newest first.
    fn list_automation_runs(
        &self,
        rule: Option<AutomationRuleId>,
        limit: u32,
    ) -> StorageResult<Vec<AutomationRunRecord>>;
}

/// Automation writes.
pub trait AutomationWrite: AutomationRead {
    /// Inserts or replaces a rule row (all columns).
    fn upsert_automation_rule(&mut self, rule: &AutomationRuleRecord) -> StorageResult<()>;
    /// Removes a rule row and its runs (the inverse of a create). Returns whether it existed.
    fn delete_automation_rule(&mut self, id: AutomationRuleId) -> StorageResult<bool>;
    /// Sets a rule's `last_run_at` (bookkeeping; nothing else changes).
    fn set_automation_rule_last_run(
        &mut self,
        id: AutomationRuleId,
        at: Timestamp,
    ) -> StorageResult<()>;
    /// Appends a run and trims the log to the newest `keep` runs. Returns its `seq`.
    fn insert_automation_run(
        &mut self,
        run: &AutomationRunRecord,
        keep: u32,
    ) -> StorageResult<i64>;
}
