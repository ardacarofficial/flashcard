-- 1470_leech_repairs (v1.4 W3): which repair the user applied to a leech card, so the next
-- real reviews can be compared with the ones before.
--
-- One row per applied repair: the card and its note, the kind (`switch_example`,
-- `add_accepted_answers`, `move_meaning`, `ai_rewrite`), the repair as applied (JSON), the
-- card's lapses at that moment and when. User history, not diagnostics: written through
-- undoable commands and kept in backups like any other table.
CREATE TABLE leech_repairs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id    TEXT NOT NULL,
    note_id    TEXT NOT NULL,
    kind       TEXT NOT NULL
               CHECK (kind IN ('switch_example', 'add_accepted_answers', 'move_meaning', 'ai_rewrite')),
    detail     TEXT NOT NULL DEFAULT '{}',
    lapses     INTEGER NOT NULL,
    applied_at INTEGER NOT NULL
) STRICT;
CREATE INDEX leech_repairs_card ON leech_repairs (card_id, applied_at);
