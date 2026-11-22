//! [`MemTx`] repository traits: Subtitle extension keys, captures, uploads (v1.2).

use super::*;
use fc_core::SubtitleKeyId;
use fc_storage::repo::{SubtitleCaptureRecord, SubtitleKeyRecord, SubtitleUploadRecord};

/// `subtitle_keys`, `subtitle_captures`, `subtitle_uploads`.
#[derive(Debug, Clone, Default)]
pub struct SubtitleState {
    keys: BTreeMap<SubtitleKeyId, SubtitleKeyRecord>,
    pub(super) captures: BTreeMap<String, SubtitleCaptureRecord>,
    uploads: BTreeMap<String, SubtitleUploadRecord>,
}

fn constraint(name: &str) -> StorageError {
    StorageError::Constraint {
        constraint: name.into(),
    }
}

impl SubtitleRead for MemTx<'_> {
    fn list_subtitle_keys(&self) -> StorageResult<Vec<SubtitleKeyRecord>> {
        let mut v: Vec<_> = self.s().domains.subtitle.keys.values().cloned().collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        Ok(v)
    }

    fn get_subtitle_key(&self, id: SubtitleKeyId) -> StorageResult<Option<SubtitleKeyRecord>> {
        Ok(self.s().domains.subtitle.keys.get(&id).cloned())
    }

    fn subtitle_key_by_token_hash(
        &self,
        token_hash: &str,
    ) -> StorageResult<Option<SubtitleKeyRecord>> {
        Ok(self
            .s()
            .domains
            .subtitle
            .keys
            .values()
            .find(|k| k.token_hash == token_hash)
            .cloned())
    }

    fn get_subtitle_capture(
        &self,
        capture_id: &str,
    ) -> StorageResult<Option<SubtitleCaptureRecord>> {
        Ok(self.s().domains.subtitle.captures.get(capture_id).cloned())
    }

    fn subtitle_capture_by_dedup_key(
        &self,
        dedup_key: &str,
    ) -> StorageResult<Option<SubtitleCaptureRecord>> {
        Ok(self
            .s()
            .domains
            .subtitle
            .captures
            .values()
            .find(|c| c.dedup_key == dedup_key)
            .cloned())
    }

    fn subtitle_capture_stats(&self) -> StorageResult<(u64, Option<Timestamp>)> {
        let c = &self.s().domains.subtitle.captures;
        Ok((
            c.len() as u64,
            c.values().map(|r| r.created_at).max(),
        ))
    }

    fn subtitle_lemma_page(
        &self,
        lang: Option<&str>,
        after: Option<(&str, &str)>,
        limit: u32,
    ) -> StorageResult<Vec<(String, String)>> {
        let pairs: std::collections::BTreeSet<(String, String)> = self
            .s()
            .domains
            .subtitle
            .captures
            .values()
            .filter(|c| lang.is_none_or(|l| c.lang == l))
            .map(|c| (c.lang.clone(), c.lemma.clone()))
            .collect();
        Ok(pairs
            .into_iter()
            .filter(|(l, m)| after.is_none_or(|(al, am)| (l.as_str(), m.as_str()) > (al, am)))
            .take(limit as usize)
            .collect())
    }

    fn subtitle_captures_of(
        &self,
        lemmas: &[(String, String)],
    ) -> StorageResult<Vec<SubtitleCaptureRecord>> {
        let mut v: Vec<_> = self
            .s()
            .domains
            .subtitle
            .captures
            .values()
            .filter(|c| lemmas.iter().any(|(l, m)| *l == c.lang && *m == c.lemma))
            .cloned()
            .collect();
        v.sort_by(|a, b| {
            (&a.lang, &a.lemma, a.created_at, &a.capture_id).cmp(&(
                &b.lang,
                &b.lemma,
                b.created_at,
                &b.capture_id,
            ))
        });
        Ok(v)
    }

    fn subtitle_encounters_of_note(
        &self,
        note: fc_core::NoteId,
    ) -> StorageResult<Vec<SubtitleCaptureRecord>> {
        let mut v: Vec<_> = self
            .s()
            .domains
            .subtitle
            .captures
            .values()
            .filter(|c| c.note_id == note)
            .cloned()
            .collect();
        v.sort_by(|a, b| {
            (!a.is_primary, a.created_at, &a.capture_id).cmp(&(
                !b.is_primary,
                b.created_at,
                &b.capture_id,
            ))
        });
        Ok(v)
    }

    fn subtitle_encounters_of_video(
        &self,
        video_id: &str,
    ) -> StorageResult<Vec<SubtitleCaptureRecord>> {
        let mut v: Vec<_> = self
            .s()
            .domains
            .subtitle
            .captures
            .values()
            .filter(|c| c.facts.video_id.as_deref() == Some(video_id))
            .cloned()
            .collect();
        v.sort_by(|a, b| {
            (a.facts.t_start_ms, a.created_at, &a.capture_id).cmp(&(
                b.facts.t_start_ms,
                b.created_at,
                &b.capture_id,
            ))
        });
        Ok(v)
    }

    fn subtitle_encounters_in_review(&self) -> StorageResult<Vec<SubtitleCaptureRecord>> {
        let mut v: Vec<_> = self
            .s()
            .domains
            .subtitle
            .captures
            .values()
            .filter(|c| c.review_note_id.is_some())
            .cloned()
            .collect();
        v.sort_by(|a, b| (a.created_at, &a.capture_id).cmp(&(b.created_at, &b.capture_id)));
        Ok(v)
    }

    fn get_subtitle_upload(&self, id: &str) -> StorageResult<Option<SubtitleUploadRecord>> {
        Ok(self.s().domains.subtitle.uploads.get(id).cloned())
    }

    fn subtitle_upload_for(
        &self,
        capture_id: &str,
        kind: &str,
    ) -> StorageResult<Option<SubtitleUploadRecord>> {
        Ok(self
            .s()
            .domains
            .subtitle
            .uploads
            .values()
            .find(|u| u.capture_id == capture_id && u.kind == kind)
            .cloned())
    }
}

