-- 1300_vocabulary_roles (v1.3, docs/03 §5.1): field roles and answer fields for the built-in
-- note types, and the labelled answer parts of the vocabulary backs — in existing
-- collections too (seeding only adds missing note types).
--
-- Only what the user has not customised changes:
--
-- * a field gets its role when the note type is that built-in, the field still has the
--   built-in key and kind, and it has no role yet;
-- * a template gets its v1.3 back and `answer_field` only when its front, back and card type
--   are still exactly the shipped v1.2 ones (names may be renamed: they are not compared)
--   and it has no answer field yet — any edited template keeps its text and grading;
-- * the shared CSS is set only where it is still the shipped empty CSS (the rules only
--   style the `fc-part*` classes of the new backs).
--
-- A note type that changed gets `version + 1` and `updated_at` = now. Running these
-- statements again changes nothing (idempotent): roles already set, templates no longer
-- equal to v1.2, CSS no longer empty. Notes and cards are not touched.

CREATE TEMP TABLE m1300_fields (builtin TEXT NOT NULL, key TEXT NOT NULL, kind TEXT NOT NULL, role TEXT NOT NULL);

INSERT INTO m1300_fields (builtin, key, kind, role) VALUES
    ('classic', 'front', 'rich_text', 'term'),
    ('classic', 'back', 'rich_text', 'meaning'),
    ('classic_reverse', 'front', 'rich_text', 'term'),
    ('classic_reverse', 'back', 'rich_text', 'meaning'),
    ('optional_reverse', 'front', 'rich_text', 'term'),
    ('optional_reverse', 'back', 'rich_text', 'meaning'),
    ('typing', 'front', 'rich_text', 'term'),
    ('typing', 'back', 'rich_text', 'meaning'),
    ('cloze', 'extra', 'rich_text', 'note'),
    ('multiple_choice', 'question', 'rich_text', 'term'),
    ('multiple_choice', 'answer', 'text', 'meaning'),
    ('multiple_choice', 'extra', 'rich_text', 'note'),
    ('image_occlusion', 'extra', 'rich_text', 'note'),
    ('image_quiz', 'answer', 'text', 'term'),
    ('image_quiz', 'extra', 'rich_text', 'note'),
    ('audio', 'answer', 'text', 'term'),
    ('audio', 'extra', 'rich_text', 'note'),
    ('vocabulary', 'word', 'rich_text', 'term'),
    ('vocabulary', 'meaning', 'rich_text', 'meaning'),
    ('vocabulary', 'example', 'rich_text', 'example'),
    ('sentence', 'sentence', 'rich_text', 'example'),
    ('sentence', 'translation', 'rich_text', 'example_translation'),
    ('sentence', 'word', 'rich_text', 'term'),
    ('sentence', 'meaning', 'rich_text', 'meaning'),
    ('sentence', 'source', 'text', 'source'),
    ('subtitle', 'word', 'rich_text', 'term'),
    ('subtitle', 'surface', 'text', 'surface'),
    ('subtitle', 'meaning', 'rich_text', 'meaning'),
    ('subtitle', 'sentence', 'rich_text', 'example'),
    ('subtitle', 'translation', 'rich_text', 'example_translation'),
    ('subtitle', 'source', 'rich_text', 'source');

CREATE TEMP TABLE m1300_templates (
    builtin TEXT NOT NULL, key TEXT NOT NULL, old_front TEXT NOT NULL, old_back TEXT NOT NULL,
    card_type TEXT NOT NULL, new_back TEXT NOT NULL, answer_field TEXT
);

