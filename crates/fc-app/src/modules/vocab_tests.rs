//! v1.3 vocabulary correctness (CONTRACT §2): field roles drive typed answers, tests, hints,
//! the deck page's term list, inbox acceptance and import mapping. The running example is
//! the owner's: a subtitle capture of "went" (go → gitmek) in "I went home.".
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::TestApp;
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::inbox::{AcceptInbox, AcceptInboxItem, AddInboxItem, InboxContext};
use fc_api::v1::io::{ColumnTarget, ImportFormat, ImportSource, PreviewImport, StartImport};
use fc_api::v1::jobs::{JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::modes::{SubmitTest, TestAnswerDto, TestPaperRequest};
use fc_api::v1::notes::{CreateNote, ListNotes, NoteDto, NoteTerm, UpdateNote};
use fc_api::v1::oplog::Actor;
use fc_api::v1::study::{
    AnswerGrade, CheckAnswer, HintDto, HintKind, HintRequest, SessionDto, StudyCardDto,
};
use fc_core::content::builtin;
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::paging::PageRequest;
use fc_core::{CardId, DeckId, NoteTypeId};
use serde_json::json;

fn deck(t: &TestApp, path: &str) -> DeckDto {
    t.ok::<DecksCreate>(CreateDeck {
        path: path.into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    })
}

fn nt(key: &str) -> NoteTypeId {
    builtin::find(key).unwrap().id
}

fn note(t: &TestApp, notetype: NoteTypeId, deck: DeckId, fields: &[(&str, &str)]) -> NoteDto {
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: notetype,
        deck_id: deck,
        fields: fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    })
}

/// The owner's subtitle capture: word=go, surface=went, meaning=gitmek, the line with the
/// clicked occurrence in bold (as `subtitle.capture` stores it), its translation, a source.
fn went(t: &TestApp, deck: DeckId) -> NoteDto {
    note(
        t,
        nt(builtin::SUBTITLE),
        deck,
        &[
            ("word", "go"),
            ("surface", "went"),
            ("meaning", "gitmek"),
            ("sentence", "I <b>went</b> home."),
            ("translation", "Eve gittim."),
            ("source", "Film · 1:23"),
        ],
    )
}

/// A practice session of `kind` (JSON: the request may grow fields this test does not know).
fn start(t: &TestApp, deck: DeckId, kind: &str, modes: serde_json::Value) -> SessionDto {
    let v = t
        .app
        .invoke(
            "v1.study.start",
            json!({ "kind": kind, "mode": "trial", "deck_id": deck, "modes": modes,
                    "tz_offset_mins": 0 }),
            &Actor::User,
        )
        .unwrap();
    serde_json::from_value(v).unwrap()
}

fn current(s: &SessionDto) -> &StudyCardDto {
    s.current.as_ref().unwrap()
}

/// Session showing `card` (others of the deck are suspended away by note choice).
fn session_on(t: &TestApp, deck: DeckId, card: CardId) -> SessionDto {
    let s = start(t, deck, "normal", serde_json::Value::Null);
    assert_eq!(current(&s).card_id, card);
    s
}

fn check(t: &TestApp, s: &SessionDto, card: CardId, given: &str) -> AnswerGrade {
    t.ok::<StudyCheckAnswer>(CheckAnswer {
        session_id: s.id,
        card_id: card,
        given: given.into(),
        choice: None,
    })
    .grade
}

fn hint(t: &TestApp, s: &SessionDto, card: CardId) -> HintDto {
    t.ok::<StudyHint>(HintRequest {
        session_id: s.id,
        card_id: card,
    })
}

fn suspend(t: &TestApp, card: CardId, suspended: bool) {
    t.ok::<StudySuspend>(fc_api::v1::study::SuspendCards {
        card_ids: vec![card],
        suspended,
        session_id: None,
    });
}

fn terms(t: &TestApp, deck: DeckId) -> Vec<NoteTerm> {
    t.ok::<NotesTerms>(ListNotes {
        deck_id: Some(deck),
        include_subdecks: None,
        page: PageRequest::default(),
    })
    .items
}

