-- 0302_search_plain_text (Phase-2 integration): the search index holds folded **plain text**.
--
-- Since the v0.3 + v0.5 merge, note fields carry cloze markup (`{{c1::Ankara::city}}`),
-- `[[links]]`, HTML and bare `media:<sha256>` references (image/audio fields), and imported
-- notes were indexed as written. Rebuild every row from the notes with the same functions a
-- note write uses (`fc_index_text` = fc_core::note::searchable_text_of + fold, `fc_index_sort`
-- = cloze/link markup removed + fold; registered on every connection before migrations run).
-- Trashed notes keep their row (as note writes do; the trash purge removes it).
DELETE FROM notes_fts;
UPDATE notes SET fts_rowid = NULL;
UPDATE notes SET fts_rowid = rowid;
INSERT INTO notes_fts (rowid, note_id, sort_field, content)
SELECT n.rowid, n.id, fc_index_sort(n.sort_field), fc_index_text(t.fields, n.fields)
  FROM notes n JOIN notetypes t ON t.id = n.notetype_id;
