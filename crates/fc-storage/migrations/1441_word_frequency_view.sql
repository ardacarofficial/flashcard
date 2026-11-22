-- 1441_word_frequency_view (v1.4 B2): the seam of the language queries (B3, docs/02 §9.1).
-- `sıklık`/`freq` and `cefr`/`level` read an optional `word_frequency(language, key, rank,
-- band)`; the word data (1440) provides it: one row per pack form, keyed like the word index
-- (`key` = a lemma, an inflected form or a profile headword), so a note's index key finds its
-- rank and band with one primary-key probe of `word_data`. Expressions without a pack row of
-- their own have no rank and no band here (their band is estimated in the app only).
CREATE VIEW word_frequency AS
    SELECT language, form AS key, rank, band FROM word_data;