#[test]
fn went_is_answered_with_its_meaning_only() {
    let t = TestApp::mem();
    let d = deck(&t, "English");
    let n = went(&t, d.id);
    let card = n.cards[0].id;
    let s = session_on(&t, d.id, card);

    // Typed answer: the learner owes "gitmek", not the example and source under it.
    assert_eq!(check(&t, &s, card, "gitmek"), AnswerGrade::Correct);
    assert_eq!(check(&t, &s, card, "Eve gittim"), AnswerGrade::Wrong);

    // The back: meaning, then the labelled example (line + translation), then the source.
    let back = &current(&s).sides.answer;
    let at = |needle: &str| back.find(needle).unwrap_or_else(|| panic!("{needle} in {back}"));
    assert!(at("fc-part-meaning") < at("fc-part-example"));
    assert!(at("Eve gittim.") > at("fc-part-example"));
    assert!(at("fc-part-source") > at("Eve gittim."));
    assert!(back.contains("data-fc-part=\"example\""));

    // X1: the front asks with the word only, so the first hint is the line as context (the
    // word highlighted), then letters of the meaning.
    let h = hint(&t, &s, card);
    assert_eq!((h.kind, h.text.as_deref()), (HintKind::Context, Some("I went home.")));
    assert_eq!(h.level, 1);
    let h = hint(&t, &s, card);
    assert_eq!((h.kind, h.text.as_deref()), (HintKind::Letters, Some("g_____")));
}

#[test]
fn a_written_test_of_went_expects_gitmek() {
    let t = TestApp::mem();
    let d = deck(&t, "English");
    went(&t, d.id);
    let s = start(
        &t,
        d.id,
        "test",
        json!({ "test": { "questions": 1, "kinds": ["written"] } }),
    );
    let paper = t.ok::<ModesTestPaper>(TestPaperRequest {
        session_id: s.id,
        with_key: Some(true),
    });
    assert_eq!(paper.questions.len(), 1);
    let q = &paper.questions[0];
    assert_eq!(q.key.as_ref().unwrap().expected, "gitmek");
    let graded = t.ok::<ModesTestSubmit>(SubmitTest {
        session_id: s.id,
        answers: vec![TestAnswerDto {
            index: q.index,
            text: Some("gitmek".into()),
            ..TestAnswerDto::default()
        }],
        duration_ms: 1_000,
    });
    assert_eq!(graded.result.unwrap().correct, 1);
}

#[test]
fn the_term_list_edits_the_meaning_never_the_surface_form() {
    let t = TestApp::mem();
    let d = deck(&t, "English");
    let n = went(&t, d.id);
    let rows = terms(&t, d.id);
    let row = &rows[0];
    assert_eq!((row.front.key.as_str(), row.front.text.as_str()), ("word", "go"));
    let back = row.back.as_ref().unwrap();
    assert_eq!((back.key.as_str(), back.text.as_str()), ("meaning", "gitmek"));
    assert_eq!(row.term_field, "word");
    assert_eq!(row.definition_field.as_deref(), Some("meaning"));
    assert!(!row.ambiguous);

    // The inline edit (TermList) writes `definition_field`.
    let stored = t.ok::<NotesGet>(fc_api::v1::notes::NoteRef { id: n.id });
    let mut fields = stored.fields.clone();
    fields.insert(row.definition_field.clone().unwrap(), "gitmek, varmak".into());
    t.ok::<NotesUpdate>(UpdateNote {
        id: n.id,
        deck_id: None,
        fields,
        tags: vec![],
        attributes: vec![],
    });
    let after = t.ok::<NotesGet>(fc_api::v1::notes::NoteRef { id: n.id });
    assert_eq!(after.fields["meaning"], "gitmek, varmak");
    assert_eq!(after.fields["surface"], "went");

    // A note type without roles: its first two fields, flagged as a guess.
    let c = note(&t, nt("cloze"), d.id, &[("text", "{{c1::Ankara}} is the capital")]);
    let rows = terms(&t, d.id);
    let cloze = rows.iter().find(|r| r.note_id == c.id).unwrap();
    assert!(cloze.ambiguous);
    assert_eq!(cloze.term_field, "text");
    assert_eq!(cloze.definition_field.as_deref(), Some("extra"));
    // Classic has roles (front = term, back = meaning).
    let k = note(&t, CLASSIC_ID, d.id, &[("front", "dog"), ("back", "köpek")]);
    let rows = terms(&t, d.id);
    assert!(!rows.iter().find(|r| r.note_id == k.id).unwrap().ambiguous);
}

