//! `subtitle_keys`, `subtitle_captures`, `subtitle_uploads` (v1.2).

use super::{all, exec, id, one, opt_id, opt_ms, opt_ts, ts, Mode, SqliteTx, WriteMode};
use crate::error::{StorageError, StorageResult};
use crate::repo::{
    SubtitleCaptureRecord, SubtitleKeyRecord, SubtitleRead, SubtitleUploadRecord, SubtitleWrite,
};
use fc_core::{SubtitleKeyId, Timestamp};
use rusqlite::params;

const KEYS: &str = "subtitle_keys";
const CAPTURES: &str = "subtitle_captures";
const UPLOADS: &str = "subtitle_uploads";

const KEY_COLS: &str = "id, name, prefix, token_hash, created_at, last_used_at, revoked_at";
const CAPTURE_COLS: &str =
    "capture_id, dedup_key, lang, lemma, note_id, card_id, key_id, created_at";
const UPLOAD_COLS: &str = "id, capture_id, kind, media_hash, mime, created_at";

fn key_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SubtitleKeyRecord> {
    Ok(SubtitleKeyRecord {
        id: id(r, 0)?,
        name: r.get(1)?,
        prefix: r.get(2)?,
        token_hash: r.get(3)?,
        created_at: ts(r, 4)?,
        last_used_at: opt_ts(r, 5)?,
        revoked_at: opt_ts(r, 6)?,
    })
}

fn capture_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SubtitleCaptureRecord> {
    Ok(SubtitleCaptureRecord {
        capture_id: r.get(0)?,
        dedup_key: r.get(1)?,
        lang: r.get(2)?,
        lemma: r.get(3)?,
        note_id: id(r, 4)?,
        card_id: id(r, 5)?,
        key_id: opt_id(r, 6)?,
        created_at: ts(r, 7)?,
    })
}

fn upload_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SubtitleUploadRecord> {
    Ok(SubtitleUploadRecord {
        id: r.get(0)?,
        capture_id: r.get(1)?,
        kind: r.get(2)?,
        media_hash: r.get(3)?,
        mime: r.get(4)?,
        created_at: ts(r, 5)?,
    })
}

