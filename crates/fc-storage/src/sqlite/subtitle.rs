//! `subtitle_keys`, `subtitle_captures`, `subtitle_uploads` (v1.2).

use super::{all, exec, id, json, one, opt_id, opt_ms, opt_ts, ts, Mode, SqliteTx, WriteMode};
use crate::error::{StorageError, StorageResult};
use crate::repo::{
    EncounterFacts, SubtitleCaptureRecord, SubtitleKeyRecord, SubtitleRead, SubtitleUploadRecord,
    SubtitleWrite,
};
use fc_core::{NoteId, SubtitleKeyId, Timestamp};
use rusqlite::params;

const KEYS: &str = "subtitle_keys";
const CAPTURES: &str = "subtitle_captures";
const UPLOADS: &str = "subtitle_uploads";

const KEY_COLS: &str = "id, name, prefix, token_hash, created_at, last_used_at, revoked_at";
const CAPTURE_COLS: &str =
    "capture_id, dedup_key, lang, lemma, note_id, card_id, key_id, created_at, surface, pos,
     sentence, translation, term_start, term_end, kind, definition, definition_source,
     video_id, title, site, url, t_start_ms, t_end_ms, screenshot_hash, audio_hash,
     example_fields, is_primary, review_note_id";
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
        facts: EncounterFacts {
            surface: r.get(8)?,
            pos: r.get(9)?,
            sentence: r.get(10)?,
            translation: r.get(11)?,
            term_start: r.get(12)?,
            term_end: r.get(13)?,
            kind: r.get(14)?,
            definition: r.get(15)?,
            definition_source: r.get(16)?,
            video_id: r.get(17)?,
            title: r.get(18)?,
            site: r.get(19)?,
            url: r.get(20)?,
            t_start_ms: r.get(21)?,
            t_end_ms: r.get(22)?,
            screenshot_hash: r.get(23)?,
            audio_hash: r.get(24)?,
            example_fields: json(r, 25)?,
        },
        is_primary: r.get::<_, i64>(26)? != 0,
        review_note_id: opt_id(r, 27)?,
    })
}

/// The 28 column values of a row, in [`CAPTURE_COLS`] order.
fn capture_params(c: &SubtitleCaptureRecord) -> Vec<rusqlite::types::Value> {
    use rusqlite::types::Value as V;
    let t = |s: &Option<String>| s.clone().map_or(V::Null, V::Text);
    let n = |x: Option<i64>| x.map_or(V::Null, V::Integer);
    let f = &c.facts;
    vec![
        V::Text(c.capture_id.clone()),
        V::Text(c.dedup_key.clone()),
        V::Text(c.lang.clone()),
        V::Text(c.lemma.clone()),
        V::Text(c.note_id.to_string()),
        V::Text(c.card_id.to_string()),
        c.key_id.map_or(V::Null, |k| V::Text(k.to_string())),
        V::Integer(c.created_at.0),
        t(&f.surface),
        t(&f.pos),
        t(&f.sentence),
        t(&f.translation),
        n(f.term_start),
        n(f.term_end),
        t(&f.kind),
        t(&f.definition),
        t(&f.definition_source),
        t(&f.video_id),
        t(&f.title),
        t(&f.site),
        t(&f.url),
        n(f.t_start_ms),
        n(f.t_end_ms),
        t(&f.screenshot_hash),
        t(&f.audio_hash),
        V::Text(serde_json::to_string(&f.example_fields).unwrap_or_else(|_| "{}".into())),
        V::Integer(i64::from(c.is_primary)),
        c.review_note_id.map_or(V::Null, |n| V::Text(n.to_string())),
    ]
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

    fn subtitle_encounters_of_note(
        &self,
        note: NoteId,
    ) -> StorageResult<Vec<SubtitleCaptureRecord>> {
        all(
            self.conn(),
            CAPTURES,
            &format!(
                "SELECT {CAPTURE_COLS} FROM subtitle_captures WHERE note_id = ?1
                 ORDER BY is_primary DESC, created_at, capture_id"
            ),
            [note.to_string()],
            capture_row,
        )
    }

    fn subtitle_encounters_of_video(
        &self,
        video_id: &str,
    ) -> StorageResult<Vec<SubtitleCaptureRecord>> {
        all(
            self.conn(),
            CAPTURES,
            &format!(
                "SELECT {CAPTURE_COLS} FROM subtitle_captures WHERE video_id = ?1
                 ORDER BY t_start_ms, created_at, capture_id"
            ),
            [video_id],
            capture_row,
        )
    }

    fn subtitle_encounters_in_review(&self) -> StorageResult<Vec<SubtitleCaptureRecord>> {
        all(
            self.conn(),
            CAPTURES,
            &format!(
                "SELECT {CAPTURE_COLS} FROM subtitle_captures WHERE review_note_id IS NOT NULL
                 ORDER BY created_at, capture_id"
            ),
            [],
            capture_row,
        )
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
            &format!(
                "INSERT INTO subtitle_captures ({CAPTURE_COLS}) VALUES ({})",
                (1..=28)
                    .map(|i| format!("?{i}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            rusqlite::params_from_iter(capture_params(c)),
        )
        .map_err(named("subtitle_captures"))?;
        Ok(())
    }

    fn update_subtitle_capture(&mut self, c: &SubtitleCaptureRecord) -> StorageResult<bool> {
        let sets = CAPTURE_COLS
            .split(',')
            .map(str::trim)
            .enumerate()
            .skip(1)
            .map(|(i, col)| format!("{col} = ?{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(exec(
            self.conn(),
            CAPTURES,
            &format!("UPDATE subtitle_captures SET {sets} WHERE capture_id = ?1"),
            rusqlite::params_from_iter(capture_params(c)),
        )
        .map_err(named("subtitle_captures"))?
            > 0)
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
