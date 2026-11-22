-- 1450_meaning_in_context (v1.4 W2, docs/06 §1.4): the meaning of a note's word in its
-- sentence, and the answers the offline written-answer check also accepts.
--
-- * `note_meanings`: one row per note — the meaning that fits the note's sentence, the phrase
--   it rests on, whether it is an expression, other correct translations, meanings that do
--   not fit, the model's confidence, and `original`: the meaning field's value before the
--   first apply (the source's definition is never overwritten silently). `status` is
--   `suggested` (asked automatically after a capture, opt-in; the note is unchanged) or
--   `applied` (the user accepted it with `ai.meaning.apply`, an undoable command).
-- * `note_accepted_answers`: per note and field, other answers the written-answer check
--   accepts ("give up" → `vazgeçmek`, `bırakmak`); no model is asked at study time.
--
-- Both follow the note: trashing keeps them (the note may come back), a note deleted for good
-- takes them with it.

CREATE TABLE note_meanings (
    note_id      TEXT PRIMARY KEY NOT NULL,
    field_key    TEXT NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('suggested', 'applied')),
    meaning      TEXT NOT NULL,
    phrase       TEXT NOT NULL DEFAULT '',
    expression   INTEGER NOT NULL DEFAULT 0 CHECK (expression IN (0, 1)),
    also_correct TEXT NOT NULL DEFAULT '[]',
    not_this     TEXT NOT NULL DEFAULT '[]',
    confidence   REAL,
    edited       INTEGER NOT NULL DEFAULT 0 CHECK (edited IN (0, 1)),
    original     TEXT NOT NULL DEFAULT '',
    model        TEXT NOT NULL DEFAULT '',
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE note_accepted_answers (
    note_id    TEXT NOT NULL,
    field_key  TEXT NOT NULL,
    answer     TEXT NOT NULL,
    position   INTEGER NOT NULL,
    PRIMARY KEY (note_id, field_key, answer)
) STRICT, WITHOUT ROWID;

CREATE TRIGGER note_meanings_note_del AFTER DELETE ON notes BEGIN
    DELETE FROM note_meanings WHERE note_id = OLD.id;
    DELETE FROM note_accepted_answers WHERE note_id = OLD.id;
END;