/// X1 hint rule: a hint may reveal context but never the answer. A card asking for the
/// meaning shows the sentence with the word highlighted; one asking for the word blanks it.
#[test]
fn hints_start_with_the_sentence_and_never_leak_the_answer() {
    let t = TestApp::mem();
    let d = deck(&t, "English");
    let n = note(
        &t,
        nt("vocabulary"),
        d.id,
        &[("word", "go"), ("meaning", "gitmek"), ("example", "I **went** home.")],
    );
    let card = |key: &str| n.cards.iter().find(|c| c.template_key == key).unwrap().id;
    let (production, recognition) = (card("production"), card("recognition"));
    // One card of the note per session (siblings are buried): the other one suspended.
    suspend(&t, recognition, true);
    let s = session_on(&t, d.id, production);

    // Production (meaning → word): the word is the answer, blanked.
    let one = hint(&t, &s, production);
    assert_eq!(one.kind, HintKind::Context);
    assert_eq!(one.text.as_deref(), Some("I ___ home."));
    assert_eq!(
        one.html.as_deref(),
        Some("I <span class=\"fc-blank\">___</span> home.")
    );
    assert_eq!(one.level, 1);
    let two = hint(&t, &s, production);
    assert_eq!((two.kind, two.level), (HintKind::Letters, 2));
    assert!(two.text.as_deref().unwrap().starts_with('g'));
    assert_eq!(check(&t, &s, production, "go"), AnswerGrade::Correct);

    // Recognition (word → meaning): the sentence as context, the word highlighted; the
    // meaning is not in it. Then letters of the meaning.
    suspend(&t, recognition, false);
    suspend(&t, production, true);
    let s = session_on(&t, d.id, recognition);
    let one = hint(&t, &s, recognition);
    assert_eq!(one.kind, HintKind::Context);
    assert_eq!(one.text.as_deref(), Some("I went home."));
    assert_eq!(
        one.html.as_deref(),
        Some("I <b class=\"fc-context-word\">went</b> home.")
    );
    let two = hint(&t, &s, recognition);
    assert_eq!((two.kind, two.text.as_deref()), (HintKind::Letters, Some("g_____")));
    assert_eq!(check(&t, &s, recognition, "gitmek"), AnswerGrade::Correct);

    // The sentence still holds the word elsewhere: asking for the word gives letters from
    // level 1; asking for the meaning still shows the context.
    let d2 = deck(&t, "Leaky");
    let leaky = note(
        &t,
        nt("vocabulary"),
        d2.id,
        &[("word", "run"), ("meaning", "koşmak"), ("example", "I run where you run.")],
    );
    let card = |key: &str| leaky.cards.iter().find(|c| c.template_key == key).unwrap().id;
    suspend(&t, card("recognition"), true);
    let s2 = session_on(&t, d2.id, card("production"));
    assert_eq!(hint(&t, &s2, card("production")).kind, HintKind::Letters);
    suspend(&t, card("recognition"), false);
    suspend(&t, card("production"), true);
    let s2 = session_on(&t, d2.id, card("recognition"));
    let ctx = hint(&t, &s2, card("recognition"));
    assert_eq!((ctx.kind, ctx.text.as_deref()), (HintKind::Context, Some("I run where you run.")));
}

