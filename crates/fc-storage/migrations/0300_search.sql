-- 0300_search (v0.4): query language support.
--
-- * notes_fts text is now written folded (fc_core::query::text::fold: lowercase, Turkish
--   dotted/dotless i unified, diacritics removed). FTS5's own unicode61 tokenizer already
--   lowercases and strips diacritics; the one thing it cannot fold is dotless `ı` (and `İ`),
--   so existing rows only need that replacement to agree with new writes.
-- * Indexes for browser sort orders over large collections.
UPDATE notes_fts
   SET content = replace(replace(content, 'ı', 'i'), 'İ', 'i'),
       sort_field = replace(replace(sort_field, 'ı', 'i'), 'İ', 'i')
 WHERE content LIKE '%ı%' OR content LIKE '%İ%'
    OR sort_field LIKE '%ı%' OR sort_field LIKE '%İ%';

CREATE INDEX cards_created ON cards(created_at);
CREATE INDEX notes_sort_field ON notes(sort_field COLLATE NOCASE);
