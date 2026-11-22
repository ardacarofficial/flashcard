-- 1410_word_index (v1.4 A3, docs/03 §4.2): one persisted word index, the single answer to
-- "which words does he know" for reading mode, the Subtitle extension and the `words.*` API.
--
-- * `word_index_notes`: what each note contributes — one row per live note whose note type
--   has a `term` field (or that reading mode made with a known language): its language
--   (primary subtag; '' = none known, reported and never guessed), its key (the term's
--   lemmas joined by a space: a lemma or a multi-word expression), its live card count, the
--   longest review interval of those cards (study days, NULL = no review card), its newest
--   review and its creation time.
-- * `word_index`: one row per (language, key) with at least one note or a manual mark. The
--   evidence is kept apart: `mark` (manual known / ignored, from `word_marks`) and the note
--   aggregates (`notes`, `cards`, `max_interval_days`). The status is derived on read with
--   the `words.known_interval_days` setting: a mark wins, else a card whose interval reached
--   the threshold → known, else any card → learning, else new.
-- * `word_note_language`: the language a producer knew for a note (reading mode's text
--   language); outlives an undone note harmlessly (a dead note is not indexed).
-- * `word_index_dirty` / `word_index_dirty_keys`: filled by the triggers below whenever a
--   note, card, note type, capture, mark or language setting changes; the app drains them
--   at the end of every write transaction (`fc_app::modules::words::index::drain`), so the
--   index is updated by every command — undo and redo included — and never by hand.
--
-- The queues have no unique key (the drain reads them DISTINCT): a whole-table copy (backup
-- restore) may add rows the triggers already queued. The triggers skip queued entries with
-- `WHERE NOT EXISTS` (never `OR IGNORE`: an outer upsert's conflict clause would override it).
--
-- The migration marks every existing note and mark dirty; the app builds the index when it
-- opens the collection.

CREATE TABLE word_index_notes (
    note_id           TEXT PRIMARY KEY NOT NULL,
    language          TEXT NOT NULL,
    key               TEXT NOT NULL,
    cards             INTEGER NOT NULL,
    max_interval_days INTEGER,
    last_review       INTEGER,
    created_at        INTEGER NOT NULL
) STRICT;
CREATE INDEX word_index_notes_key ON word_index_notes (language, key);

CREATE TABLE word_index (
    language          TEXT NOT NULL,
    key               TEXT NOT NULL,
    mark              TEXT CHECK (mark IN ('known', 'ignored')),
    mark_at           INTEGER,
    notes             INTEGER NOT NULL,
    cards             INTEGER NOT NULL,
    max_interval_days INTEGER,
    last_review       INTEGER,
    first_seen        INTEGER,
    PRIMARY KEY (language, key)
) STRICT, WITHOUT ROWID;

CREATE TABLE word_note_language (
    note_id  TEXT PRIMARY KEY NOT NULL,
    language TEXT NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE word_index_dirty (
    note_id TEXT NOT NULL
) STRICT;
CREATE INDEX word_index_dirty_note ON word_index_dirty (note_id);

CREATE TABLE word_index_dirty_keys (
    language TEXT NOT NULL,
    key      TEXT NOT NULL
) STRICT;
CREATE INDEX word_index_dirty_keys_key ON word_index_dirty_keys (language, key);

-- notes
CREATE TRIGGER word_index_notes_ins AFTER INSERT ON notes BEGIN
    INSERT INTO word_index_dirty (note_id) SELECT NEW.id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = NEW.id);
END;
CREATE TRIGGER word_index_notes_upd
AFTER UPDATE OF fields, notetype_id, home_deck_id, deleted_at, created_at ON notes BEGIN
    INSERT INTO word_index_dirty (note_id) SELECT NEW.id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = NEW.id);
END;
CREATE TRIGGER word_index_notes_del AFTER DELETE ON notes BEGIN
    INSERT INTO word_index_dirty (note_id) SELECT OLD.id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = OLD.id);
END;

-- cards
CREATE TRIGGER word_index_cards_ins AFTER INSERT ON cards BEGIN
    INSERT INTO word_index_dirty (note_id) SELECT NEW.note_id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = NEW.note_id);
END;
CREATE TRIGGER word_index_cards_upd
AFTER UPDATE OF note_id, state, due, last_review, deleted_at ON cards BEGIN
    INSERT INTO word_index_dirty (note_id) SELECT NEW.note_id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = NEW.note_id);
    INSERT INTO word_index_dirty (note_id) SELECT OLD.note_id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = OLD.note_id);