/// The owner's Subtitle card (X1): the front asks with the word only (surface large, the
/// dictionary form beside it), so the first hint is the line with the word highlighted —
/// never the meaning.
#[test]
fn the_subtitle_card_asks_with_the_word_and_hints_with_its_line() {
    let t = TestApp::mem();
    let d = deck(&t, "Subtitle");
    let n = went(&t, d.id);
    let card = n.cards[0].id;
    let s = session_on(&t, d.id, card);
    let front = &current(&s).sides.question;
    assert!(front.contains("went") && front.contains("fc-lemma"), "{front}");
    assert!(front.contains(">go<"), "the dictionary form beside the surface: {front}");
    assert!(!front.contains("home"), "the line is not on the front: {front}");
    let one = hint(&t, &s, card);
    assert_eq!(one.kind, HintKind::Context);
    assert_eq!(one.text.as_deref(), Some("I went home."));
    assert_eq!(
        one.html.as_deref(),
        Some("I <b class=\"fc-context-word\">went</b> home.")
    );
    assert!(!one.text.unwrap().contains("gitmek"));
    let two = hint(&t, &s, card);
    assert_eq!((two.kind, two.text.as_deref()), (HintKind::Letters, Some("g_____")));

    // The same word as seen: no dictionary form beside it.
    let d2 = deck(&t, "Subtitle 2");
    let same = note(
        &t,
        nt(builtin::SUBTITLE),
        d2.id,
        &[("word", "home"), ("surface", "home"), ("meaning", "ev"), ("sentence", "I went <b>home</b>.")],
    );
    let s2 = session_on(&t, d2.id, same.cards[0].id);
    let front = &current(&s2).sides.question;
    assert!(!front.contains("fc-lemma\">home"), "{front}");
}

#[test]
fn a_reverse_card_asks_for_the_term() {
    let t = TestApp::mem();
    let d = deck(&t, "English");
    let n = note(
        &t,
        nt("classic_reverse"),
        d.id,
        &[("front", "go"), ("back", "gitmek")],
    );
    let reverse = n.cards.iter().find(|c| c.template_key == "reverse").unwrap().id;
    let s = start(&t, d.id, "normal", serde_json::Value::Null);
    let shown = current(&s).card_id;
    let want = if shown == reverse { "go" } else { "gitmek" };
    assert_eq!(check(&t, &s, shown, want), AnswerGrade::Correct);
}

fn inbox_item(t: &TestApp, deck: DeckId) -> fc_api::v1::inbox::InboxItemDto {
    t.ok::<InboxAdd>(AddInboxItem {
        front: Some("went".into()),
        back: Some("gitmek".into()),
        deck_id: Some(deck),
        context: Some(InboxContext {
            url: Some("https://example.com/story".into()),
            title: Some("A story".into()),
            sentence: Some("Yesterday I went home early.".into()),
        }),
        ..AddInboxItem::default()
    })
}

fn accept(t: &TestApp, id: fc_core::InboxItemId, notetype: Option<NoteTypeId>) -> NoteDto {
    let r = t.ok::<InboxAccept>(AcceptInbox {
        items: vec![AcceptInboxItem {
            id,
            deck_id: None,
            notetype_id: notetype,
            fields: None,
            tags: None,
            attributes: None,
        }],
        allow_duplicates: None,
    });
    t.ok::<NotesGet>(fc_api::v1::notes::NoteRef {
        id: r.accepted[0].note_id,
    })
}

