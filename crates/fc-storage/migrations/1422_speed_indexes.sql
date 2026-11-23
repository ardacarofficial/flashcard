-- 1422_speed_indexes (v1.4 Z2): indexes for the measured slow paths at 200k cards (docs/12).
--
-- * cards_deck_new_order: a deck's first new cards in creation order (the study planner reads
--   the first n of each deck, then merges; the collection-wide `cards_new_order` walked past
--   every other deck's new cards to fill a small deck's queue).
-- * cards_deck_live: live cards per deck counted from the index alone (`decks.tree` totals;
--   the note's liveness is a probe into the small `notes_trashed` set).
CREATE INDEX cards_deck_new_order ON cards(deck_id, created_at, id)
    WHERE state = 'new' AND suspended = 0 AND deleted_at IS NULL;
CREATE INDEX cards_deck_live ON cards(deck_id, note_id) WHERE deleted_at IS NULL;