END;
CREATE TRIGGER word_index_cards_del AFTER DELETE ON cards BEGIN
    INSERT INTO word_index_dirty (note_id) SELECT OLD.note_id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = OLD.note_id);
END;

-- note types: a field's role may have changed
CREATE TRIGGER word_index_notetypes_upd AFTER UPDATE OF fields, deleted_at ON notetypes BEGIN
    INSERT INTO word_index_dirty (note_id)
        SELECT id FROM notes WHERE notetype_id = NEW.id
          AND id NOT IN (SELECT note_id FROM word_index_dirty);
END;

-- Subtitle captures carry the language of their note
CREATE TRIGGER word_index_captures_ins AFTER INSERT ON subtitle_captures BEGIN
    INSERT INTO word_index_dirty (note_id) SELECT NEW.note_id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = NEW.note_id);
END;
CREATE TRIGGER word_index_captures_upd AFTER UPDATE OF lang, note_id ON subtitle_captures BEGIN
    INSERT INTO word_index_dirty (note_id) SELECT NEW.note_id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = NEW.note_id);
    INSERT INTO word_index_dirty (note_id) SELECT OLD.note_id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = OLD.note_id);
END;
CREATE TRIGGER word_index_captures_del AFTER DELETE ON subtitle_captures BEGIN
    INSERT INTO word_index_dirty (note_id) SELECT OLD.note_id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = OLD.note_id);
END;

-- languages known by producers
CREATE TRIGGER word_index_note_language_ins AFTER INSERT ON word_note_language BEGIN
    INSERT INTO word_index_dirty (note_id) SELECT NEW.note_id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = NEW.note_id);
END;
CREATE TRIGGER word_index_note_language_upd AFTER UPDATE ON word_note_language BEGIN
    INSERT INTO word_index_dirty (note_id) SELECT NEW.note_id
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty WHERE note_id = NEW.note_id);
END;

-- manual marks
CREATE TRIGGER word_index_marks_ins AFTER INSERT ON word_marks BEGIN
    INSERT INTO word_index_dirty_keys (language, key) SELECT NEW.language, NEW.lemma
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty_keys
                          WHERE language = NEW.language AND key = NEW.lemma);
END;
CREATE TRIGGER word_index_marks_upd AFTER UPDATE ON word_marks BEGIN
    INSERT INTO word_index_dirty_keys (language, key) SELECT NEW.language, NEW.lemma
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty_keys
                          WHERE language = NEW.language AND key = NEW.lemma);
    INSERT INTO word_index_dirty_keys (language, key) SELECT OLD.language, OLD.lemma
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty_keys
                          WHERE language = OLD.language AND key = OLD.lemma);
END;
CREATE TRIGGER word_index_marks_del AFTER DELETE ON word_marks BEGIN
    INSERT INTO word_index_dirty_keys (language, key) SELECT OLD.language, OLD.lemma
        WHERE NOT EXISTS (SELECT 1 FROM word_index_dirty_keys
                          WHERE language = OLD.language AND key = OLD.lemma);
END;

-- the deck language setting (`words.language`, app or deck scope)
CREATE TRIGGER word_index_settings_ins AFTER INSERT ON settings
WHEN NEW.key = 'words.language' BEGIN
    INSERT INTO word_index_dirty (note_id)
        SELECT id FROM notes
        WHERE (NEW.scope = 'app' OR home_deck_id = substr(NEW.scope, 6))
          AND id NOT IN (SELECT note_id FROM word_index_dirty);
END;
CREATE TRIGGER word_index_settings_upd AFTER UPDATE ON settings
WHEN NEW.key = 'words.language' OR OLD.key = 'words.language' BEGIN
    INSERT INTO word_index_dirty (note_id)
        SELECT id FROM notes
        WHERE (NEW.scope = 'app' OR OLD.scope = 'app'
           OR home_deck_id = substr(NEW.scope, 6) OR home_deck_id = substr(OLD.scope, 6))
          AND id NOT IN (SELECT note_id FROM word_index_dirty);
END;
CREATE TRIGGER word_index_settings_del AFTER DELETE ON settings
WHEN OLD.key = 'words.language' BEGIN
    INSERT INTO word_index_dirty (note_id)
        SELECT id FROM notes
        WHERE (OLD.scope = 'app' OR home_deck_id = substr(OLD.scope, 6))
          AND id NOT IN (SELECT note_id FROM word_index_dirty);
END;

-- build the index of an existing collection on first open
INSERT INTO word_index_dirty (note_id) SELECT id FROM notes;
INSERT INTO word_index_dirty_keys (language, key) SELECT language, lemma FROM word_marks;
