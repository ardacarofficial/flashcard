//! Built-in note types (docs/02 §3): every one is a plain [`NoteType`] — fields + templates
//! naming a card type engine — so users can copy and change them freely. Each has a
//! well-known id (the same in every collection) and a `builtin` marker; the app seeds the
//! missing ones on startup with names in the UI language.

use crate::i18n::{Locale, LocalizedText};
use crate::id::NoteTypeId;
use crate::notetype::{
    builtin as classic_builtin, CardTemplate, FieldDef, FieldKey, FieldKind, FieldRole, NoteType,
    NoteTypeKind,
};
use crate::time::Timestamp;
use uuid::Uuid;

/// Card type engine ids of the built-in engines.
pub mod engine {
    /// Self-graded front/back card (also reverse and optional reverse).
    pub const CLASSIC: &str = "classic";
    /// Typed answer.
    pub const TYPING: &str = "typing";
    /// Cloze deletion (one card per `cN`).
    pub const CLOZE: &str = "cloze";
    /// Multiple choice.
    pub const MULTIPLE_CHOICE: &str = "multiple_choice";
    /// Image occlusion (one card per mask).
    pub const IMAGE_OCCLUSION: &str = "image_occlusion";
}

const fn id(n: u128) -> NoteTypeId {
    NoteTypeId(Uuid::from_u128(
        0x0190_0000_0000_7000_8000_0000_0000_0000 | n,
    ))
}

/// A built-in note type: marker, id and constructor.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinNoteType {
    /// `builtin` marker.
    pub key: &'static str,
    /// Well-known id.
    pub id: NoteTypeId,
    /// Display name.
    pub name: LocalizedText,
    build: fn(Timestamp, Locale) -> NoteType,
}

impl BuiltinNoteType {
    /// Builds the note type with names in `locale`.
    pub fn build(&self, now: Timestamp, locale: Locale) -> NoteType {
        (self.build)(now, locale)
    }
}

/// Every built-in note type, in menu order.
pub const ALL: &[BuiltinNoteType] = &[
    BuiltinNoteType {
        key: classic_builtin::CLASSIC,
        id: classic_builtin::CLASSIC_ID,
        name: LocalizedText {
            en: "Classic",
            tr: "Klasik",
        },
        build: classic,
    },
    BuiltinNoteType {
        key: "classic_reverse",
        id: id(2),
        name: LocalizedText {
            en: "Classic + reverse",
            tr: "Klasik + ters",
        },
        build: classic_reverse,
    },
    BuiltinNoteType {
        key: "optional_reverse",
        id: id(3),
        name: LocalizedText {
            en: "Optional reverse",
            tr: "İsteğe bağlı ters",
        },
        build: optional_reverse,
    },
    BuiltinNoteType {
        key: "typing",
        id: id(4),
        name: LocalizedText {
            en: "Type the answer",
            tr: "Yazarak cevaplama",
        },
        build: typing,
    },
    BuiltinNoteType {
        key: "cloze",
        id: id(5),
        name: LocalizedText {
            en: "Cloze",
            tr: "Boşluk doldurma",
        },
        build: cloze,
    },
    BuiltinNoteType {
        key: "multiple_choice",
        id: id(6),
        name: LocalizedText {
            en: "Multiple choice",
            tr: "Çoktan seçmeli",
        },
        build: multiple_choice,
    },
    BuiltinNoteType {
        key: "image_occlusion",
        id: id(7),
        name: LocalizedText {
            en: "Image occlusion",
            tr: "Resim üzerinde gizleme",
        },
        build: image_occlusion,
    },
    BuiltinNoteType {
        key: "image_quiz",
        id: id(8),
        name: LocalizedText {
            en: "Image quiz",
            tr: "Resimli quiz",
        },
        build: image_quiz,
    },
    BuiltinNoteType {
        key: "audio",
        id: id(9),
        name: LocalizedText {
            en: "Listening card",
            tr: "Sesli kart",
        },
        build: audio,
    },
    BuiltinNoteType {
        key: "vocabulary",
        id: id(10),
        name: LocalizedText {
            en: "Vocabulary",
            tr: "Kelime kartı",
        },
        build: vocabulary,
    },
    BuiltinNoteType {
        key: "sentence",
        id: id(11),
        name: LocalizedText {
            en: "Sentence card",
            tr: "Cümle kartı",
        },
        build: sentence,
    },
    BuiltinNoteType {
        key: SUBTITLE,
        id: id(12),
        name: LocalizedText {
            en: "Subtitle card",
            tr: "Altyazı kartı",
        },
        build: subtitle,
    },
];

