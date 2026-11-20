//! `automation_rules`, `automation_runs` (v0.9).

use super::{all, exec, id, json, one, opt_id, opt_ts, ts, Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{AutomationRead, AutomationRuleRecord, AutomationRunRecord, AutomationWrite};
use fc_core::{AutomationRuleId, Timestamp};
use rusqlite::params;

const RULE_COLS: &str = "id, name, enabled, position, trigger, condition, actions, last_run_at,
    created_at, updated_at, deleted_at";

fn rule_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationRuleRecord> {
    Ok(AutomationRuleRecord {
        id: id(r, 0)?,
        name: r.get(1)?,
        enabled: r.get::<_, i64>(2)? != 0,
        position: u32::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
        trigger: json(r, 4)?,
        condition: r.get(5)?,
        actions: json(r, 6)?,
        last_run_at: opt_ts(r, 7)?,
        created_at: ts(r, 8)?,
        updated_at: ts(r, 9)?,
        deleted_at: opt_ts(r, 10)?,
    })
}

const RUN_COLS: &str = "seq, rule_id, at, trigger, depth, matched, status, batch_id, error";

fn run_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationRunRecord> {
    let error: Option<String> = r.get(8)?;
    Ok(AutomationRunRecord {
        seq: r.get(0)?,
        rule_id: id(r, 1)?,
        at: ts(r, 2)?,
        trigger: r.get(3)?,
        depth: u32::try_from(r.get::<_, i64>(4)?).unwrap_or(0),
        matched: u32::try_from(r.get::<_, i64>(5)?).unwrap_or(0),
        status: r.get(6)?,
        batch_id: opt_id(r, 7)?,
        error: error.and_then(|e| serde_json::from_str(&e).ok()),
    })
}

impl<M: Mode> AutomationRead for SqliteTx<'_, M> {
    fn list_automation_rules(
        &self,
        include_deleted: bool,
    ) -> StorageResult<Vec<AutomationRuleRecord>> {
        all(
            self.conn(),
            "automation_rules",
            &format!(
                "SELECT {RULE_COLS} FROM automation_rules
                 WHERE ?1 OR deleted_at IS NULL ORDER BY position, created_at, id"
            ),
            [include_deleted],
            rule_row,
        )
    }

    fn get_automation_rule(
        &self,
        rule: AutomationRuleId,
    ) -> StorageResult<Option<AutomationRuleRecord>> {
        one(
            self.conn(),
            "automation_rules",
            &format!("SELECT {RULE_COLS} FROM automation_rules WHERE id = ?1"),
            [rule.to_string()],
            rule_row,
        )
    }

    fn list_automation_runs(
        &self,
        rule: Option<AutomationRuleId>,
        limit: u32,
    ) -> StorageResult<Vec<AutomationRunRecord>> {
        all(
            self.conn(),
            "automation_runs",
            &format!(
                "SELECT {RUN_COLS} FROM automation_runs
                 WHERE ?1 IS NULL OR rule_id = ?1 ORDER BY seq DESC LIMIT ?2"
            ),
            params![rule.map(|r| r.to_string()), i64::from(limit)],
            run_row,
        )
    }
}

impl AutomationWrite for SqliteTx<'_, WriteMode> {
    fn upsert_automation_rule(&mut self, r: &AutomationRuleRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            "automation_rules",
            "INSERT INTO automation_rules (id, name, enabled, position, trigger, condition,
                 actions, last_run_at, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET name = ?2, enabled = ?3, position = ?4,
                 trigger = ?5, condition = ?6, actions = ?7, last_run_at = ?8,
                 created_at = ?9, updated_at = ?10, deleted_at = ?11",
            params![
                r.id.to_string(),
                r.name,
                i64::from(r.enabled),
                i64::from(r.position),
                r.trigger.to_string(),
                r.condition,
                r.actions.to_string(),
                r.last_run_at.map(|t| t.0),
                r.created_at.0,
                r.updated_at.0,
                r.deleted_at.map(|t| t.0),
            ],
        )?;
        Ok(())
    }

    fn delete_automation_rule(&mut self, rule: AutomationRuleId) -> StorageResult<bool> {
        exec(
            self.conn(),
            "automation_runs",
            "DELETE FROM automation_runs WHERE rule_id = ?1",
            [rule.to_string()],
        )?;
        Ok(exec(
            self.conn(),
            "automation_rules",
            "DELETE FROM automation_rules WHERE id = ?1",
            [rule.to_string()],
        )? > 0)
    }

    fn set_automation_rule_last_run(
        &mut self,
        rule: AutomationRuleId,
        at: Timestamp,
    ) -> StorageResult<()> {
        exec(
            self.conn(),
            "automation_rules",
            "UPDATE automation_rules SET last_run_at = ?2 WHERE id = ?1",
            params![rule.to_string(), at.0],
        )?;
        Ok(())
    }

    fn insert_automation_run(
        &mut self,
        run: &AutomationRunRecord,
        keep: u32,
    ) -> StorageResult<i64> {
        exec(
            self.conn(),
            "automation_runs",
            "INSERT INTO automation_runs (rule_id, at, trigger, depth, matched, status,
                 batch_id, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run.rule_id.to_string(),
                run.at.0,
                run.trigger,
                i64::from(run.depth),
                i64::from(run.matched),
                run.status,
                run.batch_id.map(|b| b.to_string()),
                run.error.as_ref().map(ToString::to_string),
            ],
        )?;
        let seq = self.conn().last_insert_rowid();
        exec(
            self.conn(),
            "automation_runs",
            "DELETE FROM automation_runs WHERE seq <= ?1",
            [seq - i64::from(keep.max(1))],
        )?;
        Ok(seq)
    }
}
