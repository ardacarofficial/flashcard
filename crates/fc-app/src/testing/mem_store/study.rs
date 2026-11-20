//! [`MemTx`] repository traits: Study (sessions, review log, study-queue candidates).

use super::*;
use fc_core::{ReviewLogId, SessionId};

/// Study tables.
#[derive(Debug, Clone, Default)]
pub struct StudyState {
    sessions: BTreeMap<SessionId, SessionRecord>,
    /// `session_queue` rows by session and seq.
    queues: BTreeMap<SessionId, BTreeMap<i64, SessionQueueRow>>,
    pub(super) reviews: BTreeMap<ReviewLogId, ReviewLogRecord>,
}

impl MemTx<'_> {
    fn has_value(&self, attr: AttributeDefId, values: &[String], card: &Card) -> bool {
        self.s().attr_values.iter().any(|(a, entity, rows)| {
            *a == attr
                && (*entity == EntityRef::Card(card.id) || *entity == EntityRef::Note(card.note_id))
                && rows.iter().any(|r| values.contains(&r.value))
        })
    }
}

impl StudyRead for MemTx<'_> {
    fn get_session(&self, id: SessionId) -> StorageResult<Option<SessionRecord>> {
        Ok(self.s().domains.study.sessions.get(&id).cloned())
    }

    fn latest_open_session(&self) -> StorageResult<Option<SessionRecord>> {
        Ok(self
            .s()
            .domains
            .study
            .sessions
            .values()
            .filter(|s| s.ended_at.is_none())
            .max_by_key(|s| (s.started_at, s.id))
            .cloned())
    }

    fn study_cards(&self, f: &StudyCardFilter) -> StorageResult<Vec<Card>> {
        let s = self.s();
        let mut out: Vec<Card> = s
            .cards
            .values()
            .filter(|c| c.deleted_at.is_none())
            .filter(|c| s.notes.get(&c.note_id).is_some_and(|n| n.deleted_at.is_none()))
            .filter(|c| f.deck_ids.as_ref().is_none_or(|d| d.contains(&c.deck_id)))
            .filter(|c| f.card_ids.as_ref().is_none_or(|ids| ids.contains(&c.id)))
            .filter(|c| f.states.as_ref().is_none_or(|st| st.contains(&c.scheduling.state)))
            .filter(|c| f.include_suspended || !c.suspended)
            .filter(|c| f.attributes.iter().all(|(a, v)| self.has_value(*a, v, c)))
            .filter(|c| {
                c.scheduling.state == CardState::New
                    || f.due_before.is_none_or(|t| c.scheduling.due.is_none_or(|d| d < t))
            })
            .cloned()
            .collect();
        out.sort_by_key(|c| (c.created_at, c.id));
        if let Some(first) = f.new_first {
            let mut new: Vec<&Card> = out
                .iter()
                .filter(|c| c.scheduling.state == CardState::New)
                .collect();
            if first.newest {
                new.reverse();
            }
            let keep: std::collections::HashSet<CardId> =
                new.iter().take(first.n as usize).map(|c| c.id).collect();
            out.retain(|c| c.scheduling.state != CardState::New || keep.contains(&c.id));
        }
        Ok(out)
    }

    fn get_review_log(&self, id: ReviewLogId) -> StorageResult<Option<ReviewLogRecord>> {
        Ok(self.s().domains.study.reviews.get(&id).cloned())
    }

    fn review_logs_for_card(&self, card: CardId) -> StorageResult<Vec<ReviewLogRecord>> {
        let mut out: Vec<ReviewLogRecord> = self
            .s()
            .domains
            .study
            .reviews
            .values()
            .filter(|r| r.card_id == card)
            .cloned()
            .collect();
        out.sort_by_key(|r| (r.created_at, r.id));
        Ok(out)
    }

    fn review_logs_since(
        &self,
        since: Timestamp,
        mode: &str,
    ) -> StorageResult<Vec<ReviewLogRecord>> {
        let mut out: Vec<ReviewLogRecord> = self
            .s()
            .domains
            .study
            .reviews
            .values()
            .filter(|r| r.created_at >= since && r.mode == mode)
            .cloned()
            .collect();
        out.sort_by_key(|r| (r.created_at, r.id));
        Ok(out)
    }

    fn session_queue(&self, session: SessionId) -> StorageResult<Vec<SessionQueueRow>> {
        Ok(self.queue_rows(session))
    }
}

impl MemTx<'_> {
    fn queue_rows(&self, session: SessionId) -> Vec<SessionQueueRow> {
        self.s()
            .domains
            .study
            .queues
            .get(&session)
            .map(|q| q.values().cloned().collect())
            .unwrap_or_default()
    }
}

impl StudyWrite for MemTx<'_> {
    fn upsert_session(&mut self, session: &SessionRecord) -> StorageResult<()> {
        self.m()
            .domains
            .study
            .sessions
            .insert(session.id, session.clone());
        Ok(())
    }

    fn insert_review_log(&mut self, review: &ReviewLogRecord) -> StorageResult<()> {
        if !self.s().cards.contains_key(&review.card_id) {
            return Err(StorageError::Constraint {
                constraint: "foreign_key".into(),
            });
        }
        let reviews = &mut self.m().domains.study.reviews;
        if reviews.contains_key(&review.id) {
            return Err(StorageError::Constraint {
                constraint: "review_log.id".into(),
            });
        }
        reviews.insert(review.id, review.clone());
        Ok(())
    }

    fn delete_review_log(&mut self, id: ReviewLogId) -> StorageResult<bool> {
        Ok(self.m().domains.study.reviews.remove(&id).is_some())
    }

    fn edit_session_queue(
        &mut self,
        session: SessionId,
        clear: bool,
        remove: &[i64],
        add: &[SessionQueueRow],
    ) -> StorageResult<()> {
        let q = self.m().domains.study.queues.entry(session).or_default();
        if clear {
            q.clear();
        }
        for seq in remove {
            q.remove(seq);
        }
        for r in add {
            if q.insert(r.seq, r.clone()).is_some() {
                return Err(StorageError::Constraint {
                    constraint: "session_queue_pk".into(),
                });
            }
        }
        Ok(())
    }
}
