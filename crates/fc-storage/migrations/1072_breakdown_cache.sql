-- 1072_breakdown_cache (v1.0 performance): `stats.breakdown` caches its per-value day
-- aggregates in the statistics cache tables (calendar key `breakdown:<attr id>:<calendar>`).
-- `fingerprint` names the card sets the aggregates were built for (which cards had which
-- value): the cache is used only while the breakdown's cards still have exactly that
-- fingerprint, so classification edits, trashed cards and scope changes never see stale
-- numbers. The overview's whole-collection caches have none.
ALTER TABLE stats_cache_meta ADD COLUMN fingerprint TEXT;
