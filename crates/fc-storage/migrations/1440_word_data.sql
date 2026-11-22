-- 1440_word_data (v1.4 B2, docs/03 §4.3): frequency rank, CEFR band and word families from a
-- versioned, local word-data pack per language (`fc_core::word_data`). Reference data, not
-- the user's: the app installs the pack it ships at start-up (`origin = 'bundled'`, replaced
-- when the shipped pack changes) and `words.data.import` replaces a language's pack with one
-- from a file (`origin = 'imported'`, never overwritten by a shipped one).
--
-- * `word_data_packs`: one installed pack per language, with its version, licence, sources
--   and attributions (`words.data.sources` shows them) and the checksum of its bytes.
-- * `word_data`: one row per form of the pack — every lemma (its own form), every inflected
--   form (`went` → `go`, with the lemma's rank and band) and every profile headword the
--   frequency list lacks (band only). Keyed by (language, form), so the word index joins it
--   on its key with one indexed probe. `rank` 1 = heard most; `band` A1–C2; `band_origin`
--   says whether the band comes from a CEFR profile list or is estimated from the rank.
-- * `word_family`: a derived lemma and the head of its word family (`actor` → `act`). Shown
--   next to a word; it never changes a word's status.

CREATE TABLE word_data_packs (
    language     TEXT PRIMARY KEY NOT NULL,
    version      TEXT NOT NULL,
    name         TEXT NOT NULL,
    licence      TEXT NOT NULL,
    sources      TEXT NOT NULL,
    notes        TEXT NOT NULL,
    origin       TEXT NOT NULL CHECK (origin IN ('bundled', 'imported')),
    words        INTEGER NOT NULL,
    forms        INTEGER NOT NULL,
    families     INTEGER NOT NULL,
    checksum     TEXT NOT NULL,
    installed_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE word_data (
    language    TEXT NOT NULL,
    form        TEXT NOT NULL,
    lemma       TEXT NOT NULL,
    rank        INTEGER,
    band        TEXT CHECK (band IN ('A1', 'A2', 'B1', 'B2', 'C1', 'C2')),
    band_origin TEXT CHECK (band_origin IN ('profile', 'rank')),
    PRIMARY KEY (language, form)
) STRICT, WITHOUT ROWID;
CREATE INDEX word_data_rank ON word_data (language, rank);

CREATE TABLE word_family (
    language TEXT NOT NULL,
    lemma    TEXT NOT NULL,
    head     TEXT NOT NULL,
    PRIMARY KEY (language, lemma)
) STRICT, WITHOUT ROWID;
CREATE INDEX word_family_head ON word_family (language, head);