impl MemTx<'_> {
    /// Another row of the same note is primary too (`subtitle_captures_primary`).
    fn primary_clash(&self, row: &SubtitleCaptureRecord) -> bool {
        row.is_primary
            && self.s().domains.subtitle.captures.values().any(|c| {
                c.capture_id != row.capture_id && c.note_id == row.note_id && c.is_primary
            })
    }
}

impl SubtitleWrite for MemTx<'_> {
    fn upsert_subtitle_key(&mut self, key: &SubtitleKeyRecord) -> StorageResult<()> {
        if self
            .subtitle_key_by_token_hash(&key.token_hash)?
            .is_some_and(|other| other.id != key.id)
        {
            return Err(constraint("subtitle_keys_token"));
        }
        self.m().domains.subtitle.keys.insert(key.id, key.clone());
        Ok(())
    }

    fn update_subtitle_capture(&mut self, row: &SubtitleCaptureRecord) -> StorageResult<bool> {
        if self.get_subtitle_capture(&row.capture_id)?.is_none() {
            return Ok(false);
        }
        if self.primary_clash(row) {
            return Err(constraint("subtitle_captures"));
        }
        self.m()
            .domains
            .subtitle
            .captures
            .insert(row.capture_id.clone(), row.clone());
        Ok(true)
    }

    fn insert_subtitle_capture(&mut self, row: &SubtitleCaptureRecord) -> StorageResult<()> {
        if self.get_subtitle_capture(&row.capture_id)?.is_some()
            || self.subtitle_capture_by_dedup_key(&row.dedup_key)?.is_some()
            || self.primary_clash(row)
        {
            return Err(constraint("subtitle_captures"));
        }
        self.m()
            .domains
            .subtitle
            .captures
            .insert(row.capture_id.clone(), row.clone());
        Ok(())
    }

    fn delete_subtitle_capture(&mut self, capture_id: &str) -> StorageResult<bool> {
        Ok(self
            .m()
            .domains
            .subtitle
            .captures
            .remove(capture_id)
            .is_some())
    }

    fn upsert_subtitle_upload(&mut self, upload: &SubtitleUploadRecord) -> StorageResult<()> {
        let uploads = &mut self.m().domains.subtitle.uploads;
        uploads.retain(|_, u| !(u.capture_id == upload.capture_id && u.kind == upload.kind));
        uploads.insert(upload.id.clone(), upload.clone());
        Ok(())
    }

    fn delete_subtitle_uploads_before(
        &mut self,
        before: Timestamp,
    ) -> StorageResult<Vec<SubtitleUploadRecord>> {
        let uploads = &mut self.m().domains.subtitle.uploads;
        let old: Vec<_> = uploads
            .values()
            .filter(|u| u.created_at < before)
            .cloned()
            .collect();
        uploads.retain(|_, u| u.created_at >= before);
        Ok(old)
    }
}
