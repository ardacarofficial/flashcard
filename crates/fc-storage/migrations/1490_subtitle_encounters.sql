-- 1490_subtitle_encounters (v1.4 W1, docs/06 §5.1, docs/02 §25): a capture is an ENCOUNTER.
--
-- A `subtitle_captures` row was "capture → the note it made". Now it is the encounter itself:
-- one row per capture the extension sent, with its source kept as queryable columns, linked
-- to a note (term + meaning). Several encounters may share a note (two scenes of one meaning
-- = one card, two examples); exactly one of them is the note's PRIMARY encounter (the one
-- whose example, source, picture and sound the note's fields show).
--
-- New columns (all NULL for captures received before v1.4 unless the fact is still there;
-- nothing is invented):
-- * the term as clicked (`surface`), part of speech, the subtitle line as sent (`sentence`,
--   plain text), its translation, the term's UTF-16 range in the line (`term_start`,
--   `term_end`), what was picked (`kind`: word / phrase / sentence);
-- * the definition and where it came from (`definition`, `definition_source`: dict/ai/mt);
-- * the video: `video_id`, `title` (with its episode), `site`, `url`, `t_start_ms`,
--   `t_end_ms`; the media store hashes of the screenshot and the audio clip;
-- * `example_fields`: the note fields this encounter shows when it is the primary one
--   (`surface`, `sentence`, `translation`, `image`, `audio`, `source`, as JSON);
-- * `is_primary` (1 = the note's primary encounter; at most one per note), and
--   `review_note_id`: set when a capture of an already captured word arrived with a
--   meaning the app could not match to that note — it became a separate note, and the user
--   decides ("same meaning" merges into `review_note_id`, "different meaning" clears it).
-- `created_at` stays the time the app received it. Text IS stored: this is the user's own
-- local data (unlike `subtitle_capture_log`).
--
-- Backfill of existing captures (each made its own note in v1.2/v1.3): `surface`, the
-- screenshot/audio hashes and `example_fields` come from the note's current fields; `title`
-- from `note_origins` (1460). The line, definition, times, kind, video id and URL were not
-- kept as such, so they stay NULL. The earliest capture of a note is its primary encounter.
--
-- Also:
-- * `subtitle_capture_log.result` accepts `attached` (a capture added as another example of
--   an existing note: stored, no new card). SQLite cannot change a CHECK, hence the copy
--   (rows and `seq` kept).
-- * The built-in Subtitle note type gets an `examples` field (the note's other encounters,
--   filled by the app) and its back shows it before the source — only where the back is still
--   exactly the shipped v1.3 one; an empty field renders nothing, so existing cards look the
--   same. Notes are not touched.
-- * `subtitle_encounter_counts`: encounters per note (for `karşılaşma:`/`seen:`).

ALTER TABLE subtitle_captures ADD COLUMN surface TEXT;
ALTER TABLE subtitle_captures ADD COLUMN pos TEXT;
ALTER TABLE subtitle_captures ADD COLUMN sentence TEXT;
ALTER TABLE subtitle_captures ADD COLUMN translation TEXT;
ALTER TABLE subtitle_captures ADD COLUMN term_start INTEGER;
ALTER TABLE subtitle_captures ADD COLUMN term_end INTEGER;
ALTER TABLE subtitle_captures ADD COLUMN kind TEXT
    CHECK (kind IS NULL OR kind IN ('word', 'phrase', 'sentence'));
ALTER TABLE subtitle_captures ADD COLUMN definition TEXT;
ALTER TABLE subtitle_captures ADD COLUMN definition_source TEXT
    CHECK (definition_source IS NULL OR definition_source IN ('dict', 'ai', 'mt'));
ALTER TABLE subtitle_captures ADD COLUMN video_id TEXT;
ALTER TABLE subtitle_captures ADD COLUMN title TEXT;
ALTER TABLE subtitle_captures ADD COLUMN site TEXT;
ALTER TABLE subtitle_captures ADD COLUMN url TEXT;
ALTER TABLE subtitle_captures ADD COLUMN t_start_ms INTEGER;
ALTER TABLE subtitle_captures ADD COLUMN t_end_ms INTEGER;
ALTER TABLE subtitle_captures ADD COLUMN screenshot_hash TEXT;
ALTER TABLE subtitle_captures ADD COLUMN audio_hash TEXT;
ALTER TABLE subtitle_captures ADD COLUMN example_fields TEXT NOT NULL DEFAULT '{}';
ALTER TABLE subtitle_captures ADD COLUMN is_primary INTEGER NOT NULL DEFAULT 1
    CHECK (is_primary IN (0, 1));
ALTER TABLE subtitle_captures ADD COLUMN review_note_id TEXT;

-- Backfill from what exists.
UPDATE subtitle_captures SET
    surface = (SELECT nullif(json_extract(n.fields, '$.surface'), '')
                 FROM notes n WHERE n.id = subtitle_captures.note_id),
    screenshot_hash = (SELECT CASE WHEN json_extract(n.fields, '$.image') LIKE 'media:%'
                                   THEN substr(json_extract(n.fields, '$.image'), 7) END
                         FROM notes n WHERE n.id = subtitle_captures.note_id),
    audio_hash = (SELECT CASE WHEN json_extract(n.fields, '$.audio') LIKE 'media:%'
                              THEN substr(json_extract(n.fields, '$.audio'), 7) END
                    FROM notes n WHERE n.id = subtitle_captures.note_id),
    title = (SELECT nullif(o.source, '') FROM note_origins o
              WHERE o.note_id = subtitle_captures.note_id),
    example_fields = coalesce((
        SELECT json_group_object(f.key, f.value)
          FROM notes n, json_each(n.fields) f
         WHERE n.id = subtitle_captures.note_id
           AND f.key IN ('surface', 'sentence', 'translation', 'image', 'audio', 'source')
           AND f.type = 'text' AND f.value <> ''), '{}');

-- One primary per note: the earliest capture.
UPDATE subtitle_captures SET is_primary = 0
 WHERE EXISTS (SELECT 1 FROM subtitle_captures o
                WHERE o.note_id = subtitle_captures.note_id
                  AND (o.created_at < subtitle_captures.created_at
                       OR (o.created_at = subtitle_captures.created_at
                           AND o.capture_id < subtitle_captures.capture_id)));

CREATE UNIQUE INDEX subtitle_captures_primary ON subtitle_captures (note_id) WHERE is_primary = 1;
CREATE INDEX subtitle_captures_note ON subtitle_captures (note_id, created_at);
CREATE INDEX subtitle_captures_video ON subtitle_captures (video_id, t_start_ms)
    WHERE video_id IS NOT NULL;
CREATE INDEX subtitle_captures_review ON subtitle_captures (review_note_id)
    WHERE review_note_id IS NOT NULL;

CREATE VIEW subtitle_encounter_counts AS
SELECT note_id, count(*) AS encounters FROM subtitle_captures GROUP BY note_id;

-- `attached` capture attempts.
CREATE TABLE subtitle_capture_log_new (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    capture_id  TEXT NOT NULL,
    at          INTEGER NOT NULL,
    result      TEXT NOT NULL
                CHECK (result IN ('created', 'duplicate', 'rejected', 'media_missing', 'attached')),
    reason      TEXT,
    card_id     TEXT,
    duration_ms INTEGER NOT NULL
) STRICT;
INSERT INTO subtitle_capture_log_new (seq, capture_id, at, result, reason, card_id, duration_ms)
SELECT seq, capture_id, at, result, reason, card_id, duration_ms FROM subtitle_capture_log;
DROP TABLE subtitle_capture_log;
ALTER TABLE subtitle_capture_log_new RENAME TO subtitle_capture_log;
CREATE INDEX subtitle_capture_log_capture ON subtitle_capture_log (capture_id);

-- The Subtitle note type: `examples` field + its part on the back.
CREATE TEMP TABLE m1490_back (old_back TEXT NOT NULL, new_back TEXT NOT NULL);
INSERT INTO m1490_back (old_back, new_back) VALUES (
    '<div class="fc-word">{{word}}</div>' || char(10) || '<hr class="fc-answer">' || char(10) || '<div class="fc-part fc-part-meaning">{{meaning}}</div>{{#pos}}<div class="fc-pron">{{pos}}</div>{{/pos}}{{#sentence}}<div class="fc-part fc-part-example"><span class="fc-part-label" data-fc-part="example"></span><div class="fc-example-text">{{sentence}}</div>{{#translation}}<div class="fc-example-translation">{{translation}}</div>{{/translation}}</div>{{/sentence}}{{#image}}<div class="fc-media">{{image}}</div>{{/image}}{{#audio}}<div class="fc-media">{{audio}}</div>{{/audio}}{{#source}}<div class="fc-part fc-part-source"><span class="fc-part-label" data-fc-part="source"></span>{{source}}</div>{{/source}}',
    '<div class="fc-word">{{word}}</div>' || char(10) || '<hr class="fc-answer">' || char(10) || '<div class="fc-part fc-part-meaning">{{meaning}}</div>{{#pos}}<div class="fc-pron">{{pos}}</div>{{/pos}}{{#sentence}}<div class="fc-part fc-part-example"><span class="fc-part-label" data-fc-part="example"></span><div class="fc-example-text">{{sentence}}</div>{{#translation}}<div class="fc-example-translation">{{translation}}</div>{{/translation}}</div>{{/sentence}}{{#image}}<div class="fc-media">{{image}}</div>{{/image}}{{#audio}}<div class="fc-media">{{audio}}</div>{{/audio}}{{#examples}}<div class="fc-part fc-part-examples">{{examples}}</div>{{/examples}}{{#source}}<div class="fc-part fc-part-source"><span class="fc-part-label" data-fc-part="source"></span>{{source}}</div>{{/source}}');

CREATE TEMP TABLE m1490_new AS
SELECT n.id AS id,
    CASE WHEN EXISTS (SELECT 1 FROM json_each(n.fields) x WHERE json_extract(x.value, '$.key') = 'examples')
         THEN n.fields
         ELSE json_insert(n.fields, '$[#]', json_object(
             'key', 'examples',
             'name', CASE WHEN EXISTS (SELECT 1 FROM json_each(n.fields) x
                                        WHERE json_extract(x.value, '$.key') = 'word'
                                          AND json_extract(x.value, '$.name') = 'Kelime')
                          THEN 'Örnekler' ELSE 'Examples' END,
             'kind', 'rich_text',
             'required', json('false'),
             'unique', json('false'),
             'searchable', json('true'),
             'default_value', json('null')))
    END AS fields,
    (SELECT json_group_array(json(CASE
            WHEN json_extract(v.value, '$.key') = 'recognition'
                 AND json_extract(v.value, '$.back') = b.old_back
            THEN json_set(v.value, '$.back', b.new_back)
            ELSE v.value END) ORDER BY v.key)
       FROM json_each(n.templates) v, m1490_back b) AS templates
FROM notetypes n
WHERE n.builtin = 'subtitle'
  AND json_type(n.fields) = 'array'
  AND json_type(n.templates) = 'array' AND json_array_length(n.templates) > 0;

UPDATE notetypes
SET fields = m.fields,
    templates = m.templates,
    version = notetypes.version + 1,
    updated_at = CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
FROM m1490_new m
WHERE m.id = notetypes.id
  AND (json(m.fields) <> json(notetypes.fields) OR json(m.templates) <> json(notetypes.templates));

DROP TABLE m1490_new;
DROP TABLE m1490_back;
