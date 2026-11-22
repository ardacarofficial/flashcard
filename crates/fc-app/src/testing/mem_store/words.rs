//! [`MemTx`] repository traits: the word index (v1.4). There are no triggers here: every
//! drain treats every note and every mark as dirty (a full recompute), which gives the same
//! rows as the SQLite store's incremental queues.

use super::*;
use fc_core::NoteId;

/// `word_index`, `word_index_notes`, `word_note_language`.
#[derive(Debug, Clone, Default)]
pub struct WordsState {
    notes: BTreeMap<NoteId, WordIndexNoteRecord>,
    words: BTreeMap<(String, String), WordIndexRecord>,
    languages: BTreeMap<NoteId, String>,
}

fn status(w: &WordIndexRecord, threshold: u32) -> String {
    if let Some(m) = &w.mark {
        return m.clone();
    }
    if w.cards > 0 && w.max_interval_days.is_some_and(|d| d >= threshold) {
        "known".into()
    } else if w.cards > 0 {
        "learning".into()
    } else {
        "new".into()
    }
}

impl WordIndexRead for MemTx<'_> {
    fn word_index_get(&self, language: &str, keys: &[String]) -> StorageResult<Vec<WordIndexRecord>> {
        let w = &self.s().domains.words.words;
        Ok(keys
            .iter()
            .filter_map(|k| w.get(&(language.to_owned(), k.clone())).cloned())
            .collect())
    }

    fn word_index_page(&self, q: &WordIndexQuery) -> StorageResult<Vec<WordIndexRecord>> {
        Ok(self
            .s()
            .domains
            .words
            .words
            .iter()
            .filter(|((l, k), _)| {
                q.language.as_ref().is_none_or(|ql| ql == l)
                    && q.after.as_ref().is_none_or(|(al, ak)| (l, k) > (al, ak))
            })
            .map(|(_, w)| w)
            .filter(|w| q.statuses.is_empty() || q.statuses.contains(&status(w, q.threshold_days)))
            .take(q.limit as usize)
            .cloned()
            .collect())
    }

    fn word_index_counts(&self, threshold_days: u32) -> StorageResult<Vec<(String, String, u32)>> {
        let mut counts: BTreeMap<(String, String), u32> = BTreeMap::new();
        for w in self.s().domains.words.words.values() {
            *counts
                .entry((w.language.clone(), status(w, threshold_days)))
                .or_default() += 1;
        }
        Ok(counts.into_iter().map(|((l, s), n)| (l, s, n)).collect())
    }

    fn word_index_unassigned(&self) -> StorageResult<u32> {
        let n = self
            .s()
            .domains
            .words
            .notes
            .values()
            .filter(|n| n.language.is_empty())
            .count();
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }

    fn word_index_notes_of(
        &self,
        language: &str,
        keys: &[String],
    ) -> StorageResult<Vec<WordIndexNoteRecord>> {
        let mut v: Vec<WordIndexNoteRecord> = self
            .s()
            .domains
            .words
            .notes
            .values()
            .filter(|n| n.language == language && keys.contains(&n.key))
            .cloned()
            .collect();
        v.sort_by(|a, b| (&a.key, a.created_at, a.note_id).cmp(&(&b.key, b.created_at, b.note_id)));
        Ok(v)
    }

    fn word_index_notes_by_id(&self, notes: &[NoteId]) -> StorageResult<Vec<WordIndexNoteRecord>> {
        let n = &self.s().domains.words.notes;
        Ok(notes.iter().filter_map(|id| n.get(id).cloned()).collect())
    }

    fn word_index_dump(&self) -> StorageResult<WordIndexDump> {
        let w = &self.s().domains.words;
        Ok((
            w.words.values().cloned().collect(),
            w.notes.values().cloned().collect(),
        ))
    }

    fn word_note_sources(&self, notes: &[NoteId]) -> StorageResult<Vec<WordNoteSource>> {
        let s = self.s();
        Ok(notes
            .iter()
            .filter_map(|id| s.notes.get(id))
            .filter(|n| n.deleted_at.is_none())
            .map(|n| WordNoteSource {
                note_id: n.id,
                notetype_id: n.notetype_id,
                home_deck_id: n.home_deck_id,
                fields: n.fields.clone(),
                created_at: n.created_at,
                capture_lang: s
                    .domains
                    .subtitle
                    .captures
                    .values()
                    .filter(|c| c.note_id == n.id)
                    .max_by_key(|c| c.created_at)
                    .map(|c| c.lang.clone()),
                known_lang: s.domains.words.languages.get(&n.id).cloned(),
            })
            .collect())
    }

    fn word_card_rows(&self, notes: &[NoteId]) -> StorageResult<Vec<WordCardRow>> {
        let mut v: Vec<&Card> = self
            .s()
            .cards
            .values()
            .filter(|c| c.deleted_at.is_none() && notes.contains(&c.note_id))
            .collect();
        v.sort_by_key(|c| (c.note_id, c.created_at, c.id));
        Ok(v.into_iter()
            .map(|c| WordCardRow {
                note_id: c.note_id,
                card_id: c.id,
                state: c.scheduling.state,
                due: c.scheduling.due,
                last_review: c.scheduling.last_review,
            })
            .collect())
    }

    fn word_marks_of(&self, language: &str, keys: &[String]) -> StorageResult<Vec<WordMarkRecord>> {
        let m = &self.s().domains.reading.marks;
        Ok(keys
            .iter()
            .filter_map(|k| m.get(&(language.to_owned(), k.clone())).cloned())
            .collect())
    }

    fn word_index_dirty_count(&self) -> StorageResult<u32> {
        Ok(0)
    }
}

impl WordIndexWrite for MemTx<'_> {
    fn take_word_index_dirty(&mut self) -> StorageResult<WordIndexDirty> {
        let s = self.s();
        let mut notes: Vec<NoteId> = s.notes.keys().copied().collect();
        notes.extend(s.domains.words.notes.keys().copied());
        notes.sort_unstable();
        notes.dedup();
        let mut keys: Vec<(String, String)> = s.domains.reading.marks.keys().cloned().collect();
        keys.extend(s.domains.words.words.keys().cloned());
        keys.sort_unstable();
        keys.dedup();
        Ok((notes, keys))
    }

    fn reset_word_index(&mut self) -> StorageResult<()> {
        let w = &mut self.m().domains.words;
        w.notes.clear();
        w.words.clear();
        Ok(())
    }

    fn put_word_index_note(&mut self, r: &WordIndexNoteRecord) -> StorageResult<()> {
        self.m().domains.words.notes.insert(r.note_id, r.clone());
        Ok(())
    }

    fn delete_word_index_note(&mut self, note: NoteId) -> StorageResult<()> {
        self.m().domains.words.notes.remove(&note);
        Ok(())
    }

    fn put_word_index_entry(&mut self, r: &WordIndexRecord) -> StorageResult<()> {
        self.m()
            .domains
            .words
            .words
            .insert((r.language.clone(), r.key.clone()), r.clone());
        Ok(())
    }

    fn delete_word_index_entry(&mut self, language: &str, key: &str) -> StorageResult<()> {
        self.m()
            .domains
            .words
            .words
            .remove(&(language.to_owned(), key.to_owned()));
        Ok(())
    }

    fn set_word_note_language(&mut self, note: NoteId, language: &str) -> StorageResult<()> {
        self.m()
            .domains
            .words
            .languages
            .insert(note, language.to_owned());
        Ok(())
    }
}
