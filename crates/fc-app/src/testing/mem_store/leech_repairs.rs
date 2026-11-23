//! [`MemTx`] repository traits: leech repairs (v1.4 W3).

use super::*;
use fc_storage::repo::LeechRepairRecord;

/// Kinds the `leech_repairs` table accepts (its `CHECK`).
const KINDS: &[&str] = &[
    "switch_example",
    "add_accepted_answers",
    "move_meaning",
    "ai_rewrite",
];

/// `leech_repairs`.
#[derive(Debug, Clone, Default)]
pub struct LeechRepairState {
    rows: Vec<LeechRepairRecord>,
    next: i64,
}

impl LeechRepairRead for MemTx<'_> {
    fn list_leech_repairs(
        &self,
        card_ids: Option<&[CardId]>,
    ) -> StorageResult<Vec<LeechRepairRecord>> {
        let mut rows: Vec<LeechRepairRecord> = self
            .s()
            .domains
            .leech_repairs
            .rows
            .iter()
            .filter(|r| card_ids.is_none_or(|ids| ids.contains(&r.card_id)))
            .cloned()
            .collect();
        rows.sort_by_key(|r| (r.applied_at, r.id));
        Ok(rows)
    }

    fn get_leech_repair(&self, id: i64) -> StorageResult<Option<LeechRepairRecord>> {
        let rows = &self.s().domains.leech_repairs.rows;
        Ok(rows.iter().find(|r| r.id == id).cloned())
    }
}

impl LeechRepairWrite for MemTx<'_> {
    fn insert_leech_repair(&mut self, rec: &LeechRepairRecord) -> StorageResult<i64> {
        if !KINDS.contains(&rec.kind.as_str()) {
            return Err(StorageError::Constraint {
                constraint: "leech_repairs.kind".into(),
            });
        }
        let st = &mut self.m().domains.leech_repairs;
        let id = if rec.id > 0 {
            if st.rows.iter().any(|r| r.id == rec.id) {
                return Err(StorageError::Constraint {
                    constraint: "leech_repairs.id".into(),
                });
            }
            st.next = st.next.max(rec.id);
            rec.id
        } else {
            st.next += 1;
            st.next
        };
        st.rows.push(LeechRepairRecord {
            id,
            ..rec.clone()
        });
        Ok(id)
    }

    fn delete_leech_repair(&mut self, id: i64) -> StorageResult<()> {
        self.m().domains.leech_repairs.rows.retain(|r| r.id != id);
        Ok(())
    }
}