/// `builtin` marker of the Subtitle card (v1.2): captures of the Subtitle browser extension.
pub const SUBTITLE: &str = "subtitle";

/// Built-in by marker.
pub fn find(key: &str) -> Option<&'static BuiltinNoteType> {
    ALL.iter().find(|b| b.key == key)
}

struct F {
    key: &'static str,
    name: LocalizedText,
    kind: FieldKind,
    required: bool,
    unique: bool,
    role: Option<FieldRole>,
}

const fn f(key: &'static str, en: &'static str, tr: &'static str, kind: FieldKind) -> F {
    F {
        key,
        name: LocalizedText { en, tr },
        kind,
        required: false,
        unique: false,
        role: None,
    }
}

impl F {
    const fn req(mut self) -> Self {
        self.required = true;
        self
    }
    const fn uniq(mut self) -> Self {
        self.unique = true;
        self.required = true;
        self
    }
    const fn role(mut self, role: FieldRole) -> Self {
        self.role = Some(role);
        self
    }
}

struct T {
    key: &'static str,
    name: LocalizedText,
    front: &'static str,
    back: &'static str,
    engine: &'static str,
    /// Key of the field the card asks for ([`CardTemplate::answer_field`]).
    answer: Option<&'static str>,
}

/// CSS of the labelled answer parts (v1.3) of the vocabulary built-ins: the meaning largest,
/// the example set apart with its translation under it, the source smallest. Labels are
/// empty `span.fc-part-label[data-fc-part]` elements the app fills with the localized label
/// (`cards.part.<part>`); without the app they stay hidden.
pub const PARTS_CSS: &str = ".fc-part { margin-top: 0.75em; }
.fc-part-meaning, .fc-part-answer { font-size: 1.3em; font-weight: 600; line-height: 1.35; }
.fc-part-label { display: block; margin-bottom: 0.15em; font-size: 0.7em; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; opacity: 0.65; }
.fc-part-label:empty { display: none; }
.fc-part-example { padding-left: 0.75em; border-left: 2px solid rgba(128, 128, 128, 0.45); }
.fc-example-translation { margin-top: 0.25em; opacity: 0.7; }
.fc-part-source { font-size: 0.8em; opacity: 0.7; }
.fc-part-source .fc-part-label { display: inline; margin-right: 0.4em; }
";

