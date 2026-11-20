//! [`MemTx`] repository traits: automation rules and their run log (v0.9).

use super::*;
use fc_core::AutomationRuleId;
use fc_storage::repo::{AutomationRuleRecord, AutomationRunRecord};

/// `automation_rules`, `automation_runs`.
#[derive(Debug, Clone, Default)]
pub struct AutomationState {
    rules: BTreeMap<AutomationRuleId, AutomationRuleRecord>,
    runs: Vec<AutomationRunRecord>,
    next_seq: i64,
}

impl AutomationRead for MemTx<'_> {
    fn list_automation_rules(
        &self,
        include_deleted: bool,
    ) -> StorageResult<Vec<AutomationRuleRecord>> {
        let mut v: Vec<AutomationRuleRecord> = self
            .s()
            .domains
            .automation
            .rules
            .values()
            .filter(|r| include_deleted || r.deleted_at.is_none())
            .cloned()
            .collect();
        v.sort_by(|a, b| {
            (a.position, a.created_at, a.id).cmp(&(b.position, b.created_at, b.id))
        });
        Ok(v)
    }

    fn get_automation_rule(
        &self,
        id: AutomationRuleId,
    ) -> StorageResult<Option<AutomationRuleRecord>> {
        Ok(self.s().domains.automation.rules.get(&id).cloned())
    }

    fn list_automation_runs(
        &self,
        rule: Option<AutomationRuleId>,
        limit: u32,
    ) -> StorageResult<Vec<AutomationRunRecord>> {
        Ok(self
            .s()
            .domains
            .automation
            .runs
            .iter()
            .rev()
            .filter(|r| rule.is_none_or(|id| r.rule_id == id))
            .take(limit as usize)
            .cloned()
            .collect())
    }
}

impl AutomationWrite for MemTx<'_> {
    fn upsert_automation_rule(&mut self, rule: &AutomationRuleRecord) -> StorageResult<()> {
        self.m()
            .domains
            .automation
            .rules
            .insert(rule.id, rule.clone());
        Ok(())
    }

    fn delete_automation_rule(&mut self, id: AutomationRuleId) -> StorageResult<bool> {
        let state = &mut self.m().domains.automation;
        state.runs.retain(|r| r.rule_id != id);
        Ok(state.rules.remove(&id).is_some())
    }

    fn set_automation_rule_last_run(
        &mut self,
        id: AutomationRuleId,
        at: Timestamp,
    ) -> StorageResult<()> {
        if let Some(r) = self.m().domains.automation.rules.get_mut(&id) {
            r.last_run_at = Some(at);
        }
        Ok(())
    }

    fn insert_automation_run(
        &mut self,
        run: &AutomationRunRecord,
        keep: u32,
    ) -> StorageResult<i64> {
        let state = &mut self.m().domains.automation;
        state.next_seq += 1;
        let seq = state.next_seq;
        state.runs.push(AutomationRunRecord {
            seq,
            ..run.clone()
        });
        let keep = keep.max(1) as usize;
        if state.runs.len() > keep {
            let excess = state.runs.len() - keep;
            state.runs.drain(..excess);
        }
        Ok(seq)
    }
}
