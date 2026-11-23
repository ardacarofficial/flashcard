//! `media_origins` (v1.4 W4).

use super::{all, exec, json_list, one, ts, Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{MediaOriginRead, MediaOriginRecord, MediaOriginWrite, TtsCacheKey};
use fc_core::content::media::MediaOrigin;
use rusqlite::params;

const TABLE: &str = "media_origins";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MediaOriginRecord> {
    let origin: String = r.get(1)?;
    Ok(MediaOriginRecord {
        hash: r.get(0)?,
        origin: MediaOrigin::parse(&origin).unwrap_or_default(),
        provider: r.get(2)?,
        voice: r.get(3)?,
        language: r.get(4)?,
        rate_milli: r.get(5)?,
        text_hash: r.get(6)?,
        source: r.get(7)?,
        created_at: ts(r, 8)?,
    })
}

impl<M: Mode> MediaOriginRead for SqliteTx<'_, M> {
    fn media_origins_of(&self, hashes: &[String]) -> StorageResult<Vec<MediaOriginRecord>> {
        all(
            self.conn(),
            TABLE,
            "SELECT hash, origin, provider, voice, language, rate_milli, text_hash, source,
                    created_at
             FROM media_origins WHERE hash IN (SELECT value FROM json_each(?1)) ORDER BY hash",
            [json_list(hashes)],
            row,
        )
    }

    fn find_tts_media(&self, key: &TtsCacheKey) -> StorageResult<Option<String>> {
        one(
            self.conn(),
            TABLE,
            "SELECT o.hash FROM media_origins o JOIN media m ON m.hash = o.hash
             WHERE o.origin = 'tts' AND o.text_hash = ?1 AND o.language = ?2 AND o.voice = ?3
               AND o.rate_milli = ?4 AND o.provider = ?5
             ORDER BY o.created_at LIMIT 1",
            params![
                key.text_hash,
                key.language,
                key.voice,
                key.rate_milli,
                key.provider
            ],
            |r| r.get::<_, String>(0),
        )
    }
}

impl MediaOriginWrite for SqliteTx<'_, WriteMode> {
    fn put_media_origin(&mut self, o: &MediaOriginRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            TABLE,
            "INSERT OR REPLACE INTO media_origins (hash, origin, provider, voice, language,
                 rate_milli, text_hash, source, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                o.hash,
                o.origin.as_str(),
                o.provider,
                o.voice,
                o.language,
                o.rate_milli,
                o.text_hash,
                o.source,
                o.created_at.0
            ],
        )?;
        Ok(())
    }
}