INSERT INTO m1300_templates (builtin, key, old_front, old_back, card_type, new_back, answer_field) VALUES
    ('classic', 'forward',
     '{{front}}',
     '{{front}}' || char(10) || '<hr id="answer">' || char(10) || '{{back}}',
     'classic',
     '{{front}}' || char(10) || '<hr id="answer">' || char(10) || '{{back}}',
     'back'),
    ('classic_reverse', 'forward',
     '{{front}}',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{back}}',
     'classic',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{back}}',
     'back'),
    ('classic_reverse', 'reverse',
     '{{back}}',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{front}}',
     'classic',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{front}}',
     'front'),
    ('optional_reverse', 'forward',
     '{{front}}',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{back}}',
     'classic',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{back}}',
     'back'),
    ('optional_reverse', 'reverse',
     '{{#add_reverse}}{{back}}{{/add_reverse}}',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{front}}',
     'classic',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{front}}',
     'front'),
    ('typing', 'forward',
     '{{front}}' || char(10) || '' || char(10) || '{{type:back}}',
     '{{front}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{type:back}}',
     'typing',
     '{{front}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{type:back}}',
     'back'),
    ('image_quiz', 'forward',
     '{{image}}' || char(10) || '' || char(10) || '{{type:answer}}',
     '{{image}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{type:answer}}{{#extra}}<div class="fc-extra">{{extra}}</div>{{/extra}}',
     'typing',
     '{{image}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{type:answer}}{{#extra}}<div class="fc-extra">{{extra}}</div>{{/extra}}',
     'answer'),
    ('audio', 'forward',
     '{{audio}}' || char(10) || '' || char(10) || '{{type:answer}}',
     '{{audio}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{type:answer}}{{#extra}}<div class="fc-extra">{{extra}}</div>{{/extra}}',
     'typing',
     '{{audio}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{type:answer}}{{#extra}}<div class="fc-extra">{{extra}}</div>{{/extra}}',
     'answer'),
    ('vocabulary', 'recognition',
     '{{word}}{{#pronunciation}}<div class="fc-pron">{{pronunciation}}</div>{{/pronunciation}}{{#audio}}<div>{{audio}}</div>{{/audio}}',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{meaning}}{{#image}}<div>{{image}}</div>{{/image}}{{#example}}<div class="fc-example">{{example}}</div>{{/example}}',
     'classic',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '<div class="fc-part fc-part-meaning">{{meaning}}</div>{{#image}}<div class="fc-media">{{image}}</div>{{/image}}{{#example}}<div class="fc-part fc-part-example"><span class="fc-part-label" data-fc-part="example"></span><div class="fc-example-text">{{example}}</div></div>{{/example}}',
     'meaning'),
    ('vocabulary', 'production',
     '{{meaning}}{{#image}}<div>{{image}}</div>{{/image}}',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{word}}{{#pronunciation}}<div class="fc-pron">{{pronunciation}}</div>{{/pronunciation}}{{#audio}}<div>{{audio}}</div>{{/audio}}{{#example}}<div class="fc-example">{{example}}</div>{{/example}}',
     'classic',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '<div class="fc-part fc-part-answer">{{word}}</div>{{#pronunciation}}<div class="fc-pron">{{pronunciation}}</div>{{/pronunciation}}{{#audio}}<div class="fc-media">{{audio}}</div>{{/audio}}{{#example}}<div class="fc-part fc-part-example"><span class="fc-part-label" data-fc-part="example"></span><div class="fc-example-text">{{example}}</div></div>{{/example}}',
     'word'),
    ('sentence', 'forward',
     '{{#image}}<div>{{image}}</div>{{/image}}{{sentence}}{{#audio}}<div>{{audio}}</div>{{/audio}}',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{translation}}{{#word}}<div class="fc-example"><b>{{word}}</b>{{#meaning}}: {{meaning}}{{/meaning}}</div>{{/word}}{{#source}}<div class="fc-source">{{source}}</div>{{/source}}',
     'classic',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{#translation}}<div class="fc-part fc-part-meaning">{{translation}}</div>{{/translation}}{{#word}}<div class="fc-part fc-part-word"><b>{{word}}</b>{{#meaning}}: {{meaning}}{{/meaning}}</div>{{/word}}{{#source}}<div class="fc-part fc-part-source"><span class="fc-part-label" data-fc-part="source"></span>{{source}}</div>{{/source}}',
     'translation'),
    ('subtitle', 'recognition',
     '<div class="fc-word">{{word}}</div>{{#sentence}}<div class="fc-example">{{sentence}}</div>{{/sentence}}',
     '{{FrontSide}}' || char(10) || '<hr class="fc-answer">' || char(10) || '{{meaning}}{{#pos}}<div class="fc-pron">{{pos}}</div>{{/pos}}{{#translation}}<div class="fc-example">{{translation}}</div>{{/translation}}{{#image}}<div>{{image}}</div>{{/image}}{{#audio}}<div>{{audio}}</div>{{/audio}}{{#source}}<div class="fc-source">{{source}}</div>{{/source}}',
     'classic',
     '<div class="fc-word">{{word}}</div>' || char(10) || '<hr class="fc-answer">' || char(10) || '<div class="fc-part fc-part-meaning">{{meaning}}</div>{{#pos}}<div class="fc-pron">{{pos}}</div>{{/pos}}{{#sentence}}<div class="fc-part fc-part-example"><span class="fc-part-label" data-fc-part="example"></span><div class="fc-example-text">{{sentence}}</div>{{#translation}}<div class="fc-example-translation">{{translation}}</div>{{/translation}}</div>{{/sentence}}{{#image}}<div class="fc-media">{{image}}</div>{{/image}}{{#audio}}<div class="fc-media">{{audio}}</div>{{/audio}}{{#source}}<div class="fc-part fc-part-source"><span class="fc-part-label" data-fc-part="source"></span>{{source}}</div>{{/source}}',
     'meaning');

CREATE TEMP TABLE m1300_css (builtin TEXT NOT NULL, css TEXT NOT NULL);

INSERT INTO m1300_css (builtin, css) VALUES
    ('vocabulary', '.fc-part { margin-top: 0.75em; }' || char(10) || '.fc-part-meaning, .fc-part-answer { font-size: 1.3em; font-weight: 600; line-height: 1.35; }' || char(10) || '.fc-part-label { display: block; margin-bottom: 0.15em; font-size: 0.7em; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; opacity: 0.65; }' || char(10) || '.fc-part-label:empty { display: none; }' || char(10) || '.fc-part-example { padding-left: 0.75em; border-left: 2px solid rgba(128, 128, 128, 0.45); }' || char(10) || '.fc-example-translation { margin-top: 0.25em; opacity: 0.7; }' || char(10) || '.fc-part-source { font-size: 0.8em; opacity: 0.7; }' || char(10) || '.fc-part-source .fc-part-label { display: inline; margin-right: 0.4em; }' || char(10) || ''),
    ('sentence', '.fc-part { margin-top: 0.75em; }' || char(10) || '.fc-part-meaning, .fc-part-answer { font-size: 1.3em; font-weight: 600; line-height: 1.35; }' || char(10) || '.fc-part-label { display: block; margin-bottom: 0.15em; font-size: 0.7em; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; opacity: 0.65; }' || char(10) || '.fc-part-label:empty { display: none; }' || char(10) || '.fc-part-example { padding-left: 0.75em; border-left: 2px solid rgba(128, 128, 128, 0.45); }' || char(10) || '.fc-example-translation { margin-top: 0.25em; opacity: 0.7; }' || char(10) || '.fc-part-source { font-size: 0.8em; opacity: 0.7; }' || char(10) || '.fc-part-source .fc-part-label { display: inline; margin-right: 0.4em; }' || char(10) || ''),
    ('subtitle', '.fc-part { margin-top: 0.75em; }' || char(10) || '.fc-part-meaning, .fc-part-answer { font-size: 1.3em; font-weight: 600; line-height: 1.35; }' || char(10) || '.fc-part-label { display: block; margin-bottom: 0.15em; font-size: 0.7em; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; opacity: 0.65; }' || char(10) || '.fc-part-label:empty { display: none; }' || char(10) || '.fc-part-example { padding-left: 0.75em; border-left: 2px solid rgba(128, 128, 128, 0.45); }' || char(10) || '.fc-example-translation { margin-top: 0.25em; opacity: 0.7; }' || char(10) || '.fc-part-source { font-size: 0.8em; opacity: 0.7; }' || char(10) || '.fc-part-source .fc-part-label { display: inline; margin-right: 0.4em; }' || char(10) || '');

CREATE TEMP TABLE m1300_new AS
SELECT n.id AS id,
    (SELECT json_group_array(json(CASE
            WHEN m.role IS NOT NULL
                 AND json_type(f.value, '$.role') IS NULL
                 AND json_extract(f.value, '$.kind') = m.kind
            THEN json_set(f.value, '$.role', m.role)
            ELSE f.value END) ORDER BY f.key)
       FROM json_each(n.fields) f
       LEFT JOIN m1300_fields m
         ON m.builtin = n.builtin AND m.key = json_extract(f.value, '$.key')) AS fields,
    (SELECT json_group_array(json(CASE
            WHEN t.key IS NOT NULL
                 AND json_type(v.value, '$.answer_field') IS NULL
                 AND json_extract(v.value, '$.front') = t.old_front
                 AND json_extract(v.value, '$.back') = t.old_back
                 AND json_extract(v.value, '$.card_type') = t.card_type
            THEN CASE WHEN t.answer_field IS NULL
                      THEN json_set(v.value, '$.back', t.new_back)
                      ELSE json_set(v.value, '$.back', t.new_back, '$.answer_field', t.answer_field)
                 END
            ELSE v.value END) ORDER BY v.key)
       FROM json_each(n.templates) v
       LEFT JOIN m1300_templates t
         ON t.builtin = n.builtin AND t.key = json_extract(v.value, '$.key')) AS templates,
    CASE WHEN n.css = '' THEN coalesce((SELECT c.css FROM m1300_css c WHERE c.builtin = n.builtin), '')
         ELSE n.css END AS css
FROM notetypes n
WHERE n.builtin IS NOT NULL
  AND json_type(n.fields) = 'array' AND json_array_length(n.fields) > 0
  AND json_type(n.templates) = 'array' AND json_array_length(n.templates) > 0;

UPDATE notetypes
SET fields = m.fields,
    templates = m.templates,
    css = m.css,
    version = notetypes.version + 1,
    updated_at = CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
FROM m1300_new m
WHERE m.id = notetypes.id
  AND (json(m.fields) <> json(notetypes.fields)
       OR json(m.templates) <> json(notetypes.templates)
       OR m.css <> notetypes.css);

DROP TABLE m1300_new;
DROP TABLE m1300_css;
DROP TABLE m1300_templates;
DROP TABLE m1300_fields;