impl<M: Mode> SubtitleRead for SqliteTx<'_, M> {
    fn list_subtitle_keys(&self) -> StorageResult<Vec<SubtitleKeyRecord>> {
        all(
            self.conn(),
            KEYS,
            &format!("SELECT {KEY_COLS} FROM subtitle_keys ORDER BY created_at DESC, id DESC"),
            [],
            key_row,
        )
    }

    fn get_subtitle_key(&self, key: SubtitleKeyId) -> StorageResult<Option<SubtitleKeyRecord>> {
        one(
            self.conn(),
            KEYS,
            &format!("SELECT {KEY_COLS} FROM subtitle_keys WHERE id = ?1"),
            [key.to_string()],
            key_row,
        )
    }

    fn subtitle_key_by_token_hash(
        &self,
        token_hash: &str,
    ) -> StorageResult<Option<SubtitleKeyRecord>> {
        one(
            self.conn(),
            KEYS,
            &format!("SELECT {KEY_COLS} FROM subtitle_keys WHERE token_hash = ?1"),
            [token_hash],
            key_row,
        )
    }

    fn get_subtitle_capture(
        &self,
        capture_id: &str,
    ) -> StorageResult<Option<SubtitleCaptureRecord>> {
        one(
            self.conn(),
            CAPTURES,
            &format!("SELECT {CAPTURE_COLS} FROM subtitle_captures WHERE capture_id = ?1"),
            [capture_id],
            capture_row,
        )
    }

    fn subtitle_capture_by_dedup_key(
        &self,
        dedup_key: &str,
    ) -> StorageResult<Option<SubtitleCaptureRecord>> {
        one(
            self.conn(),
            CAPTURES,
            &format!("SELECT {CAPTURE_COLS} FROM subtitle_captures WHERE dedup_key = ?1"),
            [dedup_key],
            capture_row,
        )
    }

    fn subtitle_capture_stats(&self) -> StorageResult<(u64, Option<Timestamp>)> {
        let row = one(
            self.conn(),
            CAPTURES,
            "SELECT count(*), max(created_at) FROM subtitle_captures",
            [],
            |r| Ok((r.get::<_, i64>(0)?, opt_ts(r, 1)?)),
        )?;
        Ok(row.map_or((0, None), |(n, at)| (u64::try_from(n).unwrap_or(0), at)))
    }

    fn subtitle_lemma_page(
        &self,
        lang: Option<&str>,
        after: Option<(&str, &str)>,
        limit: u32,
    ) -> StorageResult<Vec<(String, String)>> {
        let (after_lang, after_lemma) = after.unwrap_or(("", ""));
        all(
            self.conn(),
            CAPTURES,
            "SELECT DISTINCT lang, lemma FROM subtitle_captures
             WHERE (?1 IS NULL OR lang = ?1)
               AND (?2 = 0 OR lang > ?3 OR (lang = ?3 AND lemma > ?4))
             ORDER BY lang, lemma LIMIT ?5",
            params![
                lang,
                i64::from(after.is_some()),
                after_lang,
                after_lemma,
                i64::from(limit)
            ],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    fn subtitle_captures_of(
        &self,
        lemmas: &[(String, String)],
    ) -> StorageResult<Vec<SubtitleCaptureRecord>> {
        let sql = format!(
            "SELECT {CAPTURE_COLS} FROM subtitle_captures WHERE lang = ?1 AND lemma = ?2
             ORDER BY created_at, capture_id"
        );
        let mut out = Vec::new();
        for (lang, lemma) in lemmas {
            out.extend(all(
                self.conn(),
                CAPTURES,
                &sql,
                [lang, lemma],
                capture_row,
            )?);
        }
        Ok(out)
    }

    fn get_subtitle_upload(&self, upload: &str) -> StorageResult<Option<SubtitleUploadRecord>> {
        one(
            self.conn(),
            UPLOADS,
            &format!("SELECT {UPLOAD_COLS} FROM subtitle_uploads WHERE id = ?1"),
            [upload],
            upload_row,
        )
    }

    fn subtitle_upload_for(
        &self,
        capture_id: &str,
        kind: &str,
    ) -> StorageResult<Option<SubtitleUploadRecord>> {
        one(
            self.conn(),
            UPLOADS,
            &format!(
                "SELECT {UPLOAD_COLS} FROM subtitle_uploads WHERE capture_id = ?1 AND kind = ?2"
            ),
            [capture_id, kind],
            upload_row,
        )
    }
}

fn named(constraint: &'static str) -> impl Fn(StorageError) -> StorageError {
    move |e| match e {
        StorageError::Constraint { .. } => StorageError::Constraint {
            constraint: constraint.into(),
        },
        other => other,
    }
}

impl SubtitleWrite for SqliteTx<'_, WriteMode> {
    fn upsert_subtitle_key(&mut self, k: &SubtitleKeyRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            KEYS,
            "INSERT INTO subtitle_keys (id, name, prefix, token_hash, created_at, last_used_at,
                 revoked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET name = ?2, prefix = ?3, token_hash = ?4,
                 created_at = ?5, last_used_at = ?6, revoked_at = ?7",
            params![
                k.id.to_string(),
                k.name,
                k.prefix,
                k.token_hash,
                k.created_at.0,
                opt_ms(k.last_used_at),
                opt_ms(k.revoked_at),
            ],
        )
        .map_err(named("subtitle_keys_token"))?;
        Ok(())
    }

    fn insert_subtitle_capture(&mut self, c: &SubtitleCaptureRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            CAPTURES,
            "INSERT INTO subtitle_captures (capture_id, dedup_key, lang, lemma, note_id, card_id,
                 key_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                c.capture_id,
                c.dedup_key,
                c.lang,
                c.lemma,
                c.note_id.to_string(),
                c.card_id.to_string(),
                c.key_id.map(|k| k.to_string()),
                c.created_at.0,
            ],
        )
        .map_err(named("subtitle_captures"))?;
        Ok(())
    }

    fn delete_subtitle_capture(&mut self, capture_id: &str) -> StorageResult<bool> {
        Ok(exec(
            self.conn(),
            CAPTURES,
            "DELETE FROM subtitle_captures WHERE capture_id = ?1",
            [capture_id],
        )? > 0)
    }

    fn upsert_subtitle_upload(&mut self, u: &SubtitleUploadRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            UPLOADS,
            "INSERT INTO subtitle_uploads (id, capture_id, kind, media_hash, mime, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(capture_id, kind) DO UPDATE SET id = ?1, media_hash = ?4, mime = ?5,
                 created_at = ?6",
            params![
                u.id,
                u.capture_id,
                u.kind,
                u.media_hash,
                u.mime,
                u.created_at.0
            ],
        )
        .map_err(named("subtitle_uploads"))?;
        Ok(())
    }

    fn delete_subtitle_uploads_before(
        &mut self,
        before: Timestamp,
    ) -> StorageResult<Vec<SubtitleUploadRecord>> {
        let old = all(
            self.conn(),
            UPLOADS,
            &format!(
                "SELECT {UPLOAD_COLS} FROM subtitle_uploads WHERE created_at < ?1 ORDER BY id"
            ),
            [before.0],
            upload_row,
        )?;
        if !old.is_empty() {
            exec(
                self.conn(),
                UPLOADS,
                "DELETE FROM subtitle_uploads WHERE created_at < ?1",
                [before.0],
            )?;
        }
        Ok(old)
    }
}
