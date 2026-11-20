-- Phase-2 integration (v0.5 + v0.3): media bytes live only in v0.3's content-addressed file
-- store (`media/<ab>/<sha256>` next to the collection). The temporary `media_blobs` table of
-- 0400 goes away. `SqliteStore::open` first copies any rows it still holds to files
-- (`media::export_legacy_blobs`), so a pre-release database loses nothing; the `media`
-- metadata rows stay as they are.
DROP TABLE media_blobs;