#[test]
fn inbox_acceptance_keeps_the_captured_sentence() {
    let t = TestApp::mem();
    let d = deck(&t, "English");

    // No note type chosen: the word-in-context card, sentence and source kept.
    let item = inbox_item(&t, d.id);
    let n = accept(&t, item.id, None);
    assert_eq!(n.notetype_id, nt(builtin::SUBTITLE));
    assert_eq!(n.fields["word"], "went");
    assert_eq!(n.fields["meaning"], "gitmek");
    assert_eq!(n.fields["sentence"], "Yesterday I <b>went</b> home early.");
    assert!(n.fields["source"].contains("https://example.com/story"));
    assert!(!n.fields.contains_key("surface") || n.fields["surface"].is_empty());

    // Vocabulary chosen: the sentence goes into its example field.
    let item = inbox_item(&t, d.id);
    let v = accept(&t, item.id, Some(nt("vocabulary")));
    assert_eq!(v.fields["word"], "went");
    assert_eq!(v.fields["meaning"], "gitmek");
    assert_eq!(v.fields["example"], "Yesterday I <b>went</b> home early.");

    // Classic chosen explicitly: front/back as before (Classic has no example field).
    let item = inbox_item(&t, d.id);
    let c = accept(&t, item.id, Some(CLASSIC_ID));
    assert_eq!((c.fields["front"].as_str(), c.fields["back"].as_str()), ("went", "gitmek"));

    // No sentence: Classic, as before.
    let plain = t.ok::<InboxAdd>(AddInboxItem {
        front: Some("dog".into()),
        back: Some("köpek".into()),
        deck_id: Some(d.id),
        ..AddInboxItem::default()
    });
    assert_eq!(accept(&t, plain.id, None).notetype_id, CLASSIC_ID);
}

fn import_text(t: &TestApp, d: DeckId, notetype: NoteTypeId, text: &str, columns: Option<Vec<ColumnTarget>>) {
    let job = t.ok::<IoImportStart>(StartImport {
        source: ImportSource::Text { text: text.into() },
        format: Some(ImportFormat::Text),
        text_options: None,
        deck_id: Some(d),
        notetype_id: Some(notetype),
        columns,
        skip_first_row: None,
        duplicates: None,
        create_missing_options: None,
        include_history: None,
    });
    t.app.run_pending_jobs().unwrap();
    let job = t.ok::<JobsGet>(JobRef { id: job.id });
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
}

fn notes_of(t: &TestApp, d: DeckId) -> Vec<NoteDto> {
    terms(t, d)
        .iter()
        .map(|r| t.ok::<NotesGet>(fc_api::v1::notes::NoteRef { id: r.note_id }))
        .collect()
}

#[test]
fn imports_map_sentence_columns_by_role() {
    let t = TestApp::mem();
    // Default mapping of a note type with roles: term, meaning, example…
    let d = deck(&t, "Subs");
    import_text(&t, d.id, nt(builtin::SUBTITLE), "go\tgitmek\tI went home.\n", None);
    let n = &notes_of(&t, d.id)[0];
    assert_eq!(
        (n.fields["word"].as_str(), n.fields["meaning"].as_str(), n.fields["sentence"].as_str()),
        ("go", "gitmek", "I went home.")
    );
    // …explicit role targets…
    let v = deck(&t, "Vocab");
    import_text(
        &t,
        v.id,
        nt("vocabulary"),
        "I went home.\tgo\tgitmek\n",
        Some(vec![
            ColumnTarget::Role {
                role: fc_core::notetype::FieldRole::Example,
            },
            ColumnTarget::Role {
                role: fc_core::notetype::FieldRole::Term,
            },
            ColumnTarget::Field {
                key: "meaning".into(),
            },
        ]),
    );
    let n = &notes_of(&t, v.id)[0];
    assert_eq!(
        (n.fields["word"].as_str(), n.fields["example"].as_str()),
        ("go", "I went home.")
    );
    // …and headers named after what a column holds.
    let p = t.ok::<IoImportPreview>(PreviewImport {
        source: ImportSource::Text {
            text: "word,definition,sentence\ngo,gitmek,I went home.\n".into(),
        },
        format: Some(ImportFormat::Text),
        text_options: None,
        notetype_id: Some(nt("vocabulary")),
        deck_id: None,
        limit: None,
    });
    assert_eq!(
        p.suggested_columns,
        vec![
            ColumnTarget::Field { key: "word".into() },
            ColumnTarget::Field { key: "meaning".into() },
            ColumnTarget::Field { key: "example".into() },
        ]
    );
    // Classic keeps its order (front, back).
    let c = deck(&t, "Classic");
    import_text(&t, c.id, CLASSIC_ID, "dog\tköpek\n", None);
    let n = &notes_of(&t, c.id)[0];
    assert_eq!(
        (n.fields["front"].as_str(), n.fields["back"].as_str()),
        ("dog", "köpek")
    );
}
