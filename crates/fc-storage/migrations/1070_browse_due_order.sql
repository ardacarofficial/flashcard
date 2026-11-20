-- 1070_browse_due_order (v1.0 performance): browser pages of big lists and the load balancer.
--
-- * cards_due_order: `ORDER BY c.due, c.id` over the whole collection had no index
--   (`cards_due_active` leaves out new and suspended cards), so every page of a big list sorted
--   every match. A page is now read by walking it in order (fc-storage sqlite/search.rs
--   `ordered_from`); `note_id` and `deleted_at` make the liveness checks of the walk index-only.
-- * notes_sort_live / notes_updated_live: the same for the sort-field and modified sorts — the
--   walk no longer reads every (large) note row to see whether the note is trashed.
-- * cards_due_load: the load balancer (v0.6 planning) counts the cards due on each day of an
--   answer's candidate range on every answer and interval preview, from this index alone
--   (`note_id` for the "note is live" check against the small `notes_trashed` set).
CREATE INDEX cards_due_order ON cards(due, id, note_id, deleted_at);
CREATE INDEX notes_sort_live ON notes(sort_field COLLATE NOCASE, id, deleted_at);
CREATE INDEX notes_updated_live ON notes(updated_at, id, deleted_at);
CREATE INDEX cards_due_load ON cards(due, note_id)
    WHERE state IN ('learning', 'review', 'relearning') AND suspended = 0
      AND deleted_at IS NULL;
