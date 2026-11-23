//! [`MemTx`] repository traits: media origins and the TTS cache (v1.4 W4).

use super::*;
use fc_core::content::media::MediaOrigin;

/// `media_origins`.
#[derive(Debug, Clone, Default)]
pub struct MediaOriginState {
    rows: BTreeMap<String, MediaOriginRecord>,
}

impl MediaOriginRead for MemTx<'_> {
    fn media_origins_of(&self, hashes: &[String]) -> StorageResult<Vec<MediaOriginRecord>> {
        let rows = &self.s().domains.media_origins.rows;
        Ok(rows
            .values()
            .filter(|r| hashes.contains(&r.hash))
            .cloned()
            .collect())
    }

    fn find_tts_media(&self, key: &TtsCacheKey) -> StorageResult<Option<String>> {
        let mut hits: Vec<MediaOriginRecord> = self
            .s()
            .domains
            .media_origins
            .rows
            .values()
            .filter(|r| {
                r.origin == MediaOrigin::Tts
                    && r.text_hash == key.text_hash
                    && r.language == key.language
                    && r.voice == key.voice
                    && r.rate_milli == key.rate_milli
                    && r.provider == key.provider
            })
            .cloned()
            .collect();
        hits.sort_by_key(|r| r.created_at);
        for r in hits {
            if self.get_media(&r.hash)?.is_some() {
                return Ok(Some(r.hash));
            }
        }
        Ok(None)
    }
}

impl MediaOriginWrite for MemTx<'_> {
    fn put_media_origin(&mut self, rec: &MediaOriginRecord) -> StorageResult<()> {
        self.m()
            .domains
            .media_origins
            .rows
            .insert(rec.hash.clone(), rec.clone());
        Ok(())
    }
}