#[allow(clippy::too_many_arguments)]
fn make(
    now: Timestamp,
    locale: Locale,
    which: &str,
    kind: NoteTypeKind,
    fields: &[F],
    templates: &[T],
    css: &str,
) -> NoteType {
    let b = find(which);
    NoteType {
        id: b.map_or(id(0), |b| b.id),
        name: b.map_or(which, |b| b.name.get(locale)).to_owned(),
        kind,
        fields: fields
            .iter()
            .map(|x| FieldDef {
                key: FieldKey::new(x.key),
                name: x.name.get(locale).to_owned(),
                kind: x.kind,
                required: x.required,
                unique: x.unique,
                searchable: !matches!(
                    x.kind,
                    FieldKind::Image | FieldKind::Audio | FieldKind::Video
                ),
                default_value: None,
                role: x.role,
            })
            .collect(),
        templates: templates
            .iter()
            .map(|t| CardTemplate {
                key: t.key.to_owned(),
                name: t.name.get(locale).to_owned(),
                front: t.front.to_owned(),
                back: t.back.to_owned(),
                card_type: t.engine.to_owned(),
                answer_field: t.answer.map(FieldKey::new),
            })
            .collect(),
        css: css.to_owned(),
        sort_field: FieldKey::new(fields.first().map_or("", |x| x.key)),
        version: 1,
        builtin: Some(which.to_owned()),
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
}

const FRONT: F = f("front", "Front", "Ön", FieldKind::RichText)
    .uniq()
    .role(FieldRole::Term);
const BACK: F = f("back", "Back", "Arka", FieldKind::RichText).role(FieldRole::Meaning);
const FORWARD: T = T {
    key: "forward",
    name: LocalizedText {
        en: "Forward",
        tr: "İleri",
    },
    front: "{{front}}",
    back: "{{FrontSide}}\n<hr class=\"fc-answer\">\n{{back}}",
    engine: engine::CLASSIC,
    answer: Some("back"),
};

/// Classic (front/back). Same as [`classic_builtin::classic`] with localized names.
pub fn classic(now: Timestamp, locale: Locale) -> NoteType {
    let (name, front, back) = match locale {
        Locale::Tr => ("Klasik", "Ön", "Arka"),
        Locale::En => ("Classic", "Front", "Back"),
    };
    classic_builtin::classic(now, name, front, back)
}

/// Classic + reverse: two cards per note.
pub fn classic_reverse(now: Timestamp, locale: Locale) -> NoteType {
    make(
        now,
        locale,
        "classic_reverse",
        NoteTypeKind::Standard,
        &[FRONT, BACK.req()],
        &[
            FORWARD,
            T {
                key: "reverse",
                name: LocalizedText {
                    en: "Reverse",
                    tr: "Ters",
                },
                front: "{{back}}",
                back: "{{FrontSide}}\n<hr class=\"fc-answer\">\n{{front}}",
                engine: engine::CLASSIC,
                answer: Some("front"),
            },
        ],
        "",
    )
}

/// Optional reverse: the reverse card exists only when "add reverse" is filled.
pub fn optional_reverse(now: Timestamp, locale: Locale) -> NoteType {
    make(
        now,
        locale,
        "optional_reverse",
        NoteTypeKind::Standard,
        &[
            FRONT,
            BACK.req(),
            f(
                "add_reverse",
                "Add reverse (any text)",
                "Ters kart ekle (herhangi bir metin)",
                FieldKind::Text,
            ),
        ],
        &[
            FORWARD,
            T {
                key: "reverse",
                name: LocalizedText {
                    en: "Reverse",
                    tr: "Ters",
                },
                front: "{{#add_reverse}}{{back}}{{/add_reverse}}",
                back: "{{FrontSide}}\n<hr class=\"fc-answer\">\n{{front}}",
                engine: engine::CLASSIC,
                answer: Some("front"),
            },
        ],
        "",
    )
}

/// Type the answer.
pub fn typing(now: Timestamp, locale: Locale) -> NoteType {
    make(
        now,
        locale,
        "typing",
        NoteTypeKind::Standard,
        &[FRONT, BACK.req()],
        &[T {
            key: "forward",
            name: FORWARD.name,
            front: "{{front}}\n\n{{type:back}}",
            back: "{{front}}\n<hr class=\"fc-answer\">\n{{type:back}}",
            engine: engine::TYPING,
            answer: Some("back"),
        }],
        "",
    )
}

/// Cloze deletion.
pub fn cloze(now: Timestamp, locale: Locale) -> NoteType {
    make(
        now,
        locale,
        "cloze",
        NoteTypeKind::Cloze,
        &[
            f("text", "Text", "Metin", FieldKind::RichText).uniq(),
            f("extra", "Extra", "Ek bilgi", FieldKind::RichText).role(FieldRole::Note),
        ],
        &[T {
            key: "cloze",
            name: LocalizedText::same("Cloze"),
            front: "{{cloze:text}}",
            back: "{{cloze:text}}{{#extra}}\n<hr class=\"fc-answer\">\n{{extra}}{{/extra}}",
            engine: engine::CLOZE,
            answer: None,
        }],
        "",
    )
}

/// Multiple choice: `answer` is right, `distractors` (one per line) are wrong. Without
/// distractors, answers of other notes of the same type in the deck are used.
pub fn multiple_choice(now: Timestamp, locale: Locale) -> NoteType {
    make(
        now,
        locale,
        "multiple_choice",
        NoteTypeKind::Standard,
        &[
            f("question", "Question", "Soru", FieldKind::RichText)
                .uniq()
                .role(FieldRole::Term),
            f("answer", "Correct answer", "Doğru cevap", FieldKind::Text)
                .req()
                .role(FieldRole::Meaning),
            f(
                "distractors",
                "Wrong answers (one per line)",
                "Yanlış cevaplar (her satıra bir)",
                FieldKind::Text,
            ),
            f("extra", "Explanation", "Açıklama", FieldKind::RichText).role(FieldRole::Note),
        ],
        &[T {
            key: "choice",
            name: LocalizedText {
                en: "Choice",
                tr: "Seçim",
            },
            front: "{{question}}",
            back: "{{question}}\n<hr class=\"fc-answer\">\n{{answer}}{{#extra}}<div class=\"fc-extra\">{{extra}}</div>{{/extra}}",
            engine: engine::MULTIPLE_CHOICE,
            answer: None,
        }],
        "",
    )
}

/// Image occlusion: `masks` holds the mask JSON written by the mask editor; one card per
/// mask.
pub fn image_occlusion(now: Timestamp, locale: Locale) -> NoteType {
    let mut nt = make(
        now,
        locale,
        "image_occlusion",
        NoteTypeKind::Standard,
        &[
            f("header", "Title", "Başlık", FieldKind::RichText),
            f("image", "Image", "Resim", FieldKind::Image).req(),
            f("masks", "Masks", "Maskeler", FieldKind::Text).req(),
            f("extra", "Extra", "Ek bilgi", FieldKind::RichText).role(FieldRole::Note),
        ],
        &[T {
            key: "occlusion",
            name: LocalizedText {
                en: "Mask",
                tr: "Maske",
            },
            front: "{{header}}",
            back: "{{header}}{{#extra}}<div class=\"fc-extra\">{{extra}}</div>{{/extra}}",
            engine: engine::IMAGE_OCCLUSION,
            answer: None,
        }],
        "",
    );
    nt.fields[2].searchable = false;
    nt.fields[1].unique = true;
    nt.sort_field = FieldKey::new("image");
    nt
}

/// Image quiz: an image is shown, the answer is typed.
pub fn image_quiz(now: Timestamp, locale: Locale) -> NoteType {
    make(
        now,
        locale,
        "image_quiz",
        NoteTypeKind::Standard,
        &[
            f("image", "Image", "Resim", FieldKind::Image).uniq(),
            f("answer", "Answer", "Cevap", FieldKind::Text)
                .req()
                .role(FieldRole::Term),
            f("extra", "Extra", "Ek bilgi", FieldKind::RichText).role(FieldRole::Note),
        ],
        &[T {
            key: "forward",
            name: FORWARD.name,
            front: "{{image}}\n\n{{type:answer}}",
            back: "{{image}}\n<hr class=\"fc-answer\">\n{{type:answer}}{{#extra}}<div class=\"fc-extra\">{{extra}}</div>{{/extra}}",
            engine: engine::TYPING,
            answer: Some("answer"),
        }],
        "",
    )
}

/// Listening card: audio is played, the user types what they heard.
pub fn audio(now: Timestamp, locale: Locale) -> NoteType {
    make(
        now,
        locale,
        "audio",
        NoteTypeKind::Standard,
        &[
            f("audio", "Audio", "Ses", FieldKind::Audio).uniq(),
            f("answer", "Transcript", "Duyulan metin", FieldKind::Text)
                .req()
                .role(FieldRole::Term),
            f("extra", "Extra", "Ek bilgi", FieldKind::RichText).role(FieldRole::Note),
        ],
        &[T {
            key: "forward",
            name: FORWARD.name,
            front: "{{audio}}\n\n{{type:answer}}",
            back: "{{audio}}\n<hr class=\"fc-answer\">\n{{type:answer}}{{#extra}}<div class=\"fc-extra\">{{extra}}</div>{{/extra}}",
            engine: engine::TYPING,
            answer: Some("answer"),
        }],
        "",
    )
}

/// The labelled example part of a back (v1.3): `field` (a sentence containing the term),
/// with `translation` under it when given.
macro_rules! example_part {
    ($field:literal) => {
        concat!(
            "{{#",
            $field,
            "}}<div class=\"fc-part fc-part-example\">",
            "<span class=\"fc-part-label\" data-fc-part=\"example\"></span>",
            "<div class=\"fc-example-text\">{{",
            $field,
            "}}</div></div>{{/",
            $field,
            "}}"
        )
    };
    ($field:literal, $translation:literal) => {
        concat!(
            "{{#",
            $field,
            "}}<div class=\"fc-part fc-part-example\">",
            "<span class=\"fc-part-label\" data-fc-part=\"example\"></span>",
            "<div class=\"fc-example-text\">{{",
            $field,
            "}}</div>",
            "{{#",
            $translation,
            "}}<div class=\"fc-example-translation\">{{",
            $translation,
            "}}</div>{{/",
            $translation,
            "}}</div>{{/",
            $field,
            "}}"
        )
    };
}

/// The labelled source part of a back (v1.3), smallest: the `source` field.
macro_rules! source_part {
    () => {
        "{{#source}}<div class=\"fc-part fc-part-source\"><span class=\"fc-part-label\" data-fc-part=\"source\"></span>{{source}}</div>{{/source}}"
    };
}

/// Vocabulary card: word, meaning, pronunciation, example sentence, audio, image; a
/// recognition and a production card. Pairs well with a "level" classification (CEFR).
/// Backs (v1.3): the answer largest, then the labelled example.
pub fn vocabulary(now: Timestamp, locale: Locale) -> NoteType {
    make(
        now,
        locale,
        "vocabulary",
        NoteTypeKind::Standard,
        &[
            f("word", "Word", "Kelime", FieldKind::RichText)
                .uniq()
                .role(FieldRole::Term),
            f("meaning", "Meaning", "Anlam", FieldKind::RichText)
                .req()
                .role(FieldRole::Meaning),
            f("pronunciation", "Pronunciation", "Telaffuz", FieldKind::Text),
            f("example", "Example sentence", "Örnek cümle", FieldKind::RichText)
                .role(FieldRole::Example),
            f("audio", "Audio", "Ses", FieldKind::Audio),
            f("image", "Image", "Resim", FieldKind::Image),
        ],
        &[
            T {
                key: "recognition",
                name: LocalizedText {
                    en: "Recognition",
                    tr: "Tanıma",
                },
                front: "{{word}}{{#pronunciation}}<div class=\"fc-pron\">{{pronunciation}}</div>{{/pronunciation}}{{#audio}}<div>{{audio}}</div>{{/audio}}",
                back: concat!(
                    "{{FrontSide}}\n<hr class=\"fc-answer\">\n",
                    "<div class=\"fc-part fc-part-meaning\">{{meaning}}</div>",
                    "{{#image}}<div class=\"fc-media\">{{image}}</div>{{/image}}",
                    example_part!("example")
                ),
                engine: engine::CLASSIC,
                answer: Some("meaning"),
            },
            T {
                key: "production",
                name: LocalizedText {
                    en: "Production",
                    tr: "Üretme",
                },
                front: "{{meaning}}{{#image}}<div>{{image}}</div>{{/image}}",
                back: concat!(
                    "{{FrontSide}}\n<hr class=\"fc-answer\">\n",
                    "<div class=\"fc-part fc-part-answer\">{{word}}</div>",
                    "{{#pronunciation}}<div class=\"fc-pron\">{{pronunciation}}</div>{{/pronunciation}}",
                    "{{#audio}}<div class=\"fc-media\">{{audio}}</div>{{/audio}}",
                    example_part!("example")
                ),
                engine: engine::CLASSIC,
                answer: Some("word"),
            },
        ],
        PARTS_CSS,
    )
}

/// Sentence card (v0.10 sentence mining, docs/02 §25): a sentence from a text or subtitle
/// with its target word in bold, the translation, the word and its meaning, where it came
/// from (title + timestamp) and, from a video, the audio clip and a screenshot. The card asks
/// for the sentence's translation.
pub fn sentence(now: Timestamp, locale: Locale) -> NoteType {
    make(
        now,
        locale,
        "sentence",
        NoteTypeKind::Standard,
        &[
            f("sentence", "Sentence", "Cümle", FieldKind::RichText)
                .uniq()
                .role(FieldRole::Example),
            f("translation", "Translation", "Çeviri", FieldKind::RichText)
                .role(FieldRole::ExampleTranslation),
            f("word", "Word", "Kelime", FieldKind::RichText).role(FieldRole::Term),
            f("meaning", "Meaning", "Anlam", FieldKind::RichText).role(FieldRole::Meaning),
            f("source", "Source", "Kaynak", FieldKind::Text).role(FieldRole::Source),
            f("audio", "Audio", "Ses", FieldKind::Audio),
            f("image", "Image", "Resim", FieldKind::Image),
        ],
        &[T {
            key: "forward",
            name: FORWARD.name,
            front: "{{#image}}<div>{{image}}</div>{{/image}}{{sentence}}{{#audio}}<div>{{audio}}</div>{{/audio}}",
            back: concat!(
                "{{FrontSide}}\n<hr class=\"fc-answer\">\n",
                "{{#translation}}<div class=\"fc-part fc-part-meaning\">{{translation}}</div>{{/translation}}",
                "{{#word}}<div class=\"fc-part fc-part-word\"><b>{{word}}</b>{{#meaning}}: {{meaning}}{{/meaning}}</div>{{/word}}",
                source_part!()
            ),
            engine: engine::CLASSIC,
            answer: Some("translation"),
        }],
        PARTS_CSS,
    )
}

/// Front of the Subtitle card (v1.3): the surface form large (else the dictionary form), the
/// dictionary form small beside it when it differs (`unless_same`).
pub const SUBTITLE_FRONT: &str = "<div class=\"fc-word\">{{#surface}}{{surface}} <span class=\"fc-lemma\">{{unless_same:surface:word}}</span>{{/surface}}{{^surface}}{{word}}{{/surface}}</div>";

/// Subtitle card (v1.2, docs/06 §5.1): a word picked in a video subtitle by the Subtitle
/// browser extension. The front (v1.3, migration 1301) shows the word only: the form as it
/// appeared large, the dictionary form small beside it when different — the line itself is
/// the first hint (the word highlighted), not part of the question. The back
/// (v1.3) shows the word, then three labelled parts: the meaning (with the part of speech),
/// the example — the line and its translation — and where it came from (title · time, a link
/// to that moment), with the screenshot and the audio clip between. No field is unique: the
/// same word from another line is another card (the extension's own dedup key decides).
pub fn subtitle(now: Timestamp, locale: Locale) -> NoteType {
    make(
        now,
        locale,
        SUBTITLE,
        NoteTypeKind::Standard,
        &[
            f("word", "Word", "Kelime", FieldKind::RichText)
                .req()
                .role(FieldRole::Term),
            f("surface", "As it appeared", "Geçtiği hâli", FieldKind::Text)
                .role(FieldRole::Surface),
            f("pos", "Part of speech", "Sözcük türü", FieldKind::Text),
            f("meaning", "Meaning", "Anlam", FieldKind::RichText).role(FieldRole::Meaning),
            f("sentence", "Sentence", "Cümle", FieldKind::RichText).role(FieldRole::Example),
            f("translation", "Translation", "Çeviri", FieldKind::RichText)
                .role(FieldRole::ExampleTranslation),
            f("image", "Screenshot", "Ekran görüntüsü", FieldKind::Image),
            f("audio", "Audio", "Ses", FieldKind::Audio),
            f("source", "Source", "Kaynak", FieldKind::RichText).role(FieldRole::Source),
            // v1.4 (migration 1490): the note's other encounters (scenes), filled by the app.
            f("examples", "Examples", "Örnekler", FieldKind::RichText),
        ],
        &[T {
            key: "recognition",
            name: LocalizedText {
                en: "Recognition",
                tr: "Tanıma",
            },
            front: SUBTITLE_FRONT,
            back: concat!(
                "<div class=\"fc-word\">{{word}}</div>\n<hr class=\"fc-answer\">\n",
                "<div class=\"fc-part fc-part-meaning\">{{meaning}}</div>",
                "{{#pos}}<div class=\"fc-pron\">{{pos}}</div>{{/pos}}",
                example_part!("sentence", "translation"),
                "{{#image}}<div class=\"fc-media\">{{image}}</div>{{/image}}",
                "{{#audio}}<div class=\"fc-media\">{{audio}}</div>{{/audio}}",
                "{{#examples}}<div class=\"fc-part fc-part-examples\">{{examples}}</div>{{/examples}}",
                source_part!()
            ),
            engine: engine::CLASSIC,
            answer: Some("meaning"),
        }],
        PARTS_CSS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::validate::validate_templates;
    use crate::notetype::validate_notetype;
    use std::collections::HashSet;

    #[test]
    fn every_builtin_is_valid_and_unique() {
        let mut ids = HashSet::new();
        for b in ALL {
            for locale in [Locale::En, Locale::Tr] {
                let nt = b.build(Timestamp(1), locale);
                validate_notetype(&nt).unwrap_or_else(|e| panic!("{}: {e}", b.key));
                validate_templates(&nt).unwrap_or_else(|e| panic!("{}: {e}", b.key));
                assert_eq!(nt.id, b.id, "{}", b.key);
                assert_eq!(nt.builtin.as_deref(), Some(b.key));
                assert_eq!(nt.name, b.name.get(locale));
            }
            assert!(ids.insert(b.id), "duplicate id {}", b.key);
        }
        assert_eq!(ALL.len(), 12);
        assert!(subtitle(Timestamp(1), Locale::En)
            .fields
            .iter()
            .all(|f| !f.unique));
        assert_eq!(find("cloze").map(|b| b.id), Some(id(5)));
        assert_eq!(classic(Timestamp(1), Locale::Tr).name, "Klasik");
    }

    #[test]
    fn vocabulary_builtins_have_roles_answer_fields_and_labelled_parts() {
        let st = subtitle(Timestamp(1), Locale::Tr);
        let role = |nt: &NoteType, r| nt.field_with_role(r).map(|f| f.key.as_str().to_owned());
        assert_eq!(role(&st, FieldRole::Term).as_deref(), Some("word"));
        assert_eq!(role(&st, FieldRole::Surface).as_deref(), Some("surface"));
        assert_eq!(role(&st, FieldRole::Meaning).as_deref(), Some("meaning"));
        assert_eq!(role(&st, FieldRole::Example).as_deref(), Some("sentence"));
        assert_eq!(
            role(&st, FieldRole::ExampleTranslation).as_deref(),
            Some("translation")
        );
        assert_eq!(role(&st, FieldRole::Source).as_deref(), Some("source"));
        assert_eq!(
            st.templates[0].answer_field.as_ref().map(FieldKey::as_str),
            Some("meaning")
        );
        // Meaning, then the example (line + translation), then the source.
        let back = &st.templates[0].back;
        let at = |needle: &str| back.find(needle).unwrap_or_else(|| panic!("{needle}"));
        assert!(at("fc-part-meaning") < at("fc-part-example"));
        assert!(at("fc-part-example") < at("fc-example-translation"));
        assert!(at("fc-example-translation") < at("fc-part-source"));
        assert!(back.contains("data-fc-part=\"example\""));
        assert!(back.contains("data-fc-part=\"source\""));
        // v1.3 X1: the front asks with the word only (the line is the first hint, so it is
        // not on the front); the back does not repeat the front.
        let front = &st.templates[0].front;
        assert!(!front.contains("sentence"));
        assert!(front.contains("{{surface}}") && front.contains("{{unless_same:surface:word}}"));
        assert!(!back.contains("{{FrontSide}}"));
        assert!(st.css.contains(".fc-part-example"));

        let v = vocabulary(Timestamp(1), Locale::En);
        let keys: Vec<_> = v
            .templates
            .iter()
            .map(|t| t.answer_field.as_ref().map(|k| k.as_str().to_owned()))
            .collect();
        assert_eq!(keys, [Some("meaning".to_owned()), Some("word".to_owned())]);
        assert!(v.templates[0].back.contains("fc-part-meaning"));
        assert!(v.templates[1].back.contains("fc-part-answer"));

        // Reverse cards ask for the term; cloze / choice / occlusion engines decide alone.
        let r = classic_reverse(Timestamp(1), Locale::En);
        assert_eq!(r.templates[1].answer_field, Some(FieldKey::new("front")));
        for key in ["cloze", "multiple_choice", "image_occlusion"] {
            let nt = find(key).unwrap().build(Timestamp(1), Locale::En);
            assert!(
                nt.templates.iter().all(|t| t.answer_field.is_none()),
                "{key}"
            );
        }
        // Every built-in names an existing answer field (validate_notetype checks it) and no
        // role twice; classic is the same in both constructors.
        assert_eq!(
            classic(Timestamp(1), Locale::En).templates[0].answer_field,
            Some(FieldKey::new("back"))
        );
    }
}
