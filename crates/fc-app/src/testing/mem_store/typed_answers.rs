//! [`MemTx`] repository traits: typed answers (v1.4 Z1).

use super::*;
use fc_core::ReviewLogId;
use fc_storage::repo::TypedAnswerRecord;

/// `typed_answers`.
#[derive(Debug, Clone, Default)]
pub struct TypedAnswerState {
    rows: Vec<TypedAnswerRecord>,
    next: i64,
}

impl TypedAnswerRead for MemTx<'_> {
    fn typed_answers_of_card(&self, card_id: CardId) -> StorageResult<Vec<TypedAnswerRecord>> {
        let mut rows: Vec<TypedAnswerRecord> = self
            .s()
            .domains
            .typed_answers
            .rows
            .iter()
            .filter(|r| r.card_id == card_id)
            .cloned()
            .collect();
        rows.sort_by_key(|r| std::cmp::Reverse((r.created_at, r.id)));
        Ok(rows)
    }

    fn typed_answers_of_reviews(
        &self,
        review_ids: &[ReviewLogId],
    ) -> StorageResult<Vec<TypedAnswerRecord>> {
        let mut rows: Vec<TypedAnswerRecord> = self
            .s()
            .domains
            .typed_answers
            .rows
            .iter()
            .filter(|r| r.review_log_id.is_some_and(|id| review_ids.contains(&id)))
            .cloned()
            .collect();
        rows.sort_by_key(|r| (r.created_at, r.id));
        Ok(rows)
    }

    fn get_typed_answer(&self, id: i64) -> StorageResult<Option<TypedAnswerRecord>> {
        let rows = &self.s().domains.typed_answers.rows;
        Ok(rows.iter().find(|r| r.id == id).cloned())
    }
}

impl TypedAnswerWrite for MemTx<'_> {
    fn insert_typed_answer(&mut self, rec: &TypedAnswerRecord) -> StorageResult<i64> {
        let st = &mut self.m().domains.typed_answers;
        let id = if rec.id > 0 {
            if st.rows.iter().any(|r| r.id == rec.id) {
                return Err(StorageError::Constraint {
                    constraint: "typed_answers.id".into(),
                });
            }
            st.next = st.next.max(rec.id);
            rec.id
        } else {
            st.next += 1;
            st.next
        };
        st.rows.push(TypedAnswerRecord {
            id,
            ..rec.clone()
        });
        Ok(id)
    }

    fn delete_typed_answer(&mut self, id: i64) -> StorageResult<()> {
        self.m().domains.typed_answers.rows.retain(|r| r.id != id);
        Ok(())
    }
}
