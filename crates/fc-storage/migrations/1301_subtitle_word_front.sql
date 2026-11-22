-- 1301_subtitle_word_front (v1.3 X1, docs/03 §5.1): the Subtitle card asks with the word only.
--
-- Hint rule: a hint may reveal context but never the answer. The v1.2 Subtitle front showed
-- the subtitle line under the word, so the line could not be the first hint; now the front
-- shows the form as it appeared (`surface`) large and the dictionary form (`word`) small
-- beside it when different (`{{unless_same:surface:word}}`), and the line is the first hint
-- (the word highlighted). The back is unchanged.
--
-- Same detection as 1300: only the built-in Subtitle type's `recognition` template whose
-- front is still exactly the shipped one, whose back is still the v1.3 back (1300) and whose
-- card type is still `classic` changes; any edited template keeps its text. The note type gets
-- `version + 1` and `updated_at` = now. Running it again changes nothing (the front no longer
-- matches). Notes and cards are not touched.

CREATE TEMP TABLE m1301_template (old_front TEXT NOT NULL, back TEXT NOT NULL, new_front TEXT NOT NULL);

INSERT INTO m1301_template (old_front, back, new_front) VALUES
    ('<div class="fc-word">{{word}}</div>{{#sentence}}<div class="fc-example">{{sentence}}</div>{{/sentence}}',
     '<div class="fc-word">{{word}}</div>' || char(10) || '<hr class="fc-answer">' || char(10) || '<div class="fc-part fc-part-meaning">{{meaning}}</div>{{#pos}}<div class="fc-pron">{{pos}}</div>{{/pos}}{{#sentence}}<div class="fc-part fc-part-example"><span class="fc-part-label" data-fc-part="example"></span><div class="fc-example-text">{{sentence}}</div>{{#translation}}<div class="fc-example-translation">{{translation}}</div>{{/translation}}</div>{{/sentence}}{{#image}}<div class="fc-media">{{image}}</div>{{/image}}{{#audio}}<div class="fc-media">{{audio}}</div>{{/audio}}{{#source}}<div class="fc-part fc-part-source"><span class="fc-part-label" data-fc-part="source"></span>{{source}}</div>{{/source}}',
     '<div class="fc-word">{{#surface}}{{surface}} <span class="fc-lemma">{{unless_same:surface:word}}</span>{{/surface}}{{^surface}}{{word}}{{/surface}}</div>');

CREATE TEMP TABLE m1301_new AS
SELECT n.id AS id,
    (SELECT json_group_array(json(CASE
            WHEN json_extract(v.value, '$.key') = 'recognition'
                 AND json_extract(v.value, '$.front') = t.old_front
                 AND json_extract(v.value, '$.back') = t.back
                 AND json_extract(v.value, '$.card_type') = 'classic'
            THEN json_set(v.value, '$.front', t.new_front)
            ELSE v.value END) ORDER BY v.key)
       FROM json_each(n.templates) v, m1301_template t) AS templates
FROM notetypes n
WHERE n.builtin = 'subtitle'
  AND json_type(n.templates) = 'array' AND json_array_length(n.templates) > 0;

UPDATE notetypes
SET templates = m.templates,
    version = notetypes.version + 1,
    updated_at = CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
FROM m1301_new m
WHERE m.id = notetypes.id
  AND json(m.templates) <> json(notetypes.templates);

DROP TABLE m1301_new;
DROP TABLE m1301_template;
