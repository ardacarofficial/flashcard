//! [`MemTx`] repository traits: meaning in context and accepted answers (v1.4 W2).

use super::*;
use fc_storage::repo::{AcceptedAnswerRecord, NoteMeaningRecord};

/// `note_meanings`, `note_accepted_answers`.
#[derive(Debug, Clone, Default)]
pub struct MeaningState {
    meanings: BTreeMap<NoteId, NoteMeaningRecord>,
    answers: BTreeMap<NoteId, Vec<AcceptedAnswerRecord>>,
}

impl MeaningRead for MemTx<'_> {
    fn get_note_meaning(&self, note: NoteId) -> StorageResult<Option<NoteMeaningRecord>> {
        Ok(self.s().domains.meanings.meanings.get(&note).cloned())
    }

    fn list_accepted_answers(&self, note: NoteId) -> StorageResult<Vec<AcceptedAnswerRecord>> {
        let mut rows = self
            .s()
            .domains
            .meanings
            .answers
            .get(&note)
            .cloned()
            .unwrap_or_default();
        rows.sort_by(|a, b| {
            (&a.field_key, a.position, &a.answer).cmp(&(&b.field_key, b.position, &b.answer))
        });
        Ok(rows)
    }
}

impl MeaningWrite for MemTx<'_> {
    fn put_note_meaning(&mut self, rec: &NoteMeaningRecord) -> StorageResult<()> {
        self.m().domains.meanings.meanings.insert(rec.note_id, rec.clone());
        Ok(())
    }

    fn delete_note_meaning(&mut self, note: NoteId) -> StorageResult<()> {
        self.m().domains.meanings.meanings.remove(&note);
        Ok(())
    }

    fn set_accepted_answers(&mut self, note: NoteId, rows: &[AcceptedAnswerRecord]) -> StorageResult<()> {
        let mut kept: Vec<AcceptedAnswerRecord> = Vec::new();
        for r in rows {
            // The primary key (note, field, answer): a repeated answer is ignored.
            if !kept.iter().any(|k| k.field_key == r.field_key && k.answer == r.answer) {
                kept.push(AcceptedAnswerRecord {
                    note_id: note,
                    ..r.clone()
                });
            }
        }
        let st = &mut self.m().domains.meanings.answers;
        if kept.is_empty() {
            st.remove(&note);
        } else {
            st.insert(note, kept);
        }
        Ok(())
    }
}
