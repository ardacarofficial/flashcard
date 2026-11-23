-- 1421_typed_answers (v1.4 Z1): what the user typed for a written answer, next to the review
-- it was rated with, so a leech's evidence can show what went wrong.
--
-- One row per typed answer: the card, the review it belongs to (Cards type-in, Learn's
-- written stage and the Exam's written questions all log one; nullable for answers that do
-- not), the review's mode, the text, whether it was judged right (NULL when the user rated
-- it himself) and when. The app keeps the newest 20 per card (pruned in the answer's own
-- transaction) and writes nothing while the setting `study.keep_typed_answers` is off. User
-- history, not diagnostics: written through undoable commands, kept in backups, never logged.
CREATE TABLE typed_answers (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id       TEXT NOT NULL,
    review_log_id TEXT,
    mode          TEXT NOT NULL,
    typed         TEXT NOT NULL,
    correct       INTEGER CHECK (correct IN (0, 1)),
    created_at    INTEGER NOT NULL
) STRICT;
CREATE INDEX typed_answers_card ON typed_answers (card_id, created_at);
CREATE INDEX typed_answers_review ON typed_answers (review_log_id) WHERE review_log_id IS NOT NULL;
